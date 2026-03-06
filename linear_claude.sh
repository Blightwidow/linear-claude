#!/bin/bash

LC_VERSION="v0.1.0"

LC_ADDITIONAL_FLAGS="--output-format stream-json --verbose"

LC_NOTES_FILE="SHARED_TASK_NOTES.md"

LC_PROMPT_JQ_INSTALL="Please install jq for JSON parsing"

LC_PROMPT_COMMIT_MESSAGE="Please review all uncommitted changes in the git repository (both modified and new files). Write a commit message with: (1) a short one-line summary, (2) two newlines, (3) then a detailed explanation. Do not include any footers or metadata like 'Generated with Claude Code' or 'Co-Authored-By'. Feel free to look at the last few commits to get a sense of the commit message style for consistency. First run 'git add .' to stage all changes including new untracked files, then commit using 'git commit -m \"your message\"' (don't push, just commit, no need to ask for confirmation)."

LC_PROMPT_WORKFLOW_CONTEXT="## CONTINUOUS WORKFLOW CONTEXT

This is part of a continuous development loop where work happens incrementally across multiple iterations. You might run once, then a human developer might make changes, then you run again, and so on. This could happen daily or on any schedule.

**Important**: You don't need to complete the entire goal in one iteration. Just make meaningful progress on one thing, then leave clear notes for the next iteration (human or AI). Think of it as a relay race where you're passing the baton.

**Do NOT commit or push changes** - The automation will handle committing and pushing your changes after you finish. Just focus on making the code changes.

**Project Completion Signal**: If you determine that not just your current task but the ENTIRE project goal is fully complete (nothing more to be done on the overall goal), only include the exact phrase \"COMPLETION_SIGNAL_PLACEHOLDER\" in your response. Only use this when absolutely certain that the whole project is finished, not just your individual task. We will stop working on this project when multiple developers independently determine that the project is complete.

## PRIMARY GOAL"

LC_PROMPT_NOTES_UPDATE_EXISTING="Update the \`$LC_NOTES_FILE\` file with relevant context for the next iteration. Add new notes and remove outdated information to keep it current and useful."

LC_PROMPT_NOTES_CREATE_NEW="Create a \`$LC_NOTES_FILE\` file with relevant context and instructions for the next iteration."

LC_PROMPT_NOTES_GUIDELINES="

This file helps coordinate work across iterations (both human and AI developers). It should:

- Contain relevant context and instructions for the next iteration
- Stay concise and actionable (like a notes file, not a detailed report)
- Help the next developer understand what to do next

The file should NOT include:
- Lists of completed work or full reports
- Information that can be discovered by running tests/coverage
- Unnecessary details"

LC_PROMPT_REVIEWER_CONTEXT="## CODE REVIEW CONTEXT

You are performing a review pass on changes just made by another developer. This is NOT a new feature implementation - you are reviewing and validating existing changes using the instructions given below by the user. Feel free to use git commands to see what changes were made if it's helpful to you.

**Do NOT commit or push changes** - The automation will handle committing and pushing your changes after you finish. Just focus on validating and fixing any issues."

LC_PROMPT=""
LC_MAX_RUNS=""
LC_MAX_COST=""
LC_MAX_DURATION=""
LC_ENABLE_COMMITS=true
LC_DISABLE_BRANCHES=false
LC_GIT_BRANCH_PREFIX="linear-claude/"
LC_GITHUB_OWNER=""
LC_GITHUB_REPO=""
LC_DRY_RUN=false
LC_COMPLETION_SIGNAL="LINEAR_CLAUDE_PROJECT_COMPLETE"
LC_COMPLETION_THRESHOLD=3
LC_ERROR_LOG=""
error_count=0
extra_iterations=0
successful_iterations=0
total_cost=0
completion_signal_count=0
i=1
LC_EXTRA_CLAUDE_FLAGS=()
LC_REVIEW_PROMPT=""
start_time=""
LC_LINEAR_VIEW=""
LC_LINEAR_ISSUES_JSON=""
LC_OPEN_PR=false
LC_GITHUB_RELEASE_REPO="Blightwidow/linear-claude"
LC_UPDATE_CACHE_DIR="$HOME/.cache/linear-claude"
LC_UPDATE_CACHE_FILE="$HOME/.cache/linear-claude/latest-version"
LC_UPDATE_CACHE_MAX_AGE=86400  # 24 hours in seconds

parse_duration() {
    local duration_str="$1"
    duration_str=$(echo "$duration_str" | tr -d '[:space:]')

    if [ -z "$duration_str" ]; then
        return 1
    fi

    local total_seconds=0
    local remaining="$duration_str"

    if [[ "$remaining" =~ ([0-9]+)[hH] ]]; then
        local hours="${BASH_REMATCH[1]}"
        total_seconds=$((total_seconds + hours * 3600))
        remaining="${remaining/${BASH_REMATCH[0]}/}"
    fi

    if [[ "$remaining" =~ ([0-9]+)[mM] ]]; then
        local minutes="${BASH_REMATCH[1]}"
        total_seconds=$((total_seconds + minutes * 60))
        remaining="${remaining/${BASH_REMATCH[0]}/}"
    fi

    if [[ "$remaining" =~ ([0-9]+)[sS] ]]; then
        local seconds="${BASH_REMATCH[1]}"
        total_seconds=$((total_seconds + seconds))
        remaining="${remaining/${BASH_REMATCH[0]}/}"
    fi

    if [ -n "$remaining" ]; then
        return 1
    fi

    if [ $total_seconds -eq 0 ]; then
        return 1
    fi

    echo "$total_seconds"
    return 0
}

format_duration() {
    local seconds="$1"

    if [ -z "$seconds" ] || [ "$seconds" -eq 0 ]; then
        echo "0s"
        return
    fi

    local hours=$((seconds / 3600))
    local minutes=$(((seconds % 3600) / 60))
    local secs=$((seconds % 60))

    local result=""
    if [ $hours -gt 0 ]; then
        result="${hours}h"
    fi
    if [ $minutes -gt 0 ]; then
        result="${result}${minutes}m"
    fi
    if [ $secs -gt 0 ] || [ -z "$result" ]; then
        result="${result}${secs}s"
    fi

    echo "$result"
}

