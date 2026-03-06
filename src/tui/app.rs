use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum IssueDisplayStatus {
    Queued,
    Running,
    Done,
    Failed,
    Skipped,
}

impl IssueDisplayStatus {
    pub fn symbol(&self) -> &str {
        match self {
            IssueDisplayStatus::Queued => "..",
            IssueDisplayStatus::Running => ">>",
            IssueDisplayStatus::Done => "ok",
            IssueDisplayStatus::Failed => "!!",
            IssueDisplayStatus::Skipped => "--",
        }
    }
}

#[derive(Debug, Clone)]
pub struct IssueEntry {
    pub identifier: String,
    pub title: String,
    pub status: IssueDisplayStatus,
}

pub struct App {
    pub issues: Vec<IssueEntry>,
    pub output_lines: Vec<String>,
    pub log_lines: Vec<String>,
    pub scroll_offset: u16,
    pub auto_scroll: bool,
    pub total_issues: usize,
    pub done_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub current_branch: Option<String>,
    pub start_time: Instant,
    pub should_quit: bool,
    pub worker_done: bool,
}

const MAX_OUTPUT_LINES: usize = 10_000;

impl App {
    pub fn new(issues: Vec<IssueEntry>) -> Self {
        let total_issues = issues.len();
        App {
            issues,
            output_lines: Vec::new(),
            log_lines: Vec::new(),
            scroll_offset: 0,
            auto_scroll: true,
            total_issues,
            done_count: 0,
            failed_count: 0,
            skipped_count: 0,
            current_branch: None,
            start_time: Instant::now(),
            should_quit: false,
            worker_done: false,
        }
    }

    pub fn push_output_line(&mut self, line: String) {
        self.output_lines.push(line);
        if self.output_lines.len() > MAX_OUTPUT_LINES {
            let drain = self.output_lines.len() - MAX_OUTPUT_LINES;
            self.output_lines.drain(..drain);
            self.scroll_offset = self.scroll_offset.saturating_sub(drain as u16);
        }
    }

    pub fn push_log_line(&mut self, line: String) {
        self.log_lines.push(line);
        if self.log_lines.len() > 500 {
            self.log_lines.drain(..self.log_lines.len() - 500);
        }
    }

    pub fn clear_output(&mut self) {
        self.output_lines.clear();
        self.scroll_offset = 0;
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.auto_scroll = false;
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    pub fn scroll_down(&mut self, amount: u16, visible_height: u16) {
        let max_scroll = (self.output_lines.len() as u16).saturating_sub(visible_height);
        self.scroll_offset = (self.scroll_offset + amount).min(max_scroll);
    }

    pub fn scroll_to_bottom(&mut self) {
        self.auto_scroll = true;
    }

    pub fn elapsed_display(&self) -> String {
        let secs = self.start_time.elapsed().as_secs();
        crate::duration::format_duration(secs)
    }

    pub fn progress_display(&self) -> String {
        let processed = self.done_count + self.failed_count + self.skipped_count;
        format!("{}/{}", processed, self.total_issues)
    }
}
