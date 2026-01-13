use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState, Tabs, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};
use std::time::SystemTime;
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
    // Force true color pure black background
    f.render_widget(Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0))), f.area());

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
    if matches!(app.mode, AppMode::AddRemote(_)) { draw_add_remote_modal(f, app); }
    if matches!(app.mode, AppMode::ImportServers) { draw_import_servers_modal(f, app); }
    if let AppMode::OpenWith(ref path) = app.mode { draw_open_with_modal(f, app, path); }
    if matches!(app.mode, AppMode::ConfirmReset) { draw_confirm_reset_modal(f, app); }
}

fn draw_confirm_reset_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Reset Column Widths? ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red));
    f.render_widget(Paragraph::new("Reset all columns to defaults? (y/Enter/n)").block(block), area);
}

fn draw_open_with_modal(f: &mut Frame, app: &App, path: &std::path::Path) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Open With... ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Info
            Constraint::Length(3), // Input
            Constraint::Min(0),    // Suggestions
        ])
        .split(inner);

    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    f.render_widget(Paragraph::new(format!("Opening: {}", file_name)), chunks[0]);

    let input_block = Block::default().borders(Borders::ALL).title(" Application / Command ").border_style(Style::default().fg(Color::Cyan));
    f.render_widget(Paragraph::new(app.input.value.as_str()).block(input_block), chunks[1]);

    // Simple common suggestions based on extension
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let suggestions = match ext.as_str() {
        "txt" | "md" | "rs" | "toml" | "json" | "c" | "cpp" | "py" | "js" | "ts" => vec!["code", "vim", "nano", "kate", "subl", "gedit"],
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" => vec!["gwenview", "feh", "imv", "nomacs", "display"],
        "pdf" => vec!["okular", "evince", "zathura", "firefox", "chromium"],
        "mp4" | "mkv" | "avi" | "mov" | "webm" => vec!["vlc", "mpv", "totem"],
        "mp3" | "wav" | "ogg" | "flac" => vec!["vlc", "clementine", "audacious"],
        "zip" | "tar" | "gz" | "7z" => vec!["ark", "file-roller"],
        _ => vec!["xdg-open", "dolphin", "code", "vim"],
    };

    let sug_text = format!("Common: {}", suggestions.join(", "));
    f.render_widget(Paragraph::new(sug_text).style(Style::default().fg(Color::DarkGray)), chunks[2]);
}

fn draw_processes_view(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .title(" System Processes ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Magenta));
    
    let inner = block.inner(area);
    f.render_widget(block, area);

    let header_cells = ["PID", "Name", "CPU %", "Memory"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)));
    let header = Row::new(header_cells).height(1).bottom_margin(1);

    let rows = app.system_state.processes.iter().map(|p| {
        let cpu_color = if p.cpu > 50.0 { Color::Red } else if p.cpu > 20.0 { Color::Yellow } else { Color::Green };
        let mem_color = if p.mem > 500.0 { Color::Red } else if p.mem > 100.0 { Color::Yellow } else { Color::Cyan };

        let cpu_bar_width = (p.cpu / 10.0).min(10.0) as usize;
        let cpu_bar = format!("{}{} {:.1}%", "█".repeat(cpu_bar_width), "░".repeat(10 - cpu_bar_width), p.cpu);

        Row::new(vec![
            Cell::from(p.pid.to_string()).style(Style::default().fg(Color::DarkGray)),
            Cell::from(p.name.clone()).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Cell::from(cpu_bar).style(Style::default().fg(cpu_color)),
            Cell::from(format!("{:.1} MB", p.mem)).style(Style::default().fg(mem_color)),
        ])
    });

    let table = Table::new(rows, [
        Constraint::Length(8),
        Constraint::Min(20),
        Constraint::Length(25),
        Constraint::Length(15),
    ]).header(header).column_spacing(2);

    f.render_widget(table, inner);
}

