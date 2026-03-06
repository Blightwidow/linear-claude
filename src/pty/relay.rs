use crate::pty::header;
use anyhow::{Context, Result};
use crossterm::terminal;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use signal_hook::consts::SIGWINCH;
use signal_hook::iterator::Signals;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// Alt-screen escape sequences to intercept and strip.
const ALT_SCREEN_SEQS: &[&[u8]] = &[
    b"\x1b[?1049h",
    b"\x1b[?1049l",
    b"\x1b[?47h",
    b"\x1b[?47l",
];

/// Spawn a child command in a PTY with a persistent header on row 1.
/// Returns the child's exit code.
pub fn run_with_header(header_text: &str, argv: &[String]) -> Result<i32> {
    if argv.is_empty() {
        anyhow::bail!("No command to run");
    }

    let (cols, rows) = terminal::size().context("Failed to get terminal size")?;
    let child_rows = rows.saturating_sub(1).max(1);

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: child_rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .context("Failed to open PTY")?;

    let mut cmd = CommandBuilder::new(&argv[0]);
    for arg in &argv[1..] {
        cmd.arg(arg);
    }

    let mut child = pair.slave.spawn_command(cmd).context("Failed to spawn child")?;

    // We need to drop the slave so reads from master detect EOF when child exits
    drop(pair.slave);

    let mut master_writer = pair.master.take_writer().context("Failed to get master writer")?;
    let mut master_reader = pair.master.try_clone_reader().context("Failed to clone master reader")?;

    // Enable raw mode
    terminal::enable_raw_mode().context("Failed to enable raw mode")?;

    let mut stdout = io::stdout();

    // Paint initial header
    header::set_terminal_title(&mut stdout, header_text)?;
    header::set_iterm2_badge(&mut stdout, header_text)?;
    header::paint_header(&mut stdout, header_text)?;

    let exit_flag = Arc::new(AtomicBool::new(false));

    // SIGWINCH handler thread
    let header_text_clone = header_text.to_string();
    let exit_flag_winch = exit_flag.clone();
    let master_for_resize = pair.master;
    let winch_thread = thread::spawn(move || {
        let mut signals = match Signals::new([SIGWINCH]) {
            Ok(s) => s,
            Err(_) => return,
        };
        for _ in signals.forever() {
            if exit_flag_winch.load(Ordering::Relaxed) {
                break;
            }
            if let Ok((cols, rows)) = terminal::size() {
                let child_rows = rows.saturating_sub(1).max(1);
                let _ = master_for_resize.resize(PtySize {
                    rows: child_rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
                let mut stdout = io::stdout();
                let _ = header::paint_header(&mut stdout, &header_text_clone);
            }
        }
    });

    // Stdin -> master writer thread
    let exit_flag_stdin = exit_flag.clone();
    let _stdin_thread = thread::spawn(move || {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 4096];
        loop {
            if exit_flag_stdin.load(Ordering::Relaxed) {
                break;
            }
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if master_writer.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    });

    // Master reader -> stdout (main thread) with alt-screen filtering
    let max_seq_len = ALT_SCREEN_SEQS.iter().map(|s| s.len()).max().unwrap_or(0);
    let mut pending = Vec::new();
    let mut buf = [0u8; 4096];

    loop {
        match master_reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                pending.extend_from_slice(&buf[..n]);
                let mut out = Vec::new();
                let mut i = 0;

                while i < pending.len() {
                    if pending[i] == 0x1b {
                        let remaining = &pending[i..];
                        let mut matched = false;
                        let mut partial = false;

                        for seq in ALT_SCREEN_SEQS {
                            if remaining.starts_with(seq) {
                                header::paint_header(&mut stdout, header_text)?;
                                i += seq.len();
                                matched = true;
                                break;
                            }
                            if seq.starts_with(remaining) && remaining.len() < seq.len() {
                                partial = true;
                            }
                        }

                        if matched {
                            continue;
                        }
                        if partial {
                            // Need more data
                            break;
                        }
                        // Not an alt-screen escape, check if we need more bytes
                        if remaining.len() < max_seq_len {
                            break;
                        }
                    }
                    out.push(pending[i]);
                    i += 1;
                }

                // Keep unprocessed bytes
                pending = pending[i..].to_vec();

                if !out.is_empty() {
                    stdout.write_all(&out)?;
                    stdout.flush()?;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }

    // Flush any remaining pending bytes
    if !pending.is_empty() {
        stdout.write_all(&pending)?;
        stdout.flush()?;
    }

    // Wait for child
    let status = child.wait().context("Failed to wait for child")?;

    // Cleanup
    exit_flag.store(true, Ordering::Relaxed);
    terminal::disable_raw_mode().ok();
    header::reset_scroll_region(&mut stdout)?;
    header::reset_terminal_title(&mut stdout)?;

    // Signal the SIGWINCH thread to exit
    // The stdin thread will exit when stdin is closed or exit flag is set
    let _ = winch_thread.join();
    // Don't wait for stdin thread - it may be blocked on read

    let exit_code = status.exit_code() as i32;

    Ok(exit_code)
}
