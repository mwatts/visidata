//! Configuration file loading (TOML format).
//!
//! Loads `~/.visidatarc.toml` with sections:
//! - `[options]` — key=value pairs applied at global level
//! - `[keybindings]` — keystroke=longname pairs

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::options::OptionsManager;
use crate::value::Value;

/// Parsed configuration file.
#[derive(Debug, Default)]
pub struct Config {
    /// Option overrides (name → value).
    pub options: HashMap<String, Value>,
    /// Keybinding overrides (keystroke → longname).
    pub keybindings: HashMap<String, String>,
}

/// Load a TOML config file, returning parsed options and keybindings.
///
/// Returns `Ok(Config::default())` if the file doesn't exist (not an error).
///
/// # Errors
///
/// Returns an error if the file exists but cannot be read or parsed.
pub fn load_config(path: &Path) -> Result<Config, String> {
    if !path.exists() {
        return Ok(Config::default());
    }

    let content =
        fs::read_to_string(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    parse_config(&content)
}

/// Parse a TOML config string.
///
/// # Errors
///
/// Returns an error if the TOML is invalid.
pub fn parse_config(content: &str) -> Result<Config, String> {
    let table: toml::Table = content.parse().map_err(|e| format!("invalid TOML: {e}"))?;

    let mut config = Config::default();

    // Parse [options] section
    if let Some(toml::Value::Table(opts)) = table.get("options") {
        for (key, val) in opts {
            config.options.insert(key.clone(), toml_to_value(val));
        }
    }

    // Parse [keybindings] section
    if let Some(toml::Value::Table(bindings)) = table.get("keybindings") {
        for (key, val) in bindings {
            if let toml::Value::String(longname) = val {
                config.keybindings.insert(key.clone(), longname.clone());
            }
        }
    }

    Ok(config)
}

/// Apply a parsed config to an options manager.
pub fn apply_config(config: &Config, opts: &mut OptionsManager) {
    for (name, value) in &config.options {
        opts.set_global(name, value.clone());
    }
}

/// Convert a TOML value to a `VisiData` `Value`.
fn toml_to_value(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s) => Value::Text(s.clone()),
        toml::Value::Integer(i) => Value::Int(*i),
        toml::Value::Float(f) => Value::Float(*f),
        toml::Value::Boolean(b) => Value::Bool(*b),
        _ => Value::Text(v.to_string()),
    }
}

/// Returns the default config file path (`~/.visidatarc.toml`).
#[must_use]
pub fn default_config_path() -> Option<std::path::PathBuf> {
    dirs_path("visidatarc.toml")
}

/// Returns the default Rhai init script path (`~/.visidatarc.rhai`).
#[must_use]
pub fn default_rhai_path() -> Option<std::path::PathBuf> {
    dirs_path("visidatarc.rhai")
}

fn dirs_path(filename: &str) -> Option<std::path::PathBuf> {
    home_dir().map(|home| home.join(format!(".{filename}")))
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_config() {
        let config = parse_config("").unwrap();
        assert!(config.options.is_empty());
        assert!(config.keybindings.is_empty());
    }

    #[test]
    fn parse_options_section() {
        let toml = r#"
[options]
encoding = "utf-8"
default_width = 30
wrap = true
disp_float_fmt = "%.4f"
"#;
        let config = parse_config(toml).unwrap();
        assert_eq!(config.options["encoding"], Value::Text("utf-8".into()));
        assert_eq!(config.options["default_width"], Value::Int(30));
        assert_eq!(config.options["wrap"], Value::Bool(true));
        assert_eq!(config.options["disp_float_fmt"], Value::Text("%.4f".into()));
    }

    #[test]
    fn parse_keybindings_section() {
        let toml = r#"
[keybindings]
"Ctrl+S" = "save-sheet"
"Ctrl+Q" = "quit-sheet"
"#;
        let config = parse_config(toml).unwrap();
        assert_eq!(config.keybindings["Ctrl+S"], "save-sheet");
        assert_eq!(config.keybindings["Ctrl+Q"], "quit-sheet");
    }

    #[test]
    fn parse_both_sections() {
        let toml = r#"
[options]
encoding = "latin1"

[keybindings]
"x" = "delete-row"
"#;
        let config = parse_config(toml).unwrap();
        assert_eq!(config.options.len(), 1);
        assert_eq!(config.keybindings.len(), 1);
    }

    #[test]
    fn apply_config_sets_globals() {
        let toml = r#"
[options]
encoding = "latin1"
default_width = 50
"#;
        let config = parse_config(toml).unwrap();
        let mut opts = crate::options::builtin_options();

        // Before: default
        assert_eq!(opts.get_global("encoding"), Value::Text("utf-8".into()));

        apply_config(&config, &mut opts);

        // After: overridden
        assert_eq!(opts.get_global("encoding"), Value::Text("latin1".into()));
        assert_eq!(opts.get_global("default_width"), Value::Int(50));
    }

    #[test]
    fn parse_invalid_toml() {
        let result = parse_config("not valid [toml");
        assert!(result.is_err());
    }

    #[test]
    fn load_nonexistent_file() {
        let config = load_config(Path::new("/nonexistent/config.toml")).unwrap();
        assert!(config.options.is_empty());
    }
}
