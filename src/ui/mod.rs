use ratatui,{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState, Chart, Dataset, Axis, GraphType},
    symbols,
    Frame,
};

use crate::app::{App, CurrentView, MonitorSubview, ProcessColumn, SidebarTarget, SidebarBounds};
use crate::ui::theme::THEME;
use crate::icons::Icon;
use terma::utils::{format_size, draw_stat_bar};

pub mod theme;
pub mod layout;

fn draw_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let inner = area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 1 });
    match app.current_view {
        CurrentView::Files => {
            let mut sidebar_items = Vec::new();
            app.sidebar_bounds.clear();
            let mut current_y = inner.y;

            let icon = Icon::Star.get(app.icon_mode);
            sidebar_items.push(ListItem::new(format!("{} FAVORITES", icon)).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)));
            app.sidebar_bounds.push(SidebarBounds { y: current_y, index: 0, target: SidebarTarget::Header("FAVORITES".to_string()) });
            current_y += 1;

            for path in &app.starred {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("?".to_string());
                let current_idx = sidebar_items.len();
                let is_focused = app.sidebar_focus && app.sidebar_index == current_idx;
                let is_active = if let Some(fs) = app.current_file_state() { fs.current_path == *path } else { false };
                
                let mut style = if is_active { Style::default().fg(THEME.accent_primary) } else { Style::default().fg(THEME.fg) };
                if is_focused { style = style.fg(Color::Black).bg(THEME.accent_primary).add_modifier(Modifier::BOLD); }
                
                let icon = if path.is_dir() { Icon::Folder.get(app.icon_mode) } else { Icon::File.get(app.icon_mode) };
                sidebar_items.push(ListItem::new(format!(" {} {}", icon, name)).style(style));
                app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_idx, target: SidebarTarget::Favorite(path.clone()) });
                current_y += 1;
            }

            sidebar_items.push(ListItem::new("")); current_y += 1;
            sidebar_items.push(ListItem::new(format!("{} STORAGES", Icon::Storage.get(app.icon_mode))).style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)));
            current_y += 1;
            
            for (i, disk) in app.system_state.disks.iter().enumerate() {
                let current_idx = sidebar_items.len();
                let is_focused = app.sidebar_focus && app.sidebar_index == current_idx;
                let mut style = Style::default().fg(Color::Rgb(100, 100, 110));
                if is_focused { style = style.fg(Color::Black).bg(THEME.accent_primary).add_modifier(Modifier::BOLD); }
                
                sidebar_items.push(ListItem::new(format!(" 󰋊 {}: {:.0}G", disk.name, disk.available_space / 1_073_741_824.0)).style(style));
                app.sidebar_bounds.push(SidebarBounds { y: current_y, index: current_idx, target: SidebarTarget::Storage(i) });
                current_y += 1;
            }

            f.render_widget(List::new(sidebar_items), area);
            f.render_widget(Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(Color::Rgb(30, 30, 35))), area);
        }
        _ => {}
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    f.render_widget(Block::default().style(Style::default().bg(Color::Rgb(5, 5, 10))), f.area());
    if app.current_view == CurrentView::Processes {
        draw_monitor_page(f, f.area(), app);
    } else {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(0), Constraint::Length(1)])
            .split(f.area());
        
        let workspace = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(if app.show_sidebar { [Constraint::Percentage(app.sidebar_width_percent), Constraint::Min(0)] } else { [Constraint::Percentage(0), Constraint::Min(0)] })
            .split(chunks[1]);

        draw_global_header(f, chunks[0], workspace[0].width, app);
        if app.show_sidebar { draw_sidebar(f, workspace[0], app); }
        draw_main_stage(f, workspace[1], app);
        draw_footer(f, chunks[2], app);
    }

    if matches!(app.mode, crate::app::AppMode::Rename) { draw_rename_modal(f, app); }
    if matches!(app.mode, crate::app::AppMode::Delete) { draw_delete_modal(f, app); }
    if matches!(app.mode, crate::app::AppMode::NewFolder) { draw_new_folder_modal(f, app); }
    if matches!(app.mode, crate::app::AppMode::NewFile) { draw_new_file_modal(f, app); }
}

