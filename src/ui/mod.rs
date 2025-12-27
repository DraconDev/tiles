use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph, Table, Row, Cell},
    Frame,
};

use crate::app::{App, AppMode, CurrentView, FileColumn};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Tabs (Top)
            Constraint::Min(0),    // Main Workspace
            Constraint::Length(1), // Footer (Bottom)
        ])
        .split(f.area());

    draw_tabs(f, chunks[0], app);

    let workspace = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Sidebar
            Constraint::Min(0), // Main Stage
        ])
        .split(chunks[1]);

    draw_sidebar(f, workspace[0], app);
    draw_main_stage(f, workspace[1], app);
    
    draw_footer(f, chunks[2], app);

    // Modals
    if matches!(app.mode, AppMode::Rename) {
        draw_rename_modal(f, app);
    }
    if matches!(app.mode, AppMode::Delete) {
        draw_delete_modal(f, app);
    }
    if matches!(app.mode, AppMode::Properties) {
        draw_properties_modal(f, app);
    }
    if matches!(app.mode, AppMode::NewFolder) {
        draw_new_folder_modal(f, app);
    }
    if matches!(app.mode, AppMode::ColumnSetup) {
        draw_column_setup_modal(f, app);
    }
    if matches!(app.mode, AppMode::CommandPalette) {
        draw_command_palette(f, app);
    }
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    let views = vec![
        ("^F Files", CurrentView::Files), 
        ("^P Proc", CurrentView::System), 
        ("^D Docker", CurrentView::Docker)
    ];
    for (label, view) in views {
        let style = if app.current_view == view {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(ratatui::text::Span::styled(format!(" {} ", label), style));
        spans.push(ratatui::text::Span::raw(" "));
    }
    spans.push(ratatui::text::Span::raw(" | "));
    if app.current_view == CurrentView::Files {
        for (i, tab) in app.file_tabs.iter().enumerate() {
            let name = tab.current_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "/".to_string());
            let style = if i == app.tab_index { Style::default().fg(Color::Yellow).add_modifier(Modifier::UNDERLINED) } else { Style::default().fg(Color::Gray) };
            spans.push(ratatui::text::Span::styled(format!("[{}]", name), style));
            spans.push(ratatui::text::Span::raw(" "));
        }
    }
    f.render_widget(Paragraph::new(ratatui::text::Line::from(spans)), area);
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(" Sidebar ").border_style(if app.sidebar_focus && app.current_view == CurrentView::Files { Style::default().fg(Color::Cyan) } else { Style::default() });
    f.render_widget(block, area);
    let inner = area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 1 });
    match app.current_view {
        CurrentView::Files => {
            let sidebar_items = vec!["Home", "Downloads", "Documents", "Pictures"];
            let items: Vec<ListItem> = sidebar_items.iter().enumerate().map(|(i, name)| {
                let style = if i == app.sidebar_index && app.sidebar_focus { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
                ListItem::new(*name).style(style)
            }).collect();
            f.render_widget(List::new(items), inner);
        },
        CurrentView::Docker => {
             let items = vec![ ListItem::new("Containers").style(Style::default().add_modifier(Modifier::BOLD)), ListItem::new("  All"), ListItem::new("  Running").style(Style::default().fg(Color::Green)), ListItem::new("  Stopped").style(Style::default().fg(Color::Red))];
             f.render_widget(List::new(items), inner);
        },
        CurrentView::System => {
             let items = vec![ ListItem::new("Overview").style(Style::default().add_modifier(Modifier::BOLD)), ListItem::new("Processes"), ListItem::new("Disks")];
             f.render_widget(List::new(items), inner);
        }
    }
}

fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default().borders(Borders::ALL).title(format!(" {:?} ", app.current_view)).border_style(if !app.sidebar_focus { Style::default().fg(Color::Cyan) } else { Style::default() });
    let inner = block.inner(area);
    f.render_widget(block, area);
    if app.current_view == CurrentView::Files {
        let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(inner);
        let path_text = if matches!(app.mode, AppMode::Location) { format!("Location: {}", app.input) } 
            else if let Some(fs) = app.current_file_state() { if !fs.search_filter.is_empty() { format!("Search: {} (Esc to clear)", fs.search_filter) } else { format!("Path: {}", fs.current_path.display()) } } 
            else { String::new() };
        let path_bar = Paragraph::new(path_text).block(Block::default().borders(Borders::ALL).border_style(if matches!(app.mode, AppMode::Location) { Style::default().fg(Color::Yellow) } else if app.current_file_state().map(|s| !s.search_filter.is_empty()).unwrap_or(false) { Style::default().fg(Color::Magenta) } else { Style::default() }));
        f.render_widget(path_bar, chunks[0]);
        draw_file_view(f, chunks[1], app);
    } else {
        match app.current_view {
            CurrentView::System => draw_system_view(f, inner, app),
            CurrentView::Docker => draw_docker_view(f, inner, app),
            _ => {}
        }
    }
}

