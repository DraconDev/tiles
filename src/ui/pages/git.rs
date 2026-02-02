use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Row, Table, Paragraph,
    },
    Frame,
};

use crate::app::{
    App, CurrentView, GitStatus, CommitInfo,
};
use crate::ui::theme::THEME;

pub fn draw_git_page(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .title(" GIT HISTORY ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_primary));
    
    let (history, pending, current_path, branch) = if let Some(fs) = app.current_file_state() {
        (&fs.git_history, &fs.git_pending, fs.current_path.clone(), fs.git_branch.clone())
    } else {
        return;
    };

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header info
            Constraint::Length(if pending.is_empty() { 0 } else { (pending.len() as u16 + 2).min(inner.height / 3) }),
            Constraint::Fill(1),   // History
        ])
        .split(inner);

    // Header Info
    let header = Line::from(vec![
        Span::styled("Path: ", Style::default().fg(THEME.accent_secondary)),
        Span::raw(current_path.to_string_lossy().to_string()),
        Span::raw("  "),
        Span::styled("Branch: ", Style::default().fg(THEME.accent_secondary)),
        Span::styled(format!("({})", branch.unwrap_or_else(|| "HEAD".to_string())), Style::default().fg(Color::Yellow)),
    ]);
    f.render_widget(Paragraph::new(header), chunks[0]);

    // Pending Changes (if any)
    if !pending.is_empty() {
        let items: Vec<Line> = pending.iter().map(|s| {
            let (sym, col) = match s {
                GitStatus::Modified => ("M", Color::Yellow),
                GitStatus::Added => ("A", Color::Green),
                GitStatus::Deleted => ("D", Color::Red),
                GitStatus::Renamed => ("R", Color::Blue),
                GitStatus::Untracked => ("?", Color::Magenta),
                GitStatus::Staged => ("S", Color::Green),
                GitStatus::Conflict => ("!", Color::Red),
            };
            Line::from(vec![Span::styled(format!(" {} ", sym), Style::default().bg(col).fg(Color::Black)), Span::raw(" Change")])
        }).collect();
        f.render_widget(Paragraph::new(items).block(Block::default().title(" Pending Changes ").borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray))), chunks[1]);
    }

    // History Table
    if history.is_empty() {
        f.render_widget(Paragraph::new("No git history found or not a git repository.").alignment(ratatui::layout::Alignment::Center), chunks[2]);
    } else {
        let rows = history.iter().map(|c| {
            Row::new(vec![
                Cell::from(Span::styled(&c.hash[..7], Style::default().fg(Color::DarkGray))),
                Cell::from(c.date.clone()),
                Cell::from(c.author.clone()),
                Cell::from(c.message.clone()),
            ])
        });

        let table = Table::new(
            rows,
            [
                Constraint::Length(8),
                Constraint::Length(20),
                Constraint::Length(20),
                Constraint::Fill(1),
            ]
        )
        .header(Row::new(vec!["HASH", "DATE", "AUTHOR", "MESSAGE"]).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)))
        .highlight_style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD));

        if let Some(fs) = app.current_file_state_mut() {
            f.render_stateful_widget(table, chunks[2], &mut fs.git_history_state);
        }
    }
}
