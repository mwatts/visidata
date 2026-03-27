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

### Ported Python Tests
- `test_date.py` — `customdate()` parser → port date parsing logic into `Value::Date` construction
- `test_path.py` — `Path.with_name()`, `base_stem`, `ext`, `iterdir()` → port as `visidata-core::path` module tests

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

### Ported Python Tests
- `test_cliptext.py` — `dispwidth()`, `clipstr()`, `clipstr_start()`, `clipstr_middle()`, `wraptext()` → port as `visidata-tui::cliptext` module tests (display width, Unicode wide chars, text truncation)
- `test_parsepos.py` — `parsePos()` for `+col:row` CLI position args → port as `bin/vd` CLI parsing tests
- `test-smoke.sh` — `vd --version`, `vd -f dir . --batch` → port as binary integration tests
- `test-startpos.sh` — `+N`, `+col:row` positioning → port as CLI integration tests

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

### Ported Python Tests
- `test-delimiter.sh` — TSV/CSV/PSV/USV delimiter handling, `-d` flag, `--csv-delimiter` → port as loader integration tests
- `test-roundtrip.sh` — load→save→reload→save idempotence for CSV/TSV/JSON → port as round-trip integration tests
- `test-stdin.sh` — `seq 10000 | vd -f txt` piped input → port as stdin integration test
- `test-stdin-replay.sh` — pipe JSON to stdin with commands → port as stdin+command test
- `test_fixed_width.py` — `columnize()` column boundary detection → port when fixed-width loader added (Phase 10)
- Batch replay tests: `load-*.vd*` files from `tests/` → port relevant load tests as golden-file comparisons
- Test data: copy `sample_data/sample.tsv`, `sample_data/benchmark.csv` to `tests/fixtures/`

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

### Ported Python Tests
- Batch replay tests: `addcol-*.vd*` (column addition), column rename/hide/resize tests from `tests/`
- `test_edittext.py` — `InputWidget.editline()` keystroke handling (Home, End, Delete, Backspace, Ctrl+keys, undo, transpose) → port as `visidata-tui::input` module tests

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

### Ported Python Tests
- Batch replay tests: `sort*.vd*`, `freq*.vdx`, `select-*.vd*` from `tests/`
- `test_commands.py` — subset of ~100 global commands tested on `sample.tsv`: sort, select, filter, frequency commands

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

### Ported Python Tests
- `test_keystrokes.py` — multi-key sequences (`zz`, `gg`, custom prefixes), duplicate prefix detection, unbound sequences → port as `visidata-core::keybinding` tests
- `test_commands.py` — full command execution suite: replay ~100 commands on `sample.tsv`, verify no errors → port as command registry integration tests
- `test_completer.py` — `CompleteExpr` autocomplete for column names and globals → port as command palette completion tests

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

### Ported Python Tests
- `test_fixed_width.py` — `columnize()` column boundary detection with internal spaces (issue #2265) → port as fixed-width loader tests
- `test-roundtrip.sh` — extend to cover all new formats (SQLite, Excel, Parquet, YAML)
- Batch replay tests: format-specific `load-*.vd*` tests from `tests/`
- Test data: copy relevant fixtures from `sample_data/` (`.xlsx`, `.sqlite`, `.parquet`, `.yml`, `.html`, `.fixed`)

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

### Ported Python Tests
- Batch replay tests: `join*.vd*`, `append.vd`, `unfurl*.vd*`, `melt*.vd*`, `pivot*.vd*` from `tests/`
- Test data: `tests/data1.tsv`, `tests/data2.tsv`, `tests/data3.tsv` (small 3-row tables for join tests)

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

### Ported Python Tests
- `test_menu.py` — `addMenuItems()` registration and validation, menu path traversal → port as `visidata-tui::menu` tests

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

### Ported Python Tests
- `test-macros.sh` — macro recording and replay with `tests/macros/test_macro.vd` → port as macro integration tests
- Batch replay tests: `macro*.vdx`, `aggregators-*.vd*` from `tests/`
- `test-perf.sh` — performance benchmark tests (wall clock timing) → port as `#[bench]` or criterion benchmarks
- `test_features.py` — dynamic feature test discovery → adapt as plugin test framework
- Remaining `test-vdx.sh` batch replay tests (~184 total) — port as golden-file integration test harness

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

### Python Test Porting Approach

VisiData's Python test suite has 3 layers. Each maps to a Rust equivalent:

| Python Layer | Files | Rust Equivalent |
|---|---|---|
| **Pytest unit tests** (12 files) | `visidata/tests/test_*.py` | `#[cfg(test)] mod tests` in each crate |
| **Batch replay tests** (184 files) | `tests/*.vd*` + `tests/golden/` | Integration test binary with golden-file comparison |
| **Shell integration tests** (12 scripts) | `tests/test-*.sh` | `cargo test` integration tests in `bin/vd` |

### Python Test → Phase Mapping

| Python Test | Phase | Rust Location |
|---|---|---|
| `test_date.py` | 1 | `visidata-core::value` |
| `test_path.py` | 1 | `visidata-core::path` |
| `test_cliptext.py` | 2 | `visidata-tui::cliptext` |
| `test_parsepos.py` | 2 | `bin/vd` CLI tests |
| `test-smoke.sh` | 2 | `bin/vd` integration tests |
| `test-startpos.sh` | 2 | `bin/vd` integration tests |
| `test-delimiter.sh` | 3 | `visidata-loaders` integration tests |
| `test-roundtrip.sh` | 3, 10 | `visidata-loaders` round-trip tests |
| `test-stdin.sh` | 3 | `bin/vd` integration tests |
| `test-stdin-replay.sh` | 3 | `bin/vd` integration tests |
| `test_edittext.py` | 4 | `visidata-tui::input` |
| `test_fixed_width.py` | 10 | `visidata-loaders::fixed_width` |
| `test_commands.py` | 5, 6 | `visidata-core::commands` integration |
| `test_keystrokes.py` | 6 | `visidata-core::keybinding` |
| `test_completer.py` | 6 | `visidata-tui::completer` |
| `test_menu.py` | 14 | `visidata-tui::menu` |
| `test_features.py` | 15 | Plugin test framework |
| `test-macros.sh` | 15 | `bin/vd` macro tests |
| `test-perf.sh` | 15 | Criterion benchmarks |
| Batch replay (184 `.vd*`) | 5–15 | Golden-file integration harness |

### Golden-File Test Harness

Port VisiData's `test-vdx.sh` pattern to Rust:
1. Define test as a sequence of commands (equivalent to `.vd`/`.vdx` replay files)
2. Run commands on input data in headless/batch mode
3. Compare output against golden files in `tests/golden/`
4. Update golden files with `UPDATE_GOLDEN=1 cargo test`

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
