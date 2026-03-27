//! CSV and TSV file loader.

use std::path::Path;

use anyhow::{Context, Result};
use visidata_core::{Column, ColumnId, Row, Sheet, Value};

use crate::registry::Loader;

/// Loader for CSV and TSV files.
///
/// Uses the `csv` crate with configurable delimiter. TSV uses tab,
/// CSV uses comma (or auto-detected delimiter).
#[derive(Debug)]
pub struct CsvLoader;

impl Loader for CsvLoader {
    fn extensions(&self) -> &[&str] {
        &["csv", "tsv", "tab"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let delimiter = match ext.as_str() {
            "tsv" | "tab" => b'\t',
            _ => b',',
        };

        load_delimited(path, delimiter)
    }
}

/// Load a delimited file with the given separator byte.
fn load_delimited(path: &Path, delimiter: u8) -> Result<Sheet> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unnamed")
        .to_owned();

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(true)
        .from_path(path)
        .with_context(|| format!("failed to open {}", path.display()))?;

    // Build columns from headers
    let headers = reader
        .headers()
        .with_context(|| format!("failed to read headers from {}", path.display()))?
        .clone();

    let columns: Vec<Column> = headers
        .iter()
        .enumerate()
        .map(|(i, name)| Column::new(ColumnId(i), name, i))
        .collect();

    // Read all rows
    let mut rows = Vec::new();
    for result in reader.records() {
        let record = result.with_context(|| format!("failed to read row from {}", path.display()))?;
        let values: Vec<Value> = record
            .iter()
            .map(|field| {
                if field.is_empty() {
                    Value::Null
                } else {
                    Value::Text(field.to_owned())
                }
            })
            .collect();
        rows.push(Row::new(values));
    }

    let mut sheet = Sheet::with_data(name, columns, rows);
    sheet.source = Some(path.to_path_buf());
    Ok(sheet)
}

/// Load a delimited file from a string (for testing and stdin).
///
/// # Errors
///
/// Returns an error if the data cannot be parsed as delimited text.
pub fn load_delimited_from_str(name: &str, data: &str, delimiter: u8) -> Result<Sheet> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(true)
        .from_reader(data.as_bytes());

    let headers = reader.headers()?.clone();
    let columns: Vec<Column> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| Column::new(ColumnId(i), h, i))
        .collect();

    let mut rows = Vec::new();
    for result in reader.records() {
        let record = result?;
        let values: Vec<Value> = record
            .iter()
            .map(|field| {
                if field.is_empty() {
                    Value::Null
                } else {
                    Value::Text(field.to_owned())
                }
            })
            .collect();
        rows.push(Row::new(values));
    }

    Ok(Sheet::with_data(name, columns, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
    }

    #[test]
    fn load_simple_csv() {
        let path = fixtures_dir().join("simple.csv");
        let sheet = CsvLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "simple");
        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.num_rows(), 3);

        // Check column names
        assert_eq!(sheet.columns[0].name, "name");
        assert_eq!(sheet.columns[1].name, "age");
        assert_eq!(sheet.columns[2].name, "city");

        // Check first row
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(sheet.get_cell(0, 1), Value::Text("30".into()));
        assert_eq!(sheet.get_cell(0, 2), Value::Text("New York".into()));

        // Check source path
        assert!(sheet.source.is_some());
    }

    #[test]
    fn load_simple_tsv() {
        let path = fixtures_dir().join("simple.tsv");
        let sheet = CsvLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "simple");
        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.num_rows(), 3);
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(sheet.get_cell(2, 2), Value::Text("Chicago".into()));
    }

    #[test]
    fn load_sample_tsv() {
        let path = fixtures_dir().join("sample.tsv");
        let sheet = CsvLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "sample");
        assert_eq!(sheet.num_cols(), 7);
        assert_eq!(sheet.num_rows(), 43);

        // Check column names match VisiData's sample data
        assert_eq!(sheet.columns[0].name, "OrderDate");
        assert_eq!(sheet.columns[1].name, "Region");
        assert_eq!(sheet.columns[6].name, "Total");

        // Spot check data
        assert_eq!(sheet.get_cell(0, 2), Value::Text("Jones".into()));
    }

    #[test]
    fn load_benchmark_csv() {
        let path = fixtures_dir().join("benchmark.csv");
        let sheet = CsvLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "benchmark");
        assert_eq!(sheet.num_cols(), 7);
        assert!(sheet.num_rows() > 50);

        assert_eq!(sheet.columns[0].name, "Date");
        assert_eq!(sheet.columns[1].name, "Customer");
    }

    #[test]
    fn load_from_str_csv() {
        let data = "x,y,z\n1,2,3\n4,5,6\n";
        let sheet = load_delimited_from_str("test", data, b',').unwrap();

        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.num_rows(), 2);
        assert_eq!(sheet.columns[0].name, "x");
        assert_eq!(sheet.get_cell(0, 0), Value::Text("1".into()));
        assert_eq!(sheet.get_cell(1, 2), Value::Text("6".into()));
    }

    #[test]
    fn load_from_str_tsv() {
        let data = "a\tb\n1\t2\n";
        let sheet = load_delimited_from_str("test", data, b'\t').unwrap();

        assert_eq!(sheet.num_cols(), 2);
        assert_eq!(sheet.num_rows(), 1);
    }

    #[test]
    fn empty_fields_become_null() {
        let data = "a,b,c\n1,,3\n";
        let sheet = load_delimited_from_str("test", data, b',').unwrap();

        assert_eq!(sheet.get_cell(0, 0), Value::Text("1".into()));
        assert_eq!(sheet.get_cell(0, 1), Value::Null);
        assert_eq!(sheet.get_cell(0, 2), Value::Text("3".into()));
    }

    #[test]
    fn csv_with_quoted_fields() {
        let data = "name,desc\nAlice,\"has, comma\"\nBob,\"has \"\"quotes\"\"\"\n";
        let sheet = load_delimited_from_str("test", data, b',').unwrap();

        assert_eq!(sheet.num_rows(), 2);
        assert_eq!(sheet.get_cell(0, 1), Value::Text("has, comma".into()));
        assert_eq!(sheet.get_cell(1, 1), Value::Text("has \"quotes\"".into()));
    }

    #[test]
    fn csv_flexible_ragged_rows() {
        let data = "a,b,c\n1,2\n4,5,6,7\n";
        let sheet = load_delimited_from_str("test", data, b',').unwrap();

        assert_eq!(sheet.num_rows(), 2);
        // First row has 2 values (missing third)
        assert_eq!(sheet.get_cell(0, 0), Value::Text("1".into()));
    }

    #[test]
    fn nonexistent_file() {
        let result = CsvLoader.load(Path::new("/nonexistent/file.csv"));
        assert!(result.is_err());
    }
}
