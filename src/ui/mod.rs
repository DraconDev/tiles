use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
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
        f.render_widget(block, area);
        // TODO: Render zoomed tile content
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
    draw_tile(f, main_chunks[0], " Files ", app.active_tile == TileType::Files);
    
    // System Tile
    draw_tile(f, right_chunks[0], " System ", app.active_tile == TileType::System);

    // Docker Tile
    draw_tile(f, right_chunks[1], " Docker ", app.active_tile == TileType::Docker);
}

fn draw_tile(f: &mut Frame, area: Rect, title: &str, is_active: bool) {
    let border_color = if is_active {
        Color::Cyan
    } else {
        Color::White
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(border_color));
    
    f.render_widget(block, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let text = match &app.license {
        LicenseStatus::FreeMode => {
            " Tiles Free Edition (<5 employees). Support us at dracon.uk "
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
