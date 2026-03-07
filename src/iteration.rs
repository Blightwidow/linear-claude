use crate::claude;
use crate::config::Config;
use crate::git;
use crate::github::client::GitHubClient;
use crate::linear::types::{IssueStatus, LinearIssue};
use crate::prompt;
use crate::summary::{SummaryEntry, SummaryResult};
use crate::tui::app::IssueDisplayStatus;
use crate::tui::event::{AppEvent, WorkerCommand};
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct IterationState {
    pub error_count: u32,
    pub extra_iterations: u32,
    pub successful_iterations: u32,
    pub iteration_num: u32,
    pub unpushed_branches: Vec<String>,
    pub summary_results: Vec<SummaryEntry>,
    pub start_time: Option<Instant>,
    pub interrupted: Arc<AtomicBool>,
}

impl IterationState {
    pub fn new(interrupted: Arc<AtomicBool>) -> Self {
        IterationState {
            error_count: 0,
            extra_iterations: 0,
            successful_iterations: 0,
            iteration_num: 1,
            unpushed_branches: Vec::new(),
            summary_results: Vec::new(),
            start_time: None,
            interrupted,
        }
    }
}

/// Worker thread entry point for TUI mode.
/// Processes issues and sends events back to the TUI.
pub fn worker_loop(
    config: &Config,
    issues: &[LinearIssue],
    state: &mut IterationState,
    event_tx: mpsc::Sender<AppEvent>,
    cmd_rx: mpsc::Receiver<WorkerCommand>,
) {
    if let Err(e) = worker_loop_inner(config, issues, state, &event_tx, &cmd_rx) {
        let _ = event_tx.send(AppEvent::LogMessage(format!("Error: {e}")));
    }
    let _ = event_tx.send(AppEvent::WorkerDone);
}

fn worker_loop_inner(
    config: &Config,
    issues: &[LinearIssue],
    state: &mut IterationState,
    event_tx: &mpsc::Sender<AppEvent>,
    cmd_rx: &mpsc::Receiver<WorkerCommand>,
) -> Result<()> {
    if config.max_duration.is_some() {
        state.start_time = Some(Instant::now());
    }

    if issues.is_empty() {
        let _ = event_tx.send(AppEvent::LogMessage("No issues found, nothing to do.".into()));
        return Ok(());
    }

    let github = if config.enable_commits {
        GitHubClient::new().ok()
    } else {
        None
    };

    for (issue_index, issue) in issues.iter().enumerate() {
        // Check for quit command
        if check_quit(cmd_rx) || state.interrupted.load(Ordering::Relaxed) {
            break;
        }

        // Check limits
        if let Some(max_runs) = config.max_runs {
            if max_runs != 0 && state.successful_iterations >= max_runs {
                break;
            }
        }

        if let Some(max_dur) = config.max_duration {
            if let Some(start) = state.start_time {
                if start.elapsed() >= max_dur {
                    let _ = event_tx.send(AppEvent::LogMessage(format!(
                        "Time limit reached ({})",
                        crate::duration::format_duration(start.elapsed().as_secs())
                    )));
                    break;
                }
            }
        }

        // Clear output for new issue
        let _ = event_tx.send(AppEvent::OutputCleared);
        let _ = event_tx.send(AppEvent::IssueStatusChanged {
            index: issue_index,
            status: IssueDisplayStatus::Running,
        });

        let _ = event_tx.send(AppEvent::LogMessage(format!(
            "Processing {} -- {} ({}, {}/{})",
            issue.identifier,
            issue.title,
            issue.state.display_name(),
            issue_index + 1,
            issues.len()
        )));

        // Abort if there are uncommitted changes
        if git::is_git_repo() {
            if let Ok(true) = git::has_uncommitted_changes() {
                let _ = event_tx.send(AppEvent::LogMessage(
                    "Error: Uncommitted changes detected. Please commit or stash.".into(),
                ));
                break;
            }
        }

        // Reset to origin/main
        if git::is_git_repo() && !config.dry_run {
            let _ = event_tx.send(AppEvent::LogMessage("Resetting to origin/main...".into()));
            git::fetch("origin", "main").ok();
            git::checkout("main")
                .or_else(|_| git::checkout_new_from("main", "origin/main"))
                .ok();
            git::reset_hard("origin/main").ok();
        }

        // Resolve actual branch name from PR if available
        let mut branch_name = issue.branch_name.clone();
        if let (Some(pr_url), Some(gh)) = (&issue.pr_url, &github) {
            if let Some((pr_owner, pr_repo, pr_number)) = parse_pr_url(pr_url) {
                if let Ok(pr_branch) = gh.get_pr_head_ref(&pr_owner, &pr_repo, pr_number) {
                    branch_name = Some(pr_branch);
                }
            }
        }

        // Status-based routing
        let iteration_display = get_iteration_display(
            state.iteration_num,
            config.max_runs.unwrap_or(0),
            state.extra_iterations,
        );

        let result = match &issue.state {
            IssueStatus::Done | IssueStatus::InProgress => {
                let _ = event_tx.send(AppEvent::LogMessage(format!(
                    "Skipping {} -- status is '{}'",
                    issue.identifier,
                    issue.state.display_name()
                )));
                SummaryResult::Skip
            }
            IssueStatus::InReview => {
                if let Some(gh) = &github {
                    match handle_in_review_issue(
                        config,
                        state,
                        issue,
                        &branch_name,
                        &iteration_display,
                        gh,
                        event_tx,
                        cmd_rx,
                    ) {
                        Ok(()) => SummaryResult::Done,
                        Err(e) => {
                            let _ = event_tx.send(AppEvent::LogMessage(format!(
                                "Review handling failed: {e}"
                            )));
                            SummaryResult::Fail
                        }
                    }
                } else {
                    let _ = event_tx.send(AppEvent::LogMessage(
                        "Skipping review -- GitHub client not available".into(),
                    ));
                    SummaryResult::Skip
                }
            }
            IssueStatus::Other(_) => {
                let issue_prompt = prompt::build_issue_prompt(
                    &issue.identifier,
                    &issue.title,
                    issue.description.as_deref().unwrap_or(""),
                );

                match execute_single_iteration(
                    config,
                    state,
                    &issue_prompt,
                    &branch_name,
                    &issue.identifier,
                    &iteration_display,
                    event_tx,
                    cmd_rx,
                ) {
                    Ok(()) => SummaryResult::Done,
                    Err(e) => {
                        let _ = event_tx.send(AppEvent::LogMessage(format!(
                            "Iteration failed: {e}"
                        )));
                        SummaryResult::Fail
                    }
                }
            }
        };

        // Update issue display status
        let display_status = match &result {
            SummaryResult::Done => IssueDisplayStatus::Done,
            SummaryResult::Fail => IssueDisplayStatus::Failed,
            SummaryResult::Skip => IssueDisplayStatus::Skipped,
        };
        let _ = event_tx.send(AppEvent::IssueStatusChanged {
            index: issue_index,
            status: display_status,
        });

        // Update counters
        if !matches!(result, SummaryResult::Skip) {
            state.iteration_num += 1;
        }

        state.summary_results.push(SummaryEntry {
            identifier: issue.identifier.clone(),
            title: issue.title.clone(),
            result,
            branch: branch_name.clone(),
        });

        std::thread::sleep(Duration::from_secs(1));
    }

    Ok(())
}

