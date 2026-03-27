# VisiData Rust Port — Gap Register

Every numbered gap is a feature present in the Python VisiData (`develop` baseline `43e129e5`) that is
absent or incomplete in the Rust port. Gaps are ordered by category then priority.

Each entry has:
- **Python**: what the Python version does and where it lives
- **Rust now**: current state
- **Instructions**: concrete implementation steps

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

### GAP-010 — Horizontal scrolling (`left_col`)
**Python**: `movement.py` — left/right column navigation. Columns that scroll off the left edge are hidden.
**Rust now**: `Sheet.left_col` field exists but the renderer always starts at column index 0, ignoring `left_col`.
**Instructions**: In `renderer.rs::draw_table`, iterate visible columns starting from `sheet.left_col` instead of 0. In `handle_normal_key` for `cursor_right`/`cursor_left`, when `cursor_col` would move off screen, increment/decrement `sheet.left_col`. Add `Ctrl+Right` / `Ctrl+Left` to scroll a page.

---

### GAP-011 — Jump to previous sheet (`Ctrl+^`)
**Python**: `vd.sheets[-2]` — `jump-prev` swaps to the previously active sheet.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Add `prev_sheet_idx: Option<usize>` to `App` or track a "previous sheet" pointer in `SheetStack`. Bind `Ctrl+^`.

---

### GAP-012 — Scroll cells within a wide cell (`zl`, `zh`, `gzl`, `gzh`)
**Python**: `movement.py` — when a cell's content is wider than its column, these commands scroll the display within the cell.
**Rust now**: Missing. Cells are simply truncated.
**Instructions**: Add a `cell_offset: usize` per column (or global). Renderer respects this offset when clipping. This is low-priority; truncation is acceptable in v1.

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

### GAP-022 — Select rows by expression (`z|`)
**Python**: `selection.py` — `select-expr` prompts for a Rhai expression; selects rows where it evaluates truthy.
**Rust now**: Missing.
**Instructions**: Reuse expression evaluation from `expr_column.rs`. Prompt for expression (command palette or new input mode). For each row, inject column values into Rhai scope, evaluate, select if result is truthy. Bind `z|`.

---

### GAP-023 — Unselect rows by expression (`z\`)
**Python**: `selection.py` — `unselect-expr`.
**Rust now**: Missing.
**Instructions**: Same as GAP-022 but deselect. Bind `z\`.

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

### GAP-035 — Toggle multiline display (`v`)
**Python**: `features/layout.py` — `toggle-multiline` toggles whether long cells wrap to multiple rows.
**Rust now**: Missing. All cells are single-line.
**Instructions**: Add `multiline: bool` to `Sheet`. When true, renderer wraps cell content to `col.width` characters. This requires multi-row rendering logic in `draw_table`. Bind `v`. Low-priority for initial implementation; declare the field and option now.

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

### GAP-038 — Add regex-split column (`:`)
**Python**: `features/regex.py` — `addcol-split` prompts for a delimiter regex, adds a new column whose value is the Nth split segment.
**Rust now**: `split:<pattern>` in command palette exists but mutates in place (splits into multiple columns). Python's `:` adds a single new derived column.
**Instructions**: Add new `InputMode` or palette prefix `split:`. Create a `SplitColumn` (stores source col index, regex, segment index). For initial port, adding the first segment as a new column is sufficient. Bind `:`.

---

### GAP-039 — Add regex-capture column (`;`)
**Python**: `features/regex.py` — `addcol-capture` adds one column per capture group in the regex.
**Rust now**: Missing.
**Instructions**: Prompt for regex with capture groups. For each capture group, add a new column that evaluates the Nth capture of the source column's value. Bind `;`.

---

### GAP-040 — Add regex-substitution column (`*`)
**Python**: `features/regex.py` — `addcol-regex-subst` adds a column with a regex substitution applied.
**Rust now**: Missing.
**Instructions**: Prompt for `pattern/replacement`. Add column that applies `Regex::replace` to source column value. Bind `*`.

---

### GAP-041 — Expand JSON column (`(`)
**Python**: `features/expand_cols.py` — `expand-col` adds new columns for each key in a JSON-valued column.
**Rust now**: Missing.
**Instructions**: If cursor column contains JSON strings (`Value::Text` parseable as object), add a new column per discovered key. Bind `(`.

---

### GAP-042 — Contract expanded column (`)`)
**Python**: `features/expand_cols.py` — `contract-col` removes the expanded columns.
**Rust now**: Missing.
**Instructions**: Track expanded-from parent column ID; remove child columns by that tag. Bind `)`.

---

### GAP-043 — Freeze/cache column (`'`)
**Python**: `features/freeze.py` — `freeze-col` materialises an expression column's current values into a static column.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Duplicate the column's current value vector into a new plain `Column`. Bind `'`.

