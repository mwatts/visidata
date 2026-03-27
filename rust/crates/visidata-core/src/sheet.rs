use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use crate::async_loader::LoadingState;
use crate::column::{Column, ColumnId};
use crate::row::Row;
use crate::undo::{UndoAction, UndoStack};
use crate::value::Value;

/// Callback invoked when the user presses Enter on a row in an index sheet.
///
/// Implementations live in `visidata-loaders` (`SQLite`, ext loaders, etc.).
/// The trait is defined here so `Sheet` can hold it without a circular
/// dependency between `visidata-core` and `visidata-loaders`.
pub trait DrillAction: Send + Sync + fmt::Debug {
    /// Open a child sheet for the given cursor row.
    ///
    /// Typically reads the table/view name from column 0 of `row` and
    /// returns a fully loaded sheet for that table.
    ///
    /// # Errors
    ///
    /// Returns an error if the child sheet cannot be constructed (e.g. the
    /// table does not exist or the subprocess fails).
    fn open_row(&self, row: &Row) -> anyhow::Result<Sheet>;
}

/// Unique identifier for a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SheetId(pub u64);

impl fmt::Display for SheetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Sort direction for a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// A sort key: column index + direction.
#[derive(Debug, Clone)]
pub struct SortKey {
    pub col_idx: usize,
    pub direction: SortDirection,
}

/// A tabular data sheet with columns and rows.
#[derive(Debug)]
pub struct Sheet {
    /// Unique identifier.
    pub id: SheetId,

    /// Display name.
    pub name: String,

    /// Column definitions.
    pub columns: Vec<Column>,

    /// Data rows.
    pub rows: Vec<Row>,

    /// Current cursor row index.
    pub cursor_row: usize,

    /// Current cursor column index (into visible columns).
    pub cursor_col: usize,

    /// Top visible row index (scroll offset).
    pub top_row: usize,

    /// Left-most visible column index (horizontal scroll offset).
    pub left_col: usize,

    /// Source file path, if loaded from a file.
    pub source: Option<PathBuf>,

    /// Active sort keys.
    pub sort_keys: Vec<SortKey>,

    /// Number of key columns (leftmost N columns are keys).
    pub num_keys: usize,

    /// Whether the sheet has been modified since load/save.
    pub modified: bool,

    /// Undo stack for reversible mutations.
    pub undo_stack: UndoStack,

    /// Background loading state.
    pub loading_state: LoadingState,

    /// Optional drill-down action: pressing Enter on a row calls this to open
    /// a child sheet. Set by index-sheet builders (`SQLite`, ext loaders, etc.).
    pub drill: Option<Arc<dyn DrillAction>>,
}

