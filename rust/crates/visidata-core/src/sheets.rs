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
#[expect(
    clippy::cast_precision_loss,
    reason = "acceptable for statistical summary"
)]
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
                let variance = numeric_vals
                    .iter()
                    .map(|v| (v - mean_f).powi(2))
                    .sum::<f64>()
                    / (n - 1.0);
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
    let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or(".");

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
        let size = metadata
            .as_ref()
            .map_or(Value::Null, |m| Value::Int(m.len() as i64));

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

    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("text");

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

    #[expect(
        clippy::cast_possible_wrap,
        reason = "sheet dimensions won't overflow i64"
    )]
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

// --- Join ---

/// Join type for multi-sheet joins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinType {
    Inner,
    Outer,
    Left,
    Right,
}

/// Join two sheets by their key columns.
///
/// Key columns are identified by `is_key == true`. Both sheets must have at least
/// one key column. The join key is the concatenated display values of all key columns.
#[must_use]
pub fn join_sheets(left: &Sheet, right: &Sheet, join_type: JoinType) -> Sheet {
    let left_keys = key_column_indices(left);
    let right_keys = key_column_indices(right);
    let left_map = build_key_map(left, &left_keys);
    let right_map = build_key_map(right, &right_keys);
    let left_non_key: Vec<usize> = non_key_indices(left);
    let right_non_key: Vec<usize> = non_key_indices(right);
    let out_cols = build_join_columns(left, right, &left_keys, &left_non_key, &right_non_key);
    let all_keys = collect_join_keys(&left_map, &right_map, join_type);
    let out_rows = build_join_rows(
        left, right,
        &left_keys, &right_keys,
        &left_non_key, &right_non_key,
        &left_map, &right_map,
        &all_keys,
    );
    Sheet::with_data(format!("{}&{}", left.name, right.name), out_cols, out_rows)
}

fn non_key_indices(sheet: &Sheet) -> Vec<usize> {
    (0..sheet.columns.len())
        .filter(|&i| !sheet.columns[i].is_key)
        .collect()
}

fn build_join_columns(
    left: &Sheet,
    right: &Sheet,
    left_keys: &[usize],
    left_non_key: &[usize],
    right_non_key: &[usize],
) -> Vec<Column> {
    let mut out_cols = Vec::new();
    let mut col_id = 0;
    for &ki in left_keys {
        let mut col = Column::new(ColumnId(col_id), &left.columns[ki].name, col_id);
        col.is_key = true;
        out_cols.push(col);
        col_id += 1;
    }
    for &ci in left_non_key {
        out_cols.push(Column::new(ColumnId(col_id), format!("{}.{}", left.name, left.columns[ci].name), col_id));
        col_id += 1;
    }
    for &ci in right_non_key {
        out_cols.push(Column::new(ColumnId(col_id), format!("{}.{}", right.name, right.columns[ci].name), col_id));
        col_id += 1;
    }
    out_cols
}

fn collect_join_keys(
    left_map: &std::collections::HashMap<String, Vec<usize>>,
    right_map: &std::collections::HashMap<String, Vec<usize>>,
    join_type: JoinType,
) -> Vec<String> {
    match join_type {
        JoinType::Inner => left_map.keys().filter(|k| right_map.contains_key(*k)).cloned().collect(),
        JoinType::Left => left_map.keys().cloned().collect(),
        JoinType::Right => right_map.keys().cloned().collect(),
        JoinType::Outer => {
            let mut keys: Vec<String> = left_map.keys().cloned().collect();
            keys.extend(right_map.keys().filter(|k| !left_map.contains_key(*k)).cloned());
            keys
        }
    }
}

