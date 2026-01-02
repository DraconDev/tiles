pub mod theme;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Gauge, List, ListItem, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table,
    },
    Frame,
};

use crate::app::{App, AppMode, CurrentView, FileColumn};
use crate::ui::theme::THEME;
use terma::compositor::engine::TilePlacement;
use terma::visuals::assets::Icon;
use terma::widgets::{TermaButton, TermaPanel};

fn draw_tabs(f: &mut Frame, area: Rect, app: &App) {
    let tile_queue = app.tile_queue.clone();

    if area.width > 0 && area.height > 0 {
        if let Ok(mut q) = tile_queue.lock() {
            q.push(TilePlacement {
                asset_id: 1002,
                is_image: false,
                x: area.x,
                y: area.y,
                z_index: 0,
                cols: Some(area.width),
                rows: Some(area.height),
                placement_id: Some(3),
            });
        }
    }

    let mut current_x = area.x;
    let views = vec![
        ("^F Files", CurrentView::Files),
        ("^P Proc", CurrentView::System),
    ];
    for (label, view) in views {
        let width = (label.len() + 4) as u16;
        let tab_area = Rect::new(current_x, area.y, width, 1);
        f.render_widget(
            TermaButton::new(label, app.current_view == view, app.tile_queue.clone()),
            tab_area,
        );
        current_x += width + 1;
    }

    let mut current_x_files = current_x;
    let sep = " | ";
    f.render_widget(
        Paragraph::new(sep),
        Rect::new(current_x_files, area.y, sep.len() as u16, 1),
    );
    current_x_files += sep.len() as u16;

    if app.current_view == CurrentView::Files {
        for (i, tab) in app.file_tabs.iter().enumerate() {
            let name = tab
                .current_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "/".to_string());
            let style = if i == app.tab_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(Color::Gray)
            };
            let label = format!(
                "[{}]
",
                name
            );
            let width = label.len() as u16;
            f.render_widget(
                Paragraph::new(ratatui::text::Span::styled(label, style)),
                Rect::new(current_x_files, area.y, width, 1),
            );
            current_x_files += width + 1;
        }
    }
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .title(" Sidebar ")
        .border_style(
            if app.sidebar_focus && app.current_view == CurrentView::Files {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            },
        );
    f.render_widget(block, area);

    let tile_queue = app.tile_queue.clone();

    if area.width > 0 && area.height > 0 {
        // Background Gradient
        let tile = TilePlacement {
            asset_id: 2001, // Sidebar Gradient
            is_image: true,
            x: area.x,
            y: area.y,
            z_index: 0,
            cols: Some(area.width),
            rows: Some(area.height),
            placement_id: Some(2),
        };
        if let Ok(mut queue) = tile_queue.lock() {
            queue.push(tile);
        }
    }

    if area.width > 10 && area.height > 5 {
        let tile = TilePlacement {
            asset_id: 1000,
            is_image: false,
            x: area.x + area.width.saturating_sub(10),
            y: area.y + 1,
            z_index: 2,
            cols: Some(8),
            rows: Some(4),
            placement_id: Some(1),
        };
        if let Ok(mut queue) = tile_queue.lock() {
            queue.push(tile);
        }
    }

    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    match app.current_view {
        CurrentView::Files => {
            let mut sidebar_items = vec![
                ListItem::new("   Local")
                    .style(Style::default().add_modifier(Modifier::UNDERLINED)),
                ListItem::new("     Home"),
                ListItem::new("     Downloads"),
                ListItem::new("     Documents"),
                ListItem::new("     Pictures"),
                ListItem::new(""),
                ListItem::new("   Remote")
                    .style(Style::default().add_modifier(Modifier::UNDERLINED)),
            ];

            if let Ok(mut q) = tile_queue.lock() {
                q.push(TilePlacement {
                    asset_id: Icon::Folder as u32,
                    is_image: true,
                    x: inner.x,
                    y: inner.y,
                    z_index: 2,
                    cols: Some(2),
                    rows: Some(1),
                    placement_id: Some(6000),
                });
                for i in 0..4 {
                    q.push(TilePlacement {
                        asset_id: Icon::Settings as u32,
                        is_image: true,
                        x: inner.x + 2,
                        y: inner.y + 1 + i as u16,
                        z_index: 2,
                        cols: Some(2),
                        rows: Some(1),
                        placement_id: Some(6001 + i),
                    });
                }
                q.push(TilePlacement {
                    asset_id: Icon::Demon as u32,
                    is_image: true,
                    x: inner.x,
                    y: inner.y + 6,
                    z_index: 2,
                    cols: Some(2),
                    rows: Some(1),
                    placement_id: Some(6010),
                });
            }

            for bookmark in &app.remote_bookmarks {
                sidebar_items.push(ListItem::new(format!("     {}", bookmark.name)));
            }

            if app.remote_bookmarks.is_empty() {
                sidebar_items.push(
                    ListItem::new("     (No remotes)").style(Style::default().fg(Color::DarkGray)),
                );
            }

            let items: Vec<ListItem> = sidebar_items
                .into_iter()
                .enumerate()
                .map(|(i, item): (usize, ListItem)| {
                    if i == app.sidebar_index + 1 && app.sidebar_focus {
                        item.clone().style(
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        item.clone()
                    }
                })
                .collect();

            f.render_widget(List::new(items), inner);
        }
        _ => {}
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(f.area());

    draw_tabs(f, chunks[0], app);

    let workspace = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Min(0)])
        .split(chunks[1]);

    draw_sidebar(f, workspace[0], app);
    draw_main_stage(f, workspace[1], app);

    draw_footer(f, chunks[2], app);

    if let AppMode::ContextMenu { x, y, item_index } = app.mode {
        draw_context_menu(f, x, y, item_index, app);
    }

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
    if matches!(app.mode, AppMode::NewFile) {
        draw_new_file_modal(f, app);
    }
    if matches!(app.mode, AppMode::ColumnSetup) {
        draw_column_setup_modal(f, app);
    }
    if matches!(app.mode, AppMode::CommandPalette) {
        draw_command_palette(f, app);
    }
    if matches!(app.mode, AppMode::AddRemote) {
        draw_add_remote_modal(f, app);
    }
}

fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
    if app.current_view == CurrentView::Files {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);
        let path_text = if let Some(fs) = app.current_file_state() {
            if !fs.search_filter.is_empty() {
                format!("Search: {} (Esc to clear)", fs.search_filter)
            } else {
                format!("Path: {}", fs.current_path.display())
            }
        } else {
            String::new()
        };
        let path_style = if app
            .current_file_state()
            .map(|s| !s.search_filter.is_empty())
            .unwrap_or(false)
        {
            Style::default().fg(Color::Magenta)
        } else {
            Style::default()
        };
        f.render_widget(
            Paragraph::new(path_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
                    .border_style(path_style),
            ),
            chunks[0],
        );

        draw_file_view(f, chunks[1], app);
    } else {
        match app.current_view {
            CurrentView::System => draw_system_view(f, area, app),

            _ => {}
        }
    }
}

use std::time::SystemTime;

fn draw_file_view(f: &mut Frame, area: Rect, app: &mut App) {
    let sidebar_focus = app.sidebar_focus;
    let tile_queue = app.tile_queue.clone();

    if let Some(file_state) = app.current_file_state_mut() {
        file_state.view_height = area.height as usize;
        let mut render_state = ratatui::widgets::TableState::default();
        if let Some(sel) = file_state.selected_index {
            let offset = file_state.table_state.offset();
            // Capacity = Height - 2 (Borders) - 1 (Header)
            let capacity = file_state.view_height.saturating_sub(3);

            // CRITICAL FIX: Only tell Ratatui to select the row if it is PHYSICALLY visible
            // based on our manual offset. Otherwise, Ratatui will auto-scroll the offset
            // to show the selection, fighting our manual scroll logic in main.rs.
            if sel >= offset && sel < offset + capacity {
                render_state.select(Some(sel));
            } else {
                render_state.select(None);
            }
        }
        // Force the render state offset to match our manual offset
        *render_state.offset_mut() = file_state.table_state.offset();

        let header_cells = file_state.columns.iter().map(|c| {
            let name = match c {
                FileColumn::Name => "Name",
                FileColumn::Size => "Size",
                FileColumn::Modified => "Modified",
                FileColumn::Created => "Created",
                FileColumn::Permissions => "Permissions",
                FileColumn::Extension => "Ext",
            };
            Cell::from(name).style(
                Style::default()
                    .fg(THEME.header_fg)
                    .add_modifier(Modifier::BOLD),
            )
        });
        let header = Row::new(header_cells).height(1).bottom_margin(0);

        let offset = file_state.table_state.offset();
        let rows = file_state.files.iter().enumerate().map(|(i, path)| {
            let metadata = file_state.metadata.get(path);
            let cells = file_state.columns.iter().map(|c| match c {
                FileColumn::Name => {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
                    let mut display_name = name.to_string();
                    let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
                    let mut style = if is_dir {
                        Style::default()
                            .fg(THEME.accent_secondary)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    };
                    if let Some(status) = file_state.git_status.get(path) {
                        display_name.push_str(&format!(" [{}]", status));
                        match status.as_str() {
                            "M" | "MM" => style = style.fg(Color::Yellow),
                            "A" | "AM" => style = style.fg(Color::Green),
                            "??" => style = style.fg(Color::DarkGray),
                            "D" => style = style.fg(Color::Red),
                            _ => {}
                        }
                    }
                    if file_state.starred.contains(path) {
                        display_name.push_str(" [*]");
                        style = style.fg(THEME.accent_primary).add_modifier(Modifier::BOLD);
                    }

                    // Icons removed per user request (was causing rendering issues/boxes)
                    // We rely purely on color and styling for differentiation now.
                    Cell::from(format!("  {}", display_name)).style(style)
                }
                FileColumn::Size => {
                    let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
                    if is_dir {
                        Cell::from("<DIR>").style(Style::default().fg(THEME.accent_secondary))
                    } else {
                        Cell::from(format_size(metadata.map(|m| m.size).unwrap_or(0)))
                            .style(Style::default().fg(THEME.fg))
                    }
                }
                FileColumn::Modified => Cell::from(format_time(
                    metadata
                        .map(|m| m.modified)
                        .unwrap_or(SystemTime::UNIX_EPOCH),
                ))
                .style(Style::default().fg(THEME.fg)),
                FileColumn::Created => Cell::from(format_time(
                    metadata
                        .map(|m| m.created)
                        .unwrap_or(SystemTime::UNIX_EPOCH),
                ))
                .style(Style::default().fg(THEME.fg)),
                FileColumn::Permissions => Cell::from(format_permissions(
                    metadata.map(|m| m.permissions).unwrap_or(0),
                ))
                .style(Style::default().fg(THEME.fg)),
                FileColumn::Extension => {
                    Cell::from(path.extension().and_then(|e| e.to_str()).unwrap_or(""))
                        .style(Style::default().fg(THEME.fg))
                }
            });
            let style = if Some(i) == file_state.selected_index && !sidebar_focus {
                Style::default()
                    .bg(THEME.selection_bg)
                    .fg(THEME.selection_fg)
            } else {
                Style::default()
            };
            Row::new(cells).style(style)
        });
        let constraints: Vec<Constraint> = file_state
            .columns
            .iter()
            .map(|c| match c {
                FileColumn::Name => Constraint::Percentage(50),
                FileColumn::Size => Constraint::Length(10),
                FileColumn::Modified => Constraint::Length(20),
                FileColumn::Created => Constraint::Length(20),
                FileColumn::Permissions => Constraint::Length(12),
                FileColumn::Extension => Constraint::Length(6),
            })
            .collect();
        let file_block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(THEME.accent_secondary));
        let table = Table::new(rows, constraints)
            .header(header)
            .block(file_block);
        f.render_stateful_widget(table, area, &mut render_state);

        // Scrollbar
        let list_height = area.height.saturating_sub(2) as usize;
        if file_state.files.len() > list_height {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_symbol(Some("│"))
                .thumb_symbol("█");
            let mut scrollbar_state = ScrollbarState::new(file_state.files.len())
                .position(file_state.table_state.offset());
            // Render inside the area, shifted by 1 for border (Right side)
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };
            f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }
}

