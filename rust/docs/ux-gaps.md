# VisiData Rust — UX Gaps & Improvement Backlog

Compared against Python VisiData. Items marked with effort and priority.
Status: `[ ]` open · `[~]` in progress · `[x]` done · `[-]` won't fix

---

## High Priority — Low Effort

- [x] **GAP-UX-01** Status bar: show selected row count (e.g. `3 selected`) — already implemented
- [x] **GAP-UX-02** Status bar: show `[+]` when sheet has unsaved modifications — already implemented (shows `[+]`)
- [x] **GAP-UX-03** Search: report match count in status — shows "found: pat — N matches"
- [x] **GAP-UX-04** Undo: descriptive status — "undone: delete 3 rows", "undone: edit cell", etc.
- [x] **GAP-UX-05** Sort: show priority number in multi-key sort headers — ↑1, ↓2 etc.
- [x] **GAP-UX-06** Navigation: `go-screen-top/middle/bottom` — bound to `zH`/`zM`/`zL`
- [x] **GAP-UX-07** Column header: type indicator character shown before column name (`~name`, `#age`)
- [x] **GAP-UX-08** Empty sheet: centred "No rows." message when table has zero rows
- [x] **GAP-UX-09** Search: shows "(wrapped)" in status when search wraps around
- [x] **GAP-UX-10** Column resize: pre-fills current width in `ResizeColInput`
- [x] **GAP-UX-11** Type-change: reports "N cells failed to convert to int" etc. in status
- [x] **GAP-UX-12** Null display: null cells now render as `~` (Python-compatible default)

---

## High Priority — Medium Effort

- [x] **GAP-UX-13** Input history: Up/Down in search inputs (`/`, `?`) recalls previous patterns
- [x] **GAP-UX-14** Join operations: all four join types implemented (`join-type-inner/left/right/outer`)
- [x] **GAP-UX-15** Numeric frequency bins: FreqSheet auto-bins numeric cols into 10 equal-width ranges when >10 distinct values
- [x] **GAP-UX-16** Status history sheet: `Ctrl+P` opens sheet of all past status messages
- [x] **GAP-UX-17** Navigation: `z<` / `z>` jump to prev/next null cell in current column

---

## Medium Priority — Low Effort

- [x] **GAP-UX-18** Column aggregator: shown in Columns Sheet `aggregator` column
- [x] **GAP-UX-19** Keybinding shadow warning: startup warns when `visidatarc.toml` overrides a builtin
- [x] **GAP-UX-20** Command palette: results sorted by usage frequency (most-used commands float to top)
- [x] **GAP-UX-21** Quit confirmation: already done — "press q again to quit" on modified sheet
- [x] **GAP-UX-22** Save feedback: now reports rows saved and file size in status

---

## Medium Priority — Medium Effort

- [x] **GAP-UX-23** Dedupe: `z,` selects rows where key column values repeat
- [x] **GAP-UX-24** Rank column: `zR` adds rank column based on current column values
- [x] **GAP-UX-25** Auto-refresh: `reload-every` command prompts for interval in seconds; auto-reloads in event loop
- [x] **GAP-UX-26** Sort feedback: status shows "sorted by colname ↑/↓" after sort commands
- [x] **GAP-UX-27** Right-click context menu: 6-item floating menu; j/k to navigate, Enter to execute, Esc to dismiss

---

## Lower Priority — High Effort

- [x] **GAP-UX-28** Deferred modifications: `toggle-deferred-mode`; `d`/`gd` marks pending (crossed-out); `gc` commits, `gD` discards
- [x] **GAP-UX-29** Sidebar: `Ctrl+G` toggles 26-char info panel — sheet name, dimensions, column type/agg, sort keys
- [x] **GAP-UX-30** Pivot tables: `W` → `pivot_sheet`, grouping + aggregation + drill-down (already implemented)

---

## Implemented (previously misclassified)
- [x] **GAP-UX-31** Melt/reshape: `M` → `melt_sheet`; `gM` → melt with column-name regex
- [x] **GAP-UX-32** Expression search: `z/`/`z?` runs Rhai expression search (equivalent to Python's)
