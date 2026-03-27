//! Menu system data model.
//!
//! Menus are trees of items. Each item has a label and either:
//! - A command longname to execute (leaf)
//! - A list of child items (submenu)

/// A menu item: either a command leaf or a submenu with children.
#[derive(Debug, Clone)]
pub enum MenuItem {
    /// A command item with label and command longname.
    Command { label: String, longname: String },
    /// A submenu with label and child items.
    Submenu { label: String, items: Vec<Self> },
    /// A visual separator line.
    Separator,
}

impl MenuItem {
    /// Returns the display label for this item.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::Command { label, .. } | Self::Submenu { label, .. } => label,
            Self::Separator => "─",
        }
    }
}

/// The top-level menu bar containing a list of menus.
#[derive(Debug, Clone)]
pub struct MenuBar {
    /// Top-level menus.
    pub menus: Vec<MenuItem>,
}

impl MenuBar {
    /// Create a new empty menu bar.
    #[must_use]
    pub const fn new() -> Self {
        Self { menus: Vec::new() }
    }

    /// Add a menu item from a path string like `"File > Save > CSV"`.
    pub fn add_item(&mut self, path: &str, longname: &str) {
        let parts: Vec<&str> = path.split('>').map(str::trim).collect();
        if parts.is_empty() {
            return;
        }
        add_to_tree(&mut self.menus, &parts, longname);
    }

    /// Returns the number of top-level menus.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.menus.len()
    }

    /// Returns `true` if the menu bar is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.menus.is_empty()
    }
}

impl Default for MenuBar {
    fn default() -> Self {
        Self::new()
    }
}

fn add_to_tree(items: &mut Vec<MenuItem>, path: &[&str], longname: &str) {
    if path.len() == 1 {
        // Leaf — add as command.
        items.push(MenuItem::Command {
            label: path[0].to_owned(),
            longname: longname.to_owned(),
        });
        return;
    }

    // Find or create submenu.
    let submenu_label = path[0];
    let existing = items
        .iter_mut()
        .find(|item| matches!(item, MenuItem::Submenu { label, .. } if label == submenu_label));

    if let Some(MenuItem::Submenu {
        items: children, ..
    }) = existing
    {
        add_to_tree(children, &path[1..], longname);
    } else {
        let mut children = Vec::new();
        add_to_tree(&mut children, &path[1..], longname);
        items.push(MenuItem::Submenu {
            label: submenu_label.to_owned(),
            items: children,
        });
    }
}

/// Build the default `VisiData` menu bar.
#[must_use]
pub fn builtin_menu_bar() -> MenuBar {
    let mut bar = MenuBar::new();

    // File menu
    bar.add_item("File > Save", "save-sheet");
    bar.add_item("File > Quit", "quit-sheet");

    // Edit menu
    bar.add_item("Edit > Edit cell", "edit-cell");
    bar.add_item("Edit > Add row", "add-row");
    bar.add_item("Edit > Delete row", "delete-row");
    bar.add_item("Edit > Delete selected", "delete-selected");
    bar.add_item("Edit > Undo", "undo");

    // View menu
    bar.add_item("View > Columns", "columns-sheet");
    bar.add_item("View > Describe", "describe-sheet");
    bar.add_item("View > Sheets", "sheets-sheet");
    bar.add_item("View > Options", "options-sheet");
    bar.add_item("View > Help", "help-commands");

    // Column menu
    bar.add_item("Column > Rename", "rename-col");
    bar.add_item("Column > Resize", "resize-col-max");
    bar.add_item("Column > Hide", "hide-col");
    bar.add_item("Column > Key toggle", "key-col");
    bar.add_item("Column > Type > Integer", "type-int");
    bar.add_item("Column > Type > Float", "type-float");
    bar.add_item("Column > Type > String", "type-string");
    bar.add_item("Column > Type > Date", "type-date");
    bar.add_item("Column > Type > Currency", "type-currency");

    // Row menu
    bar.add_item("Row > Select", "select-row");
    bar.add_item("Row > Unselect", "unselect-row");
    bar.add_item("Row > Toggle", "toggle-row");
    bar.add_item("Row > Sort ascending", "sort-asc");
    bar.add_item("Row > Sort descending", "sort-desc");

    // Sheet menu
    bar.add_item("Sheet > Join", "join-sheets");
    bar.add_item("Sheet > Concatenate", "concat-sheets");
    bar.add_item("Sheet > Pivot", "pivot");
    bar.add_item("Sheet > Melt", "melt");
    bar.add_item("Sheet > Frequency", "freq-col");
    bar.add_item("Sheet > Selected rows", "dup-selected");

    bar
}

