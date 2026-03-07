# Changelog

## v1.0.0

- chore: bump version (0c5eb3b)
- fix: release target (455d529)
- refactor: interactive claude (#1) (09e476f)
- release: update changelog and checksum for v0.1.1 (f553272)

## v0.1.1

- fix: release workflow (cce2ffb)
- fix: trap directory and notes (08e9e94)

## v0.1.0

Initial release.

### Features

- **Subcommand CLI**: `linear-claude view <url-or-id>`, `linear-claude update`, `linear-claude version`, `linear-claude help`
- **Linear integration**: Fetch issues from a Linear custom view and process them with Claude Code
- **Status-based routing**: Todo/Backlog issues get implemented, In Review issues get CI failures and PR comments addressed, Done/In Progress issues are skipped
- **Self-update**: `linear-claude update` downloads the latest release from GitHub with SHA256 verification
- **Update check**: Automatically checks for newer versions (cached for 24h) at the start of `linear-claude view`
- **GitHub Actions release workflow**: Tag a `v*.*.*` to create a release with script + checksum assets
- **Per-issue context**: Notes stored in `.claude/plans/<identifier>.md` persist across iterations
- **Review handling**: Fetches CI failures, inline comments, review bodies, and conversation comments for In Review issues
- **Q&A loops**: Up to 3 rounds of interactive questions when Claude needs user input
- **Reviewer pass**: Optional `--review-prompt` runs a second Claude pass to validate changes
- **Configurable limits**: `--max-runs`, `--max-cost`, `--max-duration`
- **Dry run mode**: `--dry-run` simulates execution without changes
