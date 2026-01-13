use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState, Scrollbar, ScrollbarOrientation, ScrollbarState, Sparkline, Gauge},
    Frame,
};
use std::collections::HashMap;

use crate::app::{App, AppMode, CurrentView, MonitorSubview, FileColumn, ProcessColumn, SidebarTarget, SidebarBounds, DropTarget, SettingsSection, SettingsTarget, FileCategory, ContextMenuAction, ContextMenuTarget};
use crate::ui::theme::THEME;
use crate::icons::Icon;
use terma::layout::centered_rect;
use terma::utils::{format_size, format_time, format_permissions, format_datetime_smart, highlight_code, draw_stat_bar};

pub mod theme;
pub mod layout;

pub fn get_context_menu_actions(_target: &ContextMenuTarget, _app: &App) -> Vec<ContextMenuAction> {
    vec![]
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let inner = area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 1 });
    match app.current_view {
        CurrentView::Files => {
            let mut sidebar_items = Vec::new();
            app.sidebar_bounds.clear();
            let mut current_y = inner.y;

            let mut active_storage_markers: HashMap<String, Vec<usize>> = HashMap::new();
            
            for (p_idx, pane) in app.panes.iter().enumerate() {
                let panel_num = p_idx + 1;
                if let Some(fs) = pane.current_state() {
                    let mut matched_disk = None;
                    let mut longest_prefix = 0;
                    for disk in &app.system_state.disks {
                        if disk.is_mounted && fs.current_path.starts_with(&disk.name) {
                            if disk.name.len() > longest_prefix {
                                longest_prefix = disk.name.len();
                                matched_disk = Some(disk.name.clone());
                            }
                        }
                    }
                    if let Some(name) = matched_disk { active_storage_markers.entry(name).or_default().push(panel_num); }
                }
            }

            let icon = Icon::Star.get(app.icon_mode);
            sidebar_items.push(ListItem::new(format!("{}FAVORITES", icon)).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)));
            app.sidebar_bounds.push(SidebarBounds { y: current_y, index: 0, target: SidebarTarget::Header("FAVORITES".to_string()) });
            current_y += 1;

            for path in &app.starred {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("?".to_string());
                let current_idx = sidebar_items.len();
                let is_focused = app.sidebar_focus && app.sidebar_index == current_idx;
                let is_active = if let Some(fs) = app.current_file_state() { fs.current_path == *path } else { false };
                let mut style = if is_active { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(THEME.fg) };
                if is_focused { style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD); }
                let icon = if path.is_dir() { Icon::Folder.get(app.icon_mode) } else { Icon::File.get(app.icon_mode) };
                sidebar_items.push(ListItem::new(format!("{}{}", icon, name)).style(style));
                app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_idx, target: SidebarTarget::Favorite(path.clone()) });
                current_y += 1;
            }

            sidebar_items.push(ListItem::new("")); current_y += 1;
            sidebar_items.push(ListItem::new(format!("{}STORAGES", Icon::Storage.get(app.icon_mode))).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)));
            current_y += 1;
            
            for (i, disk) in app.system_state.disks.iter().enumerate() {
                let current_idx = sidebar_items.len();
                let markers = active_storage_markers.get(&disk.name);
                let mut style = if markers.is_some() { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Green) };
                if app.sidebar_focus && app.sidebar_index == current_idx { style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD); }
                sidebar_items.push(ListItem::new(format!(" 󰋊 {}: {:.0}G Free", disk.name, disk.available_space / 1_073_741_824.0)).style(style));
                app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_idx, target: SidebarTarget::Storage(i) });
                current_y += 1;
            }

            let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(if app.sidebar_focus { Style::default().fg(THEME.border_active) } else { Style::default().fg(THEME.border_inactive) });
            f.render_widget(List::new(sidebar_items).block(block), area);
        }
        _ => {}
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    f.render_widget(Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0))), f.area());
    if app.current_view == CurrentView::Processes {
        draw_monitor_page(f, f.area(), app);
    } else {
        let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).split(f.area());
        let workspace = Layout::default().direction(Direction::Horizontal).constraints(if app.show_sidebar { [Constraint::Percentage(app.sidebar_width_percent), Constraint::Min(0)] } else { [Constraint::Percentage(0), Constraint::Min(0)] }).split(chunks[1]);
        draw_global_header(f, chunks[0], workspace[0].width, app);
        if app.show_sidebar { draw_sidebar(f, workspace[0], app); }
        draw_main_stage(f, workspace[1], app);
        draw_footer(f, chunks[2], app);
    }

    if matches!(app.mode, AppMode::Rename) { draw_rename_modal(f, app); }
    if matches!(app.mode, AppMode::Delete) { draw_delete_modal(f, app); }
    if matches!(app.mode, AppMode::NewFolder) { draw_new_folder_modal(f, app); }
    if matches!(app.mode, AppMode::NewFile) { draw_new_file_modal(f, app); }
}

