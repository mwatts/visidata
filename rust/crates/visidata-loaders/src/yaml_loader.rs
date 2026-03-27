//! YAML file loader.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use visidata_core::{Column, ColumnId, Row, Sheet, Value};

use crate::registry::Loader;

/// Loader for YAML files.
///
/// Supports `.yml` and `.yaml` extensions. Expects either:
/// - A top-level sequence of mappings (rows).
/// - A single mapping (one-row sheet).
#[derive(Debug)]
pub struct YamlLoader;

impl Loader for YamlLoader {
    fn extensions(&self) -> &[&str] {
        &["yml", "yaml"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_owned();

        let mut sheet = load_yaml(&name, &content)?;
        sheet.source = Some(path.to_path_buf());
        Ok(sheet)
    }
}

/// Load YAML content into a sheet.
fn load_yaml(name: &str, content: &str) -> Result<Sheet> {
    let parsed: serde_yaml::Value =
        serde_yaml::from_str(content).context("failed to parse YAML")?;

    let items = match parsed {
        serde_yaml::Value::Sequence(seq) => seq,
        _ => vec![parsed],
    };

    Ok(mappings_to_sheet(name, &items))
}

/// Convert a list of YAML values (expected mappings) to a `Sheet`.
fn mappings_to_sheet(name: &str, items: &[serde_yaml::Value]) -> Sheet {
    // Collect all keys across all mappings in sorted order.
    let mut all_keys = BTreeSet::new();
    for item in items {
        if let serde_yaml::Value::Mapping(map) = item {
            for key in map.keys() {
                if let Some(s) = key.as_str() {
                    all_keys.insert(s.to_owned());
                }
            }
        }
    }

    let key_list: Vec<String> = all_keys.into_iter().collect();

    let columns: Vec<Column> = key_list
        .iter()
        .enumerate()
        .map(|(i, key)| Column::new(ColumnId(i), key.as_str(), i))
        .collect();

    let rows: Vec<Row> = items
        .iter()
        .map(|item| {
            let values: Vec<Value> = key_list
                .iter()
                .map(|key| {
                    if let serde_yaml::Value::Mapping(map) = item {
                        map.get(serde_yaml::Value::String(key.clone()))
                            .map_or(Value::Null, yaml_value_to_value)
                    } else {
                        // Non-mapping item: stringify into first column.
                        Value::Text(format!("{item:?}"))
                    }
                })
                .collect();
            Row::new(values)
        })
        .collect();

    Sheet::with_data(name, columns, rows)
}

/// Convert a `serde_yaml::Value` to a `visidata_core::Value`.
fn yaml_value_to_value(v: &serde_yaml::Value) -> Value {
    match v {
        serde_yaml::Value::Null => Value::Null,
        serde_yaml::Value::Bool(b) => Value::Bool(*b),
        serde_yaml::Value::Number(n) => n.as_i64().map_or_else(
            || {
                n.as_f64()
                    .map_or_else(|| Value::Text(n.to_string()), Value::Float)
            },
            Value::Int,
        ),
        serde_yaml::Value::String(s) => Value::Text(s.clone()),
        // Nested sequences/mappings → serialize as YAML string.
        serde_yaml::Value::Sequence(_) | serde_yaml::Value::Mapping(_) => serde_yaml::to_string(v)
            .map_or_else(
                |_| Value::Text(format!("{v:?}")),
                |s| Value::Text(s.trim().to_owned()),
            ),
        serde_yaml::Value::Tagged(tagged) => yaml_value_to_value(&tagged.value),
    }
}

/// Load YAML from a string (for testing).
///
/// # Errors
///
/// Returns an error if the content is not valid YAML.
pub fn load_yaml_from_str(name: &str, content: &str) -> Result<Sheet> {
    load_yaml(name, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
    }

    #[test]
    fn load_simple_yaml_file() {
        let path = fixtures_dir().join("simple.yml");
        let sheet = YamlLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "simple");
        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.num_rows(), 3);

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(col_names, vec!["age", "city", "name"]);

        let name_idx = col_names.iter().position(|&n| n == "name").unwrap();
        assert_eq!(sheet.get_cell(0, name_idx), Value::Text("Alice".into()));
    }

    #[test]
    fn yaml_types() {
        let data = r#"
- s: text
  i: 42
  f: 3.14
  b: true
  n: null
"#;
        let sheet = load_yaml_from_str("test", data).unwrap();
        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        let get = |name: &str| {
            let idx = col_names.iter().position(|&n| n == name).unwrap();
            sheet.get_cell(0, idx)
        };

        assert_eq!(get("s"), Value::Text("text".into()));
        assert_eq!(get("i"), Value::Int(42));
        assert_eq!(get("f"), Value::Float(3.14));
        assert_eq!(get("b"), Value::Bool(true));
        assert_eq!(get("n"), Value::Null);
    }

    #[test]
    fn yaml_missing_keys() {
        let data = "- a: 1\n  b: 2\n- b: 3\n  c: 4\n";
        let sheet = load_yaml_from_str("test", data).unwrap();

        assert_eq!(sheet.num_cols(), 3);
        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        let get = |row: usize, name: &str| {
            let idx = col_names.iter().position(|&n| n == name).unwrap();
            sheet.get_cell(row, idx)
        };

        assert_eq!(get(0, "a"), Value::Int(1));
        assert_eq!(get(0, "c"), Value::Null);
        assert_eq!(get(1, "a"), Value::Null);
        assert_eq!(get(1, "c"), Value::Int(4));
    }

    #[test]
    fn yaml_single_mapping() {
        let data = "name: Alice\nage: 30\n";
        let sheet = load_yaml_from_str("test", data).unwrap();

        assert_eq!(sheet.num_rows(), 1);
        assert_eq!(sheet.num_cols(), 2);
    }

    #[test]
    fn yaml_empty_sequence() {
        let sheet = load_yaml_from_str("test", "[]").unwrap();
        assert_eq!(sheet.num_rows(), 0);
        assert_eq!(sheet.num_cols(), 0);
    }

    #[test]
    fn yaml_nested_values() {
        let data = "- name: Alice\n  tags:\n    - a\n    - b\n";
        let sheet = load_yaml_from_str("test", data).unwrap();

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        let tags_idx = col_names.iter().position(|&n| n == "tags").unwrap();
        if let Value::Text(s) = sheet.get_cell(0, tags_idx) {
            assert!(s.contains('a'));
            assert!(s.contains('b'));
        } else {
            panic!("expected Text for nested sequence");
        }
    }

    #[test]
    fn invalid_yaml() {
        let result = load_yaml_from_str("test", "{{invalid");
        assert!(result.is_err());
    }

    #[test]
    fn nonexistent_yaml_file() {
        let result = YamlLoader.load(Path::new("/nonexistent/file.yml"));
        assert!(result.is_err());
    }
}