---

### GAP-044 — Add empty editable column (`za`)
**Python**: `modify.py` — `addcol-new` adds an empty column the user can populate via `e`/`ge`.
**Rust now**: Missing.
**Instructions**: Append `Column::new(ColumnId(next_id), "new_col", new_source_idx)` to `sheet.columns`. Extend each row's `values` with `Value::Null`. Bind `za`.

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

### GAP-047 — Column type: `anytype` / `vlen`
**Python**: Supports `anytype` (passthrough), `vlen` (length of container).
**Rust now**: No `anytype` or `vlen` variants in `ColumnType`.
**Instructions**: Add `ColumnType::Any` (no coercion, display raw) and `ColumnType::Len` (display `Value::len()` for text/bytes). Add `z~` for Any and `z#` for Len.

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

### GAP-053 — Edit cell for all selected rows (`ge`)
**Python**: `sheets.py` — `setcol-input`.
**Rust now**: See GAP-026. Listed here as editing gap for completeness.
**Instructions**: See GAP-026.

---

### GAP-054 — Add multiple blank rows (`ga`)
**Python**: `modify.py` — `add-rows` prompts for a count N and appends N blank rows.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Prompt for N (palette or input mode). Call `sheet.insert_row_at` N times, recording each as an undo action (or a single bulk action). Bind `ga`.

---

### GAP-055 — Open cell in external editor (`Ctrl+O`)
**Python**: `features/sysedit.py` — `sysedit-cell` writes cell to a temp file, opens `$EDITOR`, reads back.
**Rust now**: Missing.
**Instructions**: Write `cell_value_string` to `NamedTempFile`, spawn `$EDITOR` (or `$VISUAL`) via `std::process::Command`, wait, read file back, parse through column type, set cell. Bind `Ctrl+O`. Mark as platform-dependent.

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

### GAP-061 — Copy cell to system clipboard (`zY`)
**Python**: `clipboard.py` — `syscopy-cell` uses `xclip`/`pbcopy`/`clip.exe`.
**Rust now**: Missing.
**Instructions**: Detect platform; pipe cell display string to `pbcopy` (macOS) / `xclip -selection clipboard` (Linux) / `clip` (Windows) via `Command`. Bind `zY`. Fail gracefully if tool not found.

---

### GAP-062 — Copy row / selected rows to system clipboard (`Y`, `gY`)
**Python**: `clipboard.py` — `syscopy-row`, `syscopy-selected` write TSV to system clipboard.
**Rust now**: Missing.
**Instructions**: Format row(s) as TSV string (reuse CSV writer logic with `\t` delimiter), pipe to system clipboard tool. Bind `Y`/`gY`.

---

### GAP-063 — Paste from system clipboard (`gzP`)
**Python**: `clipboard.py` — `syspaste-cells` reads TSV from system clipboard and pastes at cursor.
**Rust now**: Missing.
**Instructions**: Read from `pbpaste` / `xclip -o` / `Get-Clipboard`. Parse as TSV. Apply values cell-by-cell from `cursor_row`, `cursor_col`. Bind `gzP`.

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

