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

use visidata_core::{
    ColumnType, CommandRegistry, Sheet, SheetStack, SortDirection, builtin_commands,
    options::OptionsManager,
};

use crate::input::{EditResult, LineEditor};
use crate::renderer;
use crate::theme::Theme;

/// What the input line is being used for.
#[derive(Debug, Clone)]
enum InputMode {
    /// Not in input mode — normal sheet navigation.
    Normal,
    /// Renaming the current column.
    RenameColumn(LineEditor),
    /// Forward search in current column.
    SearchForward(LineEditor),
    /// Backward search in current column.
    SearchBackward(LineEditor),
    /// Command palette (fuzzy search by longname).
    CommandPalette(LineEditor),
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

    /// Last search pattern (for `n`/`N` repeat).
    last_search: Option<String>,

    /// Whether last search was forward (true) or backward (false).
    last_search_forward: bool,

    /// Command registry.
    pub commands: CommandRegistry,

    /// Options manager.
    pub options: OptionsManager,

    /// Color theme.
    pub theme: Theme,
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
            last_search: None,
            last_search_forward: true,
            commands: builtin_commands(),
            options: visidata_core::options::builtin_options(),
            theme: Theme::default(),
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
                match &self.mode {
                    InputMode::Normal => self.handle_normal_key(key),
                    InputMode::RenameColumn(_)
                    | InputMode::SearchForward(_)
                    | InputMode::SearchBackward(_)
                    | InputMode::CommandPalette(_) => self.handle_input_key(key),
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
                InputMode::SearchForward(editor) => Some(format!("/{}", editor.text())),
                InputMode::SearchBackward(editor) => Some(format!("?{}", editor.text())),
                InputMode::CommandPalette(editor) => {
                    let query = editor.text();
                    let matches = self.commands.search_commands(&query);
                    let hint = matches.first().map_or("", |c| c.longname.as_str());
                    Some(format!("command: {query}  → {hint}"))
                }
            };
            renderer::draw_sheet(
                frame,
                area,
                sheet,
                &self.status,
                input_text.as_deref(),
                &self.theme,
            );
        } else {
            let text = Text::raw("No sheets open. Press q to quit.");
            frame.render_widget(text, area);
        }
    }

    /// Handle a key event in normal mode.
    #[expect(
        clippy::too_many_lines,
        reason = "single match dispatch — splitting would reduce readability"
    )]
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
                    let col_idx = sheet.columns.iter().position(|c| c.id == col.id);
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
                    let col_idx = sheet.columns.iter().position(|c| c.id == col.id);
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

            // --- Sorting ---
            KeyCode::Char('[') => {
                let col_idx = resolve_cursor_col_idx(sheet);
                if let Some(idx) = col_idx {
                    sheet.sort_by(idx, SortDirection::Ascending);
                }
            }
            KeyCode::Char(']') => {
                let col_idx = resolve_cursor_col_idx(sheet);
                if let Some(idx) = col_idx {
                    sheet.sort_by(idx, SortDirection::Descending);
                }
            }

            // --- Selection ---
            KeyCode::Char('s') => {
                sheet.select_current();
                sheet.cursor_down(1);
            }
            KeyCode::Char('u') => {
                sheet.unselect_current();
                sheet.cursor_down(1);
            }
            KeyCode::Char('t') => {
                sheet.toggle_select_current();
                sheet.cursor_down(1);
            }

            // --- Search ---
            KeyCode::Char('/') => {
                self.mode = InputMode::SearchForward(LineEditor::new(""));
                return;
            }
            KeyCode::Char('?') => {
                self.mode = InputMode::SearchBackward(LineEditor::new(""));
                return;
            }
            KeyCode::Char('n') => {
                self.repeat_search(true);
            }
            KeyCode::Char('N') => {
                self.repeat_search(false);
            }

            // --- Filter (selected rows to new sheet) ---
            KeyCode::Char('"') => {
                if sheet.num_selected() > 0 {
                    let filtered = sheet.selected_rows_sheet();
                    self.stack.push(filtered);
                }
            }

            // --- Frequency table ---
            KeyCode::Char('F') => {
                let col_idx = resolve_cursor_col_idx(sheet);
                if let Some(idx) = col_idx {
                    let freq = sheet.frequency_sheet(idx);
                    self.stack.push(freq);
                }
            }

            // --- Sheet types ---
            KeyCode::Char('O') => {
                let sheet = visidata_core::options::options_sheet(&self.options);
                self.stack.push(sheet);
            }
            KeyCode::Char('C') => {
                let meta = visidata_core::sheets::columns_sheet(sheet);
                self.stack.push(meta);
            }
            KeyCode::Char('I') => {
                let desc = visidata_core::sheets::describe_sheet(sheet);
                self.stack.push(desc);
            }
            KeyCode::Char('S') => {
                let all: Vec<&Sheet> = self.stack.iter().collect();
                let index = visidata_core::sheets::sheets_sheet(&all);
                self.stack.push(index);
            }

            // --- Help & Command palette ---
            KeyCode::Char(':' | ' ') => {
                self.mode = InputMode::CommandPalette(LineEditor::new(""));
                return;
            }

            _ => {}
        }

        self.update_status();
    }

    /// Handle the help command — push help sheet onto stack.
    fn show_help(&mut self) {
        let sheet = visidata_core::help::help_sheet(&self.commands);
        self.stack.push(sheet);
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
                                let col_idx = sheet.columns.iter().position(|c| c.id == col.id);
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
            InputMode::SearchForward(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(pattern) => {
                    self.last_search = Some(pattern.clone());
                    self.last_search_forward = true;
                    self.execute_search(&pattern, true);
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => {
                    self.mode = InputMode::Normal;
                }
                EditResult::Continue => {
                    self.mode = InputMode::SearchForward(editor);
                }
            },
            InputMode::SearchBackward(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(pattern) => {
                    self.last_search = Some(pattern.clone());
                    self.last_search_forward = false;
                    self.execute_search(&pattern, false);
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => {
                    self.mode = InputMode::Normal;
                }
                EditResult::Continue => {
                    self.mode = InputMode::SearchBackward(editor);
                }
            },
            InputMode::CommandPalette(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(query) => {
                    self.execute_command_by_query(&query);
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => {
                    self.mode = InputMode::Normal;
                }
                EditResult::Continue => {
                    self.mode = InputMode::CommandPalette(editor);
                }
            },
            InputMode::Normal => unreachable!(),
        }

        self.update_status();
    }

    /// Execute a command by searching the registry for a query match.
    fn execute_command_by_query(&mut self, query: &str) {
        if query.is_empty() {
            return;
        }

        // Try exact longname match first
        if self.commands.lookup_by_name(query).is_some() {
            self.dispatch_command(query);
            return;
        }

        // Fuzzy search — execute first match
        let matches = self.commands.search_commands(query);
        if let Some(cmd) = matches.first() {
            let longname = cmd.longname.clone();
            self.dispatch_command(&longname);
        } else {
            self.status = format!("no command: {query}");
        }
    }

    /// Dispatch a command by longname.
    #[expect(clippy::too_many_lines, reason = "single match dispatch table")]
    fn dispatch_command(&mut self, longname: &str) {
        match longname {
            "help-commands" => self.show_help(),
            "quit-sheet" => {
                self.stack.pop();
                if self.stack.is_empty() {
                    self.running = false;
                }
            }
            "cursor-down" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_down(1);
                }
            }
            "cursor-up" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_up(1);
                }
            }
            "cursor-right" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_right(1);
                }
            }
            "cursor-left" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_left(1);
                }
            }
            "go-top" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_row = 0;
                    s.top_row = 0;
                }
            }
            "go-bottom" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                {
                    s.cursor_row = s.rows.len() - 1;
                }
            }
            "sort-asc" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                {
                    s.sort_by(idx, SortDirection::Ascending);
                }
            }
            "sort-desc" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                {
                    s.sort_by(idx, SortDirection::Descending);
                }
            }
            "select-row" => {
                if let Some(s) = self.stack.active_mut() {
                    s.select_current();
                    s.cursor_down(1);
                }
            }
            "unselect-row" => {
                if let Some(s) = self.stack.active_mut() {
                    s.unselect_current();
                    s.cursor_down(1);
                }
            }
            "toggle-row" => {
                if let Some(s) = self.stack.active_mut() {
                    s.toggle_select_current();
                    s.cursor_down(1);
                }
            }
            "search-col" => {
                self.mode = InputMode::SearchForward(LineEditor::new(""));
            }
            "search-col-backward" => {
                self.mode = InputMode::SearchBackward(LineEditor::new(""));
            }
            "search-next" => self.repeat_search(true),
            "search-prev" => self.repeat_search(false),
            "dup-selected" => {
                if let Some(s) = self.stack.active()
                    && s.num_selected() > 0
                {
                    let filtered = s.selected_rows_sheet();
                    self.stack.push(filtered);
                }
            }
            "freq-col" => {
                if let Some(s) = self.stack.active()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                {
                    let freq = s.frequency_sheet(idx);
                    self.stack.push(freq);
                }
            }
            "resize-col-max" => {
                if let Some(s) = self.stack.active_mut() {
                    auto_fit_column(s);
                }
            }
            "rename-col" => {
                if let Some(s) = self.stack.active() {
                    let vis = s.visible_columns();
                    if let Some(&col) = vis.get(s.cursor_col) {
                        self.mode = InputMode::RenameColumn(LineEditor::new(&col.name));
                    }
                }
            }
            _ => {
                self.status = format!("unknown command: {longname}");
            }
        }
    }

    /// Execute a search in the given direction.
    fn execute_search(&mut self, pattern: &str, forward: bool) {
        if pattern.is_empty() {
            return;
        }
        let Some(sheet) = self.stack.active_mut() else {
            return;
        };
        let col_idx = resolve_cursor_col_idx(sheet).unwrap_or(0);
        let result = if forward {
            sheet.search_forward(col_idx, pattern)
        } else {
            sheet.search_backward(col_idx, pattern)
        };
        if let Some(row_idx) = result {
            sheet.cursor_row = row_idx;
        } else {
            self.status = format!("not found: {pattern}");
        }
    }

    /// Repeat the last search in the same or opposite direction.
    fn repeat_search(&mut self, same_direction: bool) {
        let Some(pattern) = self.last_search.clone() else {
            return;
        };
        let forward = if same_direction {
            self.last_search_forward
        } else {
            !self.last_search_forward
        };
        self.execute_search(&pattern, forward);
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

/// Resolve the actual column index for the cursor's visible column position.
fn resolve_cursor_col_idx(sheet: &Sheet) -> Option<usize> {
    let vis = sheet.visible_columns();
    let col = vis.get(sheet.cursor_col)?;
    sheet.columns.iter().position(|c| c.id == col.id)
}

/// Convert a crossterm `KeyEvent` to a string matching the `LineEditor` key format.
fn key_event_to_string(key: &KeyEvent) -> String {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);

    match key.code {
        KeyCode::Enter => "Enter".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::Backspace => {
            if ctrl {
                "Ctrl+H".into()
            } else {
                "Bksp".into()
            }
        }
        KeyCode::Delete => {
            if ctrl {
                "Ctrl+Del".into()
            } else {
                "Del".into()
            }
        }
        KeyCode::Left => {
            if ctrl {
                "Ctrl+Left".into()
            } else {
                "Left".into()
            }
        }
        KeyCode::Right => {
            if ctrl {
                "Ctrl+Right".into()
            } else {
                "Right".into()
            }
        }
        KeyCode::Home => "Home".into(),
        KeyCode::End => "End".into(),
        KeyCode::Char(c) if ctrl => format!("Ctrl+{}", c.to_uppercase()),
        KeyCode::Char(c) => c.to_string(),
        _ => String::new(),
    }
}
