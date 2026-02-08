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
    let pc = app.panes.len();
    if pc == 0 { return; }
    
    let pw = area.width / pc as u16;

    for i in 0..pc {
        let pane_area = Rect::new(
            area.x + (i as u16 * pw),
            area.y,
            pw,
            area.height,
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