### GAP-067 — Search by expression (`z/`, `z?`)
**Python**: `search.py` — `search-expr`, `searchr-expr` — evaluates a Rhai expression per row, advances to first truthy.
**Rust now**: Missing.
**Instructions**: Add `InputMode::SearchExpr(LineEditor, Direction)`. On Accept, compile expression. Walk rows from cursor±1 evaluating expression via `rhai::Engine`; stop at first truthy result. Bind `z/`/`z?`.

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

### GAP-072 — Sort by key columns additive (`gz[`, `gz]`)
**Python**: `sort.py` — `sort-keys-asc-add`, `sort-keys-desc-add`.
**Rust now**: ⚠️ Partial (commit: feat/rust session 2) — gz[ and gz] not explicitly wired but framework supports it.
**Instructions**: Combine GAP-068 logic with additive push. Bind `gz[`/`gz]`.

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

### GAP-078 — FrequencySheet drill-down into source rows
**Python**: `freqtbl.py` — pressing Enter on a frequency row opens a filtered sheet of the source rows matching that value.
**Rust now**: `F` works but the resulting frequency sheet has no `drill` set.
**Instructions**: When building the frequency sheet in `Sheet::frequency_sheet`, set `sheet.drill = Some(Arc::new(FreqDrill { source_sheet_idx, col_idx }))`. Add `FreqDrill` struct in `visidata-loaders` or a new module. `open_row` filters source sheet rows matching the frequency group value and returns a new sheet. Requires storing a reference/snapshot of the source sheet data in `FreqDrill`.

---

---

## Meta Sheets

### GAP-079 — ColumnsSheet drill-down and editing
**Python**: `metasheets.py` — pressing Enter on a ColumnsSheet row opens the frequency table for that column. Column metadata (name, width, type, key) is editable in-place.
**Rust now**: `C` opens a read-only columns sheet with no drill and no editing.
**Instructions**:
  1. **Drill**: Set `drill` on the columns sheet to a `ColsDrill` that opens `sheet.frequency_sheet(row_col_idx)` when Enter is pressed.
  2. **Editing**: Wire `e` on the columns sheet to edit the `name`/`width`/`type`/`key` fields and write back to the source sheet's column definition. Requires the columns sheet to hold a reference to its source sheet index.

---

### GAP-080 — SheetsSheet drill-down (Enter navigates to that sheet)
**Python**: `indexsheet.py` — pressing Enter on the SheetsSheet navigates to (pushes) the selected sheet.
**Rust now**: `S` opens a sheets list sheet but Enter does nothing.
**Instructions**: The sheets sheet rows contain `Value::Text(sheet.name)` in column 0. Add a `SheetsDrill { }` struct implementing `DrillAction` that reads the sheet name from col 0, finds it in `self.stack` by name, and swaps to it. Since `DrillAction::open_row` returns a `Sheet`, the implementation needs to either return a clone of the target sheet or use a different mechanism (e.g., a special `Value::SheetRef(SheetId)` that the TUI handles separately). Alternative simpler approach: add a special `SpecialAction::GotoSheet` handled in `dispatch_command("open-row")` before calling `drill.open_row`.

---

### GAP-081 — DescribeSheet drill-down (select source rows)
**Python**: `features/describe.py` — `zs`/`zu` on a describe sheet selects/unselects source rows falling in that column's range.
**Rust now**: `I` describe sheet is read-only with no drill.
**Instructions**: Set `drill` on describe sheet. `open_row` on a describe row returns a filtered sheet of source rows for that column's non-null values. This is lower priority.

---

### GAP-082 — Open global options sheet (`O`)
**Python**: `optionssheet.py` — `O` opens the global options sheet.
**Rust now**: `O` is listed in `builtin_commands` and `dispatch_command` pushes an options sheet, but the options sheet is read-only display only.
**Instructions**: Wire `e` on the options sheet to open `InputMode::EditCell` for the value column. On Accept, call `options_manager.set(option_name, new_value, "global")`. Push `UndoAction` for the option change. The options manager already supports `set()`.

