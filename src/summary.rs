use crate::duration::format_duration;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct SummaryEntry {
    pub identifier: String,
    pub title: String,
    pub result: SummaryResult,
    pub branch: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SummaryResult {
    Done,
    Skip,
    Fail,
}

impl SummaryResult {
    pub fn as_str(&self) -> &str {
        match self {
            SummaryResult::Done => "Done",
            SummaryResult::Skip => "Skip",
            SummaryResult::Fail => "Fail",
        }
    }
}

pub fn show_completion_summary(
    entries: &[SummaryEntry],
    unpushed_branches: &[String],
    start_time: Option<Instant>,
) {
    let elapsed_msg = start_time.map(|t| {
        let secs = t.elapsed().as_secs();
        format!(" (elapsed: {})", format_duration(secs))
    }).unwrap_or_default();

    eprintln!();
    eprintln!("========================================");
    eprintln!("  LINEAR-CLAUDE SUMMARY{elapsed_msg}");
    eprintln!("========================================");
    eprintln!();

    if !entries.is_empty() {
        // Calculate column widths
        let mut max_id = 7usize; // "Issue".len() + padding
        let mut max_title = 5usize;
        let mut max_result = 6usize;
        let mut max_branch = 6usize;

        for entry in entries {
            max_id = max_id.max(entry.identifier.len());
            let display_title = truncate(&entry.title, 30);
            max_title = max_title.max(display_title.len());
            max_result = max_result.max(entry.result.as_str().len());
            if let Some(b) = &entry.branch {
                max_branch = max_branch.max(b.len());
            }
        }

        max_title = max_title.min(30);
        max_branch = max_branch.min(40);

        // Header
        eprintln!(
            "  {:<id_w$}  {:<title_w$}  {:<result_w$}  {:<branch_w$}",
            "Issue",
            "Title",
            "Result",
            "Branch",
            id_w = max_id,
            title_w = max_title,
            result_w = max_result,
            branch_w = max_branch
        );

        let line_len = max_id + max_title + max_result + max_branch + 6;
        eprintln!("  {}", "-".repeat(line_len));

        // Rows
        for entry in entries {
            let display_title = truncate(&entry.title, 30);
            let branch = entry.branch.as_deref().unwrap_or("-");
            eprintln!(
                "  {:<id_w$}  {:<title_w$}  {:<result_w$}  {:<branch_w$}",
                entry.identifier,
                display_title,
                entry.result.as_str(),
                branch,
                id_w = max_id,
                title_w = max_title,
                result_w = max_result,
                branch_w = max_branch
            );
        }
        eprintln!();
    }

    if !unpushed_branches.is_empty() {
        eprintln!("Warning: The following branches were not pushed and need attention:");
        for branch in unpushed_branches {
            eprintln!("   - {branch}");
        }
        eprintln!();
    }

    eprintln!("Done.{elapsed_msg}");
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max.saturating_sub(3)])
    } else {
        s.to_string()
    }
}
