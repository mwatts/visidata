# VisiData Rust Port — Master Plan

## Overview

Port VisiData (a Python terminal-based data exploration tool) to Rust using **ratatui** for the TUI layer. VisiData lets users open tabular data files (CSV, JSON, TSV, SQLite, etc.), explore them interactively with a spreadsheet-like interface, sort/filter/aggregate, join sheets, and more.

This plan is phased: each phase produces a working, testable binary. Early phases focus on the core data model and rendering loop; later phases add features, loaders, and extensibility.

---

## Architecture Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| TUI framework | ratatui + crossterm | De facto Rust TUI standard; crossterm is cross-platform |
| Async runtime | tokio | Needed for async file loading, background tasks |
| Error handling | anyhow (app crate), thiserror (lib crates) | M-APP-ERROR / M-ERRORS-CANONICAL-STRUCTS |
| Allocator | mimalloc | M-MIMALLOC-APPS — free perf for apps |
| Serialization | serde | Industry standard for data format support |
| CSV/TSV | csv crate | Fast, well-maintained |
| JSON | serde_json | Standard |
| SQLite | rusqlite | Mature bindings |
| Scripting | rhai | Pure Rust, no unsafe, expression eval, preserves VisiData's dynamic command model |
| CLI args | clap (derive) | Ergonomic, well-maintained |
| Edition | 2024 | Current stable (Rust 1.94) |

### Workspace Layout

```
rust/
├── Cargo.toml              # workspace root
├── Cargo.lock
├── .plans/                 # this plan
├── crates/
│   ├── visidata-core/      # data model, column, sheet, options, commands
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── visidata-scripting/  # rhai engine, command expressions, user scripts
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── visidata-tui/       # ratatui rendering, input handling, color
│   │   ├── Cargo.toml
│   │   └── src/
│   └── visidata-loaders/   # file format loaders (csv, json, tsv, sqlite, etc.)
│       ├── Cargo.toml
│       └── src/
└── bin/
    └── vd/                 # CLI binary entry point
        ├── Cargo.toml
        └── src/
            └── main.rs
```

---

## Phase 1 — Scaffold & Core Data Model

**Goal**: Cargo workspace compiles. Core types exist with unit tests. No TUI yet.

### Tasks

1. **Workspace setup** — root `Cargo.toml`, member crates, shared deps, lints
2. **`visidata-core` — Value type**
   - `Value` enum: `Null`, `Int(i64)`, `Float(f64)`, `Text(String)`, `Bool(bool)`, `Date(NaiveDateTime)`, `Bytes(Vec<u8>)`, `Error(String)`
   - `Display`, `PartialOrd`, `From` impls
   - Formatting helpers (`format_value`)
3. **`visidata-core` — Column**
   - `Column` struct: `name`, `width`, `col_type`, `getter` (fn pointer or closure), `hidden`
   - `ColumnId` newtype (M-NEWTYPE)
   - `Column::get_value(row: &Row) -> Value`
   - `Column::get_display(row: &Row) -> String`
4. **`visidata-core` — Row**
   - `Row` — wrapper around `Vec<Value>` (indexed columns) or `HashMap<String, Value>` (named)
   - `RowId` newtype
5. **`visidata-core` — Sheet**
   - `Sheet` struct: `name`, `columns: Vec<Column>`, `rows: Vec<Row>`, `cursor_row`, `cursor_col`, `source`
   - `SheetId` newtype
   - Methods: `num_rows()`, `num_cols()`, `visible_columns()`, `get_cell(row, col) -> Value`
6. **`visidata-core` — SheetStack**
   - `SheetStack`: push/pop/active sheet, navigation
7. **`visidata-scripting` — Rhai engine scaffold**
   - `ScriptEngine` struct wrapping `rhai::Engine`
   - Register `Value` as custom Rhai type with conversions
   - Register basic functions: `len()`, `str()`, `int()`, `float()`
   - Expose `Row` and `Column` access to Rhai (read-only for now)
   - `eval_expr(expr: &str, scope: &Scope) -> Result<Value>` — evaluate a Rhai expression
   - `exec_script(script: &str, scope: &mut Scope) -> Result<()>` — execute a Rhai script block
   - Unit tests for expression evaluation
