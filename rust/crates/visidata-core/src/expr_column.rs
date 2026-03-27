//! Expression column support.
//!
//! Allows creating computed columns whose values are derived from
//! a Rhai expression evaluated against each row's values.

use crate::column::{Column, ColumnId};
use crate::sheet::Sheet;
use crate::value::Value;

/// Add a computed expression column to a sheet.
///
/// The expression can reference column values by name. For each row,
/// column values are made available as variables, and the expression
/// is evaluated to produce the cell value.
///
/// This is a simplified version that evaluates expressions against
/// a scope containing column values as string variables.
pub fn add_expression_column(sheet: &mut Sheet, name: &str, expression: &str) {
    let col_id = ColumnId(sheet.columns.len());
    let source_idx = sheet.columns.len();
    let col = Column::new(col_id, name, source_idx);
    sheet.columns.push(col);

    // Evaluate the expression for each row and store the result.
    let engine = rhai::Engine::new();
    for row in &mut sheet.rows {
        let mut scope = rhai::Scope::new();

        // Add each column's value as a variable in the scope.
        for (i, c) in sheet.columns.iter().enumerate() {
            if i == source_idx {
                continue; // skip the new column itself
            }
            let val = row.get(c.source_idx);
            match val {
                Value::Int(n) => {
                    scope.push(&c.name, *n);
                }
                Value::Float(f) => {
                    scope.push(&c.name, *f);
                }
                Value::Bool(b) => {
                    scope.push(&c.name, *b);
                }
                Value::Text(s) => {
                    scope.push(&c.name, s.clone());
                }
                _ => {
                    scope.push(&c.name, rhai::Dynamic::UNIT);
                }
            }
        }

        let result = engine.eval_with_scope::<rhai::Dynamic>(&mut scope, expression);
        let value = match result {
            Ok(dyn_val) =>
            {
                #[expect(
                    clippy::option_if_let_else,
                    reason = "multi-branch chain; map_or_else nesting is worse"
                )]
                if let Ok(i) = dyn_val.as_int() {
                    Value::Int(i)
                } else if let Ok(f) = dyn_val.as_float() {
                    Value::Float(f)
                } else if let Ok(b) = dyn_val.as_bool() {
                    Value::Bool(b)
                } else {
                    dyn_val
                        .clone()
                        .into_string()
                        .map_or_else(|_| Value::Text(format!("{dyn_val}")), Value::Text)
                }
            }
            Err(e) => Value::Error(format!("{e}")),
        };

        row.set(source_idx, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::row::Row;

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

    #[test]
    fn add_sum_column() {
        let mut sheet = sample_sheet();
        add_expression_column(&mut sheet, "sum", "a + b");

        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.columns[2].name, "sum");
        assert_eq!(sheet.get_cell(0, 2), Value::Int(30));
        assert_eq!(sheet.get_cell(1, 2), Value::Int(7));
    }

    #[test]
    fn add_multiply_column() {
        let mut sheet = sample_sheet();
        add_expression_column(&mut sheet, "product", "a * b");

        assert_eq!(sheet.get_cell(0, 2), Value::Int(200));
        assert_eq!(sheet.get_cell(1, 2), Value::Int(12));
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

        assert_eq!(sheet.get_cell(0, 1), Value::Text("Hello Alice".into()));
    }

    #[test]
    fn expression_error() {
        let mut sheet = sample_sheet();
        add_expression_column(&mut sheet, "bad", "undefined_var");

        // Should produce an error value.
        assert!(sheet.get_cell(0, 2).is_error());
    }
}