---

### GAP-083 — Open sheet-local options (`zO`)
**Python**: `optionssheet.py` — `zO` opens options scoped to the current sheet.
**Rust now**: Missing.
**Instructions**: Same as GAP-082 but pass current sheet's name/type as context to `options_manager.set(name, value, sheet_name)`. Bind `zO`.

---

### GAP-084 — Open ~/.visidatarc as text sheet (`gO`)
**Python**: `optionssheet.py` — `gO` opens the user config file.
**Rust now**: Missing. There is no config file concept yet.
**Instructions**: Define a config file path (e.g., `~/.vdrc` or `~/.config/vd/config.toml`). Push a `text_sheet_from_file` for that path. Allow editing and saving. Bind `gO`.

---

### GAP-085 — Describe all sheets (`gI`)
**Python**: `features/describe.py` — `gI` opens a describe sheet combining statistics from all sheets in the stack.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Iterate `self.stack`, call `describe_sheet` on each, concatenate into one sheet with an extra `sheet_name` column. Bind `gI`.

---

### GAP-086 — All-sessions sheets sheet (`gS`)
**Python**: `indexsheet.py` — `gS` shows every sheet ever opened in the session, not just the stack.
**Rust now**: `S` shows the current stack only.
**Instructions**: Add `all_sheets: Vec<SheetId>` to `App` (or a vec of sheet names) tracking every sheet ever pushed. `gS` builds a `sheets_sheet` from that history. Bind `gS`.

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

### GAP-089 — Threads sheet (`Ctrl+T`)
**Python**: `threads.py` — `threads-all` shows running/finished background threads.
**Rust now**: Missing. Loading state is tracked in `Sheet.loading_state` but there's no browsable thread list.
**Instructions**: Add a `threads_sheet()` function showing background load handles with name, status, rows loaded. Bind `Ctrl+T`. Low priority.

---

### GAP-090 — Macro sheet (`gm`)
**Python**: `macros.py` — `macro-sheet` opens a list of all saved macros.
**Rust now**: Only `Q` toggle record / replay via palette. No browsable macro list.
**Instructions**: Add `macros_sheet(macro_store: &MacroStore) -> Sheet` showing macro name, keystroke count, preview. Bind `gm`.

---

### GAP-091 — Command log save / replay (`Ctrl+D`)
**Python**: `cmdlog.py` — `save-cmdlog` saves the session's command history as a `.vdj` file for later replay.
**Rust now**: Missing. No command log is maintained.
**Instructions**: Maintain `self.command_log: Vec<(String, String)>` (longname, key) in `App`. `Ctrl+D` saves as JSON array to a `.vdj` file. Replay would read the file and replay each command. Initial implementation: just save, no replay.

---

---

## Multi-sheet Operations

### GAP-092 — Left / Right / Outer joins not bound
**Python**: `features/join.py` — join type is chosen interactively from a palette showing all join types.
**Rust now**: `join_sheets()` supports `Inner`, `Left`, `Right`, `Outer` but `&` always uses `Inner`.
**Instructions**: When `&` is pressed, open `InputMode::CommandPalette` pre-filled with join type options, or a dedicated `InputMode::JoinType`. On selection, call `join_sheets(left, right, selected_type)` and push result.

---

### GAP-093 — Join more than two sheets
**Python**: `features/join.py` — can join all selected sheets in one operation.
**Rust now**: `join_top_two_sheets()` only joins the top two.
**Instructions**: Add `join_selected_sheets()` that collects sheets where `sheet.rows` are marked selected (or from the sheets-sheet). Cascade-join them. Bind `g&`.

---

### GAP-094 — Concat sheets key binding
**Python**: Concat / append join type.
**Rust now**: `concat_sheets` is implemented in `sheets.rs` and registered in `builtin_commands` but has no key binding and is palette-only.
**Instructions**: Bind `g&` (after GAP-093) or a dedicated key such as `gA` for concatenate.

