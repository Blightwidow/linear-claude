use crate::tui::app::{App, IssueDisplayStatus};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let [main_area, footer] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let [left, right] = Layout::horizontal([
        Constraint::Min(24),
        Constraint::Percentage(75),
    ])
    .areas(main_area);

    let [issue_list_area, stats_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(7),
    ])
    .areas(left);

    // Issue list
    draw_issue_list(frame, app, issue_list_area);

    // Stats panel
    draw_stats(frame, app, stats_area);

    // Claude output panel
    let output_height = right.height.saturating_sub(2); // borders
    draw_output(frame, app, right, output_height);

    // Footer
    draw_footer(frame, footer);
}

fn draw_issue_list(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let items: Vec<ListItem> = app
        .issues
        .iter()
        .map(|entry| {
            let style = match entry.status {
                IssueDisplayStatus::Done => Style::default().fg(Color::Green),
                IssueDisplayStatus::Running => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                IssueDisplayStatus::Failed => Style::default().fg(Color::Red),
                IssueDisplayStatus::Skipped => Style::default().fg(Color::DarkGray),
                IssueDisplayStatus::Queued => Style::default().fg(Color::Gray),
            };

            let symbol = entry.status.symbol();
            let text = format!("[{}] {} {}", symbol, entry.identifier, entry.title);
            ListItem::new(text).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Issues "),
    );

    frame.render_widget(list, area);
}

fn draw_stats(frame: &mut Frame, app: &App, area: ratatui::layout::Rect) {
    let progress = app.progress_display();
    let elapsed = app.elapsed_display();
    let branch = app.current_branch.as_deref().unwrap_or("-");

    let text = vec![
        Line::from(vec![
            Span::styled(" Progress: ", Style::default().fg(Color::Cyan)),
            Span::raw(&progress),
        ]),
        Line::from(vec![
            Span::styled(" Done:     ", Style::default().fg(Color::Green)),
            Span::raw(app.done_count.to_string()),
        ]),
        Line::from(vec![
            Span::styled(" Failed:   ", Style::default().fg(Color::Red)),
            Span::raw(app.failed_count.to_string()),
        ]),
        Line::from(vec![
            Span::styled(" Elapsed:  ", Style::default().fg(Color::Cyan)),
            Span::raw(&elapsed),
        ]),
        Line::from(vec![
            Span::styled(" Branch:   ", Style::default().fg(Color::Cyan)),
            Span::raw(branch),
        ]),
    ];

    let para = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Stats "),
    );

    frame.render_widget(para, area);
}

fn draw_output(frame: &mut Frame, app: &mut App, area: ratatui::layout::Rect, visible_height: u16) {
    let total_lines = app.output_lines.len() as u16;

    // Auto-scroll: keep scroll at bottom
    if app.auto_scroll {
        app.scroll_offset = total_lines.saturating_sub(visible_height);
    }

    let para = Paragraph::new(
        app.output_lines
            .iter()
            .map(|l| Line::raw(l.as_str()))
            .collect::<Vec<_>>(),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Claude Output "),
    )
    .wrap(Wrap { trim: false })
    .scroll((app.scroll_offset, 0));

    frame.render_widget(para, area);
}

fn draw_footer(frame: &mut Frame, area: ratatui::layout::Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(": quit  "),
        Span::styled("s", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(": skip  "),
        Span::styled("↑/↓", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(": scroll  "),
        Span::styled("PgUp/PgDn", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(": page  "),
        Span::styled("End", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(": follow  "),
        Span::styled("Home", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(": top"),
    ]));

    frame.render_widget(footer, area);
}

/// Returns the visible height of the output panel (for scroll calculations).
pub fn output_visible_height(terminal_height: u16) -> u16 {
    // main area = terminal_height - 1 (footer)
    // right panel has borders = -2
    terminal_height.saturating_sub(1).saturating_sub(2)
}
