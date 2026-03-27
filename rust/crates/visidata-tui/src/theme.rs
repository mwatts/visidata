//! Color theme system for the TUI.
//!
//! Defines named style slots that the renderer uses. Users can switch
//! between built-in themes or customize individual slots.

use ratatui::prelude::*;

/// A complete color theme for the TUI.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Theme display name.
    pub name: String,

    // --- Header ---
    /// Header row background.
    pub header: Style,
    /// Header cell for key columns.
    pub header_key: Style,
    /// Header cell under cursor.
    pub header_cursor: Style,

    // --- Data cells ---
    /// Default cell style.
    pub cell_default: Style,
    /// Cell under cursor (active cell).
    pub cell_cursor: Style,
    /// Cells in the cursor row (not the active cell).
    pub row_cursor: Style,
    /// Selected row cells.
    pub row_selected: Style,
    /// Key column cells.
    pub col_key: Style,

    // --- Type-specific ---
    /// Numeric (int/float) values.
    pub cell_numeric: Style,
    /// Null/empty values.
    pub cell_null: Style,
    /// Error values.
    pub cell_error: Style,

    // --- Chrome ---
    /// Status bar.
    pub status_bar: Style,
    /// Input line.
    pub input_line: Style,
    /// Column separator character style.
    pub column_sep: Style,
}

/// Context flags for resolving a cell's style.
#[derive(Debug, Clone, Copy, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "flags are independent display properties"
)]
pub struct CellContext {
    /// This is the cursor cell (active).
    pub is_cursor_cell: bool,
    /// This cell is in the cursor row.
    pub is_cursor_row: bool,
    /// This row is selected.
    pub is_selected: bool,
    /// This column is a key column.
    pub is_key_col: bool,
    /// The value is null.
    pub is_null: bool,
    /// The value is an error.
    pub is_error: bool,
    /// The value is numeric (int or float).
    pub is_numeric: bool,
}

impl Theme {
    /// The default theme, matching Python `VisiData`'s look.
    #[must_use]
    pub fn default_theme() -> Self {
        Self {
            name: "default".into(),

            header: Style::new().bold().on_dark_gray(),
            header_key: Style::new().bold().underlined().on_dark_gray(),
            header_cursor: Style::new().bold().reversed(),

            cell_default: Style::default(),
            cell_cursor: Style::new().reversed(),
            row_cursor: Style::new().fg(Color::White).on_black(),
            row_selected: Style::new().fg(Color::Cyan),
            col_key: Style::new().fg(Color::Green),

            cell_numeric: Style::default(),
            cell_null: Style::new().fg(Color::DarkGray),
            cell_error: Style::new().fg(Color::Red).bold(),

            status_bar: Style::new().fg(Color::White).on_dark_gray(),
            input_line: Style::new().fg(Color::Yellow).on_black(),
            column_sep: Style::new().fg(Color::DarkGray),
        }
    }

    /// A dark theme with higher contrast.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            name: "dark".into(),

            header: Style::new().bold().fg(Color::Black).bg(Color::Blue),
            header_key: Style::new()
                .bold()
                .underlined()
                .fg(Color::Black)
                .bg(Color::Blue),
            header_cursor: Style::new().bold().fg(Color::Black).bg(Color::Cyan),

            cell_default: Style::new().fg(Color::White),
            cell_cursor: Style::new().fg(Color::Black).bg(Color::White),
            row_cursor: Style::new().fg(Color::White).bg(Color::Indexed(236)),
            row_selected: Style::new().fg(Color::Yellow).bold(),
            col_key: Style::new().fg(Color::Green),

            cell_numeric: Style::new().fg(Color::Cyan),
            cell_null: Style::new().fg(Color::DarkGray).italic(),
            cell_error: Style::new().fg(Color::Red).bold(),

