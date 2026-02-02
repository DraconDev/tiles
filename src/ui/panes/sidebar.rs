use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, List, ListItem,
    },
    Frame,
};
use std::collections::HashMap;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

use crate::app::{
    App, DropTarget, SidebarBounds, SidebarTarget, CurrentView,
};
use crate::icons::Icon;
use crate::ui::theme::THEME;

pub fn draw_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    match app.current_view {
        CurrentView::Files => {
            let mut sidebar_items = Vec::new();
            app.sidebar_bounds.clear();
            let mut current_y = inner.y;

            // 1. Collect markers ONLY for the active (visible) tab of each PANE
            let mut active_storage_markers: HashMap<String, Vec<usize>> = HashMap::new();
            let mut active_remote_markers: HashMap<String, Vec<usize>> = HashMap::new();

            for (p_idx, pane) in app.panes.iter().enumerate() {
                let panel_num = p_idx + 1; // 1 for Left, 2 for Right
                if let Some(fs) = pane.current_state() {
                    if let Some(ref session) = fs.remote_session {
                        active_remote_markers
                            .entry(session.host.clone())
                            .or_default()
                            .push(panel_num);
                    } else {
                        // Check Storage
                        let mut matched_disk = None;
                        let mut longest_prefix = 0;

                        for disk in &app.system_state.disks {
                            if disk.is_mounted {
                                if fs.current_path.starts_with(&disk.name) {
                                    let len = disk.name.len();
                                    if len > longest_prefix {
                                        longest_prefix = len;
                                        matched_disk = Some(disk.name.clone());
                                    }
                                }
                            }
                        }

                        if let Some(name) = matched_disk {
                            active_storage_markers
                                .entry(name)
                                .or_default()
                                .push(panel_num);
                        }
                    }
                }
            }

            let is_dragging_folder = app.is_dragging
                && app
                    .drag_source
                    .as_ref()
                    .map(|s| s.is_dir())
                    .unwrap_or(false);
            let is_dragging_over_sidebar = is_dragging_folder && app.mouse_pos.0 < area.width;

            if is_dragging_over_sidebar {
                let current_idx = sidebar_items.len();
                sidebar_items.push(
                    ListItem::new(format!("> FAVORITES"))
                        .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                );
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_idx,
                    target: SidebarTarget::Header("FAVORITES".to_string()),
                });
                current_y += 1;
            } else {
                let current_idx = sidebar_items.len();
                let icon = Icon::Star.get(app.icon_mode);
                let is_selected = app.sidebar_index == current_idx;
                let mut style = Style::default()
                    .fg(THEME.accent_secondary)
                    .add_modifier(Modifier::BOLD);
                if is_selected {
                    style = style.bg(THEME.accent_primary).fg(Color::Black);
                }
                sidebar_items.push(ListItem::new(format!("{}FAVORITES", icon)).style(style));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_idx,
                    target: SidebarTarget::Header("FAVORITES".to_string()),
                });
                current_y += 1;
            }

            // Render Starred Folders (Favorites - NO markers as requested)
            for path in &app.starred {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or("?".to_string());
                let current_idx = sidebar_items.len();
                let is_selected = app.sidebar_index == current_idx;
                let is_hovered =
                    matches!(&app.hovered_drop_target, Some(DropTarget::Folder(p)) if p == path);

                // Active highlighting for favorites
                let mut style = Style::default().fg(THEME.fg);
                if is_selected {
                    style = style
                        .bg(THEME.accent_primary)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                } else if is_hovered && app.is_dragging {
                    style = style.bg(THEME.accent_secondary).fg(Color::Black);
                }

                if app.is_dragging && app.mouse_pos.1 == current_y && app.mouse_pos.0 < area.width {
                    app.hovered_drop_target = Some(DropTarget::ReorderFavorite(current_idx));
                }

                let cat = crate::modules::files::get_file_category(path);
                let icon = Icon::get_for_path(path, cat, path.is_dir(), app.icon_mode);

                sidebar_items.push(ListItem::new(format!("{}{}", icon, name)).style(style));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_idx,
                    target: SidebarTarget::Favorite(path.clone()),
                });
                current_y += 1;
            }

            // STORAGE Section
            sidebar_items.push(ListItem::new(""));
            current_y += 1;
            let current_storage_header_idx = sidebar_items.len();
            let storage_icon = Icon::Storage.get(app.icon_mode);
            let mut storage_style = Style::default()
                .fg(THEME.accent_secondary)
                .add_modifier(Modifier::BOLD);
            if app.sidebar_index == current_storage_header_idx {
                storage_style = storage_style.bg(THEME.accent_primary).fg(Color::Black);
            }
            sidebar_items
                .push(ListItem::new(format!("{}STORAGES", storage_icon)).style(storage_style));
            app.sidebar_bounds.push(SidebarBounds {
                y: current_y,
                index: current_storage_header_idx,
                target: SidebarTarget::Header("STORAGES".to_string()),
            });
            current_y += 1;

            for (i, disk) in app.system_state.disks.iter().enumerate() {
                let current_disk_idx = sidebar_items.len();
                let is_selected = app.sidebar_index == current_disk_idx;

                let markers = active_storage_markers.get(&disk.name);

                let mut name_style = if !disk.is_mounted {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::Green)
                };
                if is_selected {
                    name_style = name_style
                        .bg(THEME.accent_primary)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                }

                let mut display_name = if disk.name == "/" {
                    "Root (/)".to_string()
                } else {
                    std::path::Path::new(&disk.name)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or(disk.name.clone())
                };

                // If the name looks like a long hash (e.g. UUID), fallback to size
                if display_name.width() > 20 && display_name.contains('-') {
                    let total_gb = (disk.total_space / 1_073_741_824.0).round() as u64;
                    display_name = format!("{}G Drive", total_gb);
                }

                let mut spans = vec![];
                if let Some(m_list) = markers {
                    let m_str = m_list
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    spans.push(Span::styled(
                        format!("{}| ", m_str),
                        Style::default()
                            .fg(THEME.accent_primary)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                let disk_icon = Icon::Storage.get(app.icon_mode);
                if disk.is_mounted {
                    let available = (disk.available_space as f64 / 1_073_741_824.0).round() as u64;
                    spans.push(Span::styled(
                        format!("{}{}: {}G Free", disk_icon, display_name, available),
                        name_style,
                    ));
                } else {
                    spans.push(Span::styled(
                        format!("{}{}(Not mounted)", disk_icon, disk.name),
                        name_style,
                    ));
                };

                sidebar_items.push(ListItem::new(Line::from(spans)));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_disk_idx,
                    target: SidebarTarget::Storage(i),
                });
                current_y += 1;
            }

            // REMOTE Section
            sidebar_items.push(ListItem::new(""));
            current_y += 1;
            let current_header_idx = sidebar_items.len();
            let mut remotes_style = Style::default()
                .fg(THEME.accent_secondary)
                .add_modifier(Modifier::BOLD);
            if matches!(app.hovered_drop_target, Some(DropTarget::RemotesHeader))
                || app.sidebar_index == current_header_idx
            {
                remotes_style = remotes_style.bg(THEME.accent_primary).fg(Color::Black);
            }
            let remote_icon = Icon::Remote.get(app.icon_mode);
            sidebar_items.push(
                ListItem::new(format!("{}REMOTES [Import]", remote_icon)).style(remotes_style),
            );
            app.sidebar_bounds.push(SidebarBounds {
                y: current_y,
                index: current_header_idx,
                target: SidebarTarget::Header("REMOTES".to_string()),
            });
            current_y += 1;
            for (i, bookmark) in app.remote_bookmarks.iter().enumerate() {
                let current_bookmark_idx = sidebar_items.len();
                let is_selected = app.sidebar_index == current_bookmark_idx;

                let markers = active_remote_markers.get(&bookmark.host);

                let mut style = Style::default().fg(THEME.fg);
                if is_selected {
                    style = style
                        .bg(THEME.accent_primary)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                }

                let mut spans = vec![];
                if let Some(m_list) = markers {
                    let m_str = m_list
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    spans.push(Span::styled(
                        format!("{}| ", m_str),
                        Style::default()
                            .fg(THEME.accent_primary)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                let icon = Icon::Remote.get(app.icon_mode);
                spans.push(Span::styled(format!("{}{}", icon, bookmark.name), style));

                sidebar_items.push(ListItem::new(Line::from(spans)));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_bookmark_idx,
                    target: SidebarTarget::Remote(i),
                });
                current_y += 1;
            }
            if app.remote_bookmarks.is_empty() {
                sidebar_items.push(
                    ListItem::new("(No remotes)").style(Style::default().fg(Color::DarkGray)),
                );
            }

            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(if app.sidebar_focus {
                    Style::default().fg(THEME.border_active)
                } else {
                    Style::default().fg(THEME.border_inactive)
                });

            f.render_widget(List::new(sidebar_items).block(block), area);
        }
        CurrentView::Editor => {
            crate::ui::draw_project_sidebar(f, area, app);
        }
        _ => {}
    }
}
