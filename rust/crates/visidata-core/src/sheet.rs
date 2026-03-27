use std::fmt;
use std::path::PathBuf;

use crate::column::{Column, ColumnId};
use crate::row::Row;
use crate::value::Value;

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
}

/// Global sheet ID counter.
static NEXT_SHEET_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(1);

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
        }
    }

    /// Create a sheet from columns and rows.
    #[must_use]
    pub fn with_data(
        name: impl Into<String>,
        columns: Vec<Column>,
        rows: Vec<Row>,
    ) -> Self {
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
}
