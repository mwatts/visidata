//! `SQLite` file loader.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;
use visidata_core::{Column, ColumnId, Row, Sheet, Value};

use crate::registry::Loader;

/// Loader for `SQLite` database files.
///
/// When a `.sqlite` or `.db` file is opened, it produces an index sheet
/// listing all tables and views. Each table can then be loaded individually
/// via [`load_table`].
#[derive(Debug)]
pub struct SqliteLoader;

impl Loader for SqliteLoader {
    fn extensions(&self) -> &[&str] {
        &["sqlite", "sqlite3", "db"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        load_table_index(path)
    }
}

/// Load the table index sheet for a `SQLite` database.
///
/// Returns a sheet with columns: `name`, `type`, `row_count`.
fn load_table_index(path: &Path) -> Result<Sheet> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open SQLite database: {}", path.display()))?;

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed")
        .to_owned();

    let mut stmt = conn
        .prepare(
            "SELECT name, type FROM sqlite_master WHERE type IN ('table', 'view') ORDER BY name",
        )
        .context("failed to query sqlite_master")?;

    let entries: Vec<(String, String)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .context("failed to read sqlite_master")?
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("failed to collect sqlite_master rows")?;

    let columns = vec![
        Column::new(ColumnId(0), "name", 0),
        Column::new(ColumnId(1), "type", 1),
        Column::new(ColumnId(2), "row_count", 2),
    ];

    let mut rows = Vec::with_capacity(entries.len());
    for (tbl_name, tbl_type) in &entries {
        // Get row count for each table/view.
        let count: i64 = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM \"{}\"", tbl_name.replace('"', "\"\"")),
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        rows.push(Row::new(vec![
            Value::Text(tbl_name.clone()),
            Value::Text(tbl_type.clone()),
            Value::Int(count),
        ]));
    }

    let mut sheet = Sheet::with_data(name, columns, rows);
    sheet.source = Some(path.to_path_buf());
    Ok(sheet)
}

/// Load a specific table from a `SQLite` database into a sheet.
///
/// # Errors
///
/// Returns an error if the database cannot be opened or the table does not exist.
pub fn load_table(path: &Path, table_name: &str) -> Result<Sheet> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open SQLite database: {}", path.display()))?;

    // Get column names from the table.
    let stmt = conn
        .prepare(&format!(
            "SELECT * FROM \"{}\" LIMIT 0",
            table_name.replace('"', "\"\"")
        ))
        .with_context(|| format!("failed to prepare query for table: {table_name}"))?;

    let col_names: Vec<String> = stmt.column_names().iter().map(|&s| s.to_owned()).collect();

    let columns: Vec<Column> = col_names
        .iter()
        .enumerate()
        .map(|(i, name)| Column::new(ColumnId(i), name.as_str(), i))
        .collect();

    // Read all rows.
    let mut stmt = conn
        .prepare(&format!(
            "SELECT * FROM \"{}\"",
            table_name.replace('"', "\"\"")
        ))
        .with_context(|| format!("failed to query table: {table_name}"))?;

    let num_cols = col_names.len();
    let rows: Vec<Row> = stmt
        .query_map([], |row| {
            let values: Vec<Value> = (0..num_cols)
                .map(|i| sqlite_value_to_value(row, i))
                .collect();
            Ok(Row::new(values))
        })
        .with_context(|| format!("failed to read rows from table: {table_name}"))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to collect rows from table: {table_name}"))?;

    let mut sheet = Sheet::with_data(table_name, columns, rows);
    sheet.source = Some(path.to_path_buf());
    Ok(sheet)
}

/// Convert a `SQLite` row value at the given index to a `Value`.
fn sqlite_value_to_value(row: &rusqlite::Row<'_>, idx: usize) -> Value {
    // Try types in order: integer, float, text, blob, null.
    if let Ok(v) = row.get::<_, i64>(idx) {
        return Value::Int(v);
    }
    if let Ok(v) = row.get::<_, f64>(idx) {
        return Value::Float(v);
    }
    if let Ok(v) = row.get::<_, String>(idx) {
        return Value::Text(v);
    }
    if let Ok(v) = row.get::<_, Vec<u8>>(idx) {
        return Value::Bytes(v);
    }
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
    }

    #[test]
    fn load_sqlite_index() {
        let path = fixtures_dir().join("employees.sqlite");
        let sheet = SqliteLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "employees");
        // Should have dept, emp, emp_view
        assert!(sheet.num_rows() >= 3);
        assert_eq!(sheet.num_cols(), 3);

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(col_names, vec!["name", "type", "row_count"]);

        // Find the dept row.
        let mut found_dept = false;
        for i in 0..sheet.num_rows() {
            if sheet.get_cell(i, 0) == Value::Text("dept".into()) {
                found_dept = true;
                assert_eq!(sheet.get_cell(i, 1), Value::Text("table".into()));
                assert_eq!(sheet.get_cell(i, 2), Value::Int(4));
            }
        }
        assert!(found_dept, "dept table not found in index");
    }

    #[test]
    fn load_sqlite_table() {
        let path = fixtures_dir().join("employees.sqlite");
        let sheet = load_table(&path, "dept").unwrap();

        assert_eq!(sheet.name, "dept");
        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.num_rows(), 4);

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(col_names, vec!["deptno", "dname", "loc"]);

        // Check first row: 10, ACCOUNTING, NEW YORK
        assert_eq!(sheet.get_cell(0, 0), Value::Int(10));
        assert_eq!(sheet.get_cell(0, 1), Value::Text("ACCOUNTING".into()));
        assert_eq!(sheet.get_cell(0, 2), Value::Text("NEW YORK".into()));
    }

    #[test]
    fn load_sqlite_emp_table() {
        let path = fixtures_dir().join("employees.sqlite");
        let sheet = load_table(&path, "emp").unwrap();

        assert_eq!(sheet.name, "emp");
        assert_eq!(sheet.num_cols(), 8);
        assert!(sheet.num_rows() > 0);

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(col_names.contains(&"empno"));
        assert!(col_names.contains(&"ename"));
        assert!(col_names.contains(&"sal"));
    }

    #[test]
    fn load_sqlite_view() {
        let path = fixtures_dir().join("employees.sqlite");
        let sheet = load_table(&path, "emp_view").unwrap();

        // View should have same columns and rows as emp.
        assert_eq!(sheet.num_cols(), 8);
        assert!(sheet.num_rows() > 0);
    }

    #[test]
    fn nonexistent_sqlite() {
        let result = SqliteLoader.load(Path::new("/nonexistent/file.sqlite"));
        assert!(result.is_err());
    }

    #[test]
    fn nonexistent_table() {
        let path = fixtures_dir().join("employees.sqlite");
        let result = load_table(&path, "nonexistent_table");
        assert!(result.is_err());
    }
}
