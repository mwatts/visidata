//! Sheet rendering for ratatui.

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Cell, Row as TuiRow, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
};
use unicode_width::UnicodeWidthStr;

use visidata_core::{Sheet, Value};

use crate::cliptext;
use crate::theme::{CellContext, Theme};

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
    theme: &Theme,
) {
    use ratatui::widgets::LineGauge;
    use visidata_core::async_loader::LoadingState;

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

    draw_table(frame, chunks[0], sheet, theme);

    let status_area = if has_input {
        let input_text = input_line.unwrap_or("");
        let input_line_widget = Line::from(input_text).style(theme.input_line);
        frame.render_widget(input_line_widget, chunks[1]);
        chunks[2]
    } else {
        chunks[1]
    };

    // Show a loading gauge when loading, otherwise the normal status bar.
    if let LoadingState::Loading { rows_loaded } = &sheet.loading_state {
        #[expect(
            clippy::cast_precision_loss,
            reason = "visual-only gauge ratio; precision not critical"
        )]
        let ratio = if *rows_loaded > 0 {
            let pulse = (*rows_loaded % 100) as f64 / 100.0;
            pulse.clamp(0.1, 0.9)
        } else {
            0.0
        };
        let gauge = LineGauge::default()
            .ratio(ratio)
            .label(format!("loading… {rows_loaded} rows"))
            .style(theme.status_bar)
            .filled_style(theme.cell_cursor);
        frame.render_widget(gauge, status_area);
    } else {
        draw_status_bar(frame, status_area, status, theme);
    }
}

/// Draw the table (header + data rows) with cursor highlighting.
#[expect(
    clippy::cast_possible_truncation,
    reason = "display widths won't exceed u16::MAX"
)]
fn draw_table(frame: &mut Frame<'_>, area: Rect, sheet: &Sheet, theme: &Theme) {
    let visible_cols = sheet.visible_columns();
    if visible_cols.is_empty() {
        return;
    }

    // Calculate column widths
    let col_widths: Vec<u16> = visible_cols
        .iter()
        .map(|col| {
            col.width.unwrap_or_else(|| {
                let header_w = UnicodeWidthStr::width(col.name.as_str()) as u16;
                let max_data_w = sheet
                    .rows
                    .iter()
                    .take(100)
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

    let widths: Vec<Constraint> = col_widths.iter().map(|&w| Constraint::Length(w)).collect();

    // Header row
    let header_cells: Vec<Cell<'_>> = visible_cols
        .iter()
        .enumerate()
        .map(|(vi, col)| {
            let style = theme.header_style(col.is_key, vi == sheet.cursor_col);
            Cell::new(col.name.clone()).style(style)
        })
        .collect();
    let header = TuiRow::new(header_cells);

    // Calculate visible row range
    let table_height = area.height.saturating_sub(3) as usize;
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
                    let raw_value = col.raw_value(row);
                    let display = col.display_value(row);
                    let (clipped, _) =
                        cliptext::clipstr(&display, Some(col_widths[vi] as usize), "…");

                    let is_cursor_row = row_idx == sheet.cursor_row;
                    let ctx = CellContext {
                        is_cursor_cell: is_cursor_row && vi == sheet.cursor_col,
                        is_cursor_row,
                        is_selected: row.selected,
                        is_key_col: col.is_key,
                        is_null: raw_value.is_null(),
                        is_error: raw_value.is_error(),
                        is_numeric: matches!(raw_value, Value::Int(_) | Value::Float(_)),
                    };
                    let style = theme.cell_style(&ctx);

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

    // Vertical scrollbar when there are more rows than visible.
    let total_rows = sheet.num_rows();
    if total_rows > table_height {
        let mut scrollbar_state = ScrollbarState::new(total_rows).position(sheet.cursor_row);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(theme.column_sep),
            area,
            &mut scrollbar_state,
        );
    }
}

/// Draw the status bar at the bottom of the screen.
fn draw_status_bar(frame: &mut Frame<'_>, area: Rect, status: &str, theme: &Theme) {
    let status_line = Line::from(status).style(theme.status_bar);
    frame.render_widget(status_line, area);
}

/// Returns a centered `Rect` that is `percent_x`% wide and `percent_y`% tall.
#[must_use]
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

/// Draw a floating help overlay showing all keybindings.
pub fn draw_help_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    commands: &visidata_core::CommandRegistry,
    scroll: usize,
    theme: &Theme,
) {
    use ratatui::widgets::Clear;

    let popup_area = centered_rect(80, 80, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(" Keybindings — F1 or Esc to close ")
        .title_style(theme.header)
        .border_style(theme.status_bar);
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Sort by keystroke for readability in the overlay.
    let mut cmds: Vec<&visidata_core::CommandInfo> = commands.all_commands();
    cmds.sort_by(|a, b| a.keystrokes.cmp(&b.keystrokes));

    let visible_height = inner.height as usize;
    let total = cmds.len();
    let scroll = scroll.min(total.saturating_sub(1));

    let rows: Vec<TuiRow<'_>> = cmds
        .iter()
        .skip(scroll)
        .take(visible_height)
        .map(|cmd| {
            TuiRow::new(vec![
                Cell::new(cmd.keystrokes.clone()).style(theme.header_key),
                Cell::new(cmd.longname.clone()).style(theme.cell_default),
                Cell::new(cmd.help.clone()).style(theme.cell_null),
            ])
        })
        .collect();

    let widths = [
        Constraint::Length(10),
        Constraint::Length(24),
        Constraint::Fill(1),
    ];
    let table = Table::new(rows, widths)
        .header(TuiRow::new(vec![
            Cell::new("Key").style(theme.header),
            Cell::new("Command").style(theme.header),
            Cell::new("Description").style(theme.header),
        ]))
        .column_spacing(2);

    frame.render_widget(table, inner);

    // Scrollbar on the right side.
    let mut scrollbar_state = ScrollbarState::new(total).position(scroll);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight),
        inner,
        &mut scrollbar_state,
    );
}

/// Draw a menu bar at the top of the screen with a dropdown for the active menu.
pub fn draw_menu_overlay(
    frame: &mut Frame<'_>,
    area: Rect,
    menu_bar: &visidata_core::menu::MenuBar,
    menu_state: &visidata_core::menu::MenuState,
    theme: &Theme,
) {
    use ratatui::widgets::{Clear, List, ListItem};

    // Top menu bar — one row across the full width.
    let bar_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };

    let mut spans: Vec<Span<'_>> = Vec::new();
    for (i, item) in menu_bar.menus.iter().enumerate() {
        let label = item.label();
        if i == menu_state.menu_idx {
            spans.push(Span::styled(format!(" {label} "), theme.header_cursor));
        } else {
            spans.push(Span::styled(format!(" {label} "), theme.header));
        }
    }
    let bar_line = Line::from(spans);
    frame.render_widget(Clear, bar_area);
    frame.render_widget(bar_line, bar_area);

    // Dropdown below the selected menu.
    let Some(visidata_core::menu::MenuItem::Submenu { items, .. }) =
        menu_bar.menus.get(menu_state.menu_idx)
    else {
        return;
    };

    // X offset: sum of widths of all menus to the left.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "menu label lengths will not exceed u16::MAX"
    )]
    let x_offset: u16 = menu_bar
        .menus
        .iter()
        .take(menu_state.menu_idx)
        .map(|m| (m.label().len() + 2) as u16)
        .sum();

    #[expect(
        clippy::cast_possible_truncation,
        reason = "dropdown dimensions will not exceed u16::MAX"
    )]
    let dropdown_width = items.iter().map(|i| i.label().len()).max().unwrap_or(10) as u16 + 4;

    #[expect(
        clippy::cast_possible_truncation,
        reason = "item count will not exceed u16::MAX"
    )]
    let dropdown_height = items.len() as u16 + 2;

    let dropdown_area = Rect {
        x: area.x + x_offset,
        y: area.y + 1,
        width: dropdown_width.min(area.width.saturating_sub(x_offset)),
        height: dropdown_height.min(area.height.saturating_sub(1)),
    };

    frame.render_widget(Clear, dropdown_area);

    let list_items: Vec<ListItem<'_>> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let label = item.label();
            if i == menu_state.item_idx {
                ListItem::new(format!(" > {label} ")).style(theme.cell_cursor)
            } else {
                ListItem::new(format!("   {label} ")).style(theme.cell_default)
            }
        })
        .collect();

    let list = List::new(list_items).block(Block::bordered().border_style(theme.status_bar));
    frame.render_widget(list, dropdown_area);
}