fn draw_system_view(f: &mut Frame, area: Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Min(0),
        ])
        .split(area);

    // CPU
    let cpu_gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Green))
        .percent(app.system_state.cpu_usage as u16)
        .label(format!("{:.1}%", app.system_state.cpu_usage));
    let cpu_panel =
        TermaPanel::new(" CPU Usage ", app.tile_queue.clone()).border_color(THEME.border_inactive);
    let cpu_inner = cpu_panel.inner(layout[0]);
    f.render_widget(cpu_panel, layout[0]);
    f.render_widget(cpu_gauge, cpu_inner);

    // Memory
    if app.system_state.total_mem > 0.0 {
        let mem_gauge = Gauge::default()
            .gauge_style(Style::default().fg(Color::Yellow))
            .percent((app.system_state.mem_usage / app.system_state.total_mem * 100.0) as u16)
            .label(format!(
                "{:.1} / {:.1} GB",
                app.system_state.mem_usage, app.system_state.total_mem
            ));
        let mem_panel = TermaPanel::new(" Memory Usage ", app.tile_queue.clone())
            .border_color(THEME.border_inactive);
        let mem_inner = mem_panel.inner(layout[1]);
        f.render_widget(mem_panel, layout[1]);
        f.render_widget(mem_gauge, mem_inner);
    }

    // Disk
    let disk_items: Vec<ListItem> = app
        .system_state
        .disks
        .iter()
        .map(|disk| {
            let percent = (disk.used_space / disk.total_space) * 100.0;
            let bar_width: usize = 20;
            let filled = (percent / 100.0 * bar_width as f64) as usize;
            let bar = format!(
                "[{}{}]",
                "#".repeat(filled),
                "-".repeat(bar_width.saturating_sub(filled))
            );
            ListItem::new(format!(
                "{:<10} {}  {:.1} / {:.1} GB ({:.1}%)",
                disk.name, bar, disk.used_space, disk.total_space, percent
            ))
        })
        .collect();
    let disk_panel =
        TermaPanel::new(" Disk Usage ", app.tile_queue.clone()).border_color(THEME.border_inactive);
    let disk_inner = disk_panel.inner(layout[2]);
    f.render_widget(disk_panel, layout[2]);
    f.render_widget(List::new(disk_items), disk_inner);

    // Processes
    let process_items: Vec<ListItem> = app
        .system_state
        .processes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let style = if i == app.system_state.selected_process_index && !app.sidebar_focus {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(format!(
                "{:<6} {:<20} {:.1}%  {:.1} MB",
                p.pid,
                p.name.chars().take(20).collect::<String>(),
                p.cpu,
                p.mem as f64 / 1024.0 / 1024.0
            ))
            .style(style)
        })
        .collect();
    let proc_panel = TermaPanel::new(" Top Processes ", app.tile_queue.clone())
        .border_color(THEME.border_inactive);
    let proc_inner = proc_panel.inner(layout[3]);
    f.render_widget(proc_panel, layout[3]);
    f.render_widget(List::new(process_items), proc_inner);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();
    spans.push(ratatui::text::Span::styled(
        "^Q",
        Style::default().fg(Color::Yellow),
    ));
    spans.push(ratatui::text::Span::raw(" Quit | "));
    let console_style = if matches!(app.mode, AppMode::CommandPalette) {
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Yellow)
    };
    spans.push(ratatui::text::Span::styled("^.", console_style));
    spans.push(ratatui::text::Span::raw(" Console | "));
    spans.push(ratatui::text::Span::styled(
        "^H",
        Style::default().fg(Color::Yellow),
    ));
    spans.push(ratatui::text::Span::raw(" Hidden | "));
    spans.push(ratatui::text::Span::styled(
        "^B",
        Style::default().fg(Color::Yellow),
    ));
    spans.push(ratatui::text::Span::raw(" Star | "));
    spans.push(ratatui::text::Span::styled(
        "^T",
        Style::default().fg(Color::Yellow),
    ));
    spans.push(ratatui::text::Span::raw(" New Tab | "));
    spans.push(ratatui::text::Span::styled(
        "^W",
        Style::default().fg(Color::Yellow),
    ));
    spans.push(ratatui::text::Span::raw(" Close Tab | "));
    spans.push(ratatui::text::Span::styled(
        "Del",
        Style::default().fg(Color::Yellow),
    ));
    spans.push(ratatui::text::Span::raw(" Action "));
    if let Some(disk) = app.system_state.disks.first() {
        spans.push(ratatui::text::Span::raw(" | Storage: "));
        spans.push(ratatui::text::Span::styled(
            format!(
                "{:.1}/{:.1} GB",
                disk.total_space - disk.used_space,
                disk.total_space
            ),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
    }
    f.render_widget(Paragraph::new(ratatui::text::Line::from(spans)), area);
}

fn draw_context_menu(f: &mut Frame, x: u16, y: u16, item_index: Option<usize>, app: &App) {
    let mut items = Vec::new();
    let mut title = " Menu ";
    if let Some(idx) = item_index {
        if let Some(fs) = app.current_file_state() {
            if let Some(path) = fs.files.get(idx) {
                let is_dir = fs.metadata.get(path).map(|m| m.is_dir).unwrap_or(false);
                if is_dir {
                    title = " Folder ";
                    items.push(ListItem::new(" 󰉋 Open"));
                    items.push(ListItem::new(" 󰓎 Star"));
                    items.push(ListItem::new(" 󰏫 Rename"));
                    items.push(ListItem::new(" 󰆴 Delete"));
                } else {
                    title = " File ";
                    items.push(ListItem::new(" 󰚩 Edit (Demon)"));
                    items.push(ListItem::new(" 󰓎 Star"));
                    items.push(ListItem::new(" 󰏫 Rename"));
                    items.push(ListItem::new(" 󰆴 Delete"));
                    items.push(ListItem::new(" 󰈙 Properties"));
                }
            }
        }
    } else {
        title = " Actions ";
        items.push(ListItem::new(" 󰉋 New Folder"));
        items.push(ListItem::new(" 󰈔 New File"));
        items.push(ListItem::new(" 󰑐 Refresh"));
        items.push(ListItem::new(" 󰆍 Terminal Here"));
    }
    let area = Rect::new(x, y, 20, items.len() as u16 + 2);
    f.render_widget(Clear, area);
    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::Yellow))
                .title(title),
        ),
        area,
    );
}

