use anyhow::Result;
use std::io::Write;
use std::process::{Command, Stdio};

/// Run Claude in non-interactive `--print` mode, feeding prompt via stdin.
/// Used for --no-tui / headless mode.
pub fn run_claude_print(
    prompt: &str,
    allowed_tools: &str,
    extra_flags: &[String],
    dry_run: bool,
) -> Result<i32> {
    if dry_run {
        eprintln!("(DRY RUN) Would run Claude in print mode");
        return Ok(0);
    }

    let mut cmd = Command::new("claude");
    cmd.arg("--print")
        .arg("--allowedTools")
        .arg(allowed_tools);

    for flag in extra_flags {
        cmd.arg(flag);
    }

    cmd.stdin(Stdio::piped());

    let mut child = cmd.spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())?;
        // stdin dropped here, closing the pipe
    }

    let status = child.wait()?;
    Ok(status.code().unwrap_or(1))
}