/// Draw a centered command palette modal.
pub fn draw_command_palette(
    frame: &mut Frame<'_>,
    area: Rect,
    query: &str,
    matches: &[&visidata_core::CommandInfo],
    theme: &Theme,
) {
    use ratatui::widgets::{Clear, List, ListItem, Paragraph};

    let popup_area = centered_rect(60, 60, area);
    frame.render_widget(Clear, popup_area);

    let block = Block::bordered()
        .title(" Command Palette (Esc to close) ")
        .border_style(theme.status_bar);
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Split: 1 line for input, 1 line for separator, rest for match list.
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .split(inner);

    let input = Paragraph::new(format!(": {query}")).style(theme.input_line);
    frame.render_widget(input, chunks[0]);

    let divider = Paragraph::new("─".repeat(inner.width as usize)).style(theme.status_bar);
    frame.render_widget(divider, chunks[1]);

    let items: Vec<ListItem<'_>> = matches
        .iter()
        .take(chunks[2].height as usize)
        .map(|cmd| {
            let key_part = if cmd.keystrokes.is_empty() {
                "     ".to_owned()
            } else {
                format!("{:>5}", cmd.keystrokes)
            };
            ListItem::new(format!("{key_part}  {}  — {}", cmd.longname, cmd.help))
                .style(theme.cell_default)
        })
        .collect();

    let list = if items.is_empty() {
        List::new(vec![ListItem::new("  no matches").style(theme.cell_null)])
    } else {
        List::new(items)
    };
    frame.render_widget(list, chunks[2]);
}
