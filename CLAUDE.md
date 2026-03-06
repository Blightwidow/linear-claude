# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**linear-claude** is a Rust CLI tool that fetches issues from a Linear custom view and runs Claude Code iteratively on each issue, creating branches, committing, pushing, and optionally opening PRs.

## File Structure

```
Cargo.toml                  # Rust project manifest
src/
  main.rs                   # Entry point, dispatch to subcommands, signal handling
  cli.rs                    # Clap CLI definitions (Commands enum, ViewArgs struct)
  config.rs                 # Config struct built from ViewArgs after validation
  version.rs                # VERSION const, version_lt()
  duration.rs               # parse_duration(), format_duration()
  prompt.rs                 # Prompt templates and builders
  git.rs                    # Thin wrappers around git CLI commands
  claude.rs                 # Spawns claude with optional PTY header
  iteration.rs              # Main loop, execute_single_iteration, handle_in_review
  summary.rs                # Completion summary table formatting
  update.rs                 # check_for_updates(), cmd_update() with SHA256 verify
  linear/
    mod.rs
    client.rs               # fetch_view_issues() via GraphQL + ureq
    types.rs                # LinearIssue, IssueStatus, GraphQL response types
  github/
    mod.rs
    client.rs               # PR creation, CI checks, review comments via REST + ureq
    types.rs                # PullRequest, CheckRun, Annotation, PrComment, etc.
  pty/
    mod.rs
    relay.rs                # PTY I/O relay loop with alt-screen interception
    header.rs               # paint_header(), scroll region, iTerm2 badge
install.sh                  # One-line installer (detects platform, downloads binary)
.github/workflows/release.yml  # Cross-compilation matrix + GitHub Release
.env.example                # Template for LOCAL_API_KEY and GITHUB_TOKEN
CHANGELOG.md                # Release notes
LICENSE                     # MIT license
```

## Architecture

### CLI Structure

Built with `clap` derive macros. Subcommand routing in `main.rs`:

```
linear-claude view <url-or-id> [options]   # main command
linear-claude update                        # self-update
linear-claude version                       # show version
linear-claude help                          # show help
```

### Core Flow

1. **`main()`** loads `.env` via `dotenvy`, parses CLI via `clap`, routes to subcommands
2. **`cmd_view()`** builds `Config` from `ViewArgs`, checks for updates, validates requirements, fetches Linear issues, runs `main_loop()`
3. **`LinearClient::fetch_view_issues()`** calls Linear's GraphQL API directly via `ureq` with cursor-based pagination
4. **`main_loop()`** iterates over issues with status-based routing:
   - `Done` / `InProgress` -> skipped
   - `InReview` -> `handle_in_review_issue()` (fetches CI failures + PR review comments via GitHub REST API, runs Claude to fix)
   - `Other` (todo, backlog, etc.) -> `execute_single_iteration()`
5. **`execute_single_iteration()`** creates a branch, builds an enhanced prompt, runs Claude Code via PTY, then verifies commit/push
6. **`run_with_header()`** in `pty/relay.rs` spawns Claude in a PTY with a persistent header row, intercepts alt-screen sequences

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI parsing with derive macros |
| `ureq` | Blocking HTTP for Linear GraphQL + GitHub REST |
| `portable-pty` | Cross-platform PTY for Claude subprocess |
| `crossterm` | Terminal raw mode, size detection |
| `signal-hook` | SIGWINCH/SIGINT/SIGTERM handling |
| `serde` + `serde_json` | JSON serialization/deserialization |
| `sha2` | SHA256 checksum for self-update |
| `dotenvy` | Load `.env` file for credentials |
| `toml` | Parse Linear credentials file |

### External Dependencies (CLI tools)

- **`claude`** (Claude Code CLI) — spawned in a PTY, the AI engine
- **`git`** — branch management, commits, push

**Eliminated** (compared to the original bash version):
- `linear` CLI -> direct GraphQL via `ureq`
- `gh` CLI -> direct GitHub REST API (soft dep: `gh auth token` as token fallback)
- `jq` -> `serde_json`
- Python 3 -> native Rust PTY via `portable-pty`

### Credential Resolution

**Linear API key** (in order):
1. `LINEAR_API_KEY` env var
2. `.env` file in working directory
3. `~/.config/linear/credentials.toml`

**GitHub token** (in order):
1. `GITHUB_TOKEN` env var
2. `.env` file in working directory
3. `gh auth token` shell-out fallback

## Building & Running

```bash
# Build
cargo build

# Run
cargo run -- view "https://linear.app/team/view/abc123"
cargo run -- view abc123 -m 3 --max-duration 2h
cargo run -- view abc123 --open-pr
cargo run -- view abc123 --disable-commits  # testing mode
cargo run -- view abc123 --dry-run
cargo run -- update
cargo run -- version

# Tests
cargo test
```

## Testing

Unit tests are inline in their modules (`duration.rs`, `version.rs`). Run with `cargo test`.

## Releasing

1. Bump `version` in `Cargo.toml` and `VERSION` in `src/version.rs`
2. Commit: `git commit -m "Bump vX.Y.Z"`
3. Tag: `git tag vX.Y.Z`
4. Push: `git push && git push --tags`
5. GitHub Actions cross-compiles for 4 targets (macOS x86_64/aarch64, Linux x86_64/aarch64), creates a GitHub Release with binaries + checksums, and commits the updated `CHANGELOG.md` back to main

## Conventions

- Error output goes to stderr, data output to stdout
- All HTTP is blocking (`ureq`) — no async runtime
- Config struct replaces all `LC_` global variables from the bash version
- 3 consecutive errors = fatal exit
- Notes files stored in `.claude/plans/<identifier>.md` per issue

## Security / Personal Data

- **Never hardcode** absolute paths, API tokens, secret keys, email addresses, or personal names outside of `LICENSE`
- **Before committing**, grep the staged diff for the current user's home directory path and common PII patterns
- **Use placeholder values** in examples and docs: `team`, `abc123` — not real usernames, org names, or IDs
- `.gitignore` excludes `.env`, `.claude/settings.local.json`, `.claude/plans/`, `CLAUDE.local.md`
