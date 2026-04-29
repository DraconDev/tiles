use ratatui::{
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
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

fn editor_welcome_content(dir_name: &str) -> String {
    format!(
        "\n\n   << PROJECT: {} >>\n\n   Select a file from the sidebar to begin editing.",
        dir_name
    )
}

pub fn draw_pane_editor(
    f: &mut Frame,
    area: Rect,
    app: &mut App,
    pane_idx: usize,
    is_focused: bool,
) {
    let (title, welcome_path) = if let Some(pane) = app.panes.get(pane_idx) {
        if let Some(fs) = pane.current_state() {
            if let Some(preview) = &fs.preview {
                (
                    Line::from(vec![Span::styled(
                        format!(" {} ", preview.path.to_string_lossy()),
                        Style::default().fg(crate::ui::theme::accent_secondary()),
                    )]),
                    None,
                )
        } else {
            let current_dir = pane.current_state().map(|fs| fs.current_path.clone());
            if let Some(ref path) = current_dir {
                let dir_name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".to_string());
                (
                    Line::from(vec![Span::styled(
                        format!(" {} ", path.to_string_lossy()),
                        Style::default().fg(crate::ui::theme::accent_secondary()),
                    )]),
                    Some(dir_name),
                )
            } else {
                (
                    Line::from(vec![Span::styled(
                        " (no file) ",
                        Style::default().fg(crate::ui::theme::border_inactive()),
                    )]),
                    None,
                )
            }
        }
    } else {
        (
            Line::from(vec![Span::styled(
                " (no file) ",
                Style::default().fg(crate::ui::theme::border_inactive()),
            )]),
            None,
        )
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
        if let Some(fs) = pane.current_state_mut() {
            if let Some(preview) = &mut fs.preview {
                if let Some(editor) = &mut preview.editor {
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
        } else if let Some(dir_name) = welcome_path {
            let style = Style::default()
                .fg(crate::ui::theme::accent_primary())
                .add_modifier(ratatui::style::Modifier::BOLD);
            let para = Paragraph::new(editor_welcome_content(&dir_name))
                .style(style)
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(para, inner);
        }
    }
}
