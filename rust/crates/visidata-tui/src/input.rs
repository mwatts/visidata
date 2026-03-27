//! Line editor widget for the bottom input bar.
//!
//! Ported from Python `VisiData`'s `edittext.py` / `test_edittext.py`.

/// A line editor that processes keystrokes and maintains cursor position.
#[derive(Debug, Clone)]
pub struct LineEditor {
    /// The text buffer.
    chars: Vec<char>,
    /// Cursor position (0-indexed, in chars).
    cursor: usize,
    /// The original value (for undo/reset).
    original: String,
}

/// Result of processing a keystroke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditResult {
    /// Still editing — display updated buffer.
    Continue,
    /// User pressed Enter — return the final value.
    Accept(String),
    /// User pressed Escape/Ctrl+C — cancel.
    Cancel,
}

impl LineEditor {
    /// Create a new line editor with an optional initial value.
    #[must_use]
    pub fn new(initial: &str) -> Self {
        let chars: Vec<char> = initial.chars().collect();
        let cursor = chars.len();
        Self {
            chars,
            cursor,
            original: initial.to_owned(),
        }
    }

    /// Returns the current text content.
    #[must_use]
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// Returns the cursor position.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Process a keystroke and return the result.
    ///
    /// # Panics
    ///
    /// Cannot panic — the only `unwrap` is guarded by a `len() == 1` check.
    pub fn handle_key(&mut self, key: &str) -> EditResult {
        match key {
            "Enter" => EditResult::Accept(self.text()),
            "Esc" | "Ctrl+C" => EditResult::Cancel,

            // Cursor movement
            "Left" | "Ctrl+B" => {
                self.cursor = self.cursor.saturating_sub(1);
                EditResult::Continue
            }
            "Right" | "Ctrl+F" => {
                if self.cursor < self.chars.len() {
                    self.cursor += 1;
                }
                EditResult::Continue
            }
            "Home" | "Ctrl+A" => {
                self.cursor = 0;
                EditResult::Continue
            }
            "End" | "Ctrl+E" => {
                self.cursor = self.chars.len();
                EditResult::Continue
            }

            // Deletion
            "Bksp" | "Ctrl+H" => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.chars.remove(self.cursor);
                }
                EditResult::Continue
            }
            "Del" => {
                if self.cursor < self.chars.len() {
                    self.chars.remove(self.cursor);
                }
                EditResult::Continue
            }

            // Kill to end of line
            "Ctrl+K" => {
                self.chars.truncate(self.cursor);
                EditResult::Continue
            }

            // Kill to start of line
            "Ctrl+U" => {
                self.chars.drain(..self.cursor);
                self.cursor = 0;
                EditResult::Continue
            }

            // Undo (reset to original)
            "Ctrl+R" => {
                self.chars = self.original.chars().collect();
                self.cursor = self.chars.len();
                EditResult::Continue
            }

            // Transpose characters
            "Ctrl+T" => {
                if self.chars.len() >= 2 && self.cursor > 0 {
                    let pos = if self.cursor >= self.chars.len() {
                        self.cursor - 1
                    } else {
                        self.cursor
                    };
                    if pos > 0 {
                        self.chars.swap(pos - 1, pos);
                    }
                } else if !self.chars.is_empty() && self.cursor == 0 {
                    // At home with chars: delete first char (VisiData behavior)
                    self.chars.remove(0);
                }
                EditResult::Continue
            }

            // Word movement
            "Ctrl+Left" => {
                self.cursor = self.prev_word_boundary();
                EditResult::Continue
            }
            "Ctrl+Right" => {
                self.cursor = self.next_word_boundary();
                EditResult::Continue
            }

            // Delete previous word
            "Ctrl+W" => {
                let boundary = self.prev_word_boundary();
                self.chars.drain(boundary..self.cursor);
                self.cursor = boundary;
                EditResult::Continue
            }

            // Delete next word
            "Ctrl+Del" => {
                let boundary = self.next_word_boundary();
                self.chars.drain(self.cursor..boundary);
                EditResult::Continue
            }

            // Regular character insertion
            _ if key.len() == 1 => {
                let ch = key.chars().next().unwrap();
                self.chars.insert(self.cursor, ch);
                self.cursor += 1;
                EditResult::Continue
            }

