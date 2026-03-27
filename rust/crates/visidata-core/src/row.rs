use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::value::Value;

/// Unique identifier for a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowId(pub u64);

impl fmt::Display for RowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Global row ID counter for unique identification.
static NEXT_ROW_ID: AtomicU64 = AtomicU64::new(1);

fn next_row_id() -> RowId {
    RowId(NEXT_ROW_ID.fetch_add(1, Ordering::Relaxed))
}

/// A row of data in a `VisiData` sheet.
///
/// Each row has a unique ID and a vector of values indexed by column position.
#[derive(Debug, Clone)]
pub struct Row {
    /// Unique identifier for this row.
    pub id: RowId,

    /// Cell values, indexed by column's `source_idx`.
    pub values: Vec<Value>,

    /// Whether this row is selected.
    pub selected: bool,

    /// Whether this row is pending deletion (deferred-modifications mode).
    pub pending_delete: bool,
}

impl Row {
    /// Create a new row with the given values.
    #[must_use]
    pub fn new(values: Vec<Value>) -> Self {
        Self {
            id: next_row_id(),
            values,
            selected: false,
            pending_delete: false,
        }
    }

    /// Returns the number of values in this row.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.values.len()
    }

    /// Returns `true` if this row has no values.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Returns the value at the given index, or `Value::Null` if out of bounds.
    #[must_use]
    pub fn get(&self, index: usize) -> &Value {
        self.values.get(index).unwrap_or(&Value::Null)
    }

    /// Sets the value at the given index, extending with `Null` if needed.
    pub fn set(&mut self, index: usize, value: Value) {
        if index >= self.values.len() {
            self.values.resize(index + 1, Value::Null);
        }
        self.values[index] = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_creation() {
        let row = Row::new(vec![Value::Int(1), Value::Text("hello".into())]);
        assert_eq!(row.len(), 2);
        assert!(!row.is_empty());
        assert!(!row.selected);
    }

    #[test]
    fn row_unique_ids() {
        let r1 = Row::new(vec![]);
        let r2 = Row::new(vec![]);
        assert_ne!(r1.id, r2.id);
    }

    #[test]
    fn row_get() {
        let row = Row::new(vec![Value::Int(42)]);
        assert_eq!(row.get(0), &Value::Int(42));
        assert_eq!(row.get(99), &Value::Null);
    }

    #[test]
    fn row_set_extends() {
        let mut row = Row::new(vec![Value::Int(1)]);
        row.set(3, Value::Text("far".into()));
        assert_eq!(row.len(), 4);
        assert_eq!(row.get(1), &Value::Null);
        assert_eq!(row.get(3), &Value::Text("far".into()));
    }

    #[test]
    fn row_set_overwrites() {
        let mut row = Row::new(vec![Value::Int(1), Value::Int(2)]);
        row.set(0, Value::Text("replaced".into()));
        assert_eq!(row.get(0), &Value::Text("replaced".into()));
        assert_eq!(row.get(1), &Value::Int(2)); // other values unchanged
    }

    #[test]
    fn row_selection() {
        let mut row = Row::new(vec![]);
        assert!(!row.selected);
        row.selected = true;
        assert!(row.selected);
    }

    #[test]
    fn row_empty() {
        let row = Row::new(vec![]);
        assert!(row.is_empty());
        assert_eq!(row.len(), 0);
        assert_eq!(row.get(0), &Value::Null);
    }

    #[test]
    fn row_id_display() {
        let row = Row::new(vec![]);
        let display = row.id.to_string();
        assert!(!display.is_empty());
    }
}
