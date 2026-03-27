//! Excel file loader (.xlsx, .xls, .ods).
//!
//! When the workbook has multiple sheets an index sheet is returned with an
//! `ExcelDrill` drill-action so the user can open individual sheets via Enter.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use calamine::{Data, Reader, open_workbook_auto};
use visidata_core::{Column, ColumnId, DrillAction, Row, Sheet, Value};

use crate::registry::Loader;

/// Drill-action for navigating to a named worksheet inside an Excel file.
/// Used by single-worksheet links; reads `sheet_name` from `self`.
#[derive(Debug)]
#[expect(dead_code, reason = "available for external callers; constructed via API")]
pub struct ExcelDrill {
    pub path: PathBuf,
    pub sheet_name: String,
}

impl DrillAction for ExcelDrill {
    fn open_row(&self, _row: &visidata_core::Row) -> anyhow::Result<Sheet> {
        load_worksheet(&self.path, &self.sheet_name)
    }
}

/// Drill-action for the index sheet — reads sheet name from row column 0.
#[derive(Debug)]
struct ExcelIndexDrill {
    path: PathBuf,
}

impl DrillAction for ExcelIndexDrill {
    fn open_row(&self, row: &visidata_core::Row) -> anyhow::Result<Sheet> {
        let sheet_name = match row.get(0) {
            Value::Text(s) => s.clone(),
            other => anyhow::bail!("expected sheet name in column 0, got {other:?}"),
        };
        load_worksheet(&self.path, &sheet_name)
    }
}

/// Loader for Excel files using the `calamine` crate.
///
/// Opens the first worksheet by default. Supports `.xlsx`, `.xls`, and `.ods`.
#[derive(Debug)]
pub struct ExcelLoader;

impl Loader for ExcelLoader {
    fn extensions(&self) -> &[&str] {
        &["xlsx", "xls", "ods"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_owned();

        let mut workbook = open_workbook_auto(path)
            .with_context(|| format!("failed to open Excel file: {}", path.display()))?;

        let sheet_names = workbook.sheet_names();

        if sheet_names.is_empty() {
            let mut sheet = Sheet::with_data(&name, vec![], vec![]);
            sheet.source = Some(path.to_path_buf());
            return Ok(sheet);
        }

        // Single sheet → load directly; multiple sheets → index sheet with drill.
        if sheet_names.len() == 1 {
            let mut s = load_worksheet(path, &sheet_names[0])?;
            s.source = Some(path.to_path_buf());
            return Ok(s);
        }

        // Build index sheet: columns "name" and "rows".
        let columns = vec![
            Column::new(ColumnId(0), "name", 0),
            Column::new(ColumnId(1), "rows", 1),
        ];
        let rows: Vec<Row> = sheet_names.iter().map(|sn| {
            let row_count = workbook.worksheet_range(sn)
                .map(|r| r.height().saturating_sub(1))
                .unwrap_or(0);
            #[expect(clippy::cast_possible_wrap, reason = "row count < i64::MAX")]
            Row::new(vec![
                Value::Text(sn.clone()),
                Value::Int(row_count as i64),
            ])
        }).collect();

        let mut index = Sheet::with_data(&name, columns, rows);
        index.source = Some(path.to_path_buf());
        // Each row drills into its named worksheet.
        // ExcelDrill.open_row receives the row but uses its own sheet_name.
        // We need per-row drills — store path+name in each row and use a custom drill.
        // Use a shared drill that reads the sheet name from column 0.
        index.drill = Some(Arc::new(ExcelIndexDrill { path: path.to_path_buf() }));
        Ok(index)
    }
}

/// Convert a calamine cell to a display string (for headers).
fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => f.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::Error(e) => format!("{e:?}"),
        Data::DateTime(dt) => format!("{dt}"),
    }
}

/// Convert a calamine cell to a `Value`.
fn cell_to_value(cell: &Data) -> Value {
    match cell {
        Data::Empty => Value::Null,
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => Value::Text(s.clone()),
        Data::Int(i) => Value::Int(*i),
        Data::Float(f) => Value::Float(*f),
        Data::Bool(b) => Value::Bool(*b),
        Data::Error(e) => Value::Error(format!("{e:?}")),
        Data::DateTime(dt) => Value::Text(format!("{dt}")),
    }
}

/// Load a specific worksheet from an Excel file.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or the sheet does not exist.
pub fn load_worksheet(path: &Path, sheet_name: &str) -> Result<Sheet> {
    let mut workbook = open_workbook_auto(path)
        .with_context(|| format!("failed to open Excel file: {}", path.display()))?;

    let range = workbook
        .worksheet_range(sheet_name)
        .with_context(|| format!("worksheet not found: {sheet_name}"))?;

    let mut rows_iter = range.rows();

    let Some(header_row) = rows_iter.next() else {
        return Ok(Sheet::with_data(sheet_name, vec![], vec![]));
    };

    let columns: Vec<Column> = header_row
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let col_name = cell_to_string(cell);
            Column::new(ColumnId(i), &col_name, i)
        })
        .collect();

    let rows: Vec<Row> = rows_iter
        .map(|row| {
            let values: Vec<Value> = row.iter().map(cell_to_value).collect();
            Row::new(values)
        })
        .collect();

    let mut sheet = Sheet::with_data(sheet_name, columns, rows);
    sheet.source = Some(path.to_path_buf());
    Ok(sheet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonexistent_excel_file() {
        let result = ExcelLoader.load(Path::new("/nonexistent/file.xlsx"));
        assert!(result.is_err());
    }

    #[test]
    fn cell_conversions() {
        assert_eq!(cell_to_value(&Data::Empty), Value::Null);
        assert_eq!(
            cell_to_value(&Data::String("hello".into())),
            Value::Text("hello".into())
        );
        assert_eq!(cell_to_value(&Data::Int(42)), Value::Int(42));
        assert_eq!(cell_to_value(&Data::Float(3.14)), Value::Float(3.14));
        assert_eq!(cell_to_value(&Data::Bool(true)), Value::Bool(true));
    }

    #[test]
    fn cell_to_string_conversions() {
        assert_eq!(cell_to_string(&Data::Empty), "");
        assert_eq!(cell_to_string(&Data::String("Name".into())), "Name");
        assert_eq!(cell_to_string(&Data::Int(42)), "42");
        assert_eq!(cell_to_string(&Data::Bool(true)), "true");
    }
}
