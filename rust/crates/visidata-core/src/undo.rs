//! Undo/redo system for sheet mutations.

use crate::column::ColumnType;
use crate::row::Row;
use crate::value::Value;

/// A reversible action recorded on the undo stack.
#[derive(Debug, Clone)]
pub enum UndoAction {
    /// A cell was changed: (`row_idx`, `col_source_idx`, `old_value`).
    SetCell {
        row_idx: usize,
        col_source_idx: usize,
        old_value: Value,
    },
    /// Multiple cells were changed in bulk (e.g. fill-down, delete-cells).
    BulkSetCell { changes: Vec<(usize, usize, Value)> },
    /// A row was inserted at the given index.
    InsertRow { row_idx: usize },
    /// A row was deleted from the given index (stores the removed row).
    DeleteRow { row_idx: usize, row: Row },
    /// Multiple rows were deleted (stores them in reverse index order).
    DeleteRows { entries: Vec<(usize, Row)> },
    /// A column was renamed.
    RenameColumn { col_id: usize, old_name: String },
    /// A column's type was changed.
    SetColType { col_id: usize, old_type: ColumnType },
    /// Rows were reordered (sort); stores the original row order by `RowId`.
    ReorderRows { order: Vec<crate::row::RowId> },
}

/// Stack of undo actions for a sheet.
#[derive(Debug, Default)]
pub struct UndoStack {
    actions: Vec<UndoAction>,
}

impl UndoStack {
    /// Create a new empty undo stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push an action onto the undo stack.
    pub fn push(&mut self, action: UndoAction) {
        self.actions.push(action);
    }

    /// Pop the most recent action from the undo stack.
    pub fn pop(&mut self) -> Option<UndoAction> {
        self.actions.pop()
    }

    /// Returns the number of actions on the stack.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.actions.len()
    }

    /// Returns `true` if the stack is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Clear the undo stack.
    pub fn clear(&mut self) {
        self.actions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stack() {
        let stack = UndoStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn push_and_pop() {
        let mut stack = UndoStack::new();
        stack.push(UndoAction::SetCell {
            row_idx: 0,
            col_source_idx: 1,
            old_value: Value::Int(42),
        });
        assert_eq!(stack.len(), 1);
        assert!(!stack.is_empty());

        let action = stack.pop().unwrap();
        assert!(matches!(action, UndoAction::SetCell { row_idx: 0, .. }));
        assert!(stack.is_empty());
    }

    #[test]
    fn clear_stack() {
        let mut stack = UndoStack::new();
        stack.push(UndoAction::InsertRow { row_idx: 0 });
        stack.push(UndoAction::InsertRow { row_idx: 1 });
        assert_eq!(stack.len(), 2);

        stack.clear();
        assert!(stack.is_empty());
    }

    #[test]
    fn lifo_order() {
        let mut stack = UndoStack::new();
        stack.push(UndoAction::SetCell {
            row_idx: 0,
            col_source_idx: 0,
            old_value: Value::Text("first".into()),
        });
        stack.push(UndoAction::SetCell {
            row_idx: 1,
            col_source_idx: 0,
            old_value: Value::Text("second".into()),
        });

        if let Some(UndoAction::SetCell { old_value, .. }) = stack.pop() {
            assert_eq!(old_value, Value::Text("second".into()));
        } else {
            panic!("expected SetCell");
        }

        if let Some(UndoAction::SetCell { old_value, .. }) = stack.pop() {
            assert_eq!(old_value, Value::Text("first".into()));
        } else {
            panic!("expected SetCell");
        }
    }
}
