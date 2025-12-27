use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, AppMode, TileType, LicenseStatus};

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    draw_main(f, chunks[0], app);
    draw_footer(f, chunks[1], app);
}

fn draw_main(f: &mut Frame, area: Rect, app: &mut App) {
    if matches!(app.mode, AppMode::Zoomed) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {:?} (Zoomed) ", app.active_tile))
            .border_style(Style::default().fg(Color::Yellow));
        
        let inner_area = block.inner(area);
        f.render_widget(block, area);
        
        match app.active_tile {
            TileType::Files => draw_file_tile(f, inner_area, app),
            TileType::System => draw_system_tile(f, inner_area, app),
            TileType::Docker => draw_docker_tile(f, inner_area, app),
            _ => {}
        }
        return;
    }

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(main_chunks[1]);

    // File Tile
    draw_file_tile(f, main_chunks[0], app);
    
    // System Tile
    draw_system_tile(f, right_chunks[0], app);

    // Docker Tile
    draw_docker_tile(f, right_chunks[1], app);
}

fn draw_file_tile(f: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_tile == TileType::Files;
    let border_color = if is_active {
        Color::Cyan
    } else {
        Color::White
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Files: {} ", app.file_state.current_path.display()))
        .border_style(Style::default().fg(border_color));

    let items: Vec<ListItem> = app.file_state.files.iter().enumerate().map(|(i, path)| {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
        let style = if path.is_dir() {
            Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        
        let prefix = if i == app.file_state.selected_index && is_active {
            "> "
        } else {
            "  "
        };

        ListItem::new(format!("{}{}", prefix, name)).style(style)
    }).collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_system_tile(f: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_tile == TileType::System;
    let border_color = if is_active {
        Color::Cyan
    } else {
        Color::White
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" System ")
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.system_state.total_mem > 0.0 {
        let text = vec![
            format!("CPU: {:>5.1}%", app.system_state.cpu_usage),
            format!("MEM: {:>5.1} / {:.1} GB ({:.1}%)", 
                app.system_state.mem_usage, 
                app.system_state.total_mem,
                (app.system_state.mem_usage / app.system_state.total_mem) * 100.0
            ),
        ];

        let paragraph = Paragraph::new(text.join("\n"));
        f.render_widget(paragraph, inner);
    }
}

fn draw_docker_tile(f: &mut Frame, area: Rect, app: &App) {
    let is_active = app.active_tile == TileType::Docker;
    let border_color = if is_active {
        Color::Cyan
    } else {
        Color::White
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Docker ")
        .border_style(Style::default().fg(border_color));

    let items: Vec<ListItem> = app.docker_state.containers.iter().enumerate().map(|(i, name)| {
        let prefix = if i == app.docker_state.selected_index && is_active {
            "> "
        } else {
            "  "
        };
        ListItem::new(format!("{}{}", prefix, name))
    }).collect();

    let list = List::new(items).block(block);
    f.render_widget(list, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let text = match &app.license {
        LicenseStatus::FreeMode => {
            " Tiles Free Edition (<5 employees). Support us at dracon.uk ".to_string()
        }
        LicenseStatus::Commercial(company) => {
            format!(" Licensed to {} ", company)
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
