# External Loader Protocol

External loaders extend `vd` with file format support that has heavy native
dependencies — database drivers, JVM-backed systems, large C libraries — without
those dependencies entering the host binary. The first external loader is
`vd_duckdb`.

## Architecture

```
vd (host)                        vd_duckdb (subprocess)
─────────────────────────────    ──────────────────────────────────
LoaderRegistry                   main()
  ExtLoaderRegistry                ├── --manifest → print JSON, exit 0
    discover() scans $PATH         └── stdin → LoadRequest
    probes vd_* binaries                 → DuckDB query
                                         → Arrow IPC stream → stdout
ExtLoader implements Loader
  spawn subprocess
  write LoadRequest to stdin
  read Arrow IPC from stdout
  convert RecordBatch → Sheet
```

## Crates

```
crates/
  visidata-ext-protocol/   shared serde types; no heavy deps
  visidata-loaders/        gains ext_loader + ext_discovery modules
bin/
  vd_duckdb/               external loader binary for DuckDB
```

## Transport

Two response transports are supported. The extension declares which it uses
in the manifest:

| `transport` field | Format | Use when |
|---|---|---|
| `"arrow-ipc"` (default) | Arrow IPC stream on stdout | Extension uses the same Arrow version as the host |
| `"ndjson"` | Newline-delimited JSON on stdout | Extension bundles its own Arrow (e.g. DuckDB) |

`vd_duckdb` uses `"ndjson"` because DuckDB bundles Arrow internally and its
types are not compatible with the host's `arrow` crate at the Rust type level.
The host deserialises NDJSON rows using the same path as the JSON loader.

## Protocol

### Discovery

At startup, `ExtLoaderRegistry::discover()` scans every directory in `$PATH`
for executables whose name starts with `vd_`. Each candidate is probed:

```
$ vd_duckdb --manifest
{"name":"vd_duckdb","version":"0.1.0","extensions":["duckdb","ddb"],"schemes":["duckdb://"]}
```

- One JSON line on stdout, exit 0 = valid external loader.
- Non-zero exit, timeout (1 s), or parse failure = silently skipped.
- Built-in loaders always take priority; an external loader claiming `csv`
  is ignored.

### Load Request

When the user opens a file whose extension matches an external loader, the host
spawns the binary (no flags), writes one JSON line to stdin, then closes stdin:

```json
{
  "path": "/home/mark/sales.duckdb",
  "query": null,
  "options": {}
}
```

`query: null` means "table index" — return the list of tables/views so the user
can navigate into one. A non-null query is a SQL string to execute:

```json
{
  "path": "/home/mark/sales.duckdb",
  "query": "SELECT * FROM orders",
  "options": {"batch_size": 5000}
}
```

### Load Response

The extension writes a standard **Arrow IPC stream** to stdout:

1. Schema message (column names + Arrow types)
2. Zero or more record batch messages
3. EOS marker

Errors go to **stderr** as plain text. The host captures stderr and shows it in
the status bar. Exit 0 = success; non-zero = error.

### Cancellation

The host closes the subprocess stdin pipe. The extension SHOULD check for stdin
EOF between batches and stop writing. Standard Unix cooperative cancellation —
no signals, no secondary channel.

## Type Mapping

The host converts Arrow types to `Value` using the same `arrow_value_at`
function that the Parquet loader uses, now exported from
`visidata-loaders::arrow_util`. No type mapping is required on the extension
side — DuckDB's `query_arrow()` returns `RecordBatch` directly.

## File Extension vs URI Dispatch

- **Extension** (`.duckdb`, `.ddb`): host opens the file and sends
  `query: null`. Extension returns a table-index sheet (one row per
  table/view). User navigates into a table with Enter, which triggers a second
  subprocess call with the SELECT query.
- **URI scheme** (`duckdb://`): reserved for future network/remote support;
  not implemented in this version.

## DuckDB Table Navigation

Opening a `.duckdb` file produces an index sheet:

```
name          type    row_count
──────────────────────────────
orders        table   142 893
customers     table    12 445
revenue_view  view         -
```

Pressing Enter on a row calls `ExtLoader::run_query` with:

```sql
SELECT * FROM "orders"
```

This is wired in the TUI as a sheet-type-aware Enter handler (future work);
for now the user can invoke it via the command palette with `load-table`.

## DuckDB-Specific Notes

- The `duckdb` Rust crate's `query_arrow([])` method returns an iterator of
  `RecordBatch` with no intermediate format conversion.
- DuckDB types (HUGEINT, LIST, STRUCT, MAP) that have no direct `Value`
  equivalent are rendered as their Arrow display string.
- The extension opens DuckDB in **read-only mode** (`Connection::open_with_flags`
  with `AccessMode::ReadOnly`) to prevent accidental writes.

## Adding Another External Loader

1. Create a new binary crate in `bin/vd_NAME/`.
2. Add `visidata-ext-protocol` as a dependency.
3. Implement `--manifest` (print `ExtManifest` JSON, exit 0).
4. Read one `LoadRequest` line from stdin.
5. Write an Arrow IPC stream to stdout.
6. Install the binary on `$PATH`.

No changes to the host are needed.

## Security Considerations

External loaders are arbitrary executables discovered on `$PATH`. The host
trusts `$PATH` implicitly — the same trust model as any shell tool. There is no
sandboxing in this version. Do not install untrusted binaries alongside `vd`.
