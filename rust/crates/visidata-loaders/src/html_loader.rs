//! HTML table loader.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use scraper::{Html, Selector};
use visidata_core::{Column, ColumnId, Row, Sheet, Value};

use crate::registry::Loader;

/// Loader for HTML files containing `<table>` elements.
///
/// Extracts the first `<table>` found in the document. If the table has
/// `<th>` elements in the first row, they become column headers; otherwise
/// columns are named `A`, `B`, `C`, etc.
#[derive(Debug)]
pub struct HtmlLoader;

impl Loader for HtmlLoader {
    fn extensions(&self) -> &[&str] {
        &["html", "htm"]
    }

    fn load(&self, path: &Path) -> Result<Sheet> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_owned();

        let mut sheet = load_html_tables(&name, &content);
        sheet.source = Some(path.to_path_buf());
        Ok(sheet)
    }
}

/// Load the first HTML table from content into a sheet.
fn load_html_tables(name: &str, content: &str) -> Sheet {
    let document = Html::parse_document(content);
    let table_selector = Selector::parse("table").unwrap();
    let row_selector = Selector::parse("tr").unwrap();
    let header_selector = Selector::parse("th").unwrap();
    let cell_selector = Selector::parse("td").unwrap();

    let Some(table) = document.select(&table_selector).next() else {
        return Sheet::with_data(name, vec![], vec![]);
    };

    let mut all_rows: Vec<Vec<String>> = Vec::new();
    let mut has_headers = false;

    for (i, tr) in table.select(&row_selector).enumerate() {
        let ths: Vec<String> = tr
            .select(&header_selector)
            .map(|el| el.text().collect::<String>().trim().to_owned())
            .collect();

        if !ths.is_empty() && i == 0 {
            has_headers = true;
            all_rows.push(ths);
            continue;
        }

        let tds: Vec<String> = tr
            .select(&cell_selector)
            .map(|el| el.text().collect::<String>().trim().to_owned())
            .collect();

        if !tds.is_empty() {
            all_rows.push(tds);
        }
    }

    if all_rows.is_empty() {
        return Sheet::with_data(name, vec![], vec![]);
    }

    // Determine max number of columns.
    let num_cols = all_rows.iter().map(Vec::len).max().unwrap_or(0);

    let (columns, data_rows) = if has_headers {
        let headers = &all_rows[0];
        let cols: Vec<Column> = (0..num_cols)
            .map(|i| {
                let col_name = headers
                    .get(i)
                    .filter(|s| !s.is_empty())
                    .map_or_else(|| excel_col_name(i), std::clone::Clone::clone);
                Column::new(ColumnId(i), &col_name, i)
            })
            .collect();
        (cols, &all_rows[1..])
    } else {
        let cols: Vec<Column> = (0..num_cols)
            .map(|i| Column::new(ColumnId(i), excel_col_name(i), i))
            .collect();
        (cols, all_rows.as_slice())
    };

    let rows: Vec<Row> = data_rows
        .iter()
        .map(|cells| {
            let values: Vec<Value> = (0..num_cols)
                .map(|i| {
                    cells.get(i).map_or(Value::Null, |s| {
                        if s.is_empty() {
                            Value::Null
                        } else {
                            Value::Text(s.clone())
                        }
                    })
                })
                .collect();
            Row::new(values)
        })
        .collect();

    Sheet::with_data(name, columns, rows)
}

/// Generate a column name like A, B, ..., Z, AA, AB, ...
fn excel_col_name(idx: usize) -> String {
    let mut name = String::new();
    let mut n = idx;
    loop {
        name.insert(0, (b'A' + u8::try_from(n % 26).unwrap_or(0)) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    name
}

/// Load HTML from a string (for testing).
#[must_use]
pub fn load_html_from_str(name: &str, content: &str) -> Sheet {
    load_html_tables(name, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixtures_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
    }

    #[test]
    fn load_simple_html() {
        let path = fixtures_dir().join("simple.html");
        let sheet = HtmlLoader.load(&path).unwrap();

        assert_eq!(sheet.name, "simple");
        assert_eq!(sheet.num_cols(), 3);
        assert_eq!(sheet.num_rows(), 3);

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(col_names, vec!["name", "age", "city"]);

        assert_eq!(sheet.get_cell(0, 0), Value::Text("Alice".into()));
        assert_eq!(sheet.get_cell(0, 1), Value::Text("30".into()));
        assert_eq!(sheet.get_cell(2, 2), Value::Text("Chicago".into()));
    }

    #[test]
    fn load_multi_table_html() {
        let path = fixtures_dir().join("multi-table.html");
        let sheet = HtmlLoader.load(&path).unwrap();

        // Should load the first table only.
        assert_eq!(sheet.num_cols(), 2);
        assert_eq!(sheet.num_rows(), 2);

        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(col_names, vec!["fruit", "count"]);
    }

    #[test]
    fn html_no_headers() {
        let html = "<table><tr><td>1</td><td>2</td></tr><tr><td>3</td><td>4</td></tr></table>";
        let sheet = load_html_from_str("test", html);

        assert_eq!(sheet.num_cols(), 2);
        assert_eq!(sheet.num_rows(), 2);

        // Should use generated column names.
        let col_names: Vec<&str> = sheet.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(col_names, vec!["A", "B"]);

        assert_eq!(sheet.get_cell(0, 0), Value::Text("1".into()));
    }

    #[test]
    fn html_no_table() {
        let html = "<html><body><p>No table here</p></body></html>";
        let sheet = load_html_from_str("test", html);
        assert_eq!(sheet.num_rows(), 0);
        assert_eq!(sheet.num_cols(), 0);
    }

    #[test]
    fn html_empty_table() {
        let html = "<table></table>";
        let sheet = load_html_from_str("test", html);
        assert_eq!(sheet.num_rows(), 0);
    }

    #[test]
    fn excel_col_names() {
        assert_eq!(excel_col_name(0), "A");
        assert_eq!(excel_col_name(1), "B");
        assert_eq!(excel_col_name(25), "Z");
        assert_eq!(excel_col_name(26), "AA");
        assert_eq!(excel_col_name(27), "AB");
    }

    #[test]
    fn nonexistent_html_file() {
        let result = HtmlLoader.load(Path::new("/nonexistent/file.html"));
        assert!(result.is_err());
    }
}