fn draw_monitor_page(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);
    let nav_block = Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::Rgb(40, 40, 45)));
    let nav_inner = nav_block.inner(chunks[0]);
    f.render_widget(nav_block, chunks[0]);

    let nav_layout = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Min(60), Constraint::Length(35)]).split(nav_inner);
    let subviews = [(MonitorSubview::Overview, " 󰊚 ", "OVERVIEW"), (MonitorSubview::Applications, " 󰀻 ", "APPLICATIONS"), (MonitorSubview::Processes, " 󰑮 ", "PROCESSES")];

    app.monitor_subview_bounds.clear();
    let mut cur_x = nav_layout[0].x + 1;
    for (view, icon, name) in subviews {
        let is_active = app.monitor_subview == view;
        let text = format!("{}{}", icon, name);
        let width = text.len() as u16 + 2;
        let rect = Rect::new(cur_x, nav_layout[0].y, width, 1);
        let mut style = if is_active { Style::default().fg(Color::Rgb(61, 174, 233)).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Rgb(140, 145, 150)) };
        if app.mouse_pos.1 == nav_layout[0].y && app.mouse_pos.0 >= rect.x && app.mouse_pos.0 < rect.x + rect.width { style = style.fg(Color::White).bg(Color::Rgb(45, 50, 60)); }
        f.render_widget(Paragraph::new(text).style(style), rect);
        if is_active { f.render_widget(Paragraph::new("▔".repeat(width as usize)).style(Style::default().fg(Color::Rgb(61, 174, 233))), Rect::new(rect.x, rect.y + 1, rect.width, 1)); }
        app.monitor_subview_bounds.push((rect, view));
        cur_x += width + 4;
    }

    let search_text = if app.process_search_filter.is_empty() { Line::from(vec![Span::styled(" 󰍉 Search... ", Style::default().fg(Color::DarkGray))]) } else { Line::from(vec![Span::styled(" 󰍉 ", Style::default().fg(Color::Rgb(61, 174, 233))), Span::styled(&app.process_search_filter, Style::default().fg(Color::White).add_modifier(Modifier::BOLD))]) };
    f.render_widget(Paragraph::new(search_text).block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Rgb(60, 60, 65)))), nav_layout[1]);

    let content_area = chunks[1].inner(ratatui::layout::Margin { horizontal: 1, vertical: 1 });
    match app.monitor_subview {
        MonitorSubview::Overview => draw_monitor_overview(f, content_area, app),
        MonitorSubview::Processes => draw_processes_view(f, content_area, app),
        MonitorSubview::Applications => draw_monitor_applications(f, content_area, app),
    }
}