/// Global sheet ID counter.
static NEXT_SHEET_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_sheet_id() -> SheetId {
    SheetId(NEXT_SHEET_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

impl Sheet {
    /// Create a new empty sheet with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: next_sheet_id(),
            name: name.into(),
            columns: Vec::new(),
            rows: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            top_row: 0,
            left_col: 0,
            source: None,
            sort_keys: Vec::new(),
            num_keys: 0,
            modified: false,
            undo_stack: UndoStack::new(),
            loading_state: LoadingState::default(),
            drill: None,
        }
    }

    /// Create a sheet from columns and rows.
    #[must_use]
    pub fn with_data(name: impl Into<String>, columns: Vec<Column>, rows: Vec<Row>) -> Self {
        Self {
            columns,
            rows,
            ..Self::new(name)
        }
    }

    /// Total number of rows.
    #[must_use]
    pub const fn num_rows(&self) -> usize {
        self.rows.len()
    }

    /// Total number of columns.
    #[must_use]
    pub const fn num_cols(&self) -> usize {
        self.columns.len()
    }

    /// Returns visible (non-hidden) columns.
    #[must_use]
    pub fn visible_columns(&self) -> Vec<&Column> {
        self.columns.iter().filter(|c| !c.is_hidden()).collect()
    }

    /// Returns the cell value at the given row and column indices.
    #[must_use]
    pub fn get_cell(&self, row_idx: usize, col_idx: usize) -> Value {
        let Some(row) = self.rows.get(row_idx) else {
            return Value::Null;
        };
        let Some(col) = self.columns.get(col_idx) else {
            return Value::Null;
        };
        col.typed_value(row)
    }

    /// Returns the display string for a cell.
    #[must_use]
    pub fn get_cell_display(&self, row_idx: usize, col_idx: usize) -> String {
        let Some(row) = self.rows.get(row_idx) else {
            return String::new();
        };
        let Some(col) = self.columns.get(col_idx) else {
            return String::new();
        };
        col.display_value(row)
    }

    /// Returns the current cursor row, if it exists.
    #[must_use]
    pub fn current_row(&self) -> Option<&Row> {
        self.rows.get(self.cursor_row)
    }

    /// Returns the current cursor column, if it exists.
    #[must_use]
    pub fn current_column(&self) -> Option<&Column> {
        let visible = self.visible_columns();
        visible.get(self.cursor_col).copied()
    }

    /// Returns the number of selected rows.
    #[must_use]
    pub fn num_selected(&self) -> usize {
        self.rows.iter().filter(|r| r.selected).count()
    }

    /// Move cursor down by `n` rows, clamping to bounds.
    pub fn cursor_down(&mut self, n: usize) {
        if !self.rows.is_empty() {
            self.cursor_row = (self.cursor_row + n).min(self.rows.len() - 1);
        }
    }

    /// Move cursor up by `n` rows, clamping to bounds.
    pub const fn cursor_up(&mut self, n: usize) {
        self.cursor_row = self.cursor_row.saturating_sub(n);
    }

    /// Move cursor right by `n` columns, clamping to bounds.
    pub fn cursor_right(&mut self, n: usize) {
        let max_col = self.visible_columns().len().saturating_sub(1);
        self.cursor_col = (self.cursor_col + n).min(max_col);
    }

    /// Move cursor left by `n` columns, clamping to bounds.
    pub const fn cursor_left(&mut self, n: usize) {
        self.cursor_col = self.cursor_col.saturating_sub(n);
    }

    /// Ensure cursor is within valid bounds after data changes.
    pub fn clamp_cursor(&mut self) {
        if self.rows.is_empty() {
            self.cursor_row = 0;
        } else {
            self.cursor_row = self.cursor_row.min(self.rows.len() - 1);
        }
        let vis_cols = self.visible_columns().len();
        if vis_cols == 0 {
            self.cursor_col = 0;
        } else {
            self.cursor_col = self.cursor_col.min(vis_cols - 1);
        }
    }

    /// Add a column to the sheet, returning its `ColumnId`.
    pub fn add_column(&mut self, name: impl Into<String>, source_idx: usize) -> ColumnId {
        let id = ColumnId(self.columns.len());
        self.columns.push(Column::new(id, name, source_idx));
        id
    }

    /// Add a row to the sheet.
    pub fn add_row(&mut self, values: Vec<Value>) {
        self.rows.push(Row::new(values));
    }

    // --- Sorting ---

    /// Sort rows by the given column index and direction.
    ///
    /// Replaces any existing sort keys with a single key.
    pub fn sort_by(&mut self, col_idx: usize, direction: SortDirection) {
        self.sort_keys = vec![SortKey { col_idx, direction }];
        self.apply_sort();
    }

    /// Add a secondary sort key (stable sort).
    pub fn sort_by_add(&mut self, col_idx: usize, direction: SortDirection) {
        self.sort_keys.push(SortKey { col_idx, direction });
        self.apply_sort();
    }

    /// Apply display format strings from options to all columns of matching types.
    ///
    /// `float_fmt` is e.g. `"%.2f"`, `int_fmt` is `"%d"`, `date_fmt` is `"%Y-%m-%d"`.
    /// Pass `None` for a format to leave that type's columns unchanged.
    pub fn apply_display_formats(
        &mut self,
        float_fmt: Option<&str>,
        int_fmt: Option<&str>,
        date_fmt: Option<&str>,
    ) {
        for col in &mut self.columns {
            match col.col_type {
                crate::column::ColumnType::Float
                | crate::column::ColumnType::Currency => {
                    if let Some(fmt) = float_fmt {
                        col.fmt = Some(fmt.to_owned());
                    }
                }
                crate::column::ColumnType::Int => {
                    if let Some(fmt) = int_fmt {
                        col.fmt = Some(fmt.to_owned());
                    }
                }
                crate::column::ColumnType::Date => {
                    if let Some(fmt) = date_fmt {
                        col.fmt = Some(fmt.to_owned());
                    }
                }
                _ => {}
            }
        }
    }

    /// Re-apply the current sort keys (e.g. after redo). No undo action is pushed.
    pub fn resort(&mut self) {
        if self.sort_keys.is_empty() {
            return;
        }
        let columns = &self.columns;
        let sort_keys = &self.sort_keys;
        self.rows.sort_by(|a, b| {
            for key in sort_keys {
                let Some(col) = columns.get(key.col_idx) else { continue; };
                let ord = col.typed_value(a).partial_cmp(&col.typed_value(b))
                    .unwrap_or(std::cmp::Ordering::Equal);
                let ord = match key.direction {
                    SortDirection::Ascending => ord,
                    SortDirection::Descending => ord.reverse(),
                };
                if ord != std::cmp::Ordering::Equal { return ord; }
            }
            std::cmp::Ordering::Equal
        });
        self.clamp_cursor();
    }

    /// Apply the current sort keys to the rows, recording the original order for undo.
    fn apply_sort(&mut self) {
        // Save original order for undo
        let old_order: Vec<_> = self.rows.iter().map(|r| r.id).collect();

        let columns = &self.columns;
        let sort_keys = &self.sort_keys;

        self.rows.sort_by(|a, b| {
            for key in sort_keys {
                let Some(col) = columns.get(key.col_idx) else {
                    continue;
                };
                let va = col.typed_value(a);
                let vb = col.typed_value(b);
                let ord = va.partial_cmp(&vb).unwrap_or(std::cmp::Ordering::Equal);
                let ord = match key.direction {
                    SortDirection::Ascending => ord,
                    SortDirection::Descending => ord.reverse(),
                };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            std::cmp::Ordering::Equal
        });

        self.undo_stack.push(UndoAction::ReorderRows { order: old_order });
        self.clamp_cursor();
    }

    // --- Selection ---

    /// Select the current cursor row.
    pub fn select_current(&mut self) {
        if let Some(row) = self.rows.get_mut(self.cursor_row) {
            row.selected = true;
        }
    }

    /// Unselect the current cursor row.
    pub fn unselect_current(&mut self) {
        if let Some(row) = self.rows.get_mut(self.cursor_row) {
            row.selected = false;
        }
    }

    /// Select all rows.
    pub fn select_all(&mut self) {
        for row in &mut self.rows {
            row.selected = true;
        }
    }

    /// Unselect all rows.
    pub fn unselect_all(&mut self) {
        for row in &mut self.rows {
            row.selected = false;
        }
    }

    /// Toggle selection on the current cursor row.
    pub fn toggle_select_current(&mut self) {
        if let Some(row) = self.rows.get_mut(self.cursor_row) {
            row.selected = !row.selected;
        }
    }

    /// Toggle selection on every row.
    pub fn toggle_select_all(&mut self) {
        for row in &mut self.rows {
            row.selected = !row.selected;
        }
    }

    /// Select or unselect rows where `col_idx` display value matches `pattern`.
    ///
    /// Returns the number of rows affected, or `None` if the regex is invalid.
    pub fn select_by_col_regex(
        &mut self,
        col_idx: usize,
        pattern: &str,
        select: bool,
    ) -> Option<usize> {
        let re = regex::Regex::new(pattern).ok()?;
        let mut count = 0;
        for row in &mut self.rows {
            if let Some(col) = self.columns.get(col_idx) {
                let display = col.display_value(row);
                if re.is_match(&display) {
                    row.selected = select;
                    count += 1;
                }
            }
        }
        Some(count)
    }

    /// Select or unselect rows where any visible column matches `pattern`.
    pub fn select_by_any_col_regex(
        &mut self,
        pattern: &str,
        select: bool,
    ) -> Option<usize> {
        let re = regex::Regex::new(pattern).ok()?;
        let col_indices: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_hidden())
            .map(|(i, _)| i)
            .collect();
        let mut count = 0;
        for row in &mut self.rows {
            let matches = col_indices.iter().any(|&ci| {
                self.columns
                    .get(ci)
                    .is_some_and(|col| re.is_match(&col.display_value(row)))
            });
            if matches {
                row.selected = select;
                count += 1;
            }
        }
        Some(count)
    }

    /// Returns the selected rows as a new filtered sheet.
    #[must_use]
    pub fn selected_rows_sheet(&self) -> Self {
        let selected_rows: Vec<Row> = self.rows.iter().filter(|r| r.selected).cloned().collect();
        let mut sheet = Self::with_data(
            format!("{}_selected", self.name),
            self.columns.clone(),
            selected_rows,
        );
        sheet.source.clone_from(&self.source);
        sheet
    }

    /// Create a sheet containing all rows (a full copy).
    #[must_use]
    pub fn all_rows_sheet(&self) -> Self {
        let mut sheet = Self::with_data(
            format!("{}_copy", self.name),
            self.columns.clone(),
            self.rows.clone(),
        );
        sheet.source.clone_from(&self.source);
        sheet
    }

    // --- Search ---

    /// Search forward from cursor for a regex match in the current column.
    ///
    /// Returns the row index of the first match, or `None`.
    #[must_use]
    pub fn search_forward(&self, col_idx: usize, pattern: &str) -> Option<usize> {
        let re = regex::Regex::new(pattern).ok()?;
        let start = self.cursor_row + 1;
        // Search from cursor+1 to end, then wrap to beginning
        for i in (start..self.rows.len()).chain(0..start) {
            if let Some(col) = self.columns.get(col_idx) {
                let display = col.display_value(&self.rows[i]);
                if re.is_match(&display) {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Search backward from cursor for a regex match in the current column.
    #[must_use]
    pub fn search_backward(&self, col_idx: usize, pattern: &str) -> Option<usize> {
        let re = regex::Regex::new(pattern).ok()?;
        let start = self.cursor_row;
        // Search from cursor-1 backwards, then wrap from end
        for i in (0..start).rev().chain((start..self.rows.len()).rev()) {
            if let Some(col) = self.columns.get(col_idx) {
                let display = col.display_value(&self.rows[i]);
                if re.is_match(&display) {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Search forward across all visible columns for a regex match.
    ///
    /// Returns the row index of the first match, or `None`.
    #[must_use]
    pub fn search_forward_all_cols(&self, pattern: &str) -> Option<usize> {
        let re = regex::Regex::new(pattern).ok()?;
        let col_indices: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_hidden())
            .map(|(i, _)| i)
            .collect();
        let start = self.cursor_row + 1;
        for i in (start..self.rows.len()).chain(0..start) {
            for &ci in &col_indices {
                if self
                    .columns
                    .get(ci)
                    .is_some_and(|col| re.is_match(&col.display_value(&self.rows[i])))
                {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Search backward across all visible columns for a regex match.
    #[must_use]
    pub fn search_backward_all_cols(&self, pattern: &str) -> Option<usize> {
        let re = regex::Regex::new(pattern).ok()?;
        let col_indices: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_hidden())
            .map(|(i, _)| i)
            .collect();
        let start = self.cursor_row;
        for i in (0..start).rev().chain((start..self.rows.len()).rev()) {
            for &ci in &col_indices {
                if self
                    .columns
                    .get(ci)
                    .is_some_and(|col| re.is_match(&col.display_value(&self.rows[i])))
                {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Sort by all key columns in the given direction.
    pub fn sort_by_keys(&mut self, direction: SortDirection) {
        let key_indices: Vec<usize> = self
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_key)
            .map(|(i, _)| i)
            .collect();
        if key_indices.is_empty() {
            return;
        }
        self.sort_keys = key_indices
            .into_iter()
            .map(|col_idx| SortKey { col_idx, direction })
            .collect();
        self.apply_sort();
    }

    // --- Frequency ---

    /// Create a frequency table sheet for the given column.
    #[must_use]
    pub fn frequency_sheet(&self, col_idx: usize) -> Self {
        use std::collections::BTreeMap;

        let Some(col) = self.columns.get(col_idx) else {
            return Self::new(format!("{}_freq", self.name));
        };

        // Count occurrences of each display value
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for row in &self.rows {
            let display = col.display_value(row);
            *counts.entry(display).or_insert(0) += 1;
        }

        // Build the frequency sheet
        let columns = vec![
            Column::new(ColumnId(0), &col.name, 0),
            Column::new(ColumnId(1), "count", 1),
        ];

        let rows: Vec<Row> = counts
            .into_iter()
            .map(|(val, count)| {
                #[expect(
                    clippy::cast_possible_wrap,
                    reason = "row counts won't exceed i64::MAX"
                )]
                Row::new(vec![Value::Text(val), Value::Int(count as i64)])
            })
            .collect();

        Self::with_data(format!("{}_freq", col.name), columns, rows)
    }

    // --- Editing ---

    /// Set a cell value and record the old value on the undo stack.
    pub fn set_cell(&mut self, row_idx: usize, col_source_idx: usize, value: Value) {
        let Some(row) = self.rows.get_mut(row_idx) else {
            return;
        };
        let old_value = row.get(col_source_idx).clone();
        row.set(col_source_idx, value);
        self.modified = true;
        self.undo_stack.push(UndoAction::SetCell {
            row_idx,
            col_source_idx,
            old_value,
        });
    }

    /// Fill null cells downward in `col_source_idx` using the last non-null value.
    ///
    /// Records a `BulkSetCell` undo action.
    pub fn fill_down(&mut self, col_source_idx: usize) {
        let mut last_val: Option<Value> = None;
        let mut changes: Vec<(usize, usize, Value)> = Vec::new();

        for row_idx in 0..self.rows.len() {
            let current = self.rows[row_idx].get(col_source_idx).clone();
            if !matches!(current, Value::Null) {
                last_val = Some(current);
            } else if let Some(ref fill) = last_val {
                changes.push((row_idx, col_source_idx, current));
                self.rows[row_idx].set(col_source_idx, fill.clone());
            }
        }

        if !changes.is_empty() {
            self.modified = true;
            self.undo_stack
                .push(UndoAction::BulkSetCell { changes });
        }
    }

    /// Rename a column and record it as an undoable action.
    pub fn rename_column(&mut self, col_id: usize, new_name: String) {
        if let Some(col) = self.columns.iter_mut().find(|c| c.id.0 == col_id) {
            let old_name = std::mem::replace(&mut col.name, new_name);
            self.modified = true;
            self.undo_stack
                .push(UndoAction::RenameColumn { col_id, old_name });
        }
    }

    /// Set a column's type and record it as an undoable action.
    pub fn set_col_type(&mut self, col_id: usize, new_type: crate::column::ColumnType) {
        if let Some(col) = self.columns.iter_mut().find(|c| c.id.0 == col_id) {
            let old_type = col.col_type;
            col.col_type = new_type;
            self.modified = true;
            self.undo_stack
                .push(UndoAction::SetColType { col_id, old_type });
        }
    }

    /// Insert an empty row at the given index and record it for undo.
    pub fn insert_row_at(&mut self, row_idx: usize) {
        let num_values = self.columns.len();
        let row = Row::new(vec![Value::Null; num_values]);
        let idx = row_idx.min(self.rows.len());
        self.rows.insert(idx, row);
        self.modified = true;
        self.undo_stack.push(UndoAction::InsertRow { row_idx: idx });
    }

    /// Delete the row at the given index and record it for undo.
    ///
    /// Returns the deleted row, or `None` if the index is out of bounds.
    pub fn delete_row_at(&mut self, row_idx: usize) -> Option<Row> {
        if row_idx >= self.rows.len() {
            return None;
        }
        let row = self.rows.remove(row_idx);
        self.modified = true;
        self.undo_stack.push(UndoAction::DeleteRow {
            row_idx,
            row: row.clone(),
        });
        self.clamp_cursor();
        Some(row)
    }

    /// Delete all selected rows and record them for undo.
    ///
    /// Returns the number of deleted rows.
    pub fn delete_selected_rows(&mut self) -> usize {
        let mut entries: Vec<(usize, Row)> = Vec::new();
        // Collect indices in reverse order so removals don't shift later indices.
        let indices: Vec<usize> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| r.selected)
            .map(|(i, _)| i)
            .collect();

        for &idx in indices.iter().rev() {
            let row = self.rows.remove(idx);
            entries.push((idx, row));
        }

        let count = entries.len();
        if count > 0 {
            self.modified = true;
            self.undo_stack.push(UndoAction::DeleteRows { entries });
            self.clamp_cursor();
        }
        count
    }

    /// Undo the last mutation, returning `true` if an action was undone.
    pub fn undo(&mut self) -> bool {
        let Some(action) = self.undo_stack.pop() else {
            return false;
        };

        match action {
            UndoAction::SetCell {
                row_idx,
                col_source_idx,
                old_value,
            } => {
                if let Some(row) = self.rows.get_mut(row_idx) {
                    row.set(col_source_idx, old_value);
                }
            }
            UndoAction::BulkSetCell { changes } => {
                for (row_idx, col_source_idx, old_value) in changes {
                    if let Some(row) = self.rows.get_mut(row_idx) {
                        row.set(col_source_idx, old_value);
                    }
                }
            }
            UndoAction::InsertRow { row_idx } => {
                if row_idx < self.rows.len() {
                    self.rows.remove(row_idx);
                }
            }
            UndoAction::DeleteRow { row_idx, row } => {
                let idx = row_idx.min(self.rows.len());
                self.rows.insert(idx, row);
            }
            UndoAction::DeleteRows { entries } => {
                // Re-insert in forward order (entries stored in reverse).
                for (idx, row) in entries.into_iter().rev() {
                    let insert_at = idx.min(self.rows.len());
                    self.rows.insert(insert_at, row);
                }
            }
            UndoAction::RenameColumn { col_id, old_name } => {
                if let Some(col) = self.columns.iter_mut().find(|c| c.id.0 == col_id) {
                    col.name = old_name;
                }
            }
            UndoAction::SetColType { col_id, old_type } => {
                if let Some(col) = self.columns.iter_mut().find(|c| c.id.0 == col_id) {
                    col.col_type = old_type;
                }
            }
            UndoAction::ReorderRows { order } => {
                // Rebuild row order to match the saved RowId sequence.
                let mut rows_by_id: std::collections::HashMap<_, _> = self
                    .rows
                    .drain(..)
                    .map(|r| (r.id, r))
                    .collect();
                for id in order {
                    if let Some(row) = rows_by_id.remove(&id) {
                        self.rows.push(row);
                    }
                }
                // Any rows not in the saved order (shouldn't happen) stay dropped.
            }
        }

        self.clamp_cursor();
        // If undo stack is empty, sheet may no longer be modified.
        if self.undo_stack.is_empty() {
            self.modified = false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sheet() -> Sheet {
        let columns = vec![
            Column::new(ColumnId(0), "name", 0),
            Column::new(ColumnId(1), "age", 1),
            Column::new(ColumnId(2), "salary", 2),
        ];
        let rows = vec![
            Row::new(vec![
                Value::Text("Alice".into()),
                Value::Int(30),
                Value::Float(50000.0),
            ]),
            Row::new(vec![
                Value::Text("Bob".into()),
                Value::Int(25),
                Value::Float(45000.0),
            ]),
            Row::new(vec![
                Value::Text("Carol".into()),
                Value::Int(35),
                Value::Float(60000.0),
            ]),
        ];
        Sheet::with_data("test", columns, rows)
    }

    #[test]
    fn sheet_dimensions() {
        let sheet = sample_sheet();
        assert_eq!(sheet.num_rows(), 3);
        assert_eq!(sheet.num_cols(), 3);
    }

    #[test]
    fn get_cell() {
        let sheet = sample_sheet();
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(sheet.get_cell(1, 1), Value::Int(25));
        assert_eq!(sheet.get_cell(99, 0), Value::Null);
        assert_eq!(sheet.get_cell(0, 99), Value::Null);
    }

    #[test]
    fn cursor_movement() {
        let mut sheet = sample_sheet();
        assert_eq!(sheet.cursor_row, 0);

        sheet.cursor_down(1);
        assert_eq!(sheet.cursor_row, 1);

        sheet.cursor_down(100);
        assert_eq!(sheet.cursor_row, 2); // clamped

        sheet.cursor_up(1);
        assert_eq!(sheet.cursor_row, 1);

        sheet.cursor_up(100);
        assert_eq!(sheet.cursor_row, 0); // clamped
    }

    #[test]
    fn cursor_column_movement() {
        let mut sheet = sample_sheet();
        sheet.cursor_right(1);
        assert_eq!(sheet.cursor_col, 1);

        sheet.cursor_right(100);
        assert_eq!(sheet.cursor_col, 2); // clamped

        sheet.cursor_left(1);
        assert_eq!(sheet.cursor_col, 1);
    }

    #[test]
    fn visible_columns_excludes_hidden() {
        let mut sheet = sample_sheet();
        assert_eq!(sheet.visible_columns().len(), 3);

        sheet.columns[1].width = Some(0); // hide "age"
        assert_eq!(sheet.visible_columns().len(), 2);
    }

    #[test]
    fn add_column_and_row() {
        let mut sheet = Sheet::new("empty");
        sheet.add_column("x", 0);
        sheet.add_column("y", 1);
        sheet.add_row(vec![Value::Int(1), Value::Int(2)]);

        assert_eq!(sheet.num_cols(), 2);
        assert_eq!(sheet.num_rows(), 1);
        assert_eq!(sheet.get_cell(0, 0), Value::Int(1));
    }

    #[test]
    fn selection_count() {
        let mut sheet = sample_sheet();
        assert_eq!(sheet.num_selected(), 0);
        sheet.rows[0].selected = true;
        sheet.rows[2].selected = true;
        assert_eq!(sheet.num_selected(), 2);
    }

    #[test]
    fn current_row_and_column() {
        let sheet = sample_sheet();
        let row = sheet.current_row().expect("should have current row");
        assert_eq!(row.get(0), &Value::Text("Alice".into()));

        let col = sheet.current_column().expect("should have current column");
        assert_eq!(col.name, "name");
    }

    #[test]
    fn current_row_empty_sheet() {
        let sheet = Sheet::new("empty");
        assert!(sheet.current_row().is_none());
        assert!(sheet.current_column().is_none());
    }

    #[test]
    fn cursor_down_empty_sheet() {
        let mut sheet = Sheet::new("empty");
        sheet.cursor_down(1); // should not panic
        assert_eq!(sheet.cursor_row, 0);
    }

    #[test]
    fn clamp_cursor_empty_sheet() {
        let mut sheet = Sheet::new("empty");
        sheet.cursor_row = 5;
        sheet.cursor_col = 3;
        sheet.clamp_cursor();
        assert_eq!(sheet.cursor_row, 0);
        assert_eq!(sheet.cursor_col, 0);
    }

    #[test]
    fn clamp_cursor_after_deletion() {
        let mut sheet = sample_sheet();
        sheet.cursor_row = 2; // last row
        sheet.rows.pop(); // remove last row
        sheet.clamp_cursor();
        assert_eq!(sheet.cursor_row, 1); // clamped to new last
    }

    #[test]
    fn clamp_cursor_after_hide_columns() {
        let mut sheet = sample_sheet();
        sheet.cursor_col = 2; // last visible col
        sheet.columns[0].width = Some(0); // hide first
        sheet.columns[1].width = Some(0); // hide second
        sheet.clamp_cursor();
        assert_eq!(sheet.cursor_col, 0); // only 1 visible col left
    }

    #[test]
    fn get_cell_display() {
        let sheet = sample_sheet();
        assert_eq!(sheet.get_cell_display(0, 0), "Alice");
        assert_eq!(sheet.get_cell_display(1, 1), "25");
        assert_eq!(sheet.get_cell_display(99, 0), ""); // out of bounds
        assert_eq!(sheet.get_cell_display(0, 99), ""); // out of bounds
    }

    #[test]
    fn sheet_source() {
        let mut sheet = Sheet::new("test");
        assert!(sheet.source.is_none());
        sheet.source = Some(std::path::PathBuf::from("/tmp/data.csv"));
        assert_eq!(
            sheet.source.as_ref().unwrap().to_str().unwrap(),
            "/tmp/data.csv"
        );
    }

    #[test]
    fn sheet_unique_ids() {
        let s1 = Sheet::new("a");
        let s2 = Sheet::new("b");
        assert_ne!(s1.id, s2.id);
    }

    #[test]
    fn with_data_sets_name() {
        let sheet = Sheet::with_data(
            "my_sheet",
            vec![Column::new(ColumnId(0), "x", 0)],
            vec![Row::new(vec![Value::Int(1)])],
        );
        assert_eq!(sheet.name, "my_sheet");
        assert_eq!(sheet.num_rows(), 1);
        assert_eq!(sheet.num_cols(), 1);
        assert_eq!(sheet.cursor_row, 0);
        assert_eq!(sheet.cursor_col, 0);
        assert!(sheet.source.is_none());
        assert!(sheet.sort_keys.is_empty());
        assert_eq!(sheet.num_keys, 0);
    }

    // --- Sort tests ---

    #[test]
    fn sort_ascending() {
        let mut sheet = sample_sheet();
        // Sort by age (col 1) ascending: Bob(25) < Alice(30) < Carol(35)
        sheet.sort_by(1, SortDirection::Ascending);
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Bob".into()));
        assert_eq!(sheet.get_cell(1, 0), Value::Text("Alice".into()));
        assert_eq!(sheet.get_cell(2, 0), Value::Text("Carol".into()));
    }

    #[test]
    fn sort_descending() {
        let mut sheet = sample_sheet();
        sheet.sort_by(1, SortDirection::Descending);
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Carol".into()));
        assert_eq!(sheet.get_cell(2, 0), Value::Text("Bob".into()));
    }

    #[test]
    fn sort_by_name_ascending() {
        let mut sheet = sample_sheet();
        sheet.sort_by(0, SortDirection::Ascending);
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(sheet.get_cell(1, 0), Value::Text("Bob".into()));
        assert_eq!(sheet.get_cell(2, 0), Value::Text("Carol".into()));
    }

    #[test]
    fn sort_stable_secondary() {
        // Create sheet with duplicate values in first column
        let columns = vec![
            Column::new(ColumnId(0), "group", 0),
            Column::new(ColumnId(1), "value", 1),
        ];
        let rows = vec![
            Row::new(vec![Value::Text("B".into()), Value::Int(2)]),
            Row::new(vec![Value::Text("A".into()), Value::Int(3)]),
            Row::new(vec![Value::Text("B".into()), Value::Int(1)]),
            Row::new(vec![Value::Text("A".into()), Value::Int(1)]),
        ];
        let mut sheet = Sheet::with_data("test", columns, rows);

        // Primary sort by group, secondary by value
        sheet.sort_by(0, SortDirection::Ascending);
        sheet.sort_by_add(1, SortDirection::Ascending);

        assert_eq!(sheet.get_cell(0, 0), Value::Text("A".into()));
        assert_eq!(sheet.get_cell(0, 1), Value::Int(1));
        assert_eq!(sheet.get_cell(1, 0), Value::Text("A".into()));
        assert_eq!(sheet.get_cell(1, 1), Value::Int(3));
        assert_eq!(sheet.get_cell(2, 0), Value::Text("B".into()));
        assert_eq!(sheet.get_cell(2, 1), Value::Int(1));
    }

    // --- Selection tests ---

    #[test]
    fn select_and_unselect_current() {
        let mut sheet = sample_sheet();
        assert_eq!(sheet.num_selected(), 0);

        sheet.select_current();
        assert_eq!(sheet.num_selected(), 1);
        assert!(sheet.rows[0].selected);

        sheet.unselect_current();
        assert_eq!(sheet.num_selected(), 0);
    }

    #[test]
    fn select_all_unselect_all() {
        let mut sheet = sample_sheet();
        sheet.select_all();
        assert_eq!(sheet.num_selected(), 3);

        sheet.unselect_all();
        assert_eq!(sheet.num_selected(), 0);
    }

    #[test]
    fn toggle_select() {
        let mut sheet = sample_sheet();
        sheet.toggle_select_current();
        assert!(sheet.rows[0].selected);
        sheet.toggle_select_current();
        assert!(!sheet.rows[0].selected);
    }

    #[test]
    fn selected_rows_sheet() {
        let mut sheet = sample_sheet();
        sheet.rows[0].selected = true;
        sheet.rows[2].selected = true;

        let filtered = sheet.selected_rows_sheet();
        assert_eq!(filtered.name, "test_selected");
        assert_eq!(filtered.num_rows(), 2);
        assert_eq!(filtered.num_cols(), 3);
        assert_eq!(filtered.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(filtered.get_cell(1, 0), Value::Text("Carol".into()));
    }

    #[test]
    fn selected_rows_sheet_empty() {
        let sheet = sample_sheet();
        let filtered = sheet.selected_rows_sheet();
        assert_eq!(filtered.num_rows(), 0);
    }

    // --- Search tests ---

    #[test]
    fn search_forward_found() {
        let sheet = sample_sheet();
        let result = sheet.search_forward(0, "Bob");
        assert_eq!(result, Some(1));
    }

    #[test]
    fn search_forward_not_found() {
        let sheet = sample_sheet();
        let result = sheet.search_forward(0, "Zzzz");
        assert_eq!(result, None);
    }

    #[test]
    fn search_forward_regex() {
        let sheet = sample_sheet();
        let result = sheet.search_forward(0, "^C");
        assert_eq!(result, Some(2)); // Carol
    }

    #[test]
    fn search_forward_wraps() {
        let mut sheet = sample_sheet();
        sheet.cursor_row = 2; // at Carol
        let result = sheet.search_forward(0, "Alice");
        assert_eq!(result, Some(0)); // wraps to beginning
    }

    #[test]
    fn search_backward_found() {
        let mut sheet = sample_sheet();
        sheet.cursor_row = 2;
        let result = sheet.search_backward(0, "Alice");
        assert_eq!(result, Some(0));
    }

    #[test]
    fn search_backward_wraps() {
        let sheet = sample_sheet(); // cursor at 0
        let result = sheet.search_backward(0, "Carol");
        assert_eq!(result, Some(2)); // wraps to end
    }

    #[test]
    fn search_invalid_regex() {
        let sheet = sample_sheet();
        let result = sheet.search_forward(0, "[invalid");
        assert_eq!(result, None);
    }

    // --- Frequency tests ---

    #[test]
    fn frequency_sheet_basic() {
        // Create sheet with repeated values
        let columns = vec![Column::new(ColumnId(0), "color", 0)];
        let rows = vec![
            Row::new(vec![Value::Text("red".into())]),
            Row::new(vec![Value::Text("blue".into())]),
            Row::new(vec![Value::Text("red".into())]),
            Row::new(vec![Value::Text("green".into())]),
            Row::new(vec![Value::Text("red".into())]),
        ];
        let sheet = Sheet::with_data("test", columns, rows);
        let freq = sheet.frequency_sheet(0);

        assert_eq!(freq.name, "color_freq");
        assert_eq!(freq.num_cols(), 2);
        assert_eq!(freq.num_rows(), 3); // blue, green, red (sorted by BTreeMap)

        // BTreeMap sorts keys: blue, green, red
        assert_eq!(freq.get_cell(0, 0), Value::Text("blue".into()));
        assert_eq!(freq.get_cell(0, 1), Value::Int(1));
        assert_eq!(freq.get_cell(1, 0), Value::Text("green".into()));
        assert_eq!(freq.get_cell(1, 1), Value::Int(1));
        assert_eq!(freq.get_cell(2, 0), Value::Text("red".into()));
        assert_eq!(freq.get_cell(2, 1), Value::Int(3));
    }

    #[test]
    fn frequency_sheet_empty() {
        let sheet = Sheet::new("empty");
        let freq = sheet.frequency_sheet(0);
        assert_eq!(freq.num_rows(), 0);
    }

    // --- Editing tests ---

    #[test]
    fn set_cell_and_undo() {
        let mut sheet = sample_sheet();
        assert!(!sheet.modified);

        sheet.set_cell(0, 0, Value::Text("Zara".into()));
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Zara".into()));
        assert!(sheet.modified);

        // Undo should restore original value
        assert!(sheet.undo());
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alice".into()));
        assert!(!sheet.modified);
    }

    #[test]
    fn insert_row_and_undo() {
        let mut sheet = sample_sheet();
        assert_eq!(sheet.num_rows(), 3);

        sheet.insert_row_at(1);
        assert_eq!(sheet.num_rows(), 4);
        assert_eq!(sheet.get_cell(1, 0), Value::Null); // new empty row
        assert_eq!(sheet.get_cell(2, 0), Value::Text("Bob".into())); // shifted
        assert!(sheet.modified);

        assert!(sheet.undo());
        assert_eq!(sheet.num_rows(), 3);
        assert_eq!(sheet.get_cell(1, 0), Value::Text("Bob".into()));
    }

    #[test]
    fn insert_row_at_end() {
        let mut sheet = sample_sheet();
        sheet.insert_row_at(100); // beyond bounds
        assert_eq!(sheet.num_rows(), 4);
        assert_eq!(sheet.get_cell(3, 0), Value::Null);
    }

    #[test]
    fn delete_row_and_undo() {
        let mut sheet = sample_sheet();
        let deleted = sheet.delete_row_at(1);
        assert!(deleted.is_some());
        assert_eq!(sheet.num_rows(), 2);
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(sheet.get_cell(1, 0), Value::Text("Carol".into()));

        assert!(sheet.undo());
        assert_eq!(sheet.num_rows(), 3);
        assert_eq!(sheet.get_cell(1, 0), Value::Text("Bob".into()));
    }

    #[test]
    fn delete_row_out_of_bounds() {
        let mut sheet = sample_sheet();
        assert!(sheet.delete_row_at(99).is_none());
        assert!(!sheet.modified);
    }

    #[test]
    fn delete_selected_rows_and_undo() {
        let mut sheet = sample_sheet();
        sheet.rows[0].selected = true;
        sheet.rows[2].selected = true;

        let count = sheet.delete_selected_rows();
        assert_eq!(count, 2);
        assert_eq!(sheet.num_rows(), 1);
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Bob".into()));

        assert!(sheet.undo());
        assert_eq!(sheet.num_rows(), 3);
    }

    #[test]
    fn delete_selected_rows_none() {
        let mut sheet = sample_sheet();
        let count = sheet.delete_selected_rows();
        assert_eq!(count, 0);
        assert!(!sheet.modified);
    }

    #[test]
    fn undo_empty_stack() {
        let mut sheet = sample_sheet();
        assert!(!sheet.undo());
    }

    #[test]
    fn multiple_undos() {
        let mut sheet = sample_sheet();
        sheet.set_cell(0, 0, Value::Text("X".into()));
        sheet.set_cell(0, 0, Value::Text("Y".into()));
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Y".into()));

        sheet.undo();
        assert_eq!(sheet.get_cell(0, 0), Value::Text("X".into()));

        sheet.undo();
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alice".into()));
    }
}
