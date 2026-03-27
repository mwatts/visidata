# VisiData Rust Port — Closed Gaps

Gaps that have been implemented. See gaps.md for open items.

---

## Navigation

### GAP-001 — Go to previous different value (`<`)
**Python**: `movement.py` — `go-prev-value` moves the cursor up to the nearest row where the current column's value differs from the cursor row.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: In `dispatch_command`, add `"go-prev-value"`. Starting from `cursor_row - 1`, walk rows upward comparing `col.display_value(row)` against the value at the original cursor row. Stop at first difference. Bind `<` in `handle_normal_key` and register in `builtin_commands`.

---

### GAP-002 — Go to next different value (`>`)
**Python**: `movement.py` — `go-next-value`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as GAP-001 but walk downward from `cursor_row + 1`. Bind `>`.

---

### GAP-003 — Go to previous selected row (`{`)
**Python**: `movement.py` — `go-prev-selected`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Walk rows upward from `cursor_row - 1`; stop at first row where `row.selected == true`. Bind `{` in `handle_normal_key`.

---

### GAP-004 — Go to next selected row (`}`)
**Python**: `movement.py` — `go-next-selected`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Walk downward from `cursor_row + 1`. Bind `}`.

---

### GAP-005 — Go to row number (`zr`)
**Python**: `movement.py` — `go-row-number` prompts for a 0-based row number and moves cursor there.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `"zr"` prefix handling in the `pending_prefix` block or as a direct command palette call. Open `InputMode::CommandPalette` pre-filled, or add `InputMode::GotoRow(LineEditor)`. On Accept, parse as `usize`, clamp to `0..rows.len()`, set `cursor_row`. Register in `builtin_commands` as `"go-row-number"`.

---

### GAP-006 — Scroll current row to centre (`zz`)
**Python**: `movement.py` — `scroll-middle` sets `top_row` so cursor is in the vertical middle of the visible area.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: In `dispatch_command("scroll-middle")`, compute `top_row = cursor_row.saturating_sub(height / 2)`. Bind `zz` (requires z-prefix support in `handle_normal_key`).

---

### GAP-007 — Go to column by regex (`c`)
**Python**: `features/go_col.py` — `go-col-regex` prompts for a regex and moves `cursor_col` to first matching column name.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `InputMode::GotoCol(LineEditor)`. On Accept, compile regex, iterate `sheet.visible_columns()`, find first whose `name` matches, set `cursor_col`. Bind `c`.

---

### GAP-008 — Go to column by number (`zc`)
**Python**: `features/go_col.py` — `go-col-number` prompts for an integer column index.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Same as GAP-007 but parse as usize and index directly into `visible_columns()`. Bind `zc`.

---

### GAP-009 — Go to screen top / middle / bottom (unbound)
**Python**: `movement.py` — `go-screen-top`, `go-screen-middle`, `go-screen-bottom` move cursor to first/middle/last visible row without scrolling.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Compute `top_row`, `top_row + height/2`, `top_row + height - 1` and clamp. These can be palette-only commands initially.

---

### GAP-011 — Jump to previous sheet (`Ctrl+^`)
**Python**: `vd.sheets[-2]` — `jump-prev` swaps to the previously active sheet.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `prev_sheet_idx: Option<usize>` to `App` or track a "previous sheet" pointer in `SheetStack`. Bind `Ctrl+^`.

---

---

## Selection

### GAP-013 — Select all rows (`gs`)
**Python**: `selection.py` — `select-rows` marks every row selected.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Add `sheet.select_all()` method to `Sheet`. In `dispatch_command("select-rows")`, call it. In `handle_normal_key` under g-prefix handler, add `Char('s') => self.dispatch_command("select-rows")`. Register `"gs"` in `builtin_commands`.

---

### GAP-014 — Unselect all rows (`gu`)
**Python**: `selection.py` — `unselect-rows`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Add `sheet.unselect_all()`. Bind `gu`.

---

### GAP-015 — Toggle selection of all rows (`gt`)
**Python**: `selection.py` — `stoggle-rows`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Add `sheet.toggle_select_all()` — flip `.selected` on every row. Bind `gt`.

---

### GAP-016 — Select rows matching regex in current column (`|`)
**Python**: `selection.py` — `select-col-regex` prompts for a regex, selects all rows where the current column matches.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Add `InputMode::SelectRegex(LineEditor)`. On Accept, compile regex (use `regex` crate already in workspace), iterate rows, test `col.display_value(row)` against regex, set `row.selected = true`. Bind `|`. Register `"select-col-regex"`.

