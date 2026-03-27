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

use visidata_core::{Sheet, SheetStack};

use crate::renderer;

/// Application state for the TUI.
#[derive(Debug)]
pub struct App {
    /// Sheet navigation stack.
    pub stack: SheetStack,

    /// Whether the application is still running.
    pub running: bool,

    /// Status message displayed at the bottom.
    pub status: String,
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
        while self.running {
            terminal.draw(|frame| self.draw(frame))?;

            if let Event::Key(key) = event::read()? {
                self.handle_key(key);
            }
        }
        Ok(())
    }

    /// Draw the current state to the terminal frame.
    fn draw(&self, frame: &mut Frame<'_>) {
        let area = frame.area();

        if let Some(sheet) = self.stack.active() {
            renderer::draw_sheet(frame, area, sheet, &self.status);
        } else {
            let text = Text::raw("No sheets open. Press q to quit.");
            frame.render_widget(text, area);
        }
    }

    /// Handle a key event.
    fn handle_key(&mut self, key: KeyEvent) {
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

            // Home / End
            KeyCode::Home => {
                sheet.cursor_row = 0;
                sheet.top_row = 0;
            }
            KeyCode::End => {
                if !sheet.rows.is_empty() {
                    sheet.cursor_row = sheet.rows.len() - 1;
                }
            }

            // Go to first/last column
            KeyCode::Char('g') => {
                // 'g' prefix commands — simplified: gg = top, gEnd = bottom
                // For now just go to top
                sheet.cursor_row = 0;
                sheet.top_row = 0;
            }

            _ => {}
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
            self.status = format!(
                "{} | {}r x {}c | row {} col {}{}",
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
