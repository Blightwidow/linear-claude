# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**linear-claude** is a fork/adaptation of [Continuous Claude](https://github.com/AnandChowdhary/continuous-claude) that integrates with **Linear** (project management) instead of using a free-form prompt. It fetches issues from a Linear custom view and runs Claude Code iteratively on each issue, creating branches, committing, pushing, and optionally opening PRs.

The entire project is a single Bash script (`linear_claude.sh`) plus an installer (`install.sh`) and a GitHub Actions release workflow.

## File Structure

| File | Role |
|------|------|
| `linear_claude.sh` | Main script — CLI, Linear integration, Claude orchestration |
| `install.sh` | One-line installer (downloads latest release from GitHub) |
| `.github/workflows/release.yml` | CI — builds GitHub Release on `v*` tag push |
| `.claude/settings.json` | Shared Claude Code settings for this repo |
| `CHANGELOG.md` | Release notes |
| `LICENSE` | MIT license |

## Architecture

### CLI Structure

The CLI uses a subcommand architecture routed by `dispatch()`:

```
linear-claude view <url-or-id> [options]   # main command — process Linear view issues
linear-claude update                        # self-update from GitHub releases
linear-claude version                       # show version
linear-claude help                          # show help
```

Global flags `-h`/`--help` and `-v`/`--version` are handled at the dispatch level. Each subcommand has its own help screen (e.g. `linear-claude view --help`).

### Core Flow

1. **`dispatch()`** → routes to subcommands (`cmd_view`, `cmd_update`, etc.)
2. **`cmd_view()`** → parses args → checks for updates → validates requirements → fetches Linear view issues → runs the main loop
3. **`fetch_linear_view_issues()`** — calls Linear's GraphQL API via the `linear` CLI to get issues from a custom view, extracting id, title, description, branch name, state, and PR URL
4. **`main_loop_linear_view()`** — iterates over issues with status-based routing:
   - `done` / `in progress` → skipped
   - `in review` → handled by `handle_in_review_issue()` (fetches CI failures + PR review comments, runs Claude to fix them)
   - Everything else (todo, backlog, etc.) → runs `execute_single_iteration()`
5. **`execute_single_iteration()`** — creates a branch, builds an enhanced prompt with workflow context + notes from previous iterations, runs Claude Code, handles Q&A loops (up to 3 rounds), optionally runs a reviewer pass, then commits/pushes
6. **`linear_claude_commit()`** — uses Claude Code itself to generate commit messages, retries up to 3 times on failure, pushes branch, optionally creates PR via `gh`

### Update Mechanism

- **`check_for_updates()`** — called at the start of `cmd_view()`, queries the GitHub Releases API for `Blightwidow/linear-claude`, caches the result in `~/.cache/linear-claude/latest-version` for 24h, warns to stderr if a newer version exists. Uses `grep`/`cut` (not `jq`) so it works before `validate_requirements`.
- **`cmd_update()`** — downloads the latest release asset, verifies SHA256 checksum, replaces the script in-place (falls back to `sudo` if needed).
- **`version_lt()`** — compares two semver strings numerically (avoids `sort -V` for macOS compat).

### Release Pipeline

`.github/workflows/release.yml` triggers on `v*.*.*` tag push:
1. Verifies tag matches `LC_VERSION` in the script
2. Computes SHA256 of `linear_claude.sh`
3. Generates changelog from git log between tags
4. Creates a GitHub Release with `linear_claude.sh` + `linear_claude.sh.sha256` as assets

### Key Differences from Upstream (continuous-claude)

