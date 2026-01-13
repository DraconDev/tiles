use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState, Tabs, Scrollbar, ScrollbarOrientation, ScrollbarState, Sparkline, Gauge},
    Frame,
};
use std::time::SystemTime;
use std::collections::HashMap;

use crate::app::{App, AppMode, CurrentView, MonitorSubview, FileColumn, ProcessColumn, SidebarTarget, SidebarBounds, DropTarget, SettingsSection, SettingsTarget, FileCategory};
use crate::ui::theme::THEME;
use crate::icons::Icon;
use terma::layout::centered_rect;
use terma::utils::{format_size, format_time, format_permissions, format_datetime_smart, highlight_code, draw_stat_bar};

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

    let is_processes = app.current_view == CurrentView::Processes;

    if is_processes {
        draw_monitor_page(f, f.area(), app);
    } else {
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
    }

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
    if let AppMode::OpenWith(path) = &app.mode { draw_open_with_modal(f, app, path); }
    if matches!(app.mode, AppMode::Engage) { draw_editor_overlay(f, app); }
}

fn draw_monitor_page(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // 1. Modern Navigation Bar (Plasma/Breeze Style)
    let nav_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Rgb(45, 48, 50)));
    let nav_inner = nav_block.inner(chunks[0]);
    f.render_widget(nav_block, chunks[0]);

    let nav_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(60), Constraint::Length(30), Constraint::Length(12)])
        .split(nav_inner);

    // Navigation Tabs with Icons
    let subviews: [(MonitorSubview, &str, &str); 4] = [
        (MonitorSubview::Overview, " 󰊚 ", "Overview"),
        (MonitorSubview::Applications, " 󰀻 ", "Applications"),
        (MonitorSubview::History, " 󰔠 ", "History"),
        (MonitorSubview::Processes, " 󰑮 ", "Processes"),
    ];

    app.monitor_subview_bounds.clear();
    let mut cur_x = nav_layout[0].x;
    
    for (view, icon, name) in subviews {
        let is_active = app.monitor_subview == view;
        let text = format!("{}{}", icon, name);
        let width = text.len() as u16 + 2;
        let rect = Rect::new(cur_x, nav_layout[0].y, width, 1);
        
        let mut style = if is_active {
            Style::default().fg(Color::Rgb(61, 174, 233)).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(180, 180, 180))
        };

        if app.mouse_pos.1 == nav_layout[0].y && app.mouse_pos.0 >= rect.x && app.mouse_pos.0 < rect.x + rect.width {
            style = style.fg(Color::White).bg(Color::Rgb(40, 45, 50));
        }

        f.render_widget(Paragraph::new(text).style(style), rect);
        if is_active {
            f.render_widget(Paragraph::new("▔".repeat(width as usize)).style(Style::default().fg(Color::Rgb(61, 174, 233))), Rect::new(rect.x, rect.y + 1, rect.width, 1));
        }

        app.monitor_subview_bounds.push((rect, view));
        cur_x += width + 2;
    }

    // Modern Search Box
    let search_text = if app.process_search_filter.is_empty() {
        Line::from(vec![Span::styled(" 󰍉 Search...", Style::default().fg(Color::DarkGray))])
    } else {
        Line::from(vec![
            Span::styled(" 󰍉 ", Style::default().fg(Color::Rgb(61, 174, 233))),
            Span::styled(&app.process_search_filter, Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        ])
    };
    f.render_widget(
        Paragraph::new(search_text)
            .block(Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Rgb(60, 65, 70)))),
        nav_layout[1]
    );

    // Action Region (Close)
    let exit_text = " 󰅖 Close ";
    let mut exit_style = Style::default().fg(Color::Rgb(231, 76, 60));
    if app.mouse_pos.1 == nav_layout[2].y && app.mouse_pos.0 >= nav_layout[2].x && app.mouse_pos.0 < nav_layout[2].x + nav_layout[2].width {
        exit_style = exit_style.bg(Color::Rgb(60, 20, 20)).fg(Color::White);
    }
    f.render_widget(Paragraph::new(exit_text).style(exit_style).alignment(ratatui::layout::Alignment::Right), nav_layout[2]);

    // 2. Content Area
    let content_area = chunks[1].inner(ratatui::layout::Margin { horizontal: 1, vertical: 1 });
    match app.monitor_subview {
        MonitorSubview::Overview => draw_monitor_overview(f, content_area, app),
        MonitorSubview::Processes => draw_processes_view(f, content_area, app),
        MonitorSubview::History => draw_monitor_history(f, content_area, app),
        MonitorSubview::Applications => draw_monitor_applications(f, content_area, app),
    }
}

