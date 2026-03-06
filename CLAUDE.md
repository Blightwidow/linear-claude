# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**linear-claude** is a fork/adaptation of [Continuous Claude](https://github.com/AnandChowdhary/continuous-claude) that integrates with **Linear** (project management) instead of using a free-form prompt. It fetches issues from a Linear custom view and runs Claude Code iteratively on each issue, creating branches, committing, pushing, and optionally opening PRs.

The entire project is a single Bash script (`linear_claude.sh`, ~1770 lines) plus an installer (`install.sh`).

## Architecture

### Core Flow

1. **`main()`** → parses args → validates requirements → fetches Linear view issues → runs the main loop
2. **`fetch_linear_view_issues()`** — calls Linear's GraphQL API via the `linear` CLI to get issues from a custom view, extracting id, title, description, branch name, state, and PR URL
3. **`main_loop_linear_view()`** — iterates over issues with status-based routing:
   - `done` / `in progress` → skipped
   - `in review` → handled by `handle_in_review_issue()` (fetches CI failures + PR review comments, runs Claude to fix them)
   - Everything else (todo, backlog, etc.) → runs `execute_single_iteration()`
4. **`execute_single_iteration()`** — creates a branch, builds an enhanced prompt with workflow context + notes from previous iterations, runs Claude Code, handles Q&A loops (up to 3 rounds), optionally runs a reviewer pass, then commits/pushes
5. **`linear_claude_commit()`** — uses Claude Code itself to generate commit messages, retries up to 3 times on failure, pushes branch, optionally creates PR via `gh`

### Key Differences from Upstream (continuous-claude)

- First positional argument is a **Linear view URL or ID** (not `--prompt`)
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

## Running

```bash
# Basic usage (from a git repo with GitHub remote)
./linear_claude.sh "https://linear.app/team/view/abc123"

# With limits
./linear_claude.sh abc123 -m 3 --max-cost 10.00

# Open PRs for each issue
./linear_claude.sh abc123 --open-pr

# Testing mode (no git operations)
./linear_claude.sh abc123 --disable-commits

# Dry run
./linear_claude.sh abc123 --dry-run
```

## Testing

The script supports a `TESTING` environment variable — when set, `main()` is not called, allowing functions to be sourced and tested individually:

```bash
TESTING=1 source ./linear_claude.sh
# Now you can call individual functions like parse_duration, format_duration, etc.
```

## Conventions

- Error output goes to stderr (`>&2`), data output to stdout
- Emoji prefixes on all log lines for visual parsing of iteration progress
- Claude Code is invoked with `--output-format stream-json --verbose` and output is parsed with `jq` in real-time to display tool usage and text output
- Temporary files are created with `mktemp` and cleaned up via `trap ... EXIT`
- The script uses `set -e` is NOT set globally (only in `install.sh`); errors are handled explicitly via exit codes and error counters (3 consecutive errors = fatal)
