use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, AppMode, CurrentView, LicenseStatus};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    let workspace = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(12), // Dock
            Constraint::Percentage(20), // Sidebar
            Constraint::Min(0), // Main Stage
        ])
        .split(chunks[0]);

    draw_dock(f, workspace[0], app);
    draw_sidebar(f, workspace[1], app);
    draw_main_stage(f, workspace[2], app);
    
    draw_footer(f, chunks[1], app);

    if matches!(app.mode, AppMode::CommandPalette) {
        draw_command_palette(f, app);
    }

    if matches!(app.mode, AppMode::Rename) {
        draw_rename_modal(f, app);
    }

    if matches!(app.mode, AppMode::Properties) {
        draw_properties_modal(f, app);
    }
}

fn draw_rename_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    let block = Block::default().title(" Rename ").borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(Paragraph::new(app.input.as_str()), inner);
}

fn draw_properties_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 30, f.area());
    f.render_widget(Clear, area);
    let block = Block::default().title(" Properties ").borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(path) = app.file_state.files.get(app.file_state.selected_index) {
        let metadata = std::fs::metadata(path);
        let mut info = format!("Name: {}\n", path.file_name().unwrap_or_default().to_string_lossy());
        info.push_str(&format!("Type: {}\n", if path.is_dir() { "Directory" } else { "File" }));
        
        if let Ok(m) = metadata {
            info.push_str(&format!("Size: {} bytes\n", m.len()));
            if let Ok(modified) = m.modified() {
                info.push_str(&format!("Modified: {:?}\n", modified));
            }
        }
        f.render_widget(Paragraph::new(info), inner);
    }
}

fn draw_dock(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Dock ")
        .border_style(if app.sidebar_focus { Style::default().fg(Color::Cyan) } else { Style::default() });
    
    f.render_widget(block, area);

    let inner = area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 1 });
    
    let items = vec![
        ("Files", CurrentView::Files), 
        ("Docker", CurrentView::Docker), 
        ("System", CurrentView::System)
    ];

    let list_items: Vec<ListItem> = items.iter().map(|(label, view)| {
        let style = if app.current_view == *view {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        
        let prefix = if app.current_view == *view && app.sidebar_focus {
            "> "
        } else {
            "  "
        };

        ListItem::new(format!("{}{}", prefix, label)).style(style)
    }).collect();

    let list = List::new(list_items);
    f.render_widget(list, inner);
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Sidebar ")
        .border_style(if app.sidebar_focus && app.current_view == CurrentView::Files { Style::default().fg(Color::Cyan) } else { Style::default() });
    
    f.render_widget(block, area);
    
    let inner = area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 1 });

    match app.current_view {
        CurrentView::Files => {
            let sidebar_items = vec!["Home", "Downloads", "Documents", "Pictures"];
            let items: Vec<ListItem> = sidebar_items.iter().enumerate().map(|(i, name)| {
                let style = if i == app.sidebar_index && app.sidebar_focus {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(*name).style(style)
            }).collect();
            f.render_widget(List::new(items), inner);
        },
        CurrentView::Docker => {
             let items = vec![
                ListItem::new("Containers").style(Style::default().add_modifier(Modifier::BOLD)),
                ListItem::new("  All"),
                ListItem::new("  Running").style(Style::default().fg(Color::Green)),
                ListItem::new("  Stopped").style(Style::default().fg(Color::Red)),
                ListItem::new(""),
                ListItem::new("Images"),
                ListItem::new("Volumes"),
                ListItem::new("Networks"),
            ];
            f.render_widget(List::new(items), inner);
        },
        CurrentView::System => {
             let items = vec![
                ListItem::new("Overview").style(Style::default().add_modifier(Modifier::BOLD)),
                ListItem::new("Processes"),
                ListItem::new("Disks"),
                ListItem::new("Network"),
            ];
            f.render_widget(List::new(items), inner);
        }
    }
}

fn draw_main_stage(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {:?} ", app.current_view))
        .border_style(if !app.sidebar_focus { Style::default().fg(Color::Cyan) } else { Style::default() });
    
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.current_view == CurrentView::Files {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Path Bar
                Constraint::Min(0),    // File List
            ])
            .split(inner);

        let path_text = if matches!(app.mode, AppMode::Location) {
            format!("Location: {}", app.input)
        } else {
            format!("Path: {}", app.file_state.current_path.display())
        };

        let path_bar = Paragraph::new(path_text)
            .block(Block::default().borders(Borders::ALL).border_style(
                if matches!(app.mode, AppMode::Location) { Style::default().fg(Color::Yellow) } else { Style::default() }
            ));
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

