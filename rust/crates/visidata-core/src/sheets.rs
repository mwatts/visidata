//! Built-in sheet types: `MetaSheet`, `DescribeSheet`, `DirSheet`, `TextSheet`.
//!
//! Each function builds a standard `Sheet` from a data source.
//! `FrequencySheet` is already on `Sheet::frequency_sheet()` (Phase 5).

use std::collections::HashMap;
use std::path::Path;

use crate::column::{Column, ColumnId, ColumnType};
use crate::row::Row;
use crate::sheet::Sheet;
use crate::value::Value;

// --- MetaSheet (Columns sheet) ---

/// Build a meta-sheet showing the columns of the given sheet.
///
/// Each row represents a column: name, width, type, key status.
/// This is `VisiData`'s `C` (Columns sheet).
#[must_use]
pub fn columns_sheet(source: &Sheet) -> Sheet {
    let columns = vec![
        Column::new(ColumnId(0), "name", 0),
        Column::new(ColumnId(1), "width", 1),
        Column::new(ColumnId(2), "type", 2),
        Column::new(ColumnId(3), "key", 3),
        Column::new(ColumnId(4), "idx", 4),
    ];

    let rows: Vec<Row> = source
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let width_val = col
                .width
                .map_or_else(|| Value::Text("auto".into()), |w| Value::Int(i64::from(w)));
            #[expect(clippy::cast_possible_wrap, reason = "column index won't exceed i64")]
            Row::new(vec![
                Value::Text(col.name.clone()),
                width_val,
                Value::Text(col.col_type.indicator().to_owned()),
                Value::Bool(col.is_key),
                Value::Int(i as i64),
            ])
        })
        .collect();

    Sheet::with_data(format!("{}_columns", source.name), columns, rows)
}

// --- DescribeSheet (statistical summary) ---

/// Build a describe sheet with statistical summary for each column.
///
/// Shows: column name, type, nulls count, distinct count, min, max,
/// mean, median, stdev (for numeric columns).
#[must_use]
#[expect(clippy::cast_precision_loss, reason = "acceptable for statistical summary")]
pub fn describe_sheet(source: &Sheet) -> Sheet {
    let columns = vec![
        Column::new(ColumnId(0), "column", 0),
        Column::new(ColumnId(1), "type", 1),
        Column::new(ColumnId(2), "nulls", 2),
        Column::new(ColumnId(3), "distinct", 3),
        Column::new(ColumnId(4), "min", 4),
        Column::new(ColumnId(5), "max", 5),
        Column::new(ColumnId(6), "mean", 6),
        Column::new(ColumnId(7), "stdev", 7),
    ];

    let rows: Vec<Row> = source
        .columns
        .iter()
        .map(|col| {
            let mut nulls = 0_usize;
            let mut distinct: HashMap<String, bool> = HashMap::new();
            let mut numeric_vals: Vec<f64> = Vec::new();
            let mut min_val: Option<Value> = None;
            let mut max_val: Option<Value> = None;

            for row in &source.rows {
                let raw = col.raw_value(row);
                if raw.is_null() {
                    nulls += 1;
                    continue;
                }

                let val = col.typed_value(row);
                let display = val.to_string();
                distinct.insert(display, true);

                // Track min/max
                if let Some(ref current_min) = min_val {
                    if val.partial_cmp(current_min) == Some(std::cmp::Ordering::Less) {
                        min_val = Some(val.clone());
                    }
                } else {
                    min_val = Some(val.clone());
                }
                if let Some(ref current_max) = max_val {
                    if val.partial_cmp(current_max) == Some(std::cmp::Ordering::Greater) {
                        max_val = Some(val.clone());
                    }
                } else {
                    max_val = Some(val.clone());
                }

                // Collect numeric values for mean/stdev
                if let Some(f) = val.as_float() {
                    numeric_vals.push(f);
                }
            }

            let mean = if numeric_vals.is_empty() {
                Value::Null
            } else {
                let sum: f64 = numeric_vals.iter().sum();
                Value::Float(sum / numeric_vals.len() as f64)
            };

            let stdev = if numeric_vals.len() < 2 {
                Value::Null
            } else {
                let n = numeric_vals.len() as f64;
                let mean_f = numeric_vals.iter().sum::<f64>() / n;
                let variance = numeric_vals.iter().map(|v| (v - mean_f).powi(2)).sum::<f64>() / (n - 1.0);
                Value::Float(variance.sqrt())
            };

            #[expect(clippy::cast_possible_wrap, reason = "counts won't exceed i64")]
            Row::new(vec![
                Value::Text(col.name.clone()),
                Value::Text(col.col_type.indicator().to_owned()),
                Value::Int(nulls as i64),
                Value::Int(distinct.len() as i64),
                min_val.unwrap_or(Value::Null),
                max_val.unwrap_or(Value::Null),
                mean,
                stdev,
            ])
        })
        .collect();

    Sheet::with_data(format!("{}_describe", source.name), columns, rows)
}

