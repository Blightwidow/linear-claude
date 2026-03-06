use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "linear-claude", about = "Run Claude Code iteratively on Linear issues")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Show version information
    #[arg(short, long)]
    pub version: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Process issues from a Linear custom view
    View(ViewArgs),
    /// Process a single Linear issue by identifier or URL
    Ticket(TicketArgs),
    /// Update linear-claude to the latest version
    Update,
    /// Show version information
    Version,
}

#[derive(Parser, Debug)]
pub struct ViewArgs {
    /// Linear view URL or ID
    pub linear_view: String,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Parser, Debug)]
pub struct TicketArgs {
    /// Linear issue identifier (e.g., "TEAM-123") or URL
    pub issue: String,

    #[command(flatten)]
    pub common: CommonArgs,
}

#[derive(Parser, Debug)]
pub struct CommonArgs {
    /// Maximum number of successful iterations (use 0 for unlimited with --max-duration)
    #[arg(short = 'm', long)]
    pub max_runs: Option<u32>,

    /// Maximum duration to run (e.g., "2h", "30m", "1h30m")
    #[arg(long)]
    pub max_duration: Option<String>,

    /// GitHub repository owner (auto-detected from git remote if not provided)
    #[arg(long)]
    pub owner: Option<String>,

    /// GitHub repository name (auto-detected from git remote if not provided)
    #[arg(long)]
    pub repo: Option<String>,

    /// Disable automatic commits and PR creation
    #[arg(long)]
    pub disable_commits: bool,

    /// Commit on current branch without creating branches or PRs
    #[arg(long)]
    pub disable_branches: bool,

    /// Branch prefix for iterations
    #[arg(long, default_value = "linear-claude/")]
    pub git_branch_prefix: String,

    /// Directory for per-ticket notes files
    #[arg(long, default_value = "./.claude/plans")]
    pub notes_dir: PathBuf,

    /// Simulate execution without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Phrase that agents output when project is complete
    #[arg(long, default_value = "LINEAR_CLAUDE_PROJECT_COMPLETE")]
    pub completion_signal: String,

    /// Number of consecutive signals to stop early
    #[arg(long, default_value_t = 3)]
    pub completion_threshold: u32,

    /// Additional review instructions appended to the main prompt
    #[arg(short = 'r', long)]
    pub review_prompt: Option<String>,

    /// Comma-separated tools for Claude
    #[arg(long, default_value = "Bash,Read,Edit,Write,Grep,Glob,WebFetch,WebSearch")]
    pub allowed_tools: String,

    /// Create a PR after pushing
    #[arg(long)]
    pub open_pr: bool,

    /// Extra flags forwarded to claude CLI
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub extra_claude_flags: Vec<String>,
}
