//! Command registry and keybinding system.
//!
//! Commands have a longname (e.g., "sort-asc"), keystrokes (e.g., "["),
//! and a help string. The handler is stored separately in the TUI layer
//! since it needs access to `App`.

use std::collections::HashMap;

/// Metadata for a registered command (without the handler).
#[derive(Debug, Clone)]
pub struct CommandInfo {
    /// Unique command name (kebab-case, e.g., "sort-asc").
    pub longname: String,

    /// Primary keystroke(s) that trigger this command (e.g., "[", "g[").
    pub keystrokes: String,

    /// User-facing help text.
    pub help: String,

    /// Whether this command is replayable in macros.
    pub replayable: bool,
}

/// Registry of command metadata and keystroke → longname mappings.
#[derive(Debug, Default)]
pub struct CommandRegistry {
    /// All registered commands, keyed by longname.
    commands: HashMap<String, CommandInfo>,

    /// Keystroke → longname mapping.
    bindings: HashMap<String, String>,

    /// Known prefix strings (e.g., "g", "z", "gz").
    prefixes: Vec<String>,
}

impl CommandRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefixes: vec!["g".into(), "z".into(), "gz".into()],
            ..Self::default()
        }
    }

    /// Register a command.
    pub fn add(&mut self, keystrokes: &str, longname: &str, help: &str) {
        let info = CommandInfo {
            longname: longname.to_owned(),
            keystrokes: keystrokes.to_owned(),
            help: help.to_owned(),
            replayable: true,
        };
        self.commands.insert(longname.to_owned(), info);
        if !keystrokes.is_empty() {
            self.bindings
                .insert(keystrokes.to_owned(), longname.to_owned());
        }
    }

    /// Look up a command longname by keystrokes.
    #[must_use]
    pub fn lookup_by_keystroke(&self, keystrokes: &str) -> Option<&str> {
        self.bindings.get(keystrokes).map(String::as_str)
    }

    /// Look up command info by longname.
    #[must_use]
    pub fn lookup_by_name(&self, longname: &str) -> Option<&CommandInfo> {
        self.commands.get(longname)
    }

    /// Returns `true` if the given string is a known prefix.
    #[must_use]
    pub fn is_prefix(&self, s: &str) -> bool {
        self.prefixes.iter().any(|p| p == s)
    }

    /// Add a custom prefix.
    pub fn add_prefix(&mut self, prefix: &str) {
        if !self.is_prefix(prefix) {
            self.prefixes.push(prefix.to_owned());
        }
    }

    /// Returns all registered commands, sorted by longname.
    #[must_use]
    pub fn all_commands(&self) -> Vec<&CommandInfo> {
        let mut cmds: Vec<&CommandInfo> = self.commands.values().collect();
        cmds.sort_by_key(|c| &c.longname);
        cmds
    }

    /// Fuzzy-search commands by longname or help text.
    #[must_use]
    pub fn search_commands(&self, query: &str) -> Vec<&CommandInfo> {
        let query_lower = query.to_lowercase();
        let mut results: Vec<&CommandInfo> = self
            .commands
            .values()
            .filter(|c| {
                c.longname.to_lowercase().contains(&query_lower)
                    || c.help.to_lowercase().contains(&query_lower)
            })
            .collect();
        results.sort_by_key(|c| &c.longname);
        results
    }

    /// Returns all registered keystroke bindings.
    #[must_use]
    pub const fn all_bindings(&self) -> &HashMap<String, String> {
        &self.bindings
    }

    /// Accumulate keystrokes and determine the outcome.
    ///
    /// Returns `(outcome, keystrokes)` where outcome is one of:
    /// - `"execute"` — a bound command was found
    /// - `"prefix"` — waiting for more keystrokes
    /// - `"duplicate"` — duplicate prefix detected (reset)
    /// - `"no-command"` — no command or prefix matches
    #[must_use]
    pub fn accumulate(&self, pending: &str, new_key: &str) -> (KeystrokeOutcome, String) {
        let potential = format!("{pending}{new_key}");

        // Check for duplicate prefix (e.g., "gg" typed as g, then g again
        // when "g" is a prefix but not bound, and the new key is also a prefix
        // that already appeared)
        if !pending.is_empty()
            && self.is_prefix(new_key)
            && pending.contains(new_key)
            && !self.is_prefix(&potential)
            && self.lookup_by_keystroke(&potential).is_none()
        {
            return (KeystrokeOutcome::Duplicate, new_key.to_owned());
        }

        if self.lookup_by_keystroke(&potential).is_some() {
            return (KeystrokeOutcome::Execute, potential);
        }

        if self.is_prefix(&potential) {
            return (KeystrokeOutcome::Prefix, potential);
        }

        (KeystrokeOutcome::NoCommand, potential)
    }

    /// Returns the number of registered commands.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Returns `true` if no commands are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// Outcome of keystroke accumulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeystrokeOutcome {
    /// A bound command was found — execute it.
    Execute,
    /// Waiting for more keystrokes (current string is a known prefix).
    Prefix,
    /// Duplicate prefix detected — reset keystroke buffer.
    Duplicate,
    /// No command or prefix matches.
    NoCommand,
}