fn draw_monitor_overview(f: &mut Frame, area: Rect, app: &mut App) {
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    // Left Column: CPU & Memory
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_layout[0]);

    // CPU Card
    let cpu_block = Block::default()
        .title(Span::styled(" CPU Load ", Style::default().fg(Color::Rgb(46, 204, 113)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(40, 45, 50)));
    let cpu_inner = cpu_block.inner(left_chunks[0]);
    f.render_widget(cpu_block, left_chunks[0]);

    let cpu_card_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(cpu_inner);

    let cpu_gauge = Gauge::default()
        .gauge_style(Style::default().fg(if app.system_state.cpu_usage > 80.0 { Color::Red } else { Color::Rgb(46, 204, 113) }))
        .ratio((app.system_state.cpu_usage / 100.0).clamp(0.0, 1.0) as f64)
        .label(format!("{:.1}%", app.system_state.cpu_usage));
    f.render_widget(cpu_gauge, cpu_card_layout[0]);

    let cpu_history: Vec<u64> = app.system_state.cpu_history.iter().copied().collect();
    f.render_widget(Sparkline::default().data(&cpu_history).style(Style::default().fg(Color::Rgb(46, 204, 113))), cpu_card_layout[1]);

    // Memory Card
    let mem_block = Block::default()
        .title(Span::styled(" Memory Usage ", Style::default().fg(Color::Rgb(155, 89, 182)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(40, 45, 50)));
    let mem_inner = mem_block.inner(left_chunks[1]);
    f.render_widget(mem_block, left_chunks[1]);

    let mem_card_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(mem_inner);

    let mem_used = app.system_state.mem_usage;
    let mem_total = app.system_state.total_mem;
    let mem_ratio = if mem_total > 0.0 { (mem_used / mem_total).clamp(0.0, 1.0) } else { 0.0 };
    let mem_gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Rgb(155, 89, 182)))
        .ratio(mem_ratio)
        .label(format!("{:.1}GB / {:.1}GB", mem_used, mem_total));
    f.render_widget(mem_gauge, mem_card_layout[0]);

    let mem_history: Vec<u64> = app.system_state.mem_history.iter().copied().collect();
    f.render_widget(Sparkline::default().data(&mem_history).style(Style::default().fg(Color::Rgb(155, 89, 182))), mem_card_layout[1]);

    // Right Column: Storage
    let storage_block = Block::default()
        .title(Span::styled(" Storage Devices ", Style::default().fg(Color::Rgb(241, 196, 15)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(40, 45, 50)));
    let storage_inner = storage_block.inner(main_layout[1]);
    f.render_widget(storage_block, main_layout[1]);

    let disk_list: Vec<ListItem> = app.system_state.disks.iter().map(|disk| {
        let ratio = (disk.used_space / disk.total_space).clamp(0.0, 1.0);
        let color = if ratio > 0.9 { Color::Red } else if ratio > 0.7 { Color::Yellow } else { Color::Rgb(241, 196, 15) };
        
        let bar_width = 20;
        let filled = (ratio * bar_width as f64) as usize;
        let bar = format!("|{}{}|", "█".repeat(filled), "░".repeat(bar_width - filled));

        ListItem::new(vec![
            Line::from(vec![Span::styled(&disk.name, Style::default().add_modifier(Modifier::BOLD))]),
            Line::from(vec![
                Span::styled(bar, Style::default().fg(color)),
                Span::raw(format!(" {:.0}% used", ratio * 100.0))
            ]),
            Line::from(vec![Span::styled(format!("Free: {:.1}GB", disk.available_space / 1024.0 / 1024.0 / 1024.0), Style::default().fg(Color::DarkGray))]),
            Line::from(""),
        ])
    }).collect();

    f.render_widget(List::new(disk_list), storage_inner);
}

