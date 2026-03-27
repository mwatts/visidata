use std::fmt;

use crate::row::Row;
use crate::value::Value;

/// Unique identifier for a column within a sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ColumnId(pub usize);

impl fmt::Display for ColumnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The data type a column is interpreted as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColumnType {
    /// No type coercion; display raw text.
    #[default]
    Text,
    Int,
    Float,
    Bool,
    Date,
    Currency,
}

impl ColumnType {
    /// Returns the single-character indicator for this type (used in status display).
    #[must_use]
    pub const fn indicator(&self) -> &'static str {
        match self {
            Self::Text => "~",
            Self::Int => "#",
            Self::Float => "%",
            Self::Bool => "?",
            Self::Date => "@",
            Self::Currency => "$",
        }
    }

    /// Coerce a `Value` to this column's type.
    ///
    /// For `Text` (the default), the raw value is returned unchanged.
    #[must_use]
    pub fn coerce(&self, value: &Value) -> Value {
        match self {
            Self::Text => value.clone(),
            Self::Int => value.as_int().map_or_else(
                || Value::Error(format!("cannot convert {} to int", value.type_name())),
                Value::Int,
            ),
            Self::Float => value.as_float().map_or_else(
                || Value::Error(format!("cannot convert {} to float", value.type_name())),
                Value::Float,
            ),
            Self::Bool => match value {
                Value::Bool(b) => Value::Bool(*b),
                Value::Int(n) => Value::Bool(*n != 0),
                Value::Text(s) => Value::Bool(!s.is_empty()),
                Value::Null => Value::Bool(false),
                _ => Value::Error(format!("cannot convert {} to bool", value.type_name())),
            },
            Self::Date | Self::Currency => {
                // Placeholder — full parsing in later phases
                Value::Text(value.to_string())
            }
        }
    }
}

/// A column definition within a sheet.
///
/// Columns extract values from rows by index and apply optional type coercion.
#[derive(Debug, Clone)]
pub struct Column {
    /// Column identifier (position in the sheet's column list).
    pub id: ColumnId,

    /// Display name.
    pub name: String,

    /// Index into `Row.values` to extract this column's raw value.
    pub source_idx: usize,

    /// Display width in characters. `None` means auto-fit; `0` means hidden.
    pub width: Option<u16>,

    /// Data type for coercion and display.
    pub col_type: ColumnType,

    /// Whether this column is a key column (used for joins, grouping).
    pub is_key: bool,
}

impl Column {
    /// Create a new column with default settings.
    #[must_use]
    pub fn new(id: ColumnId, name: impl Into<String>, source_idx: usize) -> Self {
        Self {
            id,
            name: name.into(),
            source_idx,
            width: None,
            col_type: ColumnType::default(),
            is_key: false,
        }
    }

