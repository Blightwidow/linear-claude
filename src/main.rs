mod cli;
mod claude;
mod config;
mod duration;
mod git;
mod github;
mod iteration;
mod linear;
mod prompt;
mod pty;
mod summary;
mod update;
mod version;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn main() {
    // Load .env file if present (non-fatal)
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

    // Set up signal handling
    let interrupted = Arc::new(AtomicBool::new(false));
    let interrupted_clone = interrupted.clone();
    ctrlc_setup(interrupted_clone);

    // Fetch issues
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

    let mut state = iteration::IterationState::new(interrupted);

    if let Err(e) = iteration::main_loop(&config, &issues, &mut state) {
        eprintln!("Error: {e}");
    }

    // Reset terminal title
    let mut stdout = std::io::stderr();
    pty::header::reset_terminal_title(&mut stdout).ok();

    summary::show_completion_summary(&state.summary_results, &state.unpushed_branches, state.start_time);
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

    let mut state = iteration::IterationState::new(interrupted);

    if let Err(e) = iteration::main_loop(&config, &issues, &mut state) {
        eprintln!("Error: {e}");
    }

    let mut stdout = std::io::stderr();
    pty::header::reset_terminal_title(&mut stdout).ok();

    summary::show_completion_summary(&state.summary_results, &state.unpushed_branches, state.start_time);
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
