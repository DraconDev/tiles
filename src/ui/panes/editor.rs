use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
    Frame,
};

use crate::app::App;

pub fn draw_ide_editor(f: &mut Frame, area: Rect, app: &mut App) {
    let pc = app.panes.len();
    if pc == 0 {
        return;
    }

    for i in 0..pc {
        let pw = area.width / pc as u16;
        if pw == 0 {
            return;
        }
        let pane_x = area.x + (i as u16 * pw);
        let pane_w = if i + 1 == pc {
            area.x.saturating_add(area.width).saturating_sub(pane_x)
        } else {
            pw
        };
        let pane_area = Rect::new(pane_x, area.y, pane_w, area.height);
        let is_focused = app.focused_pane_index == i;
        draw_pane_editor(f, pane_area, app, i, is_focused);
    }
}

pub fn draw_pane_editor(
    f: &mut Frame,
    area: Rect,
    app: &mut App,
    pane_idx: usize,
    is_focused: bool,
) {
    let title = if let Some(pane) = app.panes.get(pane_idx) {
        if let Some(preview) = &pane.preview {
            Line::from(vec![Span::styled(
                format!(" {} ", preview.path.to_string_lossy()),
                Style::default().fg(crate::ui::theme::accent_secondary()),
            )])
        } else {
            Line::from(vec![Span::styled(
                " (no file) ",
                Style::default().fg(crate::ui::theme::border_inactive()),
            )])
        }
    } else {
        Line::from(vec![Span::styled(
            " (no file) ",
            Style::default().fg(crate::ui::theme::border_inactive()),
        )])
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title_top(title)
        .border_style(if is_focused {
            Style::default().fg(crate::ui::theme::border_active())
        } else {
            Style::default().fg(crate::ui::theme::border_inactive())
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
                    preview
                        .path
                        .extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string()
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
