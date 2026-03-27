# VisiData Rust Port — Gap Register

Every numbered gap is a feature present in the Python VisiData (`develop` baseline `43e129e5`) that is
absent or incomplete in the Rust port. Gaps are ordered by category then priority.

Each entry has:
- **Python**: what the Python version does and where it lives
- **Rust now**: current state
- **Instructions**: concrete implementation steps

---

## Navigation

### GAP-010 — Horizontal scrolling (`left_col`)
**Python**: `movement.py` — left/right column navigation. Columns that scroll off the left edge are hidden.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: In `renderer.rs::draw_table`, iterate visible columns starting from `sheet.left_col` instead of 0. In `handle_normal_key` for `cursor_right`/`cursor_left`, when `cursor_col` would move off screen, increment/decrement `sheet.left_col`. Add `Ctrl+Right` / `Ctrl+Left` to scroll a page.

---

### GAP-012 — Scroll cells within a wide cell (`zl`, `zh`, `gzl`, `gzh`)
**Python**: `movement.py` — when a cell's content is wider than its column, these commands scroll the display within the cell.
**Rust now**: Missing. Cells are simply truncated.
**Instructions**: Add a `cell_offset: usize` per column (or global). Renderer respects this offset when clipping. This is low-priority; truncation is acceptable in v1.

---

---

## Selection

### GAP-022 — Select rows by expression (`z|`)
**Python**: `selection.py` — `select-expr` prompts for a Rhai expression; selects rows where it evaluates truthy.
**Rust now**: ✅ Implemented (commit: feat/rust blocker-2)
**Instructions**: Reuse expression evaluation from `expr_column.rs`. Prompt for expression (command palette or new input mode). For each row, inject column values into Rhai scope, evaluate, select if result is truthy. Bind `z|`.

---

### GAP-023 — Unselect rows by expression (`z\`)
**Python**: `selection.py` — `unselect-expr`.
**Rust now**: ✅ Implemented (commit: feat/rust blocker-2)
**Instructions**: Same as GAP-022 but deselect. Bind `z\`.

---

---

## Column Operations

### GAP-035 — Toggle multiline display (`v`)
**Python**: `features/layout.py` — `toggle-multiline` toggles whether long cells wrap to multiple rows.
**Rust now**: ⚠️ Partial — field declared but renderer still single-line
**Instructions**: Add `multiline: bool` to `Sheet`. When true, renderer wraps cell content to `col.width` characters. This requires multi-row rendering logic in `draw_table`. Bind `v`. Low-priority for initial implementation; declare the field and option now.

---

### GAP-038 — Add regex-split column (`:`)
**Python**: `features/regex.py` — `addcol-split` prompts for a delimiter regex, adds a new column whose value is the Nth split segment.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Add new `InputMode` or palette prefix `split:`. Create a `SplitColumn` (stores source col index, regex, segment index). For initial port, adding the first segment as a new column is sufficient. Bind `:`.

---

### GAP-039 — Add regex-capture column (`;`)
**Python**: `features/regex.py` — `addcol-capture` adds one column per capture group in the regex.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Prompt for regex with capture groups. For each capture group, add a new column that evaluates the Nth capture of the source column's value. Bind `;`.

---

### GAP-040 — Add regex-substitution column (`*`)
**Python**: `features/regex.py` — `addcol-regex-subst` adds a column with a regex substitution applied.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Prompt for `pattern/replacement`. Add column that applies `Regex::replace` to source column value. Bind `*`.

---

### GAP-041 — Expand JSON column (`(`)
**Python**: `features/expand_cols.py` — `expand-col` adds new columns for each key in a JSON-valued column.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: If cursor column contains JSON strings (`Value::Text` parseable as object), add a new column per discovered key. Bind `(`.

---

### GAP-042 — Contract expanded column (`)`)
**Python**: `features/expand_cols.py` — `contract-col` removes the expanded columns.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Track expanded-from parent column ID; remove child columns by that tag. Bind `)`.

---

### GAP-044 — Add empty editable column (`za`)
**Python**: `modify.py` — `addcol-new` adds an empty column the user can populate via `e`/`ge`.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Append `Column::new(ColumnId(next_id), "new_col", new_source_idx)` to `sheet.columns`. Extend each row's `values` with `Value::Null`. Bind `za`.

---

### GAP-047 — Column type: `anytype` / `vlen`
**Python**: Supports `anytype` (passthrough), `vlen` (length of container).
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Add `ColumnType::Any` (no coercion, display raw) and `ColumnType::Len` (display `Value::len()` for text/bytes). Add `z~` for Any and `z#` for Len.