8. **Unit tests** for all core types

### Deliverable
`cargo test` passes with core data model tests.

---

## Phase 2 — Basic TUI Rendering

**Goal**: Open the terminal, render a single sheet with columns/rows, navigate with arrow keys.

### Tasks

1. **`visidata-tui` — App struct**
   - `App` holding `SheetStack`, terminal state, running flag
   - Main loop: `draw()` → `handle_event()` → repeat
2. **`visidata-tui` — Sheet rendering**
   - Header row with column names (styled)
   - Data rows with cell values (truncated to column width)
   - Cursor highlight (current row + current cell)
   - Row numbers in left gutter
   - Status bar (bottom): sheet name, row count, cursor position
3. **`visidata-tui` — Basic navigation**
   - Arrow keys: move cursor
   - `h/j/k/l`: vim-style movement
   - `q`: quit / pop sheet
   - `Home/End`: jump to first/last row
   - `PgUp/PgDn`: page scroll
4. **`bin/vd` — CLI entry point**
   - Parse args with clap (file path, optional flags)
   - Load hardcoded test data if no file given
   - Initialize terminal, run app loop, restore terminal on exit

### Deliverable
`cargo run -- test.csv` opens TUI showing data, arrow keys navigate, `q` quits.

---

## Phase 3 — File Loaders (CSV, TSV, JSON)

**Goal**: Open real data files from the command line.

### Tasks

1. **Loader trait**
   ```rust
   pub trait Loader: Send + Sync {
       fn can_load(&self, path: &Path) -> bool;
       fn load(&self, path: &Path) -> Result<Sheet>;
   }
   ```
2. **Loader registry** — `LoaderRegistry` with `register()` and `load_file(path)` dispatch
3. **CSV loader** — using `csv` crate, auto-detect delimiter
4. **TSV loader** — tab-delimited variant
5. **JSON loader** — array of objects → rows, keys → columns; nested values as formatted strings
6. **Auto-detect** — by file extension, fallback to content sniffing
7. **Integration tests** with sample files

### Deliverable
`cargo run -- data.csv`, `data.tsv`, `data.json` all work.

---

## Phase 4 — Column Operations & Type System

**Goal**: Users can resize, hide, rename, and type columns.

### Tasks

1. **Column resize** — `_` to auto-fit, `-` to hide, `^` to rename (input prompt)
2. **Type conversion** — `#` (int), `%` (float), `$` (currency), `@` (date), `~` (text)
3. **Column type inference** — auto-detect types on load
4. **Null handling** — display nulls distinctly, sort nulls last
5. **Key columns** — `!` to toggle key column, visual indicator
6. **Input line** — bottom-of-screen text input for rename, search, etc.

### Deliverable
Column operations work interactively.

---

## Phase 5 — Sorting, Filtering & Selection

**Goal**: Sort by column, search/filter rows, select/unselect rows.

### Tasks

1. **Sort** — `[` / `]` to sort ascending/descending by current column
2. **Multi-sort** — `g[` / `g]` for stable secondary sort
3. **Search** — `/` forward regex search, `?` backward, `n/N` next/prev
4. **Row selection** — `s` select, `u` unselect, `gs` select all, `gu` unselect all
5. **Filter** — `"` to create filtered sheet from selected rows
6. **Frequency table** — `F` to create frequency sheet for current column

### Deliverable
Full sort/filter/select workflow.

---

## Phase 6 — Command System & Status Bar

**Goal**: Formalized command registry with dual dispatch (native Rust handlers + Rhai expressions), keybinding system, and enhanced status display.

### Tasks

1. **Command enum** — dual handler model:
   ```rust
   enum CommandHandler {
       Native(Box<dyn Fn(&mut App) -> Result<()>>),  // built-in Rust commands
       Script(String),                                 // Rhai expression string
   }
   ```
