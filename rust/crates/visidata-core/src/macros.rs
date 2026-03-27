//! Macro recording and replay for keystroke sequences.

/// A recorded macro — a named sequence of keystrokes.
#[derive(Debug, Clone)]
pub struct Macro {
    /// Name/description of this macro.
    pub name: String,
    /// Recorded keystrokes (each is a key string like "j", "Enter", "Ctrl+Z").
    pub keystrokes: Vec<String>,
}

impl Macro {
    /// Create a new named macro with the given keystrokes.
    #[must_use]
    pub fn new(name: impl Into<String>, keystrokes: Vec<String>) -> Self {
        Self {
            name: name.into(),
            keystrokes,
        }
    }
}

/// Macro recorder — accumulates keystrokes while recording.
#[derive(Debug, Default)]
pub struct MacroRecorder {
    /// Whether recording is active.
    recording: bool,
    /// Accumulated keystrokes for the current recording.
    buffer: Vec<String>,
}

impl MacroRecorder {
    /// Create a new idle recorder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start recording.
    pub fn start(&mut self) {
        self.recording = true;
        self.buffer.clear();
    }

    /// Stop recording and return the recorded macro, or `None` if nothing was recorded.
    pub fn stop(&mut self, name: &str) -> Option<Macro> {
        self.recording = false;
        if self.buffer.is_empty() {
            None
        } else {
            Some(Macro::new(name, self.buffer.drain(..).collect()))
        }
    }

    /// Record a keystroke (called for each key while recording is active).
    /// Returns `true` if the keystroke was recorded.
    pub fn record(&mut self, key: &str) -> bool {
        if self.recording {
            self.buffer.push(key.to_owned());
            true
        } else {
            false
        }
    }

    /// Returns `true` if recording is active.
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        self.recording
    }

    /// Returns the number of keystrokes recorded so far.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns `true` if no keystrokes have been recorded yet.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

/// Macro store — holds named macros.
#[derive(Debug, Default)]
pub struct MacroStore {
    macros: Vec<Macro>,
}

impl MacroStore {
    /// Create an empty macro store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or replace a macro by name.
    pub fn save(&mut self, mac: Macro) {
        if let Some(existing) = self.macros.iter_mut().find(|m| m.name == mac.name) {
            *existing = mac;
        } else {
            self.macros.push(mac);
        }
    }

    /// Get a macro by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Macro> {
        self.macros.iter().find(|m| m.name == name)
    }

    /// Returns all stored macros.
    #[must_use]
    pub fn all(&self) -> &[Macro] {
        &self.macros
    }

    /// Returns the number of stored macros.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.macros.len()
    }

    /// Returns `true` if no macros are stored.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.macros.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_stop() {
        let mut rec = MacroRecorder::new();
        assert!(!rec.is_recording());

        rec.start();
        assert!(rec.is_recording());

        rec.record("j");
        rec.record("j");
        rec.record("Enter");
        assert_eq!(rec.len(), 3);

        let mac = rec.stop("test").unwrap();
        assert_eq!(mac.name, "test");
        assert_eq!(mac.keystrokes, vec!["j", "j", "Enter"]);
        assert!(!rec.is_recording());
    }

    #[test]
    fn stop_empty_returns_none() {
        let mut rec = MacroRecorder::new();
        rec.start();
        assert!(rec.stop("empty").is_none());
    }

    #[test]
    fn record_when_not_recording() {
        let mut rec = MacroRecorder::new();
        assert!(!rec.record("j")); // not recording
        assert_eq!(rec.len(), 0);
    }

    #[test]
    fn macro_store_save_and_get() {
        let mut store = MacroStore::new();
        store.save(Macro::new("foo", vec!["j".into(), "k".into()]));
        store.save(Macro::new("bar", vec!["Enter".into()]));

        assert_eq!(store.len(), 2);
        let mac = store.get("foo").unwrap();
        assert_eq!(mac.keystrokes, vec!["j", "k"]);
    }

    #[test]
    fn macro_store_replace() {
        let mut store = MacroStore::new();
        store.save(Macro::new("foo", vec!["j".into()]));
        store.save(Macro::new("foo", vec!["k".into()])); // replace

        assert_eq!(store.len(), 1);
        assert_eq!(store.get("foo").unwrap().keystrokes, vec!["k"]);
    }

    #[test]
    fn macro_store_missing() {
        let store = MacroStore::new();
        assert!(store.get("nope").is_none());
    }
}