---

---

## Editing

### GAP-053 — Edit cell for all selected rows (`ge`)
**Python**: `sheets.py` — `setcol-input`.
**Rust now**: ✅ See GAP-026 (already closed).
**Instructions**: See GAP-026.

---

### GAP-055 — Open cell in external editor (`Ctrl+O`)
**Python**: `features/sysedit.py` — `sysedit-cell` writes cell to a temp file, opens `$EDITOR`, reads back.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Write `cell_value_string` to `NamedTempFile`, spawn `$EDITOR` (or `$VISUAL`) via `std::process::Command`, wait, read file back, parse through column type, set cell. Bind `Ctrl+O`. Mark as platform-dependent.

---

---

## Clipboard

### GAP-061 — Copy cell to system clipboard (`zY`)
**Python**: `clipboard.py` — `syscopy-cell` uses `xclip`/`pbcopy`/`clip.exe`.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Detect platform; pipe cell display string to `pbcopy` (macOS) / `xclip -selection clipboard` (Linux) / `clip` (Windows) via `Command`. Bind `zY`. Fail gracefully if tool not found.

---

### GAP-062 — Copy row / selected rows to system clipboard (`Y`, `gY`)
**Python**: `clipboard.py` — `syscopy-row`, `syscopy-selected` write TSV to system clipboard.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Format row(s) as TSV string (reuse CSV writer logic with `\t` delimiter), pipe to system clipboard tool. Bind `Y`/`gY`.

---

### GAP-063 — Paste from system clipboard (`gzP`)
**Python**: `clipboard.py` — `syspaste-cells` reads TSV from system clipboard and pastes at cursor.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Read from `pbpaste` / `xclip -o` / `Get-Clipboard`. Parse as TSV. Apply values cell-by-cell from `cursor_row`, `cursor_col`. Bind `gzP`.

---

---

## Search

### GAP-067 — Search by expression (`z/`, `z?`)
**Python**: `search.py` — `search-expr`, `searchr-expr` — evaluates a Rhai expression per row, advances to first truthy.
**Rust now**: ✅ Implemented (commit: feat/rust blocker-2)
**Instructions**: Add `InputMode::SearchExpr(LineEditor, Direction)`. On Accept, compile expression. Walk rows from cursor±1 evaluating expression via `rhai::Engine`; stop at first truthy result. Bind `z/`/`z?`.

---

---

## Sorting

### GAP-072 — Sort by key columns additive (`gz[`, `gz]`)
**Python**: `sort.py` — `sort-keys-asc-add`, `sort-keys-desc-add`.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Combine GAP-068 logic with additive push. Bind `gz[`/`gz]`.

---

---

## Filtering / Derived Sheets

### GAP-078 — FrequencySheet drill-down into source rows
**Python**: `freqtbl.py` — pressing Enter on a frequency row opens a filtered sheet of the source rows matching that value.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: When building the frequency sheet in `Sheet::frequency_sheet`, set `sheet.drill = Some(Arc::new(FreqDrill { source_sheet_idx, col_idx }))`. Add `FreqDrill` struct in `visidata-loaders` or a new module. `open_row` filters source sheet rows matching the frequency group value and returns a new sheet. Requires storing a reference/snapshot of the source sheet data in `FreqDrill`.

---

---

## Meta Sheets

### GAP-079 — ColumnsSheet drill-down and editing
**Python**: `metasheets.py` — pressing Enter on a ColumnsSheet row opens the frequency table for that column. Column metadata (name, width, type, key) is editable in-place.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**:
  1. **Drill**: Set `drill` on the columns sheet to a `ColsDrill` that opens `sheet.frequency_sheet(row_col_idx)` when Enter is pressed.
  2. **Editing**: Wire `e` on the columns sheet to edit the `name`/`width`/`type`/`key` fields and write back to the source sheet's column definition. Requires the columns sheet to hold a reference to its source sheet index.

---

