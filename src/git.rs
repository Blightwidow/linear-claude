use anyhow::{bail, Context, Result};
use std::process::Command;

fn run_git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git {} failed: {}", args.join(" "), stderr);
    }
}

fn run_git_quiet(args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        bail!("git {} failed: {}", args.join(" "), stderr);
    }
}

/// Detect GitHub owner/repo from the git remote URL.
pub fn detect_github_repo() -> Result<(String, String)> {
    run_git(&["rev-parse", "--git-dir"])?;
    let remote_url = run_git(&["remote", "get-url", "origin"])?;

    let (owner, repo) = if let Some(rest) = remote_url.strip_prefix("https://github.com/") {
        parse_owner_repo(rest)
    } else if let Some(rest) = remote_url.strip_prefix("git@github.com:") {
        parse_owner_repo(rest)
    } else {
        bail!("Remote URL is not a GitHub URL: {remote_url}");
    }
    .context("Could not parse owner/repo from remote URL")?;

    Ok((owner, repo))
}

fn parse_owner_repo(path: &str) -> Option<(String, String)> {
    let path = path.trim_end_matches(".git");
    let parts: Vec<&str> = path.splitn(2, '/').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

pub fn current_branch() -> Result<String> {
    run_git(&["rev-parse", "--abbrev-ref", "HEAD"])
}

pub fn checkout(branch: &str) -> Result<()> {
    run_git_quiet(&["checkout", branch])
}

pub fn checkout_new(branch: &str) -> Result<()> {
    run_git_quiet(&["checkout", "-b", branch])
}

pub fn checkout_new_from(branch: &str, start_point: &str) -> Result<()> {
    run_git_quiet(&["checkout", "-b", branch, start_point])
}

pub fn fetch(remote: &str, branch: &str) -> Result<()> {
    run_git_quiet(&["fetch", remote, branch])
}

pub fn push_branch(branch: &str) -> Result<()> {
    run_git_quiet(&["push", "-u", "origin", branch])
}

pub fn push(remote: &str, branch: &str) -> Result<()> {
    run_git_quiet(&["push", remote, branch])
}

pub fn has_uncommitted_changes() -> Result<bool> {
    // Check staged + unstaged
    let diff = Command::new("git")
        .args(["diff", "--quiet", "--ignore-submodules=dirty"])
        .output()?;
    let diff_cached = Command::new("git")
        .args(["diff", "--cached", "--quiet", "--ignore-submodules=dirty"])
        .output()?;

    if !diff.status.success() || !diff_cached.status.success() {
        return Ok(true);
    }

    // Check untracked files
    let untracked = run_git(&["ls-files", "--others", "--exclude-standard"])?;
    Ok(!untracked.is_empty())
}

pub fn commit_count_since(base: &str, head: &str) -> Result<u64> {
    let output = run_git(&["rev-list", "--count", &format!("{base}..{head}")])?;
    Ok(output.parse().unwrap_or(0))
}

pub fn branch_exists_remote(branch: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["ls-remote", "--heads", "origin", branch])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.contains(branch))
}

pub fn branch_exists_local(branch: &str) -> Result<bool> {
    let result = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")])
        .output()?;
    Ok(result.status.success())
}

pub fn delete_branch(branch: &str) -> Result<()> {
    run_git_quiet(&["branch", "-D", branch])
}

pub fn reset_hard(refspec: &str) -> Result<()> {
    run_git_quiet(&["reset", "--hard", refspec])
}

pub fn rev_parse(refspec: &str) -> Result<String> {
    run_git(&["rev-parse", refspec])
}

pub fn ls_remote_head(branch: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .args(["ls-remote", "origin", branch])
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let sha = stdout.split_whitespace().next().map(|s| s.to_string());
    Ok(sha)
}

pub fn log_last_message(branch: &str) -> Result<String> {
    run_git(&["log", "-1", "--format=%B", branch])
}

pub fn is_git_repo() -> bool {
    Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
