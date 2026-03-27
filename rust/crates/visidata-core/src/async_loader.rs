//! Async loading infrastructure for background file loading with progress.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use crate::column::Column;
use crate::row::Row;

/// A message sent from the background loader thread to the main thread.
#[derive(Debug)]
pub enum LoadMessage {
    /// Column definitions (sent once at the start).
    Columns(Vec<Column>),
    /// A batch of rows to append.
    Rows(Vec<Row>),
    /// Loading completed successfully.
    Done,
    /// Loading failed with an error.
    Error(String),
}

/// Handle to a background loading operation.
#[derive(Debug)]
pub struct LoadHandle {
    /// Receiver for progress messages from the loader thread.
    pub receiver: Receiver<LoadMessage>,
    /// Cancel flag — set to `true` to request cancellation.
    pub cancel: Arc<AtomicBool>,
    /// The thread join handle.
    pub thread: Option<thread::JoinHandle<()>>,
}

impl LoadHandle {
    /// Request cancellation of the loading operation.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Check if cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Context passed to the loader function for reporting progress.
#[derive(Debug, Clone)]
pub struct LoadContext {
    /// Channel sender for progress messages.
    pub sender: Sender<LoadMessage>,
    /// Cancel flag — loader should check this periodically.
    pub cancel: Arc<AtomicBool>,
}

impl LoadContext {
    /// Send column definitions to the main thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the receiver has been dropped.
    pub fn send_columns(&self, columns: Vec<Column>) -> Result<(), mpsc::SendError<LoadMessage>> {
        self.sender.send(LoadMessage::Columns(columns))
    }

    /// Send a batch of rows to the main thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the receiver has been dropped.
    pub fn send_rows(&self, rows: Vec<Row>) -> Result<(), mpsc::SendError<LoadMessage>> {
        self.sender.send(LoadMessage::Rows(rows))
    }

    /// Check if cancellation was requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Spawn a background loader thread.
///
/// The provided closure receives a `LoadContext` for sending progress.
/// Returns a `LoadHandle` for receiving messages and cancellation.
pub fn spawn_loader<F>(loader_fn: F) -> LoadHandle
where
    F: FnOnce(LoadContext) + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let ctx = LoadContext {
        sender: sender.clone(),
        cancel: Arc::clone(&cancel),
    };

    let thread = thread::spawn(move || {
        loader_fn(ctx);
        // Send Done if the loader function didn't already.
        let _ = sender.send(LoadMessage::Done);
    });

    LoadHandle {
        receiver,
        cancel,
        thread: Some(thread),
    }
}

/// Loading state for a sheet.
#[derive(Debug, Default)]
pub enum LoadingState {
    /// Not loading.
    #[default]
    Idle,
    /// Currently loading in the background.
    Loading {
        /// Total rows loaded so far.
        rows_loaded: usize,
    },
    /// Loading completed.
    Complete {
        /// Total rows loaded.
        total_rows: usize,
    },
}

impl LoadingState {
    /// Returns `true` if currently loading.
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        matches!(self, Self::Loading { .. })
    }

    /// Returns a display string for the status bar.
    #[must_use]
    pub fn status_text(&self) -> String {
        match self {
            Self::Idle => String::new(),
            Self::Loading { rows_loaded } => format!(" loading... ({rows_loaded} rows)"),
            Self::Complete { total_rows } => format!(" ({total_rows} rows loaded)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::ColumnId;
    use crate::value::Value;
    use std::time::Duration;

    #[test]
    fn spawn_and_receive() {
        let handle = spawn_loader(|ctx| {
            let cols = vec![Column::new(ColumnId(0), "x", 0)];
            ctx.send_columns(cols).unwrap();

            let rows = vec![Row::new(vec![Value::Int(1)]), Row::new(vec![Value::Int(2)])];
            ctx.send_rows(rows).unwrap();
        });

        let mut got_columns = false;
        let mut total_rows = 0;

        loop {
            match handle.receiver.recv_timeout(Duration::from_secs(2)) {
                Ok(LoadMessage::Columns(cols)) => {
                    assert_eq!(cols.len(), 1);
                    got_columns = true;
                }
                Ok(LoadMessage::Rows(rows)) => {
                    total_rows += rows.len();
                }
                Ok(LoadMessage::Done) => break,
                Ok(LoadMessage::Error(e)) => panic!("unexpected error: {e}"),
                Err(_) => panic!("timeout waiting for loader"),
            }
        }

        assert!(got_columns);
        assert_eq!(total_rows, 2);
    }

    #[test]
    fn cancel_loader() {
        let handle = spawn_loader(|ctx| {
            for i in 0..1000 {
                if ctx.is_cancelled() {
                    return;
                }
                let _ = ctx.send_rows(vec![Row::new(vec![Value::Int(i)])]);
                std::thread::sleep(Duration::from_millis(1));
            }
        });

        // Cancel after a short delay.
        std::thread::sleep(Duration::from_millis(10));
        handle.cancel();

        // Drain remaining messages.
        let mut total = 0;
        loop {
            match handle.receiver.recv_timeout(Duration::from_secs(2)) {
                Ok(LoadMessage::Rows(rows)) => total += rows.len(),
                Ok(LoadMessage::Done) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }

        assert!(total < 1000, "should have been cancelled early");
    }

    #[test]
    fn loading_state_display() {
        let idle = LoadingState::Idle;
        assert!(idle.status_text().is_empty());
        assert!(!idle.is_loading());

        let loading = LoadingState::Loading { rows_loaded: 42 };
        assert!(loading.status_text().contains("42"));
        assert!(loading.is_loading());

        let complete = LoadingState::Complete { total_rows: 100 };
        assert!(complete.status_text().contains("100"));
        assert!(!complete.is_loading());
    }
}
