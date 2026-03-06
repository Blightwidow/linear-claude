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
    ClaudeOutput(String),
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

    /// Start the input + tick threads. Returns immediately.
    pub fn start_input_thread(&self) {
        let tx = self.app_tx.clone();
        thread::spawn(move || {
            loop {
                // Poll with 250ms timeout for ticks
                match event::poll(Duration::from_millis(250)) {
                    Ok(true) => {
                        if let Ok(Event::Key(key)) = event::read() {
                            if key.kind == KeyEventKind::Press {
                                if tx.send(AppEvent::Key(key)).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Ok(false) => {
                        // Timeout = tick
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
    visible_height: u16,
) -> bool {
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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
        KeyCode::Up | KeyCode::Char('k') => {
            app.scroll_up(1);
            false
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.scroll_down(1, visible_height);
            false
        }
        KeyCode::PageUp => {
            app.scroll_up(visible_height.saturating_sub(2));
            false
        }
        KeyCode::PageDown => {
            app.scroll_down(visible_height.saturating_sub(2), visible_height);
            false
        }
        KeyCode::End => {
            app.scroll_to_bottom();
            false
        }
        KeyCode::Home => {
            app.auto_scroll = false;
            app.scroll_offset = 0;
            false
        }
        _ => false,
    }
}
