use std::fmt;

use chrono::NaiveDateTime;

/// A cell value in a `VisiData` sheet.
///
/// Represents all possible data types a cell can hold, including
/// null and error states.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Date(NaiveDateTime),
    Bytes(Vec<u8>),
    Error(String),
}

impl Value {
    /// Returns `true` if this value is `Null`.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Returns `true` if this value is an `Error`.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    /// Returns the value as an `i64`, performing type coercion where possible.
    #[must_use]
    #[expect(clippy::cast_possible_truncation, reason = "intentional coercion from f64 to i64")]
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            Self::Float(f) => Some(*f as i64),
            Self::Bool(b) => Some(i64::from(*b)),
            Self::Text(s) => s.parse().ok(),
            _ => None,
        }
    }

    /// Returns the value as an `f64`, performing type coercion where possible.
    #[must_use]
    #[expect(clippy::cast_precision_loss, reason = "acceptable precision loss for i64 to f64 coercion")]
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Self::Float(f) => Some(*f),
            Self::Int(n) => Some(*n as f64),
            Self::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Self::Text(s) => s.parse().ok(),
            _ => None,
        }
    }

    /// Returns the value as a string reference if it is `Text`.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    /// Returns a type name string for display purposes.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::Text(_) => "str",
            Self::Date(_) => "date",
            Self::Bytes(_) => "bytes",
            Self::Error(_) => "error",
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => Ok(()),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(n) => write!(f, "{n}"),
            Self::Text(s) => write!(f, "{s}"),
            Self::Date(d) => write!(f, "{d}"),
            Self::Bytes(b) => write!(f, "<{} bytes>", b.len()),
            Self::Error(e) => write!(f, "!{e}"),
        }
    }
}

