use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{
        Block, BorderType, Borders, Paragraph,
    },
    Frame,
};

use crate::app::{
    App,
};

pub fn draw_editor_stage(f: &mut Frame, area: Rect, app: &mut App) {
    let pane_count = app.panes.len();
    if pane_count == 0 {
        return;
    }

    let constraints = vec![Constraint::Fill(1); pane_count];
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .spacing(0)
        .split(area);

    for i in 0..pane_count {
        let is_focused = i == app.focused_pane_index && !app.sidebar_focus;
        draw_pane_editor(f, chunks[i], app, i, is_focused);
    }
}

pub fn draw_pane_editor(f: &mut Frame, area: Rect, app: &mut App, pane_idx: usize, is_focused: bool) {
    let mut border_color = if is_focused {
        Color::Rgb(255, 0, 85) // Neon Red/Pink
    } else {
        Color::Rgb(80, 0, 0) // Dim Red
    };

    if let Some(pane) = app.panes.get(pane_idx) {
        if let Some(preview) = &pane.preview {
            if let Some(last_saved) = preview.last_saved {
                if last_saved.elapsed().as_secs() < 2 {
                    border_color = Color::Green;
                }
            }
        }
    }

    let mut border_style = Style::default().fg(border_color);
    if is_focused {
        border_style = border_style.add_modifier(Modifier::BOLD);
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Call breadcrumbs BEFORE mutably borrowing the pane
    crate::ui::panes::breadcrumbs::draw_pane_breadcrumbs(f, area, app, pane_idx);

    let pane = &mut app.panes[pane_idx];
    
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Breadcrumb space
            Constraint::Fill(1),   // Editor Area
        ])
        .split(inner);

    // Apply 2-char right margin/padding to editor area
    let editor_area = Rect {
        x: chunks[1].x,
        y: chunks[1].y,
        width: chunks[1].width.saturating_sub(2),
        height: chunks[1].height,
    };

    if let Some(preview) = &mut pane.preview {
        if let Some(editor) = &preview.editor {
            f.render_widget(editor, editor_area);
        }
    } else {
        f.render_widget(
            Paragraph::new("

 Select a file from the sidebar to edit.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray)),
            editor_area
        );
    }
}
