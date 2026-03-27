//! External loader — runs a `vd_*` subprocess and reads Arrow IPC from stdout.

use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow::ipc::reader::StreamReader;
use visidata_core::{Column, ColumnId, DrillAction, Row, Sheet, Value};
use visidata_ext_protocol::{ExtManifest, LoadRequest, Transport};

use crate::arrow_util::arrow_value_at;
use crate::registry::Loader;

/// An external loader that communicates via the Arrow IPC subprocess protocol.
///
/// Spawns a `vd_*` binary, sends a [`LoadRequest`] on stdin, and reads an
/// Arrow IPC stream from stdout. See `docs/ext-loaders.md` for the full
/// protocol specification.
#[derive(Debug)]
pub struct ExtLoader {
    /// Absolute path to the loader binary.
    binary: std::path::PathBuf,
    /// Manifest received at discovery time.
    pub manifest: ExtManifest,
    /// Cached list of `&str` for the `Loader::extensions` return value.
    ext_strs: Vec<String>,
}

impl ExtLoader {
    /// Create an `ExtLoader` from a binary path and its manifest.
    #[must_use]
    pub fn new(binary: std::path::PathBuf, manifest: ExtManifest) -> Self {
        let ext_strs = manifest.extensions.clone();
        Self {
            binary,
            manifest,
            ext_strs,
        }
    }

    /// Run a load request against the external binary.
    ///
    /// `query = None`  → table-index sheet (list of tables/views).
    /// `query = Some(sql)` → execute `sql` and return the results as a sheet.
    ///
    /// # Errors
    ///
    /// Returns an error if the subprocess fails to spawn, the Arrow IPC stream
    /// is malformed, or the subprocess exits non-zero.
    pub fn run_query(
        &self,
        path: &Path,
        query: Option<&str>,
        options: HashMap<String, serde_json::Value>,
    ) -> Result<Sheet> {
        let mut child = Command::new(&self.binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("failed to spawn {}", self.binary.display()))?;

        // Write the request and close stdin to signal start.
        let req = LoadRequest {
            path: path.to_string_lossy().into_owned(),
            query: query.map(str::to_owned),
            options,
        };
        let req_json = serde_json::to_string(&req).context("failed to serialise LoadRequest")?;
        {
            let mut stdin = child.stdin.take().context("no stdin handle")?;
            writeln!(stdin, "{req_json}").context("failed to write request to subprocess")?;
            // Dropping `stdin` here closes the pipe → extension sees EOF → starts processing.
        }

        // Read response via declared transport.
        let stdout = child.stdout.take().context("no stdout handle")?;
        let (columns, rows) = match self.manifest.transport {
            Transport::ArrowIpc => read_arrow_ipc(stdout)?,
            Transport::Ndjson => read_ndjson(stdout)?,
        };

        // Collect exit status + stderr.
        let output = child
            .wait_with_output()
            .context("failed to wait for external loader")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!(
                "external loader '{}' exited with {}: {stderr}",
                self.binary.display(),
                output.status
            );
        }

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_owned();

        let mut sheet = Sheet::with_data(name, columns, rows);
        sheet.source = Some(path.to_path_buf());
        Ok(sheet)
    }
}

/// Read an Arrow IPC stream from a reader and return (columns, rows).
fn read_arrow_ipc(reader: impl std::io::Read) -> Result<(Vec<Column>, Vec<Row>)> {
    let stream = StreamReader::try_new(reader, None)
        .context("failed to open Arrow IPC stream from external loader")?;

    let schema = stream.schema();
    let columns: Vec<Column> = schema
        .fields()
        .iter()
        .enumerate()
        .map(|(i, f)| Column::new(ColumnId(i), f.name().as_str(), i))
        .collect();
    let num_cols = columns.len();

    let mut rows = Vec::new();
    for batch_result in stream {
        let batch: arrow::record_batch::RecordBatch =
            batch_result.context("error reading Arrow batch from external loader")?;
        for row_idx in 0..batch.num_rows() {
            let values: Vec<Value> = (0..num_cols)
                .map(|ci| arrow_value_at(batch.column(ci), row_idx))
                .collect();
            rows.push(Row::new(values));
        }
    }
    Ok((columns, rows))
}

