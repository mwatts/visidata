//! Clipboard for yank/paste of cells and rows.

use crate::row::Row;
use crate::value::Value;

/// Clipboard contents — either cells or whole rows.
#[derive(Debug, Clone, Default)]
pub enum ClipboardContent {
    /// No content.
    #[default]
    Empty,
    /// A single cell value.
    Cell(Value),
    /// One or more rows.
    Rows(Vec<Row>),
}

/// Application-level clipboard.
#[derive(Debug, Default)]
pub struct Clipboard {
    content: ClipboardContent,
}

impl Clipboard {
    /// Create a new empty clipboard.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Yank (copy) a single cell value.
    pub fn yank_cell(&mut self, value: Value) {
        self.content = ClipboardContent::Cell(value);
    }

    /// Yank (copy) one or more rows.
    pub fn yank_rows(&mut self, rows: Vec<Row>) {
        self.content = ClipboardContent::Rows(rows);
    }

    /// Returns the clipboard content.
    #[must_use]
    pub const fn content(&self) -> &ClipboardContent {
        &self.content
    }

    /// Returns `true` if the clipboard is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self.content, ClipboardContent::Empty)
    }

    /// Clear the clipboard.
    pub fn clear(&mut self) {
        self.content = ClipboardContent::Empty;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_clipboard() {
        let cb = Clipboard::new();
        assert!(cb.is_empty());
    }

    #[test]
    fn yank_cell() {
        let mut cb = Clipboard::new();
        cb.yank_cell(Value::Int(42));
        assert!(!cb.is_empty());
        assert!(matches!(
            cb.content(),
            ClipboardContent::Cell(Value::Int(42))
        ));
    }

    #[test]
    fn yank_rows() {
        let mut cb = Clipboard::new();
        cb.yank_rows(vec![Row::new(vec![Value::Int(1)])]);
        assert!(matches!(cb.content(), ClipboardContent::Rows(_)));
    }

    #[test]
    fn clear() {
        let mut cb = Clipboard::new();
        cb.yank_cell(Value::Int(1));
        cb.clear();
        assert!(cb.is_empty());
    }
}
