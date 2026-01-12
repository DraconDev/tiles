use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState, Tabs, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use std::time::SystemTime;
use std::path::PathBuf;
use std::collections::HashMap;

use crate::app::{App, AppMode, CurrentView, FileColumn, SidebarTarget, SidebarBounds, DropTarget, SettingsSection, SettingsTarget, FileCategory};
use crate::ui::theme::THEME;
use crate::icons::Icon;
use terma::layout::centered_rect;
use terma::utils::{format_size, format_time, format_permissions};

pub mod theme;
pub mod layout;

fn draw_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
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
                        active_remote_markers.entry(session.host.clone()).or_default().push(panel_num);
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
                            active_storage_markers.entry(name).or_default().push(panel_num);
                        }
                    }
                }
            }

            let is_dragging_folder = app.is_dragging && app.drag_source.as_ref().map(|s| s.is_dir()).unwrap_or(false);
            let is_dragging_over_sidebar = is_dragging_folder && app.mouse_pos.0 < area.width;

            if is_dragging_over_sidebar {
                let current_idx = sidebar_items.len();
                sidebar_items.push(ListItem::new(format!("> FAVORITES")).style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)));
                app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_idx, target: SidebarTarget::Header("FAVORITES".to_string()) });
                current_y += 1;
            } else {
                let current_idx = sidebar_items.len();
                let icon = Icon::Star.get(app.icon_mode);
                sidebar_items.push(ListItem::new(format!("{}FAVORITES", icon)).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)));
                app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_idx, target: SidebarTarget::Header("FAVORITES".to_string()) });
                current_y += 1;
            }

            // Render Starred Folders (Favorites - NO markers as requested)
            for path in &app.starred {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("?".to_string());
                let current_idx = sidebar_items.len();
                let is_focused = app.sidebar_focus && app.sidebar_index == current_idx;
                let is_hovered = matches!(&app.hovered_drop_target, Some(DropTarget::Folder(p)) if p == path);
                
                // Active highlighting for favorites
                let is_active = if let Some(fs) = app.current_file_state() { fs.current_path == *path && fs.remote_session.is_none() } else { false };

                let mut style = if is_active { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(THEME.fg) };
                if is_focused { style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD); }
                else if is_hovered && app.is_dragging { style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD); }

                let icon = if path.is_dir() { Icon::Folder.get(app.icon_mode) } else { Icon::File.get(app.icon_mode) };
                sidebar_items.push(ListItem::new(format!("{}{}", icon, name)).style(style));
                app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_idx, target: SidebarTarget::Favorite(path.clone()) });
                current_y += 1;
            }

            // STORAGE Section
            sidebar_items.push(ListItem::new("")); current_y += 1;
            let current_storage_header_idx = sidebar_items.len();
            let storage_icon = Icon::Storage.get(app.icon_mode);
            sidebar_items.push(ListItem::new(format!("{}STORAGES", storage_icon)).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)));
            app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_storage_header_idx, target: SidebarTarget::Header("STORAGES".to_string()) });
            current_y += 1;
            
            for (i, disk) in app.system_state.disks.iter().enumerate() {
                let current_disk_idx = sidebar_items.len();
                let is_focused = app.sidebar_focus && app.sidebar_index == current_disk_idx;
                
                let markers = active_storage_markers.get(&disk.name);
                let is_active = markers.is_some();

                let mut name_style = if !disk.is_mounted { Style::default().fg(Color::DarkGray) } 
                                     else if is_active { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } 
                                     else { Style::default().fg(Color::Green) };
                if is_focused { name_style = name_style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD); }

                let mut display_name = if disk.name == "/" { "Root (/)".to_string() } else { 
                    std::path::Path::new(&disk.name).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(disk.name.clone())
                };

                // If the name looks like a long hash (e.g. UUID), fallback to size
                if display_name.len() > 20 && display_name.contains('-') {
                    let total_gb = (disk.total_space / 1_073_741_824.0).round() as u64;
                    display_name = format!("{}G Drive", total_gb);
                }

                let mut spans = vec![];
                if let Some(m_list) = markers {
                    let m_str = m_list.iter().map(|m| m.to_string()).collect::<Vec<_>>().join(",");
                    spans.push(Span::styled(format!("{}| ", m_str), Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)));
                }

                let disk_icon = Icon::Storage.get(app.icon_mode);
                if disk.is_mounted {
                    let available = (disk.available_space as f64 / 1_073_741_824.0).round() as u64;
                    spans.push(Span::styled(format!("{}{}: {}G Free", disk_icon, display_name, available), name_style));
                } else {
                    spans.push(Span::styled(format!("{}{}(Not mounted)", disk_icon, disk.name), name_style));
                };

                sidebar_items.push(ListItem::new(Line::from(spans)));
                app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_disk_idx, target: SidebarTarget::Storage(i) });
                current_y += 1;
            }

            // REMOTE Section
            sidebar_items.push(ListItem::new("")); current_y += 1;
            let current_header_idx = sidebar_items.len();
            let mut remotes_style = Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD);
            if matches!(app.hovered_drop_target, Some(DropTarget::RemotesHeader)) { remotes_style = remotes_style.bg(THEME.accent_primary).fg(Color::Black); }
            let remote_icon = Icon::Remote.get(app.icon_mode);
            sidebar_items.push(ListItem::new(format!("{}REMOTES [Import]", remote_icon)).style(remotes_style));
            app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_header_idx, target: SidebarTarget::Header("REMOTES".to_string()) });
            current_y += 1;
            for (i, bookmark) in app.remote_bookmarks.iter().enumerate() {
                let current_bookmark_idx = sidebar_items.len();
                let is_focused = app.sidebar_focus && app.sidebar_index == current_bookmark_idx;
                
                let markers = active_remote_markers.get(&bookmark.host);
                let is_active = markers.is_some();

                let mut style = if is_active { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(THEME.fg) };
                if is_focused { style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD); }

                let mut spans = vec![];
                if let Some(m_list) = markers {
                    let m_str = m_list.iter().map(|m| m.to_string()).collect::<Vec<_>>().join(",");
                    spans.push(Span::styled(format!("{}| ", m_str), Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)));
                }
                let icon = Icon::Remote.get(app.icon_mode);
                spans.push(Span::styled(format!("{}{}", icon, bookmark.name), style));

                sidebar_items.push(ListItem::new(Line::from(spans)));
                app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_bookmark_idx, target: SidebarTarget::Remote(i) });
                current_y += 1;
            }
            if app.remote_bookmarks.is_empty() {
                sidebar_items.push(ListItem::new("(No remotes)").style(Style::default().fg(Color::DarkGray)));
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
        _ => {}
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    // Force pure black background
    f.render_widget(Block::default().style(Style::default().bg(Color::Black)), f.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    let workspace_constraints = if app.show_sidebar {
        [Constraint::Percentage(app.sidebar_width_percent), Constraint::Min(0)]
    } else {
        [Constraint::Percentage(0), Constraint::Min(0)]
    };

    let workspace = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(workspace_constraints)
        .split(chunks[1]);

    draw_global_header(f, chunks[0], workspace[0].width, app);
    if app.show_sidebar {
        draw_sidebar(f, workspace[0], app);
    }
    draw_main_stage(f, workspace[1], app);
    draw_footer(f, chunks[2], app);

    if let AppMode::ContextMenu { x, y, ref target, .. } = app.mode { draw_context_menu(f, x, y, target, app); }
    if matches!(app.mode, AppMode::Highlight) { draw_highlight_modal(f, app); }
    if matches!(app.mode, AppMode::Rename) { draw_rename_modal(f, app); }
    if matches!(app.mode, AppMode::Delete) { draw_delete_modal(f, app); }
    if matches!(app.mode, AppMode::Properties) { draw_properties_modal(f, app); }
    if matches!(app.mode, AppMode::NewFolder) { draw_new_folder_modal(f, app); }
    if matches!(app.mode, AppMode::NewFile) { draw_new_file_modal(f, app); }
    if matches!(app.mode, AppMode::Settings) { draw_settings_modal(f, app); }
    if matches!(app.mode, AppMode::CommandPalette) { draw_command_palette(f, app); }
    if matches!(app.mode, AppMode::AddRemote) { draw_add_remote_modal(f, app); }
    if matches!(app.mode, AppMode::ImportServers) { draw_import_servers_modal(f, app); }
}

fn draw_global_header(f: &mut Frame, area: Rect, sidebar_width: u16, app: &mut App) {
    let pane_count = app.panes.len();
    let logo_width = 10;
    f.render_widget(Paragraph::new(" 👹 TILES ").style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Rect::new(area.x, area.y, logo_width, 1));

    let split_icon = if pane_count > 1 { Icon::Split.get(app.icon_mode) } else { Icon::Single.get(app.icon_mode) };
    let split_width = split_icon.len() as u16 + 2;
    f.render_widget(Paragraph::new(format!(" {} ", split_icon)).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)), Rect::new(area.x + area.width.saturating_sub(split_width), area.y, split_width, 1));

    if pane_count == 0 { return; }
    let start_x = if app.show_sidebar { 
        std::cmp::max(area.x + sidebar_width, area.x + logo_width + 1)
    } else {
        area.x + logo_width + 1
    };
    let pane_chunks = Layout::default().direction(Direction::Horizontal).constraints(vec![Constraint::Percentage(100 / pane_count as u16); pane_count]).split(Rect::new(start_x, area.y, area.width.saturating_sub(start_x + split_width), 1));

    app.tab_bounds.clear();
    for (p_i, pane) in app.panes.iter().enumerate() {
        let chunk = pane_chunks[p_i];
        let mut current_x = chunk.x;
        for (t_i, tab) in pane.tabs.iter().enumerate() {
            let mut name = if !tab.search_filter.is_empty() { format!("Search: {}", tab.search_filter) } 
                           else { tab.current_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("/".to_string()) };
            if let Some(branch) = &tab.git_branch { name = format!("{} ({})", name, branch); }
            let is_active_tab = t_i == pane.active_tab_index;
            let is_focused_pane = p_i == app.focused_pane_index && !app.sidebar_focus;
            let style = if is_active_tab { if is_focused_pane { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(THEME.accent_primary) } } 
                        else { Style::default().fg(Color::DarkGray) };
            let text = format!(" {} ", name);
            let width = text.len() as u16;
            if current_x + width > chunk.x + chunk.width { break; }
            let rect = Rect::new(current_x, area.y, width, 1);
            f.render_widget(Paragraph::new(text).style(style), rect);
            app.tab_bounds.push((rect, p_i, t_i));
            current_x += width + 1;
        }
    }
}

fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
    let pane_count = app.panes.len();
    if pane_count == 0 { return; }
    let constraints = vec![Constraint::Percentage(100 / pane_count as u16); pane_count];
    let chunks = Layout::default().direction(Direction::Horizontal).constraints(constraints).split(area);
    for i in 0..pane_count {
        let is_focused = i == app.focused_pane_index && !app.sidebar_focus;
        let borders = if pane_count > 1 { if i == 0 { Borders::ALL } else { Borders::ALL } } else { Borders::ALL };
        draw_file_view(f, chunks[i], app, i, is_focused, borders);
    }
}

fn highlight_code<'a>(content: &'a str) -> Vec<Line<'a>> {
    content.lines().map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("#") || trimmed.starts_with("//") {
            Line::from(Span::styled(line, Style::default().fg(Color::Green)))
        } else if trimmed.starts_with("[") && trimmed.ends_with("]") {
             Line::from(Span::styled(line, Style::default().fg(Color::Yellow)))
        } else if let Some(idx) = line.find('=') {
             let key = &line[..idx];
             let val = &line[idx..];
             Line::from(vec![
                 Span::styled(key, Style::default().fg(Color::Cyan)),
                 Span::raw(val)
             ])
        } else {
             let keywords = ["pub", "fn", "struct", "impl", "let", "const", "use", "mod", "crate", "import", "from", "class", "def", "func"];
             if keywords.iter().any(|k| trimmed.starts_with(k)) {
                  Line::from(Span::styled(line, Style::default().fg(Color::Magenta)))
             } else {
                  Line::from(Span::raw(line))
             }
        }
    }).collect()
}

