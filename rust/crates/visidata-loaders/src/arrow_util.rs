//! Shared Arrow → `Value` conversion utilities.
//!
//! Used by both the Parquet loader and the external loader host to convert
//! Arrow `RecordBatch` columns into `visidata_core::Value` cells.

use std::sync::Arc;

use arrow::array::{
    Array, AsArray, BinaryArray, BooleanArray, Float32Array, Float64Array, Int8Array, Int16Array,
    Int32Array, Int64Array, LargeBinaryArray, LargeStringArray, StringArray, UInt8Array,
    UInt16Array, UInt32Array, UInt64Array,
};
use arrow::datatypes::DataType as ArrowType;
use visidata_core::Value;

/// Convert a single cell from an Arrow array column to a `Value`.
///
/// Null cells always return `Value::Null`. Types with no direct `Value`
/// equivalent (nested, list, map, etc.) are rendered via Arrow's display
/// formatter and stored as `Value::Text`.
///
/// # Panics
///
/// Panics if the array's `data_type()` does not match its actual runtime type
/// (should not happen with well-formed Arrow data).
#[must_use]
pub fn arrow_value_at(array: &Arc<dyn Array>, idx: usize) -> Value {
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
        ArrowType::Utf8View => {
            let arr = array.as_string_view();
            Value::Text(arr.value(idx).to_owned())
        }
        _ => {
            // Covers Date32, Date64, Timestamp, and all other types —
            // fall back to Arrow's display formatter.
            let formatted =
                arrow::util::display::array_value_to_string(array, idx).unwrap_or_default();
            Value::Text(formatted)
        }
    }
}
