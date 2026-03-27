//! Type inference for column values.
//!
//! Examines the values in a column and determines the best `ColumnType`.

use crate::column::ColumnType;
use crate::value::Value;

/// Infer the best `ColumnType` for a slice of values.
///
/// Samples up to `max_sample` values. Returns `Text` if no stronger
/// type can be determined.
#[must_use]
pub fn infer_column_type(values: &[&Value], max_sample: usize) -> ColumnType {
    let sample: Vec<&&Value> = values.iter().take(max_sample).collect();

    if sample.is_empty() {
        return ColumnType::Text;
    }

    // Count how many non-null values match each type
    let mut int_count = 0_usize;
    let mut float_count = 0_usize;
    let mut bool_count = 0_usize;
    let mut non_null = 0_usize;

    for val in &sample {
        match val {
            Value::Null => {}
            Value::Int(_) => {
                int_count += 1;
                non_null += 1;
            }
            Value::Float(_) => {
                float_count += 1;
                non_null += 1;
            }
            Value::Bool(_) => {
                bool_count += 1;
                non_null += 1;
            }
            Value::Text(s) => {
                non_null += 1;
                if s.parse::<i64>().is_ok() {
                    int_count += 1;
                } else if s.parse::<f64>().is_ok() {
                    float_count += 1;
                } else if matches!(s.to_lowercase().as_str(), "true" | "false" | "yes" | "no") {
                    bool_count += 1;
                }
            }
            _ => {
                non_null += 1;
            }
        }
    }

    if non_null == 0 {
        return ColumnType::Text;
    }

    // If all non-null values match a type, use it
    if int_count == non_null {
        return ColumnType::Int;
    }
    // Ints can also be floats
    if int_count + float_count == non_null {
        return ColumnType::Float;
    }
    if bool_count == non_null {
        return ColumnType::Bool;
    }

    ColumnType::Text
}

/// Try to parse a date string using common formats.
///
/// Returns `Some(NaiveDateTime)` on success.
#[must_use]
pub fn parse_date(s: &str) -> Option<chrono::NaiveDateTime> {
    use chrono::NaiveDateTime;

    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%d",
        "%Y/%m/%d",
        "%m/%d/%Y",
        "%d/%m/%Y",
        "%d%m%Y",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
    ];

    let trimmed = s.trim();
    for fmt in &formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, fmt) {
            return Some(dt);
        }
    }

    // Try date-only formats (append midnight)
    let date_formats = ["%Y-%m-%d", "%Y/%m/%d", "%m/%d/%Y", "%d/%m/%Y", "%d%m%Y"];
    for fmt in &date_formats {
        if let Ok(d) = chrono::NaiveDate::parse_from_str(trimmed, fmt) {
            return d.and_hms_opt(0, 0, 0);
        }
    }

    None
}

/// Try to parse a date with a custom format string.
#[must_use]
pub fn parse_custom_date(s: &str, fmt: &str) -> Option<chrono::NaiveDateTime> {
    let trimmed = s.trim();
    chrono::NaiveDateTime::parse_from_str(trimmed, fmt)
        .ok()
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(trimmed, fmt)
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_all_ints() {
        let vals = vec![
            Value::Text("1".into()),
            Value::Text("2".into()),
            Value::Text("3".into()),
        ];
        let refs: Vec<&Value> = vals.iter().collect();
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Int);
    }

    #[test]
    fn infer_all_floats() {
        let vals = vec![
            Value::Text("1.5".into()),
            Value::Text("2.7".into()),
            Value::Text("3.0".into()),
        ];
        let refs: Vec<&Value> = vals.iter().collect();
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Float);
    }

    #[test]
    fn infer_mixed_int_float() {
        let vals = vec![
            Value::Text("1".into()),
            Value::Text("2.5".into()),
            Value::Text("3".into()),
        ];
        let refs: Vec<&Value> = vals.iter().collect();
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Float);
    }

    #[test]
    fn infer_text_fallback() {
        let vals = vec![Value::Text("hello".into()), Value::Text("world".into())];
        let refs: Vec<&Value> = vals.iter().collect();
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Text);
    }

    #[test]
    fn infer_with_nulls() {
        let vals = vec![Value::Null, Value::Text("42".into()), Value::Null];
        let refs: Vec<&Value> = vals.iter().collect();
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Int);
    }

    #[test]
    fn infer_native_ints() {
        let vals = vec![Value::Int(1), Value::Int(2), Value::Int(3)];
        let refs: Vec<&Value> = vals.iter().collect();
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Int);
    }

    #[test]
    fn infer_booleans() {
        let vals = vec![
            Value::Text("true".into()),
            Value::Text("false".into()),
            Value::Text("yes".into()),
        ];
        let refs: Vec<&Value> = vals.iter().collect();
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Bool);
    }

    #[test]
    fn infer_empty() {
        let refs: Vec<&Value> = vec![];
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Text);
    }

    #[test]
    fn infer_all_null() {
        let vals = vec![Value::Null, Value::Null];
        let refs: Vec<&Value> = vals.iter().collect();
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Text);
    }

    #[test]
    fn infer_mixed_breaks_to_text() {
        let vals = vec![
            Value::Text("1".into()),
            Value::Text("hello".into()),
            Value::Text("3".into()),
        ];
        let refs: Vec<&Value> = vals.iter().collect();
        assert_eq!(infer_column_type(&refs, 100), ColumnType::Text);
    }

    // --- Date parsing tests (ported from Python test_date.py) ---

    #[test]
    fn parse_date_iso() {
        let dt = parse_date("2021-07-01").unwrap();
        assert_eq!(dt.date().year(), 2021);
        assert_eq!(dt.date().month(), 7);
        assert_eq!(dt.date().day(), 1);
    }

    use chrono::Datelike;

    #[test]
    fn parse_date_with_time() {
        let dt = parse_date("2021-07-01 14:30:00").unwrap();
        assert_eq!(dt.date().year(), 2021);
        assert_eq!(dt.time().hour(), 14);
    }

    use chrono::Timelike;

    #[test]
    fn parse_date_iso_t() {
        let dt = parse_date("2021-07-01T14:30:00").unwrap();
        assert_eq!(dt.date().year(), 2021);
    }

    #[test]
    fn parse_date_slash() {
        let dt = parse_date("2021/07/01").unwrap();
        assert_eq!(dt.date().year(), 2021);
    }

    #[test]
    fn parse_date_invalid() {
        assert!(parse_date("not a date").is_none());
        assert!(parse_date("").is_none());
    }

    #[test]
    fn parse_custom_date_ddmmyyyy() {
        // From Python test_date.py: customdate('%d%m%Y')
        let dt = parse_custom_date("22092017", "%d%m%Y").unwrap();
        assert_eq!(dt.date().year(), 2017);
        assert_eq!(dt.date().month(), 9);
        assert_eq!(dt.date().day(), 22);
    }

    #[test]
    fn parse_custom_date_comparison() {
        // From Python test_date.py: date(2021,7,1) <= customdate('28092021')
        let threshold = chrono::NaiveDate::from_ymd_opt(2021, 7, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let d1 = parse_custom_date("22092017", "%d%m%Y").unwrap();
        let d2 = parse_custom_date("28092021", "%d%m%Y").unwrap();

        // 2017-09-22 < 2021-07-01 → threshold is NOT <= d1
        assert!(!(threshold <= d1));
        // 2021-09-28 >= 2021-07-01 → threshold IS <= d2
        assert!(threshold <= d2);
    }
}
