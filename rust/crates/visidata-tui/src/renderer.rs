//! Sheet rendering for ratatui.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Row as TuiRow, Table};
use unicode_width::UnicodeWidthStr;

use visidata_core::Sheet;

use crate::cliptext;

/// Maximum column width in characters.
const MAX_COL_WIDTH: u16 = 40;

/// Minimum column width in characters.
const MIN_COL_WIDTH: u16 = 3;

/// Draw a sheet into the given frame area.
pub fn draw_sheet(
    frame: &mut Frame<'_>,
    area: Rect,
    sheet: &Sheet,
    status: &str,
    input_line: Option<&str>,
) {
    let has_input = input_line.is_some();
    let chunks = if has_input {
        Layout::vertical([
            Constraint::Min(3),    // table
            Constraint::Length(1), // input line
            Constraint::Length(1), // status bar
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Min(3),    // table
            Constraint::Length(1), // status bar
        ])
        .split(area)
    };

    draw_table(frame, chunks[0], sheet);

    if has_input {
        let input_text = input_line.unwrap_or("");
        let input_line_widget =
            Line::from(input_text).style(Style::new().fg(Color::Yellow).on_black());
        frame.render_widget(input_line_widget, chunks[1]);
        draw_status_bar(frame, chunks[2], status);
    } else {
        draw_status_bar(frame, chunks[1], status);
    }
}

/// Draw the table (header + data rows) with cursor highlighting.
#[expect(clippy::cast_possible_truncation, reason = "display widths won't exceed u16::MAX")]
fn draw_table(frame: &mut Frame<'_>, area: Rect, sheet: &Sheet) {
    let visible_cols = sheet.visible_columns();
    if visible_cols.is_empty() {
        return;
    }

    // Calculate column widths
    let col_widths: Vec<u16> = visible_cols
        .iter()
        .map(|col| {
            col.width.unwrap_or_else(|| {
                // Auto-fit: measure header + sample data rows
                let header_w = UnicodeWidthStr::width(col.name.as_str()) as u16;
                let max_data_w = sheet
                    .rows
                    .iter()
                    .take(100) // sample first 100 rows
                    .map(|row| {
                        let display = col.display_value(row);
                        cliptext::dispwidth(&display) as u16
                    })
                    .max()
                    .unwrap_or(0);
                header_w.max(max_data_w).clamp(MIN_COL_WIDTH, MAX_COL_WIDTH)
            })
        })
        .collect();

    let widths: Vec<Constraint> = col_widths
        .iter()
        .map(|&w| Constraint::Length(w))
        .collect();

    // Header row
    let header_cells: Vec<Cell<'_>> = visible_cols
        .iter()
        .enumerate()
        .map(|(vi, col)| {
            let style = if col.is_key {
                Style::new().bold().underlined()
            } else if vi == sheet.cursor_col {
                Style::new().bold().reversed()
            } else {
                Style::new().bold()
            };
            Cell::new(col.name.clone()).style(style)
        })
        .collect();
    let header = TuiRow::new(header_cells).style(Style::new().on_dark_gray());

    // Calculate visible row range
    let table_height = area.height.saturating_sub(3) as usize; // borders + header
    let top_row = if sheet.cursor_row >= sheet.top_row + table_height {
        sheet.cursor_row.saturating_sub(table_height - 1)
    } else if sheet.cursor_row < sheet.top_row {
        sheet.cursor_row
    } else {
        sheet.top_row
    };

    // Data rows
    let rows: Vec<TuiRow<'_>> = sheet
        .rows
        .iter()
        .enumerate()
        .skip(top_row)
        .take(table_height)
        .map(|(row_idx, row)| {
            let cells: Vec<Cell<'_>> = visible_cols
                .iter()
                .enumerate()
                .map(|(vi, col)| {
                    let display = col.display_value(row);
                    let (clipped, _) =
                        cliptext::clipstr(&display, Some(col_widths[vi] as usize), "…");

                    let is_cursor_row = row_idx == sheet.cursor_row;
                    let is_cursor_cell = is_cursor_row && vi == sheet.cursor_col;

                    let style = if is_cursor_cell {
                        Style::new().reversed()
                    } else if is_cursor_row {
                        Style::new().on_black().fg(Color::White)
                    } else if row.selected {
                        Style::new().fg(Color::Cyan)
                    } else {
                        Style::default()
                    };

                    Cell::new(clipped).style(style)
                })
                .collect();
            TuiRow::new(cells)
        })
        .collect();

    let table = Table::new(rows, &widths)
        .header(header)
        .block(Block::default().borders(Borders::NONE))
        .column_spacing(1)
        .row_highlight_style(Style::new());

    frame.render_widget(table, area);
}

/// Draw the status bar at the bottom of the screen.
fn draw_status_bar(frame: &mut Frame<'_>, area: Rect, status: &str) {
    let status_line = Line::from(status).style(Style::new().on_dark_gray().fg(Color::White));
    frame.render_widget(status_line, area);
}
