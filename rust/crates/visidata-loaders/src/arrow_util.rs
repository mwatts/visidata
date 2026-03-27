//! Shared Arrow → `Value` conversion utilities.
//!
//! Handles every Arrow type that `DuckDB` (and Parquet) emit, including
//! Decimal128/256, Date32/64, Timestamp, Time32/64, Duration, Interval,
//! Float16, Dictionary (enum columns), `FixedSizeBinary`, `BinaryView`,
//! and nested types (List, Struct, Map → JSON strings).

use std::sync::Arc;

use arrow::array::{
    Array, AsArray, BinaryArray, BinaryViewArray, BooleanArray, Date32Array, Date64Array,
    Decimal128Array, Decimal256Array, FixedSizeBinaryArray, Float16Array, Float32Array,
    Float64Array, Int8Array, Int16Array, Int32Array, Int64Array, LargeBinaryArray,
    LargeStringArray, StringArray, TimestampMicrosecondArray, TimestampMillisecondArray,
    TimestampNanosecondArray, TimestampSecondArray, UInt8Array, UInt16Array, UInt32Array,
    UInt64Array,
};
use arrow::datatypes::{DataType as ArrowType, TimeUnit};
use visidata_core::Value;

/// Convert a single cell from an Arrow array column to a `Value`.
///
/// Every concrete Arrow type is handled:
/// - Integers → `Value::Int`
/// - Floats (including Float16, Decimal) → `Value::Float`
/// - Booleans → `Value::Bool`
/// - Dates/Timestamps → `Value::Date` (UTC midnight for Date32/64)
/// - Times/Durations/Intervals → `Value::Text` (ISO 8601 strings)
/// - Strings/Binary → `Value::Text` / `Value::Bytes`
/// - Dictionary (enum) → recurse into the values array
/// - Null → `Value::Null`
/// - Nested (List, Struct, Map, Union, `FixedSizeList`) → `Value::Text`
///   containing the Arrow display string
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
        // ── Null ─────────────────────────────────────────────────────────────
        ArrowType::Null => Value::Null,

        // ── Boolean ──────────────────────────────────────────────────────────
        ArrowType::Boolean => Value::Bool(
            array
                .as_any()
                .downcast_ref::<BooleanArray>()
                .unwrap()
                .value(idx),
        ),

        // ── Signed integers ───────────────────────────────────────────────────
        ArrowType::Int8 => Value::Int(i64::from(
            array
                .as_any()
                .downcast_ref::<Int8Array>()
                .unwrap()
                .value(idx),
        )),
        ArrowType::Int16 => Value::Int(i64::from(
            array
                .as_any()
                .downcast_ref::<Int16Array>()
                .unwrap()
                .value(idx),
        )),
        ArrowType::Int32 => Value::Int(i64::from(
            array
                .as_any()
                .downcast_ref::<Int32Array>()
                .unwrap()
                .value(idx),
        )),
        ArrowType::Int64 => Value::Int(
            array
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .value(idx),
        ),

        // ── Unsigned integers ─────────────────────────────────────────────────
        ArrowType::UInt8 => Value::Int(i64::from(
            array
                .as_any()
                .downcast_ref::<UInt8Array>()
                .unwrap()
                .value(idx),
        )),
        ArrowType::UInt16 => Value::Int(i64::from(
            array
                .as_any()
                .downcast_ref::<UInt16Array>()
                .unwrap()
                .value(idx),
        )),
        ArrowType::UInt32 => Value::Int(i64::from(
            array
                .as_any()
                .downcast_ref::<UInt32Array>()
                .unwrap()
                .value(idx),
        )),
        ArrowType::UInt64 => {
            let v = array
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap()
                .value(idx);
            i64::try_from(v).map_or_else(|_| Value::Text(v.to_string()), Value::Int)
        }

        // ── Floats ────────────────────────────────────────────────────────────
        ArrowType::Float16 => {
            let v = array
                .as_any()
                .downcast_ref::<Float16Array>()
                .unwrap()
                .value(idx);
            Value::Float(f64::from(v))
        }
        ArrowType::Float32 => Value::Float(f64::from(
            array
                .as_any()
                .downcast_ref::<Float32Array>()
                .unwrap()
                .value(idx),
        )),
        ArrowType::Float64 => Value::Float(
            array
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(idx),
        ),

        // ── Decimal ───────────────────────────────────────────────────────────
        // Represent as Float — precision and scale are in the type metadata.
        ArrowType::Decimal128(precision, scale) => {
            let arr = array.as_any().downcast_ref::<Decimal128Array>().unwrap();
            let raw = arr.value(idx); // i128 unscaled
            decimal128_to_float(raw, *precision, *scale)
        }
        ArrowType::Decimal256(precision, scale) => {
            let arr = array.as_any().downcast_ref::<Decimal256Array>().unwrap();
            let raw = arr.value(idx); // i256
            // Convert via string to avoid i256 arithmetic — display is exact.
            let s = format_decimal256(raw, *precision, *scale);
            s.parse::<f64>().map(Value::Float).unwrap_or(Value::Text(s))
        }

        // ── Dates → Value::Date ───────────────────────────────────────────────
        ArrowType::Date32 => {
            let days = array
                .as_any()
                .downcast_ref::<Date32Array>()
                .unwrap()
                .value(idx);
            // Date32: days since 1970-01-01
            date32_to_value(days)
        }
        ArrowType::Date64 => {
            let ms = array
                .as_any()
                .downcast_ref::<Date64Array>()
                .unwrap()
                .value(idx);
            // Date64: milliseconds since 1970-01-01 midnight UTC
            date64_to_value(ms)
        }

        // ── Timestamps → Value::Date ──────────────────────────────────────────
        ArrowType::Timestamp(unit, _tz) => timestamp_to_value(array, idx, *unit),

        // ── Times, Durations, Intervals → Value::Text (ISO 8601) ──────────────
        // Arrow's display formatter produces clean ISO 8601 strings for all of these.
        #[expect(
            clippy::match_same_arms,
            reason = "clarity: each arm is a distinct type category"
        )]
        ArrowType::Time32(_)
        | ArrowType::Time64(_)
        | ArrowType::Duration(_)
        | ArrowType::Interval(_) => display_value(array, idx),

        // ── Strings ───────────────────────────────────────────────────────────
        ArrowType::Utf8 => Value::Text(
            array
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap()
                .value(idx)
                .to_owned(),
        ),
        ArrowType::LargeUtf8 => Value::Text(
            array
                .as_any()
                .downcast_ref::<LargeStringArray>()
                .unwrap()
                .value(idx)
                .to_owned(),
        ),
        ArrowType::Utf8View => Value::Text(array.as_string_view().value(idx).to_owned()),

        // ── Binary ────────────────────────────────────────────────────────────
        ArrowType::Binary => Value::Bytes(
            array
                .as_any()
                .downcast_ref::<BinaryArray>()
                .unwrap()
                .value(idx)
                .to_vec(),
        ),
        ArrowType::LargeBinary => Value::Bytes(
            array
                .as_any()
                .downcast_ref::<LargeBinaryArray>()
                .unwrap()
                .value(idx)
                .to_vec(),
        ),
        ArrowType::BinaryView => Value::Bytes(
            array
                .as_any()
                .downcast_ref::<BinaryViewArray>()
                .unwrap()
                .value(idx)
                .to_vec(),
        ),
        ArrowType::FixedSizeBinary(_) => Value::Bytes(
            array
                .as_any()
                .downcast_ref::<FixedSizeBinaryArray>()
                .unwrap()
                .value(idx)
                .to_vec(),
        ),

        // ── Dictionary (enum columns) ─────────────────────────────────────────
        // Recursively decode the value from the values array.
        ArrowType::Dictionary(_, _) => {
            let values = arrow::compute::cast(array, array.data_type())
                .ok()
                .as_ref()
                .and_then(|_a| {
                    // Cast dictionary to its value type and recurse.
                    let value_type = match array.data_type() {
                        ArrowType::Dictionary(_, v) => v.as_ref().clone(),
                        _ => return None,
                    };
                    arrow::compute::cast(array, &value_type).ok()
                });
            values
                .as_ref()
                .map_or_else(|| display_value(array, idx), |v| arrow_value_at(v, idx))
        }

        // ── Nested (List, Struct, Map, Union, FixedSizeList, etc.) ────────────
        // Serialise to the Arrow display string. These become Value::Text.
        _ => display_value(array, idx),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Format using Arrow's built-in display, returning `Value::Text`.
