//! Loader trait and registry for dispatching file loads by extension.

use std::path::Path;

use anyhow::Result;
use visidata_core::Sheet;

/// Trait for file format loaders.
///
/// Each loader handles one or more file extensions and produces a `Sheet`.
pub trait Loader: Send + Sync {
    /// Returns the file extensions this loader handles (without the dot).
    fn extensions(&self) -> &[&str];

    /// Load a file at the given path into a `Sheet`.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    fn load(&self, path: &Path) -> Result<Sheet>;
}

/// Registry of loaders, dispatching by file extension.
#[derive(Default)]
pub struct LoaderRegistry {
    loaders: Vec<Box<dyn Loader>>,
}

impl LoaderRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with all built-in loaders registered.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(super::CsvLoader));
        registry.register(Box::new(super::JsonLoader));
        registry
    }

    /// Register a loader.
    pub fn register(&mut self, loader: Box<dyn Loader>) {
        self.loaders.push(loader);
    }

    /// Find a loader for the given file extension.
    #[must_use]
    pub fn find_loader(&self, extension: &str) -> Option<&dyn Loader> {
        let ext_lower = extension.to_lowercase();
        self.loaders
            .iter()
            .find(|l| l.extensions().iter().any(|e| *e == ext_lower))
            .map(AsRef::as_ref)
    }

    /// Load a file, auto-detecting the format from the extension.
    ///
    /// # Errors
    ///
    /// Returns an error if no loader matches the extension or if loading fails.
    pub fn load_file(&self, path: &Path) -> Result<Sheet> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let loader = self
            .find_loader(ext)
            .ok_or_else(|| anyhow::anyhow!("no loader for extension: .{ext}"))?;

        loader.load(path)
    }
}

impl std::fmt::Debug for LoaderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoaderRegistry")
            .field("num_loaders", &self.loaders.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_with_builtins() {
        let registry = LoaderRegistry::with_builtins();
        assert!(registry.find_loader("csv").is_some());
        assert!(registry.find_loader("tsv").is_some());
        assert!(registry.find_loader("json").is_some());
        assert!(registry.find_loader("jsonl").is_some());
        assert!(registry.find_loader("unknown").is_none());
    }

    #[test]
    fn registry_case_insensitive() {
        let registry = LoaderRegistry::with_builtins();
        assert!(registry.find_loader("CSV").is_some());
        assert!(registry.find_loader("Json").is_some());
    }

    #[test]
    fn load_file_no_loader() {
        let registry = LoaderRegistry::with_builtins();
        let result = registry.load_file(Path::new("test.xyz"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no loader"));
    }

    #[test]
    fn load_file_missing_file() {
        let registry = LoaderRegistry::with_builtins();
        let result = registry.load_file(Path::new("/nonexistent/file.csv"));
        assert!(result.is_err());
    }
}