// --- DirSheet (file browser) ---

/// Build a directory listing sheet.
///
/// Each row is a file/directory entry with name, size, type, modified time.
///
/// # Errors
///
/// Returns an error if the directory cannot be read.
pub fn dir_sheet(path: &Path) -> Result<Sheet, String> {
    let dir_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(".");

    let columns = vec![
        Column::new(ColumnId(0), "name", 0),
        Column::new(ColumnId(1), "size", 1),
        Column::new(ColumnId(2), "type", 2),
        Column::new(ColumnId(3), "ext", 3),
    ];

    let entries = std::fs::read_dir(path)
        .map_err(|e| format!("failed to read directory {}: {e}", path.display()))?;

    let mut rows: Vec<Row> = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let file_name = entry.file_name().to_string_lossy().into_owned();

        let metadata = entry.metadata().ok();
        #[expect(clippy::cast_possible_wrap, reason = "file sizes won't exceed i64")]
        let size = metadata.as_ref().map_or(Value::Null, |m| Value::Int(m.len() as i64));

        let file_type = if entry.path().is_dir() {
            "dir"
        } else if entry.path().is_symlink() {
            "link"
        } else {
            "file"
        };

        let ext = entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_owned();

        rows.push(Row::new(vec![
            Value::Text(file_name),
            size,
            Value::Text(file_type.into()),
            Value::Text(ext),
        ]));
    }

    // Sort by name
    rows.sort_by(|a, b| {
        let na = a.get(0).to_string();
        let nb = b.get(0).to_string();
        na.cmp(&nb)
    });

    let mut sheet = Sheet::with_data(dir_name, columns, rows);
    sheet.source = Some(path.to_path_buf());

    // Set size column type to Int
    if let Some(size_col) = sheet.columns.get_mut(1) {
        size_col.col_type = ColumnType::Int;
    }

    Ok(sheet)
}

// --- TextSheet (raw text viewer) ---

/// Build a text sheet from a string, one row per line.
#[must_use]
pub fn text_sheet(name: &str, content: &str) -> Sheet {
    let columns = vec![Column::new(ColumnId(0), "text", 0)];

    let rows: Vec<Row> = content
        .lines()
        .map(|line| Row::new(vec![Value::Text(line.to_owned())]))
        .collect();

    Sheet::with_data(name, columns, rows)
}

/// Build a text sheet from a file.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub fn text_sheet_from_file(path: &Path) -> Result<Sheet, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("text");

    let mut sheet = text_sheet(name, &content);
    sheet.source = Some(path.to_path_buf());
    Ok(sheet)
}

// --- Sheets Sheet (sheet index) ---