impl PartialOrd for Value {
    #[expect(clippy::cast_precision_loss, reason = "acceptable for mixed int/float comparison")]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Self::Null, Self::Null) => Some(std::cmp::Ordering::Equal),
            (Self::Null, _) => Some(std::cmp::Ordering::Less),
            (_, Self::Null) => Some(std::cmp::Ordering::Greater),
            (Self::Int(a), Self::Int(b)) => a.partial_cmp(b),
            (Self::Float(a), Self::Float(b)) => a.partial_cmp(b),
            (Self::Int(a), Self::Float(b)) => (*a as f64).partial_cmp(b),
            (Self::Float(a), Self::Int(b)) => a.partial_cmp(&(*b as f64)),
            (Self::Text(a), Self::Text(b)) => Some(a.cmp(b)),
            (Self::Bool(a), Self::Bool(b)) => a.partial_cmp(b),
            (Self::Date(a), Self::Date(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Self::Int(n)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Self::Float(n)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Text(s.to_owned())
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<NaiveDateTime> for Value {
    fn from(d: NaiveDateTime) -> Self {
        Self::Date(d)
    }
}

impl From<Option<&str>> for Value {
    fn from(opt: Option<&str>) -> Self {
        opt.map_or(Self::Null, |s| Self::Text(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_values() {
        assert_eq!(Value::Null.to_string(), "");
        assert_eq!(Value::Int(42).to_string(), "42");
        assert_eq!(Value::Float(3.14).to_string(), "3.14");
        assert_eq!(Value::Text("hello".into()).to_string(), "hello");
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::Error("oops".into()).to_string(), "!oops");
        assert_eq!(Value::Bytes(vec![1, 2, 3]).to_string(), "<3 bytes>");
    }

    #[test]
    fn type_coercion_int() {
        assert_eq!(Value::Int(42).as_int(), Some(42));
        assert_eq!(Value::Float(3.14).as_int(), Some(3));
        assert_eq!(Value::Float(-2.9).as_int(), Some(-2));
        assert_eq!(Value::Text("123".into()).as_int(), Some(123));
        assert_eq!(Value::Text("-5".into()).as_int(), Some(-5));
        assert_eq!(Value::Text("abc".into()).as_int(), None);
        assert_eq!(Value::Text("".into()).as_int(), None);
        assert_eq!(Value::Bool(true).as_int(), Some(1));
        assert_eq!(Value::Bool(false).as_int(), Some(0));
        assert_eq!(Value::Null.as_int(), None);
        assert_eq!(Value::Bytes(vec![]).as_int(), None);
        assert_eq!(Value::Error("err".into()).as_int(), None);
    }

    #[test]
    fn type_coercion_float() {
        assert_eq!(Value::Float(3.14).as_float(), Some(3.14));
        assert_eq!(Value::Int(42).as_float(), Some(42.0));
        assert_eq!(Value::Int(-7).as_float(), Some(-7.0));
        assert_eq!(Value::Text("3.14".into()).as_float(), Some(3.14));
        assert_eq!(Value::Text("-1.5".into()).as_float(), Some(-1.5));
        assert_eq!(Value::Text("abc".into()).as_float(), None);
        assert_eq!(Value::Text("".into()).as_float(), None);
        assert_eq!(Value::Bool(true).as_float(), Some(1.0));
        assert_eq!(Value::Bool(false).as_float(), Some(0.0));
        assert_eq!(Value::Null.as_float(), None);
        assert_eq!(Value::Bytes(vec![]).as_float(), None);
    }

    #[test]
    fn type_coercion_text() {
        assert_eq!(Value::Text("hello".into()).as_text(), Some("hello"));
        assert_eq!(Value::Text("".into()).as_text(), Some(""));
        assert_eq!(Value::Int(42).as_text(), None);
        assert_eq!(Value::Null.as_text(), None);
    }

    #[test]
    fn ordering_basics() {
        // Null is less than everything
        assert!(Value::Null < Value::Int(0));
        assert!(Value::Null < Value::Int(-1));
        assert!(Value::Null < Value::Float(0.0));
        assert!(Value::Null < Value::Text("".into()));
        assert!(Value::Null < Value::Bool(false));

        // Same-type ordering
        assert!(Value::Int(1) < Value::Int(2));
        assert!(Value::Int(-1) < Value::Int(0));
        assert!(Value::Float(1.0) < Value::Float(2.0));
        assert!(Value::Text("a".into()) < Value::Text("b".into()));
        assert!(Value::Text("a".into()) < Value::Text("aa".into()));
        assert!(Value::Bool(false) < Value::Bool(true));
    }

    #[test]
    fn ordering_cross_type_int_float() {
        // Int vs Float comparison
        assert!(Value::Int(1) < Value::Float(1.5));
        assert!(Value::Float(0.5) < Value::Int(1));
        assert_eq!(
            Value::Int(1).partial_cmp(&Value::Float(1.0)),
            Some(std::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn ordering_incomparable_types() {
        // Different type families are incomparable (returns None)
        assert_eq!(Value::Int(1).partial_cmp(&Value::Text("1".into())), None);
        assert_eq!(Value::Text("a".into()).partial_cmp(&Value::Bool(true)), None);
        assert_eq!(Value::Int(1).partial_cmp(&Value::Bool(true)), None);
        assert_eq!(
            Value::Error("x".into()).partial_cmp(&Value::Int(1)),
            None
        );
    }

    #[test]
    fn ordering_null_equality() {
        assert_eq!(
            Value::Null.partial_cmp(&Value::Null),
            Some(std::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn ordering_date() {
        let d1 = NaiveDateTime::parse_from_str("2020-01-01 00:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();
        let d2 = NaiveDateTime::parse_from_str("2021-06-15 12:00:00", "%Y-%m-%d %H:%M:%S")
            .unwrap();
        assert!(Value::Date(d1) < Value::Date(d2));
        assert_eq!(
            Value::Date(d1).partial_cmp(&Value::Date(d1)),
            Some(std::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn display_date() {
        let d =
            NaiveDateTime::parse_from_str("2021-07-01 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let display = Value::Date(d).to_string();
        assert!(display.contains("2021"));
        assert!(display.contains("07"));
        assert!(display.contains("01"));
    }

    #[test]
    fn display_edge_cases() {
        assert_eq!(Value::Bool(false).to_string(), "false");
        assert_eq!(Value::Int(0).to_string(), "0");
        assert_eq!(Value::Int(-42).to_string(), "-42");
        assert_eq!(Value::Float(0.0).to_string(), "0");
        // Note: -0.0 displays as "-0" in Rust (IEEE 754 negative zero)
        assert_eq!(Value::Float(-0.0).to_string(), "-0");
        assert_eq!(Value::Text("".into()).to_string(), "");
        assert_eq!(Value::Bytes(vec![]).to_string(), "<0 bytes>");
        assert_eq!(Value::Error("".into()).to_string(), "!");
    }

    #[test]
    fn from_conversions() {
        assert_eq!(Value::from(42_i64), Value::Int(42));
        assert_eq!(Value::from(0_i64), Value::Int(0));
        assert_eq!(Value::from(-1_i64), Value::Int(-1));
        assert_eq!(Value::from(3.14_f64), Value::Float(3.14));
        assert_eq!(Value::from("hello"), Value::Text("hello".into()));
        assert_eq!(Value::from(String::from("owned")), Value::Text("owned".into()));
        assert_eq!(Value::from(true), Value::Bool(true));
        assert_eq!(Value::from(false), Value::Bool(false));
        assert_eq!(Value::from(None::<&str>), Value::Null);
        assert_eq!(Value::from(Some("hi")), Value::Text("hi".into()));
    }

    #[test]
    fn type_names() {
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(Value::Int(0).type_name(), "int");
        assert_eq!(Value::Float(0.0).type_name(), "float");
        assert_eq!(Value::Text(String::new()).type_name(), "str");
        assert_eq!(Value::Bool(false).type_name(), "bool");
        assert_eq!(Value::Date(NaiveDateTime::default()).type_name(), "date");
        assert_eq!(Value::Bytes(vec![]).type_name(), "bytes");
        assert_eq!(Value::Error(String::new()).type_name(), "error");
    }

    #[test]
    fn predicates() {
        assert!(Value::Null.is_null());
        assert!(!Value::Int(1).is_null());
        assert!(!Value::Text("".into()).is_null());
        assert!(!Value::Bool(false).is_null());

        assert!(Value::Error("x".into()).is_error());
        assert!(Value::Error("".into()).is_error());
        assert!(!Value::Int(1).is_error());
        assert!(!Value::Null.is_error());
    }

    #[test]
    fn clone_equality() {
        let values = vec![
            Value::Null,
            Value::Int(42),
            Value::Float(3.14),
            Value::Text("test".into()),
            Value::Bool(true),
            Value::Error("err".into()),
            Value::Bytes(vec![1, 2, 3]),
        ];
        for v in &values {
            assert_eq!(v, &v.clone(), "clone should equal original for {v:?}");
        }
    }

    #[test]
    fn inequality() {
        assert_ne!(Value::Int(1), Value::Int(2));
        assert_ne!(Value::Int(1), Value::Float(1.0)); // different variants
        assert_ne!(Value::Null, Value::Bool(false));
        assert_ne!(Value::Text("a".into()), Value::Text("b".into()));
    }
}