fn draw_monitor_page(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);
    
    let nav_area = chunks[0].inner(ratatui::layout::Margin { horizontal: 2, vertical: 1 });
    let nav_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(60), Constraint::Length(30)])
        .split(nav_area);

    let subviews = [
        (MonitorSubview::Overview, "󰊚 OVERVIEW"), 
        (MonitorSubview::Applications, "󰀻 APPLICATIONS"), 
        (MonitorSubview::Processes, "󰑮 PROCESSES")
    ];

    app.monitor_subview_bounds.clear();
    let mut cur_x = nav_layout[0].x;
    for (view, name) in subviews {
        let is_active = app.monitor_subview == view;
        let width = name.len() as u16 + 4;
        let rect = Rect::new(cur_x, nav_layout[0].y, width, 1);
        
        let mut style = if is_active { Style::default().fg(Color::Rgb(0, 180, 255)).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Rgb(60, 65, 75)) };
        if app.mouse_pos.1 == nav_layout[0].y && app.mouse_pos.0 >= rect.x && app.mouse_pos.0 < rect.x + rect.width {
            style = style.fg(Color::White);
        }
        
        f.render_widget(Paragraph::new(name).style(style), rect);
        if is_active {
            f.render_widget(Paragraph::new("━━━━").style(Style::default().fg(Color::Rgb(0, 180, 255))), Rect::new(rect.x, rect.y + 1, 4, 1));
        }
        
        app.monitor_subview_bounds.push((rect, view));
        cur_x += width + 2;
    }

    let search_style = if app.process_search_filter.is_empty() { Style::default().fg(Color::Rgb(40, 45, 55)) } else { Style::default().fg(Color::Rgb(0, 180, 255)) };
    f.render_widget(Paragraph::new(format!(" 󰍉 {}", app.process_search_filter)).style(search_style), nav_layout[1]);

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
        .constraints([Constraint::Percentage(72), Constraint::Percentage(28)])
        .split(area.inner(ratatui::layout::Margin { horizontal: 1, vertical: 1 }));

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // Pulse Spectrum
            Constraint::Min(0),     // Flux Field Array
        ])
        .split(main_layout[0]);

    let metrics_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(33), Constraint::Percentage(33), Constraint::Percentage(34)])
        .split(left_chunks[0]);

    let draw_digital_pulse = |f: &mut Frame, area: Rect, label: &str, cur: f32, total: f32, unit: &str, history: &[u64]| {
        let inner = area.inner(ratatui::layout::Margin { horizontal: 2, vertical: 0 });
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Label
                Constraint::Length(1), // Value
                Constraint::Min(0),    // Pulse
            ])
            .split(inner);

        f.render_widget(Paragraph::new(Span::styled(label, Style::default().fg(Color::Rgb(70, 75, 85)).add_modifier(Modifier::BOLD))), chunks[0]);

        let intensity = (cur / if total > 0.0 { total } else { 100.0 }).clamp(0.0, 1.0);
        let color = if intensity > 0.85 { Color::Rgb(255, 60, 60) } else if intensity > 0.5 { Color::Rgb(255, 180, 0) } else { Color::Rgb(0, 255, 150) };
        
        f.render_widget(Paragraph::new(Line::from(vec![
            Span::styled(format!("{:.1}", cur), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(unit, Style::default().fg(color).add_modifier(Modifier::DIM)),
        ])), chunks[1]);

        if !history.is_empty() {
            let width = chunks[2].width;
            let height = chunks[2].height;
            let max_val = if total > 0.0 { total as u64 } else { 100 };
            
            let data: Vec<u64> = history.iter().rev().take(width as usize).rev().cloned().collect();
            for (i, &val) in data.iter().enumerate() {
                let h_ratio = (val as f32 / max_val as f32).clamp(0.0, 1.0);
                let bar_h = (h_ratio * height as f32) as u16;
                
                for y in 0..height {
                    let rel_y = height.saturating_sub(y + 1);
                    let y_ratio = y as f32 / height as f32;
                    
                    if y < bar_h {
                        let shade = if y == bar_h.saturating_sub(1) { "⎯" }
                                   else if y > height / 2 { "▓" }
                                   else { "▒" };
                        
                        let shade_color = if y_ratio > 0.8 { Color::Rgb(255, 60, 60) }
                                         else if y_ratio > 0.4 { Color::Rgb(255, 180, 0) }
                                         else { Color::Rgb(0, 255, 150) };
                        
                        f.render_widget(Paragraph::new(Span::styled(shade, Style::default().fg(shade_color).add_modifier(if shade == "⎯" { Modifier::BOLD } else { Modifier::DIM }))), 
                            Rect::new(chunks[2].x + i as u16, chunks[2].y + rel_y, 1, 1));
                    }
                }
            }
        }
        
        f.render_widget(Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(Color::Rgb(30, 30, 35))), area);
    };

    draw_digital_pulse(f, metrics_layout[0], "PROCESSOR", app.system_state.cpu_usage, 0.0, "%", &app.system_state.cpu_history);
    draw_digital_pulse(f, metrics_layout[1], "MEMORY", app.system_state.mem_usage as f32, app.system_state.total_mem as f32, "GB", &app.system_state.mem_history);
    draw_digital_pulse(f, metrics_layout[2], "SWAP", app.system_state.swap_usage as f32, app.system_state.total_swap as f32, "GB", &app.system_state.swap_history);

    let fabric_area = left_chunks[1].inner(ratatui::layout::Margin { horizontal: 2, vertical: 1 });
    let core_count = app.system_state.cpu_cores.len();
    if core_count > 0 {
        f.render_widget(Paragraph::new(Span::styled("󰓅 FLUX FIELD MAP", Style::default().fg(Color::Rgb(50, 55, 65)).add_modifier(Modifier::BOLD))), Rect::new(fabric_area.x, fabric_area.y - 1, 30, 1));
        let cols = if core_count > 16 { 4 } else if core_count > 8 { 2 } else { 1 };
        let rows = (core_count as f32 / cols as f32).ceil() as u16;
        let fabric_rows = Layout::default().direction(Direction::Vertical).constraints(vec![Constraint::Length(1); rows as usize]).split(fabric_area);

        for r in 0..rows {
            if r as usize >= fabric_rows.len() { break; }
            let core_cols = Layout::default().direction(Direction::Horizontal).constraints(vec![Constraint::Percentage(100 / cols); cols as usize]).split(fabric_rows[r as usize]);

            for c in 0..cols {
                let idx = (r * cols + c) as usize;
                if idx < core_count {
                    let usage = app.system_state.cpu_cores[idx];
                    let intensity = usage / 100.0;
                    let color = if intensity > 0.9 { Color::Rgb(255, 60, 60) } else if intensity > 0.5 { Color::Rgb(255, 180, 0) } else { Color::Rgb(0, 255, 150) };
                    let slot = core_cols[c as usize].inner(ratatui::layout::Margin { horizontal: 1, vertical: 0 });
                    let track_w: u16 = slot.width.saturating_sub(14);
                    let pos = (intensity * track_w as f32) as u16;
                    let track = format!("{}{}{}", "─".repeat(pos as usize), "┼", "─".repeat(track_w.saturating_sub(pos + 1) as usize));
                    f.render_widget(Paragraph::new(Line::from(vec![
                        Span::styled(format!("0x{:X} ", idx), Style::default().fg(Color::Rgb(40, 45, 55))),
                        Span::styled("╾", Style::default().fg(Color::Rgb(30, 30, 35))),
                        Span::styled(track, Style::default().fg(color)),
                        Span::styled("╼", Style::default().fg(Color::Rgb(30, 30, 35))),
                        Span::styled(format!(" {:>3.0}%", usage), Style::default().fg(if intensity > 0.1 { color } else { Color::Rgb(50, 55, 65) })),
                    ])), slot);
                }
            }
        }
    }

    let right_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(8), Constraint::Length(12), Constraint::Min(0)]).split(main_layout[1]);
    let id_info = vec![
        Line::from(vec![Span::styled("󰣇 ", Style::default().fg(Color::Rgb(0, 180, 255))), Span::styled(&app.system_state.hostname, Style::default().add_modifier(Modifier::BOLD))]),
        Line::from(vec![Span::styled("󰔠 ", Style::default().fg(Color::Rgb(255, 200, 0))), Span::raw(format!("{}d {}h", app.system_state.uptime / 86400, (app.system_state.uptime % 86400) / 3600))]),
        Line::from(Span::styled(&app.system_state.kernel_version, Style::default().fg(Color::Rgb(50, 55, 65)))),
    ];
    f.render_widget(Paragraph::new(id_info).block(Block::default().borders(Borders::LEFT).border_style(Style::default().fg(Color::Rgb(25, 25, 30)))), right_chunks[0]);

    let net_area = right_chunks[1].inner(ratatui::layout::Margin { horizontal: 1, vertical: 0 });
    let net_sub = Layout::default().direction(Direction::Vertical).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(net_area);
    let draw_flux_pipe = |f: &mut Frame, area: Rect, label: &str, history: &[u64], color: Color| {
        let val = history.last().cloned().unwrap_or(0);
        f.render_widget(Paragraph::new(Span::styled(format!("{} {:>8}/s", label, format_size(val)), Style::default().fg(color).add_modifier(Modifier::BOLD))), Rect::new(area.x, area.y, area.width, 1));
        if !history.is_empty() {
            let max_v = history.iter().max().cloned().unwrap_or(1024) as f64;
            let data: Vec<(f64, f64)> = history.iter().enumerate().map(|(i, &v)| (i as f64, v as f64)).collect();
            let chart = Chart::new(vec![Dataset::default().marker(symbols::Marker::Braille).graph_type(GraphType::Line).style(Style::default().fg(color).add_modifier(Modifier::DIM)).data(&data)]).x_axis(Axis::default().bounds([0.0, 100.0])).y_axis(Axis::default().bounds([0.0, max_v]));
            f.render_widget(chart, Rect::new(area.x, area.y + 1, area.width, area.height.saturating_sub(1)));
        }
    };
    draw_flux_pipe(f, net_sub[0], "↓", &app.system_state.net_in_history, Color::Rgb(0, 255, 150));
    draw_flux_pipe(f, net_sub[1], "↑", &app.system_state.net_out_history, Color::Rgb(0, 180, 255));

    let disk_list: Vec<ListItem> = app.system_state.disks.iter().map(|disk| {
        let ratio = (disk.used_space / disk.total_space).clamp(0.0, 1.0);
        let color = if ratio > 0.9 { Color::Rgb(255, 60, 60) } else if ratio > 0.7 { Color::Rgb(255, 180, 0) } else { Color::Rgb(0, 255, 150) };
        let track_w: u16 = 12;
        let pos = (ratio * track_w as f64) as u16;
        let track = format!("{}{}{}", "─".repeat(pos as usize), "┼", "─".repeat(track_w.saturating_sub(pos + 1) as usize));
        ListItem::new(vec![Line::from(vec![Span::styled("󰋊 ", Style::default().fg(color)), Span::styled(&disk.name, Style::default().fg(Color::White))]), Line::from(vec![Span::styled(track, Style::default().fg(Color::Rgb(30, 30, 35))), Span::styled(format!(" {:.0}%", ratio * 100.0), Style::default().fg(Color::Rgb(60, 65, 75)))]), Line::from("")])
    }).collect();
    f.render_widget(List::new(disk_list).block(Block::default().title(Span::styled(" STORAGE ", Style::default().fg(Color::Rgb(50, 55, 65)).add_modifier(Modifier::BOLD)).borders(Borders::LEFT).border_style(Style::default().fg(Color::Rgb(25, 25, 30))))), right_chunks[2]);
}

