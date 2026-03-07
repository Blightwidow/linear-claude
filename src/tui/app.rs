use std::sync::mpsc;
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
    pub parser: vt100::Parser,
    pub parser_rows: u16,
    pub parser_cols: u16,
    pub log_lines: Vec<String>,
    /// When Some, Claude is running and keystrokes should be forwarded here
    pub pty_input_tx: Option<mpsc::Sender<Vec<u8>>>,
    pub total_issues: usize,
    pub done_count: usize,
    pub failed_count: usize,
    pub skipped_count: usize,
    pub current_branch: Option<String>,
    pub start_time: Instant,
    pub should_quit: bool,
    pub worker_done: bool,
}

impl App {
    pub fn new(issues: Vec<IssueEntry>) -> Self {
        let (term_cols, term_rows) = crossterm::terminal::size().unwrap_or((120, 40));
        // Approximate the right panel inner size: 75% width minus borders, full height minus footer and borders
        let parser_cols = (((term_cols as f32) * 0.75) as u16).saturating_sub(2);
        let parser_rows = term_rows.saturating_sub(3);

        let total_issues = issues.len();
        App {
            issues,
            parser: vt100::Parser::new(parser_rows, parser_cols, 10000),
            parser_rows,
            parser_cols,
            log_lines: Vec::new(),
            pty_input_tx: None,
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

    pub fn push_log_line(&mut self, line: String) {
        self.log_lines.push(line);
        if self.log_lines.len() > 500 {
            self.log_lines.drain(..self.log_lines.len() - 500);
        }
    }

    /// Update tracked panel dimensions. The new size takes effect on the next parser reset.
    pub fn update_panel_size(&mut self, rows: u16, cols: u16) {
        self.parser_rows = rows;
        self.parser_cols = cols;
    }

    /// Reset the parser for a new issue, using current panel dimensions.
    pub fn reset_parser(&mut self) {
        self.parser = vt100::Parser::new(self.parser_rows, self.parser_cols, 10000);
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
