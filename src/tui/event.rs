use crate::tui::app::IssueDisplayStatus;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub enum AppEvent {
    Key(KeyEvent),
    Tick,
    IssueStatusChanged {
        index: usize,
        status: IssueDisplayStatus,
    },
    BranchChanged(Option<String>),
    /// Raw PTY output bytes from Claude
    PtyBytes(Vec<u8>),
    /// Claude's PTY input channel is ready — forward keystrokes here
    PtyInputReady(mpsc::Sender<Vec<u8>>),
    /// Claude process exited with this code
    ClaudeFinished(i32),
    OutputCleared,
    LogMessage(String),
    WorkerDone,
}

pub enum WorkerCommand {
    SkipCurrent,
    Quit,
}

pub struct EventSystem {
    pub app_rx: mpsc::Receiver<AppEvent>,
    pub app_tx: mpsc::Sender<AppEvent>,
    pub cmd_tx: mpsc::Sender<WorkerCommand>,
    pub cmd_rx: Option<mpsc::Receiver<WorkerCommand>>,
}

impl EventSystem {
    pub fn new() -> Self {
        let (app_tx, app_rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();

        EventSystem {
            app_rx,
            app_tx,
            cmd_tx,
            cmd_rx: Some(cmd_rx),
        }
    }

    /// Start the input + tick thread. Returns immediately.
    pub fn start_input_thread(&self) {
        let tx = self.app_tx.clone();
        thread::spawn(move || {
            loop {
                match event::poll(Duration::from_millis(250)) {
                    Ok(true) => {
                        if let Ok(Event::Key(key)) = event::read() {
                            if key.kind == KeyEventKind::Press
                                && tx.send(AppEvent::Key(key)).is_err()
                            {
                                break;
                            }
                        }
                    }
                    Ok(false) => {
                        if tx.send(AppEvent::Tick).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
}

/// Process a key event and return true if the app should quit.
pub fn handle_key(
    key: KeyEvent,
    app: &mut crate::tui::app::App,
    cmd_tx: &mpsc::Sender<WorkerCommand>,
) -> bool {
    match key.code {
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let _ = cmd_tx.send(WorkerCommand::Quit);
            app.should_quit = true;
            true
        }
        KeyCode::Char('q') => {
            let _ = cmd_tx.send(WorkerCommand::Quit);
            app.should_quit = true;
            true
        }
        KeyCode::Char('s') => {
            let _ = cmd_tx.send(WorkerCommand::SkipCurrent);
            false
        }
        _ => false,
    }
}
