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
            let (mut sidebar_items, search_filter) = {
                let mut items = Vec::new();
                let filter = app.current_file_state().map(|fs| fs.search_filter.clone()).unwrap_or_default();
                (items, filter)
            };
            app.sidebar_bounds.clear();
            let mut current_y = inner.y;

            // ... (Markers logic)

            let is_dragging_folder = app.is_dragging
                && app
                    .drag_source
                    .as_ref()
                    .map(|s| s.is_dir())
                    .unwrap_or(false);
            let is_dragging_over_sidebar = is_dragging_folder && app.mouse_pos.0 < area.width;

            // Helper to check if name matches search filter
            let matches_filter = |name: &str| {
                if !app.sidebar_focus || search_filter.is_empty() {
                    return true;
                }
                name.to_lowercase().contains(&search_filter.to_lowercase())
            };

            if is_dragging_over_sidebar {
                // ...
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
                
                if !matches_filter(&name) {
                    continue;
                }

                let current_idx = sidebar_items.len();
                // ...
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
                let mut display_name = if disk.name == "/" {
                    "Root (/)".to_string()
                } else {
                    std::path::Path::new(&disk.name)
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or(disk.name.clone())
                };

                if !matches_filter(&display_name) {
                    continue;
                }

                let current_disk_idx = sidebar_items.len();
                let is_selected = app.sidebar_index == current_disk_idx;

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
                if !matches_filter(&bookmark.name) {
                    continue;
                }

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
            draw_project_sidebar(f, area, app);
        }
        _ => {}
    }
}

pub fn draw_project_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" PROJECT ")
        .border_style(if app.sidebar_focus {
            Style::default().fg(THEME.border_active)
        } else {
            Style::default().fg(THEME.border_inactive)
        });

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Get the base path for the tree
    let base_path = if let Some(pane) = app.panes.get(app.focused_pane_index) {
        if let Some(tab) = pane.tabs.get(pane.active_tab_index) {
            tab.current_path.clone()
        } else {
            return;
        }
    } else {
        return;
    };

    let mut tree_items: Vec<(PathBuf, u16)> = Vec::new();
    collect_tree_items(&base_path, 0, app, &mut tree_items);

    let mut sidebar_items = Vec::new();
    app.sidebar_bounds.clear();
    let mut current_y = inner.y;

    for (path, depth) in tree_items {
        let is_dir = path.is_dir();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("?".to_string());
        let current_idx = sidebar_items.len();
        let is_selected = app.sidebar_focus && app.sidebar_index == current_idx;
        
        let mut style = Style::default().fg(THEME.fg);
        if is_selected {
            style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
        }

        let cat = crate::modules::files::get_file_category(&path);
        let icon_mode = app.icon_mode;
        
        let style = if is_selected {
            Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            let fg = match cat {
                crate::app::FileCategory::Script => THEME.file_code,
                crate::app::FileCategory::Text => THEME.file_config,
                crate::app::FileCategory::Image | crate::app::FileCategory::Video | crate::app::FileCategory::Audio => THEME.file_media,
                crate::app::FileCategory::Archive => THEME.file_archive,
                crate::app::FileCategory::Document => THEME.fg,
                _ if is_dir => THEME.header_fg,
                _ => THEME.fg,
            };
            Style::default().fg(fg)
        };

        // Show expansion marker for folders
        let marker = if is_dir {
            if app.expanded_folders.contains(&path) { "▾ " } else { "▸ " }
        } else {
            "  "
        };

        let icon = Icon::get_for_path(&path, cat, is_dir, icon_mode);
        let indent = "  ".repeat(depth as usize);
        
        sidebar_items.push(ListItem::new(format!("{}{}{}{}", indent, marker, icon, name)).style(style));
        app.sidebar_bounds.push(SidebarBounds {
            y: current_y,
            index: current_idx,
            target: SidebarTarget::Project(path.clone()),
        });
        current_y += 1;
        
        if current_y >= inner.y + inner.height {
            break;
        }
    }

    f.render_widget(List::new(sidebar_items), inner);
}

fn collect_tree_items(path: &PathBuf, depth: u16, app: &App, items: &mut Vec<(PathBuf, u16)>) {
    if let Ok(entries) = std::fs::read_dir(path) {
        let mut sorted_entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        
        sorted_entries.sort_by(|a, b| {
            let a_is_dir = a.path().is_dir();
            let b_is_dir = b.path().is_dir();
            if a_is_dir && !b_is_dir { std::cmp::Ordering::Less }
            else if !a_is_dir && b_is_dir { std::cmp::Ordering::Greater }
            else { a.file_name().cmp(&b.file_name()) }
        });

        for entry in sorted_entries {
            let p = entry.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            
            if !app.default_show_hidden && name.starts_with('.') {
                continue;
            }

            // Check if matches search filter (if any)
            let matches_filter = if let Some(fs) = app.panes.get(app.focused_pane_index).and_then(|p| p.current_state()) {
                if !fs.search_filter.is_empty() && app.sidebar_focus {
                    name.to_lowercase().contains(&fs.search_filter.to_lowercase())
                } else {
                    true
                }
            } else {
                true
            };

            if matches_filter {
                items.push((p.clone(), depth));
            }
            
            if p.is_dir() && app.expanded_folders.contains(&p) {
                collect_tree_items(&p, depth + 1, app, items);
            }
        }
    }
}