fn draw_monitor_overview(f: &mut Frame, area: Rect, app: &mut App) {
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(area);

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Heartbeat Pulse
            Constraint::Min(0),     // Processing Fabric
        ])
        .split(main_layout[0]);

    // --- 1. HEARTBEAT PULSE (Top Row) ---
    let metrics_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(33), Constraint::Percentage(33), Constraint::Percentage(34)])
        .split(left_chunks[0]);

    let draw_heartbeat = |f: &mut Frame, area: Rect, label: &str, cur: f32, total: f32, unit: &str, color: Color, history: &[u64]| {
        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(Style::default().fg(color).add_modifier(Modifier::DIM));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Label
                Constraint::Length(2), // Value
                Constraint::Min(0),    // Sparkline
            ])
            .split(inner);

        // Label with Icon
        let icon = match label {
            "CPU" => "󰍛 ",
            "MEM" => "󰘚 ",
            "SWP" => "󰓡 ",
            _ => "󰋊 ",
        };
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(icon, Style::default().fg(color)),
            Span::styled(label, Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)),
        ])), chunks[0]);

        // Dynamic Value
        let val_text = if total > 0.0 {
            format!("{:.1} / {:.1}", cur, total)
        } else {
            format!("{:.1}", cur)
        };
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(val_text, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}", unit), Style::default().fg(color).add_modifier(Modifier::DIM)),
        ])), chunks[1]);

        // Sparkline
        if !history.is_empty() {
            f.render_widget(Sparkline::default().data(history).style(Style::default().fg(color)), chunks[2]);
        }
    };

    draw_heartbeat(f, metrics_layout[0], "CPU", app.system_state.cpu_usage, 0.0, "%", Color::Rgb(0, 255, 150), &app.system_state.cpu_history);
    draw_heartbeat(f, metrics_layout[1], "MEM", app.system_state.mem_usage as f32, app.system_state.total_mem as f32, "GB", Color::Rgb(0, 180, 255), &app.system_state.mem_history);
    draw_heartbeat(f, metrics_layout[2], "SWP", app.system_state.swap_usage as f32, app.system_state.total_swap as f32, "GB", Color::Rgb(255, 100, 255), &app.system_state.swap_history);

    // --- 2. PROCESSING FABRIC (Core Grid Heatmap) ---
    let core_count = app.system_state.cpu_cores.len();
    if core_count > 0 {
        let fabric_block = Block::default()
            .title(vec![
                Span::raw("── "),
                Span::styled("󰓅 PROCESSING FABRIC ", Style::default().fg(Color::Rgb(100, 100, 110)).add_modifier(Modifier::BOLD)),
                Span::raw("─".repeat(area.width as usize)),
            ])
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::Rgb(40, 40, 45)));
        
        let fabric_inner = fabric_block.inner(left_chunks[1]);
        f.render_widget(fabric_block, left_chunks[1]);

        // Calculate grid (Fluid layout)
        let cols = if core_count > 32 { 16 } else if core_count > 16 { 8 } else if core_count > 8 { 4 } else { 2 };
        let rows = (core_count as f32 / cols as f32).ceil() as u16;
        
        let fabric_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(4); rows as usize])
            .split(fabric_inner);

        for r in 0..rows {
            if r as usize >= fabric_rows.len() { break; }
            let core_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Percentage(100 / cols); cols as usize])
                .split(fabric_rows[r as usize]);

            for c in 0..cols {
                let idx = (r * cols + c) as usize;
                if idx < core_count {
                    let usage = app.system_state.cpu_cores[idx];
                    let color = if usage > 90.0 { Color::Rgb(255, 50, 50) } 
                               else if usage > 70.0 { Color::Rgb(255, 150, 0) }
                               else if usage > 30.0 { Color::Rgb(0, 255, 150) }
                               else { Color::Rgb(40, 45, 55) };

                    let core_area = core_cols[c as usize].inner(ratatui::layout::Margin { horizontal: 1, vertical: 0 });
                    
                    // Core HUD Mini-widget
                    let label = format!("{:>2}", idx);
                    f.render_widget(Paragraph::new(Span::styled(label, Style::default().fg(Color::DarkGray))), Rect::new(core_area.x, core_area.y, 2, 1));
                    
                    let gauge_area = Rect::new(core_area.x + 3, core_area.y, core_area.width.saturating_sub(3), 1);
                    let filled = (usage / 100.0 * gauge_area.width as f32) as u16;
                    let bar = format!("{}{}", "█".repeat(filled as usize), "░".repeat(gauge_area.width.saturating_sub(filled) as usize));
                    f.render_widget(Paragraph::new(Span::styled(bar, Style::default().fg(color))), gauge_area);
                    
                    // Mini Sparkline under gauge
                    if idx < app.system_state.core_history.len() {
                        let spark_area = Rect::new(core_area.x + 3, core_area.y + 1, gauge_area.width, 2);
                        f.render_widget(Sparkline::default().data(&app.system_state.core_history[idx]).style(Style::default().fg(color).add_modifier(Modifier::DIM)), spark_area);
                    }
                }
            }
        }
    }

    // --- 3. SYSTEM PULSE SIDEBAR ---
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // OS Info
            Constraint::Length(12), // Network HUD
            Constraint::Min(0),     // Storage HUD
        ])
        .split(main_layout[1]);

    // OS Info Card
    let os_info = vec![
        Line::from(vec![Span::styled("󰣇 ", Style::default().fg(Color::Rgb(61, 174, 233))), Span::styled(&app.system_state.hostname, Style::default().add_modifier(Modifier::BOLD))]),
        Line::from(vec![Span::styled("󰔠 ", Style::default().fg(Color::Rgb(255, 200, 0))), Span::raw(format!("{}d {}h", app.system_state.uptime / 86400, (app.system_state.uptime % 86400) / 3600))]),
        Line::from(vec![Span::styled("󰌢 ", Style::default().fg(Color::DarkGray)), Span::raw(&app.system_state.kernel_version)]).style(Style::default().fg(Color::DarkGray)),
        Line::from(vec![Span::styled("󰏗 ", Style::default().fg(Color::DarkGray)), Span::raw(&app.system_state.os_name)]),
    ];
    f.render_widget(Paragraph::new(os_info).block(Block::default().borders(Borders::LEFT).border_style(Style::default().fg(Color::Rgb(40, 40, 45)))), right_chunks[0]);

    // Network Flow HUD
    let net_block = Block::default()
        .title(Span::styled(" 󰛳 NETWORK FLOW ", Style::default().fg(Color::Rgb(100, 100, 110)).add_modifier(Modifier::BOLD)))
        .borders(Borders::LEFT | Borders::TOP)
        .border_style(Style::default().fg(Color::Rgb(40, 40, 45)));
    let net_inner = net_block.inner(right_chunks[1]);
    f.render_widget(net_block, right_chunks[1]);

    let net_sub = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(net_inner);
    
    // Download
    let in_text = format!(" ↓ {:>8}/s ", format_size(app.system_state.net_in_history.last().cloned().unwrap_or(0)));
    f.render_widget(Sparkline::default().block(Block::default().title(Span::styled(in_text, Style::default().fg(Color::Rgb(0, 255, 150))))).data(&app.system_state.net_in_history).style(Style::default().fg(Color::Rgb(0, 255, 150))), net_sub[0]);
    
    // Upload
    let out_text = format!(" ↑ {:>8}/s ", format_size(app.system_state.net_out_history.last().cloned().unwrap_or(0)));
    f.render_widget(Sparkline::default().block(Block::default().title(Span::styled(out_text, Style::default().fg(Color::Rgb(0, 180, 255))))).data(&app.system_state.net_out_history).style(Style::default().fg(Color::Rgb(0, 180, 255))), net_sub[1]);

    // Storage Stack HUD
    let storage_block = Block::default()
        .title(Span::styled(" 󰋊 STORAGE HUD ", Style::default().fg(Color::Rgb(100, 100, 110)).add_modifier(Modifier::BOLD)))
        .borders(Borders::LEFT | Borders::TOP)
        .border_style(Style::default().fg(Color::Rgb(40, 40, 45)));
    let storage_inner = storage_block.inner(right_chunks[2]);
    f.render_widget(storage_block, right_chunks[2]);

    let disk_list: Vec<ListItem> = app.system_state.disks.iter().map(|disk| {
        let ratio = (disk.used_space / disk.total_space).clamp(0.0, 1.0);
        let color = if ratio > 0.9 { Color::Rgb(255, 50, 50) } else if ratio > 0.7 { Color::Rgb(255, 150, 0) } else { Color::Rgb(0, 255, 150) };
        
        let filled = (ratio * 12.0) as usize;
        let bar = format!("{}{}", "█".repeat(filled), " ".repeat(12 - filled));
        
        ListItem::new(vec![
            Line::from(vec![
                Span::styled("󰋊 ", Style::default().fg(color)),
                Span::styled(&disk.name, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(format!(" [{}] ", bar), Style::default().fg(Color::Rgb(40, 45, 55))),
                Span::styled(format!("{:.0}%", ratio * 100.0), Style::default().fg(Color::White)),
            ]),
            Line::from(Span::styled(format!("  {} / {}", format_size(disk.used_space as u64), format_size(disk.total_space as u64)), Style::default().fg(Color::DarkGray))),
            Line::from(""),
        ])
    }).collect();
    f.render_widget(List::new(disk_list), storage_inner);
}