use std::{time::SystemTime};

fn draw_file_view(f: &mut Frame, area: Rect, app: &mut App) {
    let sidebar_focus = app.sidebar_focus;
    if let Some(file_state) = app.current_file_state_mut() {
        let header_cells = file_state.columns.iter().map(|c| {
            let name = match c { FileColumn::Name => "Name", FileColumn::Size => "Size", FileColumn::Modified => "Modified", FileColumn::Created => "Created", FileColumn::Permissions => "Permissions", FileColumn::Extension => "Ext" };
            Cell::from(name).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        });
        let header = Row::new(header_cells).height(1).bottom_margin(1);
        let rows = file_state.files.iter().enumerate().map(|(i, path)| {
            let metadata = std::fs::metadata(path).ok();
            let cells = file_state.columns.iter().map(|c| {
                match c {
                    FileColumn::Name => {
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
                        let mut display_name = name.to_string();
                        let mut style = if path.is_dir() { Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD) } else { Style::default() };
                        if let Some(status) = file_state.git_status.get(path) {
                            display_name.push_str(&format!(" [{}]", status));
                            match status.as_str() { "M" | "MM" => style = style.fg(Color::Yellow), "A" | "AM" => style = style.fg(Color::Green), "??" => style = style.fg(Color::DarkGray), "D" => style = style.fg(Color::Red), _ => {} }
                        }
                        if file_state.starred.contains(path) { display_name.push_str(" [*]"); style = style.fg(Color::Yellow).add_modifier(Modifier::BOLD); }
                        let icon = if path.is_dir() { "📁 " } else { "📄 " };
                        Cell::from(format!("{}{}", icon, display_name)).style(style)
                    },
                    FileColumn::Size => Cell::from(format_size(metadata.as_ref().map(|m| m.len()).unwrap_or(0))),
                    FileColumn::Modified => Cell::from(format_time(metadata.as_ref().and_then(|m| m.modified().ok()).unwrap_or(SystemTime::UNIX_EPOCH))),
                    FileColumn::Created => Cell::from(format_time(metadata.as_ref().and_then(|m| m.created().ok()).unwrap_or(SystemTime::UNIX_EPOCH))),
                    FileColumn::Permissions => Cell::from(format_permissions(metadata.as_ref().map(|m| { #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; m.permissions().mode() } #[cfg(not(unix))] { 0 } }).unwrap_or(0))),
                    FileColumn::Extension => Cell::from(path.extension().and_then(|e| e.to_str()).unwrap_or("")),
                }
            });
            let style = if i == file_state.selected_index && !sidebar_focus { Style::default().bg(Color::DarkGray) } else { Style::default() };
            Row::new(cells).style(style)
        });
        let constraints: Vec<Constraint> = file_state.columns.iter().map(|c| {
            match c {
                FileColumn::Name => Constraint::Percentage(50),
                FileColumn::Size => Constraint::Length(10),
                FileColumn::Modified => Constraint::Length(20),
                FileColumn::Created => Constraint::Length(20),
                FileColumn::Permissions => Constraint::Length(12),
                FileColumn::Extension => Constraint::Length(6)
            }
        }).collect();
        f.render_stateful_widget(Table::new(rows, constraints).header(header).block(Block::default().borders(Borders::NONE)), area, &mut file_state.table_state);
    }
}

fn draw_system_view(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Length(6), Constraint::Min(0)]).split(area);
    f.render_widget(Gauge::default().block(Block::default().title(" CPU Usage ").borders(Borders::ALL)).gauge_style(Style::default().fg(Color::Green)).percent(app.system_state.cpu_usage as u16).label(format!("{:.1}%", app.system_state.cpu_usage)), layout[0]);
    if app.system_state.total_mem > 0.0 {
        f.render_widget(Gauge::default().block(Block::default().title(" Memory Usage ").borders(Borders::ALL)).gauge_style(Style::default().fg(Color::Yellow)).percent((app.system_state.mem_usage / app.system_state.total_mem * 100.0) as u16).label(format!("{:.1} / {:.1} GB", app.system_state.mem_usage, app.system_state.total_mem)), layout[1]);
    }
    let disk_items: Vec<ListItem> = app.system_state.disks.iter().map(|disk| {
        let percent = (disk.used_space / disk.total_space) * 100.0;
        let bar_width = 20; let filled = (percent / 100.0 * bar_width as f64) as usize;
        let bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(bar_width.saturating_sub(filled)));
        ListItem::new(format!("{:<10} {}  {:.1} / {:.1} GB ({:.1}%)", disk.name, bar, disk.used_space, disk.total_space, percent))
    }).collect();
    f.render_widget(List::new(disk_items).block(Block::default().title(" Disk Usage ").borders(Borders::ALL)), layout[2]);
    let process_items: Vec<ListItem> = app.system_state.processes.iter().enumerate().map(|(i, p)| {
        let style = if i == app.system_state.selected_process_index && !app.sidebar_focus { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default() };
        ListItem::new(format!("{}{:<6} {:<20} {:.1}%  {:.1} MB", if i == app.system_state.selected_process_index && !app.sidebar_focus { "> " } else { "  " }, p.pid, p.name.chars().take(20).collect::<String>(), p.cpu, p.mem as f64 / 1024.0 / 1024.0)).style(style)
    }).collect();
    f.render_widget(List::new(process_items).block(Block::default().title(" Top Processes ").borders(Borders::ALL)), layout[3]);
}