### GAP-080 — SheetsSheet drill-down (Enter navigates to that sheet)
**Python**: `indexsheet.py` — pressing Enter on the SheetsSheet navigates to (pushes) the selected sheet.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: The sheets sheet rows contain `Value::Text(sheet.name)` in column 0. Add a `SheetsDrill { }` struct implementing `DrillAction` that reads the sheet name from col 0, finds it in `self.stack` by name, and swaps to it. Since `DrillAction::open_row` returns a `Sheet`, the implementation needs to either return a clone of the target sheet or use a different mechanism (e.g., a special `Value::SheetRef(SheetId)` that the TUI handles separately). Alternative simpler approach: add a special `SpecialAction::GotoSheet` handled in `dispatch_command("open-row")` before calling `drill.open_row`.

---

### GAP-081 — DescribeSheet drill-down (select source rows)
**Python**: `features/describe.py` — `zs`/`zu` on a describe sheet selects/unselects source rows falling in that column's range.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Set `drill` on describe sheet. `open_row` on a describe row returns a filtered sheet of source rows for that column's non-null values. This is lower priority.

---

### GAP-082 — Open global options sheet (`O`)
**Python**: `optionssheet.py` — `O` opens the global options sheet.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Wire `e` on the options sheet to open `InputMode::EditCell` for the value column. On Accept, call `options_manager.set(option_name, new_value, "global")`. Push `UndoAction` for the option change. The options manager already supports `set()`.

---

### GAP-083 — Open sheet-local options (`zO`)
**Python**: `optionssheet.py` — `zO` opens options scoped to the current sheet.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Same as GAP-082 but pass current sheet's name/type as context to `options_manager.set(name, value, sheet_name)`. Bind `zO`.

---

### GAP-084 — Open ~/.visidatarc as text sheet (`gO`)
**Python**: `optionssheet.py` — `gO` opens the user config file.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Define a config file path (e.g., `~/.vdrc` or `~/.config/vd/config.toml`). Push a `text_sheet_from_file` for that path. Allow editing and saving. Bind `gO`.

---

### GAP-086 — All-sessions sheets sheet (`gS`)
**Python**: `indexsheet.py` — `gS` shows every sheet ever opened in the session, not just the stack.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Add `all_sheets: Vec<SheetId>` to `App` (or a vec of sheet names) tracking every sheet ever pushed. `gS` builds a `sheets_sheet` from that history. Bind `gS`.

---

### GAP-089 — Threads sheet (`Ctrl+T`)
**Python**: `threads.py` — `threads-all` shows running/finished background threads.
**Rust now**: Missing. Loading state is tracked in `Sheet.loading_state` but there's no browsable thread list.
**Instructions**: Add a `threads_sheet()` function showing background load handles with name, status, rows loaded. Bind `Ctrl+T`. Low priority.

---

### GAP-090 — Macro sheet (`gm`)
**Python**: `macros.py` — `macro-sheet` opens a list of all saved macros.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Add `macros_sheet(macro_store: &MacroStore) -> Sheet` showing macro name, keystroke count, preview. Bind `gm`.

---

### GAP-091 — Command log save / replay (`Ctrl+D`)
**Python**: `cmdlog.py` — `save-cmdlog` saves the session's command history as a `.vdj` file for later replay.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Maintain `self.command_log: Vec<(String, String)>` (longname, key) in `App`. `Ctrl+D` saves as JSON array to a `.vdj` file. Replay would read the file and replay each command. Initial implementation: just save, no replay.

---

---

## Multi-sheet Operations

### GAP-092 — Left / Right / Outer joins not bound
**Python**: `features/join.py` — join type is chosen interactively from a palette showing all join types.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: When `&` is pressed, open `InputMode::CommandPalette` pre-filled with join type options, or a dedicated `InputMode::JoinType`. On selection, call `join_sheets(left, right, selected_type)` and push result.

---

### GAP-093 — Join more than two sheets
**Python**: `features/join.py` — can join all selected sheets in one operation.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Add `join_selected_sheets()` that collects sheets where `sheet.rows` are marked selected (or from the sheets-sheet). Cascade-join them. Bind `g&`.

---

### GAP-094 — Concat sheets key binding
**Python**: Concat / append join type.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Bind `g&` (after GAP-093) or a dedicated key such as `gA` for concatenate.

---

### GAP-095 — Pivot aggregation (not count-only)
**Python**: `pivot.py` — pivot cells show the result of the column's assigned aggregators (sum, avg, etc.), not just count.
**Rust now**: `pivot_sheet` hardcodes count per group.
**Instructions**: After implementing GAP-105 (persistent aggregators on columns), update `pivot_sheet` to call `col.aggregator` (if set) instead of incrementing a counter. Default to count when no aggregator is set.