#[expect(clippy::too_many_arguments, reason = "join is inherently multi-parameter")]
fn build_join_rows(
    left: &Sheet, right: &Sheet,
    left_keys: &[usize], right_keys: &[usize],
    left_non_key: &[usize], right_non_key: &[usize],
    left_map: &std::collections::HashMap<String, Vec<usize>>,
    right_map: &std::collections::HashMap<String, Vec<usize>>,
    all_keys: &[String],
) -> Vec<Row> {
    let null_left = vec![Value::Null; left_non_key.len()];
    let null_right = vec![Value::Null; right_non_key.len()];
    let mut out_rows = Vec::new();

    for key in all_keys {
        let left_rows = left_map.get(key);
        let right_rows = right_map.get(key);
        let left_data = extract_non_key_data(left, left_rows, left_non_key, &null_left);
        let right_data = extract_non_key_data(right, right_rows, right_non_key, &null_right);

        #[expect(clippy::option_if_let_else, reason = "three-way branch is clearer")]
        let key_vals: Vec<Value> = if let Some(indices) = left_rows {
            left_keys.iter().map(|&ki| left.rows[indices[0]].get(left.columns[ki].source_idx).clone()).collect()
        } else if let Some(indices) = right_rows {
            right_keys.iter().map(|&ki| right.rows[indices[0]].get(right.columns[ki].source_idx).clone()).collect()
        } else {
            vec![Value::Null; left_keys.len()]
        };

        for lv in &left_data {
            for rv in &right_data {
                let mut values = key_vals.clone();
                values.extend(lv.iter().cloned());
                values.extend(rv.iter().cloned());
                out_rows.push(Row::new(values));
            }
        }
    }
    out_rows
}

fn extract_non_key_data(
    sheet: &Sheet,
    indices: Option<&Vec<usize>>,
    non_key: &[usize],
    null_row: &[Value],
) -> Vec<Vec<Value>> {
    indices.map_or_else(
        || vec![null_row.to_vec()],
        |idxs| {
            idxs.iter()
                .map(|&ri| non_key.iter().map(|&ci| sheet.rows[ri].get(sheet.columns[ci].source_idx).clone()).collect())
                .collect()
        },
    )
}

/// Concatenate multiple sheets vertically.
///
/// Columns are merged by name; missing values become `Null`.
#[must_use]
pub fn concat_sheets(sheets: &[&Sheet]) -> Sheet {
    if sheets.is_empty() {
        return Sheet::new("concat");
    }

    // Collect all unique column names in order of first appearance.
    let mut col_names: Vec<String> = Vec::new();
    for sheet in sheets {
        for col in &sheet.columns {
            if !col_names.contains(&col.name) {
                col_names.push(col.name.clone());
            }
        }
    }

    let columns: Vec<Column> = col_names
        .iter()
        .enumerate()
        .map(|(i, name)| Column::new(ColumnId(i), name.as_str(), i))
        .collect();

    let mut rows: Vec<Row> = Vec::new();
    for sheet in sheets {
        // Build name → source_idx mapping for this sheet.
        let name_to_idx: HashMap<&str, usize> = sheet
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c.source_idx))
            .collect();

        for row in &sheet.rows {
            let values: Vec<Value> = col_names
                .iter()
                .map(|name| {
                    name_to_idx
                        .get(name.as_str())
                        .map_or(Value::Null, |&idx| row.get(idx).clone())
                })
                .collect();
            rows.push(Row::new(values));
        }
    }

    Sheet::with_data("concat", columns, rows)
}

/// Pivot a sheet: key columns become row identifiers, a value column
/// provides the aggregated values, grouped by a pivot column.
#[must_use]
pub fn pivot_sheet(source: &Sheet, pivot_col_idx: usize) -> Sheet {
    let key_indices: Vec<usize> = key_column_indices(source);
    let Some(pivot_col) = source.columns.get(pivot_col_idx) else {
        return Sheet::new(format!("{}_pivot", source.name));
    };

    // Collect distinct pivot values.
    let mut pivot_values: Vec<String> = Vec::new();
    for row in &source.rows {
        let val = pivot_col.display_value(row);
        if !pivot_values.contains(&val) {
            pivot_values.push(val);
        }
    }

    // Build columns: key columns + one column per pivot value (count).
    let mut out_cols: Vec<Column> = Vec::new();
    let mut col_id = 0;
    for &ki in &key_indices {
        let mut col = Column::new(ColumnId(col_id), &source.columns[ki].name, col_id);
        col.is_key = true;
        out_cols.push(col);
        col_id += 1;
    }
    for pv in &pivot_values {
        out_cols.push(Column::new(ColumnId(col_id), pv.as_str(), col_id));
        col_id += 1;
    }

    // Group rows by key.
    let mut groups: HashMap<String, HashMap<String, i64>> = HashMap::new();
    let mut key_rows: HashMap<String, Vec<Value>> = HashMap::new();

    for row in &source.rows {
        let key = make_row_key(source, row, &key_indices);
        let pval = pivot_col.display_value(row);
        *groups
            .entry(key.clone())
            .or_default()
            .entry(pval)
            .or_insert(0) += 1;
        key_rows.entry(key).or_insert_with(|| {
            key_indices
                .iter()
                .map(|&ki| row.get(source.columns[ki].source_idx).clone())
                .collect()
        });
    }

    let mut out_rows: Vec<Row> = Vec::new();
    for (key, counts) in &groups {
        let mut values = key_rows[key].clone();
        for pv in &pivot_values {
            values.push(Value::Int(*counts.get(pv).unwrap_or(&0)));
        }
        out_rows.push(Row::new(values));
    }

    Sheet::with_data(format!("{}_pivot", source.name), out_cols, out_rows)
}