fn draw_file_view(f: &mut Frame, area: Rect, app: &mut App, pane_idx: usize, is_focused: bool, borders: Borders) {
    if let Some(pane) = app.panes.get_mut(pane_idx) {
        if let Some(preview) = &pane.preview {
            let block = Block::default()
                .borders(borders)
                .border_type(BorderType::Rounded)
                .title(format!(" Preview: {} ", preview.path.display()))
                .border_style(if is_focused { Style::default().fg(THEME.border_active) } else { Style::default().fg(THEME.border_inactive) });
            
            let highlighted = highlight_code(&preview.content);
            let text = Paragraph::new(highlighted)
                .block(block);
            
            f.render_widget(text, area);
            return;
        }
    }

    if let Some(file_state) = app.panes.get_mut(pane_idx).and_then(|p| p.current_state_mut()) {
        file_state.view_height = area.height as usize;
        let mut render_state = TableState::default();
        if let Some(sel) = file_state.selected_index {
            let offset = file_state.table_state.offset();
            let capacity = file_state.view_height.saturating_sub(3);
            if sel >= offset && sel < offset + capacity { render_state.select(Some(sel)); }
        }
        *render_state.offset_mut() = file_state.table_state.offset();

        let constraints: Vec<Constraint> = file_state.columns.iter().map(|c| match c {
            FileColumn::Name => Constraint::Min(20),
            FileColumn::Size => Constraint::Length(10),
            FileColumn::Modified => Constraint::Percentage(20),
            FileColumn::Permissions => Constraint::Length(12),
        }).collect();

        let dummy_block = Block::default().borders(borders);
        let column_layout = Layout::default().direction(Direction::Horizontal).constraints(constraints.clone()).spacing(0).split(dummy_block.inner(area));
        let name_col_width = column_layout.get(0).map(|r| r.width as usize).unwrap_or(20);

        let header_cells = file_state.columns.iter().map(|c| {
            let base_name = match c { FileColumn::Name => "Name", FileColumn::Size => "Size", FileColumn::Modified => "Modified", FileColumn::Permissions => "Permissions" };
            let name = if *c == file_state.sort_column { if file_state.sort_ascending { format!("{} ▲", base_name) } else { format!("{} ▼", base_name) } } else { base_name.to_string() };
            Cell::from(name).style(Style::default().fg(THEME.header_fg).add_modifier(Modifier::BOLD))
        });

        let rows = file_state.files.iter().enumerate().map(|(i, path)| {
            if path.to_string_lossy() == "__DIVIDER__" {
                let cells = file_state.columns.iter().enumerate().map(|(col_idx, _)| {
                    if col_idx == 0 { Cell::from("> Global results").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)) } 
                    else { Cell::from("──────────────────").style(Style::default().fg(Color::DarkGray)) }
                });
                return Row::new(cells);
            }
            let metadata = file_state.metadata.get(path);
            let is_multi_selected = file_state.multi_select.contains(&i) && is_focused;
            let cells = file_state.columns.iter().map(|c| match c {
                FileColumn::Name => {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
                    let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
                    
                    let path_color = app.path_colors.get(path).map(|&c| match c {
                        1 => Color::Red,
                        2 => Color::Green,
                        3 => Color::Yellow,
                        4 => Color::Blue,
                        5 => Color::Magenta,
                        6 => Color::Cyan,
                        _ => Color::White,
                    });

                    let mut final_style = if let Some(c) = path_color {
                        Style::default().fg(c).add_modifier(Modifier::BOLD)
                    } else if is_dir {
                        Style::default().fg(THEME.accent_secondary)
                    } else {
                        Style::default().fg(THEME.fg)
                    };

                    // Dim style if file is currently "Cut" in clipboard
                    if let Some((ref cb_path, op)) = app.clipboard {
                        if op == crate::app::ClipboardOp::Cut && cb_path == path {
                            final_style = final_style.add_modifier(Modifier::DIM);
                        }
                    }
                    
                    let icon = if is_dir { 
                        Icon::Folder.get(app.icon_mode) 
                    } else {
                        let cat = crate::modules::files::get_file_category(path);
                        match cat {
                            FileCategory::Archive => Icon::Archive.get(app.icon_mode),
                            FileCategory::Image => Icon::Image.get(app.icon_mode),
                            FileCategory::Audio => Icon::Audio.get(app.icon_mode),
                            FileCategory::Video => Icon::Video.get(app.icon_mode),
                            FileCategory::Script => Icon::Script.get(app.icon_mode),
                            FileCategory::Document => Icon::Document.get(app.icon_mode),
                            _ => Icon::File.get(app.icon_mode),
                        }
                    };

                    let mut suffix = String::new();
                    let is_starred = app.starred.contains(path);
                    if is_starred { suffix.push_str(" [*]"); }

                    if let Some(c) = path_color {
                        final_style = Style::default().fg(c).add_modifier(Modifier::BOLD);
                    } else if is_starred {
                        final_style = Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD);
                    }

                    if i > file_state.local_count {
                        let full_str = path.to_string_lossy();
                        let mut display_path = if full_str.starts_with("/home/dracon") { full_str.replacen("/home/dracon", "~", 1) } else { full_str.to_string() };
                        display_path.push_str(&suffix);
                        if display_path.len() > name_col_width && name_col_width > 5 {
                            let keep_len = name_col_width - 3; let start_idx = display_path.len() - keep_len;
                            display_path = format!("...{}", &display_path[start_idx..]);
                        }
                        Cell::from(format!("{}{}", icon, display_path)).style(final_style)
                    } else { Cell::from(format!("{}{}{}", icon, name, suffix)).style(final_style) }
                }
                FileColumn::Size => { 
                    let path_color = app.path_colors.get(path).map(|&c| match c {
                        1 => Color::Red, 2 => Color::Green, 3 => Color::Yellow, 4 => Color::Blue, 5 => Color::Magenta, 6 => Color::Cyan, _ => Color::White,
                    });
                    let style = if let Some(c) = path_color { Style::default().fg(c) } else if metadata.map(|m| m.is_dir).unwrap_or(false) { Style::default().fg(THEME.accent_secondary) } else { Style::default().fg(THEME.fg) };
                    if metadata.map(|m| m.is_dir).unwrap_or(false) { Cell::from("<DIR>").style(style) } else { Cell::from(format_size(metadata.map(|m| m.size).unwrap_or(0))).style(style) } 
                }
                FileColumn::Modified => {
                    let path_color = app.path_colors.get(path).map(|&c| match c {
                        1 => Color::Red, 2 => Color::Green, 3 => Color::Yellow, 4 => Color::Blue, 5 => Color::Magenta, 6 => Color::Cyan, _ => Color::White,
                    });
                    let style = if let Some(c) = path_color { Style::default().fg(c) } else { Style::default().fg(THEME.fg) };
                    Cell::from(format_time(metadata.map(|m| m.modified).unwrap_or(SystemTime::UNIX_EPOCH))).style(style)
                },
                FileColumn::Permissions => {
                    let path_color = app.path_colors.get(path).map(|&c| match c {
                        1 => Color::Red, 2 => Color::Green, 3 => Color::Yellow, 4 => Color::Blue, 5 => Color::Magenta, 6 => Color::Cyan, _ => Color::White,
                    });
                    let style = if let Some(c) = path_color { Style::default().fg(c) } else { Style::default().fg(THEME.fg) };
                    Cell::from(format_permissions(metadata.map(|m| m.permissions).unwrap_or(0))).style(style)
                },
            });
            let mut row_style = Style::default();
            if is_multi_selected { row_style = row_style.bg(Color::Rgb(100, 0, 0)).fg(Color::White); }
            Row::new(cells).style(row_style)
        });

        let mut breadcrumb_spans = Vec::new();
        file_state.breadcrumb_bounds.clear();
        let path = file_state.current_path.clone();
        let components: Vec<_> = path.components().collect();
        let mut cur_p = PathBuf::new();
        let mut cur_x = area.x + 2;
        for (i, comp) in components.iter().enumerate() {
            match comp { std::path::Component::RootDir => cur_p.push("/"), std::path::Component::Prefix(p) => cur_p.push(p.as_os_str()), std::path::Component::Normal(name) => cur_p.push(name), _ => continue }
            let d_name = if comp.as_os_str() == "/" { "/".to_string() } else { comp.as_os_str().to_string_lossy().to_string() };
            if !d_name.is_empty() {
                let s_path = cur_p.clone();
                let style = if file_state.hovered_breadcrumb.as_ref() == Some(&s_path) { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD | Modifier::UNDERLINED) } else { Style::default().fg(THEME.fg) };
                let width = d_name.len() as u16;
                breadcrumb_spans.push(Span::styled(d_name, style));
                file_state.breadcrumb_bounds.push((Rect::new(cur_x, area.y, width, 1), s_path));
                cur_x += width;
                if i < components.len() - 1 && comp.as_os_str() != "/" { breadcrumb_spans.push(Span::styled("/", Style::default().fg(Color::DarkGray))); cur_x += 1; }
            }
        }
        if let Some(branch) = &file_state.git_branch { breadcrumb_spans.push(Span::styled(format!(" ({})", branch), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))); }
        if !file_state.search_filter.is_empty() { breadcrumb_spans.push(Span::styled(format!(" [ {} ]", file_state.search_filter), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))); }

        let mut border_style = if is_focused { Style::default().fg(THEME.border_active) } else { Style::default().fg(THEME.border_inactive) };
        if matches!(app.hovered_drop_target, Some(DropTarget::Pane(idx)) if idx == pane_idx) {
            border_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
        }

        let block = Block::default().borders(borders).border_type(BorderType::Rounded).title(Line::from(breadcrumb_spans))
            .border_style(border_style);

        let table = Table::new(rows, constraints.clone()).header(Row::new(header_cells).height(1)).block(block.clone()).column_spacing(0)
            .row_highlight_style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD));

        let height = area.height.saturating_sub(2) as usize;
        let offset = render_state.offset();
        let mut display_state = render_state.clone();
        if let Some(sel) = render_state.selected() { if sel < offset || sel >= offset + height { display_state.select(None); } }

        file_state.column_bounds.clear();
        let column_layout = Layout::default().direction(Direction::Horizontal).constraints(constraints.clone()).spacing(0).split(block.inner(area));
        for (i, col_type) in file_state.columns.iter().enumerate() { file_state.column_bounds.push((column_layout[i], *col_type)); }

        f.render_stateful_widget(table, area, &mut display_state);
        *file_state.table_state.offset_mut() = display_state.offset();

        if file_state.files.len() > area.height.saturating_sub(4) as usize {
            let scrollbar = Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight).begin_symbol(Some("▲")).end_symbol(Some("▼"));
            let mut scroll_state = ScrollbarState::new(file_state.files.len()).position(file_state.table_state.offset());
            f.render_stateful_widget(scrollbar, area, &mut scroll_state);
        }
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(75), Constraint::Percentage(25)]).split(area);
    let shortcuts = vec![
        Span::styled(" ^Q ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Quit "),
        Span::styled(" ^B ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Sidebar "),
        Span::styled(" ^G ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Settings "),
        Span::styled(" ^S ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Split "),
        Span::styled(" ^T ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Tab "),
        Span::styled(" ^N ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Terminal "),
        Span::styled(" ^Spc ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Cmd "),
        Span::styled(" ^H ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Hidden "),
    ];
    f.render_widget(Paragraph::new(Line::from(shortcuts)), chunks[0]);

    let mut stats = vec![
        Span::styled(format!("CPU: {:.0}%", app.system_state.cpu_usage), Style::default().fg(Color::Green)),
        Span::raw(" | "),
        Span::styled(format!("Mem: {:.0}%", (app.system_state.mem_usage / app.system_state.total_mem.max(1.0)) * 100.0), Style::default().fg(Color::Yellow)),
    ];
    f.render_widget(Paragraph::new(Line::from(stats)).alignment(ratatui::layout::Alignment::Right), chunks[1]);
}

