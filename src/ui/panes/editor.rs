use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{
        Block, Borders,
    },
    Frame,
};

use crate::app::{
    App,
};
use crate::ui::theme::THEME;

pub fn draw_ide_editor(f: &mut Frame, area: Rect, app: &mut App) {
    let sw = app.sidebar_width();
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(sw),
            Constraint::Fill(1),
        ])
        .split(area);

    if app.show_sidebar {
        crate::ui::panes::sidebar::draw_sidebar(f, chunks[0], app);
    }

    let pc = app.panes.len();
    let pw = if pc > 0 { chunks[1].width / pc as u16 } else { chunks[1].width };

    for i in 0..pc {
        let pane_area = Rect::new(
            chunks[1].x + (i as u16 * pw),
            chunks[1].y,
            pw,
            chunks[1].height,
        );
        let is_focused = app.focused_pane_index == i;
        draw_pane_editor(f, pane_area, app, i, is_focused);
    }
}

pub fn draw_pane_editor(f: &mut Frame, area: Rect, app: &mut App, pane_idx: usize, is_focused: bool) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if is_focused {
            Style::default().fg(THEME.border_active)
        } else {
            Style::default().fg(THEME.border_inactive)
        });

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(pane) = app.panes.get_mut(pane_idx) {
        if let Some(preview) = &mut pane.preview {
            if let Some(editor) = &mut preview.editor {
                // Ensure language is set for syntax highlighting
                let path_str = preview.path.to_string_lossy();
                let ext = if path_str.starts_with("git://") {
                    "diff".to_string()
                } else {
                    preview.path.extension().and_then(|s| s.to_str()).unwrap_or("").to_string()
                };

                if editor.language != ext {
                    editor.language = ext;
                    editor.invalidate_from(0);
                }
                editor.wrap = app.is_split_mode;
                f.render_widget(&*editor, inner);
            }
        }
    }
}

use ratatui::style::Style;