fn display_value(array: &Arc<dyn Array>, idx: usize) -> Value {
    Value::Text(arrow::util::display::array_value_to_string(array, idx).unwrap_or_default())
}

/// Convert a `Decimal128` raw unscaled value to `Value::Float`.
#[expect(
    clippy::cast_precision_loss,
    reason = "i128 → f64 loses mantissa bits for very large decimals; acceptable for display"
)]
fn decimal128_to_float(raw: i128, _precision: u8, scale: i8) -> Value {
    if scale >= 0 {
        let divisor = 10_f64.powi(i32::from(scale));
        Value::Float(raw as f64 / divisor)
    } else {
        let multiplier = 10_f64.powi(i32::from(-scale));
        Value::Float(raw as f64 * multiplier)
    }
}

/// Format a `Decimal256` value as a decimal string, then parse to float.
fn format_decimal256(raw: arrow::datatypes::i256, _precision: u8, scale: i8) -> String {
    // i256 has a Display impl that gives the unscaled integer.
    let unscaled = raw.to_string();
    if scale == 0 {
        return unscaled;
    }
    // Insert decimal point at the right position.
    let negative = unscaled.starts_with('-');
    let digits = if negative { &unscaled[1..] } else { &unscaled };
    let scale_usize = scale.unsigned_abs() as usize;
    let result = if scale > 0 {
        if digits.len() <= scale_usize {
            let padding = "0".repeat(scale_usize - digits.len() + 1);
            let padded = format!("{padding}{digits}");
            let (int_part, frac_part) = padded.split_at(padded.len() - scale_usize);
            format!("{int_part}.{frac_part}")
        } else {
            let (int_part, frac_part) = digits.split_at(digits.len() - scale_usize);
            format!("{int_part}.{frac_part}")
        }
    } else {
        format!("{digits}{}", "0".repeat(scale_usize))
    };
    if negative {
        format!("-{result}")
    } else {
        result
    }
}