version_lt() {
    local a="$1" b="$2"
    # Strip leading 'v' if present
    a="${a#v}"
    b="${b#v}"

    local IFS='.'
    local -a a_parts=($a) b_parts=($b)

    local max=${#a_parts[@]}
    if [ ${#b_parts[@]} -gt $max ]; then
        max=${#b_parts[@]}
    fi

    local i=0
    while [ $i -lt $max ]; do
        local a_num=${a_parts[$i]:-0}
        local b_num=${b_parts[$i]:-0}
        if [ "$a_num" -lt "$b_num" ] 2>/dev/null; then
            return 0  # a < b
        elif [ "$a_num" -gt "$b_num" ] 2>/dev/null; then
            return 1  # a > b
        fi
        i=$((i + 1))
    done

    return 1  # equal, not less than
}

compute_sha256() {
    local file="$1"
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | cut -d' ' -f1
    elif command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | cut -d' ' -f1
    else
        echo ""
        return 1
    fi
}

check_for_updates() {
    # Check if cache exists and is fresh enough
    if [ -f "$LC_UPDATE_CACHE_FILE" ]; then
        local cache_time
        cache_time=$(head -n 1 "$LC_UPDATE_CACHE_FILE" 2>/dev/null || echo "0")
        local now
        now=$(date +%s)
        local age=$((now - cache_time))
        if [ "$age" -lt "$LC_UPDATE_CACHE_MAX_AGE" ]; then
            # Cache is fresh, read cached version
            local cached_version
            cached_version=$(sed -n '2p' "$LC_UPDATE_CACHE_FILE" 2>/dev/null || echo "")
            if [ -n "$cached_version" ] && version_lt "$LC_VERSION" "$cached_version"; then
                echo "⚠️  A newer version of linear-claude is available: $cached_version (current: $LC_VERSION)" >&2
                echo "   Run 'linear-claude update' to upgrade." >&2
                echo "" >&2
            fi
            return 0
        fi
    fi

    # Fetch latest release from GitHub API (3s timeout, no jq dependency)
    local response
    response=$(curl -fsSL --connect-timeout 3 --max-time 5 \
        "https://api.github.com/repos/$LC_GITHUB_RELEASE_REPO/releases/latest" 2>/dev/null) || return 0

    local latest_tag
    latest_tag=$(echo "$response" | grep '"tag_name"' | head -n 1 | cut -d'"' -f4)

    if [ -z "$latest_tag" ]; then
        return 0
    fi

    # Update cache
    mkdir -p "$LC_UPDATE_CACHE_DIR" 2>/dev/null || return 0
    printf '%s\n%s\n' "$(date +%s)" "$latest_tag" > "$LC_UPDATE_CACHE_FILE" 2>/dev/null || true

    if version_lt "$LC_VERSION" "$latest_tag"; then
        echo "⚠️  A newer version of linear-claude is available: $latest_tag (current: $LC_VERSION)" >&2
        echo "   Run 'linear-claude update' to upgrade." >&2
        echo "" >&2
    fi
}

show_update_help() {
    cat << 'EOF'
Linear Claude — Update

USAGE:
    linear-claude update [options]

DESCRIPTION:
    Downloads and installs the latest version of linear-claude from GitHub releases.
    Verifies the download integrity via SHA256 checksum.

OPTIONS:
    -h, --help    Show this help message
EOF
}

cmd_update() {
    # Parse update-specific flags
    while [[ $# -gt 0 ]]; do
        case $1 in
            -h|--help)
                show_update_help
                return 0
                ;;
            *)
                echo "❌ Unknown option for update: $1" >&2
                echo "Run 'linear-claude update --help' for usage." >&2
                return 1
                ;;
        esac
    done

    echo "🔄 Checking for updates..." >&2

    # Fetch latest release info
    local response
    response=$(curl -fsSL --connect-timeout 10 --max-time 30 \
        "https://api.github.com/repos/$LC_GITHUB_RELEASE_REPO/releases/latest" 2>/dev/null)

    if [ -z "$response" ]; then
        echo "❌ Failed to fetch release information from GitHub." >&2
        echo "   Check your internet connection or try again later." >&2
        return 1
    fi

    local latest_tag
    latest_tag=$(echo "$response" | grep '"tag_name"' | head -n 1 | cut -d'"' -f4)

    if [ -z "$latest_tag" ]; then
        echo "❌ No releases found for $LC_GITHUB_RELEASE_REPO." >&2
        return 1
    fi

    if ! version_lt "$LC_VERSION" "$latest_tag"; then
        echo "✅ Already up to date (version $LC_VERSION)." >&2
        return 0
    fi

    echo "📦 Updating from $LC_VERSION to $latest_tag..." >&2

    # Determine download URLs from release assets
    local script_url
    script_url=$(echo "$response" | grep '"browser_download_url"' | grep 'linear_claude\.sh"' | head -n 1 | cut -d'"' -f4)
    local checksum_url
    checksum_url=$(echo "$response" | grep '"browser_download_url"' | grep 'linear_claude\.sh\.sha256"' | head -n 1 | cut -d'"' -f4)

    if [ -z "$script_url" ]; then
        echo "❌ Could not find linear_claude.sh in release assets." >&2
        return 1
    fi

    # Download to temp files
    local tmp_script
    tmp_script=$(mktemp)
    local tmp_checksum
    tmp_checksum=$(mktemp)
    trap "rm -f '$tmp_script' '$tmp_checksum'" RETURN

    echo "📥 Downloading linear_claude.sh..." >&2
    if ! curl -fsSL --connect-timeout 10 --max-time 60 "$script_url" -o "$tmp_script"; then
        echo "❌ Failed to download linear_claude.sh" >&2
        return 1
    fi

    # Verify checksum if available
    if [ -n "$checksum_url" ]; then
        echo "🔐 Verifying checksum..." >&2
        if ! curl -fsSL --connect-timeout 10 --max-time 30 "$checksum_url" -o "$tmp_checksum"; then
            echo "❌ Failed to download checksum file" >&2
            return 1
        fi

        local expected_sha
        expected_sha=$(cut -d' ' -f1 < "$tmp_checksum")
        local actual_sha
        actual_sha=$(compute_sha256 "$tmp_script")

        if [ -z "$actual_sha" ]; then
            echo "❌ Could not compute SHA256 — neither shasum nor sha256sum found." >&2
            return 1
        fi

        if [ "$expected_sha" != "$actual_sha" ]; then
            echo "❌ Checksum verification failed!" >&2
            echo "   Expected: $expected_sha" >&2
            echo "   Got:      $actual_sha" >&2
            return 1
        fi

        echo "✅ Checksum verified." >&2
    else
        echo "⚠️  No checksum file in release assets, skipping verification." >&2
    fi

    # Determine install path
    local install_path
    install_path=$(realpath "${BASH_SOURCE[0]}" 2>/dev/null || command -v linear-claude 2>/dev/null || echo "")

    if [ -z "$install_path" ]; then
        echo "❌ Could not determine install location." >&2
        echo "   Download manually from: https://github.com/$LC_GITHUB_RELEASE_REPO/releases/latest" >&2
        return 1
    fi

    echo "📂 Installing to: $install_path" >&2

    # Replace the script
    chmod +x "$tmp_script"
    if cp "$tmp_script" "$install_path" 2>/dev/null; then
        : # success
    elif command -v sudo >/dev/null 2>&1; then
        echo "🔑 Requires elevated permissions, using sudo..." >&2
        if ! sudo cp "$tmp_script" "$install_path"; then
            echo "❌ Failed to install update." >&2
            return 1
        fi
    else
        echo "❌ Cannot write to $install_path and sudo is not available." >&2
        return 1
    fi

    # Update the version cache
    mkdir -p "$LC_UPDATE_CACHE_DIR" 2>/dev/null || true
    printf '%s\n%s\n' "$(date +%s)" "$latest_tag" > "$LC_UPDATE_CACHE_FILE" 2>/dev/null || true

    echo "✅ Updated to $latest_tag successfully!" >&2
    return 0
}

show_help() {
    cat << EOF
Linear Claude - Run Claude Code iteratively on Linear issues

USAGE:
    linear-claude <command> [options]

COMMANDS:
    view <url-or-id>    Process issues from a Linear custom view
    update              Update linear-claude to the latest version
    version             Show version information
    help                Show this help message

GLOBAL OPTIONS:
    -h, --help          Show this help message
    -v, --version       Show version information

EXAMPLES:
    linear-claude view "https://linear.app/team/view/abc123"
    linear-claude view abc123 -m 3 --max-cost 10.00
    linear-claude update
    linear-claude version

Run 'linear-claude <command> --help' for more information on a specific command.
EOF
}

show_view_help() {
    cat << EOF
Linear Claude — View

USAGE:
    linear-claude view <linear-view-url-or-id> [options]

ARGUMENTS:
    <linear-view-url-or-id>       Linear view URL or ID (required)

OPTIONS:
    -h, --help                    Show this help message
    -m, --max-runs <number>       Maximum number of successful iterations (use 0 for unlimited with --max-cost or --max-duration)
    --max-cost <dollars>          Maximum cost in USD to spend
    --max-duration <duration>     Maximum duration to run (e.g., "2h", "30m", "1h30m")
    --owner <owner>               GitHub repository owner (auto-detected from git remote if not provided)
    --repo <repo>                 GitHub repository name (auto-detected from git remote if not provided)
    --disable-commits             Disable automatic commits and PR creation
    --disable-branches            Commit on current branch without creating branches or PRs
    --git-branch-prefix <prefix>  Branch prefix for iterations (default: "linear-claude/")
    --notes-file <file>           Shared notes file for iteration context (default: "SHARED_TASK_NOTES.md")
    --dry-run                     Simulate execution without making changes
    --completion-signal <phrase>  Phrase that agents output when project is complete (default: "LINEAR_CLAUDE_PROJECT_COMPLETE")
    --completion-threshold <num>  Number of consecutive signals to stop early (default: 3)
    -r, --review-prompt <text>    Run a reviewer pass after each iteration to validate changes
    --open-pr                     Create a PR after pushing (default: no PR created)

EXAMPLES:
    # Run one iteration per issue from a Linear view
    linear-claude view "https://linear.app/alan/view/abc123"

    # Limit processing to 3 issues and \$10
    linear-claude view abc123 -m 3 --max-cost 10.00

    # Run for a maximum duration
    linear-claude view abc123 --max-duration 2h

    # Open PRs for each issue
    linear-claude view abc123 --open-pr

    # Run without commits (testing mode)
    linear-claude view abc123 --disable-commits

REQUIREMENTS:
    - Claude Code CLI (https://claude.ai/code)
    - GitHub CLI (gh) - authenticated with 'gh auth login'
    - Linear CLI (brew install schpet/tap/linear)
    - jq - JSON parsing utility
    - Git repository (unless --disable-commits is used)
EOF
}

show_version() {
    echo "linear-claude version $LC_VERSION"
}

detect_github_repo() {
    if ! git rev-parse --git-dir > /dev/null 2>&1; then
        return 1
    fi

    local remote_url
    if ! remote_url=$(git remote get-url origin 2>/dev/null); then
        return 1
    fi

    local owner=""
    local repo=""

    if [[ "$remote_url" =~ ^https://github\.com/([^/]+)/([^/]+)$ ]]; then
        owner="${BASH_REMATCH[1]}"
        repo="${BASH_REMATCH[2]}"
    elif [[ "$remote_url" =~ ^git@github\.com:([^/]+)/([^/]+)$ ]]; then
        owner="${BASH_REMATCH[1]}"
        repo="${BASH_REMATCH[2]}"
    else
        return 1
    fi

    repo="${repo%.git}"

    if [ -z "$owner" ] || [ -z "$repo" ]; then
        return 1
    fi

    echo "$owner $repo"
    return 0
}

parse_arguments() {
    while [[ $# -gt 0 ]]; do
        case $1 in
            -h|--help)
                show_view_help
                exit 0
                ;;
            -m|--max-runs)
                LC_MAX_RUNS="$2"
                shift 2
                ;;
            --max-cost)
                LC_MAX_COST="$2"
                shift 2
                ;;
            --max-duration)
                LC_MAX_DURATION="$2"
                shift 2
                ;;
            --git-branch-prefix)
                LC_GIT_BRANCH_PREFIX="$2"
                shift 2
                ;;
            --owner)
                LC_GITHUB_OWNER="$2"
                shift 2
                ;;
            --repo)
                LC_GITHUB_REPO="$2"
                shift 2
                ;;
            --disable-commits)
                LC_ENABLE_COMMITS=false
                shift
                ;;
            --disable-branches)
                LC_DISABLE_BRANCHES=true
                shift
                ;;
            --notes-file)
                LC_NOTES_FILE="$2"
                shift 2
                ;;
            --dry-run)
                LC_DRY_RUN=true
                shift
                ;;
            --completion-signal)
                LC_COMPLETION_SIGNAL="$2"
                shift 2
                ;;
            --completion-threshold)
                LC_COMPLETION_THRESHOLD="$2"
                shift 2
                ;;
            -r|--review-prompt)
                LC_REVIEW_PROMPT="$2"
                shift 2
                ;;
            --open-pr)
                LC_OPEN_PR=true
                shift
                ;;
            -*)
                LC_EXTRA_CLAUDE_FLAGS+=("$1")
                shift
                ;;
            *)
                if [ -z "$LC_LINEAR_VIEW" ]; then
                    LC_LINEAR_VIEW="$1"
                else
                    LC_EXTRA_CLAUDE_FLAGS+=("$1")
                fi
                shift
                ;;
        esac
    done
}

