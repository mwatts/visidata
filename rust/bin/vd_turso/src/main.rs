//! `vd_turso` — `TursoDB` external loader for `vd`.
//!
//! Implements the Arrow IPC subprocess protocol defined in `docs/ext-loaders.md`.
//! `TursoDB` has no native Arrow output, so rows are collected, per-column Arrow
//! types are inferred from the runtime `turso::Value` variants, typed Arrow
//! arrays are built, and one `RecordBatch` is written as an Arrow IPC stream.
//!
//! ## Usage
//!
//! ```sh
//! # Manifest probe (called by vd at startup):
//! vd_turso --manifest
//!
//! # Load table index (called when user opens a .turso or .tdb file):
//! echo '{"path":"/data/sales.turso","query":null,"options":{}}' | vd_turso
//!
//! # Load a specific table:
//! echo '{"path":"/data/sales.turso","query":"SELECT * FROM orders","options":{}}' | vd_turso
//! ```

use std::io::{self, BufRead};
use std::sync::Arc; // used by ColBuilder::finish return type

use anyhow::{Context, Result};
use arrow::array::{
    Array, BinaryBuilder, Float64Builder, Int64Builder, StringBuilder,
};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use visidata_ext_protocol::{ExtManifest, LoadRequest, Transport};

/// SQL run when `query` is null — returns a table/view index.
///
/// Uses `sqlite_master` (SQLite-compatible), unlike `DuckDB`'s `information_schema`.
const TABLE_INDEX_SQL: &str = "\
    SELECT name, type \
    FROM sqlite_master \
    WHERE type IN ('table', 'view') \
    ORDER BY type, name";

fn main() {
    if let Err(e) = run() {
        eprintln!("vd_turso: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    if std::env::args().any(|a| a == "--manifest") {
        return print_manifest();
    }
    // TursoDB is async-only. Use a single-threaded runtime — minimal overhead
    // for a short-lived subprocess with no internal parallelism.
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("failed to create Tokio runtime")?
        .block_on(async_run())
}

fn print_manifest() -> Result<()> {
    let manifest = ExtManifest {
        name: "vd_turso".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        extensions: vec!["turso".into(), "tdb".into()],
        schemes: vec![],
        transport: Transport::ArrowIpc,
    };
    println!(
        "{}",
        serde_json::to_string(&manifest).context("failed to serialise manifest")?
    );
    Ok(())
}

async fn async_run() -> Result<()> {
    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .context("failed to read LoadRequest from stdin")?;

    let req: LoadRequest =
        serde_json::from_str(line.trim()).context("failed to parse LoadRequest")?;

    run_load(req).await
}

#[expect(
    clippy::significant_drop_tightening,
    reason = "`db` must remain alive for the duration of `conn`; early drop would close the database"
)]
async fn run_load(req: LoadRequest) -> Result<()> {
    let db = turso::Builder::new_local(&req.path)
        .build()
        .await
        .with_context(|| format!("failed to open TursoDB file: {}", req.path))?;

    let conn = db
        .connect()
        .context("failed to create TursoDB connection")?;

    let sql = req.query.as_deref().unwrap_or(TABLE_INDEX_SQL);

    let mut rows = conn
        .query(sql, ())
        .await
        .with_context(|| format!("failed to execute query: {sql}"))?;

    // Snapshot column metadata before consuming rows.
    let col_count = rows.column_count();
    let col_names: Vec<String> = (0..col_count)
        .map(|i| rows.column_name(i).unwrap_or_else(|_| "?".to_owned()))
        .collect();

    // Collect all rows. TursoDB has no Arrow output, so we buffer first to
    // infer per-column Arrow types before building typed arrays.
    let mut all_rows: Vec<Vec<turso::Value>> = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .context("error iterating TursoDB rows")?
    {
        let vals = (0..col_count)
            .map(|i| row.get_value(i))
            .collect::<Result<Vec<_>, _>>()
            .context("failed to read row values")?;
        all_rows.push(vals);
    }

    // Infer Arrow DataType for each column by examining all non-null values.
    let col_types: Vec<DataType> = (0..col_count)
        .map(|ci| infer_col_type(all_rows.iter().map(|r| &r[ci])))
        .collect();

    // Build Arrow schema — all fields nullable (SQLite has dynamic typing).
    let fields: Vec<Field> = col_names
        .iter()
        .zip(col_types.iter())
        .map(|(name, dt)| Field::new(name.as_str(), dt.clone(), true))
        .collect();
    let schema = Arc::new(Schema::new(fields));

    // Build one typed Arrow array per column.
    let mut builders: Vec<ColBuilder> = col_types
        .iter()
        .map(|dt| ColBuilder::new(dt, all_rows.len()))
        .collect();

    for row in &all_rows {
        for (ci, val) in row.iter().enumerate() {
            builders[ci].append(val);
        }
    }

    let arrays: Vec<Arc<dyn Array>> = builders
        .into_iter()
        .map(ColBuilder::finish)
        .collect();

    let batch =
        RecordBatch::try_new(schema.clone(), arrays).context("failed to build RecordBatch")?;

    // Write Arrow IPC stream to stdout.
    let stdout = io::stdout();
    let mut writer = StreamWriter::try_new(stdout.lock(), &schema)
        .context("failed to create Arrow IPC stream writer")?;
    writer.write(&batch).context("failed to write Arrow batch")?;
    writer
        .finish()
        .context("failed to finalise Arrow IPC stream")?;

    Ok(())
}

