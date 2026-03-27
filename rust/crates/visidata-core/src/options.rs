//! Hierarchical options system.
//!
//! Options resolve through a chain from most specific to least specific:
//! instance → class/type → global → default.
//!
//! Ported from Python `VisiData`'s `settings.py` / `OPTIONS.md`.

use std::collections::HashMap;

use crate::value::Value;

/// Metadata for a single option declaration.
#[derive(Debug, Clone)]
pub struct OptionDef {
    /// Option name (`snake_case`).
    pub name: String,
    /// Default value.
    pub default: Value,
    /// Help text.
    pub help: String,
    /// Module that declared this option.
    pub module: String,
    /// Whether this option is recorded in replay logs.
    pub replayable: bool,
}

/// Context level for option resolution.
const LEVEL_DEFAULT: &str = "default";
const LEVEL_GLOBAL: &str = "global";

/// Hierarchical options manager.
///
/// Storage: `{ option_name: { context_key: Value } }`
///
/// Resolution chain (highest to lowest precedence):
/// 1. Instance context (e.g., sheet name)
/// 2. Type context (e.g., "Sheet", "`CsvLoader`")
/// 3. Global
/// 4. Default
#[derive(Debug, Default)]
pub struct OptionsManager {
    /// Option declarations (name → definition).
    defs: HashMap<String, OptionDef>,

    /// Option values: `{ name: { context_key: Value } }`.
    values: HashMap<String, HashMap<String, Value>>,
}

impl OptionsManager {
    /// Create a new empty options manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare an option with a default value.
    pub fn declare(&mut self, name: &str, default: Value, help: &str) {
        self.declare_full(name, default, help, "", true);
    }

    /// Declare an option with full metadata.
    pub fn declare_full(
        &mut self,
        name: &str,
        default: Value,
        help: &str,
        module: &str,
        replayable: bool,
    ) {
        // Store the default value
        self.values
            .entry(name.to_owned())
            .or_default()
            .insert(LEVEL_DEFAULT.to_owned(), default.clone());

        self.defs.insert(
            name.to_owned(),
            OptionDef {
                name: name.to_owned(),
                default,
                help: help.to_owned(),
                module: module.to_owned(),
                replayable,
            },
        );
    }

    /// Set an option at the global level.
    pub fn set_global(&mut self, name: &str, value: Value) {
        self.values
            .entry(name.to_owned())
            .or_default()
            .insert(LEVEL_GLOBAL.to_owned(), value);
    }

    /// Set an option for a specific context (e.g., sheet name or type name).
    pub fn set(&mut self, name: &str, value: Value, context: &str) {
        self.values
            .entry(name.to_owned())
            .or_default()
            .insert(context.to_owned(), value);
    }

    /// Get an option value, resolving through the context chain.
    ///
    /// `contexts` is a list of context keys from most to least specific,
    /// e.g., `["my_sheet", "Sheet", "global", "default"]`.
    #[must_use]
    pub fn get(&self, name: &str, contexts: &[&str]) -> Value {
        let Some(values) = self.values.get(name) else {
            return Value::Null;
        };

        for ctx in contexts {
            if let Some(val) = values.get(*ctx) {
                return val.clone();
            }
        }

        // Fallback to global, then default
        if let Some(val) = values.get(LEVEL_GLOBAL) {
            return val.clone();
        }
        if let Some(val) = values.get(LEVEL_DEFAULT) {
            return val.clone();
        }

        Value::Null
    }

    /// Get an option using the standard sheet resolution chain:
    /// instance name → type name → global → default.
    #[must_use]
    pub fn get_for_sheet(&self, name: &str, sheet_name: &str, sheet_type: &str) -> Value {
        self.get(name, &[sheet_name, sheet_type, LEVEL_GLOBAL, LEVEL_DEFAULT])
    }

    /// Get an option at the global level (skipping instance/type contexts).
    #[must_use]
    pub fn get_global(&self, name: &str) -> Value {
        self.get(name, &[LEVEL_GLOBAL, LEVEL_DEFAULT])
    }

    /// Check if an option is explicitly set on a specific context (not inherited).
    #[must_use]
    pub fn get_only(&self, name: &str, context: &str) -> Option<&Value> {
        self.values.get(name).and_then(|vals| vals.get(context))
    }

    /// Returns the option definition, if declared.
    #[must_use]
    pub fn definition(&self, name: &str) -> Option<&OptionDef> {
        self.defs.get(name)
    }

    /// Returns all declared option definitions, sorted by name.
    #[must_use]
    pub fn all_definitions(&self) -> Vec<&OptionDef> {
        let mut defs: Vec<&OptionDef> = self.defs.values().collect();
        defs.sort_by_key(|d| &d.name);
        defs
    }