---

### GAP-017 — Unselect rows matching regex (`\`)
**Python**: `selection.py` — `unselect-col-regex`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as GAP-016 but set `row.selected = false`. Bind `\`.

---

### GAP-018 — Select rows matching regex in any visible column (`g|`)
**Python**: `selection.py` — `select-cols-regex`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as GAP-016 but test all visible columns. Bind `g|` (g-prefix + `|`).

---

### GAP-019 — Unselect rows matching regex in any visible column (`g\`)
**Python**: `selection.py` — `unselect-cols-regex`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as GAP-018 but deselect. Bind `g\`.

---

### GAP-020 — Select rows equal to current cell (`,`)
**Python**: `selection.py` — `select-equal-cell` selects all rows where the current column equals the cursor cell's display value.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Read `col.display_value(&rows[cursor_row])` as the target. Iterate rows, select where `col.display_value(row) == target`. Bind `,`. Register `"select-equal-cell"`.

---

### GAP-021 — Select rows equal to entire current row (`g,`)
**Python**: `selection.py` — `select-equal-row` — matches all visible columns.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Extend GAP-020: collect all visible column display values for cursor row; select any row that matches all of them. Bind `g,`.

---

### GAP-024 — Select rows before cursor (`zs`, `zt`, `zu`)
**Python**: `selection.py` — `select-before`, `stoggle-before`, `unselect-before`.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Operate on `rows[0..cursor_row]`. Bind `zs`/`zt`/`zu` via z-prefix handler.

---

### GAP-025 — Select rows after cursor (`gzs`, `gzt`, `gzu`)
**Python**: `selection.py` — `select-after`, `stoggle-after`, `unselect-after`.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Operate on `rows[cursor_row..]`. Bind `gzs`/`gzt`/`gzu`.

---

---

## Column Operations

### GAP-026 — Set selected rows' column to same value (`ge`)
**Python**: `sheets.py` — `setcol-input` prompts for a value, sets the current column for all selected rows to that value.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `InputMode::SetColInput(LineEditor)`. On Accept, coerce input string through `col.col_type`, apply to all selected rows via `sheet.set_cell(row_idx, col_source_idx, val)` for each. Push a bulk `UndoAction::DeleteRows`-style variant or record individual `SetCell` actions. Bind `ge`.

---

### GAP-027 — Slide column left (`H`)
**Python**: `features/slide.py` — `slide-left` swaps the current column with the one to its left in `sheet.columns`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: In `dispatch_command("slide-left")`, find current column's index in `sheet.columns`, swap with index-1 if > 0. Also update `cursor_col`. Bind `H`.

---

### GAP-028 — Slide column right (`L`)
**Python**: `features/slide.py` — `slide-right`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Swap with index+1 if < last. Bind `L`.

---

### GAP-029 — Slide column to leftmost (`gH`)
**Python**: `features/slide.py` — `slide-leftmost`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Remove column from its position, insert at 0. Bind `gH`.

---

### GAP-030 — Slide column to rightmost (`gL`)
**Python**: `features/slide.py` — `slide-rightmost`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Move to last position. Bind `gL`.

---

### GAP-031 — Slide row up/down (`J`, `K`, `gJ`, `gK`)
**Python**: `features/slide.py` — `slide-down`, `slide-up`, `slide-bottom`, `slide-top`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Swap `rows[cursor_row]` with `rows[cursor_row ± 1]` (or move to end/start). Adjust `cursor_row`. Bind `J`/`K`/`gJ`/`gK`.

---

### GAP-032 — Unhide all columns (`gv`)
**Python**: `features/layout.py` — `unhide-cols` sets width of all hidden columns back to `None` (auto).
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Iterate `sheet.columns`, find any with `width == Some(0)`, reset to `None`. Bind `gv`. Register `"unhide-cols"`.

---

### GAP-033 — Resize all columns to max width (`g_`)
**Python**: `features/layout.py` — `resize-cols-max`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Apply the auto-width computation to all visible columns (same logic as current per-column `_` in dispatch). Bind `g_`.

---

### GAP-034 — Resize column to specific width (`z_`)
**Python**: `features/layout.py` — `resize-col-input` prompts for a width integer.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Prompt via command palette or new input mode, parse as `u16`, set `col.width = Some(n)`. Bind `z_`.

---

### GAP-036 — Rename column from selected rows content (`z^`)
**Python**: `rename_col.py` — `rename-col-selected` uses the current column values of selected rows to rename.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Collect `col.display_value` of first selected row in current column, use as new name. Bind `z^`.

---

### GAP-037 — Rename all columns from current row (`g^`)
**Python**: `rename_col.py` — `rename-cols-row` renames all visible columns using the cursor row's values as new names.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Iterate visible columns; rename each to `col.display_value(&rows[cursor_row])`. Bind `g^`.

---

### GAP-043 — Freeze/cache column (`'`)
**Python**: `features/freeze.py` — `freeze-col` materialises an expression column's current values into a static column.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Duplicate the column's current value vector into a new plain `Column`. Bind `'`.

