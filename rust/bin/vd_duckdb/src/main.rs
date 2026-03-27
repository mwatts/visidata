//! `vd_duckdb` — `DuckDB` external loader for `vd`.
//!
//! Implements the Arrow IPC subprocess protocol defined in `docs/ext-loaders.md`.
//! Uses NDJSON transport because `DuckDB` bundles its own Arrow version which
//! is not type-compatible with the host's `arrow` crate at the Rust level.
//!
//! ## Usage
//!
//! ```sh
//! # Manifest probe (called by vd at startup):
//! vd_duckdb --manifest
//!
//! # Load table index (called when user opens a .duckdb file):
//! echo '{"path":"/data/sales.duckdb","query":null,"options":{}}' | vd_duckdb
//!
//! # Load a specific table:
//! echo '{"path":"/data/sales.duckdb","query":"SELECT * FROM orders","options":{}}' | vd_duckdb
//! ```

use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};
use visidata_ext_protocol::{ExtManifest, LoadRequest, Transport};

/// SQL run when `query` is null — return a table/view index.
const TABLE_INDEX_SQL: &str = "\
    SELECT \
        table_name  AS name, \
        table_type  AS type \
    FROM information_schema.tables \
    WHERE table_schema = 'main' \
    ORDER BY table_type, table_name";

fn main() {
    if let Err(e) = run() {
        eprintln!("vd_duckdb: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    if std::env::args().any(|a| a == "--manifest") {
        return print_manifest();
    }
    run_load()
}

fn print_manifest() -> Result<()> {
    let manifest = ExtManifest {
        name: "vd_duckdb".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        extensions: vec!["duckdb".into(), "ddb".into()],
        schemes: vec!["duckdb://".into()],
        transport: Transport::Ndjson,
    };
    let json = serde_json::to_string(&manifest).context("failed to serialise manifest")?;
    println!("{json}");
    Ok(())
}

fn run_load() -> Result<()> {
    // Read the single JSON request line from stdin.
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .context("failed to read LoadRequest from stdin")?;

    let req: LoadRequest =
        serde_json::from_str(line.trim()).context("failed to parse LoadRequest")?;

    // Open DuckDB in read-only mode.
    let conn = duckdb::Connection::open_with_flags(
        &req.path,
        duckdb::Config::default()
            .access_mode(duckdb::AccessMode::ReadOnly)
            .context("failed to set DuckDB access mode")?,
    )
    .with_context(|| format!("failed to open DuckDB file: {}", req.path))?;

    let sql = req.query.as_deref().unwrap_or(TABLE_INDEX_SQL);

    let mut stmt = conn
        .prepare(sql)
        .with_context(|| format!("failed to prepare: {sql}"))?;

    // Execute as rows and write NDJSON to stdout.
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    // Collect column names before mutably borrowing via query().
    let col_names: Vec<String> = stmt.column_names();

    let mut rows = stmt.query([]).context("failed to execute query")?;

    while let Some(row) = rows.next().context("error reading DuckDB row")? {
        let mut obj = serde_json::Map::with_capacity(col_names.len());
        for (i, name) in col_names.iter().enumerate() {
            let val = duckdb_value(row, i);
            obj.insert(name.clone(), val);
        }
        let line = serde_json::to_string(&serde_json::Value::Object(obj))
            .context("failed to serialise row")?;
        writeln!(out, "{line}").context("failed to write NDJSON row")?;
    }

    out.flush().context("failed to flush output")?;
    Ok(())
}

/// Extract a value from a `DuckDB` row at column index `i` as a JSON value.
fn duckdb_value(row: &duckdb::Row<'_>, i: usize) -> serde_json::Value {
    // Try types in priority order: int → float → bool → text → null.
    if let Ok(v) = row.get::<_, i64>(i) {
        return serde_json::Value::Number(v.into());
    }
    if let Ok(v) = row.get::<_, f64>(i) {
        return serde_json::Number::from_f64(v)
            .map_or(serde_json::Value::Null, serde_json::Value::Number);
    }
    if let Ok(v) = row.get::<_, bool>(i) {
        return serde_json::Value::Bool(v);
    }
    if let Ok(v) = row.get::<_, String>(i) {
        return serde_json::Value::String(v);
    }
    serde_json::Value::Null
}