/// Headless main loop (--no-tui mode). Unchanged from original logic but uses `--print`.
pub fn headless_loop(config: &Config, issues: &[LinearIssue], state: &mut IterationState) -> Result<()> {
    if config.max_duration.is_some() {
        state.start_time = Some(Instant::now());
    }

    if issues.is_empty() {
        eprintln!("Warning: No issues found in Linear view, nothing to do.");
        return Ok(());
    }

    let github = if config.enable_commits {
        GitHubClient::new().ok()
    } else {
        None
    };

    let issue_count = issues.len();

    for (issue_index, issue) in issues.iter().enumerate() {
        if let Some(max_runs) = config.max_runs {
            if max_runs != 0 && state.successful_iterations >= max_runs {
                break;
            }
        }

        if let Some(max_dur) = config.max_duration {
            if let Some(start) = state.start_time {
                if start.elapsed() >= max_dur {
                    eprintln!(
                        "\nTime limit reached ({})",
                        crate::duration::format_duration(start.elapsed().as_secs())
                    );
                    break;
                }
            }
        }

        if state.interrupted.load(Ordering::Relaxed) {
            eprintln!("Ctrl+C received -- stopping loop.");
            break;
        }

        eprintln!(
            "--- {} -- {} ({}, {}/{}) ---",
            issue.identifier,
            issue.title,
            issue.state.display_name(),
            issue_index + 1,
            issue_count
        );

        if git::is_git_repo() {
            if let Ok(true) = git::has_uncommitted_changes() {
                eprintln!("Error: Uncommitted changes detected.");
                std::process::exit(1);
            }
        }

        if git::is_git_repo() && !config.dry_run {
            eprintln!("Resetting to origin/main...");
            git::fetch("origin", "main").ok();
            git::checkout("main")
                .or_else(|_| git::checkout_new_from("main", "origin/main"))
                .ok();
            git::reset_hard("origin/main").ok();
        }

        let mut branch_name = issue.branch_name.clone();
        if let (Some(pr_url), Some(gh)) = (&issue.pr_url, &github) {
            if let Some((pr_owner, pr_repo, pr_number)) = parse_pr_url(pr_url) {
                if let Ok(pr_branch) = gh.get_pr_head_ref(&pr_owner, &pr_repo, pr_number) {
                    branch_name = Some(pr_branch);
                }
            }
        }

        let iteration_display = get_iteration_display(
            state.iteration_num,
            config.max_runs.unwrap_or(0),
            state.extra_iterations,
        );

        let result = match &issue.state {
            IssueStatus::Done | IssueStatus::InProgress => {
                eprintln!(
                    "Skipping {} -- status is '{}'",
                    issue.identifier,
                    issue.state.display_name()
                );
                SummaryResult::Skip
            }
            IssueStatus::InReview => {
                if let Some(gh) = &github {
                    match headless_handle_in_review(config, state, issue, &branch_name, &iteration_display, gh) {
                        Ok(()) => SummaryResult::Done,
                        Err(e) => {
                            eprintln!("Warning: Review handling failed: {e}");
                            SummaryResult::Fail
                        }
                    }
                } else {
                    eprintln!("Skipping review -- GitHub client not available");
                    SummaryResult::Skip
                }
            }
            IssueStatus::Other(_) => {
                let issue_prompt = prompt::build_issue_prompt(
                    &issue.identifier,
                    &issue.title,
                    issue.description.as_deref().unwrap_or(""),
                );

                match headless_execute_iteration(config, state, &issue_prompt, &branch_name, &issue.identifier, &iteration_display) {
                    Ok(()) => SummaryResult::Done,
                    Err(e) => {
                        eprintln!("Warning: Iteration failed: {e}");
                        SummaryResult::Fail
                    }
                }
            }
        };

        if !matches!(result, SummaryResult::Skip) {
            state.iteration_num += 1;
        }

        state.summary_results.push(SummaryEntry {
            identifier: issue.identifier.clone(),
            title: issue.title.clone(),
            result,
            branch: branch_name.clone(),
        });

        std::thread::sleep(Duration::from_secs(1));
    }

    Ok(())
}