fn draw_command_palette(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .title(" Command Palette ")
                .border_style(Style::default().fg(Color::Magenta))
                .inner(area),
        );
    f.render_widget(
        Paragraph::new(format!("> {}", app.input)).style(Style::default().fg(Color::Yellow)),
        chunks[0],
    );
    let items: Vec<ListItem> = app
        .filtered_commands
        .iter()
        .enumerate()
        .map(|(i, cmd)| {
            let style = if i == app.command_index {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };
            ListItem::new(cmd.label.clone()).style(style)
        })
        .collect();
    f.render_widget(List::new(items), chunks[1]);
}

fn draw_rename_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(app.input.as_str()).block(
            Block::default()
                .title(" Rename ")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        area,
    );
}

fn draw_new_file_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(app.input.as_str()).block(
            Block::default()
                .title(" New File Name ")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::Green)),
        ),
        area,
    );
}

fn draw_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    let text = match app.current_view {
        CurrentView::Files => {
            if let Some(fs) = app.current_file_state() {
                if let Some(idx) = fs.selected_index {
                    if let Some(p) = fs.files.get(idx) {
                        format!(
                            "Delete {}? (y/n)",
                            p.file_name().unwrap_or_default().to_string_lossy()
                        )
                    } else {
                        "Delete? (y/n)".to_string()
                    }
                } else {
                    "Delete? (y/n)".to_string()
                }
            } else {
                "Delete? (y/n)".to_string()
            }
        }
        _ => "Delete? (y/n)".to_string(),
    };
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title(" Confirm Action ")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::Red)),
        ),
        area,
    );
}

