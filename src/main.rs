mod cli;
mod claude;
mod config;
mod duration;
mod git;
mod github;
mod iteration;
mod linear;
mod prompt;
mod summary;
mod tui;
mod update;
mod version;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use linear::types::LinearIssue;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::Arc;
use tui::app::{App, IssueDisplayStatus, IssueEntry};
use tui::event::{AppEvent, EventSystem};

fn main() {
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    if cli.version {
        println!("linear-claude version {}", version::VERSION);
        return;
    }

    let command = match cli.command {
        Some(cmd) => cmd,
        None => {
            show_help();
            return;
        }
    };

    match command {
        Commands::View(args) => cmd_view(args),
        Commands::Ticket(args) => cmd_ticket(args),
        Commands::Update => {
            if let Err(e) = update::cmd_update() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Version => {
            println!("linear-claude version {}", version::VERSION);
        }
    }
}

fn cmd_view(args: cli::ViewArgs) {
    let config = match Config::from_view_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    update::check_for_updates();
    validate_requirements(&config);

    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_clone = interrupted.clone();
    ctrlc_setup(interrupted_clone);

    let issues = if config.dry_run {
        eprintln!("(DRY RUN) Would fetch Linear view issues from: {}", config.linear_view);
        Vec::new()
    } else {
        match linear::client::LinearClient::new().and_then(|c| c.fetch_view_issues(&config.linear_view)) {
            Ok(issues) => {
                eprintln!("Found {} issues in Linear view", issues.len());
                issues
            }
            Err(e) => {
                eprintln!("Error: Failed to fetch Linear view issues: {e}");
                std::process::exit(1);
            }
        }
    };

    run_with_issues(config, issues, interrupted);
}

fn cmd_ticket(args: cli::TicketArgs) {
    let issue_id = args.issue.clone();
    let config = match Config::from_ticket_args(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };

    update::check_for_updates();
    validate_requirements(&config);

    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_clone = interrupted.clone();
    ctrlc_setup(interrupted_clone);

    let issues = if config.dry_run {
        eprintln!("(DRY RUN) Would fetch Linear issue: {issue_id}");
        Vec::new()
    } else {
        match linear::client::LinearClient::new().and_then(|c| c.fetch_issue(&issue_id)) {
            Ok(issue) => {
                eprintln!("Found issue: {} - {}", issue.identifier, issue.title);
                vec![issue]
            }
            Err(e) => {
                eprintln!("Error: Failed to fetch Linear issue: {e}");
                std::process::exit(1);
            }
        }
    };

    run_with_issues(config, issues, interrupted);
}

fn run_with_issues(config: Config, issues: Vec<LinearIssue>, interrupted: Arc<AtomicBool>) {
    if config.no_tui {
        run_headless(config, issues, interrupted);
    } else {
        run_tui(config, issues, interrupted);
    }
}

fn run_headless(config: Config, issues: Vec<LinearIssue>, interrupted: Arc<AtomicBool>) {
    let mut state = iteration::IterationState::new(interrupted);

    if let Err(e) = iteration::headless_loop(&config, &issues, &mut state) {
        eprintln!("Error: {e}");
    }

    summary::show_completion_summary(&state.summary_results, &state.unpushed_branches, state.start_time);
}

fn run_tui(config: Config, issues: Vec<LinearIssue>, interrupted: Arc<AtomicBool>) {
    tui::install_panic_hook();

    let issue_entries: Vec<IssueEntry> = issues
        .iter()
        .map(|i| IssueEntry {
            identifier: i.identifier.clone(),
            title: i.title.clone(),
            status: IssueDisplayStatus::Queued,
        })
        .collect();

    let mut app = App::new(issue_entries);

    let mut terminal = match tui::init_terminal() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Error: Failed to initialize terminal: {e}");
            // Fall back to headless
            run_headless(config, issues, interrupted);
            return;
        }
    };

    let mut events = EventSystem::new();
    events.start_input_thread();

    // Take cmd_rx out for the worker thread
    let cmd_rx = events.cmd_rx.take().unwrap();
    let event_tx = events.app_tx.clone();

    // Spawn worker thread
    let worker_handle = std::thread::spawn(move || {
        let mut state = iteration::IterationState::new(interrupted);
        iteration::worker_loop(&config, &issues, &mut state, event_tx, cmd_rx);
        state
    });

    // Main event loop
    loop {
        // Draw
        if let Err(e) = terminal.draw(|frame| tui::ui::draw(frame, &mut app)) {
            eprintln!("Draw error: {e}");
            break;
        }

        // Process events
        let visible_height = tui::ui::output_visible_height(terminal.size().map(|s| s.height).unwrap_or(24));

        match events.app_rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(event) => {
                match event {
                    AppEvent::Key(key) => {
                        if tui::event::handle_key(key, &mut app, &events.cmd_tx, visible_height) {
                            break;
                        }
                    }
                    AppEvent::Tick => {}
                    AppEvent::IssueStatusChanged { index, status } => {
                        // Update counters
                        match &status {
                            IssueDisplayStatus::Done => app.done_count += 1,
                            IssueDisplayStatus::Failed => app.failed_count += 1,
                            IssueDisplayStatus::Skipped => app.skipped_count += 1,
                            _ => {}
                        }
                        if let Some(entry) = app.issues.get_mut(index) {
                            entry.status = status;
                        }
                    }
                    AppEvent::BranchChanged(branch) => {
                        app.current_branch = branch;
                    }
                    AppEvent::ClaudeOutput(line) => {
                        app.push_output_line(line);
                    }
                    AppEvent::ClaudeFinished(_code) => {}
                    AppEvent::OutputCleared => {
                        app.clear_output();
                    }
                    AppEvent::LogMessage(msg) => {
                        app.push_log_line(msg.clone());
                        app.push_output_line(format!("# {msg}"));
                    }
                    AppEvent::WorkerDone => {
                        app.worker_done = true;
                        app.push_output_line(String::new());
                        app.push_output_line("--- Worker finished. Press q to exit. ---".into());
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if app.should_quit && app.worker_done {
            break;
        }
    }

    // Restore terminal
    tui::restore_terminal(&mut terminal);

    // Wait for worker and get final state
    match worker_handle.join() {
        Ok(state) => {
            summary::show_completion_summary(&state.summary_results, &state.unpushed_branches, state.start_time);
        }
        Err(_) => {
            eprintln!("Worker thread panicked");
        }
    }
}

fn validate_requirements(config: &Config) {
    if which("claude").is_none() {
        eprintln!("Error: Claude Code is not installed: https://claude.ai/code");
        std::process::exit(1);
    }

    if config.enable_commits && which("git").is_none() {
        eprintln!("Error: git is required for commit operations");
        std::process::exit(1);
    }
}

fn which(cmd: &str) -> Option<String> {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn show_help() {
    println!(
        r#"Linear Claude - Run Claude Code iteratively on Linear issues

USAGE:
    linear-claude <command> [options]

COMMANDS:
    view <url-or-id>    Process issues from a Linear custom view
    ticket <id-or-url>  Process a single Linear issue
    update              Update linear-claude to the latest version
    version             Show version information
    help                Show this help message

GLOBAL OPTIONS:
    -h, --help          Show this help message
    -v, --version       Show version information

EXAMPLES:
    linear-claude view "https://linear.app/team/view/abc123"
    linear-claude view abc123 -m 3 --max-duration 2h
    linear-claude ticket TEAM-123
    linear-claude ticket "https://linear.app/team/issue/TEAM-123/slug"
    linear-claude update
    linear-claude version

Run 'linear-claude <command> --help' for more information on a specific command."#
    );
}

fn ctrlc_setup(interrupted: Arc<AtomicBool>) {
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, interrupted.clone());
    let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, interrupted);
}