---

### GAP-095 — Pivot aggregation (not count-only)
**Python**: `pivot.py` — pivot cells show the result of the column's assigned aggregators (sum, avg, etc.), not just count.
**Rust now**: `pivot_sheet` hardcodes count per group.
**Instructions**: After implementing GAP-105 (persistent aggregators on columns), update `pivot_sheet` to call `col.aggregator` (if set) instead of incrementing a counter. Default to count when no aggregator is set.

---

### GAP-096 — Melt with regex column name parsing (`gM`)
**Python**: `features/melt.py` — `melt-regex` allows a capture regex to split column names into multiple variable columns (e.g., `sales_2023` → variable `sales`, period `2023`).
**Rust now**: `M` (basic melt) exists; `gM` missing.
**Instructions**: Extend `melt_sheet` to accept an optional `col_name_regex`. When provided, each variable row gets additional columns from the capture groups. Bind `gM`.

---

### GAP-097 — Save all sheets (`gCtrl+S`)
**Python**: `save.py` — `save-all` saves every sheet in the stack to a zip or a directory.
**Rust now**: Only single-sheet `Ctrl+S` exists.
**Instructions**: In `dispatch_command("save-all")`, iterate `self.stack`, call `save_sheet` for each that has a `source` path. Bind `gCtrl+S`.

---

---

## Aggregation

### GAP-098 — Persistent aggregators on columns (`+`)
**Python**: `aggregators.py` — `aggregate-col` marks a column with one or more aggregators; their results appear in frequency/pivot tables and in a dedicated summary row.
**Rust now**: `+` computes and shows in status bar only. No aggregator is stored on the column.
**Instructions**: Add `aggregators: Vec<AggFunc>` to `Column`. `+` prompts to choose from available functions and appends to the list. Display aggregate totals in the status bar footer row and in FrequencySheet/PivotSheet cells.

---

### GAP-099 — Memo aggregate to status (`z+`)
**Python**: `aggregators.py` — `memo-aggregate` shows result in status bar and stores in memory.
**Rust now**: `+` already shows in status bar. The difference is `z+` computes a specific aggregator chosen interactively rather than all five.
**Instructions**: Prompt for aggregator name. Compute and display chosen aggregator's result. Bind `z+`.

---

### GAP-100 — Additional aggregators: median, mode, stdev, distinct, count, percentiles
**Python**: `aggregators.py` — full set: min, max, avg/mean, median, mode, sum, distinct, count, list, stdev, q3/q4/q5/q10, p10–p99.
**Rust now**: Only count, sum, avg, min, max in `aggregation.rs`.
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
**Rust now**: `=` only adds a new expression column; cannot set values on an existing column.
**Instructions**: Detect if palette input starts with `g=`. Evaluate expression per selected row, call `sheet.set_cell(row_idx, col_source_idx, result)` for each. Bind `g=` in g-prefix block.

---

### GAP-102 — Set current cell from expression (`z=`)
**Python**: `expr.py` — `setcell-expr` applies expression to just the cursor cell.
**Rust now**: Missing.
**Instructions**: Same as GAP-101 but applied to `cursor_row` only. Bind `z=`.

---

### GAP-103 — Expression columns are lazily re-evaluated per render
**Python**: `expr.py` — `ExprColumn.calcValue` evaluates the expression each time the cell is rendered.
**Rust now**: `add_expression_column` in `expr_column.rs` materialises all values once at column-add time. If the source data changes (edit, sort, filter), expression column values become stale.
**Instructions**: Add a `Column::expr: Option<String>` field. When set, `Column::display_value(row)` evaluates the Rhai expression instead of reading from `row.values[source_idx]`. Remove the materialisation loop from `expr_column.rs`. This is a breaking change to how expression columns work; existing tests will need updating.

---

