use crate::tui::app::{App, IssueDisplayStatus};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use tui_term::widget::PseudoTerminal;

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

    // Track panel dimensions for parser resize on next reset
    let inner_rows = right.height.saturating_sub(2);
    let inner_cols = right.width.saturating_sub(2);
    app.update_panel_size(inner_rows, inner_cols);

    // Issue list
    draw_issue_list(frame, app, issue_list_area);

    // Stats panel
    draw_stats(frame, app, stats_area);

    // Claude terminal panel
    let screen = app.parser.screen();
    let pseudo_term = PseudoTerminal::new(screen).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Claude "),
    );
    frame.render_widget(pseudo_term, right);

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

fn draw_footer(frame: &mut Frame, area: ratatui::layout::Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(" q", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(": quit  "),
        Span::styled("s", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(": skip"),
    ]));

    frame.render_widget(footer, area);
}