/// Build a registry with all built-in `VisiData` commands.
#[must_use]
pub fn builtin_commands() -> CommandRegistry {
    let mut reg = CommandRegistry::new();

    // Navigation
    reg.add("j", "cursor-down", "move cursor down one row");
    reg.add("k", "cursor-up", "move cursor up one row");
    reg.add("l", "cursor-right", "move cursor right one column");
    reg.add("h", "cursor-left", "move cursor left one column");
    reg.add("gj", "go-bottom", "move cursor to last row");
    reg.add("gk", "go-top", "move cursor to first row");
    reg.add("g", "go-top", "move cursor to first row");

    // Sheet
    reg.add("q", "quit-sheet", "quit current sheet");

    // Column operations
    reg.add("_", "resize-col-max", "auto-fit column width");
    reg.add("-", "hide-col", "hide current column");
    reg.add("^", "rename-col", "rename current column");
    reg.add("!", "key-col", "toggle key column");

    // Type conversion
    reg.add("#", "type-int", "set column type to int");
    reg.add("%", "type-float", "set column type to float");
    reg.add("$", "type-currency", "set column type to currency");
    reg.add("~", "type-string", "set column type to string");
    reg.add("@", "type-date", "set column type to date");

    // Sorting
    reg.add("[", "sort-asc", "sort ascending by current column");
    reg.add("]", "sort-desc", "sort descending by current column");

    // Selection
    reg.add("s", "select-row", "select current row");
    reg.add("u", "unselect-row", "unselect current row");
    reg.add("t", "toggle-row", "toggle selection on current row");

    // Search
    reg.add("/", "search-col", "search forward in current column");
    reg.add(
        "?",
        "search-col-backward",
        "search backward in current column",
    );
    reg.add("n", "search-next", "repeat search forward");
    reg.add("N", "search-prev", "repeat search backward");

    // Filter / Frequency
    reg.add("\"", "dup-selected", "push sheet of selected rows");
    reg.add("F", "freq-col", "frequency table for current column");

    // Sheet types
    reg.add("C", "columns-sheet", "show columns of current sheet");
    reg.add("I", "describe-sheet", "statistical summary of columns");
    reg.add("S", "sheets-sheet", "show all open sheets");
    reg.add("O", "options-sheet", "show all options");

    // Multi-sheet operations
    reg.add(
        "&",
        "join-sheets",
        "join current sheet with previous by key columns",
    );
    reg.add(
        "",
        "concat-sheets",
        "concatenate selected sheets vertically",
    );
    reg.add("W", "pivot", "pivot table by current column");
    reg.add("M", "melt", "unpivot non-key columns to rows");

    // Editing
    reg.add("e", "edit-cell", "edit current cell");
    reg.add("a", "add-row", "insert empty row above cursor");
    reg.add("d", "delete-row", "delete current row");
    reg.add("gd", "delete-selected", "delete all selected rows");

    // Undo
    reg.add("", "undo", "undo last edit (Ctrl+Z)");

    // Save
    reg.add("", "save-sheet", "save sheet to source file (Ctrl+S)");

    // Help
    reg.add("", "help-commands", "show all commands");

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_registry() -> CommandRegistry {
        let mut reg = CommandRegistry::new();
        reg.add("[", "sort-asc", "sort ascending");
        reg.add("]", "sort-desc", "sort descending");
        reg.add("g[", "sort-asc-add", "add ascending sort");
        reg.add("g]", "sort-desc-add", "add descending sort");
        reg.add("gg", "go-top", "go to top");
        reg.add("zz", "center-cursor", "center cursor");
        reg
    }

    #[test]
    fn lookup_by_keystroke() {
        let reg = test_registry();
        assert_eq!(reg.lookup_by_keystroke("["), Some("sort-asc"));
        assert_eq!(reg.lookup_by_keystroke("g["), Some("sort-asc-add"));
        assert_eq!(reg.lookup_by_keystroke("xyz"), None);
    }

    #[test]
    fn lookup_by_name() {
        let reg = test_registry();
        let info = reg.lookup_by_name("sort-asc").unwrap();
        assert_eq!(info.keystrokes, "[");
        assert_eq!(info.help, "sort ascending");
    }

    #[test]
    fn is_prefix() {
        let reg = test_registry();
        assert!(reg.is_prefix("g"));
        assert!(reg.is_prefix("z"));
        assert!(!reg.is_prefix("x"));
    }

    #[test]
    fn all_commands_sorted() {
        let reg = test_registry();
        let cmds = reg.all_commands();
        let names: Vec<&str> = cmds.iter().map(|c| c.longname.as_str()).collect();
        assert!(names.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn search_commands() {
        let reg = test_registry();
        let results = reg.search_commands("sort");
        assert_eq!(results.len(), 4);

        let results = reg.search_commands("top");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].longname, "go-top");
    }

    #[test]
    fn search_commands_case_insensitive() {
        let reg = test_registry();
        let results = reg.search_commands("SORT");
        assert_eq!(results.len(), 4);
    }

    // --- Keystroke accumulation tests (ported from Python test_keystrokes.py) ---

    #[test]
    fn accumulate_prefixed_zz() {
        let reg = test_registry();
        let (outcome, ks) = reg.accumulate("", "z");
        assert_eq!(outcome, KeystrokeOutcome::Prefix);
        assert_eq!(ks, "z");

        let (outcome, ks) = reg.accumulate("z", "z");
        assert_eq!(outcome, KeystrokeOutcome::Execute);
        assert_eq!(ks, "zz");
    }

    #[test]
    fn accumulate_prefixed_gg() {
        let reg = test_registry();
        let (outcome, _) = reg.accumulate("", "g");
        assert_eq!(outcome, KeystrokeOutcome::Prefix);

        let (outcome, ks) = reg.accumulate("g", "g");
        assert_eq!(outcome, KeystrokeOutcome::Execute);
        assert_eq!(ks, "gg");
    }

    #[test]
    fn accumulate_custom_multi_prefix() {
        let mut reg = test_registry();
        reg.add_prefix("s");
        reg.add_prefix("sb");
        reg.add("sbk", "test-sbk", "test command");
        reg.add("sbs", "test-sbs", "test command");

        let (outcome, _) = reg.accumulate("", "s");
        assert_eq!(outcome, KeystrokeOutcome::Prefix);

        let (outcome, _) = reg.accumulate("s", "b");
        assert_eq!(outcome, KeystrokeOutcome::Prefix);

        let (outcome, ks) = reg.accumulate("sb", "k");
        assert_eq!(outcome, KeystrokeOutcome::Execute);
        assert_eq!(ks, "sbk");

        let (outcome, ks) = reg.accumulate("sb", "s");
        assert_eq!(outcome, KeystrokeOutcome::Execute);
        assert_eq!(ks, "sbs");
    }

    #[test]
    fn accumulate_duplicate_prefix() {
        let reg = test_registry();
        // g, z, g → "gz" is a prefix (gz), then "g" again:
        // pending="gz", new_key="g", potential="gzg"
        // "g" is a prefix, "g" is in "gz", "gzg" is not a prefix, not bound → duplicate
        let (outcome, _) = reg.accumulate("", "g");
        assert_eq!(outcome, KeystrokeOutcome::Prefix);

        let (outcome, ks) = reg.accumulate("g", "z");
        assert_eq!(outcome, KeystrokeOutcome::Prefix);
        assert_eq!(ks, "gz");

        let (outcome, ks) = reg.accumulate("gz", "g");
        assert_eq!(outcome, KeystrokeOutcome::Duplicate);
        assert_eq!(ks, "g");
    }

    #[test]
    fn accumulate_unbound() {
        let reg = test_registry();
        let (outcome, _) = reg.accumulate("", "x");
        assert_eq!(outcome, KeystrokeOutcome::NoCommand);
    }

    #[test]
    fn accumulate_single_key_execute() {
        let reg = test_registry();
        let (outcome, ks) = reg.accumulate("", "[");
        assert_eq!(outcome, KeystrokeOutcome::Execute);
        assert_eq!(ks, "[");
    }

    #[test]
    fn builtin_registry() {
        let reg = builtin_commands();
        assert!(reg.len() > 20);
        assert!(reg.lookup_by_keystroke("[").is_some());
        assert!(reg.lookup_by_keystroke("q").is_some());
        assert!(reg.lookup_by_name("sort-asc").is_some());
    }

    #[test]
    fn add_prefix() {
        let mut reg = CommandRegistry::new();
        assert!(!reg.is_prefix("x"));
        reg.add_prefix("x");
        assert!(reg.is_prefix("x"));
        // Adding same prefix twice is idempotent
        reg.add_prefix("x");
        assert_eq!(reg.prefixes.iter().filter(|p| *p == "x").count(), 1);
    }
}
