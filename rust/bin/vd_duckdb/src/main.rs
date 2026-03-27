//! `vd_duckdb` — `DuckDB` external loader for `vd`.
//!
//! Implements the Arrow IPC subprocess protocol defined in `docs/ext-loaders.md`.
//! Uses Arrow IPC transport — `DuckDB`'s `query_arrow()` produces `RecordBatch`
//! values that are type-compatible with the host because both depend on
//! `arrow = "58"`.
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

use std::io::{self, BufRead};

use anyhow::{Context, Result};
use arrow::ipc::writer::StreamWriter;
use visidata_ext_protocol::{ExtManifest, LoadRequest, Transport};

/// SQL run when `query` is null — return a table/view index.
const TABLE_INDEX_SQL: &str = "\
    SELECT \
        table_schema AS schema, \
        table_name AS name, \
        table_type AS type \
    FROM information_schema.tables \
    WHERE table_schema NOT IN ('information_schema', 'pg_catalog') \
    ORDER BY table_schema, table_type, table_name";

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
        transport: Transport::ArrowIpc,
    };
    println!(
        "{}",
        serde_json::to_string(&manifest).context("failed to serialise manifest")?
    );
    Ok(())
}

fn run_load() -> Result<()> {
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .context("failed to read LoadRequest from stdin")?;

    let req: LoadRequest =
        serde_json::from_str(line.trim()).context("failed to parse LoadRequest")?;

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

    // `query_arrow` returns `arrow 58` RecordBatch — same type the host uses.
    let arrow_stream = stmt
        .query_arrow([])
        .context("failed to execute Arrow query")?;

    let schema = arrow_stream.get_schema();
    let stdout = io::stdout();
    let mut writer = StreamWriter::try_new(stdout.lock(), &schema)
        .context("failed to create Arrow IPC stream writer")?;

    for batch in arrow_stream {
        writer
            .write(&batch)
            .context("failed to write Arrow batch")?;
    }

    writer
        .finish()
        .context("failed to finalise Arrow IPC stream")?;
    Ok(())
}
