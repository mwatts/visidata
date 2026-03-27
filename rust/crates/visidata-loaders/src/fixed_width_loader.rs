//! Fixed-width column file loader.
//!
//! Ports the `columnize()` algorithm from Python `VisiData`'s
//! `visidata/loaders/fixed_width.py` to detect column boundaries
//! from whitespace patterns in the header row.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use visidata_core::{Column, ColumnId, Row, Sheet, Value};

use crate::registry::Loader;

/// Loader for fixed-width column files.
///
/// Detects column boundaries by analysing whitespace in the header row,
/// then extracts cell values by position from each line.
#[derive(Debug)]
pub struct FixedWidthLoader;

impl Loader for FixedWidthLoader {
    fn extensions(&self) -> &[&str] {
        &["fixed", "fwf"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_owned();

        let mut sheet = load_fixed_width(&name, &content);
        sheet.source = Some(path.to_path_buf());
        Ok(sheet)
    }
}

/// Convert a line into a vector of chars for char-based indexing.
fn line_chars(line: &str) -> Vec<char> {
    line.chars().collect()
}

/// Detect column boundaries from fixed-width rows.
///
/// Uses the header row (first row) to find column starts — positions where
/// a non-space character follows a space (or is at position 0). Then scans
/// all rows to find the actual end of each column (last non-space before
/// the next column start).
///
/// Returns `(start, end)` char-index pairs for each detected column.
///
/// This is a port of Python `VisiData`'s `columnize()` function from
/// `visidata/loaders/fixed_width.py`, including the fix for issue #2265
/// (data with internal spaces should not create extra columns).
pub fn columnize(rows: &[&str]) -> Vec<(usize, usize)> {
    if rows.is_empty() {
        return vec![];
    }

    // Use the header row to determine column positions (#2265).
    let detect_chars: Vec<Vec<char>> = rows[..1].iter().map(|r| line_chars(r)).collect();

    // Find column start positions: positions where a non-space follows a space
    // (or is at position 0).
    let maxlen = detect_chars.iter().map(Vec::len).max().unwrap_or(0);
    let mut col_starts = Vec::new();

    for i in 0..maxlen {
        let all_space = detect_chars
            .iter()
            .all(|r| i >= r.len() || r[i].is_whitespace());

        if !all_space {
            let prev_all_space = i == 0
                || detect_chars.iter().all(|r| {
                    i.checked_sub(1)
                        .is_none_or(|pi| pi >= r.len() || r[pi].is_whitespace())
                });
            if prev_all_space {
                col_starts.push(i);
            }
        }
    }

    if col_starts.is_empty() {
        return vec![];
    }

    // Find the actual end of each column using all rows (#2255).
    let all_row_chars: Vec<Vec<char>> = rows.iter().map(|r| line_chars(r)).collect();
    let mut all_non_spaces = BTreeSet::new();
    for r in &all_row_chars {
        for (i, ch) in r.iter().enumerate() {
            if !ch.is_whitespace() {
                all_non_spaces.insert(i);
            }
        }
    }

    let mut result = Vec::with_capacity(col_starts.len());
    for (idx, &start) in col_starts.iter().enumerate() {
        if idx + 1 < col_starts.len() {
            let next_start = col_starts[idx + 1];
            let mut end = start;
            for pos in start..next_start {
                if all_non_spaces.contains(&pos) {
                    end = pos + 1;
                }
            }
            result.push((start, end));
        } else {
            // Final column: find last non-space position from start onward.
            let mut end = start;
            for &pos in &all_non_spaces {
                if pos >= start && pos + 1 > end {
                    end = pos + 1;
                }
            }
            result.push((start, end));
        }
    }

    result
}

/// Load a fixed-width file from content.
fn load_fixed_width(name: &str, content: &str) -> Sheet {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return Sheet::with_data(name, vec![], vec![]);
    }

    let col_bounds = columnize(&lines);

    if col_bounds.is_empty() {
        return Sheet::with_data(name, vec![], vec![]);
    }

    // Use the first row as headers.
    let header_chars = line_chars(lines[0]);
    let columns: Vec<Column> = col_bounds
        .iter()
        .enumerate()
        .map(|(i, &(start, end))| {
            let col_name = extract_field_chars(&header_chars, start, end);
            Column::new(ColumnId(i), &col_name, i)
        })
        .collect();