- Subcommand CLI (`linear-claude view <id>` instead of positional arg)
- Self-update mechanism via GitHub Releases (`linear-claude update`)
- First positional argument to `view` is a **Linear view URL or ID** (not `--prompt`)
- Default branch prefix: `linear-claude/` (not `continuous-claude/`)
- Default completion signal: `LINEAR_CLAUDE_PROJECT_COMPLETE`
- Issues are fetched via `linear api` (GraphQL) with `--paginate`
- Status-based routing: skips done/in-progress, handles in-review PRs
- Notes files stored in `.claude/plans/<identifier>.md` per issue
- `--open-pr` flag (PRs not created by default)
- PR comments with Claude's notes posted automatically

### Dependencies

- **Claude Code CLI** (`claude`) — the AI engine
- **GitHub CLI** (`gh`) — for PR creation, CI status, review comments
- **Linear CLI** (`linear`, via `brew install schpet/tap/linear`) — for fetching issues
- **jq** — JSON parsing throughout
- **Git** — branch management, commits

### Important Variables (LC_ prefix)

All global state uses the `LC_` prefix. Key ones:
- `LC_LINEAR_VIEW` — the Linear view URL/ID (first positional arg)
- `LC_LINEAR_ISSUES_JSON` — cached JSON array of issues from Linear
- `LC_PROMPT` — dynamically built per-issue from Linear issue title + description
- `LC_ADDITIONAL_FLAGS` — always includes `--output-format stream-json --verbose`
- `LC_EXTRA_CLAUDE_FLAGS` — unrecognized flags forwarded to `claude`
- `LC_OPEN_PR` — whether to create PRs (default: false)
- `LC_GITHUB_RELEASE_REPO` — GitHub repo for release checks (`Blightwidow/linear-claude`)
- `LC_UPDATE_CACHE_DIR` / `LC_UPDATE_CACHE_FILE` — cached latest version info (`~/.cache/linear-claude/`)

## Running

```bash
# Basic usage (from a git repo with GitHub remote)
./linear_claude.sh view "https://linear.app/team/view/abc123"

# With limits
./linear_claude.sh view abc123 -m 3 --max-cost 10.00

# Open PRs for each issue
./linear_claude.sh view abc123 --open-pr

# Testing mode (no git operations)
./linear_claude.sh view abc123 --disable-commits

# Dry run
./linear_claude.sh view abc123 --dry-run

# Self-update
./linear_claude.sh update
```

## Testing

The script supports a `TESTING` environment variable — when set, `dispatch()` is not called, allowing functions to be sourced and tested individually:

```bash
TESTING=1 source ./linear_claude.sh
# Now you can call individual functions like parse_duration, format_duration, version_lt, etc.
```

## Releasing

1. Bump `LC_VERSION` in `linear_claude.sh`
2. Commit: `git commit -m "Bump vX.Y.Z"`
3. Tag: `git tag vX.Y.Z`
4. Push: `git push && git push --tags`
5. GitHub Actions (`.github/workflows/release.yml`) automatically creates the release, then commits the updated `CHANGELOG.md` and `linear_claude.sh.sha256` back to main

## Conventions

- Error output goes to stderr (`>&2`), data output to stdout
- Emoji prefixes on all log lines for visual parsing of iteration progress
- Claude Code is invoked with `--output-format stream-json --verbose` and output is parsed with `jq` in real-time to display tool usage and text output
- Temporary files are created with `mktemp` and cleaned up via `trap ... EXIT`
- `set -e` is NOT used globally (only in `install.sh`); errors are handled explicitly via exit codes and error counters (3 consecutive errors = fatal)

## Security / Personal Data

- **Never hardcode** absolute paths (e.g. `/Users/...`, `/home/...`), API tokens, secret keys, email addresses, or personal names outside of `LICENSE`
- **Before committing**, grep the staged diff for the current user's home directory path (`$HOME`) and common PII patterns (emails, tokens, private keys)
- **Use placeholder values** in examples and docs: `user/branch-name`, `team`, `abc123` — not real usernames, org names, or IDs
- `.gitignore` already excludes local-only files (`.claude/settings.local.json`, `.claude/plans/`, `CLAUDE.local.md`) — keep sensitive or machine-specific config there