fn draw_monitor_history(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let cpu_history: Vec<u64> = app.system_state.cpu_history.iter().copied().collect();
    let cpu_block = Block::default()
        .title(Span::styled(" CPU History ", Style::default().fg(Color::Rgb(46, 204, 113)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(40, 45, 50)));
    f.render_widget(Sparkline::default().block(cpu_block).data(&cpu_history).max(100).style(Style::default().fg(Color::Rgb(46, 204, 113))), chunks[0]);

    let mem_history: Vec<u64> = app.system_state.mem_history.iter().copied().collect();
    let mem_block = Block::default()
        .title(Span::styled(" Memory History ", Style::default().fg(Color::Rgb(155, 89, 182)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(40, 45, 50)));
    f.render_widget(Sparkline::default().block(mem_block).data(&mem_history).max(100).style(Style::default().fg(Color::Rgb(155, 89, 182))), chunks[1]);
}

fn draw_monitor_applications(f: &mut Frame, area: Rect, app: &mut App) {
    let current_user = std::env::var("USER").unwrap_or_else(|_| "dracon".to_string());
    
    // Better application filtering: user processes with significant activity or common app names
    let mut app_procs: Vec<_> = app.system_state.processes.iter()
        .filter(|p| {
            let is_user = p.user == current_user;
            let is_significant = p.cpu > 0.1 || p.mem > 50.0;
            let is_app = !p.name.starts_with('[') && !p.name.contains("kworker") && !p.name.contains("systemd");
            
            let matches_filter = if app.process_search_filter.is_empty() { true } else {
                p.name.to_lowercase().contains(&app.process_search_filter.to_lowercase())
            };

            is_user && is_app && is_significant && matches_filter
        })
        .collect();

    app_procs.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));

    let block = Block::default()
        .title(Span::styled(" Active Applications ", Style::default().fg(Color::Rgb(61, 174, 233)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(40, 45, 50)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app_procs.is_empty() {
        f.render_widget(Paragraph::new("No active user applications matching filter.").alignment(ratatui::layout::Alignment::Center), centered_rect(60, 10, inner));
        return;
    }

    let rows = app_procs.iter().map(|p| {
        let name_lower = p.name.to_lowercase();
        let (icon, color) = if name_lower.contains("chrome") || name_lower.contains("firefox") || name_lower.contains("browser") { ("󰈹 ", Color::Rgb(231, 76, 60)) }
                            else if name_lower.contains("code") || name_lower.contains("vim") || name_lower.contains("emacs") { ("󰨞 ", Color::Rgb(61, 174, 233)) }
                            else if name_lower.contains("term") || name_lower.contains("fish") || name_lower.contains("bash") { ("󰆍 ", Color::Rgb(46, 204, 113)) }
                            else if name_lower.contains("discord") || name_lower.contains("slack") || name_lower.contains("telegram") { ("󰙯 ", Color::Rgb(155, 89, 182)) }
                            else if name_lower.contains("spotify") || name_lower.contains("vlc") { ("󰝚 ", Color::Rgb(26, 188, 156)) }
                            else { ("󰀻 ", Color::White) };

        Row::new(vec![
            Cell::from(format!("{} {}", icon, p.name)).style(Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Cell::from(format!("{:.1}%", p.cpu)).style(Style::default().fg(if p.cpu > 30.0 { Color::Red } else { Color::Green })),
            Cell::from(format!("{:.1} MB", p.mem)).style(Style::default().fg(Color::Rgb(61, 174, 233))),
            Cell::from(p.pid.to_string()).style(Style::default().fg(Color::DarkGray)),
            Cell::from(p.status.clone()).style(Style::default().fg(Color::DarkGray)),
        ])
    });

    let table = Table::new(rows, [
        Constraint::Min(30),
        Constraint::Length(10),
        Constraint::Length(15),
        Constraint::Length(10),
        Constraint::Length(15),
    ])
    .header(Row::new(vec!["Application", "CPU", "Memory", "PID", "Status"]).style(Style::default().add_modifier(Modifier::BOLD)))
    .row_highlight_style(Style::default().bg(Color::Rgb(45, 50, 55)))
    .column_spacing(2);

    f.render_widget(table, inner);
}

fn draw_processes_view(f: &mut Frame, area: Rect, app: &mut App) {
    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(40, 45, 50)));
    
    let table_inner = table_block.inner(area);
    f.render_widget(table_block, area);

    let column_constraints = [
        Constraint::Length(8),
        Constraint::Min(25),
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(10),
    ];

    app.process_column_bounds.clear();
    let header_rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(column_constraints)
        .split(Rect::new(table_inner.x, table_inner.y, table_inner.width, 1));

    let header_cells = ["PID", "Name", "User", "Status", "CPU%", "Mem%"]
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let col = match *h {
                "PID" => ProcessColumn::Pid, "Name" => ProcessColumn::Name, "User" => ProcessColumn::User,
                "Status" => ProcessColumn::Status, "CPU%" => ProcessColumn::Cpu, "Mem%" => ProcessColumn::Mem,
                _ => ProcessColumn::Pid,
            };
            app.process_column_bounds.push((header_rects[i], col));

            let mut style = Style::default().fg(Color::Rgb(180, 180, 180)).add_modifier(Modifier::BOLD);
            let mut text = h.to_string();
            if app.process_sort_col == col {
                style = style.fg(Color::Rgb(61, 174, 233));
                text.push_str(if app.process_sort_asc { " ▲" } else { " ▼" });
            }
            Cell::from(text).style(style)
        });
    let header = Row::new(header_cells).height(1).style(Style::default().bg(Color::Rgb(30, 33, 35)));

    let rows = app.system_state.processes.iter().map(|p| {
        Row::new(vec![
            Cell::from(p.pid.to_string()).style(Style::default().fg(Color::DarkGray)),
            Cell::from(p.name.clone()).style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Cell::from(p.user.clone()).style(Style::default().fg(Color::Rgb(61, 174, 233))),
            Cell::from(p.status.clone()).style(Style::default().fg(Color::DarkGray)),
            Cell::from(format!("{:.1}", p.cpu)).style(Style::default().fg(if p.cpu > 50.0 { Color::Red } else { Color::Rgb(46, 204, 113) })),
            Cell::from(format!("{:.1}", p.mem)).style(Style::default().fg(Color::Rgb(155, 89, 182))),
        ])
    });

    let table = Table::new(rows, column_constraints)
    .header(header)
    .row_highlight_style(Style::default().bg(Color::Rgb(45, 50, 55)).fg(Color::Rgb(61, 174, 233)).add_modifier(Modifier::BOLD))
    .column_spacing(1);

    f.render_stateful_widget(table, table_inner, &mut app.process_table_state);
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

fn draw_global_header(f: &mut Frame, area: Rect, sidebar_width: u16, app: &mut App) {
    let _now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis();
    
    let pane_count = app.panes.len();

    // Toolbar Icons Cluster (Far Left)
    let burger_icon = Icon::Burger.get(app.icon_mode);
    let back_icon = Icon::Back.get(app.icon_mode);
    let forward_icon = Icon::Forward.get(app.icon_mode);
    let split_icon = Icon::Split.get(app.icon_mode);
    let monitor_icon = Icon::Monitor.get(app.icon_mode);

    let icons = vec![
        (burger_icon, "burger", "Settings"),
        (back_icon, "back", "Back"),
        (forward_icon, "forward", "Forward"),
        (split_icon, "split", "Split View"),
        (monitor_icon, "monitor", "System Monitor"),
    ];

    // Start icons at the left side of the sidebar with 1 padding
    let mut cur_icon_x = area.x + 1;
    app.header_icon_bounds.clear();
    let mut hovered_tip = None;
    
    for (i, (icon, id, desc)) in icons.iter().enumerate() {
        let rect = Rect::new(cur_icon_x, area.y, 3, 1);
        let id_str = id.to_string();
        
        let is_hovered = app.mouse_pos.1 == area.y && app.mouse_pos.0 >= rect.x && app.mouse_pos.0 < rect.x + rect.width;
        let is_kb_focused = matches!(app.mode, AppMode::Header(idx) if idx == i);
        
        let mut style = Style::default().fg(THEME.accent_secondary);
        if is_kb_focused || is_hovered {
            style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
            if is_hovered {
                app.hovered_header_icon = Some(id_str.clone());
                hovered_tip = Some(desc.to_string());
            }
        }

        f.render_widget(Paragraph::new(format!(" {} ", icon)).style(style), rect);
        app.header_icon_bounds.push((rect, id_str));
        cur_icon_x += 3; 
    }

    // Draw description if hovered
    if let Some(desc) = hovered_tip {
        let desc_text = format!(" [ {} ] ", desc);
        let desc_rect = Rect::new(cur_icon_x + 1, area.y, desc_text.len() as u16, 1);
        f.render_widget(Paragraph::new(desc_text).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), desc_rect);
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
            // This is handled in draw() top level now
        }
    }
}

