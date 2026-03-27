//! JSON and JSONL file loader.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value as JsonValue;
use visidata_core::{Column, ColumnId, Row, Sheet, Value};

use crate::registry::Loader;

/// Loader for JSON and JSONL (JSON Lines) files.
///
/// - JSON: expects a top-level array of objects.
/// - JSONL: one JSON object per line (skips comment lines starting with `#`).
///
/// Nested objects/arrays are serialized as JSON strings.
#[derive(Debug)]
pub struct JsonLoader;

impl Loader for JsonLoader {
    fn extensions(&self) -> &[&str] {
        &["json", "jsonl"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_owned();

        let mut sheet = match ext.as_str() {
            "jsonl" => load_jsonl(&name, &content)?,
            _ => load_json(&name, &content)?,
        };

        sheet.source = Some(path.to_path_buf());
        Ok(sheet)
    }
}

/// Load a JSON array of objects.
fn load_json(name: &str, content: &str) -> Result<Sheet> {
    let parsed: JsonValue = serde_json::from_str(content).context("failed to parse JSON")?;

    match parsed {
        // Array of objects → standard tabular sheet
        JsonValue::Array(ref a) if a.iter().all(serde_json::Value::is_object) => {
            Ok(objects_to_sheet(name, a))
        }
        // Array of arrays → sequence sheet (row 0 = headers if all strings, else row indices)
        JsonValue::Array(ref a) if a.iter().all(serde_json::Value::is_array) => {
            Ok(array_of_arrays_to_sheet(name, a))
        }
        // Array of mixed or scalar → one column "value"
        JsonValue::Array(ref a) => {
            let columns = vec![Column::new(ColumnId(0), "value", 0)];
            let rows: Vec<Row> = a.iter()
                .map(|v| Row::new(vec![json_value_to_value(v)]))
                .collect();
            Ok(Sheet::with_data(name, columns, rows))
        }
        // Single object → one row
        JsonValue::Object(_) => Ok(objects_to_sheet(name, &[parsed])),
        // Scalar → single cell
        other => {
            let columns = vec![Column::new(ColumnId(0), "value", 0)];
            Ok(Sheet::with_data(name, columns, vec![Row::new(vec![json_value_to_value(&other)])]))
        }
    }
}

/// Load JSONL (one object per line, skipping `#` comments and blank lines).
fn load_jsonl(name: &str, content: &str) -> Result<Sheet> {
    let objects: Vec<JsonValue> = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line)
                .with_context(|| format!("failed to parse JSONL at line {}", i + 1))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(objects_to_sheet(name, &objects))
}

/// Convert a list of JSON objects to a Sheet.
///
/// Column order is determined by collecting all keys across all objects
/// (using `BTreeSet` for stable ordering).
fn objects_to_sheet(name: &str, objects: &[JsonValue]) -> Sheet {
    // Collect all unique keys in sorted order
    let mut all_keys = BTreeSet::new();
    for obj in objects {
        if let JsonValue::Object(map) = obj {
            for key in map.keys() {
                all_keys.insert(key.clone());
            }
        }
    }

    let key_list: Vec<String> = all_keys.into_iter().collect();

    let columns: Vec<Column> = key_list
        .iter()
        .enumerate()
        .map(|(i, key)| Column::new(ColumnId(i), key.as_str(), i))
        .collect();

    let rows: Vec<Row> = objects
        .iter()
        .map(|obj| {
            let values: Vec<Value> = key_list
                .iter()
                .map(|key| {
                    if let JsonValue::Object(map) = obj {
                        map.get(key).map_or(Value::Null, json_value_to_value)
                    } else {
                        // Non-object row: put the whole value in the first column
                        Value::Text(obj.to_string())
                    }
                })
                .collect();
            Row::new(values)
        })
        .collect();

    Sheet::with_data(name, columns, rows)
}

/// Convert an array-of-arrays to a Sheet.
///
/// If the first row is all strings it is treated as column headers;
/// otherwise column indices (0, 1, 2, …) are used.
fn array_of_arrays_to_sheet(name: &str, arr: &[JsonValue]) -> Sheet {
    if arr.is_empty() {
        return Sheet::new(name);
    }
    let first = arr[0].as_array().map(Vec::as_slice).unwrap_or_default();
    let (headers, data): (Vec<String>, &[JsonValue]) =
        if first.iter().all(serde_json::Value::is_string) {
            let h = first.iter().map(|v| v.as_str().unwrap_or("").to_owned()).collect();
            (h, &arr[1..])
        } else {
            let h = (0..first.len()).map(|i| i.to_string()).collect();
            (h, arr)
        };
    let columns: Vec<Column> = headers.iter().enumerate()
        .map(|(i, h)| Column::new(ColumnId(i), h.as_str(), i))
        .collect();
    let rows: Vec<Row> = data.iter().map(|row| {
        let vals: Vec<Value> = row.as_array()
            .map(|a| a.iter().map(json_value_to_value).collect())
            .unwrap_or_default();
        Row::new(vals)
    }).collect();
    Sheet::with_data(name, columns, rows)
}