fn draw_monitor_applications(f: &mut Frame, area: Rect, app: &mut App) {
    let current_user = std::env::var("USER").unwrap_or_else(|_| "dracon".to_string());
    let app_procs: Vec<_> = app.system_state.processes.iter().filter(|p| {
        let matches = if app.process_search_filter.is_empty() { true } else { p.name.to_lowercase().contains(&app.process_search_filter.to_lowercase()) };
        p.user == current_user && !p.name.starts_with('[') && !p.name.contains("kworker") && matches
    }).collect();

    let block = Block::default()
        .title(Span::styled(" ACTIVE APPLICATIONS ", Style::default().fg(Color::Rgb(61, 174, 233)).add_modifier(Modifier::BOLD)))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    let inner = block.inner(area); f.render_widget(block, area);

    let rows = app_procs.iter().enumerate().map(|(i, p)| {
        let name_lower = p.name.to_lowercase();
        let (icon, color) = if name_lower.contains("chrome") || name_lower.contains("firefox") { ("󰈹 ", Color::Rgb(231, 76, 60)) } 
                           else if name_lower.contains("code") || name_lower.contains("vim") { ("󰨞 ", Color::Rgb(61, 174, 233)) } 
                           else if name_lower.contains("tiles") { ("󰀻 ", Color::Rgb(0, 255, 150)) }
                           else { ("󰀻 ", Color::White) };
        
        let mut style = if i % 2 == 0 { Style::default().bg(Color::Rgb(15, 17, 20)) } else { Style::default() };
        let mut is_selected = false;
        if app.process_selected_idx == Some(i) && app.monitor_subview == MonitorSubview::Applications { 
            style = style.bg(Color::Rgb(61, 174, 233)).fg(Color::Black).add_modifier(Modifier::BOLD); 
            is_selected = true;
        }
        
        let cpu_color = if p.cpu > 50.0 { Color::Red } else if p.cpu > 10.0 { Color::Yellow } else { Color::Rgb(0, 255, 150) };
        
        Row::new(vec![
            Cell::from(format!("{} {}", icon, p.name)).style(Style::default().fg(if is_selected { Color::Black } else { color }).add_modifier(Modifier::BOLD)),
            Cell::from(format!("{:.1}%", p.cpu)).style(Style::default().fg(if is_selected { Color::Black } else { cpu_color })),
            Cell::from(format!("{:.1} MB", p.mem)),
            Cell::from(p.pid.to_string()).style(Style::default().fg(if is_selected { Color::Black } else { Color::DarkGray })),
            Cell::from(p.status.clone()),
        ]).style(style)
    });

    f.render_widget(Table::new(rows, [
        Constraint::Min(35),
        Constraint::Length(10),
        Constraint::Length(15),
        Constraint::Length(10),
        Constraint::Length(15),
    ]).header(Row::new(vec![" Application", "CPU", "Memory", "PID", "Status"])
        .style(Style::default().fg(Color::Rgb(140, 145, 150)).add_modifier(Modifier::BOLD))
        .height(1)
        .bottom_margin(1))
    .column_spacing(2), inner);
}

