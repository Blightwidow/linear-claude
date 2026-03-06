use anyhow::Result;
use std::io::{BufRead, Write as IoWrite};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;

pub struct ClaudeProcess {
    pub child: Child,
    pub line_rx: mpsc::Receiver<String>,
}

/// Spawn `claude --print --output-format stream-json`, feed prompt via stdin.
/// Parses streamed JSON lines and sends displayable text via channel.
pub fn spawn_claude_print(
    prompt: &str,
    allowed_tools: &str,
    extra_flags: &[String],
) -> Result<ClaudeProcess> {
    let mut cmd = Command::new("claude");
    cmd.arg("--print")
        .arg("--verbose")
        .arg("--output-format")
        .arg("stream-json")
        .arg("--allowedTools")
        .arg(allowed_tools);

    for flag in extra_flags {
        cmd.arg(flag);
    }

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;

    // Write prompt to stdin, then close it
    if let Some(mut stdin) = child.stdin.take() {
        let prompt = prompt.to_string();
        thread::spawn(move || {
            let _ = stdin.write_all(prompt.as_bytes());
            // stdin dropped here, closing the pipe
        });
    }

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let (tx, rx) = mpsc::channel();

    // Stdout reader thread — parses stream-json lines
    let tx_out = tx.clone();
    thread::spawn(move || {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    for display_line in parse_stream_json_line(&line) {
                        if tx_out.send(display_line).is_err() {
                            return;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Stderr reader thread
    thread::spawn(move || {
        let reader = std::io::BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    let stripped = strip_ansi(&line);
                    if !stripped.trim().is_empty() {
                        if tx.send(stripped).is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });

    Ok(ClaudeProcess { child, line_rx: rx })
}

/// Parse a stream-json line and return displayable text lines.
fn parse_stream_json_line(raw: &str) -> Vec<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return vec![];
    }

    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => {
            // Not JSON — show as-is
            return vec![strip_ansi(trimmed)];
        }
    };

    let event_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");

    match event_type {
        "assistant" => {
            // Extract text content from assistant message
            extract_assistant_text(&value)
        }
        "tool_use" => {
            let name = value.get("name").and_then(|v| v.as_str()).unwrap_or("unknown");
            let input = value.get("input").cloned().unwrap_or(serde_json::Value::Null);
            let mut lines = vec![format!("> Tool: {name}")];
            // Show compact tool input summary
            if let Some(cmd) = input.get("command").and_then(|v| v.as_str()) {
                lines.push(format!("  $ {cmd}"));
            } else if let Some(path) = input.get("file_path").and_then(|v| v.as_str()) {
                lines.push(format!("  {path}"));
            } else if let Some(pattern) = input.get("pattern").and_then(|v| v.as_str()) {
                lines.push(format!("  {pattern}"));
            }
            lines
        }
        "tool_result" => {
            // Show abbreviated tool output
            if let Some(content) = value.get("content").and_then(|v| v.as_str()) {
                let preview: String = content.lines().take(5).collect::<Vec<_>>().join("\n");
                let stripped = strip_ansi(&preview);
                if stripped.trim().is_empty() {
                    vec![]
                } else {
                    stripped.lines().map(|l| l.to_string()).collect()
                }
            } else {
                vec![]
            }
        }
        "result" => {
            // Final result — extract text
            if let Some(result) = value.get("result").and_then(|v| v.as_str()) {
                let stripped = strip_ansi(result);
                stripped.lines().map(|l| l.to_string()).collect()
            } else {
                vec![]
            }
        }
        // system, ping, etc. — skip
        _ => vec![],
    }
}

/// Extract text content from an assistant message event.
fn extract_assistant_text(value: &serde_json::Value) -> Vec<String> {
    let mut lines = Vec::new();

    // Try message.content array
    if let Some(content) = value
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_array())
    {
        for block in content {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                let stripped = strip_ansi(text);
                for line in stripped.lines() {
                    lines.push(line.to_string());
                }
            }
        }
    }

    // Try top-level content array
    if lines.is_empty() {
        if let Some(content) = value.get("content").and_then(|c| c.as_array()) {
            for block in content {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    let stripped = strip_ansi(text);
                    for line in stripped.lines() {
                        lines.push(line.to_string());
                    }
                }
            }
        }
    }

    lines
}

fn strip_ansi(s: &str) -> String {
    let bytes = strip_ansi_escapes::strip(s);
    String::from_utf8_lossy(&bytes).to_string()
}