/// Convert a Date32 value (days since epoch) to `Value::Date`.
fn date32_to_value(days: i32) -> Value {
    use chrono::NaiveDate;
    let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
    let date = epoch + chrono::Duration::days(i64::from(days));
    date.and_hms_opt(0, 0, 0)
        .map_or_else(|| Value::Text(date.to_string()), Value::Date)
}

/// Convert a Date64 value (milliseconds since epoch) to `Value::Date`.
fn date64_to_value(ms: i64) -> Value {
    use chrono::DateTime;
    DateTime::from_timestamp_millis(ms).map_or_else(
        || Value::Text(ms.to_string()),
        |dt| Value::Date(dt.naive_utc()),
    )
}

/// Convert a Timestamp array cell to `Value::Date`.
fn timestamp_to_value(array: &Arc<dyn Array>, idx: usize, unit: TimeUnit) -> Value {
    use chrono::DateTime;

    let epoch_nanos: Option<i64> = match unit {
        TimeUnit::Second => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampSecondArray>()
                .unwrap();
            arr.value(idx).checked_mul(1_000_000_000)
        }
        TimeUnit::Millisecond => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampMillisecondArray>()
                .unwrap();
            arr.value(idx).checked_mul(1_000_000)
        }
        TimeUnit::Microsecond => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampMicrosecondArray>()
                .unwrap();
            arr.value(idx).checked_mul(1_000)
        }
        TimeUnit::Nanosecond => {
            let arr = array
                .as_any()
                .downcast_ref::<TimestampNanosecondArray>()
                .unwrap();
            Some(arr.value(idx))
        }
    };

    epoch_nanos
        .and_then(|ns| {
            let secs = ns / 1_000_000_000;
            #[expect(
                clippy::cast_sign_loss,
                reason = "nanosecond remainder is always 0..1_000_000_000"
            )]
            let nanos = (ns % 1_000_000_000) as u32;
            DateTime::from_timestamp(secs, nanos)
        })
        .map_or_else(
            || display_value(array, idx),
            |dt| Value::Date(dt.naive_utc()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{
        BooleanArray, Float32Array, Float64Array, Int32Array, Int64Array, StringArray, UInt64Array,
    };
    use arrow::datatypes::{Field, Schema};
    use std::sync::Arc;

    fn wrap<T: Array + 'static>(arr: T) -> Arc<dyn Array> {
        Arc::new(arr)
    }

    #[test]
    fn null_cell() {
        let arr = wrap(Int32Array::from(vec![None::<i32>]));
        assert_eq!(arrow_value_at(&arr, 0), Value::Null);
    }

    #[test]
    fn bool_true() {
        let arr = wrap(BooleanArray::from(vec![true]));
        assert_eq!(arrow_value_at(&arr, 0), Value::Bool(true));
    }

    #[test]
    fn int32() {
        let arr = wrap(Int32Array::from(vec![42_i32]));
        assert_eq!(arrow_value_at(&arr, 0), Value::Int(42));
    }

    #[test]
    fn int64() {
        let arr = wrap(Int64Array::from(vec![i64::MAX]));
        assert_eq!(arrow_value_at(&arr, 0), Value::Int(i64::MAX));
    }

    #[test]
    fn uint64_fits_i64() {
        let arr = wrap(UInt64Array::from(vec![100_u64]));
        assert_eq!(arrow_value_at(&arr, 0), Value::Int(100));
    }

    #[test]
    fn uint64_overflow_becomes_text() {
        let arr = wrap(UInt64Array::from(vec![u64::MAX]));
        assert!(matches!(arrow_value_at(&arr, 0), Value::Text(_)));
    }

    #[test]
    fn float32() {
        let arr = wrap(Float32Array::from(vec![3.14_f32]));
        if let Value::Float(f) = arrow_value_at(&arr, 0) {
            assert!((f - 3.14).abs() < 0.001);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn float64() {
        let arr = wrap(Float64Array::from(vec![2.718_f64]));
        assert_eq!(arrow_value_at(&arr, 0), Value::Float(2.718));
    }

    #[test]
    fn utf8_string() {
        let arr = wrap(StringArray::from(vec!["hello"]));
        assert_eq!(arrow_value_at(&arr, 0), Value::Text("hello".into()));
    }

    #[test]
    fn decimal128_positive_scale() {
        // 12345 with scale 2 → 123.45
        let arr = wrap(
            Decimal128Array::from(vec![12345_i128])
                .with_precision_and_scale(10, 2)
                .unwrap(),
        );
        if let Value::Float(f) = arrow_value_at(&arr, 0) {
            assert!((f - 123.45).abs() < 0.001);
        } else {
            panic!("expected Float");
        }
    }

    #[test]
    fn decimal128_zero_scale() {
        let arr = wrap(
            Decimal128Array::from(vec![42_i128])
                .with_precision_and_scale(10, 0)
                .unwrap(),
        );
        assert_eq!(arrow_value_at(&arr, 0), Value::Float(42.0));
    }

    #[test]
    fn date32_epoch() {
        use chrono::Datelike;
        // Day 0 = 1970-01-01
        let arr = wrap(Date32Array::from(vec![0_i32]));
        if let Value::Date(dt) = arrow_value_at(&arr, 0) {
            assert_eq!(dt.year(), 1970);
            assert_eq!(dt.month(), 1);
            assert_eq!(dt.day(), 1);
        } else {
            panic!("expected Date");
        }
    }

    #[test]
    fn date32_known_date() {
        use chrono::{Datelike, NaiveDate};
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
        let target = NaiveDate::from_ymd_opt(2023, 6, 15).unwrap();
        let days = (target - epoch).num_days() as i32;
        let arr = wrap(Date32Array::from(vec![days]));
        if let Value::Date(dt) = arrow_value_at(&arr, 0) {
            assert_eq!(dt.year(), 2023);
            assert_eq!(dt.month(), 6);
            assert_eq!(dt.day(), 15);
        } else {
            panic!("expected Date");
        }
    }

    #[test]
    fn timestamp_second() {
        use chrono::Datelike;
        // Unix timestamp 0 = 1970-01-01 00:00:00
        let arr = wrap(TimestampSecondArray::from(vec![0_i64]));
        if let Value::Date(dt) = arrow_value_at(&arr, 0) {
            assert_eq!(dt.year(), 1970);
        } else {
            panic!("expected Date");
        }
    }

    #[test]
    fn timestamp_microsecond() {
        use chrono::Timelike;
        let arr = wrap(TimestampMicrosecondArray::from(vec![1_000_000_i64])); // 1 second
        if let Value::Date(dt) = arrow_value_at(&arr, 0) {
            assert_eq!(dt.second(), 1);
        } else {
            panic!("expected Date");
        }
    }

    #[test]
    fn binary_bytes() {
        let arr = wrap(BinaryArray::from(vec![b"hello".as_ref()]));
        assert_eq!(arrow_value_at(&arr, 0), Value::Bytes(b"hello".to_vec()));
    }

    #[test]
    fn format_decimal256_basic() {
        // 12345 scale=2 → "123.45"
        let raw = arrow::datatypes::i256::from_i128(12345);
        assert_eq!(format_decimal256(raw, 10, 2), "123.45");
    }

    #[test]
    fn format_decimal256_zero_scale() {
        let raw = arrow::datatypes::i256::from_i128(42);
        assert_eq!(format_decimal256(raw, 10, 0), "42");
    }

    #[test]
    fn format_decimal256_negative() {
        let raw = arrow::datatypes::i256::from_i128(-12345);
        assert_eq!(format_decimal256(raw, 10, 2), "-123.45");
    }
}