/// Build a sheet listing all sheets in the stack.
#[must_use]
pub fn sheets_sheet(sheets: &[&Sheet]) -> Sheet {
    let columns = vec![
        Column::new(ColumnId(0), "name", 0),
        Column::new(ColumnId(1), "rows", 1),
        Column::new(ColumnId(2), "cols", 2),
        Column::new(ColumnId(3), "keys", 3),
        Column::new(ColumnId(4), "source", 4),
    ];

    #[expect(clippy::cast_possible_wrap, reason = "sheet dimensions won't overflow i64")]
    let rows: Vec<Row> = sheets
        .iter()
        .map(|s| {
            let source = s
                .source
                .as_ref()
                .map_or(Value::Null, |p| Value::Text(p.display().to_string()));
            Row::new(vec![
                Value::Text(s.name.clone()),
                Value::Int(s.num_rows() as i64),
                Value::Int(s.num_cols() as i64),
                Value::Int(s.num_keys as i64),
                source,
            ])
        })
        .collect();

    Sheet::with_data("sheets", columns, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sheet() -> Sheet {
        let mut columns = vec![
            Column::new(ColumnId(0), "name", 0),
            Column::new(ColumnId(1), "age", 1),
            Column::new(ColumnId(2), "score", 2),
        ];
        columns[1].col_type = ColumnType::Int;
        columns[2].col_type = ColumnType::Float;

        let rows = vec![
            Row::new(vec![Value::Text("Alice".into()), Value::Int(30), Value::Float(85.5)]),
            Row::new(vec![Value::Text("Bob".into()), Value::Int(25), Value::Float(92.0)]),
            Row::new(vec![Value::Text("Carol".into()), Value::Int(35), Value::Null]),
            Row::new(vec![Value::Text("Dave".into()), Value::Int(25), Value::Float(78.0)]),
        ];
        Sheet::with_data("test", columns, rows)
    }

    // --- MetaSheet tests ---

    #[test]
    fn columns_sheet_basic() {
        let source = sample_sheet();
        let meta = columns_sheet(&source);

        assert_eq!(meta.name, "test_columns");
        assert_eq!(meta.num_cols(), 5);
        assert_eq!(meta.num_rows(), 3);

        assert_eq!(meta.get_cell(0, 0), Value::Text("name".into()));
        assert_eq!(meta.get_cell(1, 0), Value::Text("age".into()));
        assert_eq!(meta.get_cell(2, 0), Value::Text("score".into()));
    }

    #[test]
    fn columns_sheet_type_column() {
        let source = sample_sheet();
        let meta = columns_sheet(&source);

        // Type indicators
        assert_eq!(meta.get_cell(0, 2), Value::Text("~".into())); // Text
        assert_eq!(meta.get_cell(1, 2), Value::Text("#".into())); // Int
        assert_eq!(meta.get_cell(2, 2), Value::Text("%".into())); // Float
    }

    #[test]
    fn columns_sheet_width_auto() {
        let source = sample_sheet();
        let meta = columns_sheet(&source);

        // Default width is "auto"
        assert_eq!(meta.get_cell(0, 1), Value::Text("auto".into()));
    }

    // --- DescribeSheet tests ---

    #[test]
    fn describe_sheet_basic() {
        let source = sample_sheet();
        let desc = describe_sheet(&source);

        assert_eq!(desc.name, "test_describe");
        assert_eq!(desc.num_cols(), 8);
        assert_eq!(desc.num_rows(), 3); // one row per source column
    }

    #[test]
    fn describe_sheet_null_count() {
        let source = sample_sheet();
        let desc = describe_sheet(&source);

        // "score" column (row 2) has 1 null
        assert_eq!(desc.get_cell(2, 2), Value::Int(1)); // nulls
    }

    #[test]
    fn describe_sheet_distinct_count() {
        let source = sample_sheet();
        let desc = describe_sheet(&source);

        // "name" column has 4 distinct values
        assert_eq!(desc.get_cell(0, 3), Value::Int(4));

        // "age" column: 25, 30, 35 → 3 distinct
        assert_eq!(desc.get_cell(1, 3), Value::Int(3));
    }

    #[test]
    fn describe_sheet_min_max() {
        let source = sample_sheet();
        let desc = describe_sheet(&source);

        // "age" column: min=25, max=35
        assert_eq!(desc.get_cell(1, 4), Value::Int(25)); // min
        assert_eq!(desc.get_cell(1, 5), Value::Int(35)); // max
    }

    #[test]
    fn describe_sheet_mean() {
        let source = sample_sheet();
        let desc = describe_sheet(&source);

        // "age" column mean: (30+25+35+25)/4 = 28.75
        if let Value::Float(mean) = desc.get_cell(1, 6) {
            assert!((mean - 28.75).abs() < 0.001);
        } else {
            panic!("expected float for mean");
        }
    }

    #[test]
    fn describe_sheet_stdev() {
        let source = sample_sheet();
        let desc = describe_sheet(&source);

        // "age" stdev should be > 0
        if let Value::Float(stdev) = desc.get_cell(1, 7) {
            assert!(stdev > 0.0);
        } else {
            panic!("expected float for stdev");
        }
    }

    // --- TextSheet tests ---

    #[test]
    fn text_sheet_basic() {
        let sheet = text_sheet("test", "line 1\nline 2\nline 3");

        assert_eq!(sheet.name, "test");
        assert_eq!(sheet.num_cols(), 1);
        assert_eq!(sheet.num_rows(), 3);
        assert_eq!(sheet.get_cell(0, 0), Value::Text("line 1".into()));
        assert_eq!(sheet.get_cell(2, 0), Value::Text("line 3".into()));
    }

    #[test]
    fn text_sheet_empty() {
        let sheet = text_sheet("empty", "");
        assert_eq!(sheet.num_rows(), 0);
    }

    #[test]
    fn text_sheet_single_line() {
        let sheet = text_sheet("one", "hello world");
        assert_eq!(sheet.num_rows(), 1);
        assert_eq!(sheet.get_cell(0, 0), Value::Text("hello world".into()));
    }

    // --- DirSheet tests ---

    #[test]
    fn dir_sheet_current_dir() {
        let sheet = dir_sheet(Path::new(".")).unwrap();
        assert!(sheet.num_rows() > 0);
        assert_eq!(sheet.num_cols(), 4);
        assert_eq!(sheet.columns[0].name, "name");
        assert_eq!(sheet.columns[1].name, "size");
        assert_eq!(sheet.columns[2].name, "type");
    }

    #[test]
    fn dir_sheet_nonexistent() {
        let result = dir_sheet(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn dir_sheet_sorted() {
        let sheet = dir_sheet(Path::new(".")).unwrap();
        if sheet.num_rows() >= 2 {
            let first = sheet.get_cell(0, 0).to_string();
            let second = sheet.get_cell(1, 0).to_string();
            assert!(first <= second, "directory entries should be sorted");
        }
    }

    // --- SheetsSheet tests ---

    #[test]
    fn sheets_sheet_basic() {
        let s1 = sample_sheet();
        let s2 = Sheet::new("empty");
        let index = sheets_sheet(&[&s1, &s2]);

        assert_eq!(index.name, "sheets");
        assert_eq!(index.num_rows(), 2);
        assert_eq!(index.num_cols(), 5);

        assert_eq!(index.get_cell(0, 0), Value::Text("test".into()));
        assert_eq!(index.get_cell(0, 1), Value::Int(4)); // 4 rows
        assert_eq!(index.get_cell(0, 2), Value::Int(3)); // 3 cols

        assert_eq!(index.get_cell(1, 0), Value::Text("empty".into()));
        assert_eq!(index.get_cell(1, 1), Value::Int(0));
    }

    #[test]
    fn sheets_sheet_empty() {
        let index = sheets_sheet(&[]);
        assert_eq!(index.num_rows(), 0);
    }
}