/// Read newline-delimited JSON from a reader and return (columns, rows).
///
/// Columns are derived from the union of all keys across all objects,
/// in stable sorted order (matching the JSON loader's behaviour).
fn read_ndjson(reader: impl std::io::Read) -> Result<(Vec<Column>, Vec<Row>)> {
    use std::io::BufReader;
    let buf = BufReader::new(reader);
    let mut objects: Vec<serde_json::Value> = Vec::new();
    for line in buf.lines() {
        let line = line.context("error reading NDJSON line from external loader")?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let obj: serde_json::Value =
            serde_json::from_str(line).context("failed to parse NDJSON line")?;
        objects.push(obj);
    }

    // Collect all keys in sorted order.
    let mut all_keys = BTreeSet::new();
    for obj in &objects {
        if let serde_json::Value::Object(map) = obj {
            for key in map.keys() {
                all_keys.insert(key.clone());
            }
        }
    }
    let key_list: Vec<String> = all_keys.into_iter().collect();

    let columns: Vec<Column> = key_list
        .iter()
        .enumerate()
        .map(|(i, k)| Column::new(ColumnId(i), k.as_str(), i))
        .collect();

    let rows: Vec<Row> = objects
        .iter()
        .map(|obj| {
            let values: Vec<Value> = key_list
                .iter()
                .map(|key| {
                    if let serde_json::Value::Object(map) = obj {
                        map.get(key).map_or(Value::Null, json_to_value)
                    } else {
                        Value::Text(obj.to_string())
                    }
                })
                .collect();
            Row::new(values)
        })
        .collect();

    Ok((columns, rows))
}

/// Convert a `serde_json::Value` to a `visidata_core::Value`.
fn json_to_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .or_else(|| n.as_f64().map(Value::Float))
            .unwrap_or_else(|| Value::Text(n.to_string())),
        serde_json::Value::String(s) => Value::Text(s.clone()),
        v => Value::Text(v.to_string()),
    }
}

impl Loader for ExtLoader {
    fn extensions(&self) -> &[&str] {
        // Safety: ext_strs lives as long as self; we need &[&str] for the
        // trait, but the registry dispatches by extension string directly so
        // this slice is only consulted at registration time.
        let slices: Vec<&str> = self.ext_strs.iter().map(String::as_str).collect();
        // Leak is intentional — ExtLoader instances live for the process lifetime.
        Box::leak(slices.into_boxed_slice())
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        self.run_query(path, None, HashMap::new())
    }
}

/// Drill-down action for external-loader index sheets.
///
/// Pressing Enter on a row runs `SELECT * FROM "<name>"` via the same
/// external loader binary that produced the index sheet.
#[derive(Debug)]
pub struct ExtDrill {
    /// The external loader to invoke.
    pub loader: Arc<ExtLoader>,
    /// Absolute path to the database file.
    pub db_path: PathBuf,
    /// Options snapshot extracted from `OptionsManager` at drill creation time.
    ///
    /// Keys are prefixed by the loader name (e.g. `"vd_duckdb_batch_size"`).
    pub options_snapshot: HashMap<String, String>,
}

impl DrillAction for ExtDrill {
    fn open_row(&self, row: &Row) -> anyhow::Result<Sheet> {
        // Index sheets may have schema in column 0 and name in column 1
        // (e.g. vd_duckdb), or just name in column 0 (simpler loaders).
        let (schema, name) = if row.len() >= 3 {
            let schema = match row.get(0) {
                Value::Text(s) => s.clone(),
                other => anyhow::bail!("expected schema in column 0, got {other:?}"),
            };
            let name = match row.get(1) {
                Value::Text(s) => s.clone(),
                other => anyhow::bail!("expected table name in column 1, got {other:?}"),
            };
            (Some(schema), name)
        } else {
            let name = match row.get(0) {
                Value::Text(s) => s.clone(),
                other => anyhow::bail!("expected table name in column 0, got {other:?}"),
            };
            (None, name)
        };
        let sql = if let Some(schema) = &schema {
            format!("SELECT * FROM \"{schema}\".\"{name}\"")
        } else {
            format!("SELECT * FROM \"{name}\"")
        };
        let options: HashMap<String, serde_json::Value> = self.options_snapshot
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
            .collect();
        self.loader
            .run_query(&self.db_path, Some(&sql), options)
    }
}