---

### GAP-045 — Add incremental column (`i`)
**Python**: `features/incr.py` — `addcol-incr` adds a column with 1, 2, 3… (or user-specified start/step).
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Prompt for start and step. Add column whose value is `start + row_idx * step`. Bind `i`.

---

### GAP-046 — Transpose sheet (`T`)
**Python**: `features/transpose.py` — `transpose` opens a new sheet where rows become columns and vice versa.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Create `transpose_sheet(source: &Sheet) -> Sheet` in `sheets.rs`. Row 0 of transposed sheet has column names; subsequent rows are the transposed data. Bind `T`.

---

---

## Editing

### GAP-048 — Rename column not undoable
**Python**: Rename is tracked in the undo log.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Before applying rename, push `UndoAction::RenameColumn { col_id, old_name }`. Add the variant to `UndoAction` enum in `undo.rs` and handle it in `sheet.undo()`.

---

### GAP-049 — Column type change not undoable
**Python**: Type changes are undoable.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Add `UndoAction::SetColType { col_id, old_type }`. Push before changing `col.col_type`. Handle in `sheet.undo()`.

---

### GAP-050 — Fill nulls downward (`f`)
**Python**: `features/fill.py` — `setcol-fill` propagates the last non-null value down through null cells in the current column.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Walk rows top-to-bottom on current column. Track last non-null `Value`. When a null cell is found, set it to last non-null. Push a bulk undo action. Bind `f`.

---

### GAP-051 — Delete cell (set to null) (`zd`)
**Python**: `clipboard.py` — `delete-cell` sets the current cell to null without deleting the row.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: In dispatch, `"delete-cell"` calls `sheet.set_cell(cursor_row, col_source_idx, Value::Null)`. Bind `zd`.

---

### GAP-052 — Delete selected cells in column (`gzd`)
**Python**: `clipboard.py` — `delete-cells` nulls selected rows' current column.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Iterate selected rows, call `sheet.set_cell(row_idx, col_source_idx, Value::Null)` for each. Bind `gzd`.

---

### GAP-054 — Add multiple blank rows (`ga`)
**Python**: `modify.py` — `add-rows` prompts for a count N and appends N blank rows.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Prompt for N (palette or input mode). Call `sheet.insert_row_at` N times, recording each as an undo action (or a single bulk action). Bind `ga`.

---

---

## Clipboard

### GAP-056 — Cut row (`x`)
**Python**: `clipboard.py` — `cut-row` copies then deletes the current row.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: In dispatch `"cut-row"`, call `clipboard.yank_cell` (or copy full row), then `sheet.delete_row_at(cursor_row)`. Bind `x`.

---

### GAP-057 — Cut selected rows (`gx`)
**Python**: `clipboard.py` — `cut-selected`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Copy selected rows to clipboard, then `sheet.delete_selected_rows()`. Bind `gx`.

---

### GAP-058 — Cut cell (`zx`)
**Python**: `clipboard.py` — `cut-cell` copies cell value then sets it to null.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Yank cell value to clipboard, then `sheet.set_cell(... Value::Null)`. Bind `zx`.

---

### GAP-059 — Yank row (`gy`) — registered but not dispatched
**Python**: `clipboard.py` — copies the whole current row's values to clipboard as a `Row`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: In the g-prefix match block in `handle_normal_key`, add `Char('y') => self.dispatch_command("yank-row")`. In `dispatch_command("yank-row")`, clone `rows[cursor_row]`, store in `self.clipboard`. Confirm `Clipboard` struct supports full-row storage (currently stores one `Value`); extend if needed.