### GAP-104 — Expression columns are named by user
**Python**: `=name=expr` syntax sets the column name.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Parse `=name=expr` in the palette handler: if `=` input contains a second `=` after the name, split on first `=` to get name. Otherwise use `expr{N}`.

---

---

## Options System

### GAP-105 — Options consumed by CSV loader (`csv_delimiter`)
**Python**: CSV delimiter is read from `options.csv_delimiter` at load time.
**Rust now**: `CsvLoader` hardcodes delimiter based on file extension (`,` for `.csv`, `\t` for `.tsv`).
**Instructions**: In `CsvLoader::load`, read `options_manager.get("csv_delimiter", ...)`. Use as the delimiter byte. Requires threading the `OptionsManager` into loaders; either pass it through `LoaderRegistry::load_file` or make it a global (currently `App` owns it).

---

### GAP-106 — Options consumed by display formatting
**Python**: `disp_float_fmt`, `disp_int_fmt`, `disp_date_fmt` are read when formatting cell values.
**Rust now**: Column display uses hardcoded format strings in `column.rs::display_value`.
**Instructions**: Pass `OptionsManager` reference to `Column::display_value` (or store relevant format strings on `Column`). Read format strings from options instead of hardcoded values.

---

### GAP-107 — Color options consumed by renderer
**Python**: `color_selected_row`, `color_cursor_row`, `color_key_col`, etc. are read from options and can be changed at runtime.
**Rust now**: `Theme` struct has hardcoded colours; color options are declared in `builtin_options` but never read.
**Instructions**: In `app.rs`, after loading options, construct a `Theme` from option values. `color_selected_row` → `theme.row_selected`, etc. Re-derive theme when options change.

---

### GAP-108 — `quitguard` option not enforced
**Python**: When `quitguard = True`, quitting a modified sheet prompts for confirmation.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: In `dispatch_command("quit-sheet")`, check `options.get_bool("quitguard")` and `sheet.modified`. If both true, set status to `"sheet modified — press q again to quit"` and set a pending-quit flag instead of popping immediately.

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
**Rust now**: Shows raw row count `"loading… 1234 rows"`. The `LoadingState::Loading { rows_loaded }` has no total.
**Instructions**: Add `rows_total: Option<usize>` to `LoadingState::Loading`. For CSV/fixed-width, estimate from file size. Update status bar to show `"42% (1234 rows)"` when total is known.

---

### GAP-111 — Cancellation of async loads
**Python**: `Ctrl+C` cancels the current sheet's background threads.
**Rust now**: `LoadHandle.cancel` is an `Arc<AtomicBool>` but `Ctrl+C` is wired to unconditional `self.running = false`.
**Instructions**: Change `Ctrl+C` to first check if a load is in progress (`self.load_handle.is_some()`). If so, call `handle.cancel()` and set status `"load cancelled"`. Only quit the app if no load is active (or on second `Ctrl+C`).

---

---

## File I/O & Loaders

### GAP-112 — Reload sheet from source (`Ctrl+R`)
**Python**: `sheets.py` — `reload-sheet` re-runs the loader for the current sheet's source path, replacing rows/columns.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: In `dispatch_command("reload-sheet")`, if `sheet.source` is set, call `registry.load_file(&source_path)`, then replace `sheet.rows` and `sheet.columns` with the new sheet's data (keep `sheet.name`, `sheet.source`, cursor position). Bind `Ctrl+R`.

---

### GAP-113 — Save SQLite back to source
**Python**: `loaders/sqlite.py` — `SqliteSheet` has `putChanges()` executing `INSERT`/`UPDATE`/`DELETE` SQL within a transaction.
**Rust now**: `SqliteLoader` is read-only; no saver registered for `.sqlite`.
**Instructions**: Add `save_sqlite(sheet: &Sheet, path: &Path) -> Result<()>` to `sqlite_loader.rs`. Strategy: `DROP TABLE IF EXISTS "{name}"`, `CREATE TABLE`, `INSERT` all rows. More advanced: use the deferred-edit pattern (track adds/mods/dels). For initial implementation, a full table replacement is acceptable. Register as a saver in `saver.rs`.

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