2. **Command struct** — `longname`, `keystrokes`, `help`, `handler: CommandHandler`
3. **Command registry** — register commands by longname, look up by keystroke
4. **Rhai command scope** — expose `sheet`, `row`, `col`, `vd` objects to Rhai scripts during command execution
5. **User-defined commands** — users can bind keystrokes to Rhai expressions via config or at runtime
6. **`exec-rhai` command** — interactive Rhai expression evaluation (equivalent to Python VisiData's `exec-python`)
7. **Command palette** — `:` or Space to open command search (fuzzy filter)
8. **Status bar** — left: mode + sheet info; right: row count, selection count
9. **Help sheet** — `?` or `Ctrl-H` to show all commands
10. **Error display** — show errors in status bar with color

### Deliverable
Commands discoverable via palette and help sheet. Users can define and execute Rhai commands.

---

## Phase 7 — Options System

**Goal**: Hierarchical options (default → global → sheet-type → instance).

### Tasks

1. **Option struct** — `name`, `value: Value`, `help`, `module`
2. **OptionsManager** — resolution chain, get/set at each level
3. **Options sheet** — `O` to view/edit options
4. **Config file** — load `~/.visidatarc.toml` on startup (options + keybindings)
5. **Rhai init script** — load `~/.visidatarc.rhai` on startup (custom commands, computed columns, hooks)
6. **CLI option override** — `--option=value` from command line

### Deliverable
Options configurable at all levels.

---

## Phase 8 — Color & Theming

**Goal**: Configurable color scheme matching VisiData's colorizer system.

### Tasks

1. **ColorAttr** — fg, bg, modifiers (bold, underline, reverse)
2. **Theme struct** — named color slots (header, cursor_row, cursor_cell, key_col, selected, etc.)
3. **Colorizer trait** — cell/row/column colorizers with precedence
4. **Built-in themes** — default (matching Python VisiData), dark, light
5. **256-color / truecolor support** via crossterm

### Deliverable
Themed, colorful display matching VisiData aesthetics.

---

## Phase 9 — Sheet Types

**Goal**: Additional sheet types beyond basic table.

### Tasks

1. **MetaSheet** — show columns of current sheet (allows reorder, rename, type)
2. **FrequencySheet** — already started in Phase 5, enhance with histograms
3. **DescribeSheet** — statistical summary (min, max, mean, median, stdev, nulls)
4. **DirSheet** — file browser, open files from directory listing
5. **TextSheet** — view raw text (log files, etc.)

### Deliverable
Multiple sheet types navigable via sheet stack.

---

## Phase 10 — Additional Loaders

**Goal**: Expand file format support.

### Tasks

1. **SQLite loader** — `rusqlite`, show tables as index sheet, open table as data sheet
2. **Excel loader** — `calamine` crate for .xlsx/.xls
3. **Parquet loader** — `parquet` crate
4. **YAML loader** — `serde_yaml`
5. **HTML table loader** — `scraper` crate
6. **Fixed-width loader** — column-position based parsing

### Deliverable
Broad file format support.

---

## Phase 11 — Editing & Undo

**Goal**: Edit cell values, add/delete rows, undo/redo.

### Tasks

1. **Cell editing** — `e` to edit current cell (inline editor)
2. **Row operations** — `a` add row, `d` delete row, `gd` delete selected
3. **Undo stack** — record mutations, `z Ctrl-Z` to undo
4. **Modified indicator** — mark sheet as modified, warn on quit
5. **Save** — `Ctrl-S` to save back to source format

### Deliverable
Full CRUD on sheet data with undo.

---

## Phase 12 — Multi-Sheet Operations

**Goal**: Join, concatenate, and pivot sheets.

### Tasks

1. **Join** — `&` to join sheets by key columns (inner, outer, left, right)
2. **Concatenate** — stack selected sheets vertically
3. **Pivot** — pivot table from key + value columns
4. **Melt/Unpivot** — wide → long transformation
5. **Sheet index** — `S` to show all sheets, navigate between them

### Deliverable
Multi-sheet data transformation workflow.

---

## Phase 13 — Async Loading & Progress

**Goal**: Load large files without blocking the UI.

### Tasks

1. **Async loader trait** — background loading with progress reporting
2. **Progress bar** — show loading progress in status bar
3. **Cancelable loads** — `Ctrl-C` to cancel in-progress load
4. **Incremental display** — show rows as they load
5. **Thread-safe sheet updates** — safe mutation from loader thread

### Deliverable
Large files load smoothly with progress indication.

---

## Phase 14 — Menu System

**Goal**: Top menu bar matching VisiData's menu structure.

### Tasks

1. **Menu bar** — File, Edit, View, Column, Row, Sheet, Tools, Help
2. **Dropdown rendering** — overlay menu items on sheet
3. **Keyboard navigation** — Alt+letter to open menu, arrows to navigate
4. **Mouse support** — click to open/select menu items
5. **Context-sensitive menus** — show relevant commands per sheet type

### Deliverable
Full menu system.

---

## Phase 15 — Polish & Parity

**Goal**: Feature parity refinements and quality-of-life.

### Tasks

1. **Clipboard** — yank/paste cells and rows
2. **Macro recording** — record and replay keystroke sequences
3. **Expression columns** — computed columns from Rhai expressions (e.g., `col1 + col2`)
4. **Aggregation** — sum, avg, count on selected rows
5. **Regex column splitting** — split column by pattern
6. **Mouse support** — click to position cursor, scroll wheel
7. **Resize handling** — respond to terminal resize events
8. **Man page / `--help`** — comprehensive CLI documentation

### Deliverable
Production-quality TUI data explorer.

---

## Workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = [
    "crates/*",
    "bin/*",
]