fn draw_context_menu(f: &mut Frame, x: u16, y: u16, target: &crate::app::ContextMenuTarget, app: &App) {
    use crate::app::ContextMenuAction;
    let mut items = Vec::new();
    
    let actions = if let AppMode::ContextMenu { actions, .. } = &app.mode {
        actions.clone()
    } else {
        vec![] 
    };

    for action in &actions {
        let label = match action {
            ContextMenuAction::Open => format!(" {} Open", Icon::Folder.get(app.icon_mode)),
            ContextMenuAction::OpenNewTab => format!(" {} Open in New Tab", Icon::Split.get(app.icon_mode)),
            ContextMenuAction::Edit => format!(" {} Edit", Icon::Document.get(app.icon_mode)),
            ContextMenuAction::Run => format!(" {} Run", Icon::Video.get(app.icon_mode)),
            ContextMenuAction::RunTerminal => format!(" {} Run in Terminal", Icon::Script.get(app.icon_mode)),
            ContextMenuAction::ExtractHere => format!(" {} Extract Here", Icon::Archive.get(app.icon_mode)),
            ContextMenuAction::NewFolder => format!(" {} New Folder", Icon::Folder.get(app.icon_mode)),
            ContextMenuAction::NewFile => format!(" {} New File", Icon::File.get(app.icon_mode)),
            ContextMenuAction::Cut => " 󰆐 Cut".to_string(), // Keep some standard ones or update all
            ContextMenuAction::Copy => " 󰆏 Copy".to_string(),
            ContextMenuAction::Paste => " 󰆒 Paste".to_string(),
            ContextMenuAction::Rename => " 󰏫 Rename".to_string(),
            ContextMenuAction::Duplicate => " 󰆏 Duplicate".to_string(),
            ContextMenuAction::Compress => format!(" {} Compress", Icon::Archive.get(app.icon_mode)),
            ContextMenuAction::Delete => " 󰆴 Delete".to_string(),
            ContextMenuAction::AddToFavorites => format!(" {} Add to Favorites", Icon::Star.get(app.icon_mode)),
            ContextMenuAction::RemoveFromFavorites => format!(" {} Remove from Favorites", Icon::Star.get(app.icon_mode)),
            ContextMenuAction::Properties => format!(" {} Properties", Icon::Document.get(app.icon_mode)),
        ContextMenuAction::TerminalWindow => format!(" {} New Terminal Window", Icon::Script.get(app.icon_mode)),
            ContextMenuAction::Refresh => " 󰑓 Refresh".to_string(),
            ContextMenuAction::SelectAll => " 󰒆 Select All".to_string(),
            ContextMenuAction::ToggleHidden => " 󰈈 Toggle Hidden".to_string(),
            ContextMenuAction::ConnectRemote => format!(" {} Connect", Icon::Remote.get(app.icon_mode)),
            ContextMenuAction::DeleteRemote => " 󰆴 Delete Bookmark".to_string(),
            ContextMenuAction::Mount => " 󰃭 Mount".to_string(),
            ContextMenuAction::Unmount => " 󰃭 Unmount".to_string(),
            ContextMenuAction::SetWallpaper => format!(" {} Set as Wallpaper", Icon::Image.get(app.icon_mode)),
            ContextMenuAction::GitInit => format!(" {} Git Init", Icon::Git.get(app.icon_mode)),
            ContextMenuAction::GitStatus => format!(" {} Git Status", Icon::Git.get(app.icon_mode)),
            ContextMenuAction::SetColor(_) => format!(" {} Highlight...", Icon::Image.get(app.icon_mode)),
            ContextMenuAction::SortBy(col) => {
                let name = match col {
                    crate::app::FileColumn::Name => "Name",
                    crate::app::FileColumn::Size => "Size",
                    crate::app::FileColumn::Modified => "Date",
                    _ => "Unknown",
                };
                let mut label = format!(" 󰒺 Sort by {}", name);
                if let Some(fs) = app.current_file_state() {
                    if fs.sort_column == *col {
                        label.push_str(if fs.sort_ascending { " (▲)" } else { " (▼)" });
                    }
                }
                label
            },
        };
        
        let mut item = ListItem::new(label);
        if (*action == ContextMenuAction::Paste) && app.clipboard.is_none() {
            item = item.style(Style::default().fg(Color::DarkGray));
        }
        items.push(item);
    }

    let title = match target {
        crate::app::ContextMenuTarget::File(_) => " File ",
        crate::app::ContextMenuTarget::Folder(_) => " Folder ",
        crate::app::ContextMenuTarget::EmptySpace => " View ",
        crate::app::ContextMenuTarget::SidebarFavorite(_) => " Favorite ",
        crate::app::ContextMenuTarget::SidebarRemote(_) => " Remote ",
        crate::app::ContextMenuTarget::SidebarStorage(_) => " Storage ",
    };
    
    let menu_width = 25;
    let menu_height = items.len() as u16 + 2;
    let mut draw_x = x;
    let mut draw_y = y;
    if draw_x + menu_width > f.area().width { draw_x = f.area().width.saturating_sub(menu_width); }
    if draw_y + menu_height > f.area().height { draw_y = f.area().height.saturating_sub(menu_height); }

    let area = Rect::new(draw_x, draw_y, menu_width, menu_height);
    f.render_widget(Clear, area);
    f.render_widget(List::new(items).block(Block::default().title(title).borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(THEME.accent_secondary))), area);
}

