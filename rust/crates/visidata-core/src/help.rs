//! Help sheet builder — creates a Sheet from the command registry.

use crate::column::{Column, ColumnId};
use crate::commands::CommandRegistry;
use crate::row::Row;
use crate::sheet::Sheet;
use crate::value::Value;

/// Build a help sheet listing all commands in the registry.
#[must_use]
pub fn help_sheet(registry: &CommandRegistry) -> Sheet {
    let columns = vec![
        Column::new(ColumnId(0), "keystrokes", 0),
        Column::new(ColumnId(1), "longname", 1),
        Column::new(ColumnId(2), "help", 2),
    ];

    let rows: Vec<Row> = registry
        .all_commands()
        .into_iter()
        .filter(|c| !c.keystrokes.is_empty())
        .map(|cmd| {
            Row::new(vec![
                Value::Text(cmd.keystrokes.clone()),
                Value::Text(cmd.longname.clone()),
                Value::Text(cmd.help.clone()),
            ])
        })
        .collect();

    Sheet::with_data("commands", columns, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::builtin_commands;

    #[test]
    fn help_sheet_has_content() {
        let reg = builtin_commands();
        let sheet = help_sheet(&reg);
        assert_eq!(sheet.name, "commands");
        assert_eq!(sheet.num_cols(), 3);
        assert!(sheet.num_rows() > 20);
    }

    #[test]
    fn help_sheet_columns() {
        let reg = builtin_commands();
        let sheet = help_sheet(&reg);
        assert_eq!(sheet.columns[0].name, "keystrokes");
        assert_eq!(sheet.columns[1].name, "longname");
        assert_eq!(sheet.columns[2].name, "help");
    }

    #[test]
    fn help_sheet_excludes_unbound() {
        let reg = builtin_commands();
        let sheet = help_sheet(&reg);
        // All rows should have non-empty keystrokes
        for i in 0..sheet.num_rows() {
            let ks = sheet.get_cell(i, 0);
            assert!(
                !matches!(ks, Value::Text(ref s) if s.is_empty()),
                "row {i} has empty keystrokes"
            );
        }
    }
}