fn draw_file_view(f: &mut Frame, area: Rect, app: &mut App, pane_idx: usize, is_focused: bool, borders: Borders) {
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
            FileColumn::Size => Constraint::Length(9),
            FileColumn::Modified => Constraint::Length(12),
            FileColumn::Created => Constraint::Length(12),
            FileColumn::Extension => Constraint::Length(5),
            FileColumn::Permissions => Constraint::Length(10),
        }).collect();

        let dummy_block = Block::default().borders(borders);
        let inner_area = dummy_block.inner(area);
        let column_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints.clone())
            .spacing(1)
            .split(inner_area);

        file_state.column_bounds.clear();
        for (i, col_type) in file_state.columns.iter().enumerate() {
            if i < column_layout.len() { file_state.column_bounds.push((column_layout[i], *col_type)); }
        }

        let name_col_width = column_layout.get(0).map(|r| r.width as usize).unwrap_or(20);

        let header_cells = file_state.columns.iter().enumerate().map(|(_i, c)| {
            let base_name = match c { 
                FileColumn::Name => "Name", FileColumn::Size => "Size", FileColumn::Modified => "Modified", 
                FileColumn::Created => "Created", FileColumn::Extension => "Ext", FileColumn::Permissions => "Perms" 
            };
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
                    let category = crate::modules::files::get_file_category(path);
                    
                    let mut final_style = if is_dir { 
                        Style::default().fg(THEME.accent_secondary) 
                    } else { 
                        match category {
                            FileCategory::Archive => Style::default().fg(Color::Rgb(255, 170, 0)), // Orange
                            FileCategory::Image => Style::default().fg(Color::Rgb(255, 100, 255)),   // Pink/Magenta
                            FileCategory::Script => Style::default().fg(Color::Rgb(0, 255, 150)),  // Teal/Cyan-Green
                            FileCategory::Audio | FileCategory::Video => Style::default().fg(Color::Rgb(255, 255, 100)), // Yellow
                            FileCategory::Document => Style::default().fg(Color::Rgb(100, 200, 255)), // Light Blue
                            FileCategory::Text => Style::default().fg(Color::White),
                            _ => Style::default().fg(THEME.fg),
                        }
                    };

                    if let Some(c) = app.path_colors.get(path) {
                        let color = match c { 
                            1 => Color::Red, 
                            2 => Color::Green, 
                            3 => Color::Yellow, 
                            4 => Color::Blue, 
                            5 => Color::Magenta, 
                            6 => Color::Cyan, 
                            _ => Color::White 
                        };
                        final_style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                    }
                    if let Some((ref cb_path, op)) = app.clipboard { 
                        if op == crate::app::ClipboardOp::Cut && cb_path == path { 
                            final_style = final_style.add_modifier(Modifier::DIM); 
                        } 
                    }
                    
                    let icon = if is_dir { 
                        Icon::Folder.get(app.icon_mode) 
                    } else { 
                        match category { 
                            FileCategory::Archive => Icon::Archive.get(app.icon_mode), 
                            FileCategory::Image => Icon::Image.get(app.icon_mode), 
                            FileCategory::Audio => Icon::Audio.get(app.icon_mode), 
                            FileCategory::Video => Icon::Video.get(app.icon_mode), 
                            FileCategory::Script => Icon::Script.get(app.icon_mode), 
                            FileCategory::Document => Icon::Document.get(app.icon_mode), 
                            _ => Icon::File.get(app.icon_mode) 
                        } 
                    };
                    let mut dn = if i > file_state.local_count {
                        let fs = path.to_string_lossy();
                        if fs.starts_with("/home/dracon") {
                            fs.replacen("/home/dracon", "~", 1)
                        } else {
                            fs.to_string()
                        }
                    } else {
                        name.to_string()
                    };

                    if app.starred.contains(path) {
                        dn.push_str(" [*]");
                    }

                    if dn.len() > name_col_width && name_col_width > 5 {
                        let kl = name_col_width.saturating_sub(4);
                        dn = format!("{}...", &dn[..kl]);
                    }
                    Cell::from(format!("{}{}", icon, dn)).style(final_style)
                }
                FileColumn::Size => { let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false); let style = if is_dir { Style::default().fg(THEME.accent_secondary) } else { Style::default().fg(THEME.fg) }; if is_dir { Cell::from("<DIR>").style(style) } else { Cell::from(format_size(metadata.map(|m| m.size).unwrap_or(0))).style(style) } }
                FileColumn::Modified => {
                    let time = metadata.map(|m| m.modified).unwrap_or(SystemTime::UNIX_EPOCH);
                    let text = if app.smart_date { format_datetime_smart(time) } else { format_time(time) };
                    Cell::from(text).style(Style::default().fg(THEME.fg))
                },
                FileColumn::Created => {
                    let time = metadata.map(|m| m.created).unwrap_or(SystemTime::UNIX_EPOCH);
                    let text = if app.smart_date { format_datetime_smart(time) } else { format_time(time) };
                    Cell::from(text).style(Style::default().fg(THEME.fg))
                },
                FileColumn::Extension => Cell::from(metadata.map(|m| m.extension.as_str()).unwrap_or("")).style(Style::default().fg(THEME.fg)),
                FileColumn::Permissions => Cell::from(format_permissions(metadata.map(|m| m.permissions).unwrap_or(0))).style(Style::default().fg(THEME.fg)),
            });
            let mut row_style = Style::default();
            if is_multi_selected { row_style = row_style.bg(Color::Rgb(100, 0, 0)).fg(Color::White); }
            Row::new(cells).style(row_style)
        });

        let mut breadcrumb_spans = Vec::new();
        file_state.breadcrumb_bounds.clear();
        let path = file_state.current_path.clone();
        let components: Vec<_> = path.components().collect();
        let mut cur_p = std::path::PathBuf::new();
        let mut cur_x = area.x + 2;
        let tc = components.len();
        for (i, comp) in components.iter().enumerate() {
            match comp { std::path::Component::RootDir => cur_p.push("/"), std::path::Component::Prefix(p) => cur_p.push(p.as_os_str()), std::path::Component::Normal(name) => cur_p.push(name), _ => continue }
            let d_name = if comp.as_os_str() == "/" { "/".to_string() } else { comp.as_os_str().to_string_lossy().to_string() };
            if !d_name.is_empty() {
                let sp = cur_p.clone();
                let is_hovered = file_state.hovered_breadcrumb.as_ref() == Some(&sp);
                let is_last = i == tc - 1;
                let fg = if is_hovered { Color::Rgb(255, 255, 0) } else if is_last { THEME.accent_secondary } else { Color::Rgb(100, 100, 110) };
                let mut style = Style::default().fg(fg);
                if is_last { style = style.add_modifier(Modifier::BOLD); }
                if is_hovered { style = style.add_modifier(Modifier::UNDERLINED); }
                let segment = if is_last { format!(" [ {} ] ", d_name) } else { format!(" {} ", d_name) };
                breadcrumb_spans.push(Span::styled(segment.clone(), style));
                file_state.breadcrumb_bounds.push((Rect::new(cur_x, area.y, segment.len() as u16, 1), sp));
                cur_x += segment.len() as u16;
                if !is_last { breadcrumb_spans.push(Span::styled("›", Style::default().fg(Color::Rgb(60, 60, 70)))); cur_x += 1; }
            }
        }
        if !file_state.search_filter.is_empty() { breadcrumb_spans.push(Span::styled(format!(" [ {} ]", file_state.search_filter), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))); }

        let mut border_style = if is_focused { 
            let pulse = ((SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis() % 1500) as f32 / 1500.0 * std::f32::consts::PI * 2.0).sin() * 0.5 + 0.5;
            let r = (255.0 * (0.7 + 0.3 * pulse)) as u8; let g = (0.0 * (0.7 + 0.3 * pulse)) as u8; let b = (85.0 * (0.7 + 0.3 * pulse)) as u8;
            Style::default().fg(Color::Rgb(r, g, b)).add_modifier(Modifier::BOLD)
        } else { Style::default().fg(THEME.border_inactive) };
        if matches!(app.hovered_drop_target, Some(DropTarget::Pane(idx)) if idx == pane_idx) { border_style = Style::default().fg(Color::Rgb(0, 255, 200)).add_modifier(Modifier::BOLD); }
        let block = Block::default().borders(borders).border_type(BorderType::Rounded).title(Line::from(breadcrumb_spans)).border_style(border_style);

        let table = Table::new(rows, constraints.clone()).header(Row::new(header_cells).height(1)).block(block.clone()).column_spacing(1)
            .row_highlight_style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD));

        f.render_stateful_widget(table, area, &mut render_state);
        *file_state.table_state.offset_mut() = render_state.offset();

        let vr = area.height.saturating_sub(3) as usize;
        if file_state.files.len() > vr {
            let sb = Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight).begin_symbol(Some("▲")).end_symbol(Some("▼"));
            let mut ss = ScrollbarState::new(file_state.files.len()).position(file_state.table_state.offset()).viewport_content_length(vr);
            f.render_stateful_widget(sb, area, &mut ss);
        }
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),      // Log, Clipboard & Shortcuts
            Constraint::Length(30),  // Selection Summary
            Constraint::Percentage(30), // CPU/MEM Stats (Fluid)
        ])
        .split(area);

    // 1. Left Section: ^Q Quit, Activity Log, Clipboard & Essential Shortcuts
    let mut left_spans = vec![
        Span::styled(" ^Q ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw("Quit "),
    ];
    
    // Log
    if let Some((msg, time)) = &app.last_action_msg {
        if time.elapsed().as_secs() < 5 {
            left_spans.push(Span::styled(format!(" [ SYSTEM ] {} ", msg), Style::default().fg(THEME.accent_secondary).bg(Color::Rgb(20, 25, 30))));
        }
    }
    
    // Clipboard
    if let Some((ref path, op)) = app.clipboard {
        let op_str = match op { crate::app::ClipboardOp::Copy => "COPY", crate::app::ClipboardOp::Cut => "CUT" };
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string());
        left_spans.push(Span::styled(format!(" [ {} ] {} ", op_str, name), Style::default().fg(Color::Yellow).bg(Color::Rgb(30, 30, 20))));
    }

    // Spacing
    if left_spans.len() > 2 {
        left_spans.push(Span::raw("  "));
    }

    // Reduced Shortcuts
    let shortcuts = vec![
        Span::styled(" ^B ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Side "),
        Span::styled(" ^P ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Split "),
        Span::styled(" ^T ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Tab "),
    ];
    left_spans.extend(shortcuts);

    f.render_widget(Paragraph::new(Line::from(left_spans)), chunks[0]);

    // 2. Middle Section: Selection Summary
    if let Some(fs) = app.current_file_state() {
        let sel_count = if !fs.multi_select.is_empty() { fs.multi_select.len() } else if fs.selected_index.is_some() { 1 } else { 0 };
        let total_count = fs.files.len();
        let summary = format!(" SEL: {} / {} ", sel_count, total_count);
        f.render_widget(Paragraph::new(summary).style(Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)).alignment(ratatui::layout::Alignment::Right), chunks[1]);
    }

    // 3. Right Section: CPU/MEM Stats
    let stats_layout = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(chunks[2]);
    
    let cpu_bar = draw_stat_bar("CPU", app.system_state.cpu_usage, 100.0, stats_layout[0].width, THEME.fg);
    let mem_usage = (app.system_state.mem_usage / app.system_state.total_mem.max(1.0)) as f32 * 100.0;
    let mem_bar = draw_stat_bar("MEM", mem_usage, 100.0, stats_layout[1].width, THEME.fg);
    
    f.render_widget(Paragraph::new(cpu_bar).alignment(ratatui::layout::Alignment::Right), stats_layout[0]);
    f.render_widget(Paragraph::new(mem_bar).alignment(ratatui::layout::Alignment::Right), stats_layout[1]);
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
            ContextMenuAction::OpenWith => format!(" {} Open With...", Icon::Split.get(app.icon_mode)),
            ContextMenuAction::Edit => format!(" {} Edit", Icon::Document.get(app.icon_mode)),
            ContextMenuAction::Run => format!(" {} Run", Icon::Video.get(app.icon_mode)),
            ContextMenuAction::RunTerminal => format!(" {} Run in Terminal", Icon::Script.get(app.icon_mode)),
            ContextMenuAction::ExtractHere => format!(" {} Extract Here", Icon::Archive.get(app.icon_mode)),
            ContextMenuAction::NewFolder => format!(" {} New Folder", Icon::Folder.get(app.icon_mode)),
            ContextMenuAction::NewFile => format!(" {} New File", Icon::File.get(app.icon_mode)),
            ContextMenuAction::Cut => " 󰆐 Cut".to_string(), // Keep some standard ones or update all
            ContextMenuAction::Copy => " 󰆏 Copy".to_string(),
            ContextMenuAction::CopyPath => " 󰆏 Copy Path".to_string(),
            ContextMenuAction::CopyName => " 󰆏 Copy Name".to_string(),
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
        crate::app::ContextMenuTarget::Process(_) => " Process ",
    };
    
    let menu_width = 25;
    let menu_height = items.len() as u16 + 2;
    let mut draw_x = x;
    let mut draw_y = y;
    if draw_x + menu_width > f.area().width { draw_x = f.area().width.saturating_sub(menu_width); }
    if draw_y + menu_height > f.area().height { draw_y = f.area().height.saturating_sub(menu_height); }

    let area = Rect::new(draw_x, draw_y, menu_width, menu_height);

    f.render_widget(Clear, area);
    let menu_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_secondary));
    
    // Add 1 cell of padding on the left by using a nested layout or margin
    let inner_area = menu_block.inner(area);
    let padded_area = Rect::new(inner_area.x + 1, inner_area.y, inner_area.width.saturating_sub(1), inner_area.height);
    
    f.render_widget(menu_block, area);
    f.render_widget(List::new(items), padded_area);
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
    let area = centered_rect(40, 10, f.area()); 
    f.render_widget(Clear, area);
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
    let area = centered_rect(40, 10, f.area()); 
    f.render_widget(Clear, area);
    let block = Block::default().title(" New Folder ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);
}

