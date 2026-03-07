use crate::cli::CommonArgs;
use crate::duration::parse_duration;
use crate::git;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::time::Duration;

pub struct Config {
    pub linear_view: String,
    pub max_runs: Option<u32>,
    pub max_duration: Option<Duration>,
    pub github_owner: String,
    pub github_repo: String,
    pub enable_commits: bool,
    pub disable_branches: bool,
    pub git_branch_prefix: String,
    pub notes_dir: PathBuf,
    pub dry_run: bool,
    pub completion_signal: String,
    #[allow(dead_code)]
    pub completion_threshold: u32,
    pub review_prompt: Option<String>,
    pub open_pr: bool,
    pub allowed_tools: String,
    pub extra_claude_flags: Vec<String>,
    pub no_tui: bool,
}

impl Config {
    pub fn from_view_args(args: crate::cli::ViewArgs) -> Result<Self> {
        if args.linear_view.is_empty() {
            bail!("Linear view URL or ID is required");
        }
        Self::from_common(args.linear_view, args.common)
    }

    pub fn from_ticket_args(args: crate::cli::TicketArgs) -> Result<Self> {
        if args.issue.is_empty() {
            bail!("Linear issue identifier or URL is required");
        }
        // For ticket mode, use a sentinel value for linear_view since we won't use it
        Self::from_common(String::new(), args.common)
    }

    fn from_common(linear_view: String, args: CommonArgs) -> Result<Self> {
        let max_duration = match &args.max_duration {
            Some(d) => {
                let secs = parse_duration(d)
                    .context("--max-duration must be a valid duration (e.g., '2h', '30m', '1h30m', '90s')")?;
                Some(Duration::from_secs(secs))
            }
            None => None,
        };

        if args.completion_threshold < 1 {
            bail!("--completion-threshold must be a positive integer");
        }

        let enable_commits = !args.disable_commits;

        // Detect GitHub owner/repo
        let (github_owner, github_repo) = if enable_commits {
            let owner = args.owner.clone();
            let repo = args.repo.clone();

            let (detected_owner, detected_repo) = match (owner, repo) {
                (Some(o), Some(r)) => (o, r),
                (o, r) => {
                    let detected = git::detect_github_repo().ok();
                    let final_owner = o.or_else(|| detected.as_ref().map(|(o, _)| o.clone()));
                    let final_repo = r.or_else(|| detected.as_ref().map(|(_, r)| r.clone()));
                    match (final_owner, final_repo) {
                        (Some(o), Some(r)) => (o, r),
                        (None, _) => bail!("GitHub owner is required. Use --owner or run from a git repository with a GitHub remote."),
                        (_, None) => bail!("GitHub repo is required. Use --repo or run from a git repository with a GitHub remote."),
                    }
                }
            };
            (detected_owner, detected_repo)
        } else {
            (String::new(), String::new())
        };

        Ok(Config {
            linear_view,
            max_runs: args.max_runs,
            max_duration,
            github_owner,
            github_repo,
            enable_commits,
            disable_branches: args.disable_branches,
            git_branch_prefix: args.git_branch_prefix,
            notes_dir: args.notes_dir,
            dry_run: args.dry_run,
            completion_signal: args.completion_signal,
            completion_threshold: args.completion_threshold,
            review_prompt: args.review_prompt,
            open_pr: args.open_pr,
            allowed_tools: args.allowed_tools,
            extra_claude_flags: args.extra_claude_flags,
            no_tui: args.no_tui,
        })
    }
}
