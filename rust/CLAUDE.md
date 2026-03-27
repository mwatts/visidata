Always load the `rust` skill at the start of each session when working in this project.

## Build & Test

- `cargo test --workspace` — run all tests
- `cargo clippy --workspace` — lint (zero warnings required)
- `cargo fmt --check` — check formatting
- `cargo run -p vd` — run the binary

## Workspace Structure

### Library crates (`crates/`)

- `crates/visidata-core` — data model (Value, Column, Row, Sheet, SheetStack), options, commands
- `crates/visidata-scripting` — Rhai scripting engine
- `crates/visidata-tui` — ratatui TUI rendering, menu, command palette, async loading
- `crates/visidata-loaders` — built-in file format loaders (CSV, JSON, SQLite, Parquet, Excel, YAML, HTML, fixed-width) + Arrow utilities + external loader host
- `crates/visidata-ext-protocol` — shared serde types for the external loader subprocess protocol

### Binaries (`bin/`)

- `bin/vd` — CLI entry point
- `bin/vd_duckdb` — DuckDB external loader (Arrow IPC subprocess protocol)
- `bin/vd_turso` — TursoDB external loader (Arrow IPC subprocess protocol)

## External Loaders

External loaders extend `vd` without adding heavy dependencies to the host
binary. See `docs/ext-loaders.md` for the full protocol spec.

The host discovers `vd_*` binaries on `$PATH` at startup, probes them with
`--manifest`, then spawns them on demand, passing a `LoadRequest` JSON line on
stdin and reading an Arrow IPC stream from stdout.

## Python Baseline Tracking

**See `PYTHON_BASELINE.md`** in this directory for the Python commit that the
Rust port branched from, instructions for diffing Python changes since that
point, and the sync history.

The baseline commit is: `43e129e5665877c0f6710d34685837dad88fe1ed`

When evaluating whether a Python bug fix or new feature needs to be ported,
run:

```sh
git log --oneline 43e129e5..develop -- visidata/
```

## Install

```sh
just install          # install vd + vd_duckdb + vd_turso to ~/.cargo/bin
just install-turso    # install only the TursoDB loader
just install-duckdb   # install only the DuckDB loader
```