---

### GAP-060 — Paste row after current (`gp`) — registered but not dispatched
**Python**: `clipboard.py` — `paste-after` inserts yanked row after cursor.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as GAP-059 for paste: handle in g-prefix block. Insert clipboard row at `cursor_row + 1`.

---

---

## Search

### GAP-064 — Search key column (`r`)
**Python**: `search.py` — `search-keys` searches the key column(s) only.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as existing `search-col` but targets the first key column (`sheet.columns.iter().find(|c| c.is_key)`). Bind `r`.

---

### GAP-065 — Search all visible columns (`g/`)
**Python**: `search.py` — `search-cols` forward-searches across all visible columns.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: In `repeat_search` / `search_forward`, try each visible column in order; move cursor when any matches. Add `SearchScope::AllCols` variant to search state. Bind `g/`.

---

### GAP-066 — Search all visible columns backward (`g?`)
**Python**: `search.py` — `searchr-cols`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as GAP-065 but backward. Bind `g?`.

---

---

## Sorting

### GAP-068 — Sort ascending by all key columns (`g[`)
**Python**: `sort.py` — `sort-keys-asc` sorts by all `is_key` columns ascending.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Collect all column indices where `col.is_key == true`. Call `sheet.sort_by_multi(key_indices, SortDirection::Ascending)` — add this multi-key sort method to `Sheet`. Bind `g[`.

---

### GAP-069 — Sort descending by all key columns (`g]`)
**Python**: `sort.py` — `sort-keys-desc`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as GAP-068 but descending. Bind `g]`.

---

### GAP-070 — Additive sort ascending (`z[`)
**Python**: `sort.py` — `sort-asc-change` — appends current column to the sort criteria rather than replacing it.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Add `"sort-asc-add"` command that pushes a new `SortKey` onto `sheet.sort_keys` without clearing existing keys, then re-sorts. Bind `z[`.

---

### GAP-071 — Additive sort descending (`z]`)
**Python**: `sort.py` — `sort-desc-change`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as GAP-070 but descending. Bind `z]`.

---

---

## Filtering / Derived Sheets

### GAP-073 — Duplicate sheet with all rows (`g"`)
**Python**: `sheets.py` — `dup-rows` opens a full copy of the sheet with all rows.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Add `sheet.all_rows_sheet()` — clone all rows into a new Sheet with same columns. In dispatch `"dup-rows"`, push it. Bind `g"` (g-prefix + `"`).

---

### GAP-074 — Deep copy of selected rows (`z"`)
**Python**: `sheets.py` — `dup-selected-deep` — deep copy (no shared references).
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as `"` (dup-selected) in Rust since `Row` and `Value` are all owned. Register `"dup-selected-deep"` as an alias. Bind `z"`.

---

### GAP-075 — Deep copy of all rows (`gz"`)
**Python**: `sheets.py` — `dup-rows-deep`.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Same as GAP-073. Bind `gz"`.

---

### GAP-076 — Frequency table for all key columns (`gF`)
**Python**: `freqtbl.py` — `freq-keys` opens a pivot-like frequency table on all key columns.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `sheet.frequency_sheet_multi(key_col_indices)` or reuse `pivot_sheet` with key columns. Bind `gF`.

---

### GAP-077 — One-line frequency summary (`zF`)
**Python**: `freqtbl.py` — `freq-summary` shows a single-line summary of the frequency distribution in the status bar.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Compute distinct value counts, format as `"5 distinct / 100 total / top: Alice(20)"`, write to `self.status`. Bind `zF`.

---

---

## Meta Sheets

### GAP-085 — Describe all sheets (`gI`)
**Python**: `features/describe.py` — `gI` opens a describe sheet combining statistics from all sheets in the stack.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Iterate `self.stack`, call `describe_sheet` on each, concatenate into one sheet with an extra `sheet_name` column. Bind `gI`.

---

### GAP-087 — Open new empty sheet (`A`)
**Python**: `sheets.py` — `open-new` creates a blank sheet with no columns.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Push `Sheet::new("unnamed")` onto the stack. Bind `A`. Register `"open-new"`.

---