---

### GAP-096 — Melt with regex column name parsing (`gM`)
**Python**: `features/melt.py` — `melt-regex` allows a capture regex to split column names into multiple variable columns (e.g., `sales_2023` → variable `sales`, period `2023`).
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Extend `melt_sheet` to accept an optional `col_name_regex`. When provided, each variable row gets additional columns from the capture groups. Bind `gM`.

---

### GAP-097 — Save all sheets (`gCtrl+S`)
**Python**: `save.py` — `save-all` saves every sheet in the stack to a zip or a directory.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: In `dispatch_command("save-all")`, iterate `self.stack`, call `save_sheet` for each that has a `source` path. Bind `gCtrl+S`.

---

---

## Aggregation

### GAP-098 — Persistent aggregators on columns (`+`)
**Python**: `aggregators.py` — `aggregate-col` marks a column with one or more aggregators; their results appear in frequency/pivot tables and in a dedicated summary row.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Add `aggregators: Vec<AggFunc>` to `Column`. `+` prompts to choose from available functions and appends to the list. Display aggregate totals in the status bar footer row and in FrequencySheet/PivotSheet cells.

---

### GAP-099 — Memo aggregate to status (`z+`)
**Python**: `aggregators.py` — `memo-aggregate` shows result in status bar and stores in memory.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Prompt for aggregator name. Compute and display chosen aggregator's result. Bind `z+`.

---

### GAP-100 — Additional aggregators: median, mode, stdev, distinct, count, percentiles
**Python**: `aggregators.py` — full set: min, max, avg/mean, median, mode, sum, distinct, count, list, stdev, q3/q4/q5/q10, p10–p99.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Add to `AggFunc` enum and `AggFunc::compute()`:
  - `Median` — sort values, take middle (or average of two middles)
  - `Mode` — most-frequent value
  - `Stdev` — population standard deviation: `sqrt(sum((x-mean)^2) / n)`
  - `Distinct` — count of unique values
  - `CountNonNull` — alias of existing `Count`
  - Percentiles can be added as `Percentile(u8)` variant

---

---

## Expressions

### GAP-101 — Set column values from expression (`g=`)
**Python**: `expr.py` — `setcol-expr` prompts for expression, applies to selected rows' current column.
**Rust now**: ✅ Implemented (commit: feat/rust blocker-2)
**Instructions**: Detect if palette input starts with `g=`. Evaluate expression per selected row, call `sheet.set_cell(row_idx, col_source_idx, result)` for each. Bind `g=` in g-prefix block.

---

### GAP-102 — Set current cell from expression (`z=`)
**Python**: `expr.py` — `setcell-expr` applies expression to just the cursor cell.
**Rust now**: ✅ Implemented (commit: feat/rust blocker-2)
**Instructions**: Same as GAP-101 but applied to `cursor_row` only. Bind `z=`.

---

### GAP-103 — Expression columns are lazily re-evaluated per render
**Python**: `expr.py` — `ExprColumn.calcValue` evaluates the expression each time the cell is rendered.
**Rust now**: ✅ Implemented (commit: feat/rust blocker-2)
**Instructions**: Add a `Column::expr: Option<String>` field. When set, `Column::display_value(row)` evaluates the Rhai expression instead of reading from `row.values[source_idx]`. Remove the materialisation loop from `expr_column.rs`. This is a breaking change to how expression columns work; existing tests will need updating.

---

---

## Options System

### GAP-105 — Options consumed by CSV loader (`csv_delimiter`)
**Python**: CSV delimiter is read from `options.csv_delimiter` at load time.
**Rust now**: ✅ Implemented (commit: feat/rust blocker-1)
**Instructions**: In `CsvLoader::load`, read `options_manager.get("csv_delimiter", ...)`. Use as the delimiter byte. Requires threading the `OptionsManager` into loaders; either pass it through `LoaderRegistry::load_file` or make it a global (currently `App` owns it).

---

### GAP-106 — Options consumed by display formatting
**Python**: `disp_float_fmt`, `disp_int_fmt`, `disp_date_fmt` are read when formatting cell values.
**Rust now**: ✅ Implemented (commit: feat/rust blocker-1)
**Instructions**: Pass `OptionsManager` reference to `Column::display_value` (or store relevant format strings on `Column`). Read format strings from options instead of hardcoded values.

---