fn draw_docker_view(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.docker_state.containers.iter().filter_map(|c| {
        let name = c.names.as_ref().and_then(|n| n.first()).map(|s| s.as_str()).unwrap_or("").trim_start_matches('/');
        if let Some(filter) = &app.docker_state.filter { if !name.contains(filter) { return None; } }
        Some((name, c.state.as_deref().unwrap_or(""), c.status.as_deref().unwrap_or("")))
    }).enumerate().map(|(i, (name, state, status))| {
        let style = match state {
            "running" => Style::default().fg(Color::Green),
            "exited" => Style::default().fg(Color::Red),
            _ => Style::default()
        };
        ListItem::new(format!("{}{:<20} {:<10} {}", if i == app.docker_state.selected_index && !app.sidebar_focus { "> " } else { "  " }, name, state, status)).style(style)
    }).collect();
    f.render_widget(List::new(items), area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    let console_style = if matches!(app.mode, AppMode::CommandPalette) { Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Yellow) };
    spans.push(ratatui::text::Span::styled("^.", console_style)); spans.push(ratatui::text::Span::raw(" Console | "));
    spans.push(ratatui::text::Span::styled("^H", Style::default().fg(Color::Yellow))); spans.push(ratatui::text::Span::raw(" Hidden | "));
    spans.push(ratatui::text::Span::styled("^B", Style::default().fg(Color::Yellow))); spans.push(ratatui::text::Span::raw(" Star | "));
    spans.push(ratatui::text::Span::styled("^T", Style::default().fg(Color::Yellow))); spans.push(ratatui::text::Span::raw(" New Tab | "));
    spans.push(ratatui::text::Span::styled("^W", Style::default().fg(Color::Yellow))); spans.push(ratatui::text::Span::raw(" Close Tab | "));
    spans.push(ratatui::text::Span::styled("Del", Style::default().fg(Color::Yellow))); spans.push(ratatui::text::Span::raw(" Action "));
    if let Some(disk) = app.system_state.disks.first() { spans.push(ratatui::text::Span::raw(" | Storage: ")); spans.push(ratatui::text::Span::styled(format!("{:.1}/{:.1} GB", disk.total_space - disk.used_space, disk.total_space), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))); }
    f.render_widget(Paragraph::new(ratatui::text::Line::from(spans)), area);
}

fn draw_command_palette(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area()); f.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).title(" Command Palette ").border_style(Style::default().fg(Color::Magenta));
    let inner = block.inner(area); f.render_widget(block, area);
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(1), Constraint::Min(0)]).split(inner);
    f.render_widget(Paragraph::new(format!("> {}", app.input)).style(Style::default().fg(Color::Yellow)), chunks[0]);
    let items: Vec<ListItem> = app.filtered_commands.iter().enumerate().map(|(i, cmd)| {
        let style = if i == app.command_index { Style::default().bg(Color::DarkGray).fg(Color::White) } else { Style::default() };
        ListItem::new(cmd.label.clone()).style(style)
    }).collect();
    f.render_widget(List::new(items), chunks[1]);
}

fn draw_rename_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    let block = Block::default().title(" Rename ").borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area); f.render_widget(block, area);
    f.render_widget(Paragraph::new(app.input.as_str()), inner);
}