fn draw_properties_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 30, f.area());
    f.render_widget(Clear, area);
    let info = match app.current_view {
        CurrentView::Files => {
            if let Some(fs) = app.current_file_state() {
                if let Some(idx) = fs.selected_index {
                    if let Some(p) = fs.files.get(idx) {
                        let metadata = std::fs::metadata(p);
                        let mut s = format!(
                            "Name: {}\n",
                            p.file_name().unwrap_or_default().to_string_lossy()
                        );
                        s.push_str(&format!(
                            "Type: {}\n",
                            if p.is_dir() { "Directory" } else { "File" }
                        ));
                        if let Ok(m) = metadata {
                            s.push_str(&format!("Size: {} bytes\n", m.len()));
                            if let Ok(modi) = m.modified() {
                                s.push_str(&format!("Modified: {:?}\n", modi));
                            }
                        }
                        s
                    } else {
                        "No file selected".to_string()
                    }
                } else {
                    "No file selected".to_string()
                }
            } else {
                "No file selected".to_string()
            }
        }
        _ => "No info available".to_string(),
    };
    f.render_widget(
        Paragraph::new(info).block(
            Block::default()
                .title(" Properties ")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::Cyan)),
        ),
        area,
    );
}

fn draw_column_setup_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 40, f.area());
    f.render_widget(Clear, area);
    if let Some(fs) = app.current_file_state() {
        let options = vec![
            (FileColumn::Name, "Name (n)"),
            (FileColumn::Size, "Size (s)"),
            (FileColumn::Modified, "Modified (m)"),
            (FileColumn::Created, "Created (c)"),
            (FileColumn::Permissions, "Permissions (p)"),
            (FileColumn::Extension, "Extension (e)"),
        ];
        let items: Vec<ListItem> = options
            .iter()
            .map(|(col, label)| {
                let prefix = if fs.columns.contains(col) {
                    "[x] "
                } else {
                    "[ ] "
                };
                ListItem::new(format!("{}{}", prefix, label))
            })
            .collect();
        f.render_widget(
            List::new(items).block(
                Block::default()
                    .title(" Column Setup ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Plain)
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            area,
        );
    }
}

fn draw_new_folder_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(app.input.as_str()).block(
            Block::default()
                .title(" New Folder ")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::Green)),
        ),
        area,
    );
}

