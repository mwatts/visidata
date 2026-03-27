//! Sheet rendering for ratatui.

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, Cell, Row as TuiRow, Scrollbar, ScrollbarOrientation, ScrollbarState, Table,
};
use unicode_width::UnicodeWidthStr;

use visidata_core::{Sheet, SortDirection, Value};

use crate::cliptext;
use crate::theme::{CellContext, Theme};

/// Maximum column width for auto-fit (characters). Matches Python's default.
const MAX_COL_WIDTH: u16 = 80;

/// Minimum column width in characters.
const MIN_COL_WIDTH: u16 = 3;

/// Compute how many visible columns fit within `area_width` starting at `left_col`.
///
/// Used by the input handler to decide when to scroll `left_col`.
#[must_use]
pub fn cols_fitting_in_width(sheet: &Sheet, area_width: u16) -> usize {
    let visible = sheet.visible_columns();
    let left = sheet.left_col.min(visible.len().saturating_sub(1));
    let mut used: u16 = 0;
    let mut count = 0;
    for col in visible.iter().skip(left) {
        let w = col.width.unwrap_or(20).min(MAX_COL_WIDTH) + 1; // +1 for column spacing
        if used + w > area_width { break; }
        used += w;
        count += 1;
    }
    count.max(1)
}

/// Draw a sheet into the given frame area.
///
/// Returns the computed `top_row` so the caller can write it back to the sheet.
#[expect(clippy::too_many_arguments, reason = "draw functions need all rendering context")]
pub fn draw_sheet(
    frame: &mut Frame<'_>,
    area: Rect,
    sheet: &Sheet,
    status: &str,
    input_line: Option<&str>,
    theme: &Theme,
    engine: &rhai::Engine,
    show_sidebar: bool,
) -> usize {
    use ratatui::widgets::LineGauge;
    use visidata_core::async_loader::LoadingState;

    // Split into main area and optional sidebar.
    const SIDEBAR_WIDTH: u16 = 26;
    let main_area = if show_sidebar && area.width > SIDEBAR_WIDTH + 10 {
        let parts = Layout::horizontal([
            Constraint::Min(10),
            Constraint::Length(SIDEBAR_WIDTH),
        ])
        .split(area);
        draw_sidebar(frame, parts[1], sheet, theme);
        parts[0]
    } else {
        area
    };

    let has_input = input_line.is_some();
    let chunks = if has_input {
        Layout::vertical([
            Constraint::Min(3),    // table
            Constraint::Length(1), // input line
            Constraint::Length(1), // status bar
        ])
        .split(main_area)
    } else {
        Layout::vertical([
            Constraint::Min(3),    // table
            Constraint::Length(1), // status bar
        ])
        .split(main_area)
    };

    let computed_top_row = draw_table(frame, chunks[0], sheet, theme, engine);

    let status_area = if has_input {
        let input_text = input_line.unwrap_or("");
        let input_line_widget = Line::from(input_text).style(theme.input_line);
        frame.render_widget(input_line_widget, chunks[1]);
        chunks[2]
    } else {
        chunks[1]
    };

    // Show a loading gauge when loading, otherwise the normal status bar.
    if let LoadingState::Loading { rows_loaded, .. } = &sheet.loading_state {
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

    computed_top_row
}

/// Draw the table (header + data rows) with cursor highlighting.
#[expect(
    clippy::cast_possible_truncation,
    reason = "display widths won't exceed u16::MAX"
)]
#[expect(clippy::too_many_lines, reason = "renderer — splitting would not improve clarity")]
fn draw_table(frame: &mut Frame<'_>, area: Rect, sheet: &Sheet, theme: &Theme, engine: &rhai::Engine) -> usize {
    let all_visible = sheet.visible_columns();
    if all_visible.is_empty() {
        return sheet.top_row;
    }
    // Show a centered message when there are no rows to display.
    if sheet.rows.is_empty() {
        use ratatui::widgets::Paragraph;
        let msg = Paragraph::new("No rows.")
            .style(theme.cell_null)
            .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return 0;
    }
    // Respect left_col scroll offset (GAP-010): slice off columns to the left.
    let left = sheet.left_col.min(all_visible.len().saturating_sub(1));
    let visible_cols: Vec<&visidata_core::Column> = all_visible[left..].to_vec();

    // Build per-column metadata: whether there are hidden cols before each visible col (GAP-124)
    let all_cols = &sheet.columns;
    let vis_indices: Vec<usize> = all_cols.iter().enumerate()
        .filter(|(_, c)| !c.is_hidden())
        .map(|(i, _)| i)
        .collect();

    // For each visible column, does a hidden col precede it?
    let has_hidden_before: Vec<bool> = vis_indices.iter().enumerate().map(|(vi, &ci)| {
        let prev_vis = if vi == 0 { 0 } else { vis_indices[vi - 1] + 1 };
        (prev_vis..ci).any(|i| all_cols.get(i).is_some_and(visidata_core::Column::is_hidden))
    }).collect();

    // Calculate column widths (add 1 for hidden indicator `…` where needed)
    let col_widths: Vec<u16> = visible_cols
        .iter()
        .enumerate()
        .map(|(vi, col)| {
            let base = col.width.unwrap_or_else(|| {
                let header_w = UnicodeWidthStr::width(col.name.as_str()) as u16;
                let max_data_w = sheet
                    .rows
                    .iter()
                    .map(|row| {
                        let display = col.display_value(row);
                        cliptext::dispwidth(&display) as u16
                    })
                    .max()
                    .unwrap_or(0);
                header_w.max(max_data_w).clamp(MIN_COL_WIDTH, MAX_COL_WIDTH)
            });
            // If there are hidden cols before this one, add a narrow slot for the indicator
            if *has_hidden_before.get(vi).unwrap_or(&false) { base + 2 } else { base }
        })
        .collect();

    let widths: Vec<Constraint> = col_widths.iter().map(|&w| Constraint::Length(w)).collect();

    // Build sort-key lookup: col_idx → (priority, direction indicator).
    // Priority is 1-based; shown as ↑1, ↓2 etc. for multi-key sorts.
    let sort_info: std::collections::HashMap<usize, (usize, &'static str)> = sheet.sort_keys
        .iter()
        .enumerate()
        .map(|(pri, sk)| (sk.col_idx, (pri + 1, match sk.direction {
            SortDirection::Ascending  => "↑",
            SortDirection::Descending => "↓",
        })))
        .collect();
    let multi_sort = sheet.sort_keys.len() > 1;

    // Header row — hidden-column indicator (GAP-124) + type indicator + sort indicators
    let header_cells: Vec<Cell<'_>> = visible_cols
        .iter()
        .enumerate()
        .map(|(vi, col)| {
            let style = theme.header_style(col.is_key, vi + left == sheet.cursor_col);
            // Find this col's position in sheet.columns for sort lookup
            let col_idx = sheet.columns.iter().position(|c| c.id == col.id).unwrap_or(usize::MAX);
            let sort_suffix = sort_info.get(&col_idx).map_or(String::new(), |&(pri, arrow)| {
                if multi_sort {
                    format!("{arrow}{pri}")
                } else {
                    arrow.to_owned()
                }
            });
            // Type indicator character (e.g. #, %, ~, @, $)
            let type_char = col.col_type.indicator();
            let hidden_prefix = if *has_hidden_before.get(vi).unwrap_or(&false) { "…" } else { "" };
            let label = format!("{hidden_prefix}{}{}{sort_suffix}", type_char, col.name);
            Cell::new(label).style(style)
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
                    // Use lazy expr eval when the column has an expression (GAP-103).
                    let raw_value = if col.expr.is_some() {
                        col.eval_expr_value(engine, &sheet.columns, row)
                    } else {
                        col.typed_value(row)
                    };
                    let display = if col.expr.is_some() {
                        raw_value.to_string()
                    } else {
                        col.display_value(row)
                    };
                    let (clipped, _) =
                        cliptext::clipstr(&display, Some(col_widths[vi] as usize), "…");

                    let is_cursor_row = row_idx == sheet.cursor_row;
                    let ctx = CellContext {
                        is_cursor_cell: is_cursor_row && vi + left == sheet.cursor_col,
                        is_cursor_row,
                        is_selected: row.selected,
                        is_pending_delete: row.pending_delete,
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

    top_row
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

/// Draw a sidebar panel showing contextual sheet/column information.
fn draw_sidebar(frame: &mut Frame<'_>, area: Rect, sheet: &Sheet, theme: &Theme) {
    use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(theme.column_sep)
        .title(" Info ")
        .title_style(theme.header);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let cur_col = sheet.visible_columns().get(sheet.cursor_col).copied();
    let col_info = cur_col.as_ref().map_or_else(
        || "—".to_owned(),
        |c| format!("{}{} ({})", c.col_type.indicator(), c.name,
            c.width.map_or_else(|| "auto".to_owned(), |w| format!("{w}ch"))),
    );
    let agg_info = cur_col.as_ref()
        .and_then(|c| c.aggregators.first().copied())
        .map_or_else(|| "none".to_owned(), |f| f.name().to_owned());

    let sort_info = if sheet.sort_keys.is_empty() {
        "none".to_owned()
    } else {
        sheet.sort_keys.iter().enumerate()
            .map(|(i, sk)| {
                let name = sheet.columns.get(sk.col_idx)
                    .map_or("?", |c| c.name.as_str());
                let arrow = match sk.direction {
                    SortDirection::Ascending  => "↑",
                    SortDirection::Descending => "↓",
                };
                format!("{}{}{}", name, arrow, i + 1)
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    let pending = sheet.rows.iter().filter(|r| r.pending_delete).count();
    let pending_info = if pending > 0 {
        format!("\n⚠ {pending} pending delete")
    } else {
        String::new()
    };

    let text = format!(
        "{}\n{} rows × {} cols\n\nCol: {col_info}\nAgg: {agg_info}\n\nSort: {sort_info}{pending_info}",
        sheet.name,
        sheet.num_rows(),
        sheet.visible_columns().len(),
    );

    let para = Paragraph::new(text)
        .style(theme.cell_null)
        .wrap(Wrap { trim: true });
    frame.render_widget(para, inner);
}

/// Draw a floating right-click context menu.
pub fn draw_context_menu(
    frame: &mut Frame<'_>,
    area: Rect,
    items: &[(String, String)],
    selected: usize,
    anchor_row: u16,
    anchor_col: u16,
    theme: &Theme,
) {
    use ratatui::widgets::{Clear, List, ListItem};

    #[expect(clippy::cast_possible_truncation, reason = "item label lengths won't exceed u16")]
    let w = items.iter().map(|(l, _)| l.len()).max().unwrap_or(10) as u16 + 4;
    #[expect(clippy::cast_possible_truncation, reason = "context menus won't have more than u16::MAX items")]
    let h = items.len() as u16 + 2;

    let x = anchor_col.min(area.width.saturating_sub(w));
    let y = (anchor_row + 1).min(area.height.saturating_sub(h));
    let menu_area = Rect { x, y, width: w.min(area.width), height: h.min(area.height) };

    frame.render_widget(Clear, menu_area);
    let list_items: Vec<ListItem<'_>> = items.iter().enumerate()
        .map(|(i, (label, _))| {
            if i == selected {
                ListItem::new(format!(" > {label} ")).style(theme.cell_cursor)
            } else {
                ListItem::new(format!("   {label} ")).style(theme.cell_default)
            }
        })
        .collect();
    let list = List::new(list_items)
        .block(ratatui::widgets::Block::bordered().border_style(theme.status_bar));
    frame.render_widget(list, menu_area);
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
