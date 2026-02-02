use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Cell, Row, Table, Paragraph,
    },
    Frame,
};

use crate::app::App;
use crate::ui::theme::THEME;

pub fn draw_git_page(f: &mut Frame, area: Rect, app: &mut App) {
    let pane_idx = app.focused_pane_index;
    let tab_idx = if let Some(pane) = app.panes.get(pane_idx) {
        pane.active_tab_index
    } else {
        0
    };

    let (history, pending, _current_path, branch) = if let Some(pane) = app.panes.get(pane_idx) {
        if let Some(tab) = pane.tabs.get(tab_idx) {
            (&tab.git_history, &tab.git_pending, tab.current_path.clone(), tab.git_branch.clone())
        } else {
            return;
        }
    } else {
        return;
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_primary))
        .title_top(Line::from(vec![
            Span::styled(" GIT HISTORY ", Style::default().fg(Color::Black).bg(THEME.accent_primary).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(format!("({})", branch.as_ref().map(|s| s.as_str()).unwrap_or("HEAD")), Style::default().fg(Color::Yellow)),
        ]))
        .title_top(Line::from(vec![
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Back ", Style::default().fg(Color::Red)),
        ]).alignment(Alignment::Right));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(if pending.is_empty() { 0 } else { (pending.len() as u16 + 2).min(inner.height / 3) }),
            Constraint::Min(0),
        ])
        .split(inner);

    // 1. Pending Changes
    if !pending.is_empty() {
        let pending_rows: Vec<_> = pending.iter().map(|p| {
            let status_color = match p.status.as_str() {
                "M" => Color::Yellow,
                "A" | "??" => Color::Green,
                "D" => Color::Red,
                "R" => Color::Cyan,
                _ => Color::White,
            };
            Row::new(vec![
                Cell::from(p.status.clone()).style(Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
                Cell::from(p.path.clone()).style(Style::default().fg(THEME.fg)),
            ])
        }).collect();

        let pending_table = Table::new(pending_rows, [Constraint::Length(6), Constraint::Fill(1)])
            .header(Row::new(vec!["STATUS", "PATH"]).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)))
            .block(Block::default().borders(Borders::BOTTOM).title(" PENDING CHANGES ").border_style(Style::default().fg(Color::Rgb(40, 45, 55))));
        f.render_widget(pending_table, chunks[0]);
    }

    // 2. History
    if history.is_empty() {
        f.render_widget(
            Paragraph::new("\n\n No git history found for this path or not a git repository.")
                .alignment(Alignment::Center),
            chunks[1],
        );
        return;
    }

    let rows: Vec<_> = history
        .iter()
        .map(|act| {
            let h_short = act.hash.chars().take(7).collect::<String>();
            let stats = if act.files_changed > 0 {
                format!("{} files (+{}/-{})", act.files_changed, act.insertions, act.deletions)
            } else {
                String::new()
            };
            
            Row::new(vec![
                Cell::from(act.date.clone()).style(Style::default().fg(Color::DarkGray)),
                Cell::from(h_short).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)),
                Cell::from(act.author.clone()).style(Style::default().fg(Color::Cyan)),
                Cell::from(act.message.clone()).style(Style::default().fg(THEME.fg)),
                Cell::from(stats).style(Style::default().fg(Color::Rgb(100, 100, 110))),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(25), // DATE
            Constraint::Length(12), // HASH
            Constraint::Length(20), // AUTHOR
            Constraint::Fill(1),    // MESSAGE
            Constraint::Length(25), // STATS
        ],
    )
    .header(
        Row::new(vec!["DATE", "HASH", "AUTHOR", "MESSAGE", "STATS"])
            .style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(Block::default().title(" HISTORY "))
    .row_highlight_style(
        Style::default()
            .bg(Color::Rgb(40, 40, 50))
            .fg(THEME.accent_secondary)
            .add_modifier(Modifier::BOLD),
    )
    .highlight_symbol(" 󰁅 ");

    if let Some(pane) = app.panes.get_mut(pane_idx) {
        if let Some(tab) = pane.tabs.get_mut(tab_idx) {
            f.render_stateful_widget(table, chunks[1], &mut tab.git_history_state);
        }
    }
}