/// Melt (unpivot) a sheet: convert wide format to long format.
///
/// Key columns stay as-is; non-key columns become (variable, value) rows.
#[must_use]
pub fn melt_sheet(source: &Sheet) -> Sheet {
    let key_indices: Vec<usize> = key_column_indices(source);
    let value_indices: Vec<usize> = (0..source.columns.len())
        .filter(|i| !source.columns[*i].is_key)
        .collect();

    // Build output columns: key cols + "variable" + "value".
    let mut out_cols: Vec<Column> = Vec::new();
    let mut col_id = 0;
    for &ki in &key_indices {
        let mut col = Column::new(ColumnId(col_id), &source.columns[ki].name, col_id);
        col.is_key = true;
        out_cols.push(col);
        col_id += 1;
    }
    out_cols.push(Column::new(ColumnId(col_id), "variable", col_id));
    col_id += 1;
    out_cols.push(Column::new(ColumnId(col_id), "value", col_id));

    let mut out_rows: Vec<Row> = Vec::new();
    for row in &source.rows {
        let key_vals: Vec<Value> = key_indices
            .iter()
            .map(|&ki| row.get(source.columns[ki].source_idx).clone())
            .collect();

        for &vi in &value_indices {
            let mut values = key_vals.clone();
            values.push(Value::Text(source.columns[vi].name.clone()));
            values.push(row.get(source.columns[vi].source_idx).clone());
            out_rows.push(Row::new(values));
        }
    }

    Sheet::with_data(format!("{}_melt", source.name), out_cols, out_rows)
}

// --- Column splitting ---

/// Split a column by a regex pattern into multiple new columns.
///
/// For each row, the pattern is applied to the column value and the capture
/// groups (or split parts) become new column values.  If there are no capture
/// groups, the matched parts from splitting by the delimiter become the values.
///
/// Returns the number of new columns added, or `0` if the pattern is invalid
/// or the column index is out of range.
pub fn split_column(source: &mut Sheet, col_idx: usize, pattern: &str) -> usize {
    let Ok(re) = regex::Regex::new(pattern) else {
        return 0;
    };

    let Some(col) = source.columns.get(col_idx) else {
        return 0;
    };
    let col_name = col.name.clone();
    let source_idx = col.source_idx;

    // Determine max split count across all rows.
    let use_captures = re.captures_len() > 1;
    let max_parts: usize = source
        .rows
        .iter()
        .map(|row| {
            let display = source.columns[col_idx].display_value(row);
            if use_captures {
                re.captures(&display)
                    .map_or(0, |c| c.len().saturating_sub(1))
            } else {
                re.split(&display).count()
            }
        })
        .max()
        .unwrap_or(0);

    if max_parts == 0 {
        return 0;
    }

    // The new values will be appended starting at the current end of each row.
    let base_source_idx = source.rows.first().map_or(0, |r| r.values.len());
    let start_col_id = source.columns.len();

    // Add new columns.
    for i in 0..max_parts {
        let new_col_id = ColumnId(start_col_id + i);
        let new_source_idx = base_source_idx + i;
        source.columns.push(Column::new(
            new_col_id,
            format!("{col_name}_{i}"),
            new_source_idx,
        ));
    }

    // Fill values for each row.
    for row in &mut source.rows {
        let display: String = row.get(source_idx).to_string();
        let parts: Vec<String> = if use_captures {
            re.captures(&display).map_or_else(Vec::new, |caps| {
                (1..caps.len())
                    .map(|i| caps.get(i).map_or("", |m| m.as_str()).to_owned())
                    .collect()
            })
        } else {
            re.split(&display).map(str::to_owned).collect()
        };

        for i in 0..max_parts {
            let val = parts.get(i).map_or(Value::Null, |s| {
                if s.is_empty() {
                    Value::Null
                } else {
                    Value::Text(s.clone())
                }
            });
            let target_idx = base_source_idx + i;
            row.set(target_idx, val);
        }
    }

    max_parts
}