fn draw_add_remote_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 20, f.area());
    f.render_widget(Clear, area);
    let text = format!(
        "Enter connection string (user@host:port):\n> {}\n\n(Press Enter to add, Esc to cancel)",
        app.input
    );
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title(" Add Remote Host ")
                .borders(Borders::ALL)
                .border_type(BorderType::Plain)
                .border_style(Style::default().fg(Color::Green)),
        ),
        area,
    );
}

use chrono::{DateTime, Local};
fn format_size(size: u64) -> String {
    if size >= 1073741824 {
        format!("{:.1} GB", size as f64 / 1073741824.0)
    } else if size >= 1048576 {
        format!("{:.1} MB", size as f64 / 1048576.0)
    } else if size >= 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{} B", size)
    }
}
fn format_time(time: SystemTime) -> String {
    let datetime: DateTime<Local> = time.into();
    datetime.format("%Y-%m-%d %H:%M").to_string()
}
fn format_permissions(mode: u32) -> String {
    let r = |b| if b & 4 != 0 { "r" } else { "-" };
    let w = |b| if b & 2 != 0 { "w" } else { "-" };
    let x = |b| if b & 1 != 0 { "x" } else { "-" };
    format!(
        "{}{}{}{}{}{}{}{}{}",
        r((mode >> 6) & 0o7),
        w((mode >> 6) & 0o7),
        x((mode >> 6) & 0o7),
        r((mode >> 3) & 0o7),
        w((mode >> 3) & 0o7),
        x((mode >> 3) & 0o7),
        r(mode & 0o7),
        w(mode & 0o7),
        x(mode & 0o7)
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
