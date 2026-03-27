//! Application state and main event loop.

use std::io;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::DefaultTerminal;
use ratatui::prelude::*;

use visidata_core::{ColumnType, Sheet, SheetStack};

use crate::input::{EditResult, LineEditor};
use crate::renderer;

/// What the input line is being used for.
#[derive(Debug, Clone)]
enum InputMode {
    /// Not in input mode — normal sheet navigation.
    Normal,
    /// Renaming the current column.
    RenameColumn(LineEditor),
}

/// Application state for the TUI.
#[derive(Debug)]
pub struct App {
    /// Sheet navigation stack.
    pub stack: SheetStack,

    /// Whether the application is still running.
    pub running: bool,

    /// Status message displayed at the bottom.
    pub status: String,

    /// Current input mode.
    mode: InputMode,
}

impl App {
    /// Create a new app with an initial sheet.
    #[must_use]
    pub fn new(sheet: Sheet) -> Self {
        let mut stack = SheetStack::new();
        stack.push(sheet);
        Self {
            stack,
            running: true,
            status: String::new(),
            mode: InputMode::Normal,
        }
    }

    /// Run the application event loop.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal setup or I/O fails.
    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        let mut terminal = ratatui::init();

        let result = self.event_loop(&mut terminal);

        ratatui::restore();
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen)?;

        result
    }

    /// Main event loop: draw, wait for event, handle it, repeat.
    fn event_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.update_status();

        while self.running {
            terminal.draw(|frame| self.draw(frame))?;

            if let Event::Key(key) = event::read()? {
                match &mut self.mode {
                    InputMode::Normal => self.handle_normal_key(key),
                    InputMode::RenameColumn(_) => self.handle_input_key(key),
                }
            }
        }
        Ok(())
    }

    /// Draw the current state to the terminal frame.
    fn draw(&self, frame: &mut Frame<'_>) {
        let area = frame.area();

        if let Some(sheet) = self.stack.active() {
            let input_text = match &self.mode {
                InputMode::Normal => None,
                InputMode::RenameColumn(editor) => {
                    Some(format!("rename column: {}", editor.text()))
                }
            };
            renderer::draw_sheet(frame, area, sheet, &self.status, input_text.as_deref());
        } else {
            let text = Text::raw("No sheets open. Press q to quit.");
            frame.render_widget(text, area);
        }
    }

    /// Handle a key event in normal mode.
    fn handle_normal_key(&mut self, key: KeyEvent) {
        // Ctrl-C always quits
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }

        let Some(sheet) = self.stack.active_mut() else {
            self.running = false;
            return;
        };

        let height = 20_usize; // TODO: get from terminal size

        match key.code {
            // Quit / pop sheet
            KeyCode::Char('q') => {
                self.stack.pop();
                if self.stack.is_empty() {
                    self.running = false;
                }
            }

            // Cursor movement
            KeyCode::Down | KeyCode::Char('j') => sheet.cursor_down(1),
            KeyCode::Up | KeyCode::Char('k') => sheet.cursor_up(1),
            KeyCode::Right | KeyCode::Char('l') => sheet.cursor_right(1),
            KeyCode::Left | KeyCode::Char('h') => sheet.cursor_left(1),

            // Page movement
            KeyCode::PageDown => sheet.cursor_down(height.saturating_sub(2)),
            KeyCode::PageUp => sheet.cursor_up(height.saturating_sub(2)),

            // Home / go to top
            KeyCode::Home | KeyCode::Char('g') => {
                sheet.cursor_row = 0;
                sheet.top_row = 0;
            }
            KeyCode::End => {
                if !sheet.rows.is_empty() {
                    sheet.cursor_row = sheet.rows.len() - 1;
                }
            }

            // --- Column operations ---

            // Auto-fit column width
            KeyCode::Char('_') => {
                auto_fit_column(sheet);
            }

            // Hide column (width = 0)
            KeyCode::Char('-') => {
                let vis = sheet.visible_columns();
                if let Some(&col) = vis.get(sheet.cursor_col) {
                    let col_idx = sheet
                        .columns
                        .iter()
                        .position(|c| c.id == col.id);
                    if let Some(idx) = col_idx {
                        sheet.columns[idx].width = Some(0);
                        sheet.clamp_cursor();
                    }
                }
            }

            // Rename column
            KeyCode::Char('^') => {
                let vis = sheet.visible_columns();
                if let Some(&col) = vis.get(sheet.cursor_col) {
                    let editor = LineEditor::new(&col.name);
                    self.mode = InputMode::RenameColumn(editor);
                    return; // skip status update below
                }
            }

            // Key column toggle
            KeyCode::Char('!') => {
                let vis = sheet.visible_columns();
                if let Some(&col) = vis.get(sheet.cursor_col) {
                    let col_idx = sheet
                        .columns
                        .iter()
                        .position(|c| c.id == col.id);
                    if let Some(idx) = col_idx {
                        sheet.columns[idx].is_key = !sheet.columns[idx].is_key;
                        if sheet.columns[idx].is_key {
                            sheet.num_keys += 1;
                        } else {
                            sheet.num_keys = sheet.num_keys.saturating_sub(1);
                        }
                    }
                }
            }

            // --- Type conversion ---
            KeyCode::Char('#') => set_cursor_col_type(sheet, ColumnType::Int),
            KeyCode::Char('%') => set_cursor_col_type(sheet, ColumnType::Float),
            KeyCode::Char('$') => set_cursor_col_type(sheet, ColumnType::Currency),
            KeyCode::Char('~') => set_cursor_col_type(sheet, ColumnType::Text),
            KeyCode::Char('@') => set_cursor_col_type(sheet, ColumnType::Date),

            _ => {}
        }

        self.update_status();
    }

    /// Handle a key event while in input mode.
    fn handle_input_key(&mut self, key: KeyEvent) {
        let key_str = key_event_to_string(&key);

        // Extract editor, process keystroke
        let mode = std::mem::replace(&mut self.mode, InputMode::Normal);
        match mode {
            InputMode::RenameColumn(mut editor) => {
                match editor.handle_key(&key_str) {
                    EditResult::Accept(new_name) => {
                        // Apply rename
                        if let Some(sheet) = self.stack.active_mut() {
                            let vis = sheet.visible_columns();
                            if let Some(&col) = vis.get(sheet.cursor_col) {
                                let col_idx = sheet
                                    .columns
                                    .iter()
                                    .position(|c| c.id == col.id);
                                if let Some(idx) = col_idx {
                                    sheet.columns[idx].name = new_name;
                                }
                            }
                        }
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Cancel => {
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Continue => {
                        self.mode = InputMode::RenameColumn(editor);
                    }
                }
            }
            InputMode::Normal => unreachable!(),
        }

        self.update_status();
    }

    /// Update the status message based on current state.
    fn update_status(&mut self) {
        if let Some(sheet) = self.stack.active() {
            let selected = sheet.num_selected();
            let sel_text = if selected > 0 {
                format!(" ({selected} selected)")
            } else {
                String::new()
            };

            // Show column type indicator
            let type_indicator = sheet
                .current_column()
                .map_or("", |col| col.col_type.indicator());

            self.status = format!(
                "{} | {}r x {}c | row {} col {} {type_indicator}{}",
                sheet.name,
                sheet.num_rows(),
                sheet.visible_columns().len(),
                sheet.cursor_row + 1,
                sheet.cursor_col + 1,
                sel_text,
            );
        }
    }
}

/// Auto-fit the current column width based on data.
fn auto_fit_column(sheet: &mut Sheet) {
    use unicode_width::UnicodeWidthStr;

    let vis = sheet.visible_columns();
    let Some(&col) = vis.get(sheet.cursor_col) else {
        return;
    };
    let col_idx = sheet.columns.iter().position(|c| c.id == col.id);
    let Some(idx) = col_idx else { return };

    let header_w = UnicodeWidthStr::width(sheet.columns[idx].name.as_str());
    let max_data_w = sheet
        .rows
        .iter()
        .map(|row| {
            let display = sheet.columns[idx].display_value(row);
            crate::cliptext::dispwidth(&display)
        })
        .max()
        .unwrap_or(0);

    let width = header_w.max(max_data_w).min(80);
    #[expect(clippy::cast_possible_truncation, reason = "width clamped to 80")]
    let width = width as u16;
    sheet.columns[idx].width = Some(width);
}

/// Set the column type for the current cursor column.
fn set_cursor_col_type(sheet: &mut Sheet, col_type: ColumnType) {
    let vis = sheet.visible_columns();
    if let Some(&col) = vis.get(sheet.cursor_col) {
        let col_idx = sheet.columns.iter().position(|c| c.id == col.id);
        if let Some(idx) = col_idx {
            sheet.columns[idx].col_type = col_type;
        }
    }
}

/// Convert a crossterm `KeyEvent` to a string matching the `LineEditor` key format.
fn key_event_to_string(key: &KeyEvent) -> String {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Enter => "Enter".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Backspace => {
            if ctrl { "Ctrl+H".into() } else { "Bksp".into() }
        }
        KeyCode::Delete => {
            if ctrl { "Ctrl+Del".into() } else { "Del".into() }
        }
        KeyCode::Left => {
            if ctrl { "Ctrl+Left".into() } else { "Left".into() }
        }
        KeyCode::Right => {
            if ctrl { "Ctrl+Right".into() } else { "Right".into() }
        }
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::Char(c) if ctrl => format!("Ctrl+{}", c.to_uppercase()),
        KeyCode::Char(c) => c.to_string(),
        _ => String::new(),
    }
}
