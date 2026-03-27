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
        "yaml" | "yml" => save_yaml(sheet, path),
        "htm" | "html" => save_html(sheet, path),
        "parquet" => save_parquet(sheet, path),
        "sqlite" | "db" | "sqlite3" => save_sqlite(sheet, path),
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

/// Save a sheet as YAML (sequence of maps).
fn save_yaml(sheet: &Sheet, path: &Path) -> Result<()> {
    let visible = sheet.visible_columns();
    let rows: Vec<serde_yaml::Value> = sheet.rows.iter().map(|row| {
        let mut map = serde_yaml::Mapping::new();
        for col in &visible {
            let key = serde_yaml::Value::String(col.name.clone());
            let val = match col.typed_value(row) {
                visidata_core::Value::Null    => serde_yaml::Value::Null,
                visidata_core::Value::Bool(b) => serde_yaml::Value::Bool(b),
                visidata_core::Value::Int(n)  => serde_yaml::Value::Number(n.into()),
                visidata_core::Value::Float(f) => serde_yaml::Value::String(f.to_string()),
                other => serde_yaml::Value::String(other.to_string()),
            };
            map.insert(key, val);
        }
        serde_yaml::Value::Mapping(map)
    }).collect();

    let yaml = serde_yaml::to_string(&serde_yaml::Value::Sequence(rows))
        .context("failed to serialize YAML")?;
    std::fs::write(path, yaml)
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Save a sheet as an HTML table.
fn save_html(sheet: &Sheet, path: &Path) -> Result<()> {
    use std::fmt::Write as FmtWrite;
    let visible = sheet.visible_columns();
    let mut html = String::new();
    writeln!(html, "<table>").unwrap();
    writeln!(html, "<thead><tr>").unwrap();
    for col in &visible {
        writeln!(html, "  <th>{}</th>", html_escape(&col.name)).unwrap();
    }
    writeln!(html, "</tr></thead>").unwrap();
    writeln!(html, "<tbody>").unwrap();
    for row in &sheet.rows {
        writeln!(html, "<tr>").unwrap();
        for col in &visible {
            writeln!(html, "  <td>{}</td>", html_escape(&col.display_value(row))).unwrap();
        }
        writeln!(html, "</tr>").unwrap();
    }
    writeln!(html, "</tbody>").unwrap();
    writeln!(html, "</table>").unwrap();
    std::fs::write(path, html)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
}

/// Save a sheet as Parquet via Arrow.
fn save_parquet(sheet: &Sheet, path: &Path) -> Result<()> {
    use std::sync::Arc;
    use arrow::array::{ArrayRef, Float64Builder, Int64Builder, BooleanBuilder, StringBuilder};
    use arrow::datatypes::{DataType, Field, Schema};
    use parquet::arrow::ArrowWriter;

    let visible = sheet.visible_columns();

    // Infer Arrow schema from column types
    let fields: Vec<Field> = visible.iter().map(|col| {
        let dt = match col.col_type {
            visidata_core::ColumnType::Int      => DataType::Int64,
            visidata_core::ColumnType::Float
            | visidata_core::ColumnType::Currency => DataType::Float64,
            visidata_core::ColumnType::Bool     => DataType::Boolean,
            _ => DataType::Utf8,
        };
        Field::new(&col.name, dt, true)
    }).collect();
    let schema = Arc::new(Schema::new(fields));

    // Build arrays
    let arrays: Vec<ArrayRef> = visible.iter().map(|col| {
        match col.col_type {
            visidata_core::ColumnType::Int => {
                let mut b = Int64Builder::new();
                for row in &sheet.rows {
                    match col.typed_value(row) {
                        visidata_core::Value::Int(n) => b.append_value(n),
                        _ => b.append_null(),
                    }
                }
                Arc::new(b.finish()) as ArrayRef
            }
            visidata_core::ColumnType::Float | visidata_core::ColumnType::Currency => {
                let mut b = Float64Builder::new();
                for row in &sheet.rows {
                    match col.typed_value(row) {
                        visidata_core::Value::Float(f) => b.append_value(f),
                        #[expect(clippy::cast_precision_loss, reason = "i64→f64 coercion acceptable for parquet export")]
                        visidata_core::Value::Int(n) => b.append_value(n as f64),
                        _ => b.append_null(),
                    }
                }
                Arc::new(b.finish()) as ArrayRef
            }
            visidata_core::ColumnType::Bool => {
                let mut b = BooleanBuilder::new();
                for row in &sheet.rows {
                    match col.typed_value(row) {
                        visidata_core::Value::Bool(v) => b.append_value(v),
                        _ => b.append_null(),
                    }
                }
                Arc::new(b.finish()) as ArrayRef
            }
            _ => {
                let mut b = StringBuilder::new();
                for row in &sheet.rows {
                    b.append_value(col.display_value(row));
                }
                Arc::new(b.finish()) as ArrayRef
            }
        }
    }).collect();

    let batch = arrow::record_batch::RecordBatch::try_new(schema.clone(), arrays)
        .context("failed to build Arrow RecordBatch")?;

    let file = std::fs::File::create(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    let mut writer = ArrowWriter::try_new(file, schema, None)
        .context("failed to create Parquet writer")?;
    writer.write(&batch).context("failed to write Parquet batch")?;
    writer.close().context("failed to close Parquet writer")?;
    Ok(())
}

/// Save a sheet to a `SQLite` database, replacing any existing table of the same name.
fn save_sqlite(sheet: &Sheet, path: &Path) -> Result<()> {
    let conn = rusqlite::Connection::open(path)
        .with_context(|| format!("failed to open SQLite at {}", path.display()))?;
    let visible = sheet.visible_columns();
    let table = sheet.name.replace('"', "");
    conn.execute_batch(&format!("DROP TABLE IF EXISTS \"{table}\""))
        .context("DROP TABLE failed")?;
    let col_defs: String = visible.iter()
        .map(|c| format!("\"{}\" TEXT", c.name.replace('"', "")))
        .collect::<Vec<_>>()
        .join(", ");
    conn.execute_batch(&format!("CREATE TABLE \"{table}\" ({col_defs})"))
        .context("CREATE TABLE failed")?;
    let placeholders = vec!["?"; visible.len()].join(", ");
    let col_names: String = visible.iter()
        .map(|c| format!("\"{}\"", c.name.replace('"', "")))
        .collect::<Vec<_>>()
        .join(", ");
    let insert_sql = format!("INSERT INTO \"{table}\" ({col_names}) VALUES ({placeholders})");
    let mut stmt = conn.prepare(&insert_sql).context("prepare INSERT failed")?;
    for row in &sheet.rows {
        let params: Vec<rusqlite::types::ToSqlOutput<'_>> = visible.iter()
            .map(|col| rusqlite::types::ToSqlOutput::from(col.display_value(row)))
            .collect();
        stmt.execute(rusqlite::params_from_iter(params.iter()))
            .context("INSERT row failed")?;
    }
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
        assert_eq!(reloaded.get_cell(1, 1), Value::Int(25)); // age inferred as Int

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