fn draw_monitor_applications(f: &mut Frame, area: Rect, app: &mut App) {
    let current_user = std::env::var("USER").unwrap_or_else(|_| "dracon".to_string());
    let app_procs: Vec<_> = app.system_state.processes.iter().filter(|p| {
        let matches = if app.process_search_filter.is_empty() { true } else { p.name.to_lowercase().contains(&app.process_search_filter.to_lowercase()) };
        p.user == current_user && !p.name.starts_with('[') && !p.name.contains("kworker") && matches
    }).collect();

    let rows = app_procs.iter().enumerate().map(|(i, p)| {
        let mut is_selected = false;
        let mut style = if i % 2 == 0 { Style::default().fg(Color::Rgb(180, 185, 190)) } else { Style::default().fg(Color::Rgb(140, 145, 150)) };
        if app.process_selected_idx == Some(i) && app.monitor_subview == MonitorSubview::Applications { style = style.bg(Color::Rgb(0, 180, 255)).fg(Color::Black).add_modifier(Modifier::BOLD); is_selected = true; }
        let cpu_color = if is_selected { Color::Black } else if p.cpu > 50.0 { Color::Red } else { Color::Rgb(0, 255, 150) };
        Row::new(vec![Cell::from(format!("  {}", p.name)), Cell::from(format!("{:.1}%", p.cpu)).style(Style::default().fg(cpu_color)), Cell::from(format!("{:.1} MB", p.mem)), Cell::from(p.pid.to_string()).style(Style::default().fg(if is_selected { Color::Black } else { Color::Rgb(60, 65, 75) })), Cell::from(p.status.clone())]).style(style)
    });
    f.render_widget(Table::new(rows, [Constraint::Min(35), Constraint::Length(10), Constraint::Length(15), Constraint::Length(10), Constraint::Length(15)]).header(Row::new(vec!["  Application", "CPU", "Memory", "PID", "Status"]).style(Style::default().fg(Color::Rgb(80, 85, 95)).add_modifier(Modifier::BOLD)).height(1).bottom_margin(1)).column_spacing(2), area);
}