    /// Returns the raw value from the row at this column's source index.
    #[must_use]
    pub fn raw_value<'a>(&self, row: &'a Row) -> &'a Value {
        row.values.get(self.source_idx).unwrap_or(&Value::Null)
    }

    /// Returns the typed (coerced) value from the row.
    #[must_use]
    pub fn typed_value(&self, row: &Row) -> Value {
        let raw = self.raw_value(row);
        self.col_type.coerce(raw)
    }

    /// Returns the display string for this column's value in the given row.
    #[must_use]
    pub fn display_value(&self, row: &Row) -> String {
        let typed = self.typed_value(row);
        typed.to_string()
    }

    /// Returns `true` if this column is hidden (width == 0).
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.width == Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_row() -> Row {
        Row::new(vec![
            Value::Text("Alice".into()),
            Value::Int(30),
            Value::Float(55000.50),
            Value::Null,
        ])
    }

    #[test]
    fn raw_value_by_index() {
        let row = sample_row();
        let col = Column::new(ColumnId(0), "name", 0);
        assert_eq!(col.raw_value(&row), &Value::Text("Alice".into()));
    }

    #[test]
    fn raw_value_out_of_bounds_returns_null() {
        let row = sample_row();
        let col = Column::new(ColumnId(0), "missing", 99);
        assert_eq!(col.raw_value(&row), &Value::Null);
    }

    #[test]
    fn typed_value_coercion() {
        let row = sample_row();
        let mut col = Column::new(ColumnId(1), "age", 1);
        col.col_type = ColumnType::Float;
        assert_eq!(col.typed_value(&row), Value::Float(30.0));
    }

    #[test]
    fn display_value_formatting() {
        let row = sample_row();
        let col = Column::new(ColumnId(0), "name", 0);
        assert_eq!(col.display_value(&row), "Alice");
    }

    #[test]
    fn column_hidden() {
        let mut col = Column::new(ColumnId(0), "x", 0);
        assert!(!col.is_hidden());
        col.width = Some(0);
        assert!(col.is_hidden());
    }

    #[test]
    fn column_type_indicators() {
        assert_eq!(ColumnType::Int.indicator(), "#");
        assert_eq!(ColumnType::Float.indicator(), "%");
        assert_eq!(ColumnType::Text.indicator(), "~");
        assert_eq!(ColumnType::Bool.indicator(), "?");
        assert_eq!(ColumnType::Date.indicator(), "@");
        assert_eq!(ColumnType::Currency.indicator(), "$");
    }

    #[test]
    fn coerce_text_passthrough() {
        // Text type (default) returns the raw value unchanged
        let row = sample_row();
        let col = Column::new(ColumnId(1), "age", 1);
        assert_eq!(col.col_type, ColumnType::Text);
        assert_eq!(col.typed_value(&row), Value::Int(30)); // not coerced to Text
    }

    #[test]
    fn coerce_int_from_various() {
        let row = Row::new(vec![
            Value::Int(42),
            Value::Float(3.7),
            Value::Text("99".into()),
            Value::Text("abc".into()),
            Value::Bool(true),
            Value::Null,
        ]);

        let mut col = Column::new(ColumnId(0), "c", 0);
        col.col_type = ColumnType::Int;
        assert_eq!(col.typed_value(&row), Value::Int(42));

        col.source_idx = 1;
        assert_eq!(col.typed_value(&row), Value::Int(3)); // truncated

        col.source_idx = 2;
        assert_eq!(col.typed_value(&row), Value::Int(99)); // parsed from text

        col.source_idx = 3;
        assert!(col.typed_value(&row).is_error()); // "abc" can't be int

        col.source_idx = 4;
        assert_eq!(col.typed_value(&row), Value::Int(1)); // true -> 1

        col.source_idx = 5;
        assert!(col.typed_value(&row).is_error()); // null can't be int
    }

    #[test]
    fn coerce_float_from_various() {
        let row = Row::new(vec![
            Value::Float(3.14),
            Value::Int(42),
            Value::Text("2.5".into()),
            Value::Text("xyz".into()),
            Value::Bool(false),
        ]);

        let mut col = Column::new(ColumnId(0), "c", 0);
        col.col_type = ColumnType::Float;
        assert_eq!(col.typed_value(&row), Value::Float(3.14));

        col.source_idx = 1;
        assert_eq!(col.typed_value(&row), Value::Float(42.0));

        col.source_idx = 2;
        assert_eq!(col.typed_value(&row), Value::Float(2.5));

        col.source_idx = 3;
        assert!(col.typed_value(&row).is_error());

        col.source_idx = 4;
        assert_eq!(col.typed_value(&row), Value::Float(0.0));
    }

    #[test]
    fn coerce_bool_from_various() {
        let row = Row::new(vec![
            Value::Bool(true),
            Value::Int(0),
            Value::Int(5),
            Value::Text("hello".into()),
            Value::Text("".into()),
            Value::Null,
            Value::Float(1.0),
        ]);

        let mut col = Column::new(ColumnId(0), "c", 0);
        col.col_type = ColumnType::Bool;

        assert_eq!(col.typed_value(&row), Value::Bool(true));

        col.source_idx = 1;
        assert_eq!(col.typed_value(&row), Value::Bool(false)); // 0 -> false

        col.source_idx = 2;
        assert_eq!(col.typed_value(&row), Value::Bool(true)); // nonzero -> true

        col.source_idx = 3;
        assert_eq!(col.typed_value(&row), Value::Bool(true)); // non-empty -> true

        col.source_idx = 4;
        assert_eq!(col.typed_value(&row), Value::Bool(false)); // empty -> false

        col.source_idx = 5;
        assert_eq!(col.typed_value(&row), Value::Bool(false)); // null -> false

        col.source_idx = 6;
        assert!(col.typed_value(&row).is_error()); // float can't be bool
    }

    #[test]
    fn coerce_date_placeholder() {
        let row = Row::new(vec![Value::Int(42)]);
        let mut col = Column::new(ColumnId(0), "c", 0);
        col.col_type = ColumnType::Date;
        // Placeholder: converts to text representation
        assert_eq!(col.typed_value(&row), Value::Text("42".into()));
    }

    #[test]
    fn coerce_currency_placeholder() {
        let row = Row::new(vec![Value::Float(19.99)]);
        let mut col = Column::new(ColumnId(0), "c", 0);
        col.col_type = ColumnType::Currency;
        assert_eq!(col.typed_value(&row), Value::Text("19.99".into()));
    }

    #[test]
    fn column_key_flag() {
        let mut col = Column::new(ColumnId(0), "id", 0);
        assert!(!col.is_key);
        col.is_key = true;
        assert!(col.is_key);
    }

    #[test]
    fn column_width_variants() {
        let mut col = Column::new(ColumnId(0), "x", 0);
        assert_eq!(col.width, None); // auto-fit
        assert!(!col.is_hidden());

        col.width = Some(10);
        assert!(!col.is_hidden());

        col.width = Some(0);
        assert!(col.is_hidden());
    }

    #[test]
    fn column_default_type() {
        let col = Column::new(ColumnId(0), "x", 0);
        assert_eq!(col.col_type, ColumnType::Text);
    }

    #[test]
    fn display_value_null() {
        let row = Row::new(vec![Value::Null]);
        let col = Column::new(ColumnId(0), "x", 0);
        assert_eq!(col.display_value(&row), "");
    }

    #[test]
    fn display_value_error() {
        let row = Row::new(vec![Value::Error("broken".into())]);
        let col = Column::new(ColumnId(0), "x", 0);
        assert_eq!(col.display_value(&row), "!broken");
    }
}