fn draw_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    let block = Block::default().title(" Confirm Action ").borders(Borders::ALL).border_style(Style::default().fg(Color::Red));
    let inner = block.inner(area); f.render_widget(block, area);
    let text = match app.current_view {
        CurrentView::Files => if let Some(fs) = app.current_file_state() { if let Some(p) = fs.files.get(fs.selected_index) { format!("Delete {}? (y/n)", p.file_name().unwrap_or_default().to_string_lossy()) } else { "Delete? (y/n)".to_string() } } else { "Delete? (y/n)".to_string() },
        CurrentView::System => if let Some(p) = app.system_state.processes.get(app.system_state.selected_process_index) { format!("Kill process {} ({})? (y/n)", p.name, p.pid) } else { "Kill process? (y/n)".to_string() },
        CurrentView::Docker => if let Some(c) = app.docker_state.containers.get(app.docker_state.selected_index) { let name = c.names.as_ref().and_then(|n| n.first()).map(|s| s.as_str()).unwrap_or("").trim_start_matches('/'); format!("Remove container {}? (y/n)", name) } else { "Remove container? (y/n)".to_string() }
    };
    f.render_widget(Paragraph::new(text), inner);
}

fn draw_properties_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 30, f.area()); f.render_widget(Clear, area);
    let block = Block::default().title(" Properties ").borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area); f.render_widget(block, area);
    let info = match app.current_view {
        CurrentView::Files => if let Some(fs) = app.current_file_state() { if let Some(p) = fs.files.get(fs.selected_index) { let metadata = std::fs::metadata(p); let mut s = format!("Name: {}
", p.file_name().unwrap_or_default().to_string_lossy()); s.push_str(&format!("Type: {}
", if p.is_dir() { "Directory" } else { "File" })); if let Ok(m) = metadata { s.push_str(&format!("Size: {} bytes
", m.len())); if let Ok(modi) = m.modified() { s.push_str(&format!("Modified: {:?}
", modi)); } } s } else { "No file selected".to_string() } } else { "No file selected".to_string() },
        CurrentView::System => if let Some(p) = app.system_state.processes.get(app.system_state.selected_process_index) { format!("PID: {}
Name: {}
CPU: {:.2}%
Memory: {:.2} MB", p.pid, p.name, p.cpu, p.mem as f64 / 1024.0 / 1024.0) } else { "No process selected".to_string() },
        CurrentView::Docker => if let Some(c) = app.docker_state.containers.get(app.docker_state.selected_index) { let name = c.names.as_ref().and_then(|n| n.first()).map(|s| s.as_str()).unwrap_or("").trim_start_matches('/'); let id = c.id.as_deref().unwrap_or("?"); let image = c.image.as_deref().unwrap_or("?"); let state = c.state.as_deref().unwrap_or("?"); let status = c.status.as_deref().unwrap_or("?"); format!("Name: {}
ID: {}
Image: {}
State: {}
Status: {}", name, id, image, state, status) } else { "No container selected".to_string() }
    };
    f.render_widget(Paragraph::new(info), inner);
}

fn draw_column_setup_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 40, f.area()); f.render_widget(Clear, area);
    let block = Block::default().title(" Column Setup (Alt+C to close) ").borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area); f.render_widget(block, area);
    if let Some(fs) = app.current_file_state() {
        let options = vec![(FileColumn::Name, "Name (n)"), (FileColumn::Size, "Size (s)"), (FileColumn::Modified, "Modified (m)"), (FileColumn::Created, "Created (c)"), (FileColumn::Permissions, "Permissions (p)"), (FileColumn::Extension, "Extension (e)")];
        let items: Vec<ListItem> = options.iter().map(|(col, label)| { let prefix = if fs.columns.contains(col) { "[x] " } else { "[ ] " }; ListItem::new(format!("{}{}", prefix, label)) }).collect();
        f.render_widget(List::new(items), inner);
    }
}

fn draw_new_folder_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); f.render_widget(Clear, area);
    let block = Block::default().title(" New Folder ").borders(Borders::ALL).border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area); f.render_widget(block, area);
    f.render_widget(Paragraph::new(app.input.as_str()), inner);
}

use chrono::{DateTime, Local};
fn format_size(size: u64) -> String { if size >= 1073741824 { format!("{:.1} GB", size as f64 / 1073741824.0) } else if size >= 1048576 { format!("{:.1} MB", size as f64 / 1048576.0) } else if size >= 1024 { format!("{:.1} KB", size as f64 / 1024.0) } else { format!("{} B", size) } }
fn format_time(time: SystemTime) -> String { let datetime: DateTime<Local> = time.into(); datetime.format("%Y-%m-%d %H:%M").to_string() }
fn format_permissions(mode: u32) -> String {
    let user = (mode >> 6) & 0o7; let group = (mode >> 3) & 0o7; let other = mode & 0o7;
    let r = |b| if b & 4 != 0 { "r" } else { "-" }; let w = |b| if b & 2 != 0 { "w" } else { "-" }; let x = |b| if b & 1 != 0 { "x" } else { "-" };
    format!("{}{}{}{}{}{}{}{}{}", 
        r(user), w(user), x(user), 
        r(group), w(group), x(group), 
        r(other), w(other), x(other)
    )
}
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