    /// Returns the number of declared options.
    #[must_use]
    pub fn len(&self) -> usize {
        self.defs.len()
    }

    /// Returns true if no options have been declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

/// Build an options manager with `VisiData`'s built-in options.
#[must_use]
pub fn builtin_options() -> OptionsManager {
    let mut opts = OptionsManager::new();
    declare_display_options(&mut opts);
    declare_behavior_options(&mut opts);
    declare_color_options(&mut opts);
    opts
}

fn declare_display_options(opts: &mut OptionsManager) {
    for (name, val, help) in [
        ("disp_date_fmt", Value::Text("%Y-%m-%d".into()), "default date format"),
        ("disp_float_fmt", Value::Text("%.02f".into()), "default float format"),
        ("disp_int_fmt", Value::Text("%d".into()), "default int format"),
        ("disp_note_none", Value::Text("⌀".into()), "note for null values"),
        ("disp_truncator", Value::Text("…".into()), "truncation indicator"),
        ("disp_oddspace", Value::Text("·".into()), "character for odd whitespace"),
        ("disp_column_sep", Value::Text("│".into()), "column separator"),
        ("default_width", Value::Int(20), "default column width"),
        ("min_col_width", Value::Int(3), "minimum column width"),
        ("max_col_width", Value::Int(80), "maximum column width for auto-fit"),
    ] {
        opts.declare(name, val, help);
    }
}

fn declare_behavior_options(opts: &mut OptionsManager) {
    for (name, val, help) in [
        ("encoding", Value::Text("utf-8".into()), "file encoding"),
        ("encoding_errors", Value::Text("surrogateescape".into()), "encoding error handler"),
        ("bulk_select_clear", Value::Bool(false), "clear selection before bulk select"),
        ("wrap", Value::Bool(false), "wrap cell text in display"),
        ("quitguard", Value::Bool(false), "confirm before quitting modified sheet"),
        ("null_value", Value::Text(String::new()), "string to treat as null on load"),
        ("csv_delimiter", Value::Text(",".into()), "CSV field delimiter"),
        ("csv_quotechar", Value::Text("\"".into()), "CSV quote character"),
        ("tsv_safe_newline", Value::Text("\\n".into()), "TSV newline escape"),
    ] {
        opts.declare(name, val, help);
    }
}

fn declare_color_options(opts: &mut OptionsManager) {
    for (name, val, help) in [
        ("color_default", Value::Text("normal".into()), "default color"),
        ("color_key_col", Value::Text("bold".into()), "color for key columns"),
        ("color_selected_row", Value::Text("cyan".into()), "color for selected rows"),
        ("color_cursor_row", Value::Text("reverse".into()), "color for cursor row"),
        ("color_header", Value::Text("bold underline".into()), "color for header row"),
    ] {
        opts.declare(name, val, help);
    }
}

/// Build a sheet displaying all options and their current values.
#[must_use]
pub fn options_sheet(opts: &OptionsManager) -> crate::sheet::Sheet {
    use crate::column::{Column, ColumnId};
    use crate::row::Row;

    let columns = vec![
        Column::new(ColumnId(0), "option", 0),
        Column::new(ColumnId(1), "value", 1),
        Column::new(ColumnId(2), "default", 2),
        Column::new(ColumnId(3), "help", 3),
    ];

    let rows: Vec<Row> = opts
        .all_definitions()
        .into_iter()
        .map(|def| {
            let current = opts.get_global(&def.name);
            Row::new(vec![
                Value::Text(def.name.clone()),
                current,
                def.default.clone(),
                Value::Text(def.help.clone()),
            ])
        })
        .collect();

    crate::sheet::Sheet::with_data("options", columns, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declare_and_get_default() {
        let mut opts = OptionsManager::new();
        opts.declare("foo", Value::Int(42), "test option");
        assert_eq!(opts.get_global("foo"), Value::Int(42));
    }

    #[test]
    fn set_global_overrides_default() {
        let mut opts = OptionsManager::new();
        opts.declare("foo", Value::Int(42), "test");
        opts.set_global("foo", Value::Int(100));
        assert_eq!(opts.get_global("foo"), Value::Int(100));
    }

    #[test]
    fn context_overrides_global() {
        let mut opts = OptionsManager::new();
        opts.declare("foo", Value::Int(42), "test");
        opts.set_global("foo", Value::Int(100));
        opts.set("foo", Value::Int(200), "my_sheet");

        // With sheet context, should get 200
        assert_eq!(
            opts.get_for_sheet("foo", "my_sheet", "Sheet"),
            Value::Int(200)
        );

        // Without that sheet context, should get global
        assert_eq!(
            opts.get_for_sheet("foo", "other_sheet", "Sheet"),
            Value::Int(100)
        );
    }

    #[test]
    fn type_context_overrides_global() {
        let mut opts = OptionsManager::new();
        opts.declare("foo", Value::Int(42), "test");
        opts.set("foo", Value::Int(300), "CsvSheet");

        assert_eq!(
            opts.get_for_sheet("foo", "my_csv", "CsvSheet"),
            Value::Int(300)
        );

        // Other type doesn't get CsvSheet override
        assert_eq!(
            opts.get_for_sheet("foo", "my_json", "JsonSheet"),
            Value::Int(42)
        );
    }

    #[test]
    fn instance_overrides_type() {
        let mut opts = OptionsManager::new();
        opts.declare("foo", Value::Int(42), "test");
        opts.set("foo", Value::Int(300), "CsvSheet");
        opts.set("foo", Value::Int(999), "my_csv");

        assert_eq!(
            opts.get_for_sheet("foo", "my_csv", "CsvSheet"),
            Value::Int(999)
        );
    }

    #[test]
    fn get_only_checks_specific_context() {
        let mut opts = OptionsManager::new();
        opts.declare("foo", Value::Int(42), "test");
        opts.set_global("foo", Value::Int(100));

        assert!(opts.get_only("foo", "global").is_some());
        assert!(opts.get_only("foo", "my_sheet").is_none());
    }

    #[test]
    fn undeclared_option_returns_null() {
        let opts = OptionsManager::new();
        assert_eq!(opts.get_global("nonexistent"), Value::Null);
    }

    #[test]
    fn definition_lookup() {
        let mut opts = OptionsManager::new();
        opts.declare("foo", Value::Int(42), "help text");
        let def = opts.definition("foo").unwrap();
        assert_eq!(def.name, "foo");
        assert_eq!(def.help, "help text");
        assert!(opts.definition("bar").is_none());
    }

    #[test]
    fn all_definitions_sorted() {
        let mut opts = OptionsManager::new();
        opts.declare("zebra", Value::Int(1), "z");
        opts.declare("alpha", Value::Int(2), "a");
        opts.declare("middle", Value::Int(3), "m");

        let defs = opts.all_definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "middle", "zebra"]);
    }