/// Convert a `serde_json::Value` to a `visidata_core::Value`.
fn json_value_to_value(v: &JsonValue) -> Value {
    match v {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .or_else(|| n.as_f64().map(Value::Float))
            .unwrap_or_else(|| Value::Text(n.to_string())),
        JsonValue::String(s) => Value::Text(s.clone()),
        // Nested objects/arrays → serialize as JSON string
        JsonValue::Array(_) | JsonValue::Object(_) => Value::Text(v.to_string()),
    }
}

/// Load a JSON string into a sheet (for testing and stdin).
///
/// # Errors
///
/// Returns an error if the string is not valid JSON.
pub fn load_json_from_str(name: &str, content: &str) -> Result<Sheet> {
    load_json(name, content)
}

/// Load a JSONL string into a sheet (for testing and stdin).
///
/// # Errors
///
/// Returns an error if any line is not valid JSON.
pub fn load_jsonl_from_str(name: &str, content: &str) -> Result<Sheet> {
    load_jsonl(name, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
    }

    #[test]
    fn load_simple_json_file() {
        let path = fixtures_dir().join("simple.json");
        let sheet = JsonLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "simple");
        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.num_rows(), 3);

        // Columns sorted alphabetically (BTreeSet)
        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(col_names, vec!["age", "city", "name"]);

        // Find "name" column index
        let name_idx = col_names.iter().position(|&n| n == "name").unwrap();
        assert_eq!(sheet.get_cell(0, name_idx), Value::Text("Alice".into()));
    }

    #[test]
    fn load_jsonl_file() {
        let path = fixtures_dir().join("objects.jsonl");
        let sheet = JsonLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "objects");
        assert_eq!(sheet.num_rows(), 3);
        assert_eq!(sheet.num_cols(), 2);

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(col_names.contains(&"name"));
        assert!(col_names.contains(&"score"));
    }

    #[test]
    fn load_nested_json() {
        let path = fixtures_dir().join("nested.json");
        let sheet = JsonLoader.load(&path).unwrap();

        assert_eq!(sheet.num_rows(), 2);
        // "address" and "tags" should be serialized as JSON strings
        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(col_names.contains(&"address"));
        assert!(col_names.contains(&"tags"));

        // Find "address" column
        let addr_idx = col_names.iter().position(|&n| n == "address").unwrap();
        let addr = sheet.get_cell(0, addr_idx);
        if let Value::Text(s) = addr {
            assert!(s.contains("NYC"));
            assert!(s.contains("10001"));
        } else {
            panic!("expected Text for nested object, got {addr:?}");
        }
    }

    #[test]
    fn json_value_types() {
        let data = r#"[
            {"s": "text", "i": 42, "f": 3.14, "b": true, "n": null}
        ]"#;
        let sheet = load_json_from_str("test", data).unwrap();

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
    fn json_missing_keys() {
        let data = r#"[{"a": 1, "b": 2}, {"b": 3, "c": 4}]"#;
        let sheet = load_json_from_str("test", data).unwrap();

        // All three keys present as columns
        assert_eq!(sheet.num_cols(), 3);

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        let get = |row: usize, name: &str| {
            let idx = col_names.iter().position(|&n| n == name).unwrap();
            sheet.get_cell(row, idx)
        };

        // First row has a, b but not c
        assert_eq!(get(0, "a"), Value::Int(1));
        assert_eq!(get(0, "c"), Value::Null);

        // Second row has b, c but not a
        assert_eq!(get(1, "a"), Value::Null);
        assert_eq!(get(1, "c"), Value::Int(4));
    }

    #[test]
    fn jsonl_skips_comments_and_blanks() {
        let data = "# comment\n{\"x\": 1}\n\n{\"x\": 2}\n# another comment\n";
        let sheet = load_jsonl_from_str("test", data).unwrap();

        assert_eq!(sheet.num_rows(), 2);
        assert_eq!(sheet.get_cell(0, 0), Value::Int(1));
        assert_eq!(sheet.get_cell(1, 0), Value::Int(2));
    }

    #[test]
    fn json_empty_array() {
        let sheet = load_json_from_str("test", "[]").unwrap();
        assert_eq!(sheet.num_rows(), 0);
        assert_eq!(sheet.num_cols(), 0);
    }

    #[test]
    fn json_non_array_wrapped() {
        let sheet = load_json_from_str("test", r#"{"a": 1}"#).unwrap();
        assert_eq!(sheet.num_rows(), 1);
        assert_eq!(sheet.get_cell(0, 0), Value::Int(1));
    }

    #[test]
    fn nonexistent_json_file() {
        let result = JsonLoader.load(Path::new("/nonexistent/file.json"));
        assert!(result.is_err());
    }

    #[test]
    fn invalid_json() {
        let result = load_json_from_str("test", "not json at all");
        assert!(result.is_err());
    }
}
