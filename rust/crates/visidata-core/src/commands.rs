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
#[expect(clippy::too_many_lines, reason = "flat command registration table")]
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
    reg.add("<", "go-prev-value", "go to previous row with different value in current column");
    reg.add(">", "go-next-value", "go to next row with different value in current column");
    reg.add("{", "go-prev-selected", "go to previous selected row");
    reg.add("}", "go-next-selected", "go to next selected row");
    reg.add("zz", "scroll-middle", "scroll current row to middle of screen");
    reg.add("zr", "go-row-number", "go to row by number");
    reg.add("c", "go-col-regex", "go to column matching regex");
    reg.add("zc", "go-col-number", "go to column by number");
    reg.add("", "go-screen-top", "move cursor to top of visible screen");
    reg.add("", "go-screen-middle", "move cursor to middle of visible screen");
    reg.add("", "go-screen-bottom", "move cursor to bottom of visible screen");
    reg.add("", "jump-prev", "jump to previously active sheet (Ctrl+^)");

    // Sheet
    reg.add("q", "quit-sheet", "quit current sheet");
    reg.add("Enter", "open-row", "open current row as a sheet (drill into table/view)");

    // Column operations
    reg.add("_", "resize-col-max", "auto-fit column width");
    reg.add("g_", "resize-cols-max", "auto-fit all visible columns");
    reg.add("-", "hide-col", "hide current column");
    reg.add("gv", "unhide-cols", "unhide all hidden columns");
    reg.add("^", "rename-col", "rename current column");
    reg.add("!", "key-col", "toggle key column");
    reg.add("z_", "resize-col-input", "resize column to specific width");
    reg.add("T", "transpose", "open transposed sheet (rows become columns)");
    reg.add("'", "freeze-col", "freeze/materialise current column as static values");
    reg.add("i", "addcol-incr", "add incremental column (1, 2, 3…)");
    reg.add("z^", "rename-col-selected", "rename column from first selected row value");
    reg.add("g^", "rename-cols-row", "rename all columns from current row values");
    reg.add("ge", "setcol-input", "set selected rows' current column to value");
    reg.add("za", "addcol-new", "add empty editable column");
    reg.add("z~", "type-any", "set column type to anytype (no coercion)");
    reg.add("z#", "type-len", "set column type to len (display length)");
    reg.add(":", "addcol-split", "add split column (regex delimiter)");
    reg.add(";", "addcol-capture", "add capture-group columns from regex");
    reg.add("*", "addcol-subst", "add regex-substitution column");
    reg.add("(", "expand-col", "expand JSON column into per-key columns");
    reg.add(")", "contract-col", "contract expanded JSON columns");
    reg.add("H", "slide-left", "move current column one position left");
    reg.add("L", "slide-right", "move current column one position right");
    reg.add("gH", "slide-leftmost", "move current column to leftmost position");
    reg.add("gL", "slide-rightmost", "move current column to rightmost position");
    reg.add("J", "slide-row-down", "move current row one position down");
    reg.add("K", "slide-row-up", "move current row one position up");
    reg.add("gJ", "slide-row-bottom", "move current row to last position");
    reg.add("gK", "slide-row-top", "move current row to first position");

    // Type conversion
    reg.add("#", "type-int", "set column type to int");
    reg.add("%", "type-float", "set column type to float");
    reg.add("$", "type-currency", "set column type to currency");
    reg.add("~", "type-string", "set column type to string");
    reg.add("@", "type-date", "set column type to date");

    // Sorting
    reg.add("[", "sort-asc", "sort ascending by current column");
    reg.add("]", "sort-desc", "sort descending by current column");
    reg.add("g[", "sort-keys-asc", "sort ascending by all key columns");
    reg.add("g]", "sort-keys-desc", "sort descending by all key columns");
    reg.add("z[", "sort-asc-add", "add current column to ascending sort");
    reg.add("z]", "sort-desc-add", "add current column to descending sort");

    // Selection
    reg.add("s", "select-row", "select current row");
    reg.add("u", "unselect-row", "unselect current row");
    reg.add("t", "toggle-row", "toggle selection on current row");
    reg.add("gs", "select-rows", "select all rows");
    reg.add("gu", "unselect-rows", "unselect all rows");
    reg.add("gt", "stoggle-rows", "toggle selection on all rows");
    reg.add("|", "select-col-regex", "select rows matching regex in current column");
    reg.add("\\", "unselect-col-regex", "unselect rows matching regex in current column");
    reg.add("g|", "select-cols-regex", "select rows matching regex in any visible column");
    reg.add("g\\", "unselect-cols-regex", "unselect rows matching regex in any visible column");
    reg.add(",", "select-equal-cell", "select rows equal to current cell value");
    reg.add("g,", "select-equal-row", "select rows equal to entire current row");
    reg.add("zs", "select-before", "select rows before cursor");
    reg.add("zt", "stoggle-before", "toggle selection of rows before cursor");
    reg.add("zu", "unselect-before", "unselect rows before cursor");
    reg.add("gzs", "select-after", "select rows from cursor to end");
    reg.add("gzt", "stoggle-after", "toggle selection of rows from cursor to end");
    reg.add("gzu", "unselect-after", "unselect rows from cursor to end");

    // Search
    reg.add("/", "search-col", "search forward in current column");
    reg.add(
        "?",
        "search-col-backward",
        "search backward in current column",
    );
    reg.add("n", "search-next", "repeat search forward");
    reg.add("N", "search-prev", "repeat search backward");
    reg.add("r", "search-keys", "search forward in key columns");
    reg.add("g/", "search-cols", "search forward in all visible columns");
    reg.add("g?", "searchr-cols", "search backward in all visible columns");

    // Filter / Frequency
    reg.add("\"", "dup-selected", "push sheet of selected rows");
    reg.add("g\"", "dup-rows", "push sheet of all rows");
    reg.add("z\"", "dup-selected-deep", "push deep copy of selected rows");
    reg.add("gz\"", "dup-rows-deep", "push deep copy of all rows");
    reg.add("F", "freq-col", "frequency table for current column");
    reg.add("gF", "freq-keys", "frequency table for all key columns");
    reg.add("zF", "freq-summary", "one-line frequency summary in status bar");
    reg.add("gI", "describe-all", "describe all open sheets");
    reg.add("", "join-type-inner", "join sheets (inner)");
    reg.add("", "join-type-left",  "join sheets (left outer)");
    reg.add("", "join-type-right", "join sheets (right outer)");
    reg.add("", "join-type-outer", "join sheets (full outer)");

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
    reg.add("A", "open-new", "open a new empty sheet");
    reg.add("d", "delete-row", "delete current row");
    reg.add("gd", "delete-selected", "delete all selected rows");
    reg.add("zd", "delete-cell", "set current cell to null");
    reg.add("gzd", "delete-cells", "set selected rows' current column to null");
    reg.add("f", "fill-down", "fill null cells downward with last non-null value");
    reg.add("ga", "add-rows", "add N blank rows");

    // Undo / Redo
    reg.add("", "undo", "undo last edit (Ctrl+Z)");
    reg.add("R", "redo", "redo last undone edit");

    // Clipboard
    reg.add("y", "yank-cell", "yank (copy) current cell");
    reg.add("p", "paste-cell", "paste cell value");
    reg.add("gy", "yank-row", "yank (copy) current row");
    reg.add("gp", "paste-after", "paste row(s) after cursor");
    reg.add("x", "cut-row", "cut current row (yank and delete)");
    reg.add("gx", "cut-selected", "cut selected rows (yank and delete)");
    reg.add("zx", "cut-cell", "cut current cell (yank and set null)");

    // Aggregation
    reg.add("+", "aggregate-col", "show aggregation for current column");

    // Expression columns
    reg.add("=", "expr-col", "add expression column");

    // Save
    reg.add("", "save-sheet", "save sheet to source file (Ctrl+S)");
    reg.add("", "save-all", "save all sheets with source paths (gCtrl+S)");
    reg.add("", "save-cmdlog", "save command log to .vdj file (Ctrl+D)");
    reg.add("", "sysedit-cell", "edit current cell in $EDITOR (Ctrl+O)");
    reg.add("zY", "syscopy-cell", "copy cell to system clipboard");
    reg.add("Y", "syscopy-row", "copy row to system clipboard");
    reg.add("gY", "syscopy-selected", "copy selected rows to system clipboard");
    reg.add("gzP", "syspaste-cells", "paste from system clipboard");
    reg.add("", "open-config", "open config file as text sheet (gO)");
    reg.add("gS", "sheets-all", "show all sheets opened in session");
    reg.add("gm", "macro-sheet", "show all recorded macros");
    reg.add("g&", "join-sheets-all", "join all sheets in stack");
    reg.add("gA", "concat-sheets", "concatenate all sheets");
    reg.add("gM", "melt-regex", "melt sheet with column-name regex");
    reg.add("gz[", "sort-keys-asc-add", "add key columns to ascending sort");
    reg.add("gz]", "sort-keys-desc-add", "add key columns to descending sort");
    reg.add("addcol-new", "addcol-new", "add empty editable column");
    reg.add("", "expand-col", "expand JSON column");
    reg.add("", "contract-col", "contract expanded JSON columns");

    // Help
    reg.add("", "help-commands", "show all commands");

    // TUI
    reg.add("", "redraw", "force full terminal redraw (Ctrl+L)");
    reg.add("", "reload-sheet", "reload sheet from source file (Ctrl+R)");
    reg.add("", "error-recent", "show most recent error (Ctrl+E)");

    // Macros
    reg.add("Q", "macro-record-toggle", "start/stop macro recording");
    reg.add("@", "macro-replay", "replay last recorded macro");

    // Column splitting
    reg.add("", "split-col", "split column by regex pattern");

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
