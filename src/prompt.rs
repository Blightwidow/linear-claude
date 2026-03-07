use std::path::Path;

pub const WORKFLOW_CONTEXT: &str = r#"## CONTINUOUS WORKFLOW CONTEXT

This is part of a continuous development loop where work happens incrementally across multiple iterations. You might run once, then a human developer might make changes, then you run again, and so on. This could happen daily or on any schedule.

**Important**: You don't need to complete the entire goal in one iteration. Just make meaningful progress on one thing, then leave clear notes for the next iteration (human or AI). Think of it as a relay race where you're passing the baton.

**When you're done**: Stage all changes with `git add .`, commit with a clear message, and push the branch. Do not create a PR — the automation will handle that if needed.

**Project Completion Signal**: If you determine that not just your current task but the ENTIRE project goal is fully complete (nothing more to be done on the overall goal), only include the exact phrase "COMPLETION_SIGNAL_PLACEHOLDER" in your response. Only use this when absolutely certain that the whole project is finished, not just your individual task. We will stop working on this project when multiple developers independently determine that the project is complete.

## PRIMARY GOAL"#;

pub const NOTES_GUIDELINES: &str = r#"

This file helps coordinate work across iterations (both human and AI developers). It should:

- Contain relevant context and instructions for the next iteration
- Stay concise and actionable (like a notes file, not a detailed report)
- Help the next developer understand what to do next

The file should NOT include:
- Lists of completed work or full reports
- Information that can be discovered by running tests/coverage
- Unnecessary details"#;

/// Build the main iteration prompt for a Linear issue.
pub fn build_iteration_prompt(
    issue_prompt: &str,
    completion_signal: &str,
    notes_file: &str,
    notes_exist: bool,
    review_prompt: Option<&str>,
) -> String {
    let workflow = WORKFLOW_CONTEXT.replace("COMPLETION_SIGNAL_PLACEHOLDER", completion_signal);

    let mut prompt = format!("{workflow}\n\n{issue_prompt}\n\n");

    if notes_exist {
        if let Ok(notes_content) = std::fs::read_to_string(notes_file) {
            prompt.push_str(&format!(
                "## CONTEXT FROM PREVIOUS ITERATION\n\n\
                 The following notes were saved from a previous iteration working on this ticket ({notes_file}):\n\n\
                 {notes_content}\n\n"
            ));
        }
    }

    if let Some(review) = review_prompt {
        prompt.push_str(&format!(
            "## ADDITIONAL REVIEW INSTRUCTIONS\n\n\
             After completing the work, also review your changes for: {review}\n\n"
        ));
    }

    prompt.push_str("## ITERATION NOTES\n\n");

    if notes_exist {
        prompt.push_str(&format!(
            "Update the `{notes_file}` file with relevant context for the next iteration. \
             Add new notes and remove outdated information to keep it current and useful."
        ));
    } else {
        prompt.push_str(&format!(
            "Create a `{notes_file}` file with relevant context and instructions for the next iteration."
        ));
    }

    prompt.push_str(NOTES_GUIDELINES);
    prompt
}

/// Build the prompt for handling an in-review issue (CI failures + review comments).
#[allow(clippy::too_many_arguments)]
pub fn build_review_prompt(
    identifier: &str,
    title: &str,
    pr_number: u64,
    owner: &str,
    repo: &str,
    branch: &str,
    ci_failures: Option<&str>,
    inline_comments: &str,
    review_bodies: &str,
    conversation_comments: &str,
) -> String {
    let mut prompt = format!(
        "## CODE REVIEW RESOLUTION: {identifier} -- {title}\n\n\
         You are resolving review comments and CI failures on PR #{pr_number} in {owner}/{repo} (branch: {branch}).\n\n\
         **When you're done**: Stage all changes with `git add .`, commit with a clear message, and push the branch."
    );

    if let Some(ci) = ci_failures {
        prompt.push_str(&format!(
            "\n\n### CI Failures\nThe following CI checks are failing. Fix these errors:\n\n{ci}"
        ));
    }

    let has_comments = !inline_comments.is_empty()
        || !review_bodies.is_empty()
        || !conversation_comments.is_empty();

    if has_comments {
        prompt.push_str(&format!(
            "\n\n### Inline Code Review Comments\n{inline_comments}\n\n\
             ### Review Bodies\n{review_bodies}\n\n\
             ### General Conversation Comments\n{conversation_comments}"
        ));
    }

    prompt.push_str(
        "\n\n## INSTRUCTIONS\n\n\
         1. Read through all the review comments and CI failures above\n\
         2. Address each piece of feedback by making the appropriate code changes\n\
         3. Fix any CI failures -- read the error logs carefully and fix the root cause\n\
         4. If a comment is unclear or you disagree, make your best judgment\n\
         5. Focus on addressing the feedback and fixing CI, not adding unrelated changes\n\
         6. When done, stage all changes, commit with a clear message, and push the branch",
    );

    prompt
}

/// Build the issue prompt section for a Linear issue.
pub fn build_issue_prompt(identifier: &str, title: &str, description: &str) -> String {
    format!(
        "## LINEAR ISSUE: {identifier} -- {title}\n\n\
         {description}\n\n\
         ## INSTRUCTIONS\n\n\
         Implement the changes described in this Linear issue. Focus on making meaningful, well-tested progress."
    )
}

pub fn notes_file_path(notes_dir: &Path, identifier: &str) -> String {
    notes_dir.join(format!("{identifier}.md")).to_string_lossy().to_string()
}
