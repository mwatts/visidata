Always load the `rust` skill at the start of each session when working in this project.

## Build & Test

- `cargo test --workspace` — run all tests
- `cargo clippy --workspace` — lint (zero warnings required)
- `cargo fmt --check` — check formatting
- `cargo run -p vd` — run the binary

## Workspace Structure

- `crates/visidata-core` — data model (Value, Column, Row, Sheet, SheetStack)
- `crates/visidata-scripting` — Rhai scripting engine
- `crates/visidata-tui` — ratatui TUI rendering
- `crates/visidata-loaders` — file format loaders
- `bin/vd` — CLI binary entry point