// ── Type inference ────────────────────────────────────────────────────────────

/// Infer the Arrow `DataType` for a column by examining all of its values.
///
/// Rules (evaluated in priority order):
/// - All null            → `Utf8` (safe default)
/// - Only blobs (+ null) → `Binary`
/// - Any text            → `Utf8`  (mixed blob+text stringified as hex)
/// - Any real (+ int)    → `Float64` (integers widened)
/// - Only integers       → `Int64`
fn infer_col_type<'a>(vals: impl Iterator<Item = &'a turso::Value>) -> DataType {
    let (mut has_int, mut has_real, mut has_text, mut has_blob) =
        (false, false, false, false);

    for v in vals {
        match v {
            turso::Value::Null => {}
            turso::Value::Integer(_) => has_int = true,
            turso::Value::Real(_) => has_real = true,
            turso::Value::Text(_) => has_text = true,
            turso::Value::Blob(_) => has_blob = true,
        }
    }

    match (has_blob, has_text, has_real, has_int) {
        (true, false, false, false) => DataType::Binary,
        _ if has_text || has_blob => DataType::Utf8,
        _ if has_real => DataType::Float64,
        _ if has_int => DataType::Int64,
        _ => DataType::Utf8,
    }
}

// ── Column builder ────────────────────────────────────────────────────────────

/// A typed Arrow array builder for one column.
enum ColBuilder {
    Int64(Int64Builder),
    Float64(Float64Builder),
    Utf8(StringBuilder),
    Binary(BinaryBuilder),
}

impl ColBuilder {
    fn new(dt: &DataType, capacity: usize) -> Self {
        match dt {
            DataType::Int64 => Self::Int64(Int64Builder::with_capacity(capacity)),
            DataType::Float64 => Self::Float64(Float64Builder::with_capacity(capacity)),
            DataType::Binary => Self::Binary(BinaryBuilder::with_capacity(capacity, 0)),
            _ => Self::Utf8(StringBuilder::with_capacity(capacity, 0)),
        }
    }

