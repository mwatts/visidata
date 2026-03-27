//! Application state and main event loop.

use std::io;

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::DefaultTerminal;
use ratatui::prelude::*;

use visidata_core::{
    ColumnType, CommandRegistry, Sheet, SheetStack, SortDirection,
    async_loader::LoadHandle,
    builtin_commands,
    clipboard::Clipboard,
    menu::{MenuBar, MenuItem, MenuState, builtin_menu_bar},
    options::OptionsManager,
};
use visidata_scripting::ScriptEngine;

use crate::input::{EditResult, LineEditor};
use crate::renderer;
use crate::theme::Theme;

/// System clipboard operation mode.
#[derive(Debug, Clone, Copy)]
enum SysClipMode {
    Cell,
    Row,
    Selected,
}

/// Write text to the OS clipboard via pbcopy / xclip / clip.
fn sys_clipboard_write(text: &str) -> anyhow::Result<()> {
    use std::io::Write;
    #[cfg(target_os = "macos")]
    let cmd = "pbcopy";
    #[cfg(target_os = "linux")]
    let cmd = "xclip";
    #[cfg(target_os = "windows")]
    let cmd = "clip";
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return Err(anyhow::anyhow!("unsupported platform for clipboard"));

    let mut child = std::process::Command::new(cmd)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("{cmd} not found: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    child.wait()?;
    Ok(())
}

/// Read text from the OS clipboard via pbpaste / xclip / powershell.
fn sys_clipboard_read() -> anyhow::Result<String> {
    #[cfg(target_os = "macos")]
    let (cmd, args): (&str, &[&str]) = ("pbpaste", &[]);
    #[cfg(target_os = "linux")]
    let (cmd, args): (&str, &[&str]) = ("xclip", &["-selection", "clipboard", "-o"]);
    #[cfg(target_os = "windows")]
    let (cmd, args): (&str, &[&str]) = ("powershell", &["-command", "Get-Clipboard"]);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    return Err(anyhow::anyhow!("unsupported platform for clipboard"));

    let out = std::process::Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("{cmd} not found: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Which columns a search targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum SearchScope {
    /// Current cursor column only.
    #[default]
    CurrentCol,
    /// First key column (if any), falling back to current col.
    KeyCols,
}

/// What the input line is being used for.
#[derive(Debug, Clone)]
enum InputMode {
    /// Not in input mode — normal sheet navigation.
    Normal,
    /// Renaming the current column.
    RenameColumn(LineEditor),
    /// Editing a cell value.
    EditCell(LineEditor),
    /// Forward search in current column.
    SearchForward(LineEditor),
    /// Backward search in current column.
    SearchBackward(LineEditor),
    /// Forward search across all visible columns.
    SearchForwardAllCols(LineEditor),
    /// Backward search across all visible columns.
    SearchBackwardAllCols(LineEditor),
    /// Select/unselect rows matching regex in current column (`|` / `\`).
    SelectColRegex { editor: LineEditor, select: bool },
    /// Select/unselect rows matching regex in any visible column (`g|` / `g\`).
    SelectAllColsRegex { editor: LineEditor, select: bool },
    /// Name a just-recorded macro.
    NameMacroInput(LineEditor),
    /// Go to row by number (`zr`).
    GotoRow(LineEditor),
    /// Go to column by regex name match (`c`).
    GotoColRegex(LineEditor),
    /// Go to column by index (`zc`).
    GotoColNumber(LineEditor),
    /// Set all selected rows' current column to a typed value (`ge`).
    SetColInput(LineEditor),
    /// Resize column to a specific width (`z_`).
    ResizeColInput(LineEditor),
    /// Add N blank rows (`ga`).
    AddRowsInput(LineEditor),
    /// Command palette (fuzzy search by longname).
    CommandPalette(LineEditor),
    /// Menu navigation mode.
    Menu,
    /// Floating keybindings help overlay.
    Help { scroll: usize },
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

    /// Which columns the last search targeted.
    last_search_scope: SearchScope,

    /// Command registry.
    pub commands: CommandRegistry,

    /// Options manager.
    pub options: OptionsManager,

    /// Color theme.
    pub theme: Theme,

    /// Active background loading handle, if any.
    load_handle: Option<LoadHandle>,

    /// Menu bar.
    pub menu_bar: MenuBar,

    /// Menu navigation state.
    menu_state: MenuState,

    /// Clipboard for yank/paste.
    clipboard: Clipboard,

    /// Pending prefix for multi-key bindings (e.g., "g" in "gd", "gz" in "gzd").
    pending_prefix: Option<String>,

    /// Macro recorder — accumulates keystrokes while recording.
    macro_recorder: visidata_core::macros::MacroRecorder,

    /// Macro store — holds named macros.
    macro_store: visidata_core::macros::MacroStore,

    /// Keystrokes queued for replay.
    macro_replay: Vec<String>,

    /// Redo stack: actions popped by undo, waiting to be re-applied.
    redo_stack: Vec<visidata_core::undo::UndoAction>,

    /// Index of the previously active sheet (for Ctrl+^ jump-prev).
    prev_sheet_idx: Option<usize>,

    /// Recent error messages (capped at 50) for the error sheet.
    last_errors: std::collections::VecDeque<String>,

    /// Whether quitguard is waiting for a second `q` press.
    pending_quit: bool,

    /// Rhai scripting engine — shared across all expression evaluations.
    script_engine: ScriptEngine,

    /// All sheets ever pushed in this session (names, for gS).
    all_sheet_names: Vec<String>,

    /// Command log: (longname, key-string) pairs for Ctrl+D save.
    command_log: Vec<(String, String)>,
}

impl App {
    /// Create a new app with an initial sheet and pre-configured options.
    ///
    /// Used by `main` to pass in options that were loaded from config before
    /// the initial file was opened (so loader options apply to the first load).
    #[must_use]
    pub fn new_with_options(sheet: Sheet, options: visidata_core::options::OptionsManager) -> Self {
        let mut app = Self::new(sheet);
        app.options = options;
        app
    }

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
            last_search_scope: SearchScope::default(),
            commands: builtin_commands(),
            options: visidata_core::options::builtin_options(),
            theme: Theme::default(),
            load_handle: None,
            menu_bar: builtin_menu_bar(),
            menu_state: MenuState::new(),
            clipboard: Clipboard::new(),
            pending_prefix: None::<String>,
            macro_recorder: visidata_core::macros::MacroRecorder::new(),
            macro_store: visidata_core::macros::MacroStore::load_from_disk().unwrap_or_default(),
            macro_replay: vec![],
            redo_stack: vec![],
            prev_sheet_idx: None,
            last_errors: std::collections::VecDeque::new(),
            pending_quit: false,
            script_engine: ScriptEngine::new(),
            all_sheet_names: Vec::new(),
            command_log: Vec::new(),
        }
    }

    /// Run the application event loop.
    ///
    /// # Errors
    ///
    /// Returns an error if terminal setup or I/O fails.
    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
        let mut terminal = ratatui::init();

        let result = self.event_loop(&mut terminal);

        ratatui::restore();
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

        result
    }

    /// Main event loop: draw, wait for event, handle it, repeat.
    fn event_loop(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        self.update_status();

        while self.running {
            terminal.draw(|frame| self.draw(frame))?;

            // Replay macro keystrokes (drain one per iteration to keep UI responsive).
            if !self.macro_replay.is_empty() {
                self.replay_next_macro_key();
                continue;
            }

            // Poll for background loading data.
            self.poll_loader();

            // Poll for keyboard events with a timeout so we can refresh
            // during background loads.
            let timeout = if self.load_handle.is_some() {
                std::time::Duration::from_millis(50)
            } else {
                std::time::Duration::from_millis(500)
            };

            if event::poll(timeout)? {
                match event::read()? {
                    Event::Key(key) => match &self.mode {
                        InputMode::Normal => self.handle_normal_key(key),
                        InputMode::Menu => self.handle_menu_key(key),
                        InputMode::Help { .. } => self.handle_help_key(key),
                        InputMode::RenameColumn(_)
                        | InputMode::EditCell(_)
                        | InputMode::SearchForward(_)
                        | InputMode::SearchBackward(_)
                        | InputMode::SearchForwardAllCols(_)
                        | InputMode::SearchBackwardAllCols(_)
                        | InputMode::SelectColRegex { .. }
                        | InputMode::SelectAllColsRegex { .. }
                        | InputMode::NameMacroInput(_)
                        | InputMode::GotoRow(_)
                        | InputMode::GotoColRegex(_)
                        | InputMode::GotoColNumber(_)
                        | InputMode::SetColInput(_)
                        | InputMode::ResizeColInput(_)
                        | InputMode::AddRowsInput(_)
                        | InputMode::CommandPalette(_) => self.handle_input_key(key),
                    },
                    Event::Mouse(mouse) => self.handle_mouse(mouse),
                    Event::Resize(_, _) => {
                        // Terminal resized — just redraw on next iteration.
                        self.update_status();
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Poll the background loader for new data.
    fn poll_loader(&mut self) {
        use visidata_core::async_loader::{LoadMessage, LoadingState};

        let Some(handle) = &self.load_handle else {
            return;
        };

        // Drain all available messages without blocking.
        loop {
            match handle.receiver.try_recv() {
                Ok(LoadMessage::Columns(cols)) => {
                    if let Some(sheet) = self.stack.active_mut() {
                        sheet.columns = cols;
                    }
                }
                Ok(LoadMessage::Rows(rows)) => {
                    if let Some(sheet) = self.stack.active_mut() {
                        let count = rows.len();
                        sheet.rows.extend(rows);
                        if let LoadingState::Loading { rows_loaded, .. } = &mut sheet.loading_state {
                            *rows_loaded += count;
                        }
                    }
                }
                Ok(LoadMessage::Done) => {
                    if let Some(sheet) = self.stack.active_mut() {
                        let total = sheet.num_rows();
                        sheet.loading_state = LoadingState::Complete { total_rows: total };
                    }
                    self.load_handle = None;
                    self.update_status();
                    break;
                }
                Ok(LoadMessage::Error(e)) => {
                    self.status = format!("load error: {e}");
                    if let Some(sheet) = self.stack.active_mut() {
                        let total = sheet.num_rows();
                        sheet.loading_state = LoadingState::Complete { total_rows: total };
                    }
                    self.load_handle = None;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.load_handle = None;
                    break;
                }
            }
        }

        self.update_status();
    }

    /// Draw the current state to the terminal frame.
    fn draw(&mut self, frame: &mut Frame<'_>) {
        // Sync theme from options each frame (GAP-122 + GAP-107)
        self.theme = Theme::from_options(&self.options);
        if let visidata_core::Value::Text(ref name) = self.options.get_global("theme") {
            let name = name.clone();
            self.theme = Theme::by_name(&name);
        }
        // Apply display formats from options to active sheet (GAP-106)
        self.apply_display_formats_from_options();
        let area = frame.area();

        if let Some(sheet) = self.stack.active() {
            // Overlays (Menu, Help, CommandPalette) draw themselves on top of the
            // sheet. For those modes we pass `None` as the input line so no input
            // bar is shown in the sheet area.
            let input_text: Option<String> = match &self.mode {
                InputMode::RenameColumn(editor) => {
                    Some(format!("rename column: {}", editor.text()))
                }
                InputMode::EditCell(editor) => Some(format!("edit: {}", editor.text())),
                InputMode::SearchForward(editor) => Some(format!("/{}", editor.text())),
                InputMode::SearchBackward(editor) => Some(format!("?{}", editor.text())),
                InputMode::SearchForwardAllCols(editor) => {
                    Some(format!("g/{}", editor.text()))
                }
                InputMode::SearchBackwardAllCols(editor) => {
                    Some(format!("g?{}", editor.text()))
                }
                InputMode::SelectColRegex { editor, select } => {
                    let prefix = if *select { "|" } else { "\\" };
                    Some(format!("{prefix}{}", editor.text()))
                }
                InputMode::SelectAllColsRegex { editor, select } => {
                    let prefix = if *select { "g|" } else { "g\\" };
                    Some(format!("{prefix}{}", editor.text()))
                }
                InputMode::NameMacroInput(editor) => Some(format!("name macro: {}", editor.text())),
                InputMode::GotoRow(editor) => Some(format!("go to row: {}", editor.text())),
                InputMode::GotoColRegex(editor) => Some(format!("go to column: {}", editor.text())),
                InputMode::GotoColNumber(editor) => Some(format!("go to column #: {}", editor.text())),
                InputMode::SetColInput(editor) => Some(format!("set column: {}", editor.text())),
                InputMode::ResizeColInput(editor) => Some(format!("column width: {}", editor.text())),
                InputMode::AddRowsInput(editor) => Some(format!("add N rows: {}", editor.text())),
                // These modes render as overlays; suppress the inline input bar.
                InputMode::Normal
                | InputMode::CommandPalette(_)
                | InputMode::Menu
                | InputMode::Help { .. } => None,
            };

            renderer::draw_sheet(
                frame,
                area,
                sheet,
                &self.status,
                input_text.as_deref(),
                &self.theme,
                self.script_engine.engine(),
            );

            // Render overlay modes on top of the sheet.
            if let InputMode::Help { scroll } = self.mode {
                renderer::draw_help_overlay(frame, area, &self.commands, scroll, &self.theme);
            }

            if matches!(self.mode, InputMode::Menu) {
                renderer::draw_menu_overlay(
                    frame,
                    area,
                    &self.menu_bar,
                    &self.menu_state,
                    &self.theme,
                );
            }

            if let InputMode::CommandPalette(ref editor) = self.mode {
                let query = editor.text();
                let matches = self.commands.search_commands(&query);
                renderer::draw_command_palette(frame, area, &query, &matches, &self.theme);
            }
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
        // Ctrl-C: cancel active load first, quit on second press (GAP-111)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            if self.load_handle.is_some() {
                if let Some(handle) = self.load_handle.take() {
                    handle.cancel();
                }
                self.status = "load cancelled".into();
                return;
            }
            self.running = false;
            return;
        }

        // Ctrl+Z = undo
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('z') {
            self.do_undo();
            self.update_status();
            return;
        }

        // Ctrl+S = save
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            self.save_current_sheet();
            return;
        }

        // Ctrl+Right / Ctrl+Left = scroll columns by page (GAP-010)
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            if key.code == KeyCode::Right {
                if let Some(s) = self.stack.active_mut() {
                    s.left_col = (s.left_col + 5).min(s.visible_columns().len().saturating_sub(1));
                    s.cursor_col = s.left_col;
                }
                self.update_status();
                return;
            }
            if key.code == KeyCode::Left {
                if let Some(s) = self.stack.active_mut() {
                    s.left_col = s.left_col.saturating_sub(5);
                    s.cursor_col = s.left_col;
                }
                self.update_status();
                return;
            }
        }

        // Ctrl+L = redraw
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
            self.dispatch_command("redraw");
            return;
        }

        // Ctrl+R = reload sheet
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            self.dispatch_command("reload-sheet");
            return;
        }

        // Ctrl+S with g-prefix pending = save all sheets (GAP-097)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s')
            && self.pending_prefix == Some("g".into())
        {
            self.pending_prefix = None;
            self.dispatch_command("save-all");
            return;
        }

        // Ctrl+D = save command log (GAP-091)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('d') {
            self.dispatch_command("save-cmdlog");
            return;
        }

        // Ctrl+O = open cell in external editor (GAP-055)
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('o') {
            self.dispatch_command("sysedit-cell");
            return;
        }

        // Ctrl+^ = jump to previous sheet
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('^') {
            self.dispatch_command("jump-prev");
            return;
        }

        // Ctrl+E = error sheet
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('e') {
            self.dispatch_command("error-recent");
            return;
        }

        // Handle pending prefix (g/z/gz-prefixed commands)
        if let Some(prefix) = self.pending_prefix.take() {
            let prefix = prefix.as_str();
            match (prefix, key.code) {
                // --- g-prefixed ---
                ("g", KeyCode::Char('d')) => {
                    if let Some(s) = self.stack.active_mut() {
                        let count = s.delete_selected_rows();
                        self.status = format!("deleted {count} rows");
                    }
                    self.update_status();
                    return;
                }
                ("g", KeyCode::Char('e')) => {
                    self.mode = InputMode::SetColInput(LineEditor::new(""));
                    return;
                }
                ("g", KeyCode::Char('=')) => {
                    self.mode = InputMode::CommandPalette(LineEditor::new("g="));
                    return;
                }
                ("g", KeyCode::Char('^')) => {
                    self.dispatch_command("rename-cols-row");
                    return;
                }
                ("g", KeyCode::Char('j')) => {
                    if let Some(s) = self.stack.active_mut()
                        && !s.rows.is_empty()
                    {
                        s.cursor_row = s.rows.len() - 1;
                    }
                    self.update_status();
                    return;
                }
                ("g", KeyCode::Char('k')) => {
                    if let Some(s) = self.stack.active_mut() {
                        s.cursor_row = 0;
                        s.top_row = 0;
                    }
                    self.update_status();
                    return;
                }
                ("g", KeyCode::Char('a')) => {
                    self.mode = InputMode::AddRowsInput(LineEditor::new("1"));
                    return;
                }
                ("g", KeyCode::Char('A')) => {
                    self.dispatch_command("concat-sheets");
                    return;
                }
                ("g", KeyCode::Char('O')) => {
                    self.dispatch_command("open-config");
                    return;
                }
                ("g", KeyCode::Char('S')) => {
                    self.dispatch_command("sheets-all");
                    return;
                }
                ("g", KeyCode::Char('m')) => {
                    self.dispatch_command("macro-sheet");
                    return;
                }
                ("g", KeyCode::Char('&')) => {
                    self.dispatch_command("join-sheets-all");
                    return;
                }
                ("g", KeyCode::Char('M')) => {
                    self.mode = InputMode::CommandPalette(LineEditor::new("melt-regex:"));
                    return;
                }
                ("g", KeyCode::Char('Y')) => {
                    self.dispatch_command("syscopy-row");
                    return;
                }
                ("g", KeyCode::Char('F')) => {
                    self.dispatch_command("freq-keys");
                    return;
                }
                ("g", KeyCode::Char('I')) => {
                    self.dispatch_command("describe-all");
                    return;
                }
                ("g", KeyCode::Char('s')) => {
                    if let Some(s) = self.stack.active_mut() {
                        s.select_all();
                        self.status = format!("selected {} rows", s.rows.len());
                    }
                    self.update_status();
                    return;
                }
                ("g", KeyCode::Char('u')) => {
                    if let Some(s) = self.stack.active_mut() {
                        s.unselect_all();
                    }
                    self.update_status();
                    return;
                }
                ("g", KeyCode::Char('t')) => {
                    if let Some(s) = self.stack.active_mut() {
                        s.toggle_select_all();
                    }
                    self.update_status();
                    return;
                }
                ("g", KeyCode::Char('H')) => {
                    self.dispatch_command("slide-leftmost");
                    return;
                }
                ("g", KeyCode::Char('L')) => {
                    self.dispatch_command("slide-rightmost");
                    return;
                }
                ("g", KeyCode::Char('J')) => {
                    self.dispatch_command("slide-row-bottom");
                    return;
                }
                ("g", KeyCode::Char('K')) => {
                    self.dispatch_command("slide-row-top");
                    return;
                }
                ("g", KeyCode::Char('x')) => {
                    self.dispatch_command("cut-selected");
                    return;
                }
                ("g", KeyCode::Char('y')) => {
                    self.dispatch_command("yank-row");
                    return;
                }
                ("g", KeyCode::Char('p')) => {
                    self.dispatch_command("paste-after");
                    return;
                }
                ("g", KeyCode::Char('_')) => {
                    self.dispatch_command("resize-cols-max");
                    return;
                }
                ("g", KeyCode::Char('v')) => {
                    self.dispatch_command("unhide-cols");
                    return;
                }
                ("g", KeyCode::Char('[')) => {
                    self.dispatch_command("sort-keys-asc");
                    return;
                }
                ("g", KeyCode::Char(']')) => {
                    self.dispatch_command("sort-keys-desc");
                    return;
                }
                ("g", KeyCode::Char('"')) => {
                    self.dispatch_command("dup-rows");
                    return;
                }
                ("g", KeyCode::Char('/')) => {
                    self.mode =
                        InputMode::SearchForwardAllCols(LineEditor::new(""));
                    return;
                }
                ("g", KeyCode::Char('?')) => {
                    self.mode =
                        InputMode::SearchBackwardAllCols(LineEditor::new(""));
                    return;
                }
                ("g", KeyCode::Char(',')) => {
                    self.dispatch_command("select-equal-row");
                    return;
                }
                ("g", KeyCode::Char('|')) => {
                    self.mode = InputMode::SelectAllColsRegex {
                        editor: LineEditor::new(""),
                        select: true,
                    };
                    return;
                }
                ("g", KeyCode::Char('\\')) => {
                    self.mode = InputMode::SelectAllColsRegex {
                        editor: LineEditor::new(""),
                        select: false,
                    };
                    return;
                }

                // --- z-prefixed ---
                ("z", KeyCode::Char('z')) => {
                    self.dispatch_command("scroll-middle");
                    return;
                }
                ("z", KeyCode::Char('d')) => {
                    self.dispatch_command("delete-cell");
                    return;
                }
                ("z", KeyCode::Char('[')) => {
                    self.dispatch_command("sort-asc-add");
                    return;
                }
                ("z", KeyCode::Char(']')) => {
                    self.dispatch_command("sort-desc-add");
                    return;
                }
                ("z", KeyCode::Char('F')) => {
                    self.dispatch_command("freq-summary");
                    return;
                }
                ("z", KeyCode::Char('+')) => {
                    self.mode = InputMode::CommandPalette(LineEditor::new("memo-agg:"));
                    return;
                }
                ("z", KeyCode::Char('O')) => {
                    self.dispatch_command("options-sheet-local");
                    return;
                }
                ("z", KeyCode::Char('a')) => {
                    self.dispatch_command("addcol-new");
                    return;
                }
                ("z", KeyCode::Char('=')) => {
                    self.mode = InputMode::CommandPalette(LineEditor::new("z="));
                    return;
                }
                ("z", KeyCode::Char('|')) => {
                    self.mode = InputMode::CommandPalette(LineEditor::new("select-expr:"));
                    return;
                }
                ("z", KeyCode::Char('\\')) => {
                    self.mode = InputMode::CommandPalette(LineEditor::new("unselect-expr:"));
                    return;
                }
                ("z", KeyCode::Char('/')) => {
                    self.mode = InputMode::CommandPalette(LineEditor::new("search-expr:"));
                    return;
                }
                ("z", KeyCode::Char('?')) => {
                    self.mode = InputMode::CommandPalette(LineEditor::new("searchr-expr:"));
                    return;
                }
                ("z", KeyCode::Char('Y')) => {
                    self.dispatch_command("syscopy-cell");
                    return;
                }
                ("z", KeyCode::Char('~')) => {
                    self.dispatch_command("type-any");
                    return;
                }
                ("z", KeyCode::Char('#')) => {
                    self.dispatch_command("type-len");
                    return;
                }
                ("z", KeyCode::Char('r')) => {
                    self.mode = InputMode::GotoRow(LineEditor::new(""));
                    return;
                }
                ("z", KeyCode::Char('c')) => {
                    self.mode = InputMode::GotoColNumber(LineEditor::new(""));
                    return;
                }
                ("z", KeyCode::Char('_')) => {
                    self.mode = InputMode::ResizeColInput(LineEditor::new(""));
                    return;
                }
                ("z", KeyCode::Char('s')) => {
                    self.dispatch_command("select-before");
                    return;
                }
                ("z", KeyCode::Char('t')) => {
                    self.dispatch_command("stoggle-before");
                    return;
                }
                ("z", KeyCode::Char('u')) => {
                    self.dispatch_command("unselect-before");
                    return;
                }
                ("z", KeyCode::Char('^')) => {
                    self.dispatch_command("rename-col-selected");
                    return;
                }
                ("z", KeyCode::Char('x')) => {
                    self.dispatch_command("cut-cell");
                    return;
                }

                // gz-prefixed commands
                ("gz", KeyCode::Char('s')) => {
                    self.dispatch_command("select-after");
                    return;
                }
                ("gz", KeyCode::Char('t')) => {
                    self.dispatch_command("stoggle-after");
                    return;
                }
                ("gz", KeyCode::Char('u')) => {
                    self.dispatch_command("unselect-after");
                    return;
                }
                ("gz", KeyCode::Char('d')) => {
                    self.dispatch_command("delete-cells");
                    return;
                }
                ("gz", KeyCode::Char('"')) => {
                    self.dispatch_command("dup-rows-deep");
                    return;
                }
                ("gz", KeyCode::Char('[')) => {
                    self.dispatch_command("sort-keys-asc-add");
                    return;
                }
                ("gz", KeyCode::Char(']')) => {
                    self.dispatch_command("sort-keys-desc-add");
                    return;
                }
                ("gz", KeyCode::Char('P')) => {
                    self.dispatch_command("syspaste-cells");
                    return;
                }
                ("gz", KeyCode::Char('Y')) => {
                    self.dispatch_command("syscopy-selected");
                    return;
                }
                // g+z chains into gz prefix
                ("g", KeyCode::Char('z')) => {
                    self.pending_prefix = Some("gz".into());
                    return;
                }

                _ => {
                    // Unknown prefixed key — ignore silently
                    self.update_status();
                    return;
                }
            }
        }

        // Enter = open-row (drill into table/view on index sheets).
        // Must be handled before `active_mut()` borrow so we can call
        // `dispatch_command` which needs `&mut self`.
        if key.code == KeyCode::Enter {
            self.dispatch_command("open-row");
            self.update_status();
            return;
        }

        let Some(sheet) = self.stack.active_mut() else {
            self.running = false;
            return;
        };

        let height = crossterm::terminal::size().map_or(20, |(_, h)| h as usize);

        match key.code {
            // Quit / pop sheet (dispatched so quitguard is respected)
            KeyCode::Char('q') => {
                self.dispatch_command("quit-sheet");
                return;
            }

            // Cursor movement
            KeyCode::Down | KeyCode::Char('j') => sheet.cursor_down(1),
            KeyCode::Up | KeyCode::Char('k') => sheet.cursor_up(1),
            KeyCode::Right | KeyCode::Char('l') => {
                sheet.cursor_right(1);
                // Scroll right if cursor moves past visible columns (GAP-010)
                let vis_count = sheet.visible_columns().len();
                let view_cols = 8usize; // conservative default; renderer uses actual widths
                let view_right = sheet.left_col + view_cols.min(vis_count);
                if sheet.cursor_col >= view_right && sheet.left_col + 1 < vis_count {
                    sheet.left_col += 1;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                sheet.cursor_left(1);
                // Scroll left if cursor moves before left_col (GAP-010)
                if sheet.cursor_col < sheet.left_col {
                    sheet.left_col = sheet.cursor_col;
                }
            }

            // Page movement
            KeyCode::PageDown => sheet.cursor_down(height.saturating_sub(2)),
            KeyCode::PageUp => sheet.cursor_up(height.saturating_sub(2)),

            // Home / go to top
            KeyCode::Home => {
                sheet.cursor_row = 0;
                sheet.top_row = 0;
            }
            // 'g' and 'z' set pending prefix for multi-key bindings
            KeyCode::Char('g') => {
                self.pending_prefix = Some("g".into());
                return;
            }
            KeyCode::Char('z') => {
                self.pending_prefix = Some("z".into());
                return;
            }
            KeyCode::End => {
                if !sheet.rows.is_empty() {
                    sheet.cursor_row = sheet.rows.len() - 1;
                }
            }

            // --- Editing ---

            // Edit current cell
            KeyCode::Char('e') => {
                if !sheet.rows.is_empty() {
                    let vis = sheet.visible_columns();
                    if let Some(&col) = vis.get(sheet.cursor_col) {
                        let current_display = col.display_value(&sheet.rows[sheet.cursor_row]);
                        let editor = LineEditor::new(&current_display);
                        self.mode = InputMode::EditCell(editor);
                        return;
                    }
                }
            }

            // Add row above cursor
            KeyCode::Char('a') => {
                sheet.insert_row_at(sheet.cursor_row);
            }

            // Delete current row
            KeyCode::Char('d') => {
                if !sheet.rows.is_empty() {
                    sheet.delete_row_at(sheet.cursor_row);
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

            // --- Type conversion (undoable) ---
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
            KeyCode::Char('|') => {
                self.mode = InputMode::SelectColRegex {
                    editor: LineEditor::new(""),
                    select: true,
                };
                return;
            }
            KeyCode::Char('\\') => {
                self.mode = InputMode::SelectColRegex {
                    editor: LineEditor::new(""),
                    select: false,
                };
                return;
            }
            KeyCode::Char(',') => {
                self.dispatch_command("select-equal-cell");
                return;
            }

            // --- Regex-derived columns (Group B) ---
            KeyCode::Char(':') => {
                self.mode = InputMode::CommandPalette(LineEditor::new("split-col:"));
                return;
            }
            KeyCode::Char(';') => {
                self.mode = InputMode::CommandPalette(LineEditor::new("capture-col:"));
                return;
            }
            KeyCode::Char('*') => {
                self.mode = InputMode::CommandPalette(LineEditor::new("subst-col:"));
                return;
            }
            KeyCode::Char('(') => {
                self.dispatch_command("expand-col");
                return;
            }
            KeyCode::Char(')') => {
                self.dispatch_command("contract-col");
                return;
            }
            KeyCode::Char('Y') => {
                self.dispatch_command("syscopy-row");
                return;
            }

            // --- Navigation: go to column by regex ---
            KeyCode::Char('c') => {
                self.mode = InputMode::GotoColRegex(LineEditor::new(""));
                return;
            }

            // --- Navigation: go to different/selected value ---
            KeyCode::Char('<') => {
                self.dispatch_command("go-prev-value");
                return;
            }
            KeyCode::Char('>') => {
                self.dispatch_command("go-next-value");
                return;
            }
            KeyCode::Char('{') => {
                self.dispatch_command("go-prev-selected");
                return;
            }
            KeyCode::Char('}') => {
                self.dispatch_command("go-next-selected");
                return;
            }

            // --- Column sliding ---
            KeyCode::Char('H') => {
                self.dispatch_command("slide-left");
                return;
            }
            KeyCode::Char('L') => {
                self.dispatch_command("slide-right");
                return;
            }
            KeyCode::Char('J') => {
                self.dispatch_command("slide-row-down");
                return;
            }
            KeyCode::Char('K') => {
                self.dispatch_command("slide-row-up");
                return;
            }

            // --- Clipboard ---
            KeyCode::Char('x') => {
                self.dispatch_command("cut-row");
                return;
            }

            // --- Editing ---
            KeyCode::Char('f') => {
                self.dispatch_command("fill-down");
                return;
            }
            KeyCode::Char('A') => {
                self.dispatch_command("open-new");
                return;
            }

            // --- Redo ---
            KeyCode::Char('R') => {
                self.do_redo();
                self.update_status();
                return;
            }

            // --- Search ---
            KeyCode::Char('/') => {
                self.last_search_scope = SearchScope::CurrentCol;
                self.mode = InputMode::SearchForward(LineEditor::new(""));
                return;
            }
            KeyCode::Char('?') => {
                self.last_search_scope = SearchScope::CurrentCol;
                self.mode = InputMode::SearchBackward(LineEditor::new(""));
                return;
            }
            KeyCode::Char('n') => {
                self.repeat_search(true);
            }
            KeyCode::Char('N') => {
                self.repeat_search(false);
            }
            KeyCode::Char('r') => {
                // Search key columns — set scope then enter search mode
                self.last_search_scope = SearchScope::KeyCols;
                self.mode = InputMode::SearchForward(LineEditor::new(""));
                return;
            }

            // --- Filter (selected rows to new sheet) ---
            KeyCode::Char('"') => {
                if sheet.num_selected() > 0 {
                    let filtered = sheet.selected_rows_sheet();
                    self.stack.push(filtered);
                }
            }

            // --- Multi-sheet operations ---
            KeyCode::Char('&') => {
                // Open palette pre-filtered to join types (GAP-092)
                self.mode = InputMode::CommandPalette(LineEditor::new("join-type-"));
                return;
            }
            KeyCode::Char('W') => {
                let col_idx = resolve_cursor_col_idx(sheet);
                if let Some(idx) = col_idx {
                    let pivoted = visidata_core::sheets::pivot_sheet(sheet, idx);
                    self.stack.push(pivoted);
                }
            }
            KeyCode::Char('M') => {
                let melted = visidata_core::sheets::melt_sheet(sheet);
                self.stack.push(melted);
            }
            KeyCode::Char('T') => {
                self.dispatch_command("transpose");
                return;
            }
            KeyCode::Char('\'') => {
                self.dispatch_command("freeze-col");
                return;
            }
            KeyCode::Char('i') => {
                self.dispatch_command("addcol-incr");
                return;
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

            // --- Clipboard ---
            KeyCode::Char('y') => {
                // Yank current cell
                let vis = sheet.visible_columns();
                if let Some(&col) = vis.get(sheet.cursor_col) {
                    let val = col.typed_value(&sheet.rows[sheet.cursor_row]);
                    self.clipboard.yank_cell(val);
                    self.status = "yanked cell".into();
                }
            }
            KeyCode::Char('p') => {
                // Paste cell
                if let visidata_core::clipboard::ClipboardContent::Cell(val) =
                    self.clipboard.content().clone()
                {
                    let vis = sheet.visible_columns();
                    if let Some(&col) = vis.get(sheet.cursor_col) {
                        let source_idx = col.source_idx;
                        sheet.set_cell(sheet.cursor_row, source_idx, val);
                    }
                }
            }

            // --- Aggregation ---
            KeyCode::Char('+') => {
                // Open palette for aggregator selection (GAP-098)
                self.mode = InputMode::CommandPalette(LineEditor::new("add-agg:"));
                return;
            }

            // --- Expression column ---
            KeyCode::Char('=') => {
                self.mode = InputMode::CommandPalette(LineEditor::new("="));
                return;
            }

            // --- Menu ---
            KeyCode::F(10) => {
                self.menu_state.toggle();
                if self.menu_state.open {
                    self.mode = InputMode::Menu;
                } else {
                    self.mode = InputMode::Normal;
                }
                return;
            }

            // --- Help overlay ---
            KeyCode::F(1) => {
                self.mode = InputMode::Help { scroll: 0 };
                return;
            }

            // --- Command palette ---
            KeyCode::Char(' ') => {
                self.mode = InputMode::CommandPalette(LineEditor::new(""));
                return;
            }

            // --- Macro recording/replay ---
            KeyCode::Char('Q') => {
                if self.macro_recorder.is_recording() {
                    if let Some(mac) = self.macro_recorder.stop("last") {
                        self.status = format!("recorded {} keystrokes", mac.keystrokes.len());
                        self.macro_store.save(mac);
                    } else {
                        self.status = "nothing recorded".into();
                    }
                } else {
                    self.macro_recorder.start();
                    self.status = "recording...".into();
                }
                return;
            }
            // Note: '@' is bound to type-date above; replay via command palette ("macro-replay")
            _ => {}
        }

        // Record this keystroke if macro recording is active
        if self.macro_recorder.is_recording() {
            let key_str = match key.code {
                KeyCode::Char(c) => c.to_string(),
                KeyCode::Enter => "Enter".into(),
                KeyCode::Esc => "Esc".into(),
                KeyCode::Up => "Up".into(),
                KeyCode::Down => "Down".into(),
                KeyCode::Left => "Left".into(),
                KeyCode::Right => "Right".into(),
                _ => String::new(),
            };
            if !key_str.is_empty() {
                self.macro_recorder.record(&key_str);
            }
        }

        self.update_status();
    }

    /// Handle the help command — push help sheet onto stack.
    fn show_help(&mut self) {
        let sheet = visidata_core::help::help_sheet(&self.commands);
        self.stack.push(sheet);
    }

    /// Handle a key event while in input mode.
    #[expect(clippy::too_many_lines, reason = "flat match dispatch for all input modes")]
    fn handle_input_key(&mut self, key: KeyEvent) {
        let key_str = key_event_to_string(&key);

        // Extract editor, process keystroke
        let mode = std::mem::replace(&mut self.mode, InputMode::Normal);
        match mode {
            InputMode::RenameColumn(mut editor) => {
                match editor.handle_key(&key_str) {
                    EditResult::Accept(new_name) => {
                        // Apply rename with undo tracking
                        if let Some(sheet) = self.stack.active_mut() {
                            let vis = sheet.visible_columns();
                            if let Some(&col) = vis.get(sheet.cursor_col) {
                                let col_id = col.id.0;
                                sheet.rename_column(col_id, new_name);
                            }
                        }
                        self.redo_stack.clear();
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
            InputMode::EditCell(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(new_value) => {
                    // Options sheet write-back (GAP-082)
                    let is_options = self.stack.active()
                        .is_some_and(|s| s.name == "options" || s.name.ends_with(']'));
                    if is_options {
                        // Column 0 = option name; cursor on column 1 = value
                        if let Some(sheet) = self.stack.active()
                            && let Some(opt_name_val) = sheet.rows.get(sheet.cursor_row).map(|r| r.get(0).clone())
                                && let visidata_core::Value::Text(ref opt_name) = opt_name_val {
                                    let opt_name = opt_name.clone();
                                    self.options.set_global(&opt_name, visidata_core::Value::Text(new_value));
                                    self.status = format!("set {opt_name}");
                                }
                    } else if let Some(sheet) = self.stack.active_mut() {
                        let vis = sheet.visible_columns();
                        if let Some(&col) = vis.get(sheet.cursor_col) {
                            let col_source_idx = col.source_idx;
                            let value = visidata_core::Value::Text(new_value);
                            sheet.set_cell(sheet.cursor_row, col_source_idx, value);
                        }
                    }
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => {
                    self.mode = InputMode::Normal;
                }
                EditResult::Continue => {
                    self.mode = InputMode::EditCell(editor);
                }
            },
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
            InputMode::SearchForwardAllCols(mut editor) => {
                match editor.handle_key(&key_str) {
                    EditResult::Accept(pattern) => {
                        self.last_search = Some(pattern.clone());
                        self.last_search_forward = true;
                        self.last_search_scope = SearchScope::CurrentCol; // all-cols uses its own path
                        self.execute_search_all_cols(&pattern, true);
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Cancel => {
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Continue => {
                        self.mode = InputMode::SearchForwardAllCols(editor);
                    }
                }
            }
            InputMode::SearchBackwardAllCols(mut editor) => {
                match editor.handle_key(&key_str) {
                    EditResult::Accept(pattern) => {
                        self.last_search = Some(pattern.clone());
                        self.last_search_forward = false;
                        self.last_search_scope = SearchScope::CurrentCol;
                        self.execute_search_all_cols(&pattern, false);
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Cancel => {
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Continue => {
                        self.mode = InputMode::SearchBackwardAllCols(editor);
                    }
                }
            }
            InputMode::SelectColRegex { mut editor, select } => {
                match editor.handle_key(&key_str) {
                    EditResult::Accept(pattern) => {
                        let col_idx = self
                            .stack
                            .active()
                            .and_then(resolve_cursor_col_idx);
                        if let (Some(col_idx), Some(sheet)) =
                            (col_idx, self.stack.active_mut())
                        {
                            match sheet.select_by_col_regex(col_idx, &pattern, select) {
                                Some(n) => {
                                    let verb = if select { "selected" } else { "unselected" };
                                    self.status = format!("{verb} {n} rows");
                                }
                                None => {
                                    self.status = format!("invalid regex: {pattern}");
                                }
                            }
                        }
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Cancel => {
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Continue => {
                        self.mode = InputMode::SelectColRegex { editor, select };
                    }
                }
            }
            InputMode::SelectAllColsRegex { mut editor, select } => {
                match editor.handle_key(&key_str) {
                    EditResult::Accept(pattern) => {
                        if let Some(sheet) = self.stack.active_mut() {
                            match sheet.select_by_any_col_regex(&pattern, select) {
                                Some(n) => {
                                    let verb = if select { "selected" } else { "unselected" };
                                    self.status = format!("{verb} {n} rows");
                                }
                                None => {
                                    self.status = format!("invalid regex: {pattern}");
                                }
                            }
                        }
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Cancel => {
                        self.mode = InputMode::Normal;
                    }
                    EditResult::Continue => {
                        self.mode = InputMode::SelectAllColsRegex { editor, select };
                    }
                }
            }
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
            InputMode::NameMacroInput(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(name) => {
                    let name = if name.trim().is_empty() { "last".into() } else { name.trim().to_owned() };
                    // Rename the "last" macro to the given name
                    if let Some(mac) = self.macro_store.get("last").cloned() {
                        let mut named = mac;
                        named.name.clone_from(&name);
                        self.macro_store.save(named);
                    }
                    let _ = self.macro_store.save_to_disk();
                    self.status = format!("macro saved as '{name}'");
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => self.mode = InputMode::Normal,
                EditResult::Continue => self.mode = InputMode::NameMacroInput(editor),
            },
            InputMode::GotoRow(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(s) => {
                    if let Ok(n) = s.parse::<usize>() {
                        if let Some(sheet) = self.stack.active_mut() {
                            sheet.cursor_row = n.min(sheet.rows.len().saturating_sub(1));
                        }
                    } else {
                        self.status = format!("invalid row number: {s}");
                    }
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => self.mode = InputMode::Normal,
                EditResult::Continue => self.mode = InputMode::GotoRow(editor),
            },
            InputMode::GotoColRegex(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(pattern) => {
                    if let Some(sheet) = self.stack.active_mut() {
                        if let Ok(re) = regex::Regex::new(&pattern) {
                            let found = sheet.visible_columns().iter().enumerate()
                                .find(|(_, c)| re.is_match(&c.name))
                                .map(|(i, _): (usize, _)| i);
                            if let Some(idx) = found {
                                sheet.cursor_col = idx;
                            } else {
                                self.status = format!("no column matching: {pattern}");
                            }
                        } else {
                            self.status = format!("invalid regex: {pattern}");
                        }
                    }
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => self.mode = InputMode::Normal,
                EditResult::Continue => self.mode = InputMode::GotoColRegex(editor),
            },
            InputMode::GotoColNumber(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(s) => {
                    if let Ok(n) = s.parse::<usize>() {
                        if let Some(sheet) = self.stack.active_mut() {
                            let max = sheet.visible_columns().len().saturating_sub(1);
                            sheet.cursor_col = n.min(max);
                        }
                    } else {
                        self.status = format!("invalid column number: {s}");
                    }
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => self.mode = InputMode::Normal,
                EditResult::Continue => self.mode = InputMode::GotoColNumber(editor),
            },
            InputMode::SetColInput(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(new_val) => {
                    if let Some(sheet) = self.stack.active_mut()
                        && let Some(col_idx) = resolve_cursor_col_idx(sheet)
                    {
                        let source_idx = sheet.columns[col_idx].source_idx;
                        let value = visidata_core::Value::Text(new_val);
                        let selected: Vec<usize> = sheet.rows.iter().enumerate()
                            .filter(|(_, r)| r.selected)
                            .map(|(i, _)| i)
                            .collect();
                        let count = selected.len();
                        for idx in selected {
                            sheet.set_cell(idx, source_idx, value.clone());
                        }
                        self.status = format!("set {count} rows");
                    }
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => self.mode = InputMode::Normal,
                EditResult::Continue => self.mode = InputMode::SetColInput(editor),
            },
            InputMode::ResizeColInput(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(s) => {
                    if let Ok(w) = s.parse::<u16>() {
                        if let Some(sheet) = self.stack.active_mut()
                            && let Some(col_idx) = resolve_cursor_col_idx(sheet)
                        {
                            sheet.columns[col_idx].width = Some(w);
                        }
                    } else {
                        self.status = format!("invalid width: {s}");
                    }
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => self.mode = InputMode::Normal,
                EditResult::Continue => self.mode = InputMode::ResizeColInput(editor),
            },
            InputMode::AddRowsInput(mut editor) => match editor.handle_key(&key_str) {
                EditResult::Accept(s) => {
                    let n = s.parse::<usize>().unwrap_or(1);
                    if let Some(sheet) = self.stack.active_mut() {
                        let at = sheet.cursor_row;
                        for i in 0..n {
                            sheet.insert_row_at(at + i);
                        }
                        self.status = format!("added {n} rows");
                    }
                    self.mode = InputMode::Normal;
                }
                EditResult::Cancel => self.mode = InputMode::Normal,
                EditResult::Continue => self.mode = InputMode::AddRowsInput(editor),
            },
            InputMode::Normal | InputMode::Menu | InputMode::Help { .. } => unreachable!(),
        }

        self.update_status();
    }

    /// Execute a command by searching the registry for a query match.
    #[expect(clippy::too_many_lines, reason = "palette prefix dispatch table")]
    fn execute_command_by_query(&mut self, query: &str) {
        if query.is_empty() {
            return;
        }

        // Handle expression column: "=expr" or "=name=expr"
        if let Some(rest) = query.strip_prefix('=') {
            if let Some(sheet) = self.stack.active_mut() {
                // Check for =name=expr format
                let (col_name, expr) = if let Some(eq_pos) = rest.find('=') {
                    let name = rest[..eq_pos].trim().to_owned();
                    let expr = &rest[eq_pos + 1..];
                    if name.is_empty() {
                        (format!("expr{}", sheet.columns.len()), rest)
                    } else {
                        (name, expr)
                    }
                } else {
                    (format!("expr{}", sheet.columns.len()), rest)
                };
                visidata_core::expr_column::add_expression_column(sheet, &col_name, expr);
                self.status = format!("added column: {col_name}");
            }
            return;
        }

        // Handle column split: query starts with "split:"
        if let Some(pattern) = query.strip_prefix("split:") {
            if let Some(sheet) = self.stack.active_mut()
                && let Some(idx) = resolve_cursor_col_idx(sheet)
            {
                let added = visidata_core::sheets::split_column(sheet, idx, pattern);
                self.status = format!("split into {added} columns");
            }
            return;
        }

        // split-col: (`:` key prefix) — same as split: but from new binding
        if let Some(pattern) = query.strip_prefix("split-col:") {
            if let Some(sheet) = self.stack.active_mut()
                && let Some(idx) = resolve_cursor_col_idx(sheet)
            {
                let added = visidata_core::sheets::split_column(sheet, idx, pattern);
                self.status = format!("split into {added} columns");
            }
            return;
        }

        // capture-col: (`;` key) — add capture-group columns
        if let Some(pattern) = query.strip_prefix("capture-col:") {
            if let Some(sheet) = self.stack.active_mut()
                && let Some(idx) = resolve_cursor_col_idx(sheet)
            {
                let added = visidata_core::sheets::capture_columns(sheet, idx, pattern);
                match added {
                    Ok(n) => self.status = format!("added {n} capture columns"),
                    Err(e) => self.status = format!("invalid regex: {e}"),
                }
            }
            return;
        }

        // subst-col: (`*` key) — add substitution column
        if let Some(pattern_repl) = query.strip_prefix("subst-col:") {
            if let Some(sheet) = self.stack.active_mut()
                && let Some(idx) = resolve_cursor_col_idx(sheet)
            {
                match visidata_core::sheets::subst_column(sheet, idx, pattern_repl) {
                    Ok(()) => self.status = "added substitution column".into(),
                    Err(e) => self.status = format!("invalid pattern: {e}"),
                }
            }
            return;
        }

        // melt-regex: (`gM` key) — melt with column-name regex
        if let Some(regex) = query.strip_prefix("melt-regex:") {
            if let Some(sheet) = self.stack.active() {
                let melted = visidata_core::sheets::melt_sheet_regex(sheet, regex);
                match melted {
                    Ok(m) => self.stack.push(m),
                    Err(e) => self.status = format!("invalid regex: {e}"),
                }
            }
            return;
        }

        // g= — set selected rows' current column from expression
        if let Some(expr) = query.strip_prefix("g=") {
            if let Some(sheet) = self.stack.active_mut()
                && let Some(col_idx) = resolve_cursor_col_idx(sheet)
            {
                let source_idx = sheet.columns[col_idx].source_idx;
                let selected: Vec<usize> = sheet.rows.iter().enumerate()
                    .filter(|(_, r)| r.selected)
                    .map(|(i, _)| i)
                    .collect();
                let engine = self.script_engine.engine();
                let results: Vec<(usize, visidata_core::Value)> = selected.iter().map(|&ri| {
                    let mut scope = rhai::Scope::new();
                    for col in &sheet.columns {
                        match col.raw_value(&sheet.rows[ri]) {
                            visidata_core::Value::Int(n)   => { scope.push(col.name.as_str(), *n); }
                            visidata_core::Value::Float(f) => { scope.push(col.name.as_str(), *f); }
                            visidata_core::Value::Bool(b)  => { scope.push(col.name.as_str(), *b); }
                            visidata_core::Value::Text(s)  => { scope.push(col.name.as_str(), s.clone()); }
                            _ => { scope.push(col.name.as_str(), rhai::Dynamic::UNIT); }
                        }
                    }
                    let val = engine.eval_with_scope::<rhai::Dynamic>(&mut scope, expr)
                        .map_or_else(|e| visidata_core::Value::Error(format!("{e}")), visidata_core::rhai_dynamic_to_value);
                    (ri, val)
                }).collect();
                let count = results.len();
                for (ri, val) in results {
                    sheet.set_cell(ri, source_idx, val);
                }
                self.status = format!("set {count} rows");
            }
            return;
        }

        // z= — set cursor cell from expression
        if let Some(expr) = query.strip_prefix("z=") {
            if let Some(sheet) = self.stack.active_mut()
                && !sheet.rows.is_empty()
                && let Some(col_idx) = resolve_cursor_col_idx(sheet)
            {
                let source_idx = sheet.columns[col_idx].source_idx;
                let ri = sheet.cursor_row;
                let mut scope = rhai::Scope::new();
                for col in &sheet.columns {
                    match col.raw_value(&sheet.rows[ri]) {
                        visidata_core::Value::Int(n)   => { scope.push(col.name.as_str(), *n); }
                        visidata_core::Value::Float(f) => { scope.push(col.name.as_str(), *f); }
                        visidata_core::Value::Bool(b)  => { scope.push(col.name.as_str(), *b); }
                        visidata_core::Value::Text(s)  => { scope.push(col.name.as_str(), s.clone()); }
                        _ => { scope.push(col.name.as_str(), rhai::Dynamic::UNIT); }
                    }
                }
                let val = self.script_engine.engine()
                    .eval_with_scope::<rhai::Dynamic>(&mut scope, expr)
                    .map_or_else(|e| visidata_core::Value::Error(format!("{e}")), visidata_core::rhai_dynamic_to_value);
                sheet.set_cell(ri, source_idx, val);
            }
            return;
        }

        // select-expr: / unselect-expr: — select/unselect rows matching a Rhai expression
        for (prefix, select) in [("select-expr:", true), ("unselect-expr:", false)] {
            if let Some(expr) = query.strip_prefix(prefix) {
                if let Some(sheet) = self.stack.active_mut() {
                    let engine = self.script_engine.engine();
                    let mut count = 0usize;
                    // Collect (row_idx, result) first to avoid borrow conflict
                    let results: Vec<bool> = sheet.rows.iter().map(|row| {
                        let mut scope = rhai::Scope::new();
                        for col in &sheet.columns {
                            match col.raw_value(row) {
                                visidata_core::Value::Int(n)   => { scope.push(col.name.as_str(), *n); }
                                visidata_core::Value::Float(f) => { scope.push(col.name.as_str(), *f); }
                                visidata_core::Value::Bool(b)  => { scope.push(col.name.as_str(), *b); }
                                visidata_core::Value::Text(s)  => { scope.push(col.name.as_str(), s.clone()); }
                                _ => { scope.push(col.name.as_str(), rhai::Dynamic::UNIT); }
                            }
                        }
                        engine.eval_with_scope::<bool>(&mut scope, expr).unwrap_or(false)
                    }).collect();
                    for (row, matched) in sheet.rows.iter_mut().zip(results) {
                        if matched { row.selected = select; count += 1; }
                    }
                    let verb = if select { "selected" } else { "unselected" };
                    self.status = format!("{verb} {count} rows");
                }
                return;
            }
        }

        // search-expr: / searchr-expr: — advance cursor to first truthy row
        for (prefix, forward) in [("search-expr:", true), ("searchr-expr:", false)] {
            if let Some(expr) = query.strip_prefix(prefix) {
                if let Some(sheet) = self.stack.active_mut() {
                    let engine = self.script_engine.engine();
                    let n = sheet.rows.len();
                    let start = if forward { sheet.cursor_row + 1 } else { sheet.cursor_row.saturating_sub(1) };
                    let iter: Box<dyn Iterator<Item = usize>> = if forward {
                        Box::new((start..n).chain(0..start))
                    } else {
                        Box::new((0..start).rev().chain((start..n).rev()))
                    };
                    let mut found = None;
                    for i in iter {
                        let mut scope = rhai::Scope::new();
                        for col in &sheet.columns {
                            match col.raw_value(&sheet.rows[i]) {
                                visidata_core::Value::Int(n)   => { scope.push(col.name.as_str(), *n); }
                                visidata_core::Value::Float(f) => { scope.push(col.name.as_str(), *f); }
                                visidata_core::Value::Bool(b)  => { scope.push(col.name.as_str(), *b); }
                                visidata_core::Value::Text(s)  => { scope.push(col.name.as_str(), s.clone()); }
                                _ => { scope.push(col.name.as_str(), rhai::Dynamic::UNIT); }
                            }
                        }
                        if engine.eval_with_scope::<bool>(&mut scope, expr).unwrap_or(false) {
                            found = Some(i);
                            break;
                        }
                    }
                    if let Some(i) = found { sheet.cursor_row = i; }
                    else { self.status = format!("not found: {expr}"); }
                }
                return;
            }
        }

        // replay: (GAP-128) — load and replay a .vdj command log
        if let Some(path_str) = query.strip_prefix("replay:") {
            let path = std::path::Path::new(path_str.trim());
            match std::fs::read_to_string(path) {
                Ok(content) => {
                    match serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                        Ok(entries) => {
                            let cmds: Vec<String> = entries.iter()
                                .filter_map(|v| v[0].as_str().map(str::to_owned))
                                .collect();
                            self.macro_replay = cmds;
                            self.status = format!("replaying {} commands from {path_str}", self.macro_replay.len());
                        }
                        Err(e) => self.status = format!("invalid .vdj: {e}"),
                    }
                }
                Err(e) => self.status = format!("cannot read {path_str}: {e}"),
            }
            return;
        }

        // add-agg: (`+` key) — add aggregator to current column and show result
        if let Some(func_name) = query.strip_prefix("add-agg:") {
            if let Some(sheet) = self.stack.active_mut()
                && let Some(col_idx) = resolve_cursor_col_idx(sheet)
            {
                if let Some(f) = visidata_core::aggregation::AggFunc::from_name(func_name) {
                    sheet.columns[col_idx].aggregators.push(f);
                    let val = visidata_core::aggregation::aggregate(&sheet.columns[col_idx], &sheet.rows, f);
                    self.status = format!("{func_name}={val}");
                } else if func_name.is_empty() {
                    // No name entered — show current summary
                    let col = &sheet.columns[col_idx];
                    self.status = visidata_core::aggregation::aggregate_summary(col, &sheet.rows);
                } else {
                    self.status = format!("unknown aggregator: {func_name}");
                }
            }
            return;
        }

        // sql: — run arbitrary SQL on the active ext-loader sheet (GAP-134)
        if let Some(sql) = query.strip_prefix("sql:") {
            // Detect ext-loader sheet by checking if the source has an ext-loader registered.
            if let Some(sheet) = self.stack.active()
                && let Some(ref src) = sheet.source {
                    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    let registry = visidata_loaders::LoaderRegistry::with_builtins();
                    if let Some(ext_loader) = registry.find_ext_loader(&ext) {
                        let opts = self.loader_options_snapshot();
                        match ext_loader.run_query(src, Some(sql), opts.ext_options.iter().map(|(k,v)| (k.clone(), serde_json::Value::String(v.clone()))).collect()) {
                            Ok(result) => self.stack.push(result),
                            Err(e) => self.status = format!("sql error: {e}"),
                        }
                    }
                }
            return;
        }

        // memo-agg: (`z+` key) — show one aggregation in status
        if let Some(func_name) = query.strip_prefix("memo-agg:") {
            if let Some(sheet) = self.stack.active()
                && let Some(col_idx) = resolve_cursor_col_idx(sheet)
            {
                let func = visidata_core::aggregation::AggFunc::from_name(func_name);
                if let Some(f) = func {
                    let val = visidata_core::aggregation::aggregate(&sheet.columns[col_idx], &sheet.rows, f);
                    self.status = format!("{func_name}={val}");
                } else {
                    self.status = format!("unknown aggregator: {func_name}");
                }
            }
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
            "open-row" => {
                // SheetsSheet special-case (GAP-080): navigate to the named sheet.
                let sheets_nav = self.stack.active().and_then(|s| {
                    if !s.name.ends_with("_sheets") && s.name != "sheets" && s.name != "sheets_all" {
                        return None;
                    }
                    let target_name = match s.rows.get(s.cursor_row)?.get(0) {
                        visidata_core::Value::Text(t) => t.clone(),
                        _ => return None,
                    };
                    Some(target_name)
                });
                if let Some(target) = sheets_nav {
                    // Find target sheet in stack and pop back to it.
                    let idx = self.stack.iter().position(|s| s.name == target);
                    if let Some(pos) = idx {
                        while self.stack.len() > pos + 1 {
                            self.stack.pop();
                        }
                    } else {
                        self.status = format!("sheet '{target}' not in stack");
                    }
                    self.update_status();
                    return;
                }

                // Normal drill: clone Arc + row before mutable borrow.
                let maybe = self.stack.active().and_then(|s| {
                    let drill = s.drill.clone()?;
                    let row = s.rows.get(s.cursor_row).cloned()?;
                    Some((drill, row))
                });
                if let Some((drill, row)) = maybe {
                    match drill.open_row(&row) {
                        Ok(child) => self.stack.push(child),
                        Err(e) => self.status = format!("{e:#}"),
                    }
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
            "edit-cell" => {
                if let Some(s) = self.stack.active()
                    && !s.rows.is_empty()
                {
                    let vis = s.visible_columns();
                    if let Some(&col) = vis.get(s.cursor_col) {
                        let display = col.display_value(&s.rows[s.cursor_row]);
                        self.mode = InputMode::EditCell(LineEditor::new(&display));
                    }
                }
            }
            "add-row" => {
                if let Some(s) = self.stack.active_mut() {
                    let idx = s.cursor_row;
                    s.insert_row_at(idx);
                }
            }
            "delete-row" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                {
                    s.delete_row_at(s.cursor_row);
                }
            }
            "delete-selected" => {
                if let Some(s) = self.stack.active_mut() {
                    let count = s.delete_selected_rows();
                    self.status = format!("deleted {count} rows");
                }
            }
            "undo" => self.do_undo(),
            "save-sheet" => self.save_current_sheet(),
            "yank-cell" => {
                if let Some(s) = self.stack.active()
                    && !s.rows.is_empty()
                {
                    let vis = s.visible_columns();
                    if let Some(&col) = vis.get(s.cursor_col) {
                        let val = col.typed_value(&s.rows[s.cursor_row]);
                        self.clipboard.yank_cell(val);
                        self.status = "yanked cell".into();
                    }
                }
            }
            "paste-cell" => {
                if let visidata_core::clipboard::ClipboardContent::Cell(val) =
                    self.clipboard.content().clone()
                    && let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                {
                    let vis = s.visible_columns();
                    if let Some(&col) = vis.get(s.cursor_col) {
                        let source_idx = col.source_idx;
                        s.set_cell(s.cursor_row, source_idx, val);
                    }
                }
            }
            "aggregate-col" => {
                if let Some(s) = self.stack.active() {
                    let vis = s.visible_columns();
                    if let Some(&col) = vis.get(s.cursor_col) {
                        let summary = visidata_core::aggregation::aggregate_summary(col, &s.rows);
                        self.status = summary;
                    }
                } // keep status
            }
            "expr-col" => {
                self.mode = InputMode::CommandPalette(LineEditor::new("="));
            }
            "join-sheets" => self.join_top_two_sheets(),
            "concat-sheets" => {
                let sheets: Vec<&Sheet> = self.stack.iter().collect();
                if sheets.len() >= 2 {
                    let concatenated = visidata_core::sheets::concat_sheets(&sheets);
                    self.stack.push(concatenated);
                }
            }
            "pivot" => {
                if let Some(s) = self.stack.active()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                {
                    let pivoted = visidata_core::sheets::pivot_sheet(s, idx);
                    self.stack.push(pivoted);
                }
            }
            "melt" => {
                if let Some(s) = self.stack.active() {
                    let melted = visidata_core::sheets::melt_sheet(s);
                    self.stack.push(melted);
                }
            }
            "split-col" => {
                self.mode = InputMode::CommandPalette(LineEditor::new("split:"));
            }
            "macro-record-toggle" => {
                if self.macro_recorder.is_recording() {
                    if let Some(mac) = self.macro_recorder.stop("last") {
                        let n = mac.keystrokes.len();
                        self.macro_store.save(mac);
                        // Prompt for a name (GAP-127)
                        self.mode = InputMode::NameMacroInput(LineEditor::new("last"));
                        self.status = format!("recorded {n} keystrokes — enter name (Enter=keep 'last')");
                        return;
                    }
                    self.status = "nothing recorded".into();
                } else {
                    self.macro_recorder.start();
                    self.status = "recording...".into();
                }
            }
            "macro-replay" => {
                if let Some(mac) = self.macro_store.get("last") {
                    self.macro_replay = mac.keystrokes.clone();
                    self.status = format!("replaying {} keystrokes", self.macro_replay.len());
                } else {
                    self.status = "no macro recorded".into();
                }
            }

            // --- Navigation ---
            "go-prev-value" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    let cur_val = s.columns[col_idx].display_value(&s.rows[s.cursor_row]);
                    let start = s.cursor_row;
                    for i in (0..start).rev() {
                        let val = s.columns[col_idx].display_value(&s.rows[i]);
                        if val != cur_val {
                            s.cursor_row = i;
                            break;
                        }
                    }
                }
            }
            "go-next-value" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    let cur_val = s.columns[col_idx].display_value(&s.rows[s.cursor_row]);
                    let start = s.cursor_row + 1;
                    for i in start..s.rows.len() {
                        let val = s.columns[col_idx].display_value(&s.rows[i]);
                        if val != cur_val {
                            s.cursor_row = i;
                            break;
                        }
                    }
                }
            }
            "go-prev-selected" => {
                if let Some(s) = self.stack.active_mut() && !s.rows.is_empty() {
                    let start = s.cursor_row;
                    for i in (0..start).rev() {
                        if s.rows[i].selected {
                            s.cursor_row = i;
                            break;
                        }
                    }
                }
            }
            "go-next-selected" => {
                if let Some(s) = self.stack.active_mut() && !s.rows.is_empty() {
                    let start = s.cursor_row + 1;
                    for i in start..s.rows.len() {
                        if s.rows[i].selected {
                            s.cursor_row = i;
                            break;
                        }
                    }
                }
            }
            "scroll-middle" => {
                if let Some(s) = self.stack.active_mut() {
                    let height = crossterm::terminal::size().map_or(20, |(_, h)| h as usize);
                    s.top_row = s.cursor_row.saturating_sub(height / 2);
                }
            }

            // --- Selection ---
            "select-rows" => {
                if let Some(s) = self.stack.active_mut() {
                    s.select_all();
                    self.status = format!("selected {} rows", s.rows.len());
                }
            }
            "unselect-rows" => {
                if let Some(s) = self.stack.active_mut() {
                    s.unselect_all();
                }
            }
            "stoggle-rows" => {
                if let Some(s) = self.stack.active_mut() {
                    s.toggle_select_all();
                }
            }
            "select-equal-cell" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    let target = s.columns[col_idx].display_value(&s.rows[s.cursor_row]);
                    let mut count = 0;
                    for row in &mut s.rows {
                        if s.columns[col_idx].display_value(row) == target {
                            row.selected = true;
                            count += 1;
                        }
                    }
                    self.status = format!("selected {count} rows");
                }
            }
            "select-equal-row" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                {
                    let vis_indices: Vec<usize> = s
                        .columns
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| !c.is_hidden())
                        .map(|(i, _)| i)
                        .collect();
                    let target: Vec<String> = vis_indices
                        .iter()
                        .map(|&ci| s.columns[ci].display_value(&s.rows[s.cursor_row]))
                        .collect();
                    let mut count = 0;
                    for row in &mut s.rows {
                        let row_vals: Vec<String> = vis_indices
                            .iter()
                            .map(|&ci| s.columns[ci].display_value(row))
                            .collect();
                        if row_vals == target {
                            row.selected = true;
                            count += 1;
                        }
                    }
                    self.status = format!("selected {count} rows");
                }
            }

            // --- Column sliding ---
            "slide-left" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                    && idx > 0
                {
                    s.columns.swap(idx, idx - 1);
                    s.cursor_col = s.cursor_col.saturating_sub(1);
                }
            }
            "slide-right" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                    && idx + 1 < s.columns.len()
                {
                    s.columns.swap(idx, idx + 1);
                    let max_vis = s.visible_columns().len().saturating_sub(1);
                    s.cursor_col = (s.cursor_col + 1).min(max_vis);
                }
            }
            "slide-leftmost" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                    && idx > 0
                {
                    let col = s.columns.remove(idx);
                    s.columns.insert(0, col);
                    s.cursor_col = 0;
                }
            }
            "slide-rightmost" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                {
                    let last = s.columns.len() - 1;
                    if idx < last {
                        let col = s.columns.remove(idx);
                        s.columns.push(col);
                        s.cursor_col = s.visible_columns().len() - 1;
                    }
                }
            }

            // --- Row sliding ---
            "slide-row-down" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                    && s.cursor_row + 1 < s.rows.len()
                {
                    s.rows.swap(s.cursor_row, s.cursor_row + 1);
                    s.cursor_row += 1;
                }
            }
            "slide-row-up" => {
                if let Some(s) = self.stack.active_mut()
                    && s.cursor_row > 0
                {
                    s.rows.swap(s.cursor_row, s.cursor_row - 1);
                    s.cursor_row -= 1;
                }
            }
            "slide-row-bottom" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                {
                    let last = s.rows.len() - 1;
                    if s.cursor_row < last {
                        let row = s.rows.remove(s.cursor_row);
                        s.rows.push(row);
                        s.cursor_row = last;
                    }
                }
            }
            "slide-row-top" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                    && s.cursor_row > 0
                {
                    let row = s.rows.remove(s.cursor_row);
                    s.rows.insert(0, row);
                    s.cursor_row = 0;
                }
            }

            // --- Column resize / visibility ---
            "resize-cols-max" => {
                if let Some(s) = self.stack.active_mut() {
                    resize_all_columns(s);
                }
            }
            "unhide-cols" => {
                if let Some(s) = self.stack.active_mut() {
                    for col in &mut s.columns {
                        if col.width == Some(0) {
                            col.width = None;
                        }
                    }
                }
            }

            // --- Sorting ---
            "sort-keys-asc" => {
                if let Some(s) = self.stack.active_mut() {
                    s.sort_by_keys(SortDirection::Ascending);
                }
            }
            "sort-keys-desc" => {
                if let Some(s) = self.stack.active_mut() {
                    s.sort_by_keys(SortDirection::Descending);
                }
            }
            "sort-asc-add" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                {
                    s.sort_by_add(idx, SortDirection::Ascending);
                }
            }
            "sort-desc-add" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(idx) = resolve_cursor_col_idx(s)
                {
                    s.sort_by_add(idx, SortDirection::Descending);
                }
            }

            // --- Filtering / duplication ---
            "dup-rows" => {
                if let Some(s) = self.stack.active() {
                    let copy = s.all_rows_sheet();
                    self.stack.push(copy);
                }
            }
            "dup-selected-deep" | "dup-rows-deep" => {
                // In Rust, Row/Value are all owned; deep copy == shallow copy
                if longname == "dup-rows-deep" {
                    if let Some(s) = self.stack.active() {
                        let copy = s.all_rows_sheet();
                        self.stack.push(copy);
                    }
                } else if let Some(s) = self.stack.active()
                    && s.num_selected() > 0
                {
                    let copy = s.selected_rows_sheet();
                    self.stack.push(copy);
                }
            }

            // --- Search ---
            "search-keys" => {
                self.last_search_scope = SearchScope::KeyCols;
                self.mode = InputMode::SearchForward(LineEditor::new(""));
            }
            "search-cols" => {
                self.mode = InputMode::SearchForwardAllCols(LineEditor::new(""));
            }
            "searchr-cols" => {
                self.mode = InputMode::SearchBackwardAllCols(LineEditor::new(""));
            }

            // --- Editing ---
            "fill-down" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    let source_idx = s.columns[col_idx].source_idx;
                    s.fill_down(source_idx);
                }
            }
            "delete-cell" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    let source_idx = s.columns[col_idx].source_idx;
                    s.set_cell(s.cursor_row, source_idx, visidata_core::Value::Null);
                }
            }
            "delete-cells" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    let source_idx = s.columns[col_idx].source_idx;
                    let selected_indices: Vec<usize> = s
                        .rows
                        .iter()
                        .enumerate()
                        .filter(|(_, r)| r.selected)
                        .map(|(i, _)| i)
                        .collect();
                    for idx in selected_indices {
                        s.set_cell(idx, source_idx, visidata_core::Value::Null);
                    }
                }
            }
            "open-new" => {
                let sheet = Sheet::new("unnamed");
                self.stack.push(sheet);
            }

            // --- Clipboard ---
            "cut-row" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                    && let Some(row) = s.delete_row_at(s.cursor_row)
                {
                    self.clipboard.yank_rows(vec![row]);
                    self.status = "cut row".into();
                }
            }
            "cut-selected" => {
                if let Some(s) = self.stack.active_mut() {
                    let rows: Vec<_> = s
                        .rows
                        .iter()
                        .filter(|r| r.selected)
                        .cloned()
                        .collect();
                    if !rows.is_empty() {
                        let count = s.delete_selected_rows();
                        self.clipboard.yank_rows(rows);
                        self.status = format!("cut {count} rows");
                    }
                }
            }
            "yank-row" => {
                if let Some(s) = self.stack.active()
                    && !s.rows.is_empty()
                {
                    let row = s.rows[s.cursor_row].clone();
                    self.clipboard.yank_rows(vec![row]);
                    self.status = "yanked row".into();
                }
            }
            "paste-after" => {
                if let visidata_core::clipboard::ClipboardContent::Rows(rows) =
                    self.clipboard.content().clone()
                    && let Some(s) = self.stack.active_mut()
                {
                    let insert_at = (s.cursor_row + 1).min(s.rows.len());
                    for (offset, row) in rows.into_iter().enumerate() {
                        s.rows.insert(insert_at + offset, row);
                    }
                    s.modified = true;
                }
            }

            // --- Redo ---
            "redo" => self.do_redo(),

            // --- Navigation ---
            "go-screen-top" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_row = s.top_row;
                }
            }
            "go-screen-middle" => {
                if let Some(s) = self.stack.active_mut() {
                    let h = crossterm::terminal::size().map_or(20, |(_, h)| h as usize);
                    s.cursor_row = (s.top_row + h / 2).min(s.rows.len().saturating_sub(1));
                }
            }
            "go-screen-bottom" => {
                if let Some(s) = self.stack.active_mut() {
                    let h = crossterm::terminal::size().map_or(20, |(_, h)| h as usize);
                    s.cursor_row = (s.top_row + h.saturating_sub(3)).min(s.rows.len().saturating_sub(1));
                }
            }
            "jump-prev" => self.jump_prev_sheet(),

            // --- Selection before/after cursor ---
            "select-before" => {
                if let Some(s) = self.stack.active_mut() {
                    let cur = s.cursor_row;
                    for row in &mut s.rows[..cur] { row.selected = true; }
                }
            }
            "stoggle-before" => {
                if let Some(s) = self.stack.active_mut() {
                    let cur = s.cursor_row;
                    for row in &mut s.rows[..cur] { row.selected = !row.selected; }
                }
            }
            "unselect-before" => {
                if let Some(s) = self.stack.active_mut() {
                    let cur = s.cursor_row;
                    for row in &mut s.rows[..cur] { row.selected = false; }
                }
            }
            "select-after" => {
                if let Some(s) = self.stack.active_mut() {
                    let cur = s.cursor_row;
                    for row in &mut s.rows[cur..] { row.selected = true; }
                }
            }
            "stoggle-after" => {
                if let Some(s) = self.stack.active_mut() {
                    let cur = s.cursor_row;
                    for row in &mut s.rows[cur..] { row.selected = !row.selected; }
                }
            }
            "unselect-after" => {
                if let Some(s) = self.stack.active_mut() {
                    let cur = s.cursor_row;
                    for row in &mut s.rows[cur..] { row.selected = false; }
                }
            }

            // --- Column operations ---
            "rename-col-selected" => {
                // Rename current column to the value of the first selected row in that col
                if let Some(s) = self.stack.active_mut()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    let new_name = s.rows.iter()
                        .find(|r| r.selected)
                        .map(|r| s.columns[col_idx].display_value(r));
                    if let Some(name) = new_name {
                        let col_id = s.columns[col_idx].id.0;
                        s.rename_column(col_id, name);
                    } else {
                        self.status = "no selected rows".into();
                    }
                }
            }
            "rename-cols-row" => {
                // Rename all visible cols using current row values
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                {
                    let cur = s.cursor_row;
                    let renames: Vec<(usize, String)> = s.columns.iter()
                        .enumerate()
                        .filter(|(_, c)| !c.is_hidden())
                        .map(|(i, c)| (i, c.display_value(&s.rows[cur])))
                        .collect();
                    for (i, name) in renames {
                        let col_id = s.columns[i].id.0;
                        s.rename_column(col_id, name);
                    }
                }
            }
            "cut-cell" => {
                if let Some(s) = self.stack.active_mut()
                    && !s.rows.is_empty()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    let source_idx = s.columns[col_idx].source_idx;
                    let val = s.rows[s.cursor_row].get(source_idx).clone();
                    self.clipboard.yank_cell(val);
                    s.set_cell(s.cursor_row, source_idx, visidata_core::Value::Null);
                    self.status = "cut cell".into();
                }
            }

            // --- Transpose ---
            "transpose" => {
                if let Some(s) = self.stack.active() {
                    let t = visidata_core::sheets::transpose_sheet(s);
                    self.stack.push(t);
                }
            }
            // --- Freeze column ---
            "freeze-col" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    visidata_core::sheets::freeze_column(s, col_idx);
                    self.status = "column frozen".into();
                }
            }
            // --- Add incremental column ---
            "addcol-incr" => {
                if let Some(s) = self.stack.active_mut() {
                    visidata_core::sheets::add_incr_column(s, 1, 1);
                }
            }
            // --- Frequency for all key cols ---
            "freq-keys" => {
                if let Some(s) = self.stack.active() {
                    let key_indices: Vec<usize> = s.columns.iter().enumerate()
                        .filter(|(_, c)| c.is_key)
                        .map(|(i, _)| i)
                        .collect();
                    if key_indices.is_empty() {
                        self.status = "no key columns".into();
                    } else {
                        // Frequency on first key column for now
                        let freq = s.frequency_sheet(key_indices[0]);
                        self.stack.push(freq);
                    }
                }
            }
            // --- One-line frequency summary ---
            "freq-summary" => {
                if let Some(s) = self.stack.active()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    let col = &s.columns[col_idx];
                    let total = s.rows.len();
                    let mut counts = std::collections::HashMap::<String, usize>::new();
                    for row in &s.rows {
                        *counts.entry(col.display_value(row)).or_insert(0) += 1;
                    }
                    let distinct = counts.len();
                    let top = counts.iter().max_by_key(|(_, n)| *n)
                        .map(|(v, n)| format!("{v}({n})"))
                        .unwrap_or_default();
                    self.status = format!("{distinct} distinct / {total} total / top: {top}");
                }
            }
            // --- Join type picker ---
            "join-type-inner" => self.join_with_type(visidata_core::sheets::JoinType::Inner),
            "join-type-left"  => self.join_with_type(visidata_core::sheets::JoinType::Left),
            "join-type-right" => self.join_with_type(visidata_core::sheets::JoinType::Right),
            "join-type-outer" => self.join_with_type(visidata_core::sheets::JoinType::Outer),

            // --- Describe all sheets ---
            "describe-all" => {
                let all: Vec<&Sheet> = self.stack.iter().collect();
                if all.is_empty() {
                    return;
                }
                // Build a combined describe sheet
                let first = visidata_core::sheets::describe_sheet(all[0]);
                let mut combined = first;
                for s in all.iter().skip(1) {
                    let d = visidata_core::sheets::describe_sheet(s);
                    combined.rows.extend(d.rows);
                }
                combined.name = "describe_all".into();
                self.stack.push(combined);
            }

            // --- quitguard ---
            "quit-sheet" => {
                let guard = self.options.get_global("quitguard");
                let modified = self.stack.active().is_some_and(|s| s.modified);
                if matches!(guard, visidata_core::Value::Bool(true)) && modified {
                    if self.pending_quit {
                        self.stack.pop();
                        self.pending_quit = false;
                        if self.stack.is_empty() { self.running = false; }
                    } else {
                        self.pending_quit = true;
                        self.status = "sheet modified — press q again to quit".into();
                    }
                } else {
                    self.pending_quit = false;
                    self.stack.pop();
                    if self.stack.is_empty() { self.running = false; }
                }
            }

            // --- Error sheet ---
            "error-recent" => {
                if let Some(err) = self.last_errors.back().cloned() {
                    let sheet = visidata_core::sheets::text_sheet("error", &err);
                    self.stack.push(sheet);
                } else {
                    self.status = "no errors".into();
                }
            }

            // --- Group A: wiring ---
            "save-all" => {
                let mut saved = 0usize;
                for sheet in self.stack.iter() {
                    if let Some(ref path) = sheet.source {
                        let _ = visidata_loaders::save_sheet(sheet, path);
                        saved += 1;
                    }
                }
                self.status = format!("saved {saved} sheets");
            }
            "open-config" => {
                if let Some(path) = visidata_core::config::default_config_path() {
                    match visidata_core::sheets::text_sheet_from_file(&path) {
                        Ok(s) => self.stack.push(s),
                        Err(e) => self.status = format!("cannot open config: {e}"),
                    }
                } else {
                    self.status = "no config path found".into();
                }
            }
            "sheets-all" => {
                let names = self.all_sheet_names.clone();
                let mut sheet = Sheet::new("sheets_all");
                sheet.add_column("name", 0);
                for name in &names {
                    sheet.add_row(vec![visidata_core::Value::Text(name.clone())]);
                }
                self.stack.push(sheet);
            }
            "macro-sheet" => {
                let macros = self.macro_store.all().to_vec();
                let mut sheet = Sheet::new("macros");
                sheet.add_column("name", 0);
                sheet.add_column("keystrokes", 1);
                for mac in &macros {
                    sheet.add_row(vec![
                        visidata_core::Value::Text(mac.name.clone()),
                        #[expect(clippy::cast_possible_wrap, reason = "keystroke count < i64::MAX")]
                        visidata_core::Value::Int(mac.keystrokes.len() as i64),
                    ]);
                }
                self.stack.push(sheet);
            }
            "save-cmdlog" => self.save_command_log(),
            "options-sheet-local" => {
                let sheet_name = self.stack.active().map(|s| s.name.clone());
                let sheet = visidata_core::options::options_sheet(&self.options);
                let mut local = sheet;
                local.name = format!("options[{}]", sheet_name.unwrap_or_default());
                self.stack.push(local);
            }
            "addcol-new" => {
                if let Some(s) = self.stack.active_mut() {
                    let new_source_idx = s.columns.len();
                    let col_id = visidata_core::ColumnId(new_source_idx);
                    let mut col = visidata_core::Column::new(col_id, "new_col", new_source_idx);
                    col.width = Some(10);
                    for row in &mut s.rows {
                        row.set(new_source_idx, visidata_core::Value::Null);
                    }
                    s.columns.push(col);
                }
            }
            "join-sheets-all" => {
                self.join_all_sheets();
            }
            "sort-keys-asc-add" => {
                if let Some(s) = self.stack.active_mut() {
                    let key_indices: Vec<usize> = s.columns.iter().enumerate()
                        .filter(|(_, c)| c.is_key)
                        .map(|(i, _)| i)
                        .collect();
                    for idx in key_indices {
                        s.sort_by_add(idx, SortDirection::Ascending);
                    }
                }
            }
            "sort-keys-desc-add" => {
                if let Some(s) = self.stack.active_mut() {
                    let key_indices: Vec<usize> = s.columns.iter().enumerate()
                        .filter(|(_, c)| c.is_key)
                        .map(|(i, _)| i)
                        .collect();
                    for idx in key_indices {
                        s.sort_by_add(idx, SortDirection::Descending);
                    }
                }
            }

            // --- Group B: column types ---
            "type-any" => {
                if let Some(s) = self.stack.active_mut() {
                    set_cursor_col_type(s, visidata_core::ColumnType::Any);
                }
            }
            "type-len" => {
                if let Some(s) = self.stack.active_mut() {
                    set_cursor_col_type(s, visidata_core::ColumnType::Len);
                }
            }

            // --- Group B: system clipboard ---
            "syscopy-cell" => self.syscopy(SysClipMode::Cell),
            "syscopy-row" => self.syscopy(SysClipMode::Row),
            "syscopy-selected" => self.syscopy(SysClipMode::Selected),
            "syspaste-cells" => self.syspaste(),

            // --- Group B: external editor ---
            "sysedit-cell" => self.sysedit_cell(),

            // --- Group B: JSON expand/contract ---
            "expand-col" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    visidata_core::sheets::expand_json_col(s, col_idx);
                }
            }
            "contract-col" => {
                if let Some(s) = self.stack.active_mut()
                    && let Some(col_idx) = resolve_cursor_col_idx(s)
                {
                    visidata_core::sheets::contract_json_col(s, col_idx);
                }
            }

            // --- TUI ---
            "redraw" => {
                // Handled externally by the event loop — just mark for redraw.
                // In ratatui the next frame will redraw automatically.
            }
            "reload-sheet" => {
                self.reload_current_sheet();
            }

            _ => {
                self.status = format!("unknown command: {longname}");
            }
        }
    }

    /// Undo the last action, saving it to the redo stack.
    fn do_undo(&mut self) {
        let Some(sheet) = self.stack.active_mut() else {
            return;
        };
        // Peek at the top action before popping
        let action = sheet.undo_stack.pop();
        if let Some(action) = action {
            // Re-apply the undo by cloning the action onto the redo stack before undoing
            self.redo_stack.push(action.clone());
            // Now manually undo the action (replicate sheet.undo() logic inline
            // so we don't double-pop)
            match action {
                visidata_core::undo::UndoAction::SetCell {
                    row_idx,
                    col_source_idx,
                    old_value,
                } => {
                    if let Some(row) = sheet.rows.get_mut(row_idx) {
                        row.set(col_source_idx, old_value);
                    }
                }
                visidata_core::undo::UndoAction::BulkSetCell { changes } => {
                    for (row_idx, col_source_idx, old_value) in changes {
                        if let Some(row) = sheet.rows.get_mut(row_idx) {
                            row.set(col_source_idx, old_value);
                        }
                    }
                }
                visidata_core::undo::UndoAction::InsertRow { row_idx } => {
                    if row_idx < sheet.rows.len() {
                        sheet.rows.remove(row_idx);
                    }
                }
                visidata_core::undo::UndoAction::DeleteRow { row_idx, row } => {
                    let idx = row_idx.min(sheet.rows.len());
                    sheet.rows.insert(idx, row);
                }
                visidata_core::undo::UndoAction::DeleteRows { entries } => {
                    for (idx, row) in entries.into_iter().rev() {
                        let insert_at = idx.min(sheet.rows.len());
                        sheet.rows.insert(insert_at, row);
                    }
                }
                visidata_core::undo::UndoAction::RenameColumn { col_id, old_name } => {
                    if let Some(col) = sheet.columns.iter_mut().find(|c| c.id.0 == col_id) {
                        col.name = old_name;
                    }
                }
                visidata_core::undo::UndoAction::SetColType { col_id, old_type } => {
                    if let Some(col) = sheet.columns.iter_mut().find(|c| c.id.0 == col_id) {
                        col.col_type = old_type;
                    }
                }
                visidata_core::undo::UndoAction::ReorderRows { order } => {
                    let mut by_id: std::collections::HashMap<_, _> =
                        sheet.rows.drain(..).map(|r| (r.id, r)).collect();
                    for id in order {
                        if let Some(row) = by_id.remove(&id) {
                            sheet.rows.push(row);
                        }
                    }
                }
            }
            sheet.clamp_cursor();
            if sheet.undo_stack.is_empty() {
                sheet.modified = false;
            }
            self.status = "undone".into();
        } else {
            self.status = "nothing to undo".into();
        }
    }

    /// Redo the last undone action.
    fn do_redo(&mut self) {
        use visidata_core::undo::UndoAction;

        let Some(action) = self.redo_stack.pop() else {
            self.status = "nothing to redo".into();
            return;
        };
        let Some(sheet) = self.stack.active_mut() else {
            return;
        };
        // Re-apply the action forward
        match &action {
            UndoAction::SetCell {
                row_idx,
                col_source_idx,
                ..
            } => {
                // We stored old_value in the redo action — we need the "new" value
                // which we don't have. For redo we need the forward value.
                // Since we can't reconstruct "new value" from the undo entry alone,
                // skip for SetCell (requires a before/after pair — a future improvement).
                let _ = (row_idx, col_source_idx);
                self.redo_stack.push(action); // put back
                self.status = "redo not supported for cell edits yet".into();
                return;
            }
            UndoAction::BulkSetCell { .. } => {
                self.redo_stack.push(action);
                self.status = "redo not supported for bulk edits yet".into();
                return;
            }
            UndoAction::InsertRow { row_idx } => {
                let num_values = sheet.columns.len();
                let idx = (*row_idx).min(sheet.rows.len());
                sheet.rows.insert(idx, visidata_core::Row::new(vec![visidata_core::Value::Null; num_values]));
                sheet.modified = true;
                sheet.undo_stack.push(action);
            }
            UndoAction::DeleteRow { row_idx, .. } => {
                if *row_idx < sheet.rows.len() {
                    let row = sheet.rows.remove(*row_idx);
                    sheet.modified = true;
                    sheet.undo_stack.push(UndoAction::DeleteRow {
                        row_idx: *row_idx,
                        row,
                    });
                }
            }
            UndoAction::DeleteRows { entries } => {
                let indices: Vec<usize> = entries.iter().map(|(i, _)| *i).collect();
                let mut new_entries: Vec<(usize, visidata_core::Row)> = Vec::new();
                for &idx in indices.iter().rev() {
                    if idx < sheet.rows.len() {
                        new_entries.push((idx, sheet.rows.remove(idx)));
                    }
                }
                if !new_entries.is_empty() {
                    sheet.modified = true;
                    sheet.undo_stack.push(UndoAction::DeleteRows {
                        entries: new_entries,
                    });
                }
            }
            UndoAction::RenameColumn { col_id, old_name: _ } => {
                // We don't have the "new name" here — skip
                let _ = col_id;
                self.redo_stack.push(action);
                self.status = "redo not supported for rename yet".into();
                return;
            }
            UndoAction::SetColType { col_id, old_type: _ } => {
                let _ = col_id;
                self.redo_stack.push(action);
                self.status = "redo not supported for type change yet".into();
                return;
            }
            UndoAction::ReorderRows { .. } => {
                // Redo a sort: re-apply the current sort keys
                if let Some(s) = self.stack.active_mut() {
                    s.resort();
                }
                self.status = "redone".into();
                return;
            }
        }
        sheet.clamp_cursor();
        self.status = "redone".into();
    }

    /// Execute a search in the given direction using the current scope.
    fn execute_search(&mut self, pattern: &str, forward: bool) {
        if pattern.is_empty() {
            return;
        }
        let Some(sheet) = self.stack.active_mut() else {
            return;
        };
        let result = match self.last_search_scope {
            SearchScope::CurrentCol => {
                let col_idx = resolve_cursor_col_idx(sheet).unwrap_or(0);
                if forward {
                    sheet.search_forward(col_idx, pattern)
                } else {
                    sheet.search_backward(col_idx, pattern)
                }
            }
            SearchScope::KeyCols => {
                // Search first key column, falling back to cursor column
                let key_idx = sheet
                    .columns
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.is_key)
                    .map_or_else(
                        || resolve_cursor_col_idx(sheet).unwrap_or(0),
                        |(i, _)| i,
                    );
                if forward {
                    sheet.search_forward(key_idx, pattern)
                } else {
                    sheet.search_backward(key_idx, pattern)
                }
            }
        };
        if let Some(row_idx) = result {
            sheet.cursor_row = row_idx;
        } else {
            self.status = format!("not found: {pattern}");
        }
    }

    /// Execute a search across all visible columns.
    fn execute_search_all_cols(&mut self, pattern: &str, forward: bool) {
        if pattern.is_empty() {
            return;
        }
        let Some(sheet) = self.stack.active_mut() else {
            return;
        };
        let result = if forward {
            sheet.search_forward_all_cols(pattern)
        } else {
            sheet.search_backward_all_cols(pattern)
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

            // Modified indicator
            let mod_indicator = if sheet.modified { " [+]" } else { "" };

            // Loading indicator
            let load_text = sheet.loading_state.status_text();

            // Aggregator indicator (GAP-123)
            let agg_text = sheet.current_column()
                .and_then(|col| col.aggregators.first().copied())
                .map(|f| {
                    let col = sheet.current_column().unwrap();
                    let val = visidata_core::aggregation::aggregate(col, &sheet.rows, f);
                    format!(" {}={val}", f.name())
                })
                .unwrap_or_default();

            self.status = format!(
                "{}{mod_indicator}{load_text} | {}r x {}c | row {} col {} {type_indicator}{}{agg_text}",
                sheet.name,
                sheet.num_rows(),
                sheet.visible_columns().len(),
                sheet.cursor_row + 1,
                sheet.cursor_col + 1,
                sel_text,
            );
        }
    }

    /// Handle a mouse event.
    fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if let Some(sheet) = self.stack.active_mut() {
                    sheet.cursor_up(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if let Some(sheet) = self.stack.active_mut() {
                    sheet.cursor_down(3);
                }
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                // Row 0 of the table area = header; row 1+ = data rows.
                // Data rows start at y=1 (after header).
                let click_row = mouse.row as usize;
                if click_row == 0 {
                    // Header click — do nothing
                } else if let Some(sheet) = self.stack.active_mut() {
                    // Data row = click_row - 1 (subtract header) + top_row
                    let data_row_idx = (click_row.saturating_sub(1)) + sheet.top_row;
                    if data_row_idx < sheet.num_rows() {
                        sheet.cursor_row = data_row_idx;
                    }
                }
            }
            _ => {}
        }
        self.update_status();
    }

    /// Handle a key event while in menu mode.
    fn handle_menu_key(&mut self, key: KeyEvent) {
        let num_menus = self.menu_bar.menus.len();
        let num_items = self
            .menu_bar
            .menus
            .get(self.menu_state.menu_idx)
            .map_or(0, |m| match m {
                MenuItem::Submenu { items, .. } => items.len(),
                _ => 0,
            });

        match key.code {
            KeyCode::Esc | KeyCode::F(10) => {
                self.menu_state.open = false;
                self.mode = InputMode::Normal;
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.menu_state.prev_menu(num_menus);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.menu_state.next_menu(num_menus);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.menu_state.prev_item(num_items);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.menu_state.next_item(num_items);
            }
            KeyCode::Enter => {
                // Execute the selected menu item.
                let menu_idx = self.menu_state.menu_idx;
                let item_idx = self.menu_state.item_idx;
                if let Some(MenuItem::Submenu { items, .. }) = self.menu_bar.menus.get(menu_idx)
                    && let Some(MenuItem::Command { longname, .. }) = items.get(item_idx)
                {
                    let longname = longname.clone();
                    self.menu_state.open = false;
                    self.mode = InputMode::Normal;
                    self.dispatch_command(&longname);
                    self.update_status();
                    return;
                }
                self.menu_state.open = false;
                self.mode = InputMode::Normal;
            }
            _ => {}
        }
        self.update_status();
    }

    /// Handle a key event while the help overlay is open.
    fn handle_help_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::F(1) => {
                self.mode = InputMode::Normal;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let InputMode::Help { scroll } = &mut self.mode {
                    *scroll = scroll.saturating_add(1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let InputMode::Help { scroll } = &mut self.mode {
                    *scroll = scroll.saturating_sub(1);
                }
            }
            KeyCode::PageDown => {
                if let InputMode::Help { scroll } = &mut self.mode {
                    *scroll = scroll.saturating_add(20);
                }
            }
            KeyCode::PageUp => {
                if let InputMode::Help { scroll } = &mut self.mode {
                    *scroll = scroll.saturating_sub(20);
                }
            }
            _ => {}
        }
    }

    /// Apply display format strings from options to all columns of the active sheet.
    ///
    /// Called after loading a sheet or after options change (GAP-106).
    fn apply_display_formats_from_options(&mut self) {
        let float_fmt = match self.options.get_global("disp_float_fmt") {
            visidata_core::Value::Text(ref s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };
        let int_fmt = match self.options.get_global("disp_int_fmt") {
            visidata_core::Value::Text(ref s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };
        let date_fmt = match self.options.get_global("disp_date_fmt") {
            visidata_core::Value::Text(ref s) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };
        if let Some(sheet) = self.stack.active_mut() {
            sheet.apply_display_formats(
                float_fmt.as_deref(),
                int_fmt.as_deref(),
                date_fmt.as_deref(),
            );
        }
    }

    /// Join the top two sheets with the given join type.
    fn join_with_type(&mut self, join_type: visidata_core::sheets::JoinType) {
        let sheets: Vec<&Sheet> = self.stack.iter().collect();
        if sheets.len() < 2 {
            self.status = "need at least 2 sheets to join".into();
            return;
        }
        let right = sheets[sheets.len() - 1];
        let left = sheets[sheets.len() - 2];
        let joined = visidata_core::sheets::join_sheets(left, right, join_type);
        self.stack.push(joined);
    }

    /// Join the top two sheets on the stack by key columns (inner join).
    fn join_top_two_sheets(&mut self) {
        let sheets: Vec<&Sheet> = self.stack.iter().collect();
        if sheets.len() < 2 {
            self.status = "need at least 2 sheets to join".into();
            return;
        }
        let right = sheets[0]; // active (top)
        let left = sheets[1]; // previous
        let joined =
            visidata_core::sheets::join_sheets(left, right, visidata_core::sheets::JoinType::Inner);
        self.stack.push(joined);
    }

    /// Replay the next queued macro keystroke.
    fn replay_next_macro_key(&mut self) {
        let Some(key_str) = self.macro_replay.first().cloned() else {
            return;
        };
        self.macro_replay.remove(0);

        match key_str.as_str() {
            "j" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_down(1);
                }
            }
            "k" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_up(1);
                }
            }
            "l" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_right(1);
                }
            }
            "h" => {
                if let Some(s) = self.stack.active_mut() {
                    s.cursor_left(1);
                }
            }
            other => {
                self.dispatch_command(other);
            }
        }
        self.update_status();
    }

    /// Push a sheet onto the stack, recording previous index and all-sheets history.
    #[expect(dead_code, reason = "will replace direct stack.push() calls incrementally")]
    fn push_sheet(&mut self, sheet: Sheet) {
        self.prev_sheet_idx = Some(self.stack.len().saturating_sub(1));
        self.all_sheet_names.push(sheet.name.clone());
        self.stack.push(sheet);
    }

    /// Jump to the previously active sheet (Ctrl+^).
    fn jump_prev_sheet(&mut self) {
        if let Some(prev) = self.prev_sheet_idx {
            let cur = self.stack.len().saturating_sub(1);
            if prev != cur && prev < self.stack.len() {
                // Swap top with prev by rotating
                self.prev_sheet_idx = Some(cur);
                // Navigate: pop to prev if prev == cur-1, else just set active
                // Simple approach: move cursor within the stack via pop/push
                // For now, pop until we reach prev+1 sheets
                while self.stack.len() > prev + 1 {
                    self.stack.pop();
                }
            } else {
                self.status = "no previous sheet".into();
            }
        } else {
            self.status = "no previous sheet".into();
        }
    }

    /// Record an error message for the error sheet.
    #[expect(dead_code, reason = "will be called from error paths in next batch")]
    fn record_error(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.last_errors.push_back(msg.clone());
        if self.last_errors.len() > 50 {
            self.last_errors.pop_front();
        }
        self.status = msg;
    }

    /// Save the command log to a .vdj file.
    fn save_command_log(&mut self) {
        let path = std::path::PathBuf::from("vd_session.vdj");
        let json = serde_json::to_string_pretty(&self.command_log)
            .unwrap_or_default();
        match std::fs::write(&path, json) {
            Ok(()) => self.status = format!("saved command log to {}", path.display()),
            Err(e) => self.status = format!("save failed: {e}"),
        }
    }

    /// Join all sheets in the stack cascade (g&).
    fn join_all_sheets(&mut self) {
        let sheets: Vec<&Sheet> = self.stack.iter().collect();
        if sheets.len() < 2 {
            self.status = "need at least 2 sheets to join".into();
            return;
        }
        let mut result = visidata_core::sheets::join_sheets(sheets[0], sheets[1], visidata_core::sheets::JoinType::Inner);
        for s in sheets.iter().skip(2) {
            result = visidata_core::sheets::join_sheets(&result, s, visidata_core::sheets::JoinType::Inner);
        }
        self.stack.push(result);
    }

    /// Copy data to the system clipboard.
    fn syscopy(&mut self, mode: SysClipMode) {
        let text = match mode {
            SysClipMode::Cell => {
                self.stack.active().and_then(|s| {
                    if s.rows.is_empty() { return None; }
                    let vis = s.visible_columns();
                    let col = vis.get(s.cursor_col)?;
                    Some(col.display_value(&s.rows[s.cursor_row]))
                })
            }
            SysClipMode::Row => {
                self.stack.active().and_then(|s| {
                    if s.rows.is_empty() { return None; }
                    let vis = s.visible_columns();
                    let fields: Vec<_> = vis.iter().map(|c| c.display_value(&s.rows[s.cursor_row])).collect();
                    Some(fields.join("\t"))
                })
            }
            SysClipMode::Selected => {
                self.stack.active().map(|s| {
                    let vis = s.visible_columns();
                    s.rows.iter()
                        .filter(|r| r.selected)
                        .map(|row| vis.iter().map(|c| c.display_value(row)).collect::<Vec<_>>().join("\t"))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            }
        };
        let Some(text) = text else { return; };
        if let Err(e) = sys_clipboard_write(&text) {
            self.status = format!("clipboard error: {e}");
        } else {
            self.status = format!("copied {} chars", text.len());
        }
    }

    /// Paste from the system clipboard at cursor position.
    fn syspaste(&mut self) {
        match sys_clipboard_read() {
            Ok(text) => {
                if let Some(sheet) = self.stack.active_mut()
                    && !sheet.rows.is_empty()
                {
                    let vis_indices: Vec<usize> = sheet.visible_columns().iter()
                        .map(|c| c.source_idx)
                        .collect();
                    for (row_offset, line) in text.lines().enumerate() {
                        let row_idx = sheet.cursor_row + row_offset;
                        if row_idx >= sheet.rows.len() { break; }
                        for (col_offset, field) in line.split('\t').enumerate() {
                            let source_idx = vis_indices.get(sheet.cursor_col + col_offset).copied();
                            if let Some(si) = source_idx {
                                sheet.set_cell(row_idx, si, visidata_core::Value::Text(field.to_owned()));
                            }
                        }
                    }
                }
            }
            Err(e) => self.status = format!("paste error: {e}"),
        }
    }

    /// Open the current cell in `$EDITOR` and read back the result.
    fn sysedit_cell(&mut self) {
        let Some(sheet) = self.stack.active_mut() else { return; };
        if sheet.rows.is_empty() { return; }
        let Some(col_idx) = resolve_cursor_col_idx(sheet) else { return; };
        let display = sheet.columns[col_idx].display_value(&sheet.rows[sheet.cursor_row]);
        let source_idx = sheet.columns[col_idx].source_idx;
        let row_idx = sheet.cursor_row;

        // Write to temp file
        let mut tmp = std::env::temp_dir();
        tmp.push("vd_cell_edit.txt");
        if std::fs::write(&tmp, &display).is_err() { return; }

        // Suspend TUI, spawn editor
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| "vi".into());

        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        crossterm::terminal::disable_raw_mode().ok();

        let status = std::process::Command::new(&editor).arg(&tmp).status();

        crossterm::terminal::enable_raw_mode().ok();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen);

        if status.is_ok()
            && let Ok(new_val) = std::fs::read_to_string(&tmp)
        {
            let val = visidata_core::Value::Text(new_val.trim_end_matches('\n').to_owned());
            sheet.set_cell(row_idx, source_idx, val);
        }
        let _ = std::fs::remove_file(&tmp);
    }

    /// Build a `LoaderOptions` snapshot from current app options (GAP-105, 135).
    fn loader_options_snapshot(&self) -> visidata_loaders::LoaderOptions {
        let csv_delimiter = match self.options.get_global("csv_delimiter") {
            visidata_core::Value::Text(ref s) => s.bytes().next().unwrap_or(b','),
            _ => b',',
        };
        let csv_quote_char = match self.options.get_global("csv_quotechar") {
            visidata_core::Value::Text(ref s) => s.bytes().next().unwrap_or(b'"'),
            _ => b'"',
        };
        // Collect ext-loader options: any option whose key starts with "vd_"
        let ext_options = self.options
            .all_definitions()
            .into_iter()
            .filter(|d| d.name.starts_with("vd_"))
            .filter_map(|d| {
                if let visidata_core::Value::Text(ref v) = self.options.get_global(&d.name) {
                    Some((d.name.clone(), v.clone()))
                } else {
                    None
                }
            })
            .collect();
        visidata_loaders::LoaderOptions { csv_delimiter, csv_quote_char, ext_options }
    }

    /// Reload the current sheet from its source file.
    fn reload_current_sheet(&mut self) {
        let source = self.stack.active().and_then(|s| s.source.clone());
        let Some(path) = source else {
            self.status = "no source file to reload from".into();
            return;
        };
        let registry = visidata_loaders::LoaderRegistry::with_builtins();
        let opts = self.loader_options_snapshot();
        match registry.load_file_with_options(&path, &opts) {
            Ok(new_sheet) => {
                if let Some(sheet) = self.stack.active_mut() {
                    let name = sheet.name.clone();
                    let cursor_row = sheet.cursor_row;
                    let cursor_col = sheet.cursor_col;
                    sheet.rows = new_sheet.rows;
                    sheet.columns = new_sheet.columns;
                    sheet.name = name;
                    sheet.modified = false;
                    sheet.undo_stack.clear();
                    sheet.cursor_row = cursor_row.min(sheet.rows.len().saturating_sub(1));
                    sheet.cursor_col = cursor_col.min(sheet.visible_columns().len().saturating_sub(1));
                    self.status = format!("reloaded from {}", path.display());
                }
            }
            Err(e) => {
                self.status = format!("reload failed: {e}");
            }
        }
    }

    /// Save the current sheet to its source file.
    fn save_current_sheet(&mut self) {
        let Some(sheet) = self.stack.active() else {
            return;
        };
        let Some(path) = sheet.source.clone() else {
            self.status = "no source file to save to".into();
            return;
        };
        match visidata_loaders::save_sheet(sheet, &path) {
            Ok(()) => {
                self.status = format!("saved to {}", path.display());
                // Mark as no longer modified
                if let Some(sheet) = self.stack.active_mut() {
                    sheet.modified = false;
                    sheet.undo_stack.clear();
                }
            }
            Err(e) => {
                self.status = format!("save failed: {e}");
            }
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

/// Auto-fit all visible columns.
fn resize_all_columns(sheet: &mut Sheet) {
    use unicode_width::UnicodeWidthStr;

    let vis_ids: Vec<_> = sheet
        .visible_columns()
        .iter()
        .map(|c| c.id)
        .collect();

    for col_id in vis_ids {
        let Some(idx) = sheet.columns.iter().position(|c| c.id == col_id) else {
            continue;
        };
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
}

/// Set the column type for the current cursor column, with undo tracking.
fn set_cursor_col_type(sheet: &mut Sheet, col_type: ColumnType) {
    let vis = sheet.visible_columns();
    if let Some(&col) = vis.get(sheet.cursor_col) {
        sheet.set_col_type(col.id.0, col_type);
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
