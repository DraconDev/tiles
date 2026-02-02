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
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(inner);

    let nav_area = chunks[0].inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    let nav_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(50)])
        .split(nav_area);

    let subviews = [
        (MonitorSubview::Overview, "󰊚 OVERVIEW"),
        (MonitorSubview::Applications, "󰀻 APPLICATIONS"),
        (MonitorSubview::Processes, "󰑮 PROCESSES"),
    ];

    app.monitor_subview_bounds.clear();
    let mut cur_x = nav_layout[0].x;
    for (view, name) in subviews {
        let is_active = app.monitor_subview == view;
        let width = name.chars().count() as u16 + 4;
        let rect = Rect::new(cur_x, nav_layout[0].y, width, 1);

        let mut style = if is_active {
            Style::default()
                .bg(THEME.accent_primary)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Rgb(60, 65, 75))
        };
        if app.mouse_pos.1 == nav_layout[0].y
            && app.mouse_pos.0 >= rect.x
            && app.mouse_pos.0 < rect.x + rect.width
        {
            style = style.fg(Color::White);
        }

        f.render_widget(Paragraph::new(name).style(style), rect);
        if is_active {
            f.render_widget(
                Paragraph::new("━━━━").style(Style::default().fg(THEME.accent_primary)),
                Rect::new(rect.x, rect.y + 1, 4, 1),
            );
        }

        app.monitor_subview_bounds.push((rect, view));
        cur_x += width + 2;
    }

    if app.monitor_subview != MonitorSubview::Overview {
        let search_style = if app.process_search_filter.is_empty() {
            Style::default().fg(Color::Rgb(40, 45, 55))
        } else {
            Style::default().fg(THEME.accent_primary)
        };
        f.render_widget(
            Paragraph::new(format!(" 󰍉 {}", app.process_search_filter)).style(search_style),
            nav_layout[1],
        );
    }

    let content_area = chunks[1].inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    match app.monitor_subview {
        MonitorSubview::Overview => draw_monitor_overview(f, content_area, app),
        MonitorSubview::Processes => draw_processes_view(f, content_area, app),
        MonitorSubview::Applications => draw_monitor_applications(f, content_area, app),
    }
}

