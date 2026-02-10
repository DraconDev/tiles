use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};
use std::collections::HashMap;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, CurrentView, DropTarget, SidebarBounds, SidebarTarget};
use crate::icons::Icon;
use crate::ui::theme::THEME;
use terma::utils::truncate_to_width;

pub fn draw_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let selection_bg = crate::ui::theme::accent_primary();
    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    match app.current_view {
        CurrentView::Files => {
            let (mut sidebar_items, search_filter) = {
                let items = Vec::new();
                let filter = app
                    .current_file_state()
                    .map(|fs| fs.search_filter.clone())
                    .unwrap_or_default();
                (items, filter)
            };
            app.sidebar_bounds.clear();
            let mut current_y = inner.y;

            let scope_label = match app.sidebar_scope {
                crate::state::SidebarScope::All => "All",
                crate::state::SidebarScope::Favorites => "Favorites",
                crate::state::SidebarScope::Remotes => "Remotes",
            };
            app.sidebar_bounds.push(SidebarBounds {
                y: area.y,
                index: usize::MAX - 1,
                target: SidebarTarget::ScopeToggle,
            });

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
                            if disk.is_mounted && fs.current_path.starts_with(&disk.name) {
                                let len = disk.name.len();
                                if len > longest_prefix {
                                    longest_prefix = len;
                                    matched_disk = Some(disk.name.clone());
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

            // Helper to check if name matches search filter
            let matches_filter = |name: &str| {
                if !app.sidebar_focus || search_filter.is_empty() {
                    return true;
                }
                name.to_lowercase().contains(&search_filter.to_lowercase())
            };

            let show_favorites = matches!(
                app.sidebar_scope,
                crate::state::SidebarScope::All | crate::state::SidebarScope::Favorites
            );
            let show_storage = matches!(app.sidebar_scope, crate::state::SidebarScope::All);
            let show_remotes = matches!(
                app.sidebar_scope,
                crate::state::SidebarScope::All | crate::state::SidebarScope::Remotes
            );

            if show_favorites {
                let current_idx = sidebar_items.len();
                let icon = Icon::Star.get(app.icon_mode);
                let is_selected = app.sidebar_index == current_idx;
                let is_drop_target =
                    matches!(app.hovered_drop_target, Some(DropTarget::Favorites));
                let mut line_style = Style::default().fg(Color::DarkGray);
                let mut label_style = Style::default()
                    .fg(crate::ui::theme::accent_primary())
                    .add_modifier(Modifier::BOLD);
                if is_selected || is_drop_target {
                    line_style = line_style.fg(crate::ui::theme::border_active());
                    label_style = label_style
                        .fg(crate::ui::theme::border_active())
                        .add_modifier(Modifier::UNDERLINED);
                }
                sidebar_items.push(ListItem::new(Line::from(vec![
                    Span::styled("── ", line_style),
                    Span::styled(format!("{} FAVORITES", icon), label_style),
                    Span::styled(" ──", line_style),
                ])));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_idx,
                    target: SidebarTarget::Header("FAVORITES".to_string()),
                });
                current_y += 1;
            }

            // Render Starred Folders (Favorites - NO markers as requested)
            if show_favorites {
                for path in &app.starred {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or("?".to_string());

                if !matches_filter(&name) {
                    continue;
                }

                let current_idx = sidebar_items.len();
                let is_selected = app.sidebar_index == current_idx;
                let is_hovered =
                    matches!(&app.hovered_drop_target, Some(DropTarget::Folder(p)) if p == path);

                // Active highlighting for favorites
                let mut style = Style::default().fg(THEME.fg);
                if is_selected {
                    style = style
                        .bg(selection_bg)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                } else if is_hovered && app.is_dragging {
                    style = style
                        .bg(crate::ui::theme::accent_secondary())
                        .fg(Color::Black);
                }

                if app.is_dragging && app.mouse_pos.1 == current_y && app.mouse_pos.0 < area.width {
                    app.hovered_drop_target = Some(DropTarget::ReorderFavorite(current_idx));
                }

                let cat = crate::modules::files::get_file_category(path);
                let icon = Icon::get_for_path(path, cat, path.is_dir(), app.icon_mode);

                sidebar_items.push(
                    ListItem::new(format!("{}• {}", icon, name)).style(style),
                );
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_idx,
                    target: SidebarTarget::Favorite(path.clone()),
                });
                current_y += 1;
            }
            }

            if show_favorites && !app.recent_folders.is_empty() {
                sidebar_items.push(ListItem::new(""));
                current_y += 1;
                let idx = sidebar_items.len();
                let mut line_style = Style::default().fg(Color::DarkGray);
                let mut recent_style = Style::default()
                    .fg(crate::ui::theme::accent_primary())
                    .add_modifier(Modifier::BOLD);
                if app.sidebar_index == idx {
                    line_style = line_style.fg(crate::ui::theme::border_active());
                    recent_style = recent_style
                        .fg(crate::ui::theme::border_active())
                        .add_modifier(Modifier::UNDERLINED);
                }
                sidebar_items.push(ListItem::new(Line::from(vec![
                    Span::styled("── ", line_style),
                    Span::styled("RECENT", recent_style),
                    Span::styled(" ──", line_style),
                ])));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: idx,
                    target: SidebarTarget::Header("RECENT".to_string()),
                });
                current_y += 1;

                for path in app.recent_folders.iter().take(8) {
                    if app.starred.contains(path) {
                        continue;
                    }
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.to_string_lossy().to_string());
                    if !matches_filter(&name) {
                        continue;
                    }
                    let current_idx = sidebar_items.len();
                    let is_selected = app.sidebar_index == current_idx;
                    let mut style = Style::default().fg(Color::Gray);
                    if is_selected {
                        style = style
                            .bg(selection_bg)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD);
                    }
                    let icon = Icon::Folder.get(app.icon_mode);
                    sidebar_items.push(ListItem::new(format!("{}◦ {}", icon, name)).style(style));
                    app.sidebar_bounds.push(SidebarBounds {
                        y: current_y,
                        index: current_idx,
                        target: SidebarTarget::Favorite(path.clone()),
                    });
                    current_y += 1;
                }
            }

            // STORAGE Section
            if show_storage {
                sidebar_items.push(ListItem::new(""));
                current_y += 1;
                let current_storage_header_idx = sidebar_items.len();
                let storage_icon = Icon::Storage.get(app.icon_mode);
                let mut line_style = Style::default().fg(Color::DarkGray);
                let mut storage_style = Style::default()
                    .fg(crate::ui::theme::accent_primary())
                    .add_modifier(Modifier::BOLD);
                if app.sidebar_index == current_storage_header_idx {
                    line_style = line_style.fg(crate::ui::theme::border_active());
                    storage_style = storage_style
                        .fg(crate::ui::theme::border_active())
                        .add_modifier(Modifier::UNDERLINED);
                }
                sidebar_items.push(ListItem::new(Line::from(vec![
                    Span::styled("── ", line_style),
                    Span::styled(format!("{} STORAGES", storage_icon), storage_style),
                    Span::styled(" ──", line_style),
                ])));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_storage_header_idx,
                    target: SidebarTarget::Header("STORAGES".to_string()),
                });
                current_y += 1;
            }

            for (i, disk) in app.system_state.disks.iter().enumerate() {
                if !show_storage {
                    break;
                }
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

                let markers = active_storage_markers.get(&disk.name);

                let mut name_style = if !disk.is_mounted {
                    Style::default().fg(Color::DarkGray)
                } else {
                    Style::default().fg(Color::White)
                };
                if is_selected {
                    name_style = name_style
                        .bg(selection_bg)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                }

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
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                let disk_icon = Icon::Storage.get(app.icon_mode);
                if disk.is_mounted {
                    let available = (disk.available_space / 1_073_741_824.0).round() as u64;
                    let free_ratio = if disk.total_space > 0.0 {
                        disk.available_space / disk.total_space
                    } else {
                        0.0
                    };
                    let mut free_style = if free_ratio < 0.15 {
                        Style::default().fg(Color::Red)
                    } else if free_ratio < 0.35 {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default().fg(Color::Green)
                    };
                    if is_selected {
                        free_style = free_style.fg(Color::Black).add_modifier(Modifier::BOLD);
                    }
                    spans.push(Span::styled(
                        format!("{}{} ", disk_icon, display_name),
                        name_style,
                    ));
                    spans.push(Span::styled(format!("[{}G]", available), free_style));
                } else {
                    spans.push(Span::styled(
                        format!("{}{} [off]", disk_icon, disk.name),
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
            if show_remotes {
                sidebar_items.push(ListItem::new(""));
                current_y += 1;
                let current_header_idx = sidebar_items.len();
                let mut line_style = Style::default().fg(Color::DarkGray);
                let mut remotes_style = Style::default()
                    .fg(crate::ui::theme::accent_primary())
                    .add_modifier(Modifier::BOLD);
                if matches!(app.hovered_drop_target, Some(DropTarget::RemotesHeader))
                    || app.sidebar_index == current_header_idx
                {
                    line_style = line_style.fg(crate::ui::theme::border_active());
                    remotes_style = remotes_style
                        .fg(crate::ui::theme::border_active())
                        .add_modifier(Modifier::UNDERLINED);
                }
                let remote_icon = Icon::Remote.get(app.icon_mode);
                sidebar_items.push(ListItem::new(Line::from(vec![
                    Span::styled("── ", line_style),
                    Span::styled(format!("{} REMOTES [Import]", remote_icon), remotes_style),
                    Span::styled(" ──", line_style),
                ])));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_header_idx,
                    target: SidebarTarget::Header("REMOTES".to_string()),
                });
                current_y += 1;
            }
            for (i, bookmark) in app.remote_bookmarks.iter().enumerate() {
                if !show_remotes {
                    break;
                }
                if !matches_filter(&bookmark.name) {
                    continue;
                }

                let current_bookmark_idx = sidebar_items.len();
                let is_selected = app.sidebar_index == current_bookmark_idx;

                let markers = active_remote_markers.get(&bookmark.host);

                let mut style = Style::default().fg(THEME.fg);
                if is_selected {
                    style = style
                        .bg(selection_bg)
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
                            .fg(crate::ui::theme::accent_primary())
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                let icon = Icon::Remote.get(app.icon_mode);
                spans.push(Span::styled(format!("{}{} ", icon, bookmark.name), style));
                spans.push(Span::styled(
                    "[ssh]",
                    if is_selected {
                        Style::default()
                            .bg(selection_bg)
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ));

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

            let scope_title_spans = vec![
                Span::styled(" Scope: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    scope_label,
                    Style::default()
                        .fg(crate::ui::theme::accent_secondary())
                        .add_modifier(Modifier::BOLD),
                ),
            ];

            let block = Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title_top(Line::from(scope_title_spans))
                .border_style(if app.sidebar_focus {
                    Style::default().fg(crate::ui::theme::border_active())
                } else {
                    Style::default().fg(crate::ui::theme::border_inactive())
                });

            let list_block = block.clone();
            let list_inner = list_block.inner(area);
            f.render_widget(List::new(sidebar_items).block(list_block), area);

            let hint_target = app
                .sidebar_bounds
                .iter()
                .find(|b| b.y == app.mouse_pos.1)
                .or_else(|| app.sidebar_bounds.iter().find(|b| b.index == app.sidebar_index));
            if let Some(bound) = hint_target {
                let hint = match &bound.target {
                    SidebarTarget::Favorite(path) | SidebarTarget::Project(path) => {
                        path.to_string_lossy().to_string()
                    }
                    SidebarTarget::Remote(idx) => app
                        .remote_bookmarks
                        .get(*idx)
                        .map(|r| format!("{}@{}:{}", r.user, r.host, r.port))
                        .unwrap_or_default(),
                    SidebarTarget::Storage(idx) => app
                        .system_state
                        .disks
                        .get(*idx)
                        .map(|d| d.name.clone())
                        .unwrap_or_default(),
                    SidebarTarget::ScopeToggle => format!("Scope: {}", scope_label),
                    SidebarTarget::Header(name) => name.clone(),
                };
                if !hint.is_empty() && list_inner.height > 0 {
                    let text = truncate_to_width(&hint, list_inner.width.saturating_sub(1) as usize, "..");
                    f.render_widget(
                        Paragraph::new(Span::styled(
                            text,
                            Style::default().fg(Color::DarkGray),
                        )),
                        Rect::new(
                            list_inner.x,
                            list_inner.y + list_inner.height.saturating_sub(1),
                            list_inner.width,
                            1,
                        ),
                    );
                }
            }
        }
        CurrentView::Editor => {
            draw_project_sidebar(f, area, app);
        }
        _ => {}
    }
}

pub fn draw_project_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let selection_bg = crate::ui::theme::selection_bg();
    // Resolve both tree base path and a user-facing title path from focused editor context.
    let (base_path, title_path) = if let Some(pane) = app.panes.get(app.focused_pane_index) {
        if let Some(preview) = &pane.preview {
            if preview.path.is_dir() {
                (preview.path.clone(), preview.path.clone())
            } else {
                (
                    preview
                        .path
                        .parent()
                        .map(|p| p.to_path_buf())
                        .unwrap_or_else(|| PathBuf::from("/")),
                    preview.path.clone(),
                )
            }
        } else if let Some(tab) = pane.tabs.get(pane.active_tab_index) {
            (tab.current_path.clone(), tab.current_path.clone())
        } else {
            return;
        }
    } else if let Some(preview) = &app.editor_state {
        if preview.path.is_dir() {
            (preview.path.clone(), preview.path.clone())
        } else {
            (
                preview
                    .path
                    .parent()
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("/")),
                preview.path.clone(),
            )
        }
    } else {
        return;
    };
    let title_text = {
        let full = title_path.to_string_lossy().to_string();
        let max_w = area.width.saturating_sub(10) as usize;
        if full.width() > max_w && max_w > 4 {
            let tail: String = full
                .chars()
                .rev()
                .take(max_w - 3)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("...{}", tail)
        } else {
            full
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(" {} ", title_text))
        .border_style(if app.sidebar_focus {
            Style::default().fg(crate::ui::theme::border_active())
        } else {
            Style::default().fg(crate::ui::theme::border_inactive())
        });

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut tree_items: Vec<(PathBuf, u16)> = Vec::new();
    collect_tree_items(&base_path, 0, app, &mut tree_items);

    let mut sidebar_items = Vec::new();
    app.sidebar_bounds.clear();
    let mut current_y = inner.y;

    for (path, depth) in tree_items {
        let is_dir = path.is_dir();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or("?".to_string());
        let current_idx = sidebar_items.len();
        let is_selected = app.sidebar_focus && app.sidebar_index == current_idx;
        let is_hovered_drop =
            matches!(&app.hovered_drop_target, Some(DropTarget::Folder(p)) if p == &path);

        let cat = crate::modules::files::get_file_category(&path);
        let icon_mode = app.icon_mode;

        let style = if is_selected {
            Style::default()
                .bg(selection_bg)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else if is_hovered_drop {
            Style::default()
                .bg(crate::ui::theme::accent_secondary())
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            let fg = if app.semantic_coloring {
                if is_dir {
                    crate::ui::theme::accent_secondary()
                } else {
                    cat.cyber_color()
                }
            } else {
                THEME.fg
            };
            Style::default().fg(fg)
        };

        // Show expansion marker for folders
        let marker = if is_dir {
            if app.expanded_folders.contains(&path) {
                "▾ "
            } else {
                "▸ "
            }
        } else {
            "  "
        };

        let icon = Icon::get_for_path(&path, cat, is_dir, icon_mode);
        let indent = "  ".repeat(depth as usize);

        sidebar_items
            .push(ListItem::new(format!("{}{}{}{}", indent, marker, icon, name)).style(style));
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
            if a_is_dir && !b_is_dir {
                std::cmp::Ordering::Less
            } else if !a_is_dir && b_is_dir {
                std::cmp::Ordering::Greater
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });

        for entry in sorted_entries {
            let p = entry.path();
            let name = p.file_name().unwrap_or_default().to_string_lossy();

            if !app.default_show_hidden && name.starts_with('.') {
                continue;
            }

            // Check if matches search filter (if any)
            let matches_filter = if let Some(fs) = app
                .panes
                .get(app.focused_pane_index)
                .and_then(|p| p.current_state())
            {
                if !fs.search_filter.is_empty() && app.sidebar_focus {
                    name.to_lowercase()
                        .contains(&fs.search_filter.to_lowercase())
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