/// Registry of discovered external loaders, keyed by file extension.
///
/// Kept separate from `LoaderRegistry` so that built-in loaders always
/// take priority and the extension machinery is clearly isolated.
#[derive(Debug, Default)]
pub struct ExtLoaderRegistry {
    /// extension (lowercase) → loader
    by_extension: HashMap<String, Arc<ExtLoader>>,
}

impl ExtLoaderRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an external loader for all its declared extensions.
    ///
    /// An extension already registered (e.g. by a built-in) is silently
    /// skipped — built-ins always win.
    pub fn register(&mut self, loader: ExtLoader, skip_extensions: &[&str]) {
        let loader = Arc::new(loader);
        for ext in &loader.manifest.extensions {
            let ext_lower = ext.to_lowercase();
            if skip_extensions.iter().any(|e| *e == ext_lower) {
                continue; // built-in has priority
            }
            self.by_extension
                .entry(ext_lower)
                .or_insert_with(|| Arc::clone(&loader));
        }
    }

    /// Find an external loader for the given extension.
    #[must_use]
    pub fn find(&self, extension: &str) -> Option<Arc<ExtLoader>> {
        self.by_extension.get(&extension.to_lowercase()).cloned()
    }

    /// Returns the number of registered external loaders.
    #[must_use]
    pub fn len(&self) -> usize {
        // Count distinct Arc pointers (one loader may handle multiple exts).
        let mut ptrs: Vec<*const ExtLoader> = self.by_extension.values().map(Arc::as_ptr).collect();
        ptrs.sort_unstable();
        ptrs.dedup();
        ptrs.len()
    }

    /// Returns `true` if no external loaders are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_extension.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_manifest(name: &str, exts: &[&str]) -> ExtManifest {
        ExtManifest {
            name: name.into(),
            version: "0.1.0".into(),
            extensions: exts.iter().map(|s| (*s).to_owned()).collect(),
            schemes: vec![],
            transport: Transport::ArrowIpc,
        }
    }

    #[test]
    fn ext_registry_register_and_find() {
        let mut reg = ExtLoaderRegistry::new();
        let loader = ExtLoader::new(
            std::path::PathBuf::from("/usr/local/bin/vd_duckdb"),
            fake_manifest("vd_duckdb", &["duckdb", "ddb"]),
        );
        reg.register(loader, &[]);

        assert!(reg.find("duckdb").is_some());
        assert!(reg.find("DuckDB").is_some()); // case-insensitive
        assert!(reg.find("ddb").is_some());
        assert!(reg.find("csv").is_none());
    }

    #[test]
    fn ext_registry_builtin_wins() {
        let mut reg = ExtLoaderRegistry::new();
        let loader = ExtLoader::new(
            std::path::PathBuf::from("/usr/local/bin/vd_csv"),
            fake_manifest("vd_csv", &["csv"]),
        );
        // Simulate "csv" already taken by a built-in.
        reg.register(loader, &["csv"]);
        assert!(reg.find("csv").is_none());
    }

    #[test]
    fn ext_registry_len() {
        let mut reg = ExtLoaderRegistry::new();
        assert_eq!(reg.len(), 0);
        assert!(reg.is_empty());

        let loader = ExtLoader::new(
            std::path::PathBuf::from("/bin/vd_duckdb"),
            fake_manifest("vd_duckdb", &["duckdb", "ddb"]),
        );
        reg.register(loader, &[]);

        // One loader, two extensions.
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
    }
}
