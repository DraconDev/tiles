use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, List, ListItem, Paragraph, Row, Table,
    },
    Frame,
};

use crate::app::{
    App, MonitorSubview, ProcessColumn,
};
use crate::ui::theme::THEME;
use terma::utils::{format_size};

pub fn draw_monitor_page(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .title_top(Line::from(vec![
            Span::styled(" SYSTEM MONITOR ", Style::default().fg(Color::Black).bg(THEME.accent_primary).add_modifier(Modifier::BOLD)),
        ]))
        .title_top(Line::from(vec![
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Back ", Style::default().fg(Color::Red)),
        ]).alignment(Alignment::Right))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_primary));
    
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Subview Tabs
            Constraint::Fill(1),   // Content
        ])
        .split(inner);

    draw_subview_tabs(f, chunks[0], app);

    match app.monitor_subview {
        MonitorSubview::Overview => draw_overview(f, chunks[1], app),
        MonitorSubview::Processes => draw_processes(f, chunks[1], app),
        _ => {
            f.render_widget(Paragraph::new("Subview Not Implemented Yet").alignment(Alignment::Center), chunks[1]);
        }
    }
}

fn draw_subview_tabs(f: &mut Frame, area: Rect, app: &mut App) {
    let subviews = vec![
        (MonitorSubview::Overview, "󰍛 Overview"),
        (MonitorSubview::Cpu, "󰻠 CPU"),
        (MonitorSubview::Memory, "󰍛 RAM"),
        (MonitorSubview::Disk, "󰋊 Disk"),
        (MonitorSubview::Network, "󰖩 Net"),
        (MonitorSubview::Processes, "󰢮 Tasks"),
    ];

    app.monitor_subview_bounds.clear();
    let mut x = area.x;
    for (sv, label) in subviews {
        let width = label.chars().count() as u16 + 4;
        let tab_area = Rect::new(x, area.y, width, 1);
        app.monitor_subview_bounds.push((tab_area, sv));

        let style = if app.monitor_subview == sv {
            Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        f.render_widget(Paragraph::new(format!(" {} ", label)).style(style), tab_area);
        x += width + 1;
    }
}

fn draw_overview(f: &mut Frame, area: Rect, app: &App) {
    let sys = &app.system_state;
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

    // Left: CPU & RAM
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // CPU
            Constraint::Length(3), // MEM
            Constraint::Length(3), // SWAP
            Constraint::Fill(1),
        ])
        .split(chunks[0]);

    f.render_widget(Paragraph::new(terma::utils::draw_stat_bar("CPU ", sys.cpu_usage, 100.0, left_chunks[0].width, Color::White)), left_chunks[0]);
    f.render_widget(Paragraph::new(terma::utils::draw_stat_bar("MEM ", sys.mem_usage, 100.0, left_chunks[1].width, Color::White)), left_chunks[1]);
    f.render_widget(Paragraph::new(terma::utils::draw_stat_bar("SWP ", sys.swap_usage, 100.0, left_chunks[2].width, Color::White)), left_chunks[2]);

    // Right: System Info
    let info = vec![
        Line::from(vec![Span::styled("Host:     ", Style::default().fg(THEME.accent_secondary)), Span::raw(&sys.hostname)]),
        Line::from(vec![Span::styled("OS:       ", Style::default().fg(THEME.accent_secondary)), Span::raw(format!("{} {}", sys.os_name, sys.os_version))]),
        Line::from(vec![Span::styled("Kernel:   ", Style::default().fg(THEME.accent_secondary)), Span::raw(&sys.kernel_version)]),
        Line::from(vec![Span::styled("Uptime:   ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_uptime(sys.uptime))]),
    ];
    f.render_widget(Paragraph::new(info).block(Block::default().title(" System Info ").borders(Borders::LEFT).border_style(Style::default().fg(Color::DarkGray))), chunks[1]);
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;
    if days > 0 { format!("{}d {}h {}m", days, hours, mins) }
    else if hours > 0 { format!("{}h {}m", hours, mins) }
    else { format!("{}m", mins) }
}

fn draw_processes(f: &mut Frame, area: Rect, app: &mut App) {
    let sys = &app.system_state;
    
    let header_cells = ["PID", "NAME", "CPU%", "MEM%", "USER", "STATUS"]
        .iter()
        .map(|h| Cell::from(Span::styled(*h, Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD))));
    let header = Row::new(header_cells).height(1).bottom_margin(0);

    let rows = sys.processes.iter().map(|p| {
        let cells = vec![
            Cell::from(p.pid.to_string()),
            Cell::from(p.name.clone()),
            Cell::from(format!("{:.1}", p.cpu)),
            Cell::from(format_size(p.mem as u64)),
            Cell::from(p.user.clone()),
            Cell::from(p.status.clone()),
        ];
        Row::new(cells).style(Style::default().fg(THEME.fg))
    });

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(10),
        ]
    )
    .header(header)
    .highlight_style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD))
    .column_spacing(1);

    f.render_stateful_widget(table, area, &mut app.process_table_state);
}