    #[test]
    fn len_and_empty() {
        let mut opts = OptionsManager::new();
        assert!(opts.is_empty());
        assert_eq!(opts.len(), 0);

        opts.declare("x", Value::Int(1), "");
        assert!(!opts.is_empty());
        assert_eq!(opts.len(), 1);
    }

    #[test]
    fn builtin_options_has_content() {
        let opts = builtin_options();
        assert!(opts.len() > 20);
        assert!(opts.definition("encoding").is_some());
        assert!(opts.definition("csv_delimiter").is_some());
        assert!(opts.definition("disp_date_fmt").is_some());
    }

    #[test]
    fn options_sheet_has_rows() {
        let opts = builtin_options();
        let sheet = options_sheet(&opts);
        assert_eq!(sheet.name, "options");
        assert_eq!(sheet.num_cols(), 4);
        assert!(sheet.num_rows() > 20);
    }

    #[test]
    fn options_sheet_columns() {
        let opts = builtin_options();
        let sheet = options_sheet(&opts);
        assert_eq!(sheet.columns[0].name, "option");
        assert_eq!(sheet.columns[1].name, "value");
        assert_eq!(sheet.columns[2].name, "default");
        assert_eq!(sheet.columns[3].name, "help");
    }

    #[test]
    fn full_resolution_chain() {
        let mut opts = OptionsManager::new();
        opts.declare("color", Value::Text("white".into()), "");
        // Default = white
        assert_eq!(
            opts.get_for_sheet("color", "s1", "Sheet"),
            Value::Text("white".into())
        );

        opts.set_global("color", Value::Text("blue".into()));
        // Global = blue
        assert_eq!(
            opts.get_for_sheet("color", "s1", "Sheet"),
            Value::Text("blue".into())
        );

        opts.set("color", Value::Text("green".into()), "Sheet");
        // Type = green
        assert_eq!(
            opts.get_for_sheet("color", "s1", "Sheet"),
            Value::Text("green".into())
        );

        opts.set("color", Value::Text("red".into()), "s1");
        // Instance = red
        assert_eq!(
            opts.get_for_sheet("color", "s1", "Sheet"),
            Value::Text("red".into())
        );

        // Other sheet still gets type-level
        assert_eq!(
            opts.get_for_sheet("color", "s2", "Sheet"),
            Value::Text("green".into())
        );
    }
}
