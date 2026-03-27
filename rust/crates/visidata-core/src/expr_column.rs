//! Expression column support — lazy per-render evaluation (GAP-103).
//!
//! `add_expression_column` sets `col.expr` and reserves a `source_idx` slot
//! but does **not** materialise values at add time.  The renderer calls
//! `col.eval_expr_value(engine, columns, row)` each frame so values are
//! always current even after edits, sorts, or filters.

use crate::column::{Column, ColumnId};
use crate::sheet::Sheet;

/// Add a lazy computed expression column to a sheet.
///
/// The column is given a `source_idx` slot (initialised to `Null`) and its
/// expression is stored in `col.expr`.  The actual evaluation happens at
/// render time via `Column::eval_expr_value`.
pub fn add_expression_column(sheet: &mut Sheet, name: &str, expression: &str) {
    let col_id = ColumnId(sheet.columns.len());
    let source_idx = sheet.columns.len();
    let mut col = Column::new(col_id, name, source_idx);
    col.expr = Some(expression.to_owned());
    // Extend existing rows with a Null placeholder (filled lazily at render).
    for row in &mut sheet.rows {
        row.set(source_idx, crate::value::Value::Null);
    }
    sheet.columns.push(col);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::ColumnId;
    use crate::row::Row;
    use crate::value::Value;

    fn sample_sheet() -> Sheet {
        let columns = vec![
            Column::new(ColumnId(0), "a", 0),
            Column::new(ColumnId(1), "b", 1),
        ];
        let rows = vec![
            Row::new(vec![Value::Int(10), Value::Int(20)]),
            Row::new(vec![Value::Int(3), Value::Int(4)]),
        ];
        Sheet::with_data("test", columns, rows)
    }

    /// Evaluate all expression columns for all rows using a fresh Engine.
    fn eval_all(sheet: &Sheet) -> Vec<Vec<Value>> {
        let engine = rhai::Engine::new();
        sheet.rows.iter().map(|row| {
            sheet.columns.iter().map(|col| {
                col.eval_expr_value(&engine, &sheet.columns, row)
            }).collect()
        }).collect()
    }

    #[test]
    fn add_sum_column() {
        let mut sheet = sample_sheet();
        add_expression_column(&mut sheet, "sum", "a + b");

        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.columns[2].name, "sum");
        assert!(sheet.columns[2].expr.is_some());

        let vals = eval_all(&sheet);
        assert_eq!(vals[0][2], Value::Int(30));
        assert_eq!(vals[1][2], Value::Int(7));
    }

    #[test]
    fn add_multiply_column() {
        let mut sheet = sample_sheet();
        add_expression_column(&mut sheet, "product", "a * b");

        let vals = eval_all(&sheet);
        assert_eq!(vals[0][2], Value::Int(200));
        assert_eq!(vals[1][2], Value::Int(12));
    }

    #[test]
    fn expression_with_string() {
        let columns = vec![Column::new(ColumnId(0), "name", 0)];
        let rows = vec![
            Row::new(vec![Value::Text("Alice".into())]),
            Row::new(vec![Value::Text("Bob".into())]),
        ];
        let mut sheet = Sheet::with_data("test", columns, rows);
        add_expression_column(&mut sheet, "greeting", r#""Hello " + name"#);

        let engine = rhai::Engine::new();
        let val = sheet.columns[1].eval_expr_value(&engine, &sheet.columns, &sheet.rows[0]);
        assert_eq!(val, Value::Text("Hello Alice".into()));
    }

    #[test]
    fn expression_error() {
        let mut sheet = sample_sheet();
        add_expression_column(&mut sheet, "bad", "undefined_var");

        let engine = rhai::Engine::new();
        let val = sheet.columns[2].eval_expr_value(&engine, &sheet.columns, &sheet.rows[0]);
        assert!(val.is_error());
    }

    #[test]
    fn expr_column_placeholder_is_null() {
        // Before eval, the slot in each row is Null.
        let mut sheet = sample_sheet();
        add_expression_column(&mut sheet, "computed", "a + b");
        assert_eq!(sheet.rows[0].get(2), &Value::Null);
    }

    #[test]
    fn expr_col_reflects_mutation() {
        // After editing a source cell, re-evaluation picks up the new value.
        let mut sheet = sample_sheet();
        add_expression_column(&mut sheet, "sum", "a + b");
        sheet.set_cell(0, 0, Value::Int(100)); // change `a` in row 0

        let engine = rhai::Engine::new();
        let val = sheet.columns[2].eval_expr_value(&engine, &sheet.columns, &sheet.rows[0]);
        assert_eq!(val, Value::Int(120)); // 100 + 20
    }
}
