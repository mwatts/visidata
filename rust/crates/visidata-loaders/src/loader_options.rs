//! Snapshot of options needed by loaders at load time.
//!
//! `App` extracts values from `OptionsManager` into this struct before
//! calling `LoaderRegistry::load_file_with_options`.  Loaders receive only
//! what they need — no reference to `OptionsManager` crosses the boundary.

/// Options snapshot passed to loaders.
///
/// All fields have defaults that match the previous hardcoded behaviour, so
/// callers that don't need custom options can use `LoaderOptions::default()`.
#[derive(Debug, Clone)]
pub struct LoaderOptions {
    /// CSV field delimiter byte (default: `,`).
    pub csv_delimiter: u8,
    /// CSV quote character (default: `"`).
    pub csv_quote_char: u8,
    /// Options for external loaders: arbitrary key-value pairs prefixed by
    /// loader name (e.g. `"vd_duckdb_batch_size"`).
    pub ext_options: std::collections::HashMap<String, String>,
}

impl Default for LoaderOptions {
    fn default() -> Self {
        Self {
            csv_delimiter: b',',
            csv_quote_char: b'"',
            ext_options: std::collections::HashMap::new(),
        }
    }
}