fn draw_processes_view(f: &mut Frame, area: Rect, app: &mut App) {
    let column_constraints = [Constraint::Length(8), Constraint::Min(25), Constraint::Length(15), Constraint::Length(12), Constraint::Length(10), Constraint::Length(10)];
    app.process_column_bounds.clear();
    let header_rects = Layout::default().direction(Direction::Horizontal).constraints(column_constraints).split(Rect::new(area.x, area.y, area.width, 1));
    let header_cells = ["PID", "NAME", "USER", "STATUS", "CPU%", "MEM%"].iter().enumerate().map(|(i, h)| {
        let col = match *h {
            "PID" => ProcessColumn::Pid,
            "NAME" => ProcessColumn::Name,
            "USER" => ProcessColumn::User,
            "STATUS" => ProcessColumn::Status,
            "CPU%" => ProcessColumn::Cpu,
            "MEM%" => ProcessColumn::Mem,
            _ => ProcessColumn::Pid,
        };
        app.process_column_bounds.push((header_rects[i], col));
        let mut text = h.to_string();
        if app.process_sort_col == col { text.push_str(if app.process_sort_asc { " 󰁝" } else { " 󰁅" }); }
        Cell::from(text).style(Style::default().fg(if app.process_sort_col == col { Color::Rgb(0, 180, 255) } else { Color::Rgb(60, 65, 75) }).add_modifier(Modifier::BOLD))
    });
    let rows = app.system_state.processes.iter().enumerate().map(|(i, p)| {
        let mut is_selected = false;
        let mut style = if i % 2 == 0 { Style::default().fg(Color::Rgb(180, 185, 190)) } else { Style::default().fg(Color::Rgb(140, 145, 150)) };
        if app.process_selected_idx == Some(i) && app.monitor_subview == MonitorSubview::Processes { style = style.bg(Color::Rgb(0, 180, 255)).fg(Color::Black).add_modifier(Modifier::BOLD); is_selected = true; }
        let cpu_color = if is_selected { Color::Black } else if p.cpu > 50.0 { Color::Red } else { Color::Rgb(0, 255, 150) };
        Row::new(vec![Cell::from(format!("  {}", p.pid)).style(Style::default().fg(if is_selected { Color::Black } else { Color::Rgb(60, 65, 75) })), Cell::from(p.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)), Cell::from(p.user.clone()).style(Style::default().fg(if is_selected { Color::Black } else { Color::Rgb(0, 180, 255) })), Cell::from(p.status.clone()), Cell::from(format!("{:.1}", p.cpu)).style(Style::default().fg(cpu_color)), Cell::from(format!("{:.1}", p.mem))]).style(style)
    });
    f.render_stateful_widget(Table::new(rows, column_constraints).header(Row::new(header_cells).height(1).bottom_margin(1)).column_spacing(1), area, &mut app.process_table_state);
}

