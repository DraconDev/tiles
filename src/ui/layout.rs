use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Borders, Paragraph,
    },
    Frame,
};
use std::time::SystemTime;
use unicode_width::UnicodeWidthStr;

use crate::app::{
    App, AppMode, CurrentView, DropTarget,
};
use crate::ui::theme::THEME;
use terma::widgets::HotkeyHint;

pub fn draw_global_header(f: &mut Frame, area: Rect, _sw: u16, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(25), // Title/Breadcrumbs start
            Constraint::Fill(1),    // Fill
            Constraint::Length(35), // Icons/Stats
        ])
        .split(area);

    // Title
    let title = Line::from(vec![
        Span::styled(" TILES ", Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(format!(" v{}", env!("CARGO_PKG_VERSION")), Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(title), chunks[0]);

    // Icons
    draw_header_icons(f, chunks[2], app);
}

pub fn draw_header_icons(f: &mut Frame, area: Rect, app: &mut App) {
    app.header_icon_bounds.clear();
    let mut current_x = area.x + area.width;
    
    let icons = vec![
        ("burger", "󰍜 "),
        ("monitor", "󰢮 "),
        ("git", "󰊢 "),
        ("project", "󰙅 "),
        ("split", "󰝤 "),
        ("forward", "󰁔 "),
        ("back", "󰁍 "),
    ];

    for (id, symbol) in icons {
        let width = symbol.width() as u16;
        current_x -= width + 1;
        let icon_area = Rect::new(current_x, area.y, width, 1);
        
        let is_hovered = app.hovered_header_icon.as_deref() == Some(id);
        let style = if is_hovered {
            Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        f.render_widget(Paragraph::new(symbol).style(style), icon_area);
        app.header_icon_bounds.push((icon_area, id.to_string()));
    }
}

pub fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
    match app.current_view {
        CurrentView::Files => {
            let pc = app.panes.len();
            let pw = if pc > 0 { area.width / pc as u16 } else { area.width };
            
            for i in 0..pc {
                let pane_area = Rect::new(
                    area.x + (i as u16 * pw),
                    area.y,
                    pw,
                    area.height,
                );
                crate::ui::panes::files::draw_pane(f, pane_area, i, app);
            }
        }
        CurrentView::Editor => {
            crate::ui::panes::editor::draw_ide_editor(f, area, app);
        }
        _ => {}
    }
}

pub fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    // Row 1: Hotkeys
    let mut hints = Vec::new();
    match app.current_view {
        CurrentView::Files => {
            hints.extend(HotkeyHint::new("F1", "Help", Color::Cyan));
            hints.extend(HotkeyHint::new("F2", "Rename", Color::Yellow));
            hints.extend(HotkeyHint::new("F3", "Select", Color::Magenta));
            hints.extend(HotkeyHint::new("Space", "Preview/Edit", THEME.accent_primary));
            hints.extend(HotkeyHint::new("^N", "Terminal", Color::Green));
            hints.extend(HotkeyHint::new("^P", "Split", Color::Blue));
        }
        CurrentView::Editor => {
            hints.extend(HotkeyHint::new("Esc", "Back", Color::Red));
            hints.extend(HotkeyHint::new("^S", "Save", Color::Green));
            hints.extend(HotkeyHint::new("^F", "Find", Color::Cyan));
            hints.extend(HotkeyHint::new("^G", "Line", Color::Yellow));
        }
        _ => {}
    }
    
    if let Some((msg, time)) = &app.last_action_msg {
        if time.elapsed().as_secs() < 3 {
            hints.push(Span::raw(" | "));
            hints.push(Span::styled(msg, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
        }
    }

    f.render_widget(Paragraph::new(Line::from(hints)), chunks[0]);

    // Row 2: Stats / System
    let sys = &app.system_state;
    let stats = Line::from(vec![
        Span::styled(format!(" 󰻠 {:.0}% ", sys.cpu_usage), Style::default().fg(Color::Yellow)),
        Span::styled(format!(" 󰍛 {:.0}% ", sys.mem_usage), Style::default().fg(Color::Cyan)),
        Span::raw(" | "),
        Span::styled(format!(" 󰖩 {} ", sys.hostname), Style::default().fg(Color::DarkGray)),
        Span::styled(format!(" 󰅐 {} ", format_uptime(sys.uptime)), Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(stats).alignment(ratatui::layout::Alignment::Right), chunks[1]);
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;
    if days > 0 { format!("{}d {}h", days, hours) }
    else if hours > 0 { format!("{}h {}m", hours, mins) }
    else { format!("{}m", mins) }
}