// ---- TUI-mode iteration helpers ----

#[allow(clippy::too_many_arguments)]
fn execute_single_iteration(
    config: &Config,
    state: &mut IterationState,
    issue_prompt: &str,
    override_branch: &Option<String>,
    identifier: &str,
    iteration_display: &str,
    event_tx: &mpsc::Sender<AppEvent>,
    cmd_rx: &mpsc::Receiver<WorkerCommand>,
) -> Result<()> {
    let _ = event_tx.send(AppEvent::LogMessage(format!(
        "{iteration_display} Starting iteration..."
    )));

    let main_branch = git::current_branch().unwrap_or_else(|_| "main".to_string());
    let mut branch_name: Option<String> = None;

    if config.enable_commits && !config.disable_branches {
        match create_iteration_branch(
            iteration_display,
            state.iteration_num,
            override_branch,
            &config.git_branch_prefix,
            config.dry_run,
        ) {
            Ok(name) => {
                if let Some(ref b) = name {
                    let _ = event_tx.send(AppEvent::BranchChanged(Some(b.clone())));
                }
                branch_name = name;
            }
            Err(e) => {
                if git::is_git_repo() {
                    let _ = event_tx.send(AppEvent::LogMessage(format!(
                        "{iteration_display} Failed to create branch: {e}"
                    )));
                    state.error_count += 1;
                    state.extra_iterations += 1;
                    if state.error_count >= 3 {
                        anyhow::bail!("Fatal: 3 consecutive errors occurred.");
                    }
                    return Err(e);
                }
            }
        }
    }

    std::fs::create_dir_all(&config.notes_dir).ok();
    let notes_file = prompt::notes_file_path(&config.notes_dir, identifier);
    let notes_exist = std::path::Path::new(&notes_file).exists();

    let enhanced_prompt = prompt::build_iteration_prompt(
        issue_prompt,
        &config.completion_signal,
        &notes_file,
        notes_exist,
        config.review_prompt.as_deref(),
    );

    let _ = event_tx.send(AppEvent::LogMessage(format!(
        "{iteration_display} Launching Claude Code..."
    )));

    let exit_code = run_claude_with_events(
        &enhanced_prompt,
        &config.allowed_tools,
        &config.extra_claude_flags,
        config.dry_run,
        event_tx,
        cmd_rx,
    )?;

    if exit_code != 0 {
        let _ = event_tx.send(AppEvent::LogMessage(format!(
            "Claude Code exited with code: {exit_code}"
        )));
        if let Some(ref branch) = branch_name {
            if git::is_git_repo() {
                git::checkout(&main_branch).ok();
                git::delete_branch(branch).ok();
            }
        }
        state.error_count += 1;
        state.extra_iterations += 1;
        if state.error_count >= 3 {
            anyhow::bail!("Fatal: 3 consecutive errors occurred.");
        }
        return Err(anyhow::anyhow!("Claude exited with code {exit_code}"));
    }

    let _ = event_tx.send(AppEvent::LogMessage(format!(
        "{iteration_display} Claude session completed"
    )));

    if config.enable_commits {
        verify_commit_and_push(config, state, iteration_display, &branch_name, &main_branch, &notes_file, event_tx)?;
    } else {
        let _ = event_tx.send(AppEvent::LogMessage(format!(
            "{iteration_display} Skipping commit verification (--disable-commits)"
        )));
        if let Some(ref branch) = branch_name {
            if git::is_git_repo() {
                git::checkout(&main_branch).ok();
                git::delete_branch(branch).ok();
            }
        }
    }

    state.error_count = 0;
    if state.extra_iterations > 0 {
        state.extra_iterations -= 1;
    }
    state.successful_iterations += 1;
    Ok(())
}