### GAP-117 — Excel loader: only reads first sheet
**Python**: `loaders/xlsx.py` — `XlsxIndexSheet` lists all worksheets; Enter opens any sheet.
**Rust now**: `ExcelLoader` opens only the first worksheet from any `.xlsx`/`.xls`/`.ods` file.
**Instructions**: Use `calamine::open_workbook_auto` and iterate `workbook.sheet_names()`. Build an index sheet (name, row_count). Set `drill` to an `ExcelDrill { path, sheet_name }` that loads the named worksheet. The `calamine` crate supports named sheet access via `worksheet_range(name)`.

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

### GAP-120 — JSON streaming / JSONL improvements
**Python**: Handles deeply nested JSON, arrays at root level, single objects, auto-detects format.
**Rust now**: `JsonLoader` handles flat arrays of objects and JSONL. Nested objects/arrays are serialised to string. Root-level single objects produce a single row.
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
**Rust now**: `Theme` is hardcoded to `Theme::default()` in `App::new`. Three themes defined but none switchable.
**Instructions**: Read a `theme` option (`"default"`, `"dark"`, `"light"`) in `App`. On each render, call `Theme::by_name(options.get("theme"))`. When the user edits the theme option, the next render picks it up.

---

### GAP-123 — Status bar: show current aggregator / selection info
**Python**: Status bar shows the aggregator total for the cursor column when one is assigned.
**Rust now**: Status bar shows row/col counts and selection count but no aggregator total.
**Instructions**: After GAP-098 (persistent aggregators), read `col.aggregators.first()`, compute result, append to status bar string.

---

### GAP-124 — Column header shows hidden-column indicators
**Python**: When columns are hidden between visible ones, a narrow indicator column shows `…` or the count of hidden columns.
**Rust now**: Hidden columns (`width = Some(0)`) are simply skipped; no indicator is shown.
**Instructions**: In `draw_table`, when iterating visible columns, detect gaps between column indices (hidden columns in between) and render a narrow `…` divider cell.

---

### GAP-125 — Redraw / refresh screen (`Ctrl+L`)
**Python**: `Ctrl+L` forces a full terminal redraw.
**Rust now**: ✅ Implemented (commit: feat/rust session).
**Instructions**: Add `Ctrl+L` handling in `handle_normal_key` that calls `terminal.clear()` before the next render. Bind `Ctrl+L`.

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
**Rust now**: Only one macro ("last") is stored in memory; no naming, no keystroke assignment, no persistence.
**Instructions**: Add `HashMap<String, Macro>` to `MacroStore`. Allow naming a macro when stopping recording. Save/load from `~/.config/vd/macros.json` on startup/shutdown. Bind named macros to their assigned keystrokes.

---

### GAP-128 — Macro replay from file / command log
**Python**: `cmdlog.py` — a `.vdj` file can be replayed to reproduce a session.
**Rust now**: `macro_recorder.rs` supports recording but no file-based replay.
**Instructions**: After GAP-091 (command log), add `replay_command_log(path)` that reads the `.vdj` file and dispatches each command via `dispatch_command`. Bind `gCtrl+P` or palette `replay:`.

---

---

## Undo / Redo

### GAP-129 — Redo (`R`)
**Python**: `undo.py` — `redo-last` replays the most recently undone action.
**Rust now**: ✅ Implemented (commit: feat/rust session). Partial: works for InsertRow/DeleteRow/DeleteRows; cell/rename/type-change redo deferred.
**Instructions**: Add `redo_stack: Vec<UndoAction>` to `UndoStack`. When `sheet.undo()` pops an action and reverses it, push the action to `redo_stack`. Add `sheet.redo()` that pops from `redo_stack`, re-applies the action, and pushes to `undo_stack`. Any new edit clears `redo_stack`. Bind `R`.

---

