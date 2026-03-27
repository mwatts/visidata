//! Excel file loader (.xlsx, .xls, .ods).

use std::path::Path;

use anyhow::{Context, Result};
use calamine::{Data, Reader, open_workbook_auto};
use visidata_core::{Column, ColumnId, Row, Sheet, Value};

use crate::registry::Loader;

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

        // Load the first worksheet.
        let first_sheet = sheet_names[0].clone();
        let range = workbook
            .worksheet_range(&first_sheet)
            .with_context(|| format!("failed to read worksheet: {first_sheet}"))?;

        let mut rows_iter = range.rows();

        // First row is treated as headers.
        let Some(header_row) = rows_iter.next() else {
            let mut sheet = Sheet::with_data(&name, vec![], vec![]);
            sheet.source = Some(path.to_path_buf());
            return Ok(sheet);
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

        let mut sheet = Sheet::with_data(&name, columns, rows);
        sheet.source = Some(path.to_path_buf());
        Ok(sheet)
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