/// Spawn Claude in a PTY, forward output bytes as events, and wait for exit.
fn run_claude_with_events(
    prompt: &str,
    allowed_tools: &str,
    extra_flags: &[String],
    dry_run: bool,
    event_tx: &mpsc::Sender<AppEvent>,
    cmd_rx: &mpsc::Receiver<WorkerCommand>,
) -> Result<i32> {
    if dry_run {
        let _ = event_tx.send(AppEvent::LogMessage("(DRY RUN) Would run Claude".into()));
        return Ok(0);
    }

    // Get approximate terminal dimensions for the PTY
    let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
    let pty_cols = (((term_cols as f32) * 0.75) as u16).saturating_sub(2);
    let pty_rows = term_rows.saturating_sub(3);

    let mut proc = crate::tui::claude_runner::spawn_claude(
        prompt,
        allowed_tools,
        extra_flags,
        pty_rows,
        pty_cols,
    )?;

    let _ = event_tx.send(AppEvent::OutputCleared);
    let _ = event_tx.send(AppEvent::PtyInputReady(proc.input_tx.clone()));

    loop {
        // Check skip/quit
        match cmd_rx.try_recv() {
            Ok(WorkerCommand::SkipCurrent) | Ok(WorkerCommand::Quit) => {
                let _ = proc.child.kill();
                let _ = proc.child.wait();
                return Ok(-1);
            }
            _ => {}
        }

        // Forward PTY bytes
        let mut got_data = false;
        while let Ok(bytes) = proc.byte_rx.try_recv() {
            got_data = true;
            let _ = event_tx.send(AppEvent::PtyBytes(bytes));
        }

        // Check child exit
        match proc.child.try_wait() {
            Ok(Some(status)) => {
                // Drain remaining bytes
                for bytes in proc.byte_rx.try_iter() {
                    let _ = event_tx.send(AppEvent::PtyBytes(bytes));
                }
                let code = if status.success() { 0 } else { 1 };
                let _ = event_tx.send(AppEvent::ClaudeFinished(code));
                return Ok(code);
            }
            Ok(None) => {
                if !got_data {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_in_review_issue(
    config: &Config,
    _state: &mut IterationState,
    issue: &LinearIssue,
    branch_name: &Option<String>,
    iteration_display: &str,
    github: &GitHubClient,
    event_tx: &mpsc::Sender<AppEvent>,
    cmd_rx: &mpsc::Receiver<WorkerCommand>,
) -> Result<()> {
    let _ = event_tx.send(AppEvent::LogMessage(format!(
        "{iteration_display} Handling review for {} ...",
        issue.identifier
    )));

    let branch = branch_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No branch name for issue {}", issue.identifier))?;

    let pr_url = issue
        .pr_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No PR found for issue {}", issue.identifier))?;

    let (pr_owner, pr_repo, pr_number) =
        parse_pr_url(pr_url).ok_or_else(|| anyhow::anyhow!("Could not parse PR URL: {pr_url}"))?;

    let _ = event_tx.send(AppEvent::BranchChanged(Some(branch.clone())));

    let ci_failures = fetch_ci_failures(github, &pr_owner, &pr_repo, pr_number)?;

    let inline_comments = github
        .get_pr_review_comments(&pr_owner, &pr_repo, pr_number)
        .unwrap_or_default();
    let reviews = github
        .get_pr_reviews(&pr_owner, &pr_repo, pr_number)
        .unwrap_or_default();
    let conversation = github
        .get_issue_comments(&pr_owner, &pr_repo, pr_number)
        .unwrap_or_default();

    let has_ci = ci_failures.is_some();
    let has_comments = !inline_comments.is_empty() || !reviews.is_empty() || !conversation.is_empty();

    if !has_ci && !has_comments {
        let _ = event_tx.send(AppEvent::LogMessage(
            "No review comments or CI failures, nothing to do".into(),
        ));
        return Ok(());
    }

    let inline_json = serde_json::to_string_pretty(&inline_comments).unwrap_or_default();
    let review_json = serde_json::to_string_pretty(&reviews).unwrap_or_default();
    let convo_json = serde_json::to_string_pretty(&conversation).unwrap_or_default();

    let review_prompt = prompt::build_review_prompt(
        &issue.identifier,
        &issue.title,
        pr_number,
        &pr_owner,
        &pr_repo,
        branch,
        ci_failures.as_deref(),
        &inline_json,
        &review_json,
        &convo_json,
    );

    let _ = event_tx.send(AppEvent::LogMessage(format!(
        "{iteration_display} Checking out PR branch: {branch}"
    )));
    git::fetch("origin", branch)?;
    git::checkout(branch)
        .or_else(|_| git::checkout_new_from(branch, &format!("origin/{branch}")))?;
    git::reset_hard(&format!("origin/{branch}")).ok();

    let _ = event_tx.send(AppEvent::LogMessage(format!(
        "{iteration_display} Launching Claude Code to resolve review..."
    )));

    let exit_code = run_claude_with_events(
        &review_prompt,
        &config.allowed_tools,
        &config.extra_claude_flags,
        config.dry_run,
        event_tx,
        cmd_rx,
    )?;

    if exit_code != 0 {
        git::checkout("main").ok();
        return Err(anyhow::anyhow!("Claude exited with code {exit_code}"));
    }

    let local_sha = git::rev_parse("HEAD")?;
    let remote_sha = git::ls_remote_head(branch)?.unwrap_or_default();

    if local_sha != remote_sha {
        let _ = event_tx.send(AppEvent::LogMessage("Pushing review fixes...".into()));
        git::push("origin", branch).ok();
    }

    git::checkout("main").ok();
    let _ = event_tx.send(AppEvent::LogMessage(format!(
        "{iteration_display} Review comments addressed for {}",
        issue.identifier
    )));
    Ok(())
}

fn verify_commit_and_push(
    config: &Config,
    state: &mut IterationState,
    iteration_display: &str,
    branch_name: &Option<String>,
    main_branch: &str,
    notes_file: &str,
    event_tx: &mpsc::Sender<AppEvent>,
) -> Result<()> {
    if !git::is_git_repo() {
        return Ok(());
    }

    let has_uncommitted = git::has_uncommitted_changes()?;
    if has_uncommitted {
        let _ = event_tx.send(AppEvent::LogMessage(format!(
            "{iteration_display} Uncommitted changes detected after Claude session"
        )));
    }

    let has_commits = if let Some(ref branch) = branch_name {
        git::commit_count_since(main_branch, branch).unwrap_or(0) > 0
    } else {
        false
    };

    if !has_commits && !has_uncommitted {
        let _ = event_tx.send(AppEvent::LogMessage(format!(
            "{iteration_display} No changes detected, cleaning up branch..."
        )));
        if let Some(ref branch) = branch_name {
            git::checkout(main_branch).ok();
            git::delete_branch(branch).ok();
        }
        return Ok(());
    }

    if config.dry_run {
        return Ok(());
    }

    if let Some(ref branch) = branch_name {
        if has_commits && !git::branch_exists_remote(branch)? {
            let _ = event_tx.send(AppEvent::LogMessage(format!(
                "{iteration_display} Pushing branch..."
            )));
            if git::push_branch(branch).is_err() {
                state.unpushed_branches.push(branch.clone());
            } else {
                let _ = event_tx.send(AppEvent::LogMessage(format!(
                    "{iteration_display} Pushed branch: {branch}"
                )));
            }
        }
    }

    if config.open_pr {
        if let Some(ref branch) = branch_name {
            if !state.unpushed_branches.contains(branch) {
                create_pr_if_needed(config, state, iteration_display, branch, main_branch, notes_file, event_tx)?;
            }
        }
    }

    if branch_name.is_some() {
        git::checkout(main_branch)?;
    }

    Ok(())
}

fn create_pr_if_needed(
    config: &Config,
    _state: &mut IterationState,
    iteration_display: &str,
    branch: &str,
    base: &str,
    notes_file: &str,
    event_tx: &mpsc::Sender<AppEvent>,
) -> Result<()> {
    let gh = GitHubClient::new()?;

    if let Some(pr_num) = gh.find_pr_for_branch(&config.github_owner, &config.github_repo, branch)? {
        let _ = event_tx.send(AppEvent::LogMessage(format!(
            "{iteration_display} PR #{pr_num} already exists for {branch}"
        )));
        return Ok(());
    }

    let _ = event_tx.send(AppEvent::LogMessage(format!(
        "{iteration_display} Creating pull request..."
    )));

    let commit_message = git::log_last_message(branch)?;
    let mut lines = commit_message.lines();
    let title = lines.next().unwrap_or("").to_string();
    let body: String = lines.skip(1).collect::<Vec<&str>>().join("\n");

    match gh.create_pr(
        &config.github_owner,
        &config.github_repo,
        &title,
        &body,
        branch,
        base,
        true,
    ) {
        Ok(pr_number) => {
            let _ = event_tx.send(AppEvent::LogMessage(format!(
                "{iteration_display} PR #{pr_number} created: {title}"
            )));

            if std::path::Path::new(notes_file).exists() {
                if let Ok(notes) = std::fs::read_to_string(notes_file) {
                    if !notes.is_empty() {
                        let comment = format!("## Claude's Notes\n\n{notes}");
                        gh.post_comment(&config.github_owner, &config.github_repo, pr_number, &comment).ok();
                    }
                }
            }
        }
        Err(e) => {
            let _ = event_tx.send(AppEvent::LogMessage(format!(
                "{iteration_display} Failed to create PR: {e}"
            )));
        }
    }

    Ok(())
}

// ---- Headless mode helpers ----

fn headless_execute_iteration(
    config: &Config,
    state: &mut IterationState,
    issue_prompt: &str,
    override_branch: &Option<String>,
    identifier: &str,
    iteration_display: &str,
) -> Result<()> {
    eprintln!("{iteration_display} Starting iteration...");

    let main_branch = git::current_branch().unwrap_or_else(|_| "main".to_string());
    let mut branch_name: Option<String> = None;

    if config.enable_commits && !config.disable_branches {
        match create_iteration_branch(iteration_display, state.iteration_num, override_branch, &config.git_branch_prefix, config.dry_run) {
            Ok(name) => branch_name = name,
            Err(e) => {
                if git::is_git_repo() {
                    eprintln!("{iteration_display} Failed to create branch: {e}");
                    state.error_count += 1;
                    state.extra_iterations += 1;
                    if state.error_count >= 3 {
                        anyhow::bail!("Fatal: 3 consecutive errors occurred.");
                    }
                    return Err(e);
                }
            }
        }
    }

    std::fs::create_dir_all(&config.notes_dir).ok();
    let notes_file = prompt::notes_file_path(&config.notes_dir, identifier);
    let notes_exist = std::path::Path::new(&notes_file).exists();

    let enhanced_prompt = prompt::build_iteration_prompt(
        issue_prompt,
        &config.completion_signal,
        &notes_file,
        notes_exist,
        config.review_prompt.as_deref(),
    );

    eprintln!("{iteration_display} Launching Claude Code (print mode)...");

    let exit_code = claude::run_claude_print(
        &enhanced_prompt,
        &config.allowed_tools,
        &config.extra_claude_flags,
        config.dry_run,
    )?;

    if exit_code != 0 {
        eprintln!("Warning: Claude Code exited with code: {exit_code}");
        if let Some(ref branch) = branch_name {
            if git::is_git_repo() {
                git::checkout(&main_branch).ok();
                git::delete_branch(branch).ok();
            }
        }
        state.error_count += 1;
        state.extra_iterations += 1;
        if state.error_count >= 3 {
            anyhow::bail!("Fatal: 3 consecutive errors occurred.");
        }
        return Err(anyhow::anyhow!("Claude exited with code {exit_code}"));
    }

    eprintln!("{iteration_display} Claude session completed");

    if config.enable_commits {
        headless_verify_commit_and_push(config, state, iteration_display, &branch_name, &main_branch, &notes_file)?;
    } else {
        if let Some(ref branch) = branch_name {
            if git::is_git_repo() {
                git::checkout(&main_branch).ok();
                git::delete_branch(branch).ok();
            }
        }
    }

    state.error_count = 0;
    if state.extra_iterations > 0 {
        state.extra_iterations -= 1;
    }
    state.successful_iterations += 1;
    Ok(())
}

fn headless_verify_commit_and_push(
    config: &Config,
    state: &mut IterationState,
    iteration_display: &str,
    branch_name: &Option<String>,
    main_branch: &str,
    notes_file: &str,
) -> Result<()> {
    if !git::is_git_repo() {
        return Ok(());
    }

    let has_uncommitted = git::has_uncommitted_changes()?;
    let has_commits = if let Some(ref branch) = branch_name {
        git::commit_count_since(main_branch, branch).unwrap_or(0) > 0
    } else {
        false
    };

    if !has_commits && !has_uncommitted {
        eprintln!("{iteration_display} No changes detected, cleaning up branch...");
        if let Some(ref branch) = branch_name {
            git::checkout(main_branch).ok();
            git::delete_branch(branch).ok();
        }
        return Ok(());
    }

    if config.dry_run {
        return Ok(());
    }

    if let Some(ref branch) = branch_name {
        if has_commits && !git::branch_exists_remote(branch)? {
            eprintln!("{iteration_display} Pushing branch...");
            if git::push_branch(branch).is_err() {
                state.unpushed_branches.push(branch.clone());
            }
        }
    }

    if config.open_pr {
        if let Some(ref branch) = branch_name {
            if !state.unpushed_branches.contains(branch) {
                headless_create_pr(config, state, iteration_display, branch, main_branch, notes_file)?;
            }
        }
    }

    if branch_name.is_some() {
        git::checkout(main_branch)?;
    }

    Ok(())
}

fn headless_create_pr(
    config: &Config,
    _state: &mut IterationState,
    iteration_display: &str,
    branch: &str,
    base: &str,
    notes_file: &str,
) -> Result<()> {
    let gh = GitHubClient::new()?;
    if let Some(pr_num) = gh.find_pr_for_branch(&config.github_owner, &config.github_repo, branch)? {
        eprintln!("{iteration_display} PR #{pr_num} already exists for {branch}");
        return Ok(());
    }

    eprintln!("{iteration_display} Creating pull request...");
    let commit_message = git::log_last_message(branch)?;
    let mut lines = commit_message.lines();
    let title = lines.next().unwrap_or("").to_string();
    let body: String = lines.skip(1).collect::<Vec<&str>>().join("\n");

    match gh.create_pr(&config.github_owner, &config.github_repo, &title, &body, branch, base, true) {
        Ok(pr_number) => {
            eprintln!("{iteration_display} PR #{pr_number} created: {title}");
            if std::path::Path::new(notes_file).exists() {
                if let Ok(notes) = std::fs::read_to_string(notes_file) {
                    if !notes.is_empty() {
                        let comment = format!("## Claude's Notes\n\n{notes}");
                        gh.post_comment(&config.github_owner, &config.github_repo, pr_number, &comment).ok();
                    }
                }
            }
        }
        Err(e) => eprintln!("{iteration_display} Failed to create PR: {e}"),
    }

    Ok(())
}

fn headless_handle_in_review(
    config: &Config,
    _state: &mut IterationState,
    issue: &LinearIssue,
    branch_name: &Option<String>,
    iteration_display: &str,
    github: &GitHubClient,
) -> Result<()> {
    let branch = branch_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No branch name for issue {}", issue.identifier))?;

    let pr_url = issue
        .pr_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No PR found for issue {}", issue.identifier))?;

    let (pr_owner, pr_repo, pr_number) =
        parse_pr_url(pr_url).ok_or_else(|| anyhow::anyhow!("Could not parse PR URL: {pr_url}"))?;

    let ci_failures = fetch_ci_failures(github, &pr_owner, &pr_repo, pr_number)?;

    let inline_comments = github.get_pr_review_comments(&pr_owner, &pr_repo, pr_number).unwrap_or_default();
    let reviews = github.get_pr_reviews(&pr_owner, &pr_repo, pr_number).unwrap_or_default();
    let conversation = github.get_issue_comments(&pr_owner, &pr_repo, pr_number).unwrap_or_default();

    let has_ci = ci_failures.is_some();
    let has_comments = !inline_comments.is_empty() || !reviews.is_empty() || !conversation.is_empty();

    if !has_ci && !has_comments {
        eprintln!("{iteration_display} No review comments or CI failures, nothing to do");
        return Ok(());
    }

    let inline_json = serde_json::to_string_pretty(&inline_comments).unwrap_or_default();
    let review_json = serde_json::to_string_pretty(&reviews).unwrap_or_default();
    let convo_json = serde_json::to_string_pretty(&conversation).unwrap_or_default();

    let review_prompt = prompt::build_review_prompt(
        &issue.identifier, &issue.title, pr_number, &pr_owner, &pr_repo, branch,
        ci_failures.as_deref(), &inline_json, &review_json, &convo_json,
    );

    git::fetch("origin", branch)?;
    git::checkout(branch)
        .or_else(|_| git::checkout_new_from(branch, &format!("origin/{branch}")))?;
    git::reset_hard(&format!("origin/{branch}")).ok();

    eprintln!("{iteration_display} Launching Claude Code to resolve review...");

    let exit_code = claude::run_claude_print(
        &review_prompt,
        &config.allowed_tools,
        &config.extra_claude_flags,
        config.dry_run,
    )?;

    if exit_code != 0 {
        git::checkout("main").ok();
        return Err(anyhow::anyhow!("Claude exited with code {exit_code}"));
    }

    let local_sha = git::rev_parse("HEAD")?;
    let remote_sha = git::ls_remote_head(branch)?.unwrap_or_default();

    if local_sha != remote_sha {
        eprintln!("{iteration_display} Pushing review fixes...");
        git::push("origin", branch).ok();
    }

    git::checkout("main").ok();
    Ok(())
}

// ---- Shared helpers ----

fn check_quit(cmd_rx: &mpsc::Receiver<WorkerCommand>) -> bool {
    matches!(cmd_rx.try_recv(), Ok(WorkerCommand::Quit))
}

fn create_iteration_branch(
    _iteration_display: &str,
    iteration_num: u32,
    override_branch: &Option<String>,
    prefix: &str,
    dry_run: bool,
) -> Result<Option<String>> {
    if !git::is_git_repo() {
        return Ok(None);
    }

    let current = git::current_branch()?;
    if current.starts_with(prefix) {
        git::checkout("main")?;
    }

    let branch_name = if let Some(name) = override_branch {
        name.clone()
    } else {
        let date = chrono_free_date();
        let hash = random_hex(8);
        format!("{prefix}iteration-{iteration_num}/{date}-{hash}")
    };

    if dry_run {
        return Ok(Some(branch_name));
    }

    if git::branch_exists_remote(&branch_name)? {
        git::fetch("origin", &branch_name)?;
        git::checkout(&branch_name)
            .or_else(|_| git::checkout_new_from(&branch_name, &format!("origin/{branch_name}")))?;
    } else if git::branch_exists_local(&branch_name)? {
        git::checkout(&branch_name)?;
    } else {
        git::checkout_new(&branch_name)?;
    }

    Ok(Some(branch_name))
}

fn fetch_ci_failures(
    github: &GitHubClient,
    owner: &str,
    repo: &str,
    pr_number: u64,
) -> Result<Option<String>> {
    let head_sha = github.get_pr_head_sha(owner, repo, pr_number)?;

    let failed_checks = github.get_failed_checks(owner, repo, &head_sha)?;

    if failed_checks.is_empty() {
        let failed_statuses = github.get_failed_statuses(owner, repo, &head_sha)?;
        if failed_statuses.is_empty() {
            return Ok(None);
        }

        let mut context = "### Failing CI Statuses\n".to_string();
        for s in &failed_statuses {
            context.push_str(&format!(
                "- **{}**: {} -- {}\n",
                s.context,
                s.state,
                s.description.as_deref().unwrap_or("no description")
            ));
        }
        return Ok(Some(context));
    }

    let mut ci_context = "### Failing CI Checks\n".to_string();

    for check in &failed_checks {
        ci_context.push_str(&format!("\n#### Check: {}\n", check.name));

        if let Ok(annotations) = github.get_check_annotations(owner, repo, check.id) {
            if !annotations.is_empty() {
                ci_context.push_str("Annotations:\n```\n");
                for ann in &annotations {
                    let line = format!(
                        "[{}] {}:{} -- {}\n",
                        ann.annotation_level, ann.path, ann.start_line, ann.message
                    );
                    ci_context.push_str(&line[..line.len().min(3000)]);
                }
                ci_context.push_str("```\n");
            }
        }

        if let Ok(run_ids) = github.get_failed_workflow_runs(owner, repo, &head_sha) {
            for run_id in run_ids.iter().take(3) {
                if let Ok(jobs) = github.get_failed_jobs(owner, repo, *run_id) {
                    for job in &jobs {
                        if let Ok(logs) = github.get_job_logs(owner, repo, job.id) {
                            let truncated = if logs.len() > 5000 { &logs[..5000] } else { &logs };
                            ci_context.push_str(&format!(
                                "\nJob: {} (last 100 lines):\n```\n{truncated}\n```\n",
                                job.name
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(Some(ci_context))
}

fn parse_pr_url(url: &str) -> Option<(String, String, u64)> {
    let re = regex::Regex::new(r"github\.com/([^/]+)/([^/]+)/pull/(\d+)").ok()?;
    let caps = re.captures(url)?;
    let owner = caps[1].to_string();
    let repo = caps[2].to_string();
    let number: u64 = caps[3].parse().ok()?;
    Some((owner, repo, number))
}

fn get_iteration_display(iteration_num: u32, max_runs: u32, extra_iters: u32) -> String {
    if max_runs == 0 {
        format!("({iteration_num})")
    } else {
        let total = max_runs + extra_iters;
        format!("({iteration_num}/{total})")
    }
}

fn chrono_free_date() -> String {
    std::process::Command::new("date")
        .arg("+%Y-%m-%d")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown-date".to_string())
}

fn random_hex(len: usize) -> String {
    use std::io::Read;
    let mut buf = vec![0u8; len / 2 + 1];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        f.read_exact(&mut buf).ok();
    } else {
        let seed = std::process::id() as u64 ^ now_epoch_secs();
        buf = seed.to_le_bytes().to_vec();
    }
    hex::encode(&buf)[..len].to_string()
}

fn now_epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
