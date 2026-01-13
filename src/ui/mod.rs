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

    // 1. High-Gloss Navigation Bar (Plasma/Breeze Style)
    let nav_block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    let nav_inner = nav_block.inner(chunks[0]);
    f.render_widget(nav_block, chunks[0]);

    let nav_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(60), Constraint::Length(35), Constraint::Length(12)])
        .split(nav_inner);

    // Navigation Tabs
    let subviews: [(MonitorSubview, &str, &str); 4] = [
        (MonitorSubview::Overview, " 󰊚 ", "OVERVIEW"),
        (MonitorSubview::Applications, " 󰀻 ", "APPLICATIONS"),
        (MonitorSubview::History, " 󰔠 ", "HISTORY"),
        (MonitorSubview::Processes, " 󰑮 ", "PROCESSES"),
    ];

    app.monitor_subview_bounds.clear();
    let mut cur_x = nav_layout[0].x + 1;
    
    for (view, icon, name) in subviews {
        let is_active = app.monitor_subview == view;
        let text = format!("{}{}", icon, name);
        let width = text.len() as u16 + 2;
        let rect = Rect::new(cur_x, nav_layout[0].y, width, 1);
        
        let mut style = if is_active {
            Style::default().fg(Color::Rgb(61, 174, 233)).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(140, 145, 150))
        };

        if app.mouse_pos.1 == nav_layout[0].y && app.mouse_pos.0 >= rect.x && app.mouse_pos.0 < rect.x + rect.width {
            style = style.fg(Color::White).bg(Color::Rgb(45, 50, 60));
        }

        f.render_widget(Paragraph::new(text).style(style), rect);
        if is_active {
            f.render_widget(Paragraph::new("▔".repeat(width as usize)).style(Style::default().fg(Color::Rgb(61, 174, 233))), Rect::new(rect.x, rect.y + 1, rect.width, 1));
        }

        app.monitor_subview_bounds.push((rect, view));
        cur_x += width + 3;
    }

    // Centered Search Box
    let search_text = if app.process_search_filter.is_empty() {
        Line::from(vec![Span::styled(" 󰍉 Search... ", Style::default().fg(Color::DarkGray))])
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
                .border_style(Style::default().fg(Color::Rgb(70, 75, 80)))),
        nav_layout[1]
    );

    // Close Region
    let exit_text = " 󰅖 CLOSE ";
    let mut exit_style = Style::default().fg(Color::Rgb(231, 76, 60)).add_modifier(Modifier::BOLD);
    if app.mouse_pos.1 == nav_layout[2].y && app.mouse_pos.0 >= nav_layout[2].x && app.mouse_pos.0 < nav_layout[2].x + nav_layout[2].width {
        exit_style = exit_style.bg(Color::Rgb(80, 20, 20)).fg(Color::White);
    }
    f.render_widget(Paragraph::new(exit_text).style(exit_style).alignment(ratatui::layout::Alignment::Right), nav_layout[2]);

    let content_area = chunks[1].inner(ratatui::layout::Margin { horizontal: 1, vertical: 1 });
    match app.monitor_subview {
        MonitorSubview::Overview => draw_monitor_overview(f, content_area, app),
        MonitorSubview::Processes => draw_processes_view(f, content_area, app),
        MonitorSubview::History => draw_monitor_history(f, content_area, app),
        MonitorSubview::Applications => draw_monitor_applications(f, content_area, app),
    }
}