    // Data rows start from the second line.
    let rows: Vec<Row> = lines[1..]
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let chars = line_chars(line);
            let values: Vec<Value> = col_bounds
                .iter()
                .map(|&(start, end)| {
                    let field = extract_field_chars(&chars, start, end);
                    if field.is_empty() {
                        Value::Null
                    } else {
                        Value::Text(field)
                    }
                })
                .collect();
            Row::new(values)
        })
        .collect();

    Sheet::with_data(name, columns, rows)
}

/// Extract and trim a field from a char array given start and end char positions.
fn extract_field_chars(chars: &[char], start: usize, end: usize) -> String {
    if start >= chars.len() {
        return String::new();
    }
    let actual_end = end.min(chars.len());
    let s: String = chars[start..actual_end].iter().collect();
    s.trim().to_owned()
}

/// Load fixed-width content from a string (for testing).
#[must_use]
pub fn load_fixed_width_from_str(name: &str, content: &str) -> Sheet {
    load_fixed_width(name, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
    }

    // --- columnize tests ported from Python's test_fixed_width.py ---

    #[test]
    fn columnize_basic() {
        let rows = vec!["name age city", "Alice 30  NYC "];
        let cols = columnize(&rows);
        assert_eq!(cols.len(), 3);
    }

    #[test]
    fn columnize_data_with_internal_spaces() {
        // Issue #2265: data values with spaces should not create extra columns.
        let rows = vec![
            "colours shades               counts",
            "red     light                3     ",
            "green   very very very light 5     ",
            "blue    dark                 8     ",
        ];
        let cols = columnize(&rows);
        assert_eq!(cols.len(), 3, "Expected 3, got {}: {cols:?}", cols.len());

        // Verify last data row extracts correctly.
        let row_chars = line_chars(rows[2]);
        let vals: Vec<String> = cols
            .iter()
            .map(|&(i, j)| extract_field_chars(&row_chars, i, j))
            .collect();
        assert_eq!(vals, vec!["green", "very very very light", "5"]);
    }

    #[test]
    fn columnize_empty() {
        assert!(columnize(&[]).is_empty());
    }

    #[test]
    fn columnize_single_column() {
        let rows = vec!["name", "Alice"];
        let cols = columnize(&rows);
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0], (0, 5));
    }

    // --- Loader tests ---

    #[test]
    fn load_test_fixed_file() {
        let path = fixtures_dir().join("test.fixed");
        let sheet = FixedWidthLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "test");
        assert!(sheet.num_cols() > 0, "should detect columns");
        assert!(sheet.num_rows() > 0, "should have data rows");
    }

    #[test]
    fn load_benchmark_fixed_file() {
        let path = fixtures_dir().join("benchmark.fixed");
        let sheet = FixedWidthLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "benchmark");
        assert!(sheet.num_cols() > 0);
        assert!(sheet.num_rows() > 0);
    }

    #[test]
    fn load_fixed_from_str_simple() {
        let data = "name  age city\nAlice 30  NYC\nBob   25  LA\n";
        let sheet = load_fixed_width_from_str("test", data);

        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.num_rows(), 2);

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(col_names, vec!["name", "age", "city"]);

        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(sheet.get_cell(0, 1), Value::Text("30".into()));
        assert_eq!(sheet.get_cell(0, 2), Value::Text("NYC".into()));
    }

    #[test]
    fn load_fixed_unicode() {
        // CJK chars are 1 char each in Rust, so char-column positions align.
        let data = "name   city\nAlicé  NYC\nBob    LA\n";
        let sheet = load_fixed_width_from_str("test", data);

        assert_eq!(sheet.num_cols(), 2);
        assert_eq!(sheet.num_rows(), 2);
        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alicé".into()));
        assert_eq!(sheet.get_cell(0, 1), Value::Text("NYC".into()));
    }

    #[test]
    fn load_fixed_empty_content() {
        let sheet = load_fixed_width_from_str("test", "");
        assert_eq!(sheet.num_rows(), 0);
        assert_eq!(sheet.num_cols(), 0);
    }

    #[test]
    fn nonexistent_fixed_file() {
        let result = FixedWidthLoader.load(Path::new("/nonexistent/file.fixed"));
        assert!(result.is_err());
    }
}