fn draw_global_header(f: &mut Frame, area: Rect, sidebar_width: u16, app: &mut App) {
    let _now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis();
    
    let pane_count = app.panes.len();

    // Toolbar Icons Cluster (Far Left)
    let back_icon = Icon::Back.get(app.icon_mode);
    let forward_icon = Icon::Forward.get(app.icon_mode);
    let split_icon = Icon::Split.get(app.icon_mode);
    let burger_icon = Icon::Burger.get(app.icon_mode);
    let reset_icon = Icon::Refresh.get(app.icon_mode);

    app.header_icon_bounds.clear();
    
    let icons = [
        (burger_icon, "burger", "Settings"),
        (back_icon, "back", "Back"),
        (forward_icon, "forward", "Forward"),
        (split_icon, "split", "Toggle Split"),
        (reset_icon, "reset", "Reset Columns"),
    ];

    // Start icons at the left side of the sidebar with 1 padding
    let mut cur_icon_x = area.x + 1;
    
    for (i, (icon, id, _desc)) in icons.into_iter().enumerate() {
        let rect = Rect::new(cur_icon_x, area.y, 3, 1);
        
        let is_hovered = app.mouse_pos.1 == area.y && app.mouse_pos.0 >= rect.x && app.mouse_pos.0 < rect.x + rect.width;
        let is_kb_focused = matches!(app.mode, AppMode::Header(idx) if idx == i);
        
        let mut style = Style::default().fg(THEME.accent_secondary);
        if is_kb_focused || is_hovered {
            style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
            if is_hovered {
                app.hovered_header_icon = Some(id.to_string());
            }
        }

        f.render_widget(Paragraph::new(format!(" {} ", icon)).style(style), rect);
        app.header_icon_bounds.push((rect, id.to_string()));
        cur_icon_x += 3; 
    }

    // Draw description if hovered
    if let Some(hovered_id) = &app.hovered_header_icon {
        if let Some((_, _, desc)) = icons.iter().find(|(_, id, _)| id == hovered_id) {
            let desc_text = format!(" [ {} ] ", desc);
            let desc_rect = Rect::new(cur_icon_x + 1, area.y, desc_text.len() as u16, 1);
            f.render_widget(Paragraph::new(desc_text).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), desc_rect);
        }
    }
    app.hovered_header_icon = None; // Reset for next frame

    if pane_count == 0 { return; }
    let start_x = if app.show_sidebar { 
        std::cmp::max(area.x + sidebar_width, cur_icon_x + 1)
    } else {
        cur_icon_x + 1
    };
    let pane_chunks = Layout::default().direction(Direction::Horizontal).constraints(vec![Constraint::Percentage(100 / pane_count as u16); pane_count]).split(Rect::new(start_x, area.y, area.width.saturating_sub(start_x), 1));

    app.tab_bounds.clear();
    let mut global_tab_idx = 4; // Start after 4 icons
    for (p_i, pane) in app.panes.iter().enumerate() {
        let chunk = pane_chunks[p_i];
        let mut current_x = chunk.x;
        for (t_i, tab) in pane.tabs.iter().enumerate() {
            let mut name = if !tab.search_filter.is_empty() { format!("Search: {}", tab.search_filter) } 
                           else { tab.current_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("/".to_string()) };
            if let Some(branch) = &tab.git_branch { name = format!("{} ({})", name, branch); }
            let is_active_tab = t_i == pane.active_tab_index;
            let is_focused_pane = p_i == app.focused_pane_index && !app.sidebar_focus;
            
            let mut style = if is_active_tab { if is_focused_pane { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(THEME.accent_primary) } } 
                        else { Style::default().fg(Color::DarkGray) };
            
            if let AppMode::Header(idx) = app.mode {
                if idx == global_tab_idx {
                    style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
                }
            }

            let text = format!(" {} ", name);
            let width = text.len() as u16;
            if current_x + width > chunk.x + chunk.width { break; }
            let rect = Rect::new(current_x, area.y, width, 1);
            f.render_widget(Paragraph::new(text).style(style), rect);
            app.tab_bounds.push((rect, p_i, t_i));
            current_x += width + 1;
            global_tab_idx += 1;
        }
    }
}

fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
    match app.current_view {
        CurrentView::Files => {
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
        CurrentView::Processes => {
            draw_processes_view(f, area, app);
        }
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

