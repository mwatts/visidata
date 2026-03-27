//! TOML file loader.
//!
//! - Top-level array of tables → each element is a row.
//! - Top-level table of tables → each sub-table is a row, keyed by name.
//! - Flat top-level table → single row.

use std::path::Path;

use anyhow::{Context, Result};
use visidata_core::{Sheet, Value};

use crate::registry::Loader;

#[derive(Debug)]
pub struct TomlLoader;

impl Loader for TomlLoader {
    fn extensions(&self) -> &[&str] {
        &["toml"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        load_toml_from_str(&content, path.display().to_string())
    }
}

/// Parse TOML content into a Sheet.
///
/// # Errors
///
/// Returns an error if the TOML is invalid.
pub fn load_toml_from_str(content: &str, name: impl Into<String>) -> Result<Sheet> {
    let value: toml::Value = content.parse().context("invalid TOML")?;
    let name = name.into();

    match value {
        toml::Value::Array(arr) => Ok(load_array_of_tables(&arr, &name)),
        toml::Value::Table(tbl) => Ok(load_table(&tbl, &name)),
        other => {
            // Scalar root — single row with one column "value"
            let mut sheet = Sheet::new(name);
            sheet.add_column("value", 0);
            sheet.add_row(vec![toml_to_value(&other)]);
            Ok(sheet)
        }
    }
}

fn load_array_of_tables(arr: &[toml::Value], name: &str) -> Sheet {
    // Collect all keys across all elements
    let mut all_keys: Vec<String> = Vec::new();
    for item in arr {
        if let toml::Value::Table(tbl) = item {
            for k in tbl.keys() {
                if !all_keys.contains(k) {
                    all_keys.push(k.clone());
                }
            }
        }
    }

    let mut sheet = Sheet::new(name);
    for (i, key) in all_keys.iter().enumerate() {
        sheet.add_column(key.as_str(), i);
    }
    for item in arr {
        if let toml::Value::Table(tbl) = item {
            let vals: Vec<Value> = all_keys.iter()
                .map(|k| tbl.get(k).map_or(Value::Null, toml_to_value))
                .collect();
            sheet.add_row(vals);
        }
    }
    sheet
}

fn load_table(tbl: &toml::map::Map<String, toml::Value>, name: &str) -> Sheet {
    // If all values are tables → table-of-tables
    if tbl.values().all(|v| matches!(v, toml::Value::Table(_))) {
        let mut all_keys: Vec<String> = vec!["name".into()];
        for sub in tbl.values() {
            if let toml::Value::Table(sub_tbl) = sub {
                for k in sub_tbl.keys() {
                    if !all_keys.contains(k) {
                        all_keys.push(k.clone());
                    }
                }
            }
        }
        let mut sheet = Sheet::new(name);
        for (i, key) in all_keys.iter().enumerate() {
            sheet.add_column(key.as_str(), i);
        }
        for (row_name, sub) in tbl {
            if let toml::Value::Table(sub_tbl) = sub {
                let mut vals: Vec<Value> = vec![Value::Text(row_name.clone())];
                for key in all_keys.iter().skip(1) {
                    vals.push(sub_tbl.get(key).map_or(Value::Null, toml_to_value));
                }
                sheet.add_row(vals);
            }
        }
        return sheet;
    }

    // Flat table → column per key, single row
    let keys: Vec<String> = tbl.keys().cloned().collect();
    let mut sheet = Sheet::new(name);
    for (i, key) in keys.iter().enumerate() {
        sheet.add_column(key.as_str(), i);
    }
    let vals: Vec<Value> = keys.iter()
        .map(|k| tbl.get(k).map_or(Value::Null, toml_to_value))
        .collect();
    sheet.add_row(vals);
    sheet
}

fn toml_to_value(v: &toml::Value) -> Value {
    match v {
        toml::Value::String(s)   => Value::Text(s.clone()),
        toml::Value::Integer(n)  => Value::Int(*n),
        toml::Value::Float(f)    => Value::Float(*f),
        toml::Value::Boolean(b)  => Value::Bool(*b),
        toml::Value::Datetime(d) => Value::Text(d.to_string()),
        toml::Value::Array(_) | toml::Value::Table(_) => {
            Value::Text(toml::to_string(v).unwrap_or_default())
        }
    }
}