### GAP-107 — Color options consumed by renderer
**Python**: `color_selected_row`, `color_cursor_row`, `color_key_col`, etc. are read from options and can be changed at runtime.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: In `app.rs`, after loading options, construct a `Theme` from option values. `color_selected_row` → `theme.row_selected`, etc. Re-derive theme when options change.

---

---

## Async Loading

### GAP-109 — Built-in loaders use synchronous loading
**Python**: All loaders use `@asyncthread` — they load in a background thread and stream rows to the sheet as they arrive.
**Rust now**: `async_loader.rs` infrastructure exists and is tested but none of the built-in loaders (`CsvLoader`, `JsonLoader`, etc.) use it. They call `loader.load(path)` synchronously, blocking the UI for large files.
**Instructions**: Refactor `LoaderRegistry::load_file` to return a `LoadHandle` instead of a `Sheet`. Move each loader's row generation into a closure passed to `spawn_loader`. The `App` already has `poll_loader` machinery to drain incoming rows into the active sheet. Thread the `LoadHandle` back to `App::open_file`.

---

### GAP-110 — Progress percentage vs row count
**Python**: Shows percentage like `"42%"` based on total file size or row count estimate.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Add `rows_total: Option<usize>` to `LoadingState::Loading`. For CSV/fixed-width, estimate from file size. Update status bar to show `"42% (1234 rows)"` when total is known.

---

### GAP-111 — Cancellation of async loads
**Python**: `Ctrl+C` cancels the current sheet's background threads.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Change `Ctrl+C` to first check if a load is in progress (`self.load_handle.is_some()`). If so, call `handle.cancel()` and set status `"load cancelled"`. Only quit the app if no load is active (or on second `Ctrl+C`).

---

---

## File I/O & Loaders

### GAP-113 — Save SQLite back to source
**Python**: `loaders/sqlite.py` — `SqliteSheet` has `putChanges()` executing `INSERT`/`UPDATE`/`DELETE` SQL within a transaction.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Add `save_sqlite(sheet: &Sheet, path: &Path) -> Result<()>` to `sqlite_loader.rs`. Strategy: `DROP TABLE IF EXISTS "{name}"`, `CREATE TABLE`, `INSERT` all rows. More advanced: use the deferred-edit pattern (track adds/mods/dels). For initial implementation, a full table replacement is acceptable. Register as a saver in `saver.rs`.

---

### GAP-117 — Excel loader: only reads first sheet
**Python**: `loaders/xlsx.py` — `XlsxIndexSheet` lists all worksheets; Enter opens any sheet.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Use `calamine::open_workbook_auto` and iterate `workbook.sheet_names()`. Build an index sheet (name, row_count). Set `drill` to an `ExcelDrill { path, sheet_name }` that loads the named worksheet. The `calamine` crate supports named sheet access via `worksheet_range(name)`.

---

### GAP-120 — JSON streaming / JSONL improvements
**Python**: Handles deeply nested JSON, arrays at root level, single objects, auto-detects format.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Improve `JsonLoader` to detect root-level arrays of arrays (SequenceSheet), objects within arrays whose values are themselves arrays (list columns), and provide better nested key expansion (related to GAP-041).

---

---

## TUI / Rendering

### GAP-121 — Split-window pane (`Z`, `gZ`, `Tab`)
**Python**: `Z` splits the terminal vertically; each pane can show a different sheet.
**Rust now**: Single-pane only.
**Instructions**: Add `pane: u8` to `Sheet` and a split-mode to `App`. The renderer splits the terminal area and renders two `SheetStack` views. `Tab` switches active pane. This is a significant rendering refactor.

---

### GAP-122 — Runtime theme switching
**Python**: Options `color_*` can be changed at runtime and take effect immediately.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: Read a `theme` option (`"default"`, `"dark"`, `"light"`) in `App`. On each render, call `Theme::by_name(options.get("theme"))`. When the user edits the theme option, the next render picks it up.

---

### GAP-123 — Status bar: show current aggregator / selection info
**Python**: Status bar shows the aggregator total for the cursor column when one is assigned.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: After GAP-098 (persistent aggregators), read `col.aggregators.first()`, compute result, append to status bar string.

---

### GAP-124 — Column header shows hidden-column indicators
**Python**: When columns are hidden between visible ones, a narrow indicator column shows `…` or the count of hidden columns.
**Rust now**: ✅ Implemented (commit: feat/rust session 3)
**Instructions**: In `draw_table`, when iterating visible columns, detect gaps between column indices (hidden columns in between) and render a narrow `…` divider cell.