fn draw_processes_view(f: &mut Frame, area: Rect, app: &mut App) {
    let table_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 55)));
    let table_inner = table_block.inner(area); f.render_widget(table_block, area);

    let column_constraints = [
        Constraint::Length(8),  // PID
        Constraint::Min(25),    // Name
        Constraint::Length(15), // User
        Constraint::Length(12), // Status
        Constraint::Length(10), // CPU%
        Constraint::Length(10), // Mem%
    ];

    app.process_column_bounds.clear();
    let header_rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(column_constraints)
        .split(Rect::new(table_inner.x, table_inner.y, table_inner.width, 1));

    let header_cells = ["PID", "NAME", "USER", "STATUS", "CPU%", "MEM%"].iter().enumerate().map(|(i, h)| {
        let col = match *h { 
            "PID" => ProcessColumn::Pid, "NAME" => ProcessColumn::Name, 
            "USER" => ProcessColumn::User, "STATUS" => ProcessColumn::Status, 
            "CPU%" => ProcessColumn::Cpu, "MEM%" => ProcessColumn::Mem, 
            _ => ProcessColumn::Pid 
        };
        app.process_column_bounds.push((header_rects[i], col));
        
        let mut text = h.to_string();
        if app.process_sort_col == col {
            text.push_str(if app.process_sort_asc { " 󰁝" } else { " 󰁅" });
        }
        Cell::from(text).style(Style::default()
            .fg(if app.process_sort_col == col { Color::Rgb(61, 174, 233) } else { Color::Rgb(100, 100, 110) })
            .add_modifier(Modifier::BOLD))
    });

    let rows = app.system_state.processes.iter().enumerate().map(|(i, p)| {
        let mut style = if i % 2 == 0 { Style::default().bg(Color::Rgb(15, 17, 20)) } else { Style::default() };
        if app.process_selected_idx == Some(i) && app.monitor_subview == MonitorSubview::Processes { 
            style = style.bg(Color::Rgb(61, 174, 233)).fg(Color::Black).add_modifier(Modifier::BOLD); 
        }
        
        let cpu_color = if p.cpu > 50.0 { Color::Red } else if p.cpu > 10.0 { Color::Yellow } else { Color::Rgb(0, 255, 150) };

        Row::new(vec![
            Cell::from(p.pid.to_string()).style(Style::default().fg(Color::DarkGray)),
            Cell::from(p.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from(p.user.clone()).style(Style::default().fg(Color::Rgb(61, 174, 233))),
            Cell::from(p.status.clone()),
            Cell::from(format!("{:.1}", p.cpu)).style(Style::default().fg(if app.process_selected_idx == Some(i) { Color::Black } else { cpu_color })),
            Cell::from(format!("{:.1}", p.mem)),
        ]).style(style)
    });

    f.render_stateful_widget(Table::new(rows, column_constraints)
        .header(Row::new(header_cells)
            .height(1)
            .bottom_margin(1)
            .style(Style::default().bg(Color::Rgb(30, 33, 35))))
        .column_spacing(1), table_inner, &mut app.process_table_state);
}

