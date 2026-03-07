use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::sync::mpsc;
use std::thread;

pub struct ClaudeProcess {
    pub child: Box<dyn portable_pty::Child + Send + Sync>,
    pub byte_rx: mpsc::Receiver<Vec<u8>>,
    /// Send bytes here to write to Claude's stdin
    pub input_tx: mpsc::Sender<Vec<u8>>,
    /// Keep master alive so the PTY stays open
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

/// Spawn Claude in a PTY. Returns a ClaudeProcess with byte channels
/// for raw terminal I/O and the child handle.
pub fn spawn_claude(
    prompt: &str,
    allowed_tools: &str,
    extra_flags: &[String],
    rows: u16,
    cols: u16,
) -> Result<ClaudeProcess> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    let mut cmd = CommandBuilder::new("claude");
    cmd.arg(prompt);
    cmd.arg("--allowedTools");
    cmd.arg(allowed_tools);

    for flag in extra_flags {
        cmd.arg(flag);
    }

    // Set working directory to the current process's cwd
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }

    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);

    // Output reader thread
    let reader = pair.master.try_clone_reader()?;
    let (out_tx, out_rx) = mpsc::channel();

    thread::spawn(move || {
        use std::io::Read;
        let mut reader = reader;
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Input writer thread
    let writer = pair.master.take_writer()?;
    let (in_tx, in_rx) = mpsc::channel::<Vec<u8>>();

    thread::spawn(move || {
        use std::io::Write;
        let mut writer = writer;
        while let Ok(bytes) = in_rx.recv() {
            if writer.write_all(&bytes).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    });

    Ok(ClaudeProcess {
        child,
        byte_rx: out_rx,
        input_tx: in_tx,
        _master: pair.master,
    })
}

/// Convert a crossterm KeyEvent into raw terminal bytes for PTY input.
pub fn key_to_bytes(key: &crossterm::event::KeyEvent) -> Option<Vec<u8>> {
    use crossterm::event::{KeyCode, KeyModifiers};

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return match key.code {
            KeyCode::Char(c) => {
                // Ctrl+A = 0x01 .. Ctrl+Z = 0x1A
                let lower = c.to_ascii_lowercase();
                if lower.is_ascii_lowercase() {
                    Some(vec![lower as u8 - b'a' + 1])
                } else {
                    None
                }
            }
            _ => None,
        };
    }

    if key.modifiers.contains(KeyModifiers::ALT) {
        return match key.code {
            KeyCode::Char(c) => {
                let mut bytes = vec![0x1b]; // ESC prefix
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                bytes.extend_from_slice(s.as_bytes());
                Some(bytes)
            }
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            let s = c.encode_utf8(&mut buf);
            Some(s.as_bytes().to_vec())
        }
        KeyCode::Enter => Some(vec![b'\r']),
        KeyCode::Backspace => Some(vec![0x7f]),
        KeyCode::Tab => Some(vec![b'\t']),
        KeyCode::Esc => Some(vec![0x1b]),
        KeyCode::Up => Some(b"\x1b[A".to_vec()),
        KeyCode::Down => Some(b"\x1b[B".to_vec()),
        KeyCode::Right => Some(b"\x1b[C".to_vec()),
        KeyCode::Left => Some(b"\x1b[D".to_vec()),
        KeyCode::Home => Some(b"\x1b[H".to_vec()),
        KeyCode::End => Some(b"\x1b[F".to_vec()),
        KeyCode::Delete => Some(b"\x1b[3~".to_vec()),
        KeyCode::PageUp => Some(b"\x1b[5~".to_vec()),
        KeyCode::PageDown => Some(b"\x1b[6~".to_vec()),
        _ => None,
    }
}
