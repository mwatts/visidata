# Python Baseline for Rust Port

This file records the Python codebase commit that the Rust port branched from.
Its purpose is to make the **merge baseline easily discoverable** so that any
coding agent (or human) can identify exactly which Python features, bug fixes,
and changes need to be evaluated for porting into the Rust codebase.

## Baseline Commit

```
Commit:  43e129e5665877c0f6710d34685837dad88fe1ed
Date:    2026-03-24 10:18:48 -0700
Message: [tests] export XDG_DATA_HOME for macro tests in test-vdx.sh
Branch:  develop (at time of Rust branch creation)
```

This is the `develop` branch tip from which `feature/rust` diverged.
Verified via: `git merge-base feature/rust develop`

## How to Use This Baseline

### Find all Python changes since the baseline

```sh
# From the repo root:
git log --oneline 43e129e5..develop -- visidata/
```

### Diff Python source since the baseline

```sh
git diff 43e129e5..develop -- visidata/
```

### Check a specific Python file for changes

```sh
git log --oneline 43e129e5..develop -- visidata/loaders/csv.py
```

### Summarise changed Python files (for triage)

```sh
git diff --stat 43e129e5..develop -- visidata/
```

## When to Update This File

Update the `Baseline Commit` block whenever the Rust port is synced forward
to a newer Python commit — i.e., after a deliberate merge/sync pass where the
team has reviewed Python changes and either ported them or consciously deferred
them.

Record the new baseline here along with the date of the sync and a brief note
on what was covered, so the history of sync points is preserved.

## Sync History

| Sync date  | Baseline commit  | Notes                              |
|------------|------------------|------------------------------------|
| 2026-03-24 | `43e129e5`       | Initial Rust port branch point     |

## Crate Coverage at Baseline

The following Python subsystems have Rust equivalents as of the baseline:

| Python module / area          | Rust crate / binary                     | Status        |
|-------------------------------|-----------------------------------------|---------------|
| Core data model               | `crates/visidata-core`                  | Ported        |
| Rhai scripting scaffold       | `crates/visidata-scripting`             | Ported        |
| ratatui TUI rendering         | `crates/visidata-tui`                   | Ported        |
| CSV / TSV / JSON loaders      | `crates/visidata-loaders`               | Ported        |
| Column ops, type system       | `crates/visidata-core`                  | Ported        |
| Sorting, filtering, selection | `crates/visidata-core`                  | Ported        |
| Command system, help sheet    | `crates/visidata-tui`                   | Ported        |
| Options system                | `crates/visidata-core`                  | Ported        |
| Color theming                 | `crates/visidata-tui`                   | Ported        |
| Sheet types (cols, desc, dir) | `crates/visidata-tui`                   | Ported        |
| SQLite / Excel / Parquet      | `crates/visidata-loaders`               | Ported        |
| YAML / HTML / fixed-width     | `crates/visidata-loaders`               | Ported        |
| Editing, undo, save           | `crates/visidata-core` + tui            | Ported        |
| Multi-sheet operations        | `crates/visidata-core`                  | Ported        |
| Async loading + progress      | `crates/visidata-tui`                   | Ported        |
| Menu system                   | `crates/visidata-tui`                   | Ported        |
| DuckDB loader                 | `bin/vd_duckdb` (ext loader)            | Ported        |
| TursoDB loader                | `bin/vd_turso` (ext loader)             | New in Rust   |
