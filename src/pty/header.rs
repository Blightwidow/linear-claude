use base64::Engine;
use crossterm::terminal;
use std::io::Write;

/// Paint the persistent header on row 1, set scroll region to rows 2..N.
pub fn paint_header(stdout: &mut impl Write, header_text: &str) -> std::io::Result<()> {
    let (cols, rows) = terminal::size().unwrap_or((80, 24));
    let cols = cols as usize;

    // Truncate/pad header text to terminal width
    let display_text: String = if header_text.len() > cols {
        header_text[..cols].to_string()
    } else {
        format!("{:<width$}", header_text, width = cols)
    };

    // Save cursor, move to row 1 col 1, reverse video header, set scroll region, restore cursor
    write!(
        stdout,
        "\x1b7\x1b[1;1H\x1b[7m{display_text}\x1b[0m\x1b[2;{rows}r\x1b8"
    )?;
    stdout.flush()?;
    Ok(())
}

/// Reset scroll region and move cursor home.
pub fn reset_scroll_region(stdout: &mut impl Write) -> std::io::Result<()> {
    write!(stdout, "\x1b[r\x1b[H")?;
    stdout.flush()?;
    Ok(())
}

/// Set terminal title via OSC escape sequence.
pub fn set_terminal_title(stdout: &mut impl Write, title: &str) -> std::io::Result<()> {
    write!(stdout, "\x1b]0;{title}\x07")?;
    stdout.flush()?;
    Ok(())
}

/// Reset terminal title to default.
pub fn reset_terminal_title(stdout: &mut impl Write) -> std::io::Result<()> {
    let program = std::env::var("TERM_PROGRAM").unwrap_or_else(|_| "Terminal".to_string());
    set_terminal_title(stdout, &program)?;
    clear_iterm2_badge(stdout)?;
    Ok(())
}

/// Set iTerm2 badge (persists through alt screen buffer).
pub fn set_iterm2_badge(stdout: &mut impl Write, text: &str) -> std::io::Result<()> {
    if std::env::var("TERM_PROGRAM").as_deref() == Ok("iTerm.app") {
        let encoded = base64::engine::general_purpose::STANDARD.encode(text);
        write!(stdout, "\x1b]1337;SetBadge={encoded}\x07")?;
        stdout.flush()?;
    }
    Ok(())
}

/// Clear iTerm2 badge.
pub fn clear_iterm2_badge(stdout: &mut impl Write) -> std::io::Result<()> {
    if std::env::var("TERM_PROGRAM").as_deref() == Ok("iTerm.app") {
        write!(stdout, "\x1b]1337;SetBadge=\x07")?;
        stdout.flush()?;
    }
    Ok(())
}