            status_bar: Style::new().fg(Color::White).bg(Color::Blue),
            input_line: Style::new().fg(Color::Yellow).bg(Color::Indexed(236)),
            column_sep: Style::new().fg(Color::DarkGray),
        }
    }

    /// A light theme for light terminal backgrounds.
    #[must_use]
    pub fn light() -> Self {
        Self {
            name: "light".into(),

            header: Style::new().bold().fg(Color::White).bg(Color::DarkGray),
            header_key: Style::new()
                .bold()
                .underlined()
                .fg(Color::White)
                .bg(Color::DarkGray),
            header_cursor: Style::new().bold().reversed(),

            cell_default: Style::new().fg(Color::Black),
            cell_cursor: Style::new().fg(Color::White).bg(Color::Blue),
            row_cursor: Style::new().fg(Color::Black).bg(Color::Indexed(254)),
            row_selected: Style::new().fg(Color::Blue).bold(),
            col_key: Style::new().fg(Color::DarkGray).bold(),

            cell_numeric: Style::new().fg(Color::DarkGray),
            cell_null: Style::new().fg(Color::LightRed).italic(),
            cell_error: Style::new().fg(Color::Red).bold(),

            status_bar: Style::new().fg(Color::White).bg(Color::DarkGray),
            input_line: Style::new().fg(Color::Blue).bg(Color::Indexed(254)),
            column_sep: Style::new().fg(Color::LightRed),
        }
    }

    /// Look up a theme by name.
    #[must_use]
    pub fn by_name(name: &str) -> Self {
        match name {
            "dark" => Self::dark(),
            "light" => Self::light(),
            _ => Self::default_theme(),
        }
    }

    /// Resolve the style for a data cell based on its context.
    #[must_use]
    pub const fn cell_style(&self, ctx: &CellContext) -> Style {
        // Priority order: cursor cell > cursor row > selected > error > null > key col > numeric > default
        if ctx.is_cursor_cell {
            return self.cell_cursor;
        }
        if ctx.is_cursor_row {
            return self.row_cursor;
        }
        if ctx.is_selected {
            return self.row_selected;
        }
        if ctx.is_error {
            return self.cell_error;
        }
        if ctx.is_null {
            return self.cell_null;
        }
        if ctx.is_key_col {
            return self.col_key;
        }
        if ctx.is_numeric {
            return self.cell_numeric;
        }
        self.cell_default
    }

    /// Resolve the style for a header cell.
    #[must_use]
    pub const fn header_style(&self, is_key: bool, is_cursor: bool) -> Style {
        if is_key {
            self.header_key
        } else if is_cursor {
            self.header_cursor
        } else {
            self.header
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_theme()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_has_name() {
        let theme = Theme::default_theme();
        assert_eq!(theme.name, "default");
    }

    #[test]
    fn dark_theme() {
        let theme = Theme::dark();
        assert_eq!(theme.name, "dark");
    }

    #[test]
    fn light_theme() {
        let theme = Theme::light();
        assert_eq!(theme.name, "light");
    }

    #[test]
    fn by_name_lookup() {
        assert_eq!(Theme::by_name("dark").name, "dark");
        assert_eq!(Theme::by_name("light").name, "light");
        assert_eq!(Theme::by_name("default").name, "default");
        assert_eq!(Theme::by_name("unknown").name, "default");
    }

    fn ctx(f: impl FnOnce(&mut CellContext)) -> CellContext {
        let mut c = CellContext::default();
        f(&mut c);
        c
    }

    #[test]
    fn cell_style_priority_cursor() {
        let theme = Theme::default_theme();
        let c = ctx(|c| {
            c.is_cursor_cell = true;
            c.is_cursor_row = true;
            c.is_selected = true;
            c.is_key_col = true;
        });
        assert_eq!(theme.cell_style(&c), theme.cell_cursor);
    }

    #[test]
    fn cell_style_priority_cursor_row() {
        let theme = Theme::default_theme();
        let c = ctx(|c| {
            c.is_cursor_row = true;
            c.is_selected = true;
        });
        assert_eq!(theme.cell_style(&c), theme.row_cursor);
    }

    #[test]
    fn cell_style_priority_selected() {
        let theme = Theme::default_theme();
        let c = ctx(|c| {
            c.is_selected = true;
        });
        assert_eq!(theme.cell_style(&c), theme.row_selected);
    }

    #[test]
    fn cell_style_error() {
        let theme = Theme::default_theme();
        let c = ctx(|c| {
            c.is_error = true;
        });
        assert_eq!(theme.cell_style(&c), theme.cell_error);
    }

    #[test]
    fn cell_style_null() {
        let theme = Theme::default_theme();
        let c = ctx(|c| {
            c.is_null = true;
        });
        assert_eq!(theme.cell_style(&c), theme.cell_null);
    }

    #[test]
    fn cell_style_key_col() {
        let theme = Theme::default_theme();
        let c = ctx(|c| {
            c.is_key_col = true;
        });
        assert_eq!(theme.cell_style(&c), theme.col_key);
    }

    #[test]
    fn cell_style_numeric() {
        let theme = Theme::default_theme();
        let c = ctx(|c| {
            c.is_numeric = true;
        });
        assert_eq!(theme.cell_style(&c), theme.cell_numeric);
    }

    #[test]
    fn cell_style_default() {
        let theme = Theme::default_theme();
        let c = CellContext::default();
        assert_eq!(theme.cell_style(&c), theme.cell_default);
    }

    #[test]
    fn header_style_variants() {
        let theme = Theme::default_theme();
        assert_eq!(theme.header_style(false, false), theme.header);
        assert_eq!(theme.header_style(true, false), theme.header_key);
        assert_eq!(theme.header_style(false, true), theme.header_cursor);
        // Key takes priority over cursor
        assert_eq!(theme.header_style(true, true), theme.header_key);
    }

    #[test]
    fn default_trait() {
        let theme = Theme::default();
        assert_eq!(theme.name, "default");
    }
}
