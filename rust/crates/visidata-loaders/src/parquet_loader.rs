//! Apache Parquet file loader.

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use visidata_core::{Column, ColumnId, Row, Sheet, Value};

use crate::arrow_util::arrow_value_at;
use crate::registry::Loader;

/// Loader for Apache Parquet files.
///
/// Uses the `parquet` and `arrow` crates to read columnar data.
#[derive(Debug)]
pub struct ParquetLoader;

impl Loader for ParquetLoader {
    fn extensions(&self) -> &[&str] {
        &["parquet", "pq"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_owned();

        let file = File::open(path)
            .with_context(|| format!("failed to open Parquet file: {}", path.display()))?;

        let builder = ParquetRecordBatchReaderBuilder::try_new(file)
            .with_context(|| format!("failed to read Parquet metadata: {}", path.display()))?;

        let schema = builder.schema().clone();
        let reader = builder.build().context("failed to build Parquet reader")?;

        // Build columns from the Arrow schema.
        let columns: Vec<Column> = schema
            .fields()
            .iter()
            .enumerate()
            .map(|(i, field)| Column::new(ColumnId(i), field.name().as_str(), i))
            .collect();

        let num_cols = columns.len();

        // Read all record batches and convert to rows.
        let mut rows = Vec::new();
        for batch_result in reader {
            let batch = batch_result.context("failed to read Parquet record batch")?;
            for row_idx in 0..batch.num_rows() {
                let values: Vec<Value> = (0..num_cols)
                    .map(|ci| arrow_value_at(batch.column(ci), row_idx))
                    .collect();
                rows.push(Row::new(values));
            }
        }

        let mut sheet = Sheet::with_data(&name, columns, rows);
        sheet.source = Some(path.to_path_buf());
        Ok(sheet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
    }

    #[test]
    fn load_sample_parquet() {
        let path = fixtures_dir().join("sample.parquet");
        let sheet = ParquetLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "sample");
        assert!(sheet.num_cols() > 0, "should have columns");
        assert!(sheet.num_rows() > 0, "should have rows");
        assert!(sheet.source.is_some());

        for col in &sheet.columns {
            assert!(!col.name.is_empty(), "column name should not be empty");
        }
    }

    #[test]
    fn nonexistent_parquet() {
        let result = ParquetLoader.load(Path::new("/nonexistent/file.parquet"));
        assert!(result.is_err());
    }

    #[test]
    fn invalid_parquet() {
        let path = fixtures_dir().join("simple.csv");
        let result = ParquetLoader.load(&path);
        assert!(result.is_err());
    }
}