fn draw_monitor_overview(f: &mut Frame, area: Rect, app: &mut App) {
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(7), Constraint::Fill(3)])
        .split(area.inner(ratatui::layout::Margin {
            horizontal: 1,
            vertical: 1,
        }));

    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Instant Telemetry Banks
            Constraint::Min(0),    // Flux Rack (Cores)
        ])
        .split(main_layout[0]);

    // --- 1. TELEMETRY BANKS (Instant Data, Wireframe) ---
    let bank_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Fill(1),
        ])
        .split(left_chunks[0]);

    let draw_telemetry_bank =
        |f: &mut Frame, area: Rect, label: &str, cur: f32, total: f32, unit: &str| {
            let inner = area.inner(ratatui::layout::Margin {
                horizontal: 1,
                vertical: 0,
            });
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Header
                    Constraint::Length(1), // Big Value
                    Constraint::Length(1), // Pipe Gauge
                ])
                .split(inner);

            // Header: "SYS // CPU"
            f.render_widget(
                Paragraph::new(Span::styled(
                    format!("SYS // {}", label),
                    Style::default()
                        .fg(Color::Rgb(80, 85, 95))
                        .add_modifier(Modifier::BOLD),
                )),
                chunks[0],
            );

            // Big Value: "12.5 %"
            let val_str = format!("{:.1}", cur);
            let total_str = if total > 0.0 {
                format!("/ {:.0}", total)
            } else {
                String::new()
            };

            let ratio = (cur / if total > 0.0 { total } else { 100.0 }).clamp(0.0, 1.0);
            let color = if ratio > 0.85 {
                Color::Rgb(255, 60, 60)
            } else if ratio > 0.5 {
                Color::Rgb(255, 180, 0)
            } else {
                THEME.accent_secondary
            };

            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        val_str,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!(" {}{}", unit, total_str),
                        Style::default().fg(Color::Rgb(100, 100, 110)),
                    ),
                ])),
                chunks[1],
            );

            // Wireframe Pipe Gauge: "││││││············"
            let gauge_w = chunks[2].width as usize;
            let filled = (ratio * gauge_w as f32) as usize;
            let pipe_gauge = format!(
                "{}{}",
                "│".repeat(filled),
                "·".repeat(gauge_w.saturating_sub(filled))
            );

            f.render_widget(
                Paragraph::new(Span::styled(pipe_gauge, Style::default().fg(color))),
                chunks[2],
            );

            // Separator
            f.render_widget(
                Block::default()
                    .borders(Borders::RIGHT)
                    .border_style(Style::default().fg(Color::Rgb(30, 30, 35))),
                area,
            );
        };

    draw_telemetry_bank(
        f,
        bank_layout[0],
        "CPU",
        app.system_state.cpu_usage,
        0.0,
        "%",
    );
    draw_telemetry_bank(
        f,
        bank_layout[1],
        "MEM",
        app.system_state.mem_usage as f32,
        app.system_state.total_mem as f32,
        "GB",
    );
    draw_telemetry_bank(
        f,
        bank_layout[2],
        "SWAP",
        app.system_state.swap_usage as f32,
        app.system_state.total_swap as f32,
        "GB",
    );

    // --- 2. FLUX RACK (Core Grid) ---
    let rack_area = left_chunks[1].inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 1,
    });
    let core_count = app.system_state.cpu_cores.len();
    if core_count > 0 {
        f.render_widget(
            Paragraph::new(Span::styled(
                "RACK // THREAD_FLUX",
                Style::default()
                    .fg(Color::Rgb(60, 65, 75))
                    .add_modifier(Modifier::BOLD),
            )),
            Rect::new(rack_area.x, rack_area.y - 1, 30, 1),
        );

        let cols = if core_count > 16 {
            4
        } else if core_count > 8 {
            2
        } else {
            1
        };
        let rows = (core_count as f32 / cols as f32).ceil() as u16;

        let rack_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(vec![Constraint::Length(1); rows as usize])
            .split(rack_area);

        for r in 0..rows {
            if r as usize >= rack_rows.len() {
                break;
            }
            let core_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(vec![Constraint::Fill(1); cols as usize])
                .split(rack_rows[r as usize]);

            for c in 0..cols {
                let idx = (r * cols + c) as usize;
                if idx < core_count {
                    let usage = app.system_state.cpu_cores[idx];
                    let intensity = usage / 100.0;
                    let color = if intensity > 0.9 {
                        Color::Rgb(255, 60, 60)
                    } else if intensity > 0.5 {
                        Color::Rgb(255, 180, 0)
                    } else {
                        THEME.accent_secondary
                    };

                    let slot = core_cols[c as usize].inner(ratatui::layout::Margin {
                        horizontal: 1,
                        vertical: 0,
                    });

                    let track_w: usize = slot.width.saturating_sub(14).into();
                    let pos = (intensity * track_w as f32) as usize;
                    let track = format!(
                        "{}{}{}",
                        "─".repeat(pos),
                        "┼",
                        "─".repeat(track_w.saturating_sub(pos))
                    );

                    f.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::styled(
                                format!("0x{:02X} ", idx),
                                Style::default().fg(Color::Rgb(50, 55, 65)),
                            ),
                            Span::styled("╾", Style::default().fg(Color::Rgb(40, 40, 45))),
                            Span::styled(track, Style::default().fg(color)),
                            Span::styled("╼", Style::default().fg(Color::Rgb(40, 40, 45))),
                            Span::styled(
                                format!(" {:>3.0}%", usage),
                                Style::default().fg(if intensity > 0.1 {
                                    Color::White
                                } else {
                                    Color::Rgb(60, 65, 75)
                                }),
                            ),
                        ])),
                        slot,
                    );
                }
            }
        }
    }

    // --- 3. I/O STREAM SIDEBAR ---
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Identity
            Constraint::Length(8), // Network Stream
            Constraint::Min(0),    // Storage Arrays
        ])
        .split(main_layout[1]);

    // Identity
    let id_info = vec![
        Line::from(vec![
            Span::styled("ID  ", Style::default().fg(Color::Rgb(60, 65, 75))),
            Span::styled(
                &app.system_state.hostname,
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("UP  ", Style::default().fg(Color::Rgb(60, 65, 75))),
            Span::raw(format!(
                "{}d {}h",
                app.system_state.uptime / 86400,
                (app.system_state.uptime % 86400) / 3600
            )),
        ]),
        Line::from(vec![
            Span::styled("KER ", Style::default().fg(Color::Rgb(60, 65, 75))),
            Span::raw(&app.system_state.kernel_version),
        ]),
        Line::from(vec![
            Span::styled("OS  ", Style::default().fg(Color::Rgb(60, 65, 75))),
            Span::raw(&app.system_state.os_name),
        ]),
    ];
    f.render_widget(
        Paragraph::new(id_info).block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(Color::Rgb(30, 30, 35))),
        ),
        right_chunks[0],
    );

    // Network Stream
    let rx = app.system_state.net_in_history.last().cloned().unwrap_or(0);
    let tx = app
        .system_state
        .net_out_history
        .last()
        .cloned()
        .unwrap_or(0);

    let net_lines = vec![
        Line::from(Span::styled(
            "NET // STREAM",
            Style::default()
                .fg(Color::Rgb(60, 65, 75))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("RX ▼ ", Style::default().fg(THEME.accent_secondary)),
            Span::styled(
                format_size(rx),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("TX ▲ ", Style::default().fg(THEME.accent_primary)),
            Span::styled(
                format_size(tx),
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    f.render_widget(
        Paragraph::new(net_lines).block(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(Color::Rgb(30, 30, 35))),
        ),
        right_chunks[1],
    );

    // Storage Arrays
    let disk_list: Vec<ListItem> = app
        .system_state
        .disks
        .iter()
        .map(|disk| {
            let ratio = (disk.used_space as f64 / disk.total_space as f64).clamp(0.0, 1.0);
            let color = if ratio > 0.9 {
                Color::Rgb(255, 60, 60)
            } else if ratio > 0.7 {
                Color::Rgb(255, 180, 0)
            } else {
                THEME.accent_secondary
            };

            let track_w: usize = 12;
            let pos = (ratio * track_w as f64) as usize;
            let track = format!(
                "[{}|{}]",
                "-".repeat(pos),
                "·".repeat(track_w.saturating_sub(pos))
            );

            ListItem::new(vec![
                Line::from(vec![
                    Span::styled("DSK ", Style::default().fg(Color::Rgb(60, 65, 75))),
                    Span::styled(&disk.name, Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled(track, Style::default().fg(color)),
                    Span::styled(
                        format!(" {:.0}%", ratio * 100.0),
                        Style::default().fg(Color::Rgb(100, 100, 110)),
                    ),
                ]),
                Line::from(""),
            ])
        })
        .collect();

    f.render_widget(
        List::new(disk_list).block(
            Block::default()
                .title(Span::styled(
                    "STO // ARRAY",
                    Style::default()
                        .fg(Color::Rgb(60, 65, 75))
                        .add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::LEFT)
                .border_style(Style::default().fg(Color::Rgb(30, 30, 35))),
        ),
        right_chunks[2],
    );
}

fn draw_monitor_applications(f: &mut Frame, area: Rect, app: &mut App) {
    let current_user = std::env::var("USER").unwrap_or_else(|_| "dracon".to_string());
    let app_procs: Vec<_> = app
        .system_state
        .processes
        .iter()
        .filter(|p| {
            let matches = if app.process_search_filter.is_empty() {
                true
            } else {
                p.name
                    .to_lowercase()
                    .contains(&app.process_search_filter.to_lowercase())
            };
            p.user == current_user
                && !p.name.starts_with('[')
                && !p.name.contains("kworker")
                && matches
        })
        .collect();

    let rows = app_procs.iter().enumerate().map(|(i, p)| {
        let mut is_selected = false;
        let mut style = if i % 2 == 0 {
            Style::default().fg(Color::Rgb(180, 185, 190))
        } else {
            Style::default().fg(Color::Rgb(140, 145, 150))
        };
        if app.process_selected_idx == Some(i)
            && app.monitor_subview == MonitorSubview::Applications
        {
            style = style
                .bg(THEME.accent_primary)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD);
            is_selected = true;
        }
        let cpu_color = if is_selected {
            Color::Black
        } else if p.cpu > 50.0 {
            Color::Red
        } else {
            THEME.accent_secondary
        };
        Row::new(vec![
            Cell::from(format!("  {}", p.name)),
            Cell::from(format!("{:.1}%", p.cpu)).style(Style::default().fg(cpu_color)),
            Cell::from(format!("{:.1} MB", p.mem)),
            Cell::from(p.pid.to_string()).style(Style::default().fg(if is_selected {
                Color::Black
            } else {
                Color::Rgb(60, 65, 75)
            })),
            Cell::from(p.status.clone()),
        ])
        .style(style)
    });
    let column_constraints = [
        Constraint::Min(35),
        Constraint::Length(10),
        Constraint::Length(15),
        Constraint::Length(10),
        Constraint::Length(15),
    ];
    let num_cols = 5;
    let spacing = 2;
    let total_spacing = (num_cols - 1) * spacing;
    let effective_width = area.width.saturating_sub(total_spacing);

    let header_rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(column_constraints.clone())
        .split(Rect::new(area.x, area.y, effective_width, 1));

    app.process_column_bounds.clear();
    let mut current_col_x = area.x;
    let header_cells = [
        ("  Application", ProcessColumn::Name),
        ("CPU", ProcessColumn::Cpu),
        ("Memory", ProcessColumn::Mem),
        ("PID", ProcessColumn::Pid),
        ("Status", ProcessColumn::Status),
    ]
    .iter()
    .enumerate()
    .map(|(i, (h, col))| {
        let width = header_rects[i].width;
        app.process_column_bounds
            .push((Rect::new(current_col_x, area.y, width, 1), *col));
        current_col_x += width + spacing;
        let mut text = h.to_string();
        if app.process_sort_col == *col {
            text.push_str(if app.process_sort_asc {
                " 󰁝"
            } else {
                " 󰁅"
            });
        }
        Cell::from(text).style(
            Style::default()
                .fg(if app.process_sort_col == *col {
                    THEME.accent_primary
                } else {
                    Color::Rgb(60, 65, 75)
                })
                .add_modifier(Modifier::BOLD),
        )
    });

    f.render_widget(
        Table::new(rows, column_constraints)
            .header(Row::new(header_cells).height(1).bottom_margin(1))
            .column_spacing(2),
        area,
    );
}

fn draw_processes_view(f: &mut Frame, area: Rect, app: &mut App) {
    let column_constraints = [
        Constraint::Length(8),
        Constraint::Min(25),
        Constraint::Length(15),
        Constraint::Length(12),
        Constraint::Length(10),
        Constraint::Length(10),
    ];
    let num_cols = 6;
    let spacing = 2;
    let total_spacing = (num_cols - 1) * spacing;
    let effective_width = area.width.saturating_sub(total_spacing);

    app.process_column_bounds.clear();
    let header_rects = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(column_constraints.clone())
        .split(Rect::new(area.x, area.y, effective_width, 1));
    let mut current_col_x = area.x;
    let header_cells = ["PID", "NAME", "USER", "STATUS", "CPU%", "MEM%"]
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let col = match *h {
                "PID" => ProcessColumn::Pid,
                "NAME" => ProcessColumn::Name,
                "USER" => ProcessColumn::User,
                "STATUS" => ProcessColumn::Status,
                "CPU%" => ProcessColumn::Cpu,
                "MEM%" => ProcessColumn::Mem,
                _ => ProcessColumn::Pid,
            };
            let width = header_rects[i].width;
            app.process_column_bounds
                .push((Rect::new(current_col_x, area.y, width, 1), col));
            current_col_x += width + spacing;
            let mut text = h.to_string();
            if app.process_sort_col == col {
                text.push_str(if app.process_sort_asc {
                    " 󰁝"
                } else {
                    " 󰁅"
                });
            }
            Cell::from(text).style(
                Style::default()
                    .fg(if app.process_sort_col == col {
                        THEME.accent_primary
                    } else {
                        Color::Rgb(60, 65, 75)
                    })
                    .add_modifier(Modifier::BOLD),
            )
        });
    let rows = app.system_state.processes.iter().enumerate().map(|(i, p)| {
        let mut is_selected = false;
        let mut style = if i % 2 == 0 {
            Style::default().fg(Color::Rgb(180, 185, 190))
        } else {
            Style::default().fg(Color::Rgb(140, 145, 150))
        };
        if app.process_selected_idx == Some(i) && app.monitor_subview == MonitorSubview::Processes {
            style = style
                .bg(THEME.accent_primary)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD);
            is_selected = true;
        }
        let cpu_color = if is_selected {
            Color::Black
        } else if p.cpu > 50.0 {
            Color::Red
        } else {
            THEME.accent_secondary
        };
        Row::new(vec![
            Cell::from(format!("  {}", p.pid)).style(Style::default().fg(if is_selected {
                Color::Black
            } else {
                Color::Rgb(60, 65, 75)
            })),
            Cell::from(p.name.clone()).style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from(p.user.clone()).style(Style::default().fg(if is_selected {
                Color::Black
            } else {
                THEME.accent_primary
            })),
            Cell::from(p.status.clone()),
            Cell::from(format!("{:.1}", p.cpu)).style(Style::default().fg(cpu_color)),
            Cell::from(format!("{:.1}", p.mem)),
        ])
        .style(style)
    });
    f.render_stateful_widget(
        Table::new(rows, column_constraints)
            .header(Row::new(header_cells).height(1).bottom_margin(1))
            .column_spacing(1),
        area,
        &mut app.process_table_state,
    );
}