[workspace.package]
edition = "2024"
version = "0.1.0"
license = "GPL-3.0"

[workspace.lints.rust]
unsafe_op_in_unsafe_fn = "warn"
missing_debug_implementations = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }

[workspace.dependencies]
# Internal crates
visidata-core = { path = "crates/visidata-core" }
visidata-scripting = { path = "crates/visidata-scripting" }
visidata-tui = { path = "crates/visidata-tui" }
visidata-loaders = { path = "crates/visidata-loaders" }

# TUI
ratatui = "0.29"
crossterm = "0.28"

# Async
tokio = { version = "1", features = ["full"] }

# Error handling
anyhow = "1"
thiserror = "2"

# Scripting
rhai = "1"

# CLI
clap = { version = "4", features = ["derive"] }

# Data formats
csv = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.32", features = ["bundled"] }
calamine = "0.26"
serde_yaml = "0.9"

# Utilities
chrono = { version = "0.4", features = ["serde"] }
regex = "1"
unicode-width = "0.2"
mimalloc = { version = "0.1", default-features = false }
```

---

## Build & Test Strategy

- `cargo clippy --workspace` — zero warnings
- `cargo test --workspace` — all phases have tests
- `cargo fmt --check` — enforced formatting
- Integration tests with sample data files in `tests/fixtures/`
- Each phase merges to feature branch only when tests pass

---

## Execution Order

Phases are sequential — each builds on the previous. Estimated scope:

| Phase | Focus | Complexity |
|-------|-------|------------|
| 1 | Data model | Low |
| 2 | Basic TUI | Medium |
| 3 | File loaders | Low |
| 4 | Column ops | Medium |
| 5 | Sort/filter/select | Medium |
| 6 | Command system | Medium |
| 7 | Options system | Medium |
| 8 | Colors/themes | Low |
| 9 | Sheet types | Medium |
| 10 | More loaders | Low |
| 11 | Editing/undo | High |
| 12 | Multi-sheet ops | High |
| 13 | Async loading | High |
| 14 | Menu system | Medium |
| 15 | Polish & parity | Medium |