validate_arguments() {
    if [ -z "$LC_LINEAR_VIEW" ]; then
        echo "❌ Error: Linear view URL or ID is required as the first argument." >&2
        echo "Usage: linear-claude <linear-view-url-or-id> [options]" >&2
        echo "Run '$0 --help' for usage information." >&2
        exit 1
    fi

    if [ -n "$LC_MAX_RUNS" ] && ! [[ "$LC_MAX_RUNS" =~ ^[0-9]+$ ]]; then
        echo "❌ Error: --max-runs must be a non-negative integer" >&2
        exit 1
    fi

    if [ -n "$LC_MAX_COST" ]; then
        if ! [[ "$LC_MAX_COST" =~ ^[0-9]+\.?[0-9]*$ ]] || [ "$(awk "BEGIN {print ($LC_MAX_COST <= 0)}")" = "1" ]; then
            echo "❌ Error: --max-cost must be a positive number" >&2
            exit 1
        fi
    fi

    if [ -n "$LC_MAX_DURATION" ]; then
        local duration_seconds
        if ! duration_seconds=$(parse_duration "$LC_MAX_DURATION"); then
            echo "❌ Error: --max-duration must be a valid duration (e.g., '2h', '30m', '1h30m', '90s')" >&2
            exit 1
        fi
        LC_MAX_DURATION="$duration_seconds"
    fi

    if [ -n "$LC_COMPLETION_THRESHOLD" ]; then
        if ! [[ "$LC_COMPLETION_THRESHOLD" =~ ^[0-9]+$ ]] || [ "$LC_COMPLETION_THRESHOLD" -lt 1 ]; then
            echo "❌ Error: --completion-threshold must be a positive integer" >&2
            exit 1
        fi
    fi

    # Only require GitHub info if commits are enabled
    if [ "$LC_ENABLE_COMMITS" = "true" ]; then
        if [ -z "$LC_GITHUB_OWNER" ] || [ -z "$LC_GITHUB_REPO" ]; then
            local detected_info
            if detected_info=$(detect_github_repo); then
                local detected_owner=$(echo "$detected_info" | awk '{print $1}')
                local detected_repo=$(echo "$detected_info" | awk '{print $2}')

                if [ -z "$LC_GITHUB_OWNER" ]; then
                    LC_GITHUB_OWNER="$detected_owner"
                fi
                if [ -z "$LC_GITHUB_REPO" ]; then
                    LC_GITHUB_REPO="$detected_repo"
                fi
            fi
        fi

        if [ -z "$LC_GITHUB_OWNER" ]; then
            echo "❌ Error: GitHub owner is required. Use --owner to provide the owner, or run from a git repository with a GitHub remote." >&2
            echo "Run '$0 --help' for usage information." >&2
            exit 1
        fi

        if [ -z "$LC_GITHUB_REPO" ]; then
            echo "❌ Error: GitHub repo is required. Use --repo to provide the repo, or run from a git repository with a GitHub remote." >&2
            echo "Run '$0 --help' for usage information." >&2
            exit 1
        fi
    fi
}

validate_requirements() {
    if ! command -v claude &> /dev/null; then
        echo "❌ Error: Claude Code is not installed: https://claude.ai/code" >&2
        exit 1
    fi

    if ! command -v jq &> /dev/null; then
        echo "⚠️ jq is required for JSON parsing but is not installed. Asking Claude Code to install it..." >&2
        claude -p "$LC_PROMPT_JQ_INSTALL" --allowedTools "Bash,Read"
        if ! command -v jq &> /dev/null; then
            echo "❌ Error: jq is still not installed after Claude Code attempt." >&2
            exit 1
        fi
    fi

    if ! command -v linear &> /dev/null; then
        echo "❌ Error: Linear CLI is not installed. Install with: brew install schpet/tap/linear" >&2
        exit 1
    fi

    # Only check for GitHub CLI if commits are enabled
    if [ "$LC_ENABLE_COMMITS" = "true" ]; then
        if ! command -v gh &> /dev/null; then
            echo "❌ Error: GitHub CLI (gh) is not installed: https://cli.github.com" >&2
            exit 1
        fi

        if ! gh auth status >/dev/null 2>&1; then
            echo "❌ Error: GitHub CLI is not authenticated. Run 'gh auth login' first." >&2
            exit 1
        fi
    fi
}

create_iteration_branch() {
    local iteration_display="$1"
    local iteration_num="$2"
    local override_branch="$3"

    if ! git rev-parse --git-dir > /dev/null 2>&1; then
        echo ""
        return 0
    fi

    local current_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "main")

    if [[ "$current_branch" == ${LC_GIT_BRANCH_PREFIX}* ]]; then
        echo "⚠️  $iteration_display Already on iteration branch: $current_branch" >&2
        git checkout main >/dev/null 2>&1 || return 1
        current_branch="main"
    fi

    local branch_name=""
    if [ -n "$override_branch" ]; then
        branch_name="$override_branch"
    else
        local date_str=$(date +%Y-%m-%d)
        local random_hash
        if command -v openssl >/dev/null 2>&1; then
            random_hash=$(openssl rand -hex 4)
        elif [ -r /dev/urandom ]; then
            random_hash=$(LC_ALL=C tr -dc 'a-f0-9' < /dev/urandom | head -c 8)
        else
            random_hash=$(printf "%x" $(($(date +%s) % 100000000)))$(printf "%x" $$)
            random_hash=${random_hash:0:8}
        fi
        branch_name="${LC_GIT_BRANCH_PREFIX}iteration-${iteration_num}/${date_str}-${random_hash}"
    fi

    echo "🌿 $iteration_display Creating/checking out branch: $branch_name" >&2

    if [ "$LC_DRY_RUN" = "true" ]; then
        echo "   (DRY RUN) Would create branch $branch_name" >&2
        echo "$branch_name"
        return 0
    fi

    # Check if branch exists remotely
    if git ls-remote --heads origin "$branch_name" 2>/dev/null | grep -q "$branch_name"; then
        echo "🌿 $iteration_display Branch exists remotely, fetching and checking out..." >&2
        git fetch origin "$branch_name" >/dev/null 2>&1
        if ! git checkout "$branch_name" >/dev/null 2>&1; then
            if ! git checkout -b "$branch_name" "origin/$branch_name" >/dev/null 2>&1; then
                echo "⚠️  $iteration_display Failed to checkout existing remote branch" >&2
                echo ""
                return 1
            fi
        fi
    elif git show-ref --verify --quiet "refs/heads/$branch_name" 2>/dev/null; then
        echo "🌿 $iteration_display Branch exists locally, checking out..." >&2
        if ! git checkout "$branch_name" >/dev/null 2>&1; then
            echo "⚠️  $iteration_display Failed to checkout existing local branch" >&2
            echo ""
            return 1
        fi
    else
        if ! git checkout -b "$branch_name" >/dev/null 2>&1; then
            echo "⚠️  $iteration_display Failed to create branch" >&2
            echo ""
            return 1
        fi
    fi

    echo "$branch_name"
    return 0
}

linear_claude_commit() {
    local iteration_display="$1"
    local branch_name="$2"
    local main_branch="$3"
    local notes_file="$4"

    if ! git rev-parse --git-dir > /dev/null 2>&1; then
        return 0
    fi

    local has_changes=false
    if ! git diff --quiet --ignore-submodules=dirty || ! git diff --cached --quiet --ignore-submodules=dirty; then
        has_changes=true
    fi

    if [ -z "$(git ls-files --others --exclude-standard)" ]; then
        : # no untracked files
    else
        has_changes=true
    fi

    if [ "$has_changes" = "false" ]; then
        echo "🫙 $iteration_display No changes detected, cleaning up branch..." >&2
        git checkout "$main_branch" >/dev/null 2>&1
        git branch -D "$branch_name" >/dev/null 2>&1 || true
        return 0
    fi

    if [ "$LC_DRY_RUN" = "true" ]; then
        echo "💬 $iteration_display (DRY RUN) Would commit changes..." >&2
        echo "📦 $iteration_display (DRY RUN) Changes committed on branch: $branch_name" >&2
        echo "📤 $iteration_display (DRY RUN) Would push branch..." >&2
        if [ "$LC_OPEN_PR" = "true" ]; then
            echo "🔨 $iteration_display (DRY RUN) Would create pull request..." >&2
        fi
        return 0
    fi

    local commit_attempts=0
    local max_commit_attempts=3

    while [ $commit_attempts -lt $max_commit_attempts ]; do
        commit_attempts=$((commit_attempts + 1))

        if [ $commit_attempts -eq 1 ]; then
            echo "💬 $iteration_display Committing changes..." >&2
        else
            echo "🔄 $iteration_display Commit retry $commit_attempts/$max_commit_attempts..." >&2
        fi

        local commit_output
        commit_output=$(claude -p "$LC_PROMPT_COMMIT_MESSAGE" --allowedTools "Bash(git)" 2>&1)
        local commit_exit=$?

        if [ $commit_exit -ne 0 ]; then
            echo "⚠️  $iteration_display Commit command failed (attempt $commit_attempts/$max_commit_attempts)" >&2
            if [ $commit_attempts -ge $max_commit_attempts ]; then
                echo "❌ $iteration_display Failed to commit after $max_commit_attempts attempts" >&2
                git checkout "$main_branch" >/dev/null 2>&1
                return 1
            fi
            local fix_prompt="The previous commit attempt failed with the following output:\n\n$commit_output\n\nPlease fix the issues (e.g., linting errors, pre-commit hook failures) and then stage and commit all changes using 'git add . && git commit -m \"your message\"'. Do not push."
            echo "🔧 $iteration_display Asking Claude to fix commit issues..." >&2
            claude -p "$fix_prompt" --allowedTools "Bash,Read,Edit,Write,Grep,Glob" >/dev/null 2>&1 || true
            continue
        fi

        # Check if changes are still present after commit
        if ! git diff --quiet --ignore-submodules=dirty || ! git diff --cached --quiet --ignore-submodules=dirty || [ -n "$(git ls-files --others --exclude-standard)" ]; then
            echo "⚠️  $iteration_display Changes still present after commit (attempt $commit_attempts/$max_commit_attempts)" >&2
            if [ $commit_attempts -ge $max_commit_attempts ]; then
                echo "❌ $iteration_display Uncommitted changes remain after $max_commit_attempts attempts" >&2
                git checkout "$main_branch" >/dev/null 2>&1
                return 1
            fi
            local remaining_files
            remaining_files=$(git diff --name-only --ignore-submodules=dirty 2>/dev/null; git diff --cached --name-only --ignore-submodules=dirty 2>/dev/null; git ls-files --others --exclude-standard 2>/dev/null)
            local fix_prompt="The previous commit did not include all changes. These files still have uncommitted changes:\n\n$remaining_files\n\nPlease stage ALL changes (including untracked files) and commit them. Use 'git add . && git commit -m \"your message\"' or amend the previous commit with 'git add . && git commit --amend --no-edit'. Do not push."
            echo "🔧 $iteration_display Asking Claude to commit remaining files..." >&2
            claude -p "$fix_prompt" --allowedTools "Bash(git)" >/dev/null 2>&1 || true
            continue
        fi

        break
    done

    echo "📦 $iteration_display Changes committed on branch: $branch_name" >&2

    local commit_message=$(git log -1 --format="%B" "$branch_name")
    local commit_title=$(echo "$commit_message" | head -n 1)
    local commit_body=$(echo "$commit_message" | tail -n +4)

    echo "📤 $iteration_display Pushing branch..." >&2
    if ! git push -u origin "$branch_name" >/dev/null 2>&1; then
        echo "⚠️  $iteration_display Failed to push branch" >&2
        git checkout "$main_branch" >/dev/null 2>&1
        return 1
    fi

    echo "✅ $iteration_display Pushed branch: $branch_name" >&2

    if [ "$LC_OPEN_PR" = "true" ]; then
        echo "🔨 $iteration_display Creating pull request..." >&2
        local pr_output
        if ! pr_output=$(gh pr create --repo "$LC_GITHUB_OWNER/$LC_GITHUB_REPO" --title "$commit_title" --body "$commit_body" --base "$main_branch" 2>&1); then
            echo "⚠️  $iteration_display Failed to create PR: $pr_output" >&2
            # Not fatal — branch was pushed successfully
        else
            local pr_number=$(echo "$pr_output" | grep -oE '(pull/|#)[0-9]+' | grep -oE '[0-9]+' | head -n 1)
            echo "✅ $iteration_display PR #$pr_number created: $commit_title" >&2

            # Post notes as a PR comment
            if [ -n "$pr_number" ] && [ -n "$notes_file" ] && [ -f "$notes_file" ]; then
                local notes_content
                notes_content=$(cat "$notes_file")
                if [ -n "$notes_content" ]; then
                    local comment_body="## Claude's Notes

$notes_content"
                    if gh pr comment "$pr_number" --repo "$LC_GITHUB_OWNER/$LC_GITHUB_REPO" --body "$comment_body" >/dev/null 2>&1; then
                        echo "💬 $iteration_display Posted notes as PR comment" >&2
                    else
                        echo "⚠️  $iteration_display Failed to post notes as PR comment" >&2
                    fi
                fi
            fi
        fi
    fi

    # Return to main branch
    if ! git checkout "$main_branch" >/dev/null 2>&1; then
        echo "⚠️  $iteration_display Failed to checkout $main_branch" >&2
        return 1
    fi

    return 0
}