fn draw_file_view(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.file_state.files.iter().enumerate().map(|(i, path)| {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
        let style = if path.is_dir() {
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        
        let prefix = if i == app.file_state.selected_index && !app.sidebar_focus {
            "> "
        } else {
            "  "
        };

        ListItem::new(format!("{}{}", prefix, name)).style(style)
    }).collect();

    let list = List::new(items);
    f.render_widget(list, area);
}

fn draw_system_view(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // CPU
            Constraint::Length(3), // MEM
            Constraint::Length(6), // Disks
            Constraint::Min(0),    // Processes
        ])
        .split(area);

    // CPU Gauge
    let cpu_gauge = Gauge::default()
        .block(Block::default().title(" CPU Usage ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green))
        .percent(app.system_state.cpu_usage as u16)
        .label(format!("{:.1}%", app.system_state.cpu_usage));
    f.render_widget(cpu_gauge, layout[0]);

    // Memory Gauge
    if app.system_state.total_mem > 0.0 {
        let mem_percent = (app.system_state.mem_usage / app.system_state.total_mem) * 100.0;
        let mem_gauge = Gauge::default()
            .block(Block::default().title(" Memory Usage ").borders(Borders::ALL))
            .gauge_style(Style::default().fg(Color::Yellow))
            .percent(mem_percent as u16)
            .label(format!("{:.1} / {:.1} GB", app.system_state.mem_usage, app.system_state.total_mem));
        f.render_widget(mem_gauge, layout[1]);
    }

    // Disk Usage List
    let disk_items: Vec<ListItem> = app.system_state.disks.iter().map(|disk| {
        let percent = (disk.used_space / disk.total_space) * 100.0;
        
        // Create a visual bar for the disk
        let bar_width: usize = 20;
        let filled = (percent / 100.0 * bar_width as f64) as usize;
        let empty = bar_width.saturating_sub(filled);
        let bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(empty));

        ListItem::new(format!(
            "{:<10} {}  {:.1} / {:.1} GB ({:.1}%)", 
            disk.name, bar, disk.used_space, disk.total_space, percent
        ))
    }).collect();
    
    let disk_list = List::new(disk_items).block(Block::default().title(" Disk Usage ").borders(Borders::ALL));
    f.render_widget(disk_list, layout[2]);

    // Process List
    let process_items: Vec<ListItem> = app.system_state.processes.iter().map(|p| {
        ListItem::new(format!(
            "{:<6} {:<20} {:.1}%  {:.1} MB", 
            p.pid, 
            p.name.chars().take(20).collect::<String>(), 
            p.cpu, 
            p.mem as f64 / 1024.0 / 1024.0
        ))
    }).collect();
    
    let process_list = List::new(process_items)
        .block(Block::default().title(" Top Processes ").borders(Borders::ALL));
    f.render_widget(process_list, layout[3]);
}

fn draw_docker_view(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.docker_state.containers.iter()
        .filter(|name| {
            if let Some(filter) = &app.docker_state.filter {
                name.contains(filter)
            } else {
                true
            }
        })
        .enumerate().map(|(i, name)| {
        let prefix = if i == app.docker_state.selected_index && !app.sidebar_focus {
            "> "
        } else {
            "  "
        };
        ListItem::new(format!("{}{}", prefix, name))
    }).collect();

    let list = List::new(items);
    f.render_widget(list, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let text = match &app.license {
        LicenseStatus::FreeMode => {
            " Arrows: Move | Enter: Open | Tiles Free Edition (<5 employees). Support us at dracon.uk ".to_string()
        }
        LicenseStatus::Commercial(company) => {
            format!(" Arrows: Move | Enter: Open | Licensed to {} ", company)
        }
    };

    let style = if matches!(app.license, LicenseStatus::FreeMode) {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    };

    let footer = Paragraph::new(text).style(style);
    f.render_widget(footer, area);
}

fn draw_command_palette(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Command Palette ")
        .border_style(Style::default().fg(Color::Magenta));
    
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .split(inner);

    let input = Paragraph::new(format!("> {}", app.input))
        .style(Style::default().fg(Color::Yellow));
    f.render_widget(input, chunks[0]);

    let items: Vec<ListItem> = app.filtered_commands.iter().enumerate().map(|(i, cmd)| {
        let style = if i == app.command_index {
             Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
             Style::default()
        };
        ListItem::new(cmd.label.clone()).style(style)
    }).collect();
    
    let list = List::new(items);
    f.render_widget(list, chunks[1]);
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