fn draw_global_header(f: &mut Frame, area: Rect, sidebar_width: u16, app: &mut App) {
    let icons = vec![(Icon::Burger.get(app.icon_mode), "burger"), (Icon::Back.get(app.icon_mode), "back"), (Icon::Forward.get(app.icon_mode), "forward"), (Icon::Split.get(app.icon_mode), "split"), (Icon::Monitor.get(app.icon_mode), "monitor")];
    let mut cur_x = area.x + 1;
    app.header_icon_bounds.clear();
    for (_i, (icon, id)) in icons.iter().enumerate() {
        let rect = Rect::new(cur_x, area.y, 4, 1);
        let is_hovered = app.mouse_pos.1 == area.y && app.mouse_pos.0 >= rect.x && app.mouse_pos.0 < rect.x + rect.width;
        let mut style = Style::default().fg(Color::Rgb(100, 100, 110));
        if is_hovered { style = style.fg(THEME.accent_primary); f.render_widget(Paragraph::new("▔").style(style), Rect::new(rect.x + 1, rect.y + 1, 2, 1)); }
        f.render_widget(Paragraph::new(format!(" {} ", icon)).style(style), rect);
        app.header_icon_bounds.push((rect, id.to_string())); cur_x += 4;
    }
    let start_x = if app.show_sidebar { std::cmp::max(area.x + sidebar_width, cur_x + 2) } else { cur_x + 2 };
    let pane_count = app.panes.len();
    if pane_count > 0 {
        let pane_chunks = Layout::default().direction(Direction::Horizontal).constraints(vec![Constraint::Percentage(100 / pane_count as u16); pane_count]).split(Rect::new(start_x, area.y, area.width.saturating_sub(start_x), 1));
        app.tab_bounds.clear();
        for (p_i, pane) in app.panes.iter().enumerate() {
            let chunk = pane_chunks[p_i]; let mut current_x = chunk.x;
            for (t_i, tab) in pane.tabs.iter().enumerate() {
                let name = tab.current_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or("/".to_string());
                let is_active = t_i == pane.active_tab_index;
                let is_focused = p_i == app.focused_pane_index && !app.sidebar_focus;
                let style = if is_active && is_focused { Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD) } else if is_active { Style::default().fg(THEME.accent_primary) } else { Style::default().fg(Color::Rgb(60, 65, 75)) };
                let text = format!(" {} ", name); let width = text.len() as u16;
                if current_x + width > chunk.x + chunk.width { break; }
                let rect = Rect::new(current_x, area.y, width, 1);
                f.render_widget(Paragraph::new(text).style(style), rect);
                if is_active && is_focused { f.render_widget(Paragraph::new("▔".repeat(width as usize)).style(style), Rect::new(rect.x, rect.y + 1, rect.width, 1)); }
                app.tab_bounds.push((rect, p_i, t_i)); current_x += width + 1;
            }
        }
    }
}

fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
    if app.current_view == CurrentView::Files {
        let pane_count = app.panes.len();
        if pane_count > 0 {
            let chunks = Layout::default().direction(Direction::Horizontal).constraints(vec![Constraint::Percentage(100 / pane_count as u16); pane_count]).split(area);
            for i in 0..pane_count { let is_focused = i == app.focused_pane_index && !app.sidebar_focus; draw_file_view(f, chunks[i], app, i, is_focused); }
        }
    }
}

fn draw_file_view(f: &mut Frame, area: Rect, app: &mut App, pane_idx: usize, is_focused: bool) {
    if let Some(file_state) = app.panes.get_mut(pane_idx).and_then(|p| p.current_state_mut()) {
        file_state.view_height = area.height as usize;
        let mut render_state = TableState::default();
        if let Some(sel) = file_state.selected_index { let offset = file_state.table_state.offset(); if sel >= offset && sel < offset + area.height as usize - 2 { render_state.select(Some(sel)); } }
        *render_state.offset_mut() = file_state.table_state.offset();
        let constraints = [Constraint::Min(20), Constraint::Length(10)];
        let rows = file_state.files.iter().enumerate().map(|(i, path)| {
            let metadata = file_state.metadata.get(path);
            let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
            let mut style = if is_dir { Style::default().fg(THEME.accent_secondary) } else { Style::default().fg(Color::Rgb(180, 185, 190)) };
            if file_state.multi_select.contains(&i) && is_focused { style = style.bg(Color::Rgb(80, 0, 0)).fg(Color::White); }
            let icon = if is_dir { Icon::Folder.get(app.icon_mode) } else { Icon::File.get(app.icon_mode) };
            Row::new(vec![Cell::from(format!(" {} {}", icon, path.file_name().unwrap_or_default().to_string_lossy())), Cell::from(format_size(metadata.map(|m| m.size).unwrap_or(0))).style(Style::default().fg(Color::Rgb(60, 65, 75)))]).style(style)
        });
        f.render_stateful_widget(Table::new(rows, constraints).row_highlight_style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD)).column_spacing(1), area, &mut render_state);
        f.render_widget(Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(Color::Rgb(30, 30, 35))), area);
        *file_state.table_state.offset_mut() = render_state.offset();
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Min(0), Constraint::Length(30), Constraint::Percentage(30)]).split(area);
    f.render_widget(Paragraph::new(Line::from(vec![Span::styled(" ^Q ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw("Quit ")])), chunks[0]);
    f.render_widget(Paragraph::new(draw_stat_bar("CPU", app.system_state.cpu_usage, 100.0, chunks[2].width / 2, THEME.fg)).alignment(ratatui::layout::Alignment::Right), chunks[2]);
}

fn draw_rename_modal(f: &mut Frame, app: &App) {
    let area = terma::layout::centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(format!("{}", &app.input.value)).block(Block::default().borders(Borders::ALL).title(" Rename ")), area);
}

fn draw_delete_modal(f: &mut Frame, _app: &App) {
    let area = terma::layout::centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new("Confirm Delete? (y/n)").block(Block::default().borders(Borders::ALL).title(" Delete ")), area);
}

fn draw_new_folder_modal(f: &mut Frame, app: &App) {
    let area = terma::layout::centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(format!("{}", &app.input.value)).block(Block::default().borders(Borders::ALL).title(" New Folder ")), area);
}

fn draw_new_file_modal(f: &mut Frame, app: &App) {
    let area = terma::layout::centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    f.render_widget(Paragraph::new(format!("{}", &app.input.value)).block(Block::default().borders(Borders::ALL).title(" New File ")), area);
}