            _ => EditResult::Continue,
        }
    }

    /// Find the previous word boundary from cursor.
    fn prev_word_boundary(&self) -> usize {
        if self.cursor == 0 {
            return 0;
        }
        let mut pos = self.cursor - 1;
        // Skip spaces
        while pos > 0 && self.chars[pos] == ' ' {
            pos -= 1;
        }
        // Skip word chars
        while pos > 0 && self.chars[pos - 1] != ' ' {
            pos -= 1;
        }
        pos
    }

    /// Find the next word boundary from cursor.
    fn next_word_boundary(&self) -> usize {
        let len = self.chars.len();
        if self.cursor >= len {
            return len;
        }
        let mut pos = self.cursor;
        // Skip current word chars
        while pos < len && self.chars[pos] != ' ' {
            pos += 1;
        }
        // Skip spaces
        while pos < len && self.chars[pos] == ' ' {
            pos += 1;
        }
        pos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: feed a sequence of space-separated keystrokes to a fresh editor.
    fn run_keys(keys: &str) -> EditResult {
        run_keys_with_initial(keys, "")
    }

    /// Helper: feed keys to an editor with an initial value.
    fn run_keys_with_initial(keys: &str, initial: &str) -> EditResult {
        let mut editor = LineEditor::new(initial);
        let tokens: Vec<&str> = keys.split_whitespace().collect();
        let mut last = EditResult::Continue;
        for key in tokens {
            let k = if key == "Space" { " " } else { key };
            last = editor.handle_key(k);
            if matches!(last, EditResult::Accept(_) | EditResult::Cancel) {
                return last;
            }
        }
        last
    }

    // Ported from Python test_edittext.py

    #[test]
    fn enter_empty() {
        assert_eq!(run_keys("Enter"), EditResult::Accept(String::new()));
    }

    #[test]
    fn basic_typing_with_home() {
        assert_eq!(
            run_keys("a b Home c d Ctrl+A e f Enter"),
            EditResult::Accept("efcdab".into())
        );
    }

    #[test]
    fn left_past_home() {
        assert_eq!(
            run_keys("a b Left 1 Left Left Left 2 Enter"),
            EditResult::Accept("2a1b".into())
        );
    }

    #[test]
    fn ctrl_c_cancels() {
        assert_eq!(run_keys("a b Ctrl+C"), EditResult::Cancel);
    }

    #[test]
    fn esc_cancels() {
        assert_eq!(run_keys("a b Esc"), EditResult::Cancel);
    }

    #[test]
    fn del_at_end() {
        assert_eq!(run_keys("a Del Enter"), EditResult::Accept("a".into()));
    }

    #[test]
    fn del_in_middle() {
        assert_eq!(
            run_keys("a b Left Del Enter"),
            EditResult::Accept("a".into())
        );
    }

    #[test]
    fn end_key() {
        assert_eq!(
            run_keys("a b Left c End d Enter"),
            EditResult::Accept("acbd".into())
        );
    }

    #[test]
    fn home_right() {
        assert_eq!(
            run_keys("a b Home Right c Enter"),
            EditResult::Accept("acb".into())
        );
    }

    #[test]
    fn backspace() {
        assert_eq!(
            run_keys("a b Bksp c Enter"),
            EditResult::Accept("ac".into())
        );
    }

    #[test]
    fn backspace_at_home() {
        // Backspace at start does nothing
        assert_eq!(
            run_keys("a b Home Bksp c Enter"),
            EditResult::Accept("cab".into())
        );
    }

    #[test]
    fn backspace_combos() {
        assert_eq!(
            run_keys("a b c Bksp Ctrl+H Left Del Enter"),
            EditResult::Accept(String::new())
        );
    }

    #[test]
    fn ctrl_k_kill_to_end() {
        assert_eq!(
            run_keys("a b c Ctrl+B Ctrl+B Ctrl+K Enter"),
            EditResult::Accept("a".into())
        );
    }

    #[test]
    fn ctrl_r_undo_empty() {
        assert_eq!(
            run_keys("a Ctrl+R Enter"),
            EditResult::Accept(String::new())
        );
    }

    #[test]
    fn ctrl_r_undo_to_initial() {
        assert_eq!(
            run_keys_with_initial("a Ctrl+R Enter", "foo"),
            EditResult::Accept("foo".into())
        );
    }

    #[test]
    fn ctrl_t_transpose() {
        assert_eq!(
            run_keys("a b Ctrl+T Enter"),
            EditResult::Accept("ba".into())
        );
    }

    #[test]
    fn ctrl_t_at_home_deletes() {
        assert_eq!(
            run_keys("a b Home Ctrl+T Enter"),
            EditResult::Accept("b".into())
        );
    }

    #[test]
    fn ctrl_u_kill_to_start() {
        assert_eq!(
            run_keys("a b Left Ctrl+U Enter"),
            EditResult::Accept("b".into())
        );
    }

    #[test]
    fn ctrl_u_at_end() {
        assert_eq!(
            run_keys("a b Ctrl+U c Enter"),
            EditResult::Accept("c".into())
        );
    }

    #[test]
    fn ctrl_w_delete_word() {
        assert_eq!(
            run_keys(
                "w e Space a r e Space t h e Space w o r l d Ctrl+Left Ctrl+Left Ctrl+W Enter"
            ),
            EditResult::Accept("we the world".into())
        );
    }

    #[test]
    fn ctrl_del_delete_next_word() {
        assert_eq!(
            run_keys(
                "w e Space a r e Space t h e Space w o r l d Ctrl+Left Ctrl+Left Ctrl+Del Enter"
            ),
            EditResult::Accept("we are world".into())
        );
    }
}