---

### GAP-126 — Mouse: column resize by dragging header
**Python**: Column width can be adjusted by dragging the separator in the header.
**Rust now**: Mouse handling only scrolls and moves cursor row. No column resize.
**Instructions**: Track `MouseEventKind::Down` in the header row. On `Drag`, compute delta and adjust `col.width`. This requires tracking which column was clicked.

---

---

## Macros

### GAP-127 — Named macros
**Python**: `macros.py` — macros can be assigned a keystroke trigger (e.g., `@1`–`@9`) and saved persistently.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: Add `HashMap<String, Macro>` to `MacroStore`. Allow naming a macro when stopping recording. Save/load from `~/.config/vd/macros.json` on startup/shutdown. Bind named macros to their assigned keystrokes.

---

### GAP-128 — Macro replay from file / command log
**Python**: `cmdlog.py` — a `.vdj` file can be replayed to reproduce a session.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: After GAP-091 (command log), add `replay_command_log(path)` that reads the `.vdj` file and dispatches each command via `dispatch_command`. Bind `gCtrl+P` or palette `replay:`.

---

---

## Undo / Redo

### GAP-130 — Column rename undoable (see GAP-048)
Already listed as GAP-048.

---

### GAP-131 — Column type change undoable (see GAP-049)
Already listed as GAP-049.

---

---

## External Loaders

### GAP-133 — External loader drill confirmed working (no change needed)
**Note**: Both `vd_duckdb` (line 83) and `vd_turso` (line 104) use `req.query.as_deref().unwrap_or(TABLE_INDEX_SQL)`. When `ExtDrill::open_row` sends `query: Some("SELECT * FROM \"table\"")`, the binary executes it and returns Arrow IPC. **No changes to either external loader binary are needed** for drill-down to work.

---

### GAP-134 — External loader: custom query via command palette
**Python**: `SqliteSheet.addCommand('', 'exec-sql', ...)` — user can type arbitrary SQL.
**Rust now**: ✅ Implemented (commit: feat/rust final)
**Instructions**: When the active sheet has a `drill` of type `ExtDrill`, add a `exec-sql` command (palette prefix `sql:`) that takes arbitrary SQL, calls `ext_drill.loader.run_query(&ext_drill.db_path, Some(&sql), HashMap::new())`, and pushes the result.

---

### GAP-135 — External loader: pass options to subprocess
**Python**: Loader options (e.g., batch size) can be set via the options sheet and are passed to the subprocess.
**Rust now**: ✅ Implemented (commit: feat/rust blocker-1)
**Instructions**: In `ExtDrill::open_row` and `ExtLoader::load`, populate the options map from the `OptionsManager` using keys prefixed with the loader name (e.g., `vd_duckdb_batch_size`). This requires threading `OptionsManager` into the drill call.

---

---

## Summary Table

| ID | Category | Open Gaps |
|----|----------|-----------|
| GAP-010, GAP-012 | Navigation | 2 |
| GAP-022, GAP-023 | Selection | 2 |
| GAP-035, GAP-038 to GAP-042, GAP-044, GAP-047 | Column Ops | 9 |
| GAP-053, GAP-055 | Editing | 2 |
| GAP-061 to GAP-063 | Clipboard | 3 |
| GAP-067 | Search | 1 |
| GAP-072 | Sorting | 1 |
| GAP-078 | Filtering | 1 |
| GAP-079 to GAP-084, GAP-086, GAP-089 to GAP-091 | Meta Sheets | 10 |
| GAP-092 to GAP-097 | Multi-sheet | 6 |
| GAP-098 to GAP-100 | Aggregation | 3 |
| GAP-101 to GAP-103 | Expressions | 3 |
| GAP-105 to GAP-107 | Options | 3 |
| GAP-109 to GAP-111 | Async Loading | 3 |
| GAP-113, GAP-117, GAP-120 | Loaders/I/O | 3 |
| GAP-121 to GAP-124, GAP-126 | TUI | 5 |
| GAP-127 to GAP-128 | Macros | 2 |
| GAP-130 to GAP-131 | Undo/Redo (cross-refs) | 2 |
| GAP-133 to GAP-135 | Ext Loaders | 3 |

**Open gaps: 63** (GAP-053, GAP-130, and GAP-131 are cross-references to closed gaps; effective distinct open gaps: 60)