fn draw_new_file_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); 
    f.render_widget(Clear, area);
    let block = Block::default().title(" New File ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);
}

fn draw_delete_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(40, 10, f.area()); 
    f.render_widget(Clear, area);
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
    let area = centered_rect(80, 80, f.area()); 
    f.render_widget(Clear, area);
    let block = Block::default().title(" Settings ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area); f.render_widget(block, area);
    let chunks = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Length(15), Constraint::Min(0)]).split(inner);
    let sections = vec![ListItem::new(" 󰟜 Columns "), ListItem::new(" 󰓩 Tabs "), ListItem::new(" 󰒓 General "), ListItem::new(" 󰒍 Remotes "), ListItem::new(" 󰌌 Shortcuts ")];
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

fn draw_shortcuts_settings(f: &mut Frame, area: Rect, app: &App) {
    let shortcuts = vec![
        ("General", vec![
            ("Ctrl + q", "Quit Application"),
            ("Ctrl + g", "Open Settings"),
            ("Ctrl + Space", "Open Command Palette"),
            ("Ctrl + b", "Toggle Sidebar"),
            ("Ctrl + i", "AI Introspect (State Dump)"),
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
            ("Ctrl + p", "Toggle Split View"),
            ("Ctrl + t", "New Duplicate Tab"),
            ("Ctrl + h", "Toggle Hidden Files"),
            ("Ctrl + b", "Toggle Sidebar"),
            ("Ctrl + l / u", "Clear Search Filter"),
            ("Ctrl + z / y", "Undo / Redo (Rename/Move)"),
            ("F1", "Show this Help"),
        ]),
        ("File Operations", vec![
            ("Ctrl + s", "Save Modifications"),
            ("Ctrl + c", "Copy Selected"),
            ("Ctrl + x", "Cut Selected"),
            ("Ctrl + v", "Paste Selected"),
            ("Ctrl + a", "Select All"),
            ("F6", "Rename Selected"),
            ("Delete", "Delete Selected"),
            ("Alt + Enter", "Show Properties"),
        ]),
        ("Terminal", vec![
            ("Ctrl + n", "Open External Terminal"),
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

    let total_rows = rows.len();
    let visible_rows = area.height as usize;
    let scroll = app.settings_scroll.min(total_rows.saturating_sub(visible_rows));
    
    let table = Table::new(rows.into_iter().skip(scroll).collect::<Vec<_>>(), [Constraint::Length(20), Constraint::Min(0)])
        .block(Block::default().title(" Keyboard Shortcuts ").borders(Borders::NONE));
    
    f.render_widget(table, area);

    if total_rows > visible_rows {
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some("│"))
            .thumb_symbol("█");
        let mut scrollbar_state = ScrollbarState::new(total_rows)
            .position(scroll)
            .viewport_content_length(visible_rows);
        f.render_stateful_widget(scrollbar, area, &mut scrollbar_state);
    }
}

fn draw_column_settings(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);
    let titles = vec![" [Single] ", " [Split] "];
    let sel = match app.settings_target { SettingsTarget::SingleMode => 0, SettingsTarget::SplitMode => 1 };
    f.render_widget(Tabs::new(titles).block(Block::default().borders(Borders::BOTTOM).title(" Configure Mode ")).select(sel).highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), chunks[0]);
    let options = vec![
        (FileColumn::Extension, "Extension (e)"),
        (FileColumn::Size, "Size (s)"), 
        (FileColumn::Modified, "Modified (m)"), 
        (FileColumn::Created, "Created (c)"),
        (FileColumn::Permissions, "Permissions (p)")
    ];
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
        ListItem::new(format!("[{}] Smart Date Format (t)", if app.smart_date { "x" } else { " " })),
        ListItem::new(format!("[{}] Auto Save Modifications (a)", if app.auto_save { "x" } else { " " })),
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

fn draw_add_remote_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, f.area()); 
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Add Remote Server ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Name
            Constraint::Length(3), // Host
            Constraint::Length(3), // User
            Constraint::Length(3), // Port
            Constraint::Length(3), // Key Path
            Constraint::Min(0),    // Help
        ])
        .split(inner);

    let active_idx = if let AppMode::AddRemote(idx) = app.mode { idx } else { 0 };

    let fields = [
        ("Name", &app.pending_remote.name),
        ("Host", &app.pending_remote.host),
        ("User", &app.pending_remote.user),
        ("Port", &app.pending_remote.port.to_string()),
        ("Key Path", &app.pending_remote.key_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()),
    ];

    for (i, (label, value)) in fields.iter().enumerate() {
        let is_active = i == active_idx;
        let mut style = Style::default().fg(Color::DarkGray);
        if is_active { style = Style::default().fg(Color::Yellow); }

        let block = Block::default().borders(Borders::ALL).title(format!(" {} ", label)).border_style(style);
        let field_area = chunks[i];
        
        if is_active {
            f.render_widget(Paragraph::new(app.input.value.as_str()).block(block), field_area);
        } else {
            f.render_widget(Paragraph::new(value.as_str()).block(block), field_area);
        }
    }

    let help_text = vec![
        Line::from(vec![
            Span::styled(" [Tab/Enter] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Next Field  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("Cancel"),
        ]),
        Line::from("On the last field, [Enter] will save the bookmark."),
    ];
    f.render_widget(Paragraph::new(help_text), chunks[5]);
}

fn draw_highlight_modal(f: &mut Frame, _app: &App) {
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

fn draw_editor_overlay(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Clear, area);
    
    if let Some(preview) = &app.editor_state {
        let mut spans = vec![Span::styled(" Engage Overlay: ", Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)), Span::raw(format!(" {} ", preview.path.display()))];
        if let Some(ed) = &preview.editor {
            if ed.modified { spans.push(Span::styled("[Modified] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))); }
            spans.push(Span::styled("(Ctrl+S to Save, Esc to Close) ", Style::default().fg(Color::Cyan)));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(Line::from(spans))
            .border_style(Style::default().fg(THEME.accent_primary));
        
        let inner = block.inner(area);
        f.render_widget(block, area);

        if let Some(editor) = &preview.editor {
            f.render_widget(editor, inner);
        } else {
            let highlighted = highlight_code(&preview.content);
            let text = Paragraph::new(highlighted)
                .scroll((preview.scroll as u16, 0));
            f.render_widget(text, inner);
        }
    }
}