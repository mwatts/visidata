//! File format savers (write sheets back to disk).

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use visidata_core::Sheet;

/// Save a sheet to the given path, auto-detecting the format from the extension.
///
/// # Errors
///
/// Returns an error if the format is unsupported or writing fails.
pub fn save_sheet(sheet: &Sheet, path: &Path) -> Result<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "csv" => save_delimited(sheet, path, b','),
        "tsv" | "tab" => save_delimited(sheet, path, b'\t'),
        "json" => save_json(sheet, path),
        _ => anyhow::bail!("no saver for extension: .{ext}"),
    }
}

/// Save a sheet as delimited text (CSV or TSV).
fn save_delimited(sheet: &Sheet, path: &Path, delimiter: u8) -> Result<()> {
    let file = std::fs::File::create(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    let mut wtr = csv::WriterBuilder::new()
        .delimiter(delimiter)
        .from_writer(file);

    // Write headers
    let visible = sheet.visible_columns();
    let headers: Vec<&str> = visible.iter().map(|c| c.name.as_str()).collect();
    wtr.write_record(&headers)
        .with_context(|| format!("failed to write headers to {}", path.display()))?;

    // Write rows
    for row in &sheet.rows {
        let fields: Vec<String> = visible.iter().map(|col| col.display_value(row)).collect();
        wtr.write_record(&fields)
            .with_context(|| format!("failed to write row to {}", path.display()))?;
    }

    wtr.flush()
        .with_context(|| format!("failed to flush {}", path.display()))?;
    Ok(())
}

/// Save a sheet as JSON (array of objects).
fn save_json(sheet: &Sheet, path: &Path) -> Result<()> {
    let visible = sheet.visible_columns();
    let mut objects = Vec::with_capacity(sheet.rows.len());

    for row in &sheet.rows {
        let mut map = serde_json::Map::new();
        for col in &visible {
            let value = col.typed_value(row);
            let json_val = match value {
                visidata_core::Value::Null => serde_json::Value::Null,
                visidata_core::Value::Int(n) => serde_json::Value::Number(n.into()),
                visidata_core::Value::Float(f) => serde_json::Number::from_f64(f)
                    .map_or(serde_json::Value::Null, serde_json::Value::Number),
                visidata_core::Value::Bool(b) => serde_json::Value::Bool(b),
                other => serde_json::Value::String(other.to_string()),
            };
            map.insert(col.name.clone(), json_val);
        }
        objects.push(serde_json::Value::Object(map));
    }

    let json = serde_json::to_string_pretty(&objects).context("failed to serialize JSON")?;
    let mut file = std::fs::File::create(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(json.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use visidata_core::{Column, ColumnId, Row, Value};

    use crate::registry::Loader;

    fn sample_sheet() -> Sheet {
        let columns = vec![
            Column::new(ColumnId(0), "name", 0),
            Column::new(ColumnId(1), "age", 1),
        ];
        let rows = vec![
            Row::new(vec![Value::Text("Alice".into()), Value::Int(30)]),
            Row::new(vec![Value::Text("Bob".into()), Value::Int(25)]),
        ];
        Sheet::with_data("test", columns, rows)
    }

    #[test]
    fn save_and_reload_csv() {
        let sheet = sample_sheet();
        let dir = std::env::temp_dir();
        let path = dir.join("vd_test_save.csv");

        save_sheet(&sheet, &path).unwrap();

        // Reload and verify
        let reloaded = crate::CsvLoader.load(&path).unwrap();
        assert_eq!(reloaded.num_rows(), 2);
        assert_eq!(reloaded.num_cols(), 2);
        assert_eq!(reloaded.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(reloaded.get_cell(1, 1), Value::Text("25".into())); // CSV loads as text

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_and_reload_tsv() {
        let sheet = sample_sheet();
        let dir = std::env::temp_dir();
        let path = dir.join("vd_test_save.tsv");

        save_sheet(&sheet, &path).unwrap();

        let reloaded = crate::CsvLoader.load(&path).unwrap();
        assert_eq!(reloaded.num_rows(), 2);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_and_reload_json() {
        let sheet = sample_sheet();
        let dir = std::env::temp_dir();
        let path = dir.join("vd_test_save.json");

        save_sheet(&sheet, &path).unwrap();

        let reloaded = crate::JsonLoader.load(&path).unwrap();
        assert_eq!(reloaded.num_rows(), 2);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn save_unsupported_format() {
        let sheet = sample_sheet();
        let result = save_sheet(&sheet, &PathBuf::from("/tmp/test.xyz"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no saver"));
    }
}