fn draw_import_servers_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Import Servers (TOML) ").border_style(Style::default().fg(THEME.accent_primary));
    let inner = block.inner(area);
    f.render_widget(block, area);
    
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(2), Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)]).split(inner);
    
    f.render_widget(Paragraph::new("Enter path to server configuration file:"), chunks[0]);
    
    let input_area = chunks[1];
    f.render_widget(Paragraph::new("> ").style(Style::default().fg(THEME.accent_secondary)), Rect::new(input_area.x, input_area.y, 2, 1));
    f.render_widget(&app.input, Rect::new(input_area.x + 2, input_area.y, input_area.width.saturating_sub(2), 1));
    
    let footer_text = vec![Span::styled(" [Enter] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)), Span::raw("Import "), Span::styled(" [Esc] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw("Cancel")];
    f.render_widget(Paragraph::new(Line::from(footer_text)), chunks[3]);
}

fn draw_command_palette(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let inner = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Command Palette ").border_style(Style::default().fg(Color::Magenta)).inner(area);
    f.render_widget(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Command Palette ").border_style(Style::default().fg(Color::Magenta)), area);
    
    f.render_widget(Paragraph::new("> ").style(Style::default().fg(Color::Yellow)), Rect::new(inner.x, inner.y, 2, 1));
    f.render_widget(&app.input, Rect::new(inner.x + 2, inner.y, inner.width.saturating_sub(2), 1));
    
    let items: Vec<ListItem> = app.filtered_commands.iter().enumerate().map(|(i, cmd)| {
        let style = if i == app.command_index { Style::default().bg(Color::DarkGray).fg(Color::White) } else { Style::default() };
        ListItem::new(cmd.desc.clone()).style(style)
    }).collect();
    f.render_widget(List::new(items), Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1));
}

