use crate::pty;
use anyhow::Result;
use std::process::Command;

/// Run Claude Code interactively. If `header_text` is provided, use PTY wrapper for persistent header.
/// Returns the exit code from Claude.
pub fn run_claude_interactive(
    prompt: &str,
    allowed_tools: &str,
    extra_flags: &[String],
    header_text: Option<&str>,
    dry_run: bool,
) -> Result<i32> {
    if dry_run {
        eprintln!("(DRY RUN) Would run Claude interactively");
        return Ok(0);
    }

    let mut argv = vec![
        "claude".to_string(),
        prompt.to_string(),
        "--allowedTools".to_string(),
        allowed_tools.to_string(),
    ];
    argv.extend(extra_flags.iter().cloned());

    if let Some(header) = header_text {
        // Use PTY wrapper with header
        pty::relay::run_with_header(header, &argv)
    } else {
        // Run claude directly without PTY wrapper
        let status = Command::new(&argv[0])
            .args(&argv[1..])
            .status()?;
        Ok(status.code().unwrap_or(1))
    }
}