    fn append(&mut self, val: &turso::Value) {
        match self {
            Self::Int64(b) => match val {
                turso::Value::Integer(i) => b.append_value(*i),
                _ => b.append_null(),
            },
            Self::Float64(b) => match val {
                turso::Value::Real(f) => b.append_value(*f),
                // Integer widened to float when the column contains mixed int+real.
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "i64→f64 widening for mixed int+real columns; display precision is acceptable"
                )]
                turso::Value::Integer(i) => b.append_value(*i as f64),
                _ => b.append_null(),
            },
            Self::Binary(b) => match val {
                turso::Value::Blob(bytes) => b.append_value(&bytes[..]),
                _ => b.append_null(),
            },
            Self::Utf8(b) => match val {
                turso::Value::Text(s) => b.append_value(s.as_str()),
                turso::Value::Integer(i) => b.append_value(i.to_string().as_str()),
                turso::Value::Real(f) => b.append_value(f.to_string().as_str()),
                // Blobs in a mixed column are hex-encoded so they display readably.
                turso::Value::Blob(bytes) => {
                    use std::fmt::Write as _;
                    let hex = bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, byte| {
                        let _ = write!(s, "{byte:02x}");
                        s
                    });
                    b.append_value(hex.as_str());
                }
                turso::Value::Null => b.append_null(),
            },
        }
    }

    fn finish(self) -> Arc<dyn Array> {
        match self {
            Self::Int64(mut b) => Arc::new(b.finish()),
            Self::Float64(mut b) => Arc::new(b.finish()),
            Self::Binary(mut b) => Arc::new(b.finish()),
            Self::Utf8(mut b) => Arc::new(b.finish()),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use turso::Value as V;

    // ── infer_col_type ────────────────────────────────────────────────────────

    #[test]
    fn infer_all_null() {
        assert_eq!(
            infer_col_type([V::Null, V::Null].iter()),
            DataType::Utf8
        );
    }

    #[test]
    fn infer_integers_only() {
        assert_eq!(
            infer_col_type([V::Integer(1), V::Null, V::Integer(2)].iter()),
            DataType::Int64
        );
    }

    #[test]
    fn infer_reals_only() {
        assert_eq!(
            infer_col_type([V::Real(1.5), V::Null].iter()),
            DataType::Float64
        );
    }

    #[test]
    fn infer_int_and_real_widens_to_float() {
        assert_eq!(
            infer_col_type([V::Integer(1), V::Real(2.5)].iter()),
            DataType::Float64
        );
    }

    #[test]
    fn infer_text_only() {
        assert_eq!(
            infer_col_type([V::Text("hi".to_owned()), V::Null].iter()),
            DataType::Utf8
        );
    }

    #[test]
    fn infer_blob_only() {
        assert_eq!(
            infer_col_type([V::Blob(vec![0xde, 0xad]), V::Null].iter()),
            DataType::Binary
        );
    }

    #[test]
    fn infer_mixed_blob_and_text_is_utf8() {
        assert_eq!(
            infer_col_type([V::Text("hi".to_owned()), V::Blob(vec![0x01])].iter()),
            DataType::Utf8
        );
    }

    // ── ColBuilder ────────────────────────────────────────────────────────────

    fn finish_int64(vals: &[turso::Value]) -> arrow::array::Int64Array {
        let mut b = ColBuilder::new(&DataType::Int64, vals.len());
        for v in vals {
            b.append(v);
        }
        match b.finish().as_any().downcast_ref::<arrow::array::Int64Array>() {
            Some(a) => a.clone(),
            None => panic!("expected Int64Array"),
        }
    }

    fn finish_float64(vals: &[turso::Value]) -> arrow::array::Float64Array {
        let mut b = ColBuilder::new(&DataType::Float64, vals.len());
        for v in vals {
            b.append(v);
        }
        match b
            .finish()
            .as_any()
            .downcast_ref::<arrow::array::Float64Array>()
        {
            Some(a) => a.clone(),
            None => panic!("expected Float64Array"),
        }
    }

    fn finish_utf8(vals: &[turso::Value]) -> arrow::array::StringArray {
        let mut b = ColBuilder::new(&DataType::Utf8, vals.len());
        for v in vals {
            b.append(v);
        }
        match b
            .finish()
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
        {
            Some(a) => a.clone(),
            None => panic!("expected StringArray"),
        }
    }

    fn finish_binary(vals: &[turso::Value]) -> arrow::array::BinaryArray {
        let mut b = ColBuilder::new(&DataType::Binary, vals.len());
        for v in vals {
            b.append(v);
        }
        match b
            .finish()
            .as_any()
            .downcast_ref::<arrow::array::BinaryArray>()
        {
            Some(a) => a.clone(),
            None => panic!("expected BinaryArray"),
        }
    }

    #[test]
    fn col_int64_appends_integer() {
        let arr = finish_int64(&[V::Integer(42), V::Null, V::Integer(-1)]);
        assert_eq!(arr.value(0), 42);
        assert!(arr.is_null(1));
        assert_eq!(arr.value(2), -1);
    }

    #[test]
    fn col_float64_appends_real() {
        let arr = finish_float64(&[V::Real(3.14), V::Null]);
        assert!((arr.value(0) - 3.14).abs() < f64::EPSILON);
        assert!(arr.is_null(1));
    }

    #[test]
    fn col_float64_widens_integer() {
        let arr = finish_float64(&[V::Integer(7)]);
        assert_eq!(arr.value(0), 7.0_f64);
    }

    #[test]
    fn col_binary_appends_blob() {
        let arr = finish_binary(&[V::Blob(vec![0xde, 0xad, 0xbe, 0xef]), V::Null]);
        assert_eq!(arr.value(0), &[0xde, 0xad, 0xbe, 0xef]);
        assert!(arr.is_null(1));
    }

    #[test]
    fn col_utf8_appends_text() {
        let arr = finish_utf8(&[V::Text("hello".to_owned()), V::Null]);
        assert_eq!(arr.value(0), "hello");
        assert!(arr.is_null(1));
    }

    #[test]
    fn col_utf8_stringifies_integer() {
        let arr = finish_utf8(&[V::Integer(99)]);
        assert_eq!(arr.value(0), "99");
    }

    #[test]
    fn col_utf8_hex_encodes_blob() {
        let arr = finish_utf8(&[V::Blob(vec![0xde, 0xad])]);
        assert_eq!(arr.value(0), "dead");
    }

    // ── manifest ──────────────────────────────────────────────────────────────

    #[test]
    fn manifest_roundtrip() {
        use visidata_ext_protocol::Transport;
        let m = ExtManifest {
            name: "vd_turso".into(),
            version: "0.1.0".into(),
            extensions: vec!["turso".into(), "tdb".into()],
            schemes: vec![],
            transport: Transport::ArrowIpc,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: ExtManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "vd_turso");
        assert_eq!(back.extensions, ["turso", "tdb"]);
        assert_eq!(back.transport, Transport::ArrowIpc);
        assert!(back.schemes.is_empty());
    }

    #[test]
    fn extensions_do_not_conflict_with_builtins() {
        let exts = ["turso", "tdb"];
        let forbidden = ["db", "sqlite", "sqlite3", "ddb", "duckdb"];
        for ext in exts {
            assert!(
                !forbidden.contains(&ext),
                "extension .{ext} conflicts with a built-in loader"
            );
        }
    }
}