commit_on_current_branch() {
    local iteration_display="$1"

    if ! git rev-parse --git-dir > /dev/null 2>&1; then
        return 0
    fi

    local has_changes=false
    if ! git diff --quiet --ignore-submodules=dirty || ! git diff --cached --quiet --ignore-submodules=dirty; then
        has_changes=true
    fi

    if [ -n "$(git ls-files --others --exclude-standard)" ]; then
        has_changes=true
    fi

    if [ "$has_changes" = "false" ]; then
        echo "ℹ️  $iteration_display No changes to commit" >&2
        return 0
    fi

    if [ "$LC_DRY_RUN" = "true" ]; then
        echo "💬 $iteration_display (DRY RUN) Would commit changes on current branch..." >&2
        return 0
    fi

    local commit_attempts=0
    local max_commit_attempts=3

    while [ $commit_attempts -lt $max_commit_attempts ]; do
        commit_attempts=$((commit_attempts + 1))

        if [ $commit_attempts -eq 1 ]; then
            echo "💬 $iteration_display Committing changes on current branch..." >&2
        else
            echo "🔄 $iteration_display Commit retry $commit_attempts/$max_commit_attempts..." >&2
        fi

        local commit_output
        commit_output=$(claude -p "$LC_PROMPT_COMMIT_MESSAGE" --allowedTools "Bash(git)" 2>&1)
        local commit_exit=$?

        if [ $commit_exit -ne 0 ]; then
            echo "⚠️  $iteration_display Commit failed (attempt $commit_attempts/$max_commit_attempts)" >&2
            if [ $commit_attempts -ge $max_commit_attempts ]; then
                echo "❌ $iteration_display Failed to commit after $max_commit_attempts attempts" >&2
                return 1
            fi
            local fix_prompt="The previous commit attempt failed with the following output:\n\n$commit_output\n\nPlease fix the issues (e.g., linting errors, pre-commit hook failures) and then stage and commit all changes using 'git add . && git commit -m \"your message\"'. Do not push."
            echo "🔧 $iteration_display Asking Claude to fix commit issues..." >&2
            claude -p "$fix_prompt" --allowedTools "Bash,Read,Edit,Write,Grep,Glob" >/dev/null 2>&1 || true
            continue
        fi

        if ! git diff --quiet --ignore-submodules=dirty || ! git diff --cached --quiet --ignore-submodules=dirty || [ -n "$(git ls-files --others --exclude-standard)" ]; then
            echo "⚠️  $iteration_display Changes still present after commit (attempt $commit_attempts/$max_commit_attempts)" >&2
            if [ $commit_attempts -ge $max_commit_attempts ]; then
                echo "❌ $iteration_display Uncommitted changes remain after $max_commit_attempts attempts" >&2
                return 1
            fi
            local remaining_files
            remaining_files=$(git diff --name-only --ignore-submodules=dirty 2>/dev/null; git diff --cached --name-only --ignore-submodules=dirty 2>/dev/null; git ls-files --others --exclude-standard 2>/dev/null)
            local fix_prompt="The previous commit did not include all changes. These files still have uncommitted changes:\n\n$remaining_files\n\nPlease stage ALL changes and commit them. Use 'git add . && git commit -m \"your message\"' or amend with 'git add . && git commit --amend --no-edit'. Do not push."
            echo "🔧 $iteration_display Asking Claude to commit remaining files..." >&2
            claude -p "$fix_prompt" --allowedTools "Bash(git)" >/dev/null 2>&1 || true
            continue
        fi

        break
    done

    local commit_title=$(git log -1 --format="%s")
    echo "✅ $iteration_display Committed: $commit_title" >&2
    return 0
}

get_iteration_display() {
    local iteration_num=$1
    local max_runs=$2
    local extra_iters=$3

    if [ -z "$max_runs" ] || [ "$max_runs" -eq 0 ]; then
        echo "($iteration_num)"
    else
        local total=$((max_runs + extra_iters))
        echo "($iteration_num/$total)"
    fi
}