fn draw_rename_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    let block = Block::default().title(" Rename ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);
    
    if app.rename_selected {
        let text = if let Some(idx) = app.input.value.rfind('.') {
             if idx > 0 {
                 let stem_part = &app.input.value[..idx];
                 let ext_part = &app.input.value[idx..];
                 Line::from(vec![
                     Span::styled(stem_part, Style::default().bg(Color::Blue).fg(Color::White)),
                     Span::raw(ext_part)
                 ])
             } else {
                 Line::from(vec![Span::styled(&app.input.value, Style::default().bg(Color::Blue).fg(Color::White))])
             }
        } else {
             Line::from(vec![Span::styled(&app.input.value, Style::default().bg(Color::Blue).fg(Color::White))])
        };
        f.render_widget(Paragraph::new(text), inner);
    } else {
        f.render_widget(&app.input, inner);
    }
}

fn draw_new_folder_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    let block = Block::default().title(" New Folder ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);
}

fn draw_new_file_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    let block = Block::default().title(" New File ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);
}

fn draw_delete_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new("Delete selected item(s)? (y/n)").block(Block::default().title(" Delete ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Red))), area);
}

fn draw_properties_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 50, f.area()); 
    f.render_widget(Clear, area);
    
    let mut text = Vec::new();
    
    if let Some(fs) = app.current_file_state() {
        let target_path = fs.selected_index.and_then(|idx| fs.files.get(idx)).unwrap_or(&fs.current_path);
        
        let name = target_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| target_path.to_string_lossy().to_string());
        let parent = target_path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        
        text.push(Line::from(vec![Span::styled("Name: ", Style::default().fg(THEME.accent_secondary)), Span::raw(name)]));
        text.push(Line::from(vec![Span::styled("Location: ", Style::default().fg(THEME.accent_secondary)), Span::raw(parent)]));
        text.push(Line::from(""));
        
        if let Some(meta) = fs.metadata.get(target_path) {
            let type_str = if meta.is_dir { "Folder" } else { "File" };
            text.push(Line::from(vec![Span::styled("Type: ", Style::default().fg(THEME.accent_secondary)), Span::raw(type_str)]));
            text.push(Line::from(vec![Span::styled("Size: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_size(meta.size))]));
            text.push(Line::from(vec![Span::styled("Modified: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_time(meta.modified))]));
            text.push(Line::from(vec![Span::styled("Created: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_time(meta.created))]));
            text.push(Line::from(vec![Span::styled("Permissions: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_permissions(meta.permissions))]));
        } else {
             if fs.remote_session.is_none() {
                 if let Ok(m) = std::fs::metadata(target_path) {
                     let is_dir = m.is_dir();
                     text.push(Line::from(vec![Span::styled("Type: ", Style::default().fg(THEME.accent_secondary)), Span::raw(if is_dir { "Folder" } else { "File" })]));
                     text.push(Line::from(vec![Span::styled("Size: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_size(m.len()))]));
                     if let Ok(mod_time) = m.modified() {
                         text.push(Line::from(vec![Span::styled("Modified: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_time(mod_time))]));
                     }
                 } else {
                     text.push(Line::from(Span::styled("No metadata available", Style::default().fg(Color::DarkGray))));
                 }
             } else {
                 text.push(Line::from(Span::styled("No metadata available (Remote)", Style::default().fg(Color::DarkGray))));
             }
        }
    }

    let block = Block::default().title(" Properties ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Cyan));
    f.render_widget(Paragraph::new(text).block(block), area);
}

fn draw_settings_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 80, f.area()); f.render_widget(Clear, area);
    let block = Block::default().title(" Settings ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area); f.render_widget(block, area);
    let chunks = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Length(15), Constraint::Min(0)]).split(inner);
    let sections = vec![ListItem::new(" 󰟜 Columns "), ListItem::new(" 󰓩 Tabs "), ListItem::new(" 󰒓 General "), ListItem::new(" 󰒍 Remotes "), ListItem::new(" 󰌌 Shortcuts ")];
    let mut state = ScrollbarState::default(); 
    let sel = match app.settings_section { SettingsSection::Columns => 0, SettingsSection::Tabs => 1, SettingsSection::General => 2, SettingsSection::Remotes => 3, SettingsSection::Shortcuts => 4 };
    let items: Vec<ListItem> = sections.into_iter().enumerate().map(|(i, item)| {
        if i == sel { item.style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD)) } else { item }
    }).collect();
    f.render_widget(List::new(items).block(Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(Color::DarkGray))), chunks[0]);
    match app.settings_section {
        SettingsSection::Columns => draw_column_settings(f, chunks[1], app),
        SettingsSection::Tabs => draw_tab_settings(f, chunks[1], app),
        SettingsSection::General => draw_general_settings(f, chunks[1], app),
        SettingsSection::Remotes => draw_remote_settings(f, chunks[1], app),
        SettingsSection::Shortcuts => draw_shortcuts_settings(f, chunks[1], app),
    }
}

fn draw_shortcuts_settings(f: &mut Frame, area: Rect, _app: &App) {
    let shortcuts = vec![
        ("General", vec![
            ("Ctrl + q", "Quit Application"),
            ("Ctrl + g", "Open Settings"),
            ("Ctrl + Space", "Open Command Palette"),
            ("Ctrl + b", "Toggle Sidebar"),
        ]),
        ("Navigation", vec![
            ("↑ / ↓", "Move Selection"),
            ("Left / Right", "Change Pane / Enter/Leave Sidebar"),
            ("Enter", "Open Directory / File"),
            ("Shift + Enter", "Open Folder in New Tab"),
            ("Backspace", "Go to Parent Directory"),
            ("Alt + Left / Right", "Back / Forward in History"),
            ("~", "Go to Home Directory"),
            ("Middle Click / Space", "Preview File in Other Pane"),
        ]),
        ("View & Tabs", vec![
            ("Ctrl + s", "Toggle Split View"),
            ("Ctrl + t", "New Duplicate Tab"),
            ("Ctrl + h", "Toggle Hidden Files"),
            ("Ctrl + b", "Toggle Sidebar"),
            ("Ctrl + l / u", "Clear Search Filter"),
            ("Ctrl + z / y", "Undo / Redo (Rename/Move)"),
            ("F1", "Show this Help"),
        ]),
        ("File Operations", vec![
            ("F6", "Rename Selected"),
            ("Delete", "Delete Selected"),
            ("Alt + Enter", "Show Properties"),
        ]),
        ("Terminal", vec![
            ("Ctrl + n / o", "Open External Terminal"),
            ("g", "Open Terminal Here (Git Mode)"),
        ]),
    ];

    let mut rows = Vec::new();
    for (category, items) in shortcuts {
        rows.push(Row::new(vec![Cell::from(Span::styled(category, Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD))), Cell::from("")]));
        for (key, desc) in items {
            rows.push(Row::new(vec![
                Cell::from(Span::styled(key, Style::default().fg(Color::Yellow))),
                Cell::from(desc),
            ]));
        }
        rows.push(Row::new(vec![Cell::from(""), Cell::from("")])); // Spacer
    }

    let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(0)])
        .block(Block::default().title(" Keyboard Shortcuts ").borders(Borders::NONE));
    
    f.render_widget(table, area);
}

fn draw_column_settings(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);
    let titles = vec![" [Single] ", " [Split] "];
    let sel = match app.settings_target { SettingsTarget::SingleMode => 0, SettingsTarget::SplitMode => 1 };
    f.render_widget(Tabs::new(titles).block(Block::default().borders(Borders::BOTTOM).title(" Configure Mode ")).select(sel).highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), chunks[0]);
    let options = vec![(FileColumn::Size, "Size (s)"), (FileColumn::Modified, "Modified (m)"), (FileColumn::Permissions, "Permissions (p)")];
    let target = match app.settings_target { SettingsTarget::SingleMode => &app.single_columns, SettingsTarget::SplitMode => &app.split_columns };
    let items: Vec<ListItem> = options.iter().map(|(col, label)| {
        let prefix = if target.contains(col) { "[x] " } else { "[ ] " };
        ListItem::new(format!("{}{}", prefix, label))
    }).collect();
    f.render_widget(List::new(items).block(Block::default().title(" Visible Columns ").borders(Borders::NONE)), chunks[1]);
}

fn draw_tab_settings(f: &mut Frame, area: Rect, _app: &App) {
    f.render_widget(Paragraph::new("Tab settings placeholder"), area);
}

fn draw_general_settings(f: &mut Frame, area: Rect, app: &App) {
    let items = vec![
        ListItem::new(format!("[{}] Show Hidden Files (h)", if app.default_show_hidden { "x" } else { " " })),
        ListItem::new(format!("[{}] Confirm Delete (d)", if app.confirm_delete { "x" } else { " " })),
        ListItem::new(format!("Icon Mode: {:?} (i)", app.icon_mode)),
    ];
    f.render_widget(List::new(items).block(Block::default().title(" General Preferences ").borders(Borders::NONE)), area);
}

fn draw_remote_settings(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.remote_bookmarks.iter().map(|b| ListItem::new(format!("󰒍 {} ({}@{})", b.name, b.user, b.host))).collect();
    let list = if items.is_empty() { List::new(vec![ListItem::new("(No remote servers configured)").style(Style::default().fg(Color::DarkGray))]) } else { List::new(items) };
    let text = vec![Line::from("Manage your remote server bookmarks here."), Line::from(""), Line::from("Tip: You can bulk import servers by clicking the"), Line::from(vec![Span::styled("REMOTES [Import] ", Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)), Span::raw("header in the sidebar.")]), Line::from(""), Line::from("Current Servers:")];
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(7), Constraint::Min(0)]).split(area);
    f.render_widget(Paragraph::new(text), chunks[0]);
    f.render_widget(list.block(Block::default().borders(Borders::TOP).title(" Bookmarks ")), chunks[1]);
}

fn draw_add_remote_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(60, 40, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new("Add remote server modal placeholder").block(Block::default().title(" Add Remote ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Green))), area);
}

fn draw_highlight_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(30, 10, f.area()); // Fixed-ish percentage or small
    // Actually let's use absolute sizing for palette
    let area = Rect::new(
        (f.area().width.saturating_sub(34)) / 2,
        (f.area().height.saturating_sub(5)) / 2,
        34,
        5
    );
    
    f.render_widget(Clear, area);
    let block = Block::default().title(" Highlight ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let colors = vec![
        (1, " R ", Color::Red),
        (2, " G ", Color::Green),
        (3, " Y ", Color::Yellow),
        (4, " B ", Color::Blue),
        (5, " M ", Color::Magenta),
        (6, " C ", Color::Cyan),
        (0, " X ", Color::Reset),
    ];

    let mut spans = Vec::new();
    for (i, (code, label, color)) in colors.iter().enumerate() {
        let style = if *code == 0 { 
            Style::default().bg(Color::DarkGray).fg(Color::White) 
        } else { 
            Style::default().bg(*color).fg(Color::Black) 
        };
        spans.push(Span::styled(*label, style));
        if i < colors.len() - 1 {
            spans.push(Span::raw(" "));
        }
    }

    f.render_widget(Paragraph::new(Line::from(spans)).alignment(ratatui::layout::Alignment::Center), Rect::new(inner.x, inner.y + 1, inner.width, 1));
    f.render_widget(Paragraph::new("1   2   3   4   5   6   0").alignment(ratatui::layout::Alignment::Center).style(Style::default().fg(Color::DarkGray)), Rect::new(inner.x, inner.y + 2, inner.width, 1));
}