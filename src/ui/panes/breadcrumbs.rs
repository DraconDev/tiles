use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::Paragraph,
    Frame,
};
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, AppMode, CurrentView, DropTarget};
use terma::utils::{get_visual_width, squarify, truncate_to_width};

pub fn draw_pane_breadcrumbs(f: &mut Frame, area: Rect, app: &mut App, pane_idx: usize) {
    let _is_focused = pane_idx == app.focused_pane_index && !app.sidebar_focus;

    let active_tab_idx = app.panes[pane_idx].active_tab_index;
    let (mut path, mut search_filter) = {
        let tab = &app.panes[pane_idx].tabs[active_tab_idx];
        (tab.current_path.clone(), tab.search_filter.clone())
    };

    if app.current_view == CurrentView::Editor {
        if let Some(pane) = app.panes.get(pane_idx) {
            if let Some(preview) = &pane.preview {
                path = preview.path.clone();
            }
        }
    }

    let mut search_label = "";
    let mut search_color = Color::Cyan;

    // IDE Mode Search Integration
    if app.current_view == CurrentView::Editor && search_filter.is_empty() {
        if let Some(pane) = app.panes.get(pane_idx) {
            if let Some(preview) = &pane.preview {
                if let Some(editor) = &preview.editor {
                    if _is_focused {
                        match app.mode {
                            AppMode::EditorSearch => {
                                search_filter = app.input.value.clone();
                                search_label = "";
                            }
                            AppMode::EditorGoToLine => {
                                search_filter = app.input.value.clone();
                                search_label = "LINE: ";
                            }
                            AppMode::EditorReplace => {
                                search_filter = app.input.value.clone();
                                search_label = if app.replace_buffer.is_empty() {
                                    ""
                                } else {
                                    "WITH: "
                                };
                                search_color = Color::Magenta;
                            }
                            _ => {
                                if !editor.filter_query.is_empty() {
                                    search_filter = editor.filter_query.clone();
                                    search_label = "";
                                }
                            }
                        }
                    } else if !editor.filter_query.is_empty() {
                        search_filter = editor.filter_query.clone();
                        search_label = "";
                    }
                }
            }
        }
    }

    if let Some(tab) = app.panes[pane_idx].tabs.get_mut(active_tab_idx) {
        tab.breadcrumb_bounds.clear();
        tab.breadcrumb_header_bounds = Some(area);
        // Track the top border row separately for "copy path" click detection.
        // The border is one row above the inner area where breadcrumbs render.
        tab.breadcrumb_border_bounds = Some(Rect::new(area.x, area.y, area.width, 1));
    }

    if _is_focused
        && app.current_view == CurrentView::Files
        && matches!(app.mode, AppMode::PathInput)
    {
        f.render_widget(&app.input, area);
        return;
    }

    let mut cur_p = PathBuf::new();
    let breadcrumb_y = area.y;
    let mut cur_x = area.x + 2;

    let components: Vec<_> = path.components().collect();
    let total_comps = components.len();

    let search_filter_text = if !search_filter.is_empty() {
        format!(" [{}{}]", search_label, search_filter.trim())
    } else {
        String::new()
    };
    let search_filter_width = search_filter_text
        .chars()
        .map(get_visual_width)
        .sum::<usize>() as u16;
    let max_header_width = area.width.saturating_sub(search_filter_width + 10);

    let mut accumulated_width = 0;

    for (i, comp) in components.into_iter().enumerate() {
        match comp {
            std::path::Component::RootDir => cur_p.push("/"),
            std::path::Component::Prefix(p) => cur_p.push(p.as_os_str()),
            std::path::Component::Normal(name) => cur_p.push(name),
            _ => continue,
        }
        let d_name = if comp.as_os_str() == "/" {
            "/".to_string()
        } else {
            squarify(&comp.as_os_str().to_string_lossy())
        };
        if !d_name.is_empty() {
            let s_path = cur_p.clone();
            let is_last = i == total_comps - 1;

            let fg_color = if is_last {
                crate::ui::theme::accent_secondary()
            } else {
                Color::Rgb(100, 100, 110)
            };
            let mut style = Style::default().fg(fg_color);
            if is_last {
                style = style.add_modifier(Modifier::BOLD);
            }
            if matches!(&app.hovered_drop_target, Some(DropTarget::Folder(p)) if p == &s_path)
                && app.is_dragging
            {
                style = style
                    .bg(crate::ui::theme::accent_secondary())
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD);
            }

            let d_name_clipped = if d_name.len() > 15 && !is_last {
                truncate_to_width(&d_name, 15, "...")
            } else {
                d_name
            };

            let segment = if is_last {
                format!("  {}  ", d_name_clipped)
            } else {
                format!(" {} ", d_name_clipped)
            };
            let width = segment.chars().map(get_visual_width).sum::<usize>() as u16;

            if (accumulated_width + width) > max_header_width {
                f.render_widget(Paragraph::new("..."), Rect::new(cur_x, breadcrumb_y, 3, 1));
                break;
            }

            let bread_rect = Rect::new(cur_x, breadcrumb_y, width, 1);
            f.render_widget(Paragraph::new(Span::styled(segment, style)), bread_rect);

            if let Some(tab) = app.panes[pane_idx].tabs.get_mut(active_tab_idx) {
                tab.breadcrumb_bounds.push((bread_rect, s_path));
            }

            cur_x += width;
            accumulated_width += width;

            if !is_last {
                let sep = "›";
                let sep_w = 1;
                if (accumulated_width + sep_w) <= max_header_width {
                    f.render_widget(
                        Paragraph::new(Span::styled(
                            sep,
                            Style::default().fg(Color::Rgb(80, 80, 90)),
                        )),
                        Rect::new(cur_x, breadcrumb_y, 1, 1),
                    );
                    cur_x += sep_w;
                    accumulated_width += sep_w;
                }
            }
        }
    }

    if !search_filter_text.is_empty() {
        let max_filter_w = area.right().saturating_sub(cur_x + 2) as usize;
        let display_filter = if search_filter_text.width() > max_filter_w {
            truncate_to_width(&search_filter_text, max_filter_w, "..]")
        } else {
            search_filter_text
        };

        let filter_rect = Rect::new(cur_x + 1, area.y, display_filter.width() as u16, 1);
        f.render_widget(
            Paragraph::new(Span::styled(
                display_filter,
                Style::default()
                    .fg(search_color)
                    .add_modifier(Modifier::BOLD),
            )),
            filter_rect,
        );
    }
}
