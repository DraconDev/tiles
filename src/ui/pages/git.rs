use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Row, Table,
    },
    Frame,
};

use crate::app::{
    App,
};
use crate::ui::theme::THEME;

pub fn draw_git_page(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .title_top(Line::from(vec![
            Span::styled(" GIT HISTORY ", Style::default().fg(Color::Black).bg(THEME.accent_primary).add_modifier(Modifier::BOLD)),
        ]))
        .title_top(Line::from(vec![
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Back ", Style::default().fg(Color::Red)),
        ]).alignment(Alignment::Right))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_primary));
    
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let pane = &mut app.panes[app.focused_pane_index];
    let tab = &mut pane.tabs[pane.active_tab_index];

    let header_cells = ["Hash", "Date", "Author", "Message"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells).height(1).bottom_margin(0);

    let rows = tab.git_history.iter().enumerate().map(|(i, commit)| {
        let mut style = if i % 2 == 0 {
            Style::default().fg(Color::Rgb(180, 185, 190))
        } else {
            Style::default().fg(Color::Rgb(140, 145, 150))
        };

        if Some(i) == tab.git_history_state.selected() {
            style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
        }

        Row::new(vec![
            Cell::from(commit.hash.chars().take(7).collect::<String>()).style(Style::default().fg(Color::Yellow)),
            Cell::from(commit.date.clone()),
            Cell::from(commit.author.clone()).style(Style::default().fg(THEME.accent_secondary)),
            Cell::from(commit.message.clone()),
        ]).style(style)
    });

    let table = Table::new(rows, [
        Constraint::Length(10),
        Constraint::Length(20),
        Constraint::Length(20),
        Constraint::Min(40),
    ])
    .header(header)
    .column_spacing(2);

    f.render_stateful_widget(table, chunks[1], &mut tab.git_history_state);
}