### GAP-088 — Error sheet (`Ctrl+E`)
**Python**: `textsheet.py` — `error-recent` opens a TextSheet with the most recent exception traceback.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Keep `self.last_errors: VecDeque<String>` in `App`. Append to it whenever an error occurs. `Ctrl+E` pushes `text_sheet("error", last_errors.last())`. Bind `Ctrl+E`.

---

---

## Expressions

### GAP-104 — Expression columns are named by user
**Python**: `=name=expr` syntax sets the column name.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Parse `=name=expr` in the palette handler: if `=` input contains a second `=` after the name, split on first `=` to get name. Otherwise use `expr{N}`.

---

---

## Options System

### GAP-108 — `quitguard` option not enforced
**Python**: When `quitguard = True`, quitting a modified sheet prompts for confirmation.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: In `dispatch_command("quit-sheet")`, check `options.get_bool("quitguard")` and `sheet.modified`. If both true, set status to `"sheet modified — press q again to quit"` and set a pending-quit flag instead of popping immediately.

---

---

## File I/O & Loaders

### GAP-112 — Reload sheet from source (`Ctrl+R`)
**Python**: `sheets.py` — `reload-sheet` re-runs the loader for the current sheet's source path, replacing rows/columns.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: In `dispatch_command("reload-sheet")`, if `sheet.source` is set, call `registry.load_file(&source_path)`, then replace `sheet.rows` and `sheet.columns` with the new sheet's data (keep `sheet.name`, `sheet.source`, cursor position). Bind `Ctrl+R`.

---

### GAP-114 — Save Parquet
**Python**: `loaders/parquet.py` — `save_parquet` writes Arrow record batches via the Parquet writer.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `save_parquet(sheet: &Sheet, path: &Path) -> Result<()>` using the `parquet` crate already in workspace. Map `Value` to Arrow arrays (reuse the type-mapping from `vd_turso`). Write via `ArrowWriter`. Register in `saver.rs`.

---

### GAP-115 — Save YAML
**Python**: `loaders/yts.py` — `save_yaml`.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `save_yaml(sheet: &Sheet, path: &Path) -> Result<()>` using `serde_yaml`. Serialize rows as a sequence of maps `{col_name: value}`. Register in `saver.rs`.

---

### GAP-116 — Save HTML
**Python**: `loaders/html.py` — `save_html` writes a `<table>`.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Write `<table><thead><tr><th>…</th></tr></thead><tbody>…</tbody></table>`. Use `std::fmt::Write` to build the string. Register in `saver.rs`.

---

### GAP-118 — TOML loader
**Python**: `loaders/toml.py` — `open_toml` loads a TOML file as a key-value sheet.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `TomlLoader` in `visidata-loaders`. Use `toml::from_str::<toml::Value>`. If top-level is a table of tables → each sub-table is a row. If top-level is an array of tables → each element is a row. Flat tables → single row. Register for `.toml`.

---

### GAP-119 — Arrow/Feather loader
**Python**: `loaders/arrow.py` — `open_arrow` / `open_feather` / `save_arrow`.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `ArrowLoader` using `arrow::ipc::reader::FileReader`. Register for `.arrow`, `.feather`. For save, use `arrow::ipc::writer::FileWriter`.

---

---

## TUI / Rendering

### GAP-125 — Redraw / refresh screen (`Ctrl+L`)
**Python**: `Ctrl+L` forces a full terminal redraw.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Add `Ctrl+L` handling in `handle_normal_key` that calls `terminal.clear()` before the next render. Bind `Ctrl+L`.

---

---

## Undo / Redo

### GAP-129 — Redo (`R`)
**Python**: `undo.py` — `redo-last` replays the most recently undone action.
**Rust now**: ✅ Implemented (commit: feat/rust session). Partial: works for InsertRow/DeleteRow/DeleteRows; cell/rename/type-change redo deferred.
**Instructions**: Add `redo_stack: Vec<UndoAction>` to `UndoStack`. When `sheet.undo()` pops an action and reverses it, push the action to `redo_stack`. Add `sheet.redo()` that pops from `redo_stack`, re-applies the action, and pushes to `undo_stack`. Any new edit clears `redo_stack`. Bind `R`.

---

### GAP-132 — Sort is not undoable
**Python**: Sort is tracked in the undo log.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Before sorting, save the current row order as `UndoAction::ReorderRows { order: Vec<RowId> }`. Add this variant to `UndoAction`. In `undo()`, restore row order by re-sorting `rows` to match the saved `RowId` sequence.

---
