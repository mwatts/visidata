//! Apache Parquet file loader.

use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow::array::{
    Array, AsArray, BinaryArray, BooleanArray, Float32Array, Float64Array, Int8Array, Int16Array,
    Int32Array, Int64Array, LargeBinaryArray, LargeStringArray, StringArray, UInt8Array,
    UInt16Array, UInt32Array, UInt64Array,
};
use arrow::datatypes::DataType as ArrowType;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use visidata_core::{Column, ColumnId, Row, Sheet, Value};

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
            let num_rows = batch.num_rows();

            for row_idx in 0..num_rows {
                let values: Vec<Value> = (0..num_cols)
                    .map(|col_idx| {
                        let col_array = batch.column(col_idx);
                        arrow_value_at(col_array, row_idx)
                    })
                    .collect();
                rows.push(Row::new(values));
            }
        }

        let mut sheet = Sheet::with_data(&name, columns, rows);
        sheet.source = Some(path.to_path_buf());
        Ok(sheet)
    }
}

/// Extract a `Value` from an Arrow array at the given row index.
fn arrow_value_at(array: &Arc<dyn Array>, idx: usize) -> Value {
    if array.is_null(idx) {
        return Value::Null;
    }

    match array.data_type() {
        ArrowType::Boolean => {
            let arr = array.as_any().downcast_ref::<BooleanArray>().unwrap();
            Value::Bool(arr.value(idx))
        }
        ArrowType::Int8 => {
            let arr = array.as_any().downcast_ref::<Int8Array>().unwrap();
            Value::Int(i64::from(arr.value(idx)))
        }
        ArrowType::Int16 => {
            let arr = array.as_any().downcast_ref::<Int16Array>().unwrap();
            Value::Int(i64::from(arr.value(idx)))
        }
        ArrowType::Int32 => {
            let arr = array.as_any().downcast_ref::<Int32Array>().unwrap();
            Value::Int(i64::from(arr.value(idx)))
        }
        ArrowType::Int64 => {
            let arr = array.as_any().downcast_ref::<Int64Array>().unwrap();
            Value::Int(arr.value(idx))
        }
        ArrowType::UInt8 => {
            let arr = array.as_any().downcast_ref::<UInt8Array>().unwrap();
            Value::Int(i64::from(arr.value(idx)))
        }
        ArrowType::UInt16 => {
            let arr = array.as_any().downcast_ref::<UInt16Array>().unwrap();
            Value::Int(i64::from(arr.value(idx)))
        }
        ArrowType::UInt32 => {
            let arr = array.as_any().downcast_ref::<UInt32Array>().unwrap();
            Value::Int(i64::from(arr.value(idx)))
        }
        ArrowType::UInt64 => {
            let arr = array.as_any().downcast_ref::<UInt64Array>().unwrap();
            // u64 may overflow i64; use text for large values.
            let v = arr.value(idx);
            i64::try_from(v).map_or_else(|_| Value::Text(v.to_string()), Value::Int)
        }
        ArrowType::Float32 => {
            let arr = array.as_any().downcast_ref::<Float32Array>().unwrap();
            Value::Float(f64::from(arr.value(idx)))
        }
        ArrowType::Float64 => {
            let arr = array.as_any().downcast_ref::<Float64Array>().unwrap();
            Value::Float(arr.value(idx))
        }
        ArrowType::Utf8 => {
            let arr = array.as_any().downcast_ref::<StringArray>().unwrap();
            Value::Text(arr.value(idx).to_owned())
        }
        ArrowType::LargeUtf8 => {
            let arr = array.as_any().downcast_ref::<LargeStringArray>().unwrap();
            Value::Text(arr.value(idx).to_owned())
        }
        ArrowType::Binary => {
            let arr = array.as_any().downcast_ref::<BinaryArray>().unwrap();
            Value::Bytes(arr.value(idx).to_vec())
        }
        ArrowType::LargeBinary => {
            let arr = array.as_any().downcast_ref::<LargeBinaryArray>().unwrap();
            Value::Bytes(arr.value(idx).to_vec())
        }
        ArrowType::Date32 | ArrowType::Date64 | ArrowType::Timestamp(_, _) => {
            // Use Arrow's display formatting for date/time types.
            let formatted =
                arrow::util::display::array_value_to_string(array, idx).unwrap_or_default();
            Value::Text(formatted)
        }
        ArrowType::Utf8View => {
            let arr = array.as_string_view();
            Value::Text(arr.value(idx).to_owned())
        }
        // Fallback: use Arrow's string formatting.
        _ => {
            let formatted =
                arrow::util::display::array_value_to_string(array, idx).unwrap_or_default();
            Value::Text(formatted)
        }
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

        // Verify source is set.
        assert!(sheet.source.is_some());

        // Verify all column names are non-empty.
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
        // Try loading a non-parquet file as parquet.
        let path = fixtures_dir().join("simple.csv");
        let result = ParquetLoader.load(&path);
        assert!(result.is_err());
    }
}
