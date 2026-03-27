//! Loader trait and registry for dispatching file loads by extension.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use visidata_core::Sheet;

use crate::ext_loader::{ExtDrill, ExtLoader, ExtLoaderRegistry};

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
///
/// Built-in loaders are checked first. External loaders (discovered via
/// [`crate::ext_discovery::discover`]) fill in any extensions not covered
/// by built-ins.
#[derive(Default)]
pub struct LoaderRegistry {
    /// Built-in loaders, checked in registration order.
    loaders: Vec<Box<dyn Loader>>,
    /// External loaders discovered from `$PATH`.
    ext: ExtLoaderRegistry,
}

impl LoaderRegistry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a registry with all built-in loaders and any discovered
    /// external loaders registered.
    ///
    /// Built-ins always take priority over external loaders for the same
    /// file extension.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();

        // Register built-ins first — they have priority.
        registry.register(Box::new(super::CsvLoader));
        registry.register(Box::new(super::JsonLoader));
        registry.register(Box::new(super::YamlLoader));
        registry.register(Box::new(super::SqliteLoader));
        registry.register(Box::new(super::ExcelLoader));
        registry.register(Box::new(super::ParquetLoader));
        registry.register(Box::new(super::HtmlLoader));
        registry.register(Box::new(super::FixedWidthLoader));
        registry.register(Box::new(super::TomlLoader));
        registry.register(Box::new(super::ArrowLoader));

        // Collect built-in extensions so external loaders can't shadow them.
        let builtin_exts: Vec<String> = registry
            .loaders
            .iter()
            .flat_map(|l| l.extensions().iter().map(|e| (*e).to_owned()))
            .collect();
        let builtin_ext_strs: Vec<&str> = builtin_exts.iter().map(String::as_str).collect();

        // Discover and register external loaders.
        for ext_loader in crate::ext_discovery::discover() {
            registry.ext.register(ext_loader, &builtin_ext_strs);
        }

        registry
    }

    /// Register a built-in loader.
    pub fn register(&mut self, loader: Box<dyn Loader>) {
        self.loaders.push(loader);
    }

    /// Find a built-in loader for the given file extension.
    #[must_use]
    pub fn find_loader(&self, extension: &str) -> Option<&dyn Loader> {
        let ext_lower = extension.to_lowercase();
        self.loaders
            .iter()
            .find(|l| l.extensions().iter().any(|e| *e == ext_lower))
            .map(AsRef::as_ref)
    }

    /// Find an external loader for the given file extension.
    #[must_use]
    pub fn find_ext_loader(&self, extension: &str) -> Option<Arc<ExtLoader>> {
        self.ext.find(extension)
    }

    /// Load a file, checking built-ins then external loaders.
    ///
    /// # Errors
    ///
    /// Returns an error if no loader matches the extension or loading fails.
    pub fn load_file(&self, path: &Path) -> Result<Sheet> {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

        // Built-ins first.
        if let Some(loader) = self.find_loader(ext) {
            return loader.load(path);
        }

        // External loaders second.
        if let Some(ext_loader) = self.find_ext_loader(ext) {
            let mut sheet = ext_loader.load(path)?;
            sheet.drill = Some(Arc::new(ExtDrill {
                loader: Arc::clone(&ext_loader),
                db_path: path.to_path_buf(),
            }));
            return Ok(sheet);
        }

        anyhow::bail!("no loader for extension: .{ext}")
    }

    /// Returns the number of registered external loaders.
    #[must_use]
    pub fn num_ext_loaders(&self) -> usize {
        self.ext.len()
    }
}

impl std::fmt::Debug for LoaderRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoaderRegistry")
            .field("num_builtins", &self.loaders.len())
            .field("num_ext", &self.ext.len())
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
        assert!(registry.find_loader("yml").is_some());
        assert!(registry.find_loader("yaml").is_some());
        assert!(registry.find_loader("sqlite").is_some());
        assert!(registry.find_loader("db").is_some());
        assert!(registry.find_loader("xlsx").is_some());
        assert!(registry.find_loader("xls").is_some());
        assert!(registry.find_loader("parquet").is_some());
        assert!(registry.find_loader("html").is_some());
        assert!(registry.find_loader("htm").is_some());
        assert!(registry.find_loader("fixed").is_some());
        assert!(registry.find_loader("unknown").is_none());
    }

    #[test]
    fn registry_case_insensitive() {
        let registry = LoaderRegistry::with_builtins();
        assert!(registry.find_loader("CSV").is_some());
        assert!(registry.find_loader("Json").is_some());
        assert!(registry.find_loader("XLSX").is_some());
        assert!(registry.find_loader("PARQUET").is_some());
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