// --- Helpers ---

fn key_column_indices(sheet: &Sheet) -> Vec<usize> {
    sheet
        .columns
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_key)
        .map(|(i, _)| i)
        .collect()
}

fn build_key_map(sheet: &Sheet, key_indices: &[usize]) -> HashMap<String, Vec<usize>> {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (row_idx, row) in sheet.rows.iter().enumerate() {
        let key = make_row_key(sheet, row, key_indices);
        map.entry(key).or_default().push(row_idx);
    }
    map
}

fn make_row_key(sheet: &Sheet, row: &Row, key_indices: &[usize]) -> String {
    key_indices
        .iter()
        .map(|&ki| {
            let col = &sheet.columns[ki];
            col.display_value(row)
        })
        .collect::<Vec<_>>()
        .join("\x00")
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
            Row::new(vec![
                Value::Text("Alice".into()),
                Value::Int(30),
                Value::Float(85.5),
            ]),
            Row::new(vec![
                Value::Text("Bob".into()),
                Value::Int(25),
                Value::Float(92.0),
            ]),
            Row::new(vec![
                Value::Text("Carol".into()),
                Value::Int(35),
                Value::Null,
            ]),
            Row::new(vec![
                Value::Text("Dave".into()),
                Value::Int(25),
                Value::Float(78.0),
            ]),
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

    // --- Join tests ---

    fn join_left_sheet() -> Sheet {
        let mut columns = vec![
            Column::new(ColumnId(0), "id", 0),
            Column::new(ColumnId(1), "name", 1),
        ];
        columns[0].is_key = true;
        let rows = vec![
            Row::new(vec![Value::Int(1), Value::Text("Alice".into())]),
            Row::new(vec![Value::Int(2), Value::Text("Bob".into())]),
            Row::new(vec![Value::Int(3), Value::Text("Carol".into())]),
        ];
        let mut s = Sheet::with_data("left", columns, rows);
        s.num_keys = 1;
        s
    }

    fn join_right_sheet() -> Sheet {
        let mut columns = vec![
            Column::new(ColumnId(0), "id", 0),
            Column::new(ColumnId(1), "score", 1),
        ];
        columns[0].is_key = true;
        let rows = vec![
            Row::new(vec![Value::Int(1), Value::Float(90.0)]),
            Row::new(vec![Value::Int(3), Value::Float(75.0)]),
            Row::new(vec![Value::Int(4), Value::Float(88.0)]),
        ];
        let mut s = Sheet::with_data("right", columns, rows);
        s.num_keys = 1;
        s
    }

    #[test]
    fn join_inner() {
        let left = join_left_sheet();
        let right = join_right_sheet();
        let result = join_sheets(&left, &right, JoinType::Inner);

        assert_eq!(result.num_rows(), 2); // id 1, 3
        assert_eq!(result.num_cols(), 3); // id, left.name, right.score
    }

    #[test]
    fn join_left() {
        let left = join_left_sheet();
        let right = join_right_sheet();
        let result = join_sheets(&left, &right, JoinType::Left);

        assert_eq!(result.num_rows(), 3); // all left rows
    }

    #[test]
    fn join_outer() {
        let left = join_left_sheet();
        let right = join_right_sheet();
        let result = join_sheets(&left, &right, JoinType::Outer);

        assert_eq!(result.num_rows(), 4); // 1, 2, 3, 4
    }

    // --- Concat tests ---

    #[test]
    fn concat_two_sheets() {
        let s1 = Sheet::with_data(
            "a",
            vec![
                Column::new(ColumnId(0), "x", 0),
                Column::new(ColumnId(1), "y", 1),
            ],
            vec![Row::new(vec![Value::Int(1), Value::Int(2)])],
        );
        let s2 = Sheet::with_data(
            "b",
            vec![
                Column::new(ColumnId(0), "x", 0),
                Column::new(ColumnId(1), "z", 1),
            ],
            vec![Row::new(vec![Value::Int(3), Value::Int(4)])],
        );

        let result = concat_sheets(&[&s1, &s2]);
        assert_eq!(result.num_rows(), 2);
        assert_eq!(result.num_cols(), 3); // x, y, z

        // First row from s1: x=1, y=2, z=null
        assert_eq!(result.get_cell(0, 0), Value::Int(1));
        assert_eq!(result.get_cell(0, 1), Value::Int(2));
        assert_eq!(result.get_cell(0, 2), Value::Null);

        // Second row from s2: x=3, y=null, z=4
        assert_eq!(result.get_cell(1, 0), Value::Int(3));
        assert_eq!(result.get_cell(1, 1), Value::Null);
        assert_eq!(result.get_cell(1, 2), Value::Int(4));
    }

    #[test]
    fn concat_empty() {
        let result = concat_sheets(&[]);
        assert_eq!(result.num_rows(), 0);
        assert_eq!(result.num_cols(), 0);
    }

    // --- Pivot tests ---

    #[test]
    fn pivot_basic() {
        let mut columns = vec![
            Column::new(ColumnId(0), "region", 0),
            Column::new(ColumnId(1), "product", 1),
        ];
        columns[0].is_key = true;
        let rows = vec![
            Row::new(vec![
                Value::Text("East".into()),
                Value::Text("Widget".into()),
            ]),
            Row::new(vec![
                Value::Text("East".into()),
                Value::Text("Gadget".into()),
            ]),
            Row::new(vec![
                Value::Text("West".into()),
                Value::Text("Widget".into()),
            ]),
            Row::new(vec![
                Value::Text("East".into()),
                Value::Text("Widget".into()),
            ]),
        ];
        let mut source = Sheet::with_data("sales", columns, rows);
        source.num_keys = 1;

        let pivoted = pivot_sheet(&source, 1);
        assert_eq!(pivoted.num_cols(), 3); // region, Widget, Gadget
        assert_eq!(pivoted.num_rows(), 2); // East, West
    }

    // --- Melt tests ---

    #[test]
    fn melt_basic() {
        let mut columns = vec![
            Column::new(ColumnId(0), "name", 0),
            Column::new(ColumnId(1), "q1", 1),
            Column::new(ColumnId(2), "q2", 2),
        ];
        columns[0].is_key = true;
        let rows = vec![
            Row::new(vec![
                Value::Text("Alice".into()),
                Value::Int(10),
                Value::Int(20),
            ]),
            Row::new(vec![
                Value::Text("Bob".into()),
                Value::Int(30),
                Value::Int(40),
            ]),
        ];
        let mut source = Sheet::with_data("data", columns, rows);
        source.num_keys = 1;

        let melted = melt_sheet(&source);
        assert_eq!(melted.num_cols(), 3); // name, variable, value
        assert_eq!(melted.num_rows(), 4); // 2 rows × 2 value columns

        // First row: Alice, q1, 10
        assert_eq!(melted.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(melted.get_cell(0, 1), Value::Text("q1".into()));
        assert_eq!(melted.get_cell(0, 2), Value::Int(10));
    }

    // --- split_column tests ---

    #[test]
    fn split_column_basic() {
        let columns = vec![Column::new(ColumnId(0), "email", 0)];
        let rows = vec![
            Row::new(vec![Value::Text("alice@example.com".into())]),
            Row::new(vec![Value::Text("bob@test.org".into())]),
        ];
        let mut sheet = Sheet::with_data("test", columns, rows);

        let added = split_column(&mut sheet, 0, "@");
        assert_eq!(added, 2);
        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.get_cell(0, 1), Value::Text("alice".into()));
        assert_eq!(sheet.get_cell(0, 2), Value::Text("example.com".into()));
    }

    #[test]
    fn split_column_captures() {
        let columns = vec![Column::new(ColumnId(0), "date", 0)];
        let rows = vec![Row::new(vec![Value::Text("2023-06-15".into())])];
        let mut sheet = Sheet::with_data("test", columns, rows);

        let added = split_column(&mut sheet, 0, r"(\d{4})-(\d{2})-(\d{2})");
        assert_eq!(added, 3);
        assert_eq!(sheet.get_cell(0, 1), Value::Text("2023".into()));
        assert_eq!(sheet.get_cell(0, 2), Value::Text("06".into()));
        assert_eq!(sheet.get_cell(0, 3), Value::Text("15".into()));
    }

    #[test]
    fn split_column_invalid_regex() {
        let columns = vec![Column::new(ColumnId(0), "x", 0)];
        let rows = vec![Row::new(vec![Value::Text("foo".into())])];
        let mut sheet = Sheet::with_data("test", columns, rows);

        let added = split_column(&mut sheet, 0, "[invalid");
        assert_eq!(added, 0); // no columns added for bad regex
    }
}