fn draw_global_header(f: &mut Frame, area: Rect, sidebar_width: u16, app: &mut App) {
    let icons = vec![(Icon::Burger.get(app.icon_mode), "burger", "Settings"), (Icon::Back.get(app.icon_mode), "back", "Back"), (Icon::Forward.get(app.icon_mode), "forward", "Forward"), (Icon::Split.get(app.icon_mode), "split", "Split View"), (Icon::Monitor.get(app.icon_mode), "monitor", "System Monitor")];
    let mut cur_icon_x = area.x + 1;
    app.header_icon_bounds.clear();
    let mut hovered_tip = None;
    for (i, (icon, id, desc)) in icons.iter().enumerate() {
        let rect = Rect::new(cur_icon_x, area.y, 3, 1);
        let is_hovered = app.mouse_pos.1 == area.y && app.mouse_pos.0 >= rect.x && app.mouse_pos.0 < rect.x + rect.width;
        let is_kb_focused = matches!(app.mode, AppMode::Header(idx) if idx == i);
        let mut style = Style::default().fg(THEME.accent_secondary);
        if is_kb_focused || is_hovered { style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD); if is_hovered { app.hovered_header_icon = Some(id.to_string()); hovered_tip = Some(desc.to_string()); } }
        f.render_widget(Paragraph::new(format!(" {} ", icon)).style(style), rect);
        app.header_icon_bounds.push((rect, id.to_string())); cur_icon_x += 3;
    }
    if let Some(desc) = hovered_tip { f.render_widget(Paragraph::new(format!(" [ {} ] ", desc)).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Rect::new(cur_icon_x + 1, area.y, (desc.len() + 6) as u16, 1)); }
    let start_x = if app.show_sidebar { std::cmp::max(area.x + sidebar_width, cur_icon_x + 1) } else { cur_icon_x + 1 };
    let pane_count = app.panes.len();
    if pane_count > 0 {
        let pane_chunks = Layout::default().direction(Direction::Horizontal).constraints(vec![Constraint::Percentage(100 / pane_count as u16); pane_count]).split(Rect::new(start_x, area.y, area.width.saturating_sub(start_x), 1));
        app.tab_bounds.clear();
        let mut global_tab_idx = 4;
        for (p_i, pane) in app.panes.iter().enumerate() {
            let chunk = pane_chunks[p_i]; let mut current_x = chunk.x;
            for (t_i, tab) in pane.tabs.iter().enumerate() {
                let name = if !tab.search_filter.is_empty() { format!("Search: {}", tab.search_filter) } else { tab.current_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("/".to_string()) };
                let is_active = t_i == pane.active_tab_index;
                let mut style = if is_active { if p_i == app.focused_pane_index && !app.sidebar_focus { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(THEME.accent_primary) } } else { Style::default().fg(Color::DarkGray) };
                if let AppMode::Header(idx) = app.mode { if idx == global_tab_idx { style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD); } }
                let text = format!(" {} ", name); let width = text.len() as u16;
                if current_x + width > chunk.x + chunk.width { break; }
                let rect = Rect::new(current_x, area.y, width, 1);
                f.render_widget(Paragraph::new(text).style(style), rect);
                app.tab_bounds.push((rect, p_i, t_i)); current_x += width + 1; global_tab_idx += 1;
            }
        }
    }
}

fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
    if app.current_view == CurrentView::Files {
        let pane_count = app.panes.len();
        if pane_count > 0 {
            let chunks = Layout::default().direction(Direction::Horizontal).constraints(vec![Constraint::Percentage(100 / pane_count as u16); pane_count]).split(area);
            for i in 0..pane_count { let is_focused = i == app.focused_pane_index && !app.sidebar_focus; draw_file_view(f, chunks[i], app, i, is_focused, Borders::ALL); }
        }
    }
}

fn draw_file_view(f: &mut Frame, area: Rect, app: &mut App, pane_idx: usize, is_focused: bool, borders: Borders) {
    if let Some(file_state) = app.panes.get_mut(pane_idx).and_then(|p| p.current_state_mut()) {
        file_state.view_height = area.height as usize;
        let mut render_state = TableState::default();
        if let Some(sel) = file_state.selected_index { let offset = file_state.table_state.offset(); if sel >= offset && sel < offset + area.height as usize - 3 { render_state.select(Some(sel)); } }
        *render_state.offset_mut() = file_state.table_state.offset();
        let constraints: Vec<Constraint> = file_state.columns.iter().map(|c| match c { FileColumn::Name => Constraint::Min(20), FileColumn::Size => Constraint::Length(9), _ => Constraint::Length(12) }).collect();
        let rows = file_state.files.iter().enumerate().map(|(i, path)| {
            let metadata = file_state.metadata.get(path);
            let mut style = if metadata.map(|m| m.is_dir).unwrap_or(false) { Style::default().fg(THEME.accent_secondary) } else { Style::default().fg(THEME.fg) };
            if file_state.multi_select.contains(&i) && is_focused { style = style.bg(Color::Rgb(100, 0, 0)).fg(Color::White); }
            let cells = vec![Cell::from(format!("{} {}", if metadata.map(|m| m.is_dir).unwrap_or(false) { Icon::Folder.get(app.icon_mode) } else { Icon::File.get(app.icon_mode) }, path.file_name().unwrap_or_default().to_string_lossy())), Cell::from(format_size(metadata.map(|m| m.size).unwrap_or(0)))];
            Row::new(cells).style(style)
        });
        let border_style = if is_focused { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } else { Style::default().fg(THEME.border_inactive) };
        f.render_stateful_widget(Table::new(rows, constraints).header(Row::new(vec!["Name"]).height(1)).block(Block::default().borders(borders).border_type(BorderType::Rounded).border_style(border_style)).row_highlight_style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD)), area, &mut render_state);
        *file_state.table_state.offset_mut() = render_state.offset();
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Min(0), Constraint::Length(30), Constraint::Percentage(30)]).split(area);
    f.render_widget(Paragraph::new(Line::from(vec![Span::styled(" ^Q ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw("Quit ")])), chunks[0]);
    f.render_widget(Paragraph::new(draw_stat_bar("CPU", app.system_state.cpu_usage, 100.0, chunks[2].width / 2, THEME.fg)).alignment(ratatui::layout::Alignment::Right), chunks[2]);
}

fn draw_rename_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(format!("{}", &app.input.value)).block(Block::default().borders(Borders::ALL).title(" Rename ")), area);
}

fn draw_delete_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new("Confirm Delete? (y/n)").block(Block::default().borders(Borders::ALL).title(" Delete ")), area);
}

fn draw_new_folder_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(format!("{}", &app.input.value)).block(Block::default().borders(Borders::ALL).title(" New Folder ")), area);
}

fn draw_new_file_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(format!("{}", &app.input.value)).block(Block::default().borders(Borders::ALL).title(" New File ")), area);
}