### GAP-130 — Column rename undoable (see GAP-048)
Already listed as GAP-048.

---

### GAP-131 — Column type change undoable (see GAP-049)
Already listed as GAP-049.

---

### GAP-132 — Sort is not undoable
**Python**: Sort is tracked in the undo log.
**Rust now**: ✅ Implemented (commit: feat/rust session 2).
**Instructions**: Before sorting, save the current row order as `UndoAction::ReorderRows { order: Vec<RowId> }`. Add this variant to `UndoAction`. In `undo()`, restore row order by re-sorting `rows` to match the saved `RowId` sequence.

---

---

## External Loaders

### GAP-133 — External loader drill confirmed working (no change needed)
**Note**: Both `vd_duckdb` (line 83) and `vd_turso` (line 104) use `req.query.as_deref().unwrap_or(TABLE_INDEX_SQL)`. When `ExtDrill::open_row` sends `query: Some("SELECT * FROM \"table\"")`, the binary executes it and returns Arrow IPC. **No changes to either external loader binary are needed** for drill-down to work.

---

### GAP-134 — External loader: custom query via command palette
**Python**: `SqliteSheet.addCommand('', 'exec-sql', ...)` — user can type arbitrary SQL.
**Rust now**: No `exec-sql` equivalent for ext-loader sheets.
**Instructions**: When the active sheet has a `drill` of type `ExtDrill`, add a `exec-sql` command (palette prefix `sql:`) that takes arbitrary SQL, calls `ext_drill.loader.run_query(&ext_drill.db_path, Some(&sql), HashMap::new())`, and pushes the result.

---

### GAP-135 — External loader: pass options to subprocess
**Python**: Loader options (e.g., batch size) can be set via the options sheet and are passed to the subprocess.
**Rust now**: `run_query` always passes `HashMap::new()` for options.
**Instructions**: In `ExtDrill::open_row` and `ExtLoader::load`, populate the options map from the `OptionsManager` using keys prefixed with the loader name (e.g., `vd_duckdb_batch_size`). This requires threading `OptionsManager` into the drill call.

---

---

## Summary Table

| ID | Category | Priority |
|----|----------|----------|
| GAP-001 to GAP-012 | Navigation | High (001–005), Medium (006–012) |
| GAP-013 to GAP-025 | Selection | Critical (013–015, 020), High (016–025) |
| GAP-026 to GAP-047 | Column Ops | High (026–033), Medium (034–047) |
| GAP-048 to GAP-055 | Editing | High (048–053), Medium (054–055) |
| GAP-056 to GAP-063 | Clipboard | High (056–060), Medium (061–063) |
| GAP-064 to GAP-067 | Search | High (064–066), Medium (067) |
| GAP-068 to GAP-072 | Sorting | High (068–069), Medium (070–072) |
| GAP-073 to GAP-078 | Filtering | High (073–074), Medium (075–078) |
| GAP-079 to GAP-091 | Meta Sheets | Critical (079–084), High (085–091) |
| GAP-092 to GAP-097 | Multi-sheet | High (092–094), Medium (095–097) |
| GAP-098 to GAP-100 | Aggregation | High (098–099), Medium (100) |
| GAP-101 to GAP-104 | Expressions | High (101–103), Medium (104) |
| GAP-105 to GAP-108 | Options | High (105–107), Critical (108) |
| GAP-109 to GAP-111 | Async Loading | High (109–110), Medium (111) |
| GAP-112 to GAP-120 | Loaders/I/O | Critical (112), High (113–117), Low (118–120) |
| GAP-121 to GAP-126 | TUI | Medium (121–122), Low (123–126) |
| GAP-127 to GAP-128 | Macros | Low |
| GAP-129 to GAP-132 | Undo/Redo | High (129), Medium (130–132) |
| GAP-133 to GAP-135 | Ext Loaders | Info (133), Low (134–135) |

**Total gaps: 135** (GAP-130 and GAP-131 cross-reference GAP-048/049; effective distinct gaps: 133)
