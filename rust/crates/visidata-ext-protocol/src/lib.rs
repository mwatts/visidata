//! Shared protocol types for `vd` external loaders.
//!
//! Both the host (`visidata-loaders`) and external loader binaries (e.g.
//! `vd_duckdb`) depend on this crate to ensure the manifest and request
//! structs stay in sync.
//!
//! ## Protocol summary
//!
//! 1. Host probes `vd_*` binaries on `$PATH` with `--manifest`.
//!    Each responds with one [`ExtManifest`] JSON line on stdout and exits 0.
//! 2. Host spawns the binary (no flags), writes one [`LoadRequest`] JSON line
//!    to stdin, then closes stdin.
//! 3. Extension writes an Arrow IPC stream to stdout.
//! 4. Errors go to stderr as plain text; exit code signals success/failure.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Response transport declared by the extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    /// Arrow IPC stream written to stdout (default).
    ///
    /// Use when the extension links the same Arrow version as the host.
    #[default]
    ArrowIpc,
    /// Newline-delimited JSON written to stdout.
    ///
    /// Use when the extension bundles its own Arrow (e.g. `DuckDB`) to avoid
    /// Rust type-level incompatibility between Arrow versions.
    Ndjson,
}

/// Response to `--manifest`. One JSON line, then the process exits 0.
///
/// This is the only communication that flows from extension → host at
/// discovery time. Everything else uses the declared transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtManifest {
    /// Unique name of this loader (matches the binary name).
    pub name: String,

    /// Semver version string of the loader binary.
    pub version: String,

    /// File extensions this loader handles, without leading dot.
    ///
    /// Example: `["duckdb", "ddb"]`
    pub extensions: Vec<String>,

    /// URI schemes this loader handles, with trailing `://`.
    ///
    /// Reserved for future network/remote support. Leave empty for local
    /// file loaders.
    #[serde(default)]
    pub schemes: Vec<String>,

    /// Response transport (default: `arrow-ipc`).
    ///
    /// Extensions that bundle their own Arrow should use `ndjson` to avoid
    /// Rust type-level conflicts with the host's Arrow version.
    #[serde(default)]
    pub transport: Transport,
}

/// Sent to the extension on stdin as a single JSON line.
///
/// Stdin is closed immediately after writing this line. The extension MUST
/// treat stdin EOF as the signal to begin processing (and as a cancellation
/// signal if it arrives during batch output).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadRequest {
    /// Absolute path to the file being opened.
    pub path: String,

    /// SQL query to execute, or `null` for the default table-index view.
    ///
    /// When `null`, the extension should return a sheet listing available
    /// tables/views so the user can navigate into one.
    pub query: Option<String>,

    /// Loader-specific options passed from the host.
    ///
    /// Keys and semantics are defined by each extension. The host passes
    /// through whatever the user has set via `--option` or the options sheet.
    #[serde(default)]
    pub options: HashMap<String, serde_json::Value>,
}

/// Convenience: build a table-index request (no query).
impl LoadRequest {
    /// Create a request to load the table index for the given file.
    #[must_use]
    pub fn index(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            query: None,
            options: HashMap::new(),
        }
    }

    /// Create a request to run a specific SQL query against the given file.
    #[must_use]
    pub fn query(path: impl Into<String>, sql: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            query: Some(sql.into()),
            options: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_roundtrip() {
        let m = ExtManifest {
            name: "vd_duckdb".into(),
            version: "0.1.0".into(),
            extensions: vec!["duckdb".into(), "ddb".into()],
            schemes: vec!["duckdb://".into()],
            transport: Transport::Ndjson,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: ExtManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "vd_duckdb");
        assert_eq!(back.extensions, vec!["duckdb", "ddb"]);
        assert_eq!(back.transport, Transport::Ndjson);
    }

    #[test]
    fn manifest_default_transport_is_arrow_ipc() {
        let json = r#"{"name":"x","version":"0.1.0","extensions":["x"]}"#;
        let m: ExtManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.transport, Transport::ArrowIpc);
    }

    #[test]
    fn request_index() {
        let r = LoadRequest::index("/tmp/sales.duckdb");
        assert_eq!(r.path, "/tmp/sales.duckdb");
        assert!(r.query.is_none());
        assert!(r.options.is_empty());
    }

    #[test]
    fn request_query() {
        let r = LoadRequest::query("/tmp/sales.duckdb", "SELECT * FROM orders");
        assert_eq!(r.query.as_deref(), Some("SELECT * FROM orders"));
    }

    #[test]
    fn request_roundtrip() {
        let r = LoadRequest::query("/tmp/x.duckdb", "SELECT 1");
        let json = serde_json::to_string(&r).unwrap();
        let back: LoadRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.query.as_deref(), Some("SELECT 1"));
    }

    #[test]
    fn manifest_schemes_defaults_empty() {
        let json = r#"{"name":"x","version":"0.1.0","extensions":["x"]}"#;
        let m: ExtManifest = serde_json::from_str(json).unwrap();
        assert!(m.schemes.is_empty());
    }
}
