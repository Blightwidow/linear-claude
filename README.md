# Linear Claude

Run Claude Code iteratively on [Linear](https://linear.app) issues — derived from [Continuous Claude](https://github.com/AnandChowdhary/continuous-claude) by Anand Chowdhary.

Instead of a free-form prompt, Linear Claude fetches issues from a Linear custom view and runs Claude Code on each one, creating branches, committing changes, pushing, and optionally opening PRs. Issues are routed by status: "todo"/"backlog" issues get implemented, "in review" issues get their CI failures and PR review comments addressed, and "done"/"in progress" issues are skipped.

## ⚙️ How it works

* Fetches issues from a Linear custom view via the Linear CLI
* For each issue, Claude Code runs with the issue title and description as the prompt
* Changes are committed to a per-issue branch and pushed
* Optionally creates a pull request with `--open-pr`
* Per-issue notes are stored in `.claude/plans/<identifier>.md` to maintain context across iterations
* Issues "in review" are handled specially: CI failures and PR review comments are fetched and passed to Claude for resolution
* If multiple agents signal project completion, the loop stops early

## 🚀 Quick start

### Installation

Install with a single command:

```bash
curl -fsSL https://raw.githubusercontent.com/Blightwidow/linear-claude/main/install.sh | bash
```

This will install `linear-claude` to `~/.local/bin` and check for required dependencies.

If you prefer to install manually:

```bash
curl -fsSL https://raw.githubusercontent.com/Blightwidow/linear-claude/main/linear_claude.sh -o linear-claude
chmod +x linear-claude
sudo mv linear-claude /usr/local/bin/
```

### Uninstall

```bash
rm ~/.local/bin/linear-claude
# or if you installed to /usr/local/bin:
sudo rm /usr/local/bin/linear-claude
```

### Prerequisites

1. **[Claude Code CLI](https://claude.ai/code)** — Authenticate with `claude auth`
2. **[GitHub CLI](https://cli.github.com)** — Authenticate with `gh auth login`
3. **[Linear CLI](https://github.com/schpet/linear)** — Install with `brew install schpet/tap/linear` and authenticate via `linear auth login`
4. **jq** — Install with `brew install jq` (macOS) or `apt-get install jq` (Linux)

### Usage

```bash
# Run one iteration per issue from a Linear view (owner and repo auto-detected from git remote)
linear-claude "https://linear.app/team/view/abc123"

# Or use just the view ID
linear-claude abc123

# Limit processing to 3 issues and $10
linear-claude abc123 -m 3 --max-cost 10.00

# Run for a maximum duration
linear-claude abc123 --max-duration 2h

# Open PRs for each issue
linear-claude abc123 --open-pr

# Run without commits (testing mode)
linear-claude abc123 --disable-commits

# Dry run
linear-claude abc123 --dry-run
```

## 🎯 Flags

* `-m, --max-runs <number>`: Maximum number of successful iterations (use `0` for unlimited with `--max-cost` or `--max-duration`)
* `--max-cost <dollars>`: Maximum USD to spend
* `--max-duration <duration>`: Maximum duration to run (e.g., `2h`, `30m`, `1h30m`)
* `--owner <owner>`: GitHub repository owner (auto-detected from git remote if not provided)
* `--repo <repo>`: GitHub repository name (auto-detected from git remote if not provided)
* `--git-branch-prefix <prefix>`: Prefix for git branch names (default: `linear-claude/`)
* `--notes-file <file>`: Shared notes file for iteration context (default: `SHARED_TASK_NOTES.md`)
* `--disable-commits`: Disable automatic commits and PR creation
* `--disable-branches`: Commit on current branch without creating branches or PRs
* `--open-pr`: Create a PR after pushing (default: no PR created)
* `--dry-run`: Simulate execution without making changes
* `--completion-signal <phrase>`: Phrase that agents output when project is complete (default: `LINEAR_CLAUDE_PROJECT_COMPLETE`)
* `--completion-threshold <num>`: Number of consecutive completion signals required to stop early (default: `3`)
* `-r, --review-prompt <text>`: Run a reviewer pass after each iteration to validate changes

Any additional flags not recognized by `linear-claude` are forwarded to the underlying `claude` command (e.g., `--allowedTools`, `--model`).

## 📊 Example output

```
🔍 Fetching issues from Linear view: abc123
✅ Found 4 issues in Linear view

📋 Processing issue 1/4: PROJ-42 — Add user authentication (state: Todo)
🔄 Resetting to origin/main...
🔄 (1) Starting iteration...
🌿 (1) Creating/checking out branch: linear-claude/iteration-1/2025-11-15-be939873
🤖 (1) Running Claude Code...
   (1) 📖 src/auth.ts
   (1) ✏️ src/auth.ts
   (1) 💻 npm test
💰 (1) Iteration cost: $0.042
✅ (1) Work completed
💬 (1) Committing changes...
📤 (1) Pushing branch...
✅ (1) Pushed branch: linear-claude/iteration-1/2025-11-15-be939873

📋 Processing issue 2/4: PROJ-43 — Fix login redirect (state: In Review)
🔍 (2) Handling review for PROJ-43 (theo/fix-login-redirect)...
📋 (2) Found PR #12 from Linear for branch theo/fix-login-redirect
🔄 (2) Checking CI status for PR #12...
🤖 (2) Running Claude Code to resolve review comments...
✅ (2) Review comments addressed for PROJ-43

📋 Processing issue 3/4: PROJ-44 — Refactor database layer (state: In Progress)
⏭️  Skipping PROJ-44 — status is 'In Progress'

🎉 Done with total cost: $0.089
```

## 🔗 Attribution

This project is derived from [Continuous Claude](https://github.com/AnandChowdhary/continuous-claude) by [Anand Chowdhary](https://anandchowdhary.com). The core loop-and-commit architecture comes from that project; Linear Claude adapts it to be driven by Linear issues instead of a free-form prompt.

## 📃 License

[MIT](./LICENSE)