run_claude_iteration() {
    local prompt="$1"
    local flags="$2"
    local error_log="$3"
    local iteration_display="$4"

    if [ "$LC_DRY_RUN" = "true" ]; then
        echo "🤖 (DRY RUN) Would run Claude Code with prompt: $prompt" >&2
        echo "📝 (DRY RUN) Output: This is a simulated response from Claude Code." > "$error_log"
        return 0
    fi

    local temp_stdout=$(mktemp)
    local temp_stderr=$(mktemp)
    local exit_code=0

    set -o pipefail
    { echo $BASHPID > "$LC_CLAUDE_PID_FILE"; exec claude -p "$prompt" $flags "${LC_EXTRA_CLAUDE_FLAGS[@]}"; } 2> >(tee "$temp_stderr" >&2) | \
        tee "$temp_stdout" | \
        while IFS= read -r line; do
            text=$(echo "$line" | jq -r '
                if .type == "assistant" then
                    .message.content[]? | select(.type == "text") | .text // empty
                elif .type == "result" then
                    empty
                else
                    empty
                end
            ' 2>/dev/null)
            if [ -n "$text" ]; then
                echo "$text" | while IFS= read -r output_line; do
                    printf "   %s 💬 %s\n" "$iteration_display" "$output_line" >&2
                done
            fi

            tool_info=$(echo "$line" | jq -r --arg pwd "$PWD" '
                def relpath: (if startswith($pwd + "/") then .[$pwd | length + 1:] elif . == $pwd then "." else . end) // .;
                def get_detail:
                    if .name == "Bash" then
                        ((.input.command // "" | gsub($pwd + "/"; "") | split("\n")[0] | if length > 1000 then .[0:1000] + "..." else . end) // "")
                    elif .name == "Read" then
                        (((.input.file_path // "") | relpath) + (if .input.offset then " (line " + (.input.offset | tostring) + ")" else "" end)) // ""
                    elif .name == "Write" or .name == "Edit" or .name == "MultiEdit" then
                        ((.input.file_path // "") | relpath) // ""
                    elif .name == "Glob" then
                        ((.input.pattern // "") + (if .input.path then " in " + (.input.path | relpath) else "" end)) // ""
                    elif .name == "Grep" then
                        (("\"" + (.input.pattern // "") + "\"" + (if .input.path then " in " + (.input.path | relpath) else "" end) + (if .input.glob then " (" + .input.glob + ")" else "" end))) // ""
                    elif .name == "WebFetch" or (.name | startswith("WebFetch")) then
                        (((.input.url // "") + " → " + ((.input.prompt // "") | if length > 1000 then .[0:1000] + "..." else . end))) // ""
                    elif .name == "WebSearch" or (.name | startswith("WebSearch")) then
                        (("\"" + (.input.query // "") + "\"" + (if .input.allowed_domains then " (domains: " + (.input.allowed_domains | join(", ")) + ")" else "" end))) // ""
                    elif .name == "Task" then
                        (("[" + (.input.subagent_type // "agent") + "] " + (.input.description // ""))) // ""
                    elif .name == "NotebookEdit" then
                        ((((.input.notebook_path // "") | relpath) + " [" + (.input.edit_mode // "replace") + "]")) // ""
                    elif .name == "AskUserQuestion" then
                        ((.input.questions[0].question // "" | if length > 1000 then .[0:1000] + "..." else . end)) // ""
                    elif .name == "Skill" or .name == "SlashCommand" then
                        (("/" + (.input.skill // .input.command // "") + (if .input.args then " " + .input.args else "" end))) // ""
                    elif (.name | test("TodoWrite"; "i")) then
                        ((if .input.todos then
                            (.input.todos | map(select(.status == "in_progress") | .content // .activeForm) | first //
                             (.input.todos | first | .content // .activeForm // "")) |
                            if length > 1000 then .[0:1000] + "..." else . end
                        else "" end)) // ""
                    elif (.name | test("TaskCreate"; "i")) then
                        (.input.subject // .input.description // "")
                    elif (.name | test("TaskUpdate"; "i")) then
                        (("#" + (.input.taskId // "") + " → " + (.input.status // "update"))) // ""
                    elif (.name | test("TaskList|TaskGet"; "i")) then
                        ((if .input.taskId then "#" + .input.taskId else "" end)) // ""
                    elif .name == "TaskOutput" or .name == "BashOutput" then
                        (("id:" + (.input.task_id // .input.bash_id // ""))) // ""
                    elif .name == "KillShell" then
                        (("id:" + (.input.shell_id // ""))) // ""
                    elif .name == "ExitPlanMode" or .name == "EnterPlanMode" then
                        ""
                    elif (.name | startswith("mcp__")) then
                        ((.name | split("__") | .[1:] | join("/"))) // .name
                    else
                        .name
                    end;
                def get_emoji:
                    if .name == "Read" then "📖"
                    elif .name == "Write" then "✍️"
                    elif .name == "Edit" or .name == "MultiEdit" then "✏️"
                    elif .name == "Bash" then "💻"
                    elif .name == "Glob" then "📁"
                    elif .name == "Grep" then "🔎"
                    elif .name == "Task" then "📋"
                    elif .name == "WebFetch" or ((.name | startswith("WebFetch")) // false) then "🌍"
                    elif .name == "WebSearch" or ((.name | startswith("WebSearch")) // false) then "🔍"
                    elif .name == "NotebookEdit" then "📓"
                    elif .name == "AskUserQuestion" then "❓"
                    elif .name == "Skill" or .name == "SlashCommand" then "⚡"
                    elif ((.name | test("Todo|TaskCreate|TaskUpdate|TaskList|TaskGet"; "i")) // false) then "📝"
                    elif .name == "TaskOutput" or .name == "BashOutput" then "📤"
                    elif .name == "KillShell" then "🛑"
                    elif .name == "ExitPlanMode" or .name == "EnterPlanMode" then "🗺️"
                    elif ((.name | startswith("mcp__")) // false) then "🔌"
                    else "🛠️"
                    end;
                if .type == "assistant" then
                    .message.content[]? |
                    select(.type == "tool_use") |
                    ((get_emoji) + " " + ((get_detail) // .name // "unknown"))
                else
                    empty
                end
            ' 2>/dev/null)

            if [ -z "$tool_info" ]; then
                tool_info=$(echo "$line" | jq -r '
                    if .type == "assistant" then
                        .message.content[]? | select(.type == "tool_use") | "🛠️ " + .name
                    else empty end
                ' 2>/dev/null)
            fi

            if [ -n "$tool_info" ]; then
                echo "$tool_info" | while IFS= read -r tool_line; do
                    printf "   %s %s\n" "$iteration_display" "$tool_line" >&2
                done
            fi
        done
    exit_code=${PIPESTATUS[0]}
    set +o pipefail
    : > "$LC_CLAUDE_PID_FILE"

    wait

    if [ -f "$temp_stdout" ] && [ -s "$temp_stdout" ]; then
        cat "$temp_stdout"
    fi

    if [ -f "$temp_stderr" ] && [ -s "$temp_stderr" ]; then
        cat "$temp_stderr" > "$error_log"
    fi

    if [ $exit_code -ne 0 ]; then
        if [ ! -s "$error_log" ] && [ -f "$temp_stdout" ] && [ -s "$temp_stdout" ]; then
            local json_error=$(cat "$temp_stdout" | jq -s -r '.[-1] | if .is_error == true then .result // .error // "Unknown error" else empty end' 2>/dev/null || echo "")
            if [ -n "$json_error" ]; then
                echo "$json_error" > "$error_log"
                echo "$json_error" >&2
            fi
        fi

        if [ ! -s "$error_log" ]; then
            {
                echo "Claude Code exited with code $exit_code but produced no error output"
                echo ""
                echo "This usually means:"
                echo "  - Claude Code crashed or failed to start"
                echo "  - An authentication or permission issue occurred"
                echo "  - The command arguments are invalid"
                echo ""
                echo "Try running this command directly to see the full error:"
                echo "  claude -p \"$prompt\" $flags ${LC_EXTRA_CLAUDE_FLAGS[*]}"
            } >> "$error_log"
        fi

        rm -f "$temp_stdout" "$temp_stderr"
        return $exit_code
    fi

    rm -f "$temp_stdout" "$temp_stderr"

    return 0
}

run_reviewer_iteration() {
    local iteration_display="$1"
    local review_prompt="$2"
    local error_log="$3"

    echo "🔍 $iteration_display Running reviewer pass..." >&2

    local full_reviewer_prompt="${LC_PROMPT_REVIEWER_CONTEXT}

## USER REVIEW INSTRUCTIONS

${review_prompt}"

    local result
    local claude_exit_code=0
    result=$(run_claude_iteration "$full_reviewer_prompt" "$LC_ADDITIONAL_FLAGS" "$error_log" "$iteration_display") || claude_exit_code=$?

    if [ $claude_exit_code -ne 0 ]; then
        echo "❌ $iteration_display Reviewer pass failed with exit code: $claude_exit_code" >&2
        return 1
    fi

    local parse_result=$(parse_claude_result "$result")
    if [ "$?" != "0" ]; then
        echo "❌ $iteration_display Reviewer pass returned error: $parse_result" >&2
        return 1
    fi

    local reviewer_cost=$(echo "$result" | jq -s -r '.[-1].total_cost_usd // empty')
    if [ -n "$reviewer_cost" ]; then
        printf "💰 $iteration_display Reviewer cost: \$%.3f\n" "$reviewer_cost" >&2
        total_cost=$(awk "BEGIN {printf \"%.3f\", $total_cost + $reviewer_cost}")
        printf "   Running total: \$%.3f\n" "$total_cost" >&2
    fi

    echo "✅ $iteration_display Reviewer pass completed" >&2
    return 0
}

parse_claude_result() {
    local result="$1"

    if ! echo "$result" | jq -s -e '.[-1]' >/dev/null 2>&1; then
        echo "invalid_json"
        return 1
    fi

    local is_error=$(echo "$result" | jq -s -r '.[-1].is_error // false')
    if [ "$is_error" = "true" ]; then
        echo "claude_error"
        return 1
    fi

    echo "success"
    return 0
}

handle_iteration_error() {
    local iteration_display="$1"
    local error_type="$2"
    local error_output="$3"

    error_count=$((error_count + 1))
    extra_iterations=$((extra_iterations + 1))

    case "$error_type" in
        "exit_code")
            echo "" >&2
            echo "❌ $iteration_display Error occurred ($error_count consecutive errors):" >&2
            echo "" >&2
            if [ -f "$LC_ERROR_LOG" ] && [ -s "$LC_ERROR_LOG" ]; then
                echo "Error details:" >&2
                cat "$LC_ERROR_LOG" >&2
            else
                echo "No error details captured in log file" >&2
                echo "Error log path: $LC_ERROR_LOG" >&2
            fi
            echo "" >&2
            ;;
        "invalid_json")
            echo "" >&2
            echo "❌ $iteration_display Error: Invalid JSON response ($error_count consecutive errors):" >&2
            echo "" >&2
            echo "$error_output" >&2
            echo "" >&2
            ;;
        "claude_error")
            echo "" >&2
            echo "❌ $iteration_display Error in Claude Code response ($error_count consecutive errors):" >&2
            echo "" >&2
            echo "$error_output" | jq -s -r '.[-1].result // .[-1] // empty' >&2
            echo "" >&2
            ;;
    esac

    if [ $error_count -ge 3 ]; then
        echo "❌ Fatal: 3 consecutive errors occurred. Exiting." >&2
        exit 1
    fi

    return 1
}

handle_iteration_success() {
    local iteration_display="$1"
    local result="$2"
    local branch_name="$3"
    local main_branch="$4"
    local notes_file="$5"

    local result_text=$(echo "$result" | jq -s -r '.[-1].result // empty')

    if [ -n "$result_text" ] && [[ "$result_text" == *"$LC_COMPLETION_SIGNAL"* ]]; then
        completion_signal_count=$((completion_signal_count + 1))
        echo "" >&2
        echo "🎯 $iteration_display Completion signal detected ($completion_signal_count/$LC_COMPLETION_THRESHOLD)" >&2
    else
        if [ $completion_signal_count -gt 0 ]; then
            echo "" >&2
            echo "🔄 $iteration_display Completion signal not found, resetting counter" >&2
        fi
        completion_signal_count=0
    fi

    local cost=$(echo "$result" | jq -s -r '.[-1].total_cost_usd // empty')
    if [ -n "$cost" ]; then
        echo "" >&2
        printf "💰 $iteration_display Iteration cost: \$%.3f\n" "$cost" >&2
        total_cost=$(awk "BEGIN {printf \"%.3f\", $total_cost + $cost}")
        printf "   Running total: \$%.3f\n" "$total_cost" >&2
    fi

    echo "✅ $iteration_display Work completed" >&2
    if [ "$LC_ENABLE_COMMITS" = "true" ]; then
        if [ "$LC_DISABLE_BRANCHES" = "true" ]; then
            if ! commit_on_current_branch "$iteration_display"; then
                echo "❌ $iteration_display Failed to commit all files. Stopping." >&2
                exit 1
            fi
        else
            if ! linear_claude_commit "$iteration_display" "$branch_name" "$main_branch" "$notes_file"; then
                echo "❌ $iteration_display Failed to commit all files. Stopping." >&2
                exit 1
            fi
        fi
    else
        echo "⏭️  $iteration_display Skipping commits (--disable-commits flag set)" >&2
        if [ -n "$branch_name" ] && git rev-parse --git-dir > /dev/null 2>&1; then
            git checkout "$main_branch" >/dev/null 2>&1
            git branch -D "$branch_name" >/dev/null 2>&1 || true
        fi
    fi

    error_count=0
    if [ $extra_iterations -gt 0 ]; then
        extra_iterations=$((extra_iterations - 1))
    fi
    successful_iterations=$((successful_iterations + 1))
    return 0
}

handle_claude_questions() {
    local result="$1"
    local notes_file="$2"
    local iteration_display="$3"

    # Extract AskUserQuestion tool calls from the JSON stream
    local questions_json
    questions_json=$(echo "$result" | jq -s '[.[] | select(.type == "assistant") | .message.content[]? | select(.type == "tool_use" and .name == "AskUserQuestion") | .input.questions[]?]' 2>/dev/null)

    if [ -z "$questions_json" ] || [ "$questions_json" = "[]" ] || [ "$questions_json" = "null" ]; then
        return 1  # No questions found
    fi

    local question_count
    question_count=$(echo "$questions_json" | jq length 2>/dev/null)

    if [ -z "$question_count" ] || [ "$question_count" -eq 0 ]; then
        return 1
    fi

    echo "" >&2
    echo "❓ $iteration_display Claude needs your input:" >&2
    echo "" >&2

    local answers=""
    local q_idx=0

    while [ $q_idx -lt "$question_count" ]; do
        local question_text
        question_text=$(echo "$questions_json" | jq -r ".[$q_idx].question // \"\"")

        local options
        options=$(echo "$questions_json" | jq -r ".[$q_idx].options // []")
        local opt_count
        opt_count=$(echo "$options" | jq length 2>/dev/null || echo 0)

        echo "  $question_text" >&2

        if [ "$opt_count" -gt 0 ]; then
            echo "" >&2
            local o_idx=0
            while [ $o_idx -lt "$opt_count" ]; do
                local label desc
                label=$(echo "$options" | jq -r ".[$o_idx].label // \"\"")
                desc=$(echo "$options" | jq -r ".[$o_idx].description // empty")
                printf "    %d) %s" "$((o_idx + 1))" "$label" >&2
                if [ -n "$desc" ]; then
                    printf " — %s" "$desc" >&2
                fi
                echo "" >&2
                o_idx=$((o_idx + 1))
            done
            echo "" >&2
        fi

        local answer
        read -r -p "  > " answer </dev/tty
        echo "" >&2

        # Resolve numbered option to label
        if [ "$opt_count" -gt 0 ] && [[ "$answer" =~ ^[0-9]+$ ]] && [ "$answer" -ge 1 ] && [ "$answer" -le "$opt_count" ]; then
            answer=$(echo "$options" | jq -r ".[$(($answer - 1))].label // \"$answer\"")
        fi

        answers+="Q: $question_text
A: $answer

"
        q_idx=$((q_idx + 1))
    done

    # Export answers for the caller to accumulate
    LC_LAST_ANSWERS="$answers"

    # Save Q&A to the notes file
    mkdir -p "$(dirname "$notes_file")"
    {
        if [ -f "$notes_file" ]; then
            cat "$notes_file"
            echo ""
        fi
        echo "## Answers from user"
        echo ""
        printf "%s" "$answers"
    } > "${notes_file}.tmp" && mv "${notes_file}.tmp" "$notes_file"

    echo "💾 $iteration_display Answers saved to $notes_file" >&2
    return 0
}

execute_single_iteration() {
    local iteration_num=$1
    local override_branch="$2"
    local identifier="${3:-iteration-${iteration_num}}"
    local notes_file="./.claude/plans/${identifier}.md"

    local iteration_display=$(get_iteration_display $iteration_num "${LC_MAX_RUNS:-0}" $extra_iterations)
    echo "🔄 $iteration_display Starting iteration..." >&2

    local main_branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "main")
    local branch_name=""

    if [ "$LC_ENABLE_COMMITS" = "true" ] && [ "$LC_DISABLE_BRANCHES" != "true" ]; then
        branch_name=$(create_iteration_branch "$iteration_display" "$iteration_num" "$override_branch")
        if [ $? -ne 0 ] || [ -z "$branch_name" ]; then
            if git rev-parse --git-dir > /dev/null 2>&1; then
                echo "❌ $iteration_display Failed to create branch" >&2
                handle_iteration_error "$iteration_display" "exit_code" ""
                return 1
            fi
            branch_name=""
        fi
    fi

    mkdir -p ./.claude/plans

    local max_question_rounds=3
    local question_round=0
    local result=""
    local accumulated_answers=""

    while [ $question_round -lt $max_question_rounds ]; do
        local enhanced_prompt="${LC_PROMPT_WORKFLOW_CONTEXT//COMPLETION_SIGNAL_PLACEHOLDER/$LC_COMPLETION_SIGNAL}

$LC_PROMPT

"

        if [ -f "$notes_file" ]; then
            local notes_content
            notes_content=$(cat "$notes_file")
            enhanced_prompt+="## CONTEXT FROM PREVIOUS ITERATION

The following notes were saved from a previous iteration working on this ticket ($notes_file):

$notes_content

"
        fi

        if [ -n "$accumulated_answers" ]; then
            enhanced_prompt+="## USER ANSWERS TO YOUR PREVIOUS QUESTIONS

IMPORTANT: These questions have already been answered by the user. Do NOT ask them again. Use these answers and proceed with the implementation.

$accumulated_answers
"
        fi

        enhanced_prompt+="## ITERATION NOTES

"

        if [ -f "$notes_file" ]; then
            enhanced_prompt+="Update the \`$notes_file\` file with relevant context for the next iteration. Add new notes and remove outdated information to keep it current and useful."
        else
            enhanced_prompt+="Create a \`$notes_file\` file with relevant context and instructions for the next iteration."
        fi

        enhanced_prompt+="$LC_PROMPT_NOTES_GUIDELINES"

        echo "🤖 $iteration_display Running Claude Code..." >&2

        local claude_exit_code=0
        result=$(run_claude_iteration "$enhanced_prompt" "$LC_ADDITIONAL_FLAGS" "$LC_ERROR_LOG" "$iteration_display") || claude_exit_code=$?

        if [ $claude_exit_code -ne 0 ]; then
            echo "" >&2
            echo "⚠️  Claude Code command failed with exit code: $claude_exit_code" >&2
            if [ -n "$branch_name" ] && git rev-parse --git-dir > /dev/null 2>&1; then
                git checkout "$main_branch" >/dev/null 2>&1
                git branch -D "$branch_name" >/dev/null 2>&1 || true
            fi
            handle_iteration_error "$iteration_display" "exit_code" ""
            return 1
        fi

        # Save result to .claude/plans/
        if [ -n "$result" ]; then
            echo "$result" > "./.claude/plans/$(date +%Y%m%d-%H%M%S)-${identifier}.json"
        fi

        local parse_result=$(parse_claude_result "$result")
        if [ "$?" != "0" ]; then
            if [ -n "$branch_name" ] && git rev-parse --git-dir > /dev/null 2>&1; then
                git checkout "$main_branch" >/dev/null 2>&1
                git branch -D "$branch_name" >/dev/null 2>&1 || true
            fi
            handle_iteration_error "$iteration_display" "$parse_result" "$result"
            return 1
        fi

        # Check if Claude asked questions — if so, collect answers and re-run
        if handle_claude_questions "$result" "$notes_file" "$iteration_display"; then
            accumulated_answers+="$LC_LAST_ANSWERS"
            question_round=$((question_round + 1))
            echo "🔄 $iteration_display Re-running with user answers (round $((question_round + 1))/$max_question_rounds)..." >&2
            continue
        fi

        break
    done

    # Run reviewer pass if LC_REVIEW_PROMPT is set
    if [ -n "$LC_REVIEW_PROMPT" ]; then
        if ! run_reviewer_iteration "$iteration_display" "$LC_REVIEW_PROMPT" "$LC_ERROR_LOG"; then
            echo "❌ $iteration_display Reviewer failed, aborting iteration" >&2
            if [ -n "$branch_name" ] && git rev-parse --git-dir > /dev/null 2>&1; then
                git checkout "$main_branch" >/dev/null 2>&1
                git branch -D "$branch_name" >/dev/null 2>&1 || true
            fi
            error_count=$((error_count + 1))
            extra_iterations=$((extra_iterations + 1))
            if [ $error_count -ge 3 ]; then
                echo "❌ Fatal: 3 consecutive errors occurred. Exiting." >&2
                exit 1
            fi
            return 1
        fi
    fi

    handle_iteration_success "$iteration_display" "$result" "$branch_name" "$main_branch" "$notes_file"
    return 0
}

fetch_ci_failures() {
    local pr_owner="$1"
    local pr_repo="$2"
    local pr_number="$3"
    local iteration_display="$4"

    # Get the head SHA of the PR
    local head_sha
    head_sha=$(gh api "repos/$pr_owner/$pr_repo/pulls/$pr_number" --jq '.head.sha' 2>/dev/null)
    if [ -z "$head_sha" ]; then
        echo ""
        return
    fi

    # Get check runs for this commit
    local check_runs
    check_runs=$(gh api "repos/$pr_owner/$pr_repo/commits/$head_sha/check-runs" 2>/dev/null)
    if [ -z "$check_runs" ]; then
        echo ""
        return
    fi

    # Get failed check runs
    local failed_checks
    failed_checks=$(echo "$check_runs" | jq -r '[.check_runs[] | select(.conclusion == "failure" or .conclusion == "timed_out") | {name: .name, conclusion: .conclusion, id: .id}]' 2>/dev/null)

    local failed_count
    failed_count=$(echo "$failed_checks" | jq -r 'length' 2>/dev/null || echo "0")

    if [ "$failed_count" = "0" ] || [ "$failed_count" = "" ]; then
        # Also check commit statuses (some CI systems use the status API instead of checks)
        local commit_statuses
        commit_statuses=$(gh api "repos/$pr_owner/$pr_repo/commits/$head_sha/status" 2>/dev/null)
        local failed_statuses
        failed_statuses=$(echo "$commit_statuses" | jq -r '[.statuses[] | select(.state == "failure" or .state == "error") | {context: .context, state: .state, description: .description, target_url: .target_url}]' 2>/dev/null)
        local failed_status_count
        failed_status_count=$(echo "$failed_statuses" | jq -r 'length' 2>/dev/null || echo "0")

        if [ "$failed_status_count" = "0" ] || [ "$failed_status_count" = "" ]; then
            echo ""
            return
        fi

        echo "🔴 $iteration_display Found $failed_status_count failing CI status(es)" >&2
        echo "$failed_statuses" | jq -r '.[] | "  - \(.context): \(.state) — \(.description // "no description")"' >&2
        # Return status failures as context (no logs available for status API)
        echo "$failed_statuses" | jq -r '"### Failing CI Statuses\n" + (. | map("- **\(.context)**: \(.state) — \(.description // "no description")") | join("\n"))'
        return
    fi

    echo "🔴 $iteration_display Found $failed_count failing CI check(s)" >&2
    echo "$failed_checks" | jq -r '.[] | "  - \(.name): \(.conclusion)"' >&2

    # Fetch logs for each failed check run (truncated to avoid huge prompts)
    local ci_context="### Failing CI Checks\n"
    local check_id check_name
    while IFS=$'\t' read -r check_id check_name; do
        [ -z "$check_id" ] && continue
        ci_context+="\\n#### Check: $check_name\\n"
        # Fetch the log via the GitHub API — returns plain text
        local log_content
        log_content=$(gh api "repos/$pr_owner/$pr_repo/check-runs/$check_id/annotations" 2>/dev/null | jq -r '.[] | "[\(.annotation_level)] \(.path):\(.start_line) — \(.message)"' 2>/dev/null)

        if [ -n "$log_content" ]; then
            ci_context+="Annotations:\\n\`\`\`\\n${log_content:0:3000}\\n\`\`\`\\n"
        fi

        # Also try to get the failed job logs
        local jobs_data
        jobs_data=$(gh api "repos/$pr_owner/$pr_repo/actions/runs" --jq ".workflow_runs[] | select(.head_sha == \"$head_sha\" and .conclusion == \"failure\") | .id" 2>/dev/null | head -n 3)

        if [ -n "$jobs_data" ]; then
            while read -r run_id; do
                [ -z "$run_id" ] && continue
                local failed_jobs
                failed_jobs=$(gh api "repos/$pr_owner/$pr_repo/actions/runs/$run_id/jobs" --jq '[.jobs[] | select(.conclusion == "failure") | {name: .name, id: .id}]' 2>/dev/null)
                local job_id job_name
                while IFS=$'\t' read -r job_id job_name; do
                    [ -z "$job_id" ] && continue
                    local job_log
                    job_log=$(gh api "repos/$pr_owner/$pr_repo/actions/jobs/$job_id/logs" 2>/dev/null | tail -n 100)
                    if [ -n "$job_log" ]; then
                        ci_context+="\\nJob: $job_name (last 100 lines):\\n\`\`\`\\n${job_log:0:5000}\\n\`\`\`\\n"
                    fi
                done < <(echo "$failed_jobs" | jq -r '.[] | "\(.id)\t\(.name)"' 2>/dev/null)
            done <<< "$jobs_data"
        fi
    done < <(echo "$failed_checks" | jq -r '.[] | "\(.id)\t\(.name)"' 2>/dev/null)

    printf '%b' "$ci_context"
}

handle_in_review_issue() {
    local identifier="$1"
    local title="$2"
    local branch_name="$3"
    local iteration_display="$4"
    local pr_url="$5"

    echo "🔍 $iteration_display Handling review for $identifier ($branch_name)..." >&2

    if [ -z "$branch_name" ]; then
        echo "⚠️  $iteration_display No branch name for issue $identifier, skipping review handling" >&2
        return 1
    fi

    # Extract PR owner/repo/number from Linear attachment URL
    local pr_owner pr_repo pr_number
    if [ -n "$pr_url" ]; then
        # Parse https://github.com/owner/repo/pull/123
        pr_owner=$(echo "$pr_url" | sed -n 's|.*github\.com/\([^/]*\)/.*|\1|p')
        pr_repo=$(echo "$pr_url" | sed -n 's|.*github\.com/[^/]*/\([^/]*\)/.*|\1|p')
        pr_number=$(echo "$pr_url" | sed -n 's|.*/pull/\([0-9]*\).*|\1|p')
    fi

    if [ -z "$pr_number" ]; then
        echo "⚠️  $iteration_display No PR found in Linear attachments for $identifier, skipping review handling" >&2
        return 1
    fi

    echo "📋 $iteration_display Found PR #$pr_number ($pr_owner/$pr_repo) from Linear for branch $branch_name" >&2

    # Check CI status
    echo "🔄 $iteration_display Checking CI status for PR #$pr_number..." >&2
    local ci_failures
    ci_failures=$(fetch_ci_failures "$pr_owner" "$pr_repo" "$pr_number" "$iteration_display")

    # Fetch all three types of comments
    local inline_comments
    inline_comments=$(gh api "repos/$pr_owner/$pr_repo/pulls/$pr_number/comments" 2>/dev/null | jq -r '[.[] | {user: .user.login, body: .body, path: .path, line: .line, created_at: .created_at}]' 2>/dev/null || echo "[]")

    local review_bodies
    review_bodies=$(gh api "repos/$pr_owner/$pr_repo/pulls/$pr_number/reviews" 2>/dev/null | jq -r '[.[] | select(.body != null and .body != "") | {user: .user.login, body: .body, state: .state, created_at: .submitted_at}]' 2>/dev/null || echo "[]")

    local conversation_comments
    conversation_comments=$(gh api "repos/$pr_owner/$pr_repo/issues/$pr_number/comments" 2>/dev/null | jq -r '[.[] | {user: .user.login, body: .body, created_at: .created_at}]' 2>/dev/null || echo "[]")

    # Check if there's anything to do
    local has_review_comments=false
    local has_ci_failures=false
    if [ -n "$ci_failures" ]; then
        has_ci_failures=true
    fi
    local total_comments
    total_comments=$(echo "$inline_comments" "$review_bodies" "$conversation_comments" | jq -s 'map(length) | add' 2>/dev/null || echo "0")
    if [ "$total_comments" -gt 0 ] 2>/dev/null; then
        has_review_comments=true
    fi

    if [ "$has_review_comments" = "false" ] && [ "$has_ci_failures" = "false" ]; then
        echo "✅ $iteration_display No review comments or CI failures for $identifier, nothing to do" >&2
        return 0
    fi

    # Build the review prompt
    local review_prompt="## CODE REVIEW RESOLUTION: $identifier — $title

You are resolving review comments and CI failures on PR #$pr_number in $pr_owner/$pr_repo (branch: $branch_name)."

    if [ "$has_ci_failures" = "true" ]; then
        review_prompt+="

### CI Failures
The following CI checks are failing. Fix these errors:

$ci_failures"
    fi

    if [ "$has_review_comments" = "true" ]; then
        review_prompt+="

### Inline Code Review Comments
$inline_comments

### Review Bodies
$review_bodies

### General Conversation Comments
$conversation_comments"
    fi

    review_prompt+="

## INSTRUCTIONS

1. Read through all the review comments and CI failures above
2. Address each piece of feedback by making the appropriate code changes
3. Fix any CI failures — read the error logs carefully and fix the root cause
4. If a comment is unclear or you disagree, make your best judgment
5. Focus on addressing the feedback and fixing CI, not adding unrelated changes
6. Do NOT commit or push changes — the automation will handle that"

    # Fetch and checkout the PR branch
    echo "🌿 $iteration_display Checking out PR branch: $branch_name" >&2
    git fetch origin "$branch_name" >/dev/null 2>&1
    if ! git checkout "$branch_name" >/dev/null 2>&1; then
        if ! git checkout -b "$branch_name" "origin/$branch_name" >/dev/null 2>&1; then
            echo "⚠️  $iteration_display Failed to checkout branch $branch_name" >&2
            return 1
        fi
    fi
    git reset --hard "origin/$branch_name" >/dev/null 2>&1 || true

    echo "🤖 $iteration_display Running Claude Code to resolve review comments..." >&2

    local result
    local claude_exit_code=0
    LC_PROMPT="$review_prompt"
    result=$(run_claude_iteration "$review_prompt" "$LC_ADDITIONAL_FLAGS" "$LC_ERROR_LOG" "$iteration_display") || claude_exit_code=$?

    # Save result
    if [ -n "$result" ]; then
        mkdir -p ./.claude/plans
        echo "$result" > "./.claude/plans/$(date +%Y%m%d-%H%M%S)-${identifier}-review.json"
    fi

    if [ $claude_exit_code -ne 0 ]; then
        echo "⚠️  $iteration_display Claude Code failed for review resolution" >&2
        git checkout main >/dev/null 2>&1
        return 1
    fi

    # Extract cost
    local cost=$(echo "$result" | jq -s -r '.[-1].total_cost_usd // empty')
    if [ -n "$cost" ]; then
        printf "💰 $iteration_display Review resolution cost: \$%.3f\n" "$cost" >&2
        total_cost=$(awk "BEGIN {printf \"%.3f\", $total_cost + $cost}")
        printf "   Running total: \$%.3f\n" "$total_cost" >&2
    fi

    # Commit and push if there are changes
    local has_changes=false
    if ! git diff --quiet --ignore-submodules=dirty || ! git diff --cached --quiet --ignore-submodules=dirty; then
        has_changes=true
    fi
    if [ -n "$(git ls-files --others --exclude-standard)" ]; then
        has_changes=true
    fi

    if [ "$has_changes" = "true" ]; then
        echo "💬 $iteration_display Committing review fixes..." >&2
        if claude -p "$LC_PROMPT_COMMIT_MESSAGE" --allowedTools "Bash(git)" >/dev/null 2>&1; then
            echo "📤 $iteration_display Pushing review fixes..." >&2
            git push origin "$branch_name" >/dev/null 2>&1 || echo "⚠️  $iteration_display Failed to push review fixes" >&2
        else
            echo "⚠️  $iteration_display Failed to commit review fixes" >&2
        fi
    else
        echo "ℹ️  $iteration_display No changes needed for review comments" >&2
    fi

    # Return to main branch
    git checkout main >/dev/null 2>&1
    echo "✅ $iteration_display Review comments addressed for $identifier" >&2
    return 0
}

fetch_linear_view_issues() {
    echo "🔍 Fetching issues from Linear view: $LC_LINEAR_VIEW" >&2

    if [ "$LC_DRY_RUN" = "true" ]; then
        echo "🤖 (DRY RUN) Would fetch Linear view issues from: $LC_LINEAR_VIEW" >&2
        LC_LINEAR_ISSUES_JSON='[]'
        return 0
    fi

    # Extract view ID from URL or use as-is
    local view_id="$LC_LINEAR_VIEW"
    if [[ "$LC_LINEAR_VIEW" == http* ]]; then
        view_id=$(echo "$LC_LINEAR_VIEW" | sed -n 's|.*/view/\([^/]*\).*|\1|p')
        if [ -z "$view_id" ]; then
            echo "❌ Could not extract view ID from URL: $LC_LINEAR_VIEW" >&2
            return 1
        fi
    fi

    # Validate view_id contains only alphanumeric, hyphens, underscores (prevent GraphQL injection)
    if ! [[ "$view_id" =~ ^[a-zA-Z0-9_-]+$ ]]; then
        echo "❌ Invalid view ID format (must be alphanumeric/hyphens/underscores): $view_id" >&2
        return 1
    fi

    echo "🔍 View ID: $view_id" >&2

    local response
    local exit_code=0
    response=$(linear api "{ customView(id: \"$view_id\") { issues { nodes { id identifier title description branchName state { name } attachments { nodes { url title sourceType } } } } } }" --paginate 2>&1) || exit_code=$?

    if [ $exit_code -ne 0 ]; then
        echo "❌ Failed to fetch Linear view issues (exit code: $exit_code)" >&2
        echo "Response: $response" >&2
        return 1
    fi

    local errors
    errors=$(echo "$response" | jq -r '.errors[0].message // empty' 2>/dev/null)
    if [ -n "$errors" ]; then
        echo "❌ Linear API error: $errors" >&2
        return 1
    fi

    local issues_node
    issues_node=$(echo "$response" | jq '.data.customView.issues.nodes // empty' 2>/dev/null)
    if [ -z "$issues_node" ] || [ "$issues_node" = "null" ]; then
        echo "❌ Could not find view or no issues in view: $view_id" >&2
        return 1
    fi

    local json_array
    json_array=$(echo "$response" | jq '[.data.customView.issues.nodes[] | {id, identifier, title, description: (.description // "" | .[0:500]), branchName: (.branchName // ""), state: (.state.name // ""), prUrl: ([.attachments.nodes[] | select(.url | test("github.com/.*/pull/"))] | first | .url // "")}]' 2>/dev/null)

    if [ -z "$json_array" ] || ! echo "$json_array" | jq empty 2>/dev/null; then
        echo "❌ Failed to parse issues from Linear API response" >&2
        return 1
    fi

    local issue_count
    issue_count=$(echo "$json_array" | jq length)
    echo "✅ Found $issue_count issues in Linear view" >&2

    LC_LINEAR_ISSUES_JSON="$json_array"
    return 0
}

main_loop_linear_view() {
    # Initialize start time if LC_MAX_DURATION is set
    if [ -n "$LC_MAX_DURATION" ]; then
        start_time=$(date +%s)
    fi

    local issue_count
    issue_count=$(echo "$LC_LINEAR_ISSUES_JSON" | jq length)

    if [ "$issue_count" -eq 0 ]; then
        echo "⚠️  No issues found in Linear view, nothing to do." >&2
        return
    fi

    local issue_index=0
    while [ $issue_index -lt "$issue_count" ]; do
        # Check limits
        local should_continue=true

        if [ -n "$LC_MAX_RUNS" ] && [ "$LC_MAX_RUNS" -ne 0 ] && [ $successful_iterations -ge "$LC_MAX_RUNS" ]; then
            should_continue=false
        fi

        if [ -n "$LC_MAX_COST" ] && [ "$(awk "BEGIN {print ($total_cost >= $LC_MAX_COST)}")" = "1" ]; then
            should_continue=false
        fi

        if [ -n "$LC_MAX_DURATION" ] && [ -n "$start_time" ]; then
            local current_time
            current_time=$(date +%s)
            local elapsed_time=$((current_time - start_time))
            if [ $elapsed_time -ge "$LC_MAX_DURATION" ]; then
                echo "" >&2
                echo "⏱️  Maximum duration reached ($(format_duration $elapsed_time))" >&2
                should_continue=false
            fi
        fi

        if [ "$LC_INTERRUPTED" = "true" ]; then
            echo "⛔ Ctrl+C received — stopping loop." >&2
            should_continue=false
        fi

        if [ "$should_continue" = "false" ]; then
            break
        fi

        # Extract issue details
        local issue
        issue=$(echo "$LC_LINEAR_ISSUES_JSON" | jq -r ".[$issue_index]")
        local identifier
        identifier=$(echo "$issue" | jq -r '.identifier')
        local title
        title=$(echo "$issue" | jq -r '.title')
        local description
        description=$(echo "$issue" | jq -r '.description // ""')
        local branch_name
        branch_name=$(echo "$issue" | jq -r '.branchName // ""')
        local pr_url
        pr_url=$(echo "$issue" | jq -r '.prUrl // ""')

        # If a PR exists, get the actual branch name from GitHub instead of assuming from Linear
        if [ -n "$pr_url" ]; then
            local pr_owner_tmp pr_repo_tmp pr_number_tmp pr_branch_tmp
            pr_owner_tmp=$(echo "$pr_url" | sed -n 's|.*github\.com/\([^/]*\)/.*|\1|p')
            pr_repo_tmp=$(echo "$pr_url" | sed -n 's|.*github\.com/[^/]*/\([^/]*\)/.*|\1|p')
            pr_number_tmp=$(echo "$pr_url" | sed -n 's|.*/pull/\([0-9]*\).*|\1|p')
            if [ -n "$pr_number_tmp" ]; then
                pr_branch_tmp=$(gh api "repos/$pr_owner_tmp/$pr_repo_tmp/pulls/$pr_number_tmp" --jq '.head.ref' 2>/dev/null)
                if [ -n "$pr_branch_tmp" ]; then
                    if [ "$pr_branch_tmp" != "$branch_name" ] && [ -n "$branch_name" ]; then
                        echo "ℹ️  PR branch '$pr_branch_tmp' differs from Linear branch '$branch_name', using PR branch" >&2
                    fi
                    branch_name="$pr_branch_tmp"
                fi
            fi
        fi
        local state
        state=$(echo "$issue" | jq -r '.state // ""')

        # Lowercase state for case-insensitive matching
        local state_lower
        state_lower=$(echo "$state" | tr '[:upper:]' '[:lower:]')

        echo "" >&2
        echo "📋 Processing issue $((issue_index + 1))/$issue_count: $identifier — $title (state: $state)" >&2

        # Abort if there are uncommitted changes
        if ! git diff --quiet 2>/dev/null || ! git diff --cached --quiet 2>/dev/null; then
            echo "❌ Error: Uncommitted changes detected. Please commit or stash before continuing." >&2
            git status --short >&2
            exit 1
        fi

        # Start from origin/main
        echo "🔄 Resetting to origin/main..." >&2
        git fetch origin main >/dev/null 2>&1 || { echo "❌ Error: Failed to fetch origin/main" >&2; exit 1; }
        git checkout main >/dev/null 2>&1 || git checkout -b main origin/main >/dev/null 2>&1
        git reset --hard origin/main >/dev/null 2>&1 || { echo "❌ Error: Failed to reset to origin/main" >&2; exit 1; }

        # Status-based routing
        case "$state_lower" in
            "done"|"in progress")
                echo "⏭️  Skipping $identifier — status is '$state'" >&2
                ;;
            "in review")
                local iteration_display=$(get_iteration_display $i "${LC_MAX_RUNS:-0}" $extra_iterations)
                handle_in_review_issue "$identifier" "$title" "$branch_name" "$iteration_display" "$pr_url"
                i=$((i + 1))
                ;;
            *)
                # Normal iteration: Todo, Backlog, etc.
                LC_PROMPT="## LINEAR ISSUE: $identifier — $title

$description

## INSTRUCTIONS

Implement the changes described in this Linear issue. Focus on making meaningful, well-tested progress."

                execute_single_iteration $i "$branch_name" "$identifier"
                i=$((i + 1))
                ;;
        esac

        sleep 1
        issue_index=$((issue_index + 1))
    done
}

show_completion_summary() {
    local elapsed_msg=""
    if [ -n "$start_time" ]; then
        local current_time=$(date +%s)
        local elapsed_time=$((current_time - start_time))
        elapsed_msg=" (elapsed: $(format_duration $elapsed_time))"
    fi

    if [ $completion_signal_count -ge $LC_COMPLETION_THRESHOLD ]; then
        if [ -n "$total_cost" ] && [ "$(awk "BEGIN {print ($total_cost > 0)}")" = "1" ]; then
            printf "✨ Project completed! Detected completion signal %d times in a row. Total cost: \$%.3f%s\n" "$completion_signal_count" "$total_cost" "$elapsed_msg"
        else
            printf "✨ Project completed! Detected completion signal %d times in a row.%s\n" "$completion_signal_count" "$elapsed_msg"
        fi
    elif [ -n "$LC_MAX_RUNS" ] && [ "$LC_MAX_RUNS" -ne 0 ] || [ -n "$LC_MAX_COST" ] || [ -n "$LC_MAX_DURATION" ]; then
        if [ -n "$total_cost" ] && [ "$(awk "BEGIN {print ($total_cost > 0)}")" = "1" ]; then
            printf "🎉 Done with total cost: \$%.3f%s\n" "$total_cost" "$elapsed_msg"
        else
            printf "🎉 Done%s\n" "$elapsed_msg"
        fi
    fi
}

cmd_view() {
    parse_arguments "$@"
    check_for_updates
    validate_arguments
    validate_requirements

    LC_ERROR_LOG=$(mktemp)
    LC_INTERRUPTED=false
    LC_CLAUDE_PID_FILE=$(mktemp)
    trap 'LC_INTERRUPTED=true; echo "" >&2; echo "⛔ Interrupted — killing claude and exiting..." >&2; LC_TRAP_PID=$(cat "$LC_CLAUDE_PID_FILE" 2>/dev/null); if [ -n "$LC_TRAP_PID" ] && kill -0 "$LC_TRAP_PID" 2>/dev/null; then kill "$LC_TRAP_PID" 2>/dev/null; fi' INT TERM
    trap "rm -f $LC_ERROR_LOG $LC_CLAUDE_PID_FILE" EXIT

    fetch_linear_view_issues || exit 1
    main_loop_linear_view
    show_completion_summary
}

dispatch() {
    if [ $# -eq 0 ]; then
        show_help
        exit 0
    fi

    case "$1" in
        view)
            shift
            cmd_view "$@"
            ;;
        update)
            shift
            cmd_update "$@"
            ;;
        version|-v|--version)
            show_version
            ;;
        help|-h|--help)
            show_help
            ;;
        *)
            echo "❌ Unknown command: $1" >&2
            echo "" >&2
            echo "Usage: linear-claude <command> [options]" >&2
            echo "" >&2
            echo "Available commands: view, update, version, help" >&2
            echo "Run 'linear-claude help' for more information." >&2
            exit 1
            ;;
    esac
}

if [ -z "$TESTING" ]; then
    dispatch "$@"
fi