/// State of the menu navigation.
#[derive(Debug, Clone, Default)]
pub struct MenuState {
    /// Whether the menu is currently open.
    pub open: bool,
    /// Index of the selected top-level menu.
    pub menu_idx: usize,
    /// Index of the selected item within the current menu.
    pub item_idx: usize,
    /// Depth of submenu navigation (0 = top menu items).
    pub depth: usize,
}

impl MenuState {
    /// Create a new closed menu state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle the menu open/closed.
    pub const fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            self.menu_idx = 0;
            self.item_idx = 0;
            self.depth = 0;
        }
    }

    /// Move to the next top-level menu.
    pub const fn next_menu(&mut self, max: usize) {
        if max > 0 {
            self.menu_idx = (self.menu_idx + 1) % max;
            self.item_idx = 0;
        }
    }

    /// Move to the previous top-level menu.
    pub const fn prev_menu(&mut self, max: usize) {
        if max > 0 {
            self.menu_idx = if self.menu_idx == 0 {
                max - 1
            } else {
                self.menu_idx - 1
            };
            self.item_idx = 0;
        }
    }

    /// Move to the next item in the current menu.
    pub const fn next_item(&mut self, max: usize) {
        if max > 0 {
            self.item_idx = (self.item_idx + 1) % max;
        }
    }

    /// Move to the previous item in the current menu.
    pub const fn prev_item(&mut self, max: usize) {
        if max > 0 {
            self.item_idx = if self.item_idx == 0 {
                max - 1
            } else {
                self.item_idx - 1
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_bar_add_items() {
        let mut bar = MenuBar::new();
        bar.add_item("File > Save", "save-sheet");
        bar.add_item("File > Quit", "quit-sheet");
        bar.add_item("Edit > Undo", "undo");

        assert_eq!(bar.len(), 2); // File, Edit

        if let MenuItem::Submenu { items, label } = &bar.menus[0] {
            assert_eq!(label, "File");
            assert_eq!(items.len(), 2);
        } else {
            panic!("expected submenu");
        }
    }

    #[test]
    fn menu_bar_nested() {
        let mut bar = MenuBar::new();
        bar.add_item("Column > Type > Integer", "type-int");
        bar.add_item("Column > Type > Float", "type-float");

        assert_eq!(bar.len(), 1);
        if let MenuItem::Submenu { items, .. } = &bar.menus[0] {
            assert_eq!(items.len(), 1); // "Type" submenu
            if let MenuItem::Submenu {
                items: type_items, ..
            } = &items[0]
            {
                assert_eq!(type_items.len(), 2);
            } else {
                panic!("expected nested submenu");
            }
        } else {
            panic!("expected submenu");
        }
    }

    #[test]
    fn builtin_menu_bar_not_empty() {
        let bar = builtin_menu_bar();
        assert!(bar.len() >= 6); // File, Edit, View, Column, Row, Sheet
    }

    #[test]
    fn menu_state_navigation() {
        let mut state = MenuState::new();
        assert!(!state.open);

        state.toggle();
        assert!(state.open);
        assert_eq!(state.menu_idx, 0);

        state.next_menu(5);
        assert_eq!(state.menu_idx, 1);

        state.prev_menu(5);
        assert_eq!(state.menu_idx, 0);

        state.prev_menu(5);
        assert_eq!(state.menu_idx, 4); // wraps

        state.next_item(3);
        assert_eq!(state.item_idx, 1);

        state.toggle();
        assert!(!state.open);
    }

    #[test]
    fn menu_item_labels() {
        let cmd = MenuItem::Command {
            label: "Save".into(),
            longname: "save-sheet".into(),
        };
        assert_eq!(cmd.label(), "Save");

        let sep = MenuItem::Separator;
        assert_eq!(sep.label(), "─");
    }
}