fn draw_monitor_overview(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // System Banner
            Constraint::Percentage(45), // Big Gauges
            Constraint::Min(0)      // Disks
        ])
        .split(area);
    
    // 1. System Banner Card
    let info_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(60, 60, 65)));
    let info_inner = info_block.inner(chunks[0]);
    f.render_widget(info_block, chunks[0]);

    let uptime_days = app.system_state.uptime / 86400;
    let uptime_hours = (app.system_state.uptime % 86400) / 3600;
    let uptime_mins = (app.system_state.uptime % 3600) / 60;

    let banner_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(30), Constraint::Percentage(30)])
        .split(info_inner);

    f.render_widget(Paragraph::new(vec![
        Line::from(vec![Span::styled("󰣇 ", Style::default().fg(Color::Rgb(61, 174, 233))), Span::styled(&app.system_state.hostname, Style::default().add_modifier(Modifier::BOLD))]),
        Line::from(vec![Span::raw(format!("{} {}", app.system_state.os_name, app.system_state.os_version))]).style(Style::default().fg(Color::DarkGray)),
    ]), banner_layout[0]);

    f.render_widget(Paragraph::new(vec![
        Line::from(vec![Span::styled("󰔠 ", Style::default().fg(Color::Yellow)), Span::raw("UPTIME")]),
        Line::from(vec![Span::styled(format!("{}d {}h {}m", uptime_days, uptime_hours, uptime_mins), Style::default().add_modifier(Modifier::BOLD))]),
    ]), banner_layout[1]);

    f.render_widget(Paragraph::new(vec![
        Line::from(vec![Span::styled("󰛳 ", Style::default().fg(Color::Green)), Span::raw("NETWORK")]),
        Line::from(vec![Span::styled(format!("↓{} ↑{}", format_size(app.system_state.net_in), format_size(app.system_state.net_out)), Style::default().add_modifier(Modifier::BOLD))]),
    ]), banner_layout[2]);

    // 2. Performance Dashboard
    let stats_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // CPU Card
    let cpu_block = Block::default()
        .title(Span::styled(" 󰍛 CPU LOAD ", Style::default().fg(Color::Rgb(46, 204, 113)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    let cpu_inner = cpu_block.inner(stats_layout[0]);
    f.render_widget(cpu_block, stats_layout[0]);

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
        .title(Span::styled(" 󰘚 MEMORY ", Style::default().fg(Color::Rgb(155, 89, 182)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    let mem_inner = mem_block.inner(stats_layout[1]);
    f.render_widget(mem_block, stats_layout[1]);

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

    // 3. Storage Tiles
    let storage_block = Block::default()
        .title(Span::styled(" 󰋊 STORAGE DEVICES ", Style::default().fg(Color::Rgb(241, 196, 15)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    let storage_inner = storage_block.inner(chunks[2]);
    f.render_widget(storage_block, chunks[2]);

    let disk_list: Vec<ListItem> = app.system_state.disks.iter().map(|disk| {
        let ratio = (disk.used_space / disk.total_space).clamp(0.0, 1.0);
        let color = if ratio > 0.9 { Color::Red } else if ratio > 0.7 { Color::Yellow } else { Color::Rgb(241, 196, 15) };
        
        let bar_width = 40;
        let filled = (ratio * bar_width as f64) as usize;
        let bar = format!("{}{}", "█".repeat(filled), "░".repeat(bar_width - filled));

        ListItem::new(vec![
            Line::from(vec![
                Span::styled(format!(" 󰋊 {} ", disk.name), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(bar, Style::default().fg(color)),
                Span::raw(format!(" {:.0}% used", ratio * 100.0)),
                Span::styled(format!("   ({:.1}GB FREE)", disk.available_space / 1024.0 / 1024.0 / 1024.0), Style::default().fg(Color::DarkGray)),
            ]),
        ])
    }).collect();

    f.render_widget(List::new(disk_list), storage_inner);
}

fn draw_monitor_history(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45), // Per-core CPU Grid
            Constraint::Percentage(25), // Network
            Constraint::Min(0)          // Memory
        ])
        .split(area);

    // 1. Per-Core CPU Activity Grid
    let core_count = app.system_state.cpu_cores.len();
    if core_count > 0 {
        let cols = if core_count > 16 { 8 } else if core_count > 8 { 4 } else { 2 };
        let rows = (core_count as f32 / cols as f32).ceil() as u16;
        
        let core_block = Block::default()
            .title(Span::styled(" 󰍛 CPU CORE ACTIVITY ", Style::default().fg(Color::Rgb(46, 204, 113)).add_modifier(Modifier::BOLD)))
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
        let core_inner = core_block.inner(chunks[0]);
        f.render_widget(core_block, chunks[0]);

        let row_constraints = vec![Constraint::Percentage(100 / rows); rows as usize];
        let core_rows = Layout::default().direction(Direction::Vertical).constraints(row_constraints).split(core_inner);

        for r in 0..rows {
            let col_constraints = vec![Constraint::Percentage(100 / cols); cols as usize];
            let core_cols = Layout::default().direction(Direction::Horizontal).constraints(col_constraints).split(core_rows[r as usize]);
            
            for c in 0..cols {
                let core_idx = (r * cols + c) as usize;
                if core_idx < core_count {
                    let usage = app.system_state.cpu_cores[core_idx];
                    let history = &app.system_state.core_history[core_idx];
                    let spark = Sparkline::default()
                        .block(Block::default().title(format!(" CORE {} ", core_idx)).title_alignment(ratatui::layout::Alignment::Left))
                        .data(history)
                        .style(Style::default().fg(if usage > 80.0 { Color::Red } else { Color::Rgb(46, 204, 113) }));
                    f.render_widget(spark, core_cols[c as usize]);
                }
            }
        }
    }

    // 2. Traffic Dashboard
    let net_block = Block::default()
        .title(Span::styled(" 󰛳 NETWORK TRAFFIC ", Style::default().fg(Color::Rgb(61, 174, 233)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    let net_inner = net_block.inner(chunks[1]);
    f.render_widget(net_block, chunks[1]);

    let net_layout = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(net_inner);
    
    let in_history: Vec<u64> = app.system_state.net_in_history.iter().copied().collect();
    let out_history: Vec<u64> = app.system_state.net_out_history.iter().copied().collect();

    f.render_widget(Sparkline::default().block(Block::default().title(" 󰁍 DOWNLOAD ")).data(&in_history).style(Style::default().fg(Color::Rgb(46, 204, 113))), net_layout[0]);
    f.render_widget(Sparkline::default().block(Block::default().title(" 󰁔 UPLOAD ")).data(&out_history).style(Style::default().fg(Color::Rgb(61, 174, 233))), net_layout[1]);

    // 3. Large Memory Chart
    let mem_history: Vec<u64> = app.system_state.mem_history.iter().copied().collect();
    let mem_block = Block::default()
        .title(Span::styled(" 󰘚 MEMORY USAGE HISTORY ", Style::default().fg(Color::Rgb(155, 89, 182)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    f.render_widget(Sparkline::default().block(mem_block).data(&mem_history).max(100).style(Style::default().fg(Color::Rgb(155, 89, 182))), chunks[2]);
}

fn draw_monitor_applications(f: &mut Frame, area: Rect, app: &mut App) {
    let current_user = std::env::var("USER").unwrap_or_else(|_| "dracon".to_string());
    
    let mut app_procs: Vec<_> = app.system_state.processes.iter()
        .filter(|p| {
            let matches_filter = if app.process_search_filter.is_empty() { true } else {
                p.name.to_lowercase().contains(&app.process_search_filter.to_lowercase())
            };
            let is_user = p.user == current_user;
            let is_app = !p.name.starts_with('[') && !p.name.contains("kworker");
            is_user && is_app && matches_filter
        })
        .collect();

    app_procs.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));

    let block = Block::default()
        .title(Span::styled(" ACTIVE APPLICATIONS ", Style::default().fg(Color::Rgb(61, 174, 233)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = app_procs.iter().enumerate().map(|(i, p)| {
        let name_lower = p.name.to_lowercase();
        let (icon, color) = if name_lower.contains("chrome") || name_lower.contains("firefox") || name_lower.contains("browser") { ("󰈹 ", Color::Rgb(231, 76, 60)) }
                            else if name_lower.contains("code") || name_lower.contains("vim") || name_lower.contains("emacs") { ("󰨞 ", Color::Rgb(61, 174, 233)) }
                            else if name_lower.contains("term") || name_lower.contains("fish") || name_lower.contains("bash") { ("󰆍 ", Color::Rgb(46, 204, 113)) }
                            else if name_lower.contains("discord") || name_lower.contains("slack") || name_lower.contains("telegram") { ("󰙯 ", Color::Rgb(155, 89, 182)) }
                            else if name_lower.contains("spotify") || name_lower.contains("vlc") { ("󰝚 ", Color::Rgb(26, 188, 156)) }
                            else { ("󰀻 ", Color::White) };

        let mut row_style = if i % 2 == 0 { Style::default().bg(Color::Rgb(25, 27, 30)) } else { Style::default() };
        if app.process_selected_idx == Some(i) && app.monitor_subview == MonitorSubview::Applications {
            row_style = row_style.bg(Color::Rgb(61, 174, 233)).fg(Color::Black);
        }

        Row::new(vec![
            Cell::from(format!("{} {}", icon, p.name)).style(Style::default().fg(if app.process_selected_idx == Some(i) { Color::Black } else { color }).add_modifier(Modifier::BOLD)),
            Cell::from(format!("{:.1}%", p.cpu)).style(Style::default().fg(if p.cpu > 30.0 { Color::Red } else { Color::Rgb(46, 204, 113) })),
            Cell::from(format!("{:.1} MB", p.mem)).style(Style::default().fg(Color::Rgb(61, 174, 233))),
            Cell::from(p.pid.to_string()).style(Style::default().fg(Color::DarkGray)),
            Cell::from(p.status.clone()).style(Style::default().fg(Color::DarkGray)),
        ]).style(row_style)
    });

    let table = Table::new(rows, [
        Constraint::Min(35),
        Constraint::Length(10),
        Constraint::Length(15),
        Constraint::Length(10),
        Constraint::Length(15),
    ])
    .header(Row::new(vec![" Application", "CPU", "Memory", "PID", "Status"]).style(Style::default().add_modifier(Modifier::BOLD)).height(1).bottom_margin(1))
    .column_spacing(2);

    f.render_widget(table, inner);
}

fn draw_processes_view(f: &mut Frame, area: Rect, app: &mut App) {
    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    
    let table_inner = table_block.inner(area);
    f.render_widget(table_block, area);

    let column_constraints = [
        Constraint::Length(8),
        Constraint::Min(25),
        Constraint::Length(15),
        Constraint::Length(15),
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

    let rows = app.system_state.processes.iter().enumerate().map(|(i, p)| {
        let mut row_style = if i % 2 == 0 { Style::default().bg(Color::Rgb(25, 27, 30)) } else { Style::default() };
        if app.process_selected_idx == Some(i) && app.monitor_subview == MonitorSubview::Processes {
            row_style = row_style.bg(Color::Rgb(61, 174, 233)).fg(Color::Black);
        }

        Row::new(vec![
            Cell::from(p.pid.to_string()).style(Style::default().fg(if app.process_selected_idx == Some(i) { Color::Black } else { Color::DarkGray })),
            Cell::from(p.name.clone()).style(Style::default().fg(if app.process_selected_idx == Some(i) { Color::Black } else { Color::White }).add_modifier(Modifier::BOLD)),
            Cell::from(p.user.clone()).style(Style::default().fg(if app.process_selected_idx == Some(i) { Color::Black } else { Color::Rgb(61, 174, 233) })),
            Cell::from(p.status.clone()).style(Style::default().fg(if app.process_selected_idx == Some(i) { Color::Black } else { Color::DarkGray })),
            Cell::from(format!("{:.1}", p.cpu)).style(Style::default().fg(if p.cpu > 50.0 { Color::Red } else { Color::Rgb(46, 204, 113) })),
            Cell::from(format!("{:.1}", p.mem)).style(Style::default().fg(Color::Rgb(155, 89, 182))),
        ]).style(row_style)
    });

    let table = Table::new(rows, column_constraints)
    .header(header)
    .column_spacing(1);

    f.render_stateful_widget(table, table_inner, &mut app.process_table_state);
}