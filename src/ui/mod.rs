use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, TableState, Tabs,
    },
    Frame,
};
use std::time::SystemTime;

use crate::app::{
    App, AppMode, CurrentView, DropTarget, FileColumn, MonitorSubview, ProcessColumn,
    SettingsSection, SettingsTarget,
};
use crate::icons::Icon;
use crate::ui::theme::THEME;
use terma::layout::centered_rect;
use terma::utils::{
    format_permissions, format_size, format_time, get_visual_width, squarify, truncate_to_width,
};
use terma::widgets::HotkeyHint;
use unicode_width::UnicodeWidthStr;

pub mod modals;
pub mod panes;
pub mod theme;

pub use panes::breadcrumbs::draw_pane_breadcrumbs;

pub fn draw(f: &mut Frame, app: &mut App) {
    f.render_widget(Clear, f.area());

    if matches!(app.mode, AppMode::Editor | AppMode::Viewer | AppMode::EditorSearch | AppMode::EditorGoToLine | AppMode::EditorReplace) && app.show_main_stage && !app.is_split_mode {
        // --- FULL SCREEN EDITOR VIEW (Zen Mode / Overlay) ---
        let mut header_left = Vec::new();
        let border_color = if let Some(preview) = &app.editor_state {
            if let Some(last_saved) = preview.last_saved {
                if last_saved.elapsed().as_secs() < 2 {
                    Color::Green
                } else if let Some(editor) = &preview.editor {
                    if editor.modified {
                        Color::Yellow
                    } else {
                        Color::White
                    }
                } else {
                    Color::White
                }
            } else if let Some(editor) = &preview.editor {
                if editor.modified {
                    Color::Yellow
                } else {
                    Color::White
                }
            } else {
                Color::White
            }
        } else {
            Color::White
        };

        match app.mode {
            AppMode::EditorSearch => {
                header_left.push(Span::styled(
                    "SEARCH: ",
                    Style::default()
                        .fg(THEME.accent_primary)
                        .add_modifier(Modifier::BOLD),
                ));
                header_left.push(Span::styled(
                    &app.input.value,
                    Style::default().fg(Color::White),
                ));
            }
            AppMode::EditorGoToLine => {
                header_left.push(Span::styled(
                    "GO TO LINE: ",
                    Style::default()
                        .fg(THEME.accent_primary)
                        .add_modifier(Modifier::BOLD),
                ));
                header_left.push(Span::styled(
                    &app.input.value,
                    Style::default().fg(Color::White),
                ));
            }
            AppMode::EditorReplace => {
                if app.replace_buffer.is_empty() {
                    header_left.push(Span::styled(
                        "REPLACE [FIND]: ",
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ));
                    header_left.push(Span::styled(
                        &app.input.value,
                        Style::default().fg(Color::White),
                    ));
                } else {
                    header_left.push(Span::styled(
                        "REPLACE [WITH]: ",
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD),
                    ));
                    header_left.push(Span::styled(
                        &app.input.value,
                        Style::default().fg(Color::White),
                    ));
                }
            }
            _ => {
                header_left.extend(HotkeyHint::render("^F", "Find", THEME.accent_secondary));
                header_left.extend(HotkeyHint::render(
                    "^R/F2",
                    "Replace",
                    THEME.accent_secondary,
                ));
                header_left.extend(HotkeyHint::render("^G", "Line", THEME.accent_secondary));
            }
        }

        let mut header_right = Vec::new();
        header_right.extend(HotkeyHint::render("Esc", "Back", Color::Red));
        header_right.extend(HotkeyHint::render("^Q", "Quit", Color::Red));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title_top(Line::from(header_left))
            .title_top(Line::from(header_right).alignment(ratatui::layout::Alignment::Right))
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(Color::Rgb(0, 0, 0)));

        f.render_widget(block.clone(), f.area());

        let inner_area = block.inner(f.area());
        // Fix for line number border overlap: add 1 column of padding on left
        let inner_area = ratatui::layout::Rect {
            x: inner_area.x + 1,
            width: inner_area.width.saturating_sub(1),
            ..inner_area
        };

        if let Some(preview) = &app.editor_state {
            if let Some(editor) = &preview.editor {
                // Ensure wrap is handled
                let mut editor_clone = editor.clone();
                editor_clone.wrap = app.is_split_mode;
                f.render_widget(&editor_clone, inner_area);
            }
        }
    } else if matches!(app.mode, AppMode::Settings) {
        f.render_widget(
            Block::default().style(Style::default().bg(Color::Black)),
            f.area(),
        );
        draw_settings_modal(f, app);
    } else if matches!(
        app.current_view,
        CurrentView::Processes | CurrentView::Git
    ) {
        f.render_widget(
            Block::default().style(Style::default().bg(Color::Black)),
            f.area(),
        );
        match app.current_view {
            CurrentView::Processes => draw_monitor_page(f, f.area(), app),
            CurrentView::Git => draw_git_page(f, f.area(), app),
            _ => {}
        }
    } else {
        // Normal File Manager Background
        f.render_widget(
            Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0))),
            f.area(),
        );

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(2),
            ])
            .split(f.area());

        let workspace_constraints = if app.show_main_stage {
            if app.show_sidebar {
                [Constraint::Length(app.sidebar_width()), Constraint::Fill(1)]
            } else {
                [Constraint::Length(0), Constraint::Fill(1)]
            }
        } else {
            [Constraint::Fill(1), Constraint::Length(0)]
        };

        let workspace = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(workspace_constraints)
            .split(chunks[1]);

        draw_global_header(f, chunks[0], workspace[0].width, app);

        if app.show_sidebar || !app.show_main_stage {
            crate::ui::panes::sidebar::draw_sidebar(f, workspace[0], app);
        }

        if app.show_main_stage {
            draw_main_stage(f, workspace[1], app);
        }

        draw_footer(f, chunks[2], app);
    }

    // --- OVERLAYS ---
    if let AppMode::Hotkeys = app.mode {
        draw_hotkeys_modal(f, f.area());
    }
    if matches!(app.mode, AppMode::ContextMenu { .. }) {
        if let AppMode::ContextMenu {
            x, y, ref target, ..
        } = app.mode
        {
            draw_context_menu(f, x, y, target, app);
        }
    }
    if matches!(app.mode, AppMode::Highlight) {
        draw_highlight_modal(f, app);
    }
    if matches!(app.mode, AppMode::Rename) {
        draw_rename_modal(f, app);
    }
    if matches!(app.mode, AppMode::Delete | AppMode::DeleteFile(_)) {
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
    if matches!(app.mode, AppMode::CommandPalette) {
        draw_command_palette(f, app);
    }
    if matches!(app.mode, AppMode::AddRemote(_)) {
        draw_add_remote_modal(f, app);
    }
    if matches!(app.mode, AppMode::ImportServers) {
        draw_import_servers_modal(f, app);
    }
    if let AppMode::OpenWith(ref path) = app.mode {
        draw_open_with_modal(f, app, path);
    }
    if let AppMode::DragDropMenu {
        ref sources,
        ref target,
    } = app.mode
    {
        draw_drag_drop_modal(f, app, sources, target);
    }

    if app.is_dragging {
        draw_drag_ghost(f, app);
    }
}

fn draw_drag_drop_modal(
    f: &mut Frame,
    app: &App,
    sources: &[std::path::PathBuf],
    target: &std::path::Path,
) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Choice Action ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let dest_path = target.to_string_lossy();

    // Calculate correct button offset based on content
    let button_y_offset = if sources.len() == 1 {
        3
    } else {
        let display_count = std::cmp::min(sources.len(), 3);
        let mut offset = 1 + display_count;
        if sources.len() > 3 {
            offset += 1;
        }
        offset + 2 // + To: line + spacing line
    };

    let (mx, my) = app.mouse_pos;

    let is_hover = |bx: u16, len: u16| {
        mx >= inner.x + bx && mx < inner.x + bx + len && my == inner.y + button_y_offset as u16
    };

    let copy_style = if is_hover(0, 10) {
        Style::default().bg(Color::Green).fg(Color::Black)
    } else {
        Style::default().fg(Color::Green)
    };
    let move_style = if is_hover(12, 10) {
        Style::default().bg(Color::Yellow).fg(Color::Black)
    } else {
        Style::default().fg(Color::Yellow)
    };
    let link_style = if is_hover(24, 10) {
        Style::default().bg(Color::Magenta).fg(Color::Black)
    } else {
        Style::default().fg(Color::Magenta)
    };
    let cancel_style = if is_hover(36, 14) {
        Style::default().bg(Color::Red).fg(Color::Black)
    } else {
        Style::default().fg(Color::Red)
    };

    let mut text = Vec::new();

    if sources.len() == 1 {
        let src_name = sources[0].file_name().unwrap_or_default().to_string_lossy();
        text.push(Line::from(vec![
            Span::raw("Item: "),
            Span::styled(
                src_name,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    } else {
        text.push(Line::from(vec![
            Span::raw("Items: "),
            Span::styled(
                format!("{} files/folders", sources.len()),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        // List first few items
        for source in sources.iter().take(std::cmp::min(sources.len(), 3)) {
            let name = source.file_name().unwrap_or_default().to_string_lossy();
            text.push(Line::from(vec![
                Span::raw("  - "),
                Span::styled(name, Style::default().fg(Color::DarkGray)),
            ]));
        }
        if sources.len() > 3 {
            text.push(Line::from(vec![Span::raw("  ... ")]));
        }
    }

    text.push(Line::from(vec![
        Span::raw("To:    "),
        Span::styled(
            truncate_to_width(&dest_path, (inner.width as usize).saturating_sub(7), "..."),
            Style::default().fg(Color::Cyan),
        ),
    ]));

    // Spacing
    text.push(Line::from(""));

    text.push(Line::from(vec![
        Span::styled(" [C] Copy ", copy_style.add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" [M] Move ", move_style.add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" [L] Link ", link_style.add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(" [Esc] Cancel ", cancel_style.add_modifier(Modifier::BOLD)),
    ]));

    f.render_widget(Paragraph::new(text), inner);
}

fn draw_hotkeys_modal(f: &mut Frame, _area: Rect) {
    let area = centered_rect(70, 80, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" KEYBINDINGS ")
        .border_style(Style::default().fg(THEME.accent_primary));
    f.render_widget(block.clone(), area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(2),
        ])
        .split(block.inner(area));

    f.render_widget(
        Paragraph::new("Press ESC or F1 to Close")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(ratatui::layout::Alignment::Center),
        chunks[0],
    );

    let keys = vec![
        (
            "Global",
            vec![
                ("F1", "Show this Help"),
                ("Ctrl + Q", "Quit Application"),
                ("Ctrl + B", "Toggle Sidebar"),
                ("Ctrl + M", "Toggle Main Stage"),
                ("Ctrl + P", "Toggle Split View"),
                ("Ctrl + G", "Open Settings"),
                ("Ctrl + L", "Git History"),
                ("Ctrl + E", "Toggle Editor View (IDE)"),
                ("Ctrl + J", "Toggle Bottom Panel"),
                ("Ctrl + Space", "Command Palette"),
                ("Ctrl + N", "Open Terminal"),
                ("Backspace", "Go Up Directory"),
            ],
        ),
        (
            "IDE Mode",
            vec![
                ("Ctrl + B", "Toggle Sidebar"),
                ("Ctrl + P", "Toggle Split Panes"),
                ("Esc", "Focus Sidebar / Back"),
                ("Enter", "Open File/Folder"),
                ("Arrows", "Navigate Tree / Editor"),
            ],
        ),
        (
            "File Navigation",
            vec![
                ("Arrows", "Navigate"),
                ("Enter", "Open Folder / Launch"),
                ("Space", "Editor"),
                ("Ctrl + I", "Information"),
                ("Backspace", "Go Up Directory"),
                ("Home / ~", "Go Home"),
                ("Alt + Left/Right", "Resize Sidebar"),
                ("F2", "Rename File"),
                ("Delete", "Delete File"),
            ],
        ),
        (
            "Editor",
            vec![
                ("Ctrl + F", "Find (Live Filter)"),
                ("Ctrl + R / F2", "Replace All"),
                ("Ctrl + G", "Go To Line"),
                ("Ctrl + C", "Copy Line"),
                ("Ctrl + X", "Cut Line / Delete Line"),
                ("Ctrl + Bksp", "Delete Word"),
                ("Esc", "Exit Editor"),
            ],
        ),
    ];

    let mut rows = Vec::new();
    for (section, items) in keys {
        rows.push(Row::new(vec![
            Cell::from(Span::styled(
                section,
                Style::default()
                    .fg(THEME.accent_primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Cell::from(""),
        ]));
        for (key, desc) in items {
            rows.push(Row::new(vec![
                Cell::from(Span::styled(
                    format!("  {}", key),
                    Style::default().fg(Color::Yellow),
                )),
                Cell::from(desc),
            ]));
        }
        rows.push(Row::new(vec![Cell::from(""), Cell::from("")]));
    }

    let table = Table::new(
        rows,
        [Constraint::Percentage(30), Constraint::Percentage(70)],
    )
    .block(Block::default());

    f.render_widget(table, chunks[1]);
}

fn draw_open_with_modal(f: &mut Frame, app: &App, path: &std::path::Path) {
    let area = centered_rect(60, 60, f.area()); // Increased height
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Open With... ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Info
            Constraint::Length(3), // Input
            Constraint::Min(0),    // Suggestions List
        ])
        .split(inner);

    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    f.render_widget(Paragraph::new(format!("Opening: {}", file_name)), chunks[0]);

    let input_block = Block::default()
        .borders(Borders::ALL)
        .title(" Custom Command ")
        .border_style(Style::default().fg(THEME.accent_primary));
    f.render_widget(
        Paragraph::new(app.input.value.as_str()).block(input_block),
        chunks[1],
    );

    // Simple common suggestions based on extension
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mut suggestions = crate::event_helpers::get_open_with_suggestions(app, &ext);

    // Filter suggestions based on input
    if !app.input.value.is_empty() {
        let query = app.input.value.to_lowercase();
        suggestions.retain(|s| s.to_lowercase().contains(&query));
    }

    let (mx, my) = app.mouse_pos;
    let list_items: Vec<ListItem> = suggestions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let item_y = chunks[2].y + i as u16;
            let is_mouse_hovered =
                mx >= chunks[2].x && mx < chunks[2].x + chunks[2].width && my == item_y;
            let is_selected = i == app.open_with_index;

            let style = if is_mouse_hovered || is_selected {
                Style::default()
                    .bg(THEME.accent_primary)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(format!("  󰀻  {}", s)).style(style)
        })
        .collect();

    let title = if app.input.value.is_empty() {
        " Suggestions (Click to Launch) "
    } else {
        " Filtered Suggestions (Click to Launch) "
    };

    let list = List::new(list_items).block(
        Block::default()
            .title(title)
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(list, chunks[2]);
}

fn draw_monitor_page(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .title_top(Line::from(vec![Span::styled(
            " SYSTEM MONITOR ",
            Style::default()
                .fg(Color::Black)
                .bg(THEME.accent_primary)
                .add_modifier(Modifier::BOLD),
        )]))
        .title_top(
            Line::from(vec![
                Span::styled(
                    " Esc ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Back ", Style::default().fg(Color::Red)),
            ])
            .alignment(Alignment::Right),
        )
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
        MonitorSubview::Cpu
        | MonitorSubview::Memory
        | MonitorSubview::Disk
        | MonitorSubview::Network => draw_monitor_overview(f, content_area, app),
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
        app.system_state.mem_usage,
        app.system_state.total_mem,
        "GB",
    );
    draw_telemetry_bank(
        f,
        bank_layout[2],
        "SWAP",
        app.system_state.swap_usage,
        app.system_state.total_swap,
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
    let _net_area = right_chunks[1].inner(ratatui::layout::Margin {
        horizontal: 1,
        vertical: 0,
    });
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

fn draw_global_header(f: &mut Frame, area: Rect, sidebar_width: u16, app: &mut App) {
    let _now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let pane_count = app.panes.len();

    // Toolbar Icons Cluster (Far Left)
    let back_icon = Icon::Back.get(app.icon_mode);
    let forward_icon = Icon::Forward.get(app.icon_mode);
    let split_icon = Icon::Split.get(app.icon_mode);
    let burger_icon = Icon::Burger.get(app.icon_mode);

    let monitor_icon = Icon::Monitor.get(app.icon_mode);
    let git_icon = Icon::Git.get(app.icon_mode);

    app.header_icon_bounds.clear();
    let mut cur_icon_x = area.x + 2;

    let show_icons = app.show_sidebar;

    if show_icons {
        let icons = [
            (burger_icon, "burger"),
            (back_icon, "back"),
            (forward_icon, "forward"),
            (split_icon, "split"),
            (monitor_icon, "monitor"),
            (git_icon, "git"),
        ];

        for (i, (icon, id)) in icons.into_iter().enumerate() {
            let width = icon.width() as u16;
            let rect = Rect::new(cur_icon_x, area.y, width, 1);

            let mut style = Style::default().fg(THEME.accent_secondary);
            if let AppMode::Header(idx) = app.mode {
                if idx == i {
                    style = style
                        .bg(THEME.accent_primary)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                }
            }

            f.render_widget(Paragraph::new(icon).style(style), rect);
            app.header_icon_bounds.push((rect, id.to_string()));
            cur_icon_x += width + 2;
        }
    }

    if pane_count == 0 {
        return;
    }
    let start_x = if show_icons {
        std::cmp::max(area.x + sidebar_width, cur_icon_x + 1)
    } else {
        area.x + 2
    };
    let pane_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Fill(1); pane_count])
        .split(Rect::new(
            start_x,
            area.y,
            area.width.saturating_sub(start_x),
            1,
        ));

    app.tab_bounds.clear();
    let mut global_tab_idx = if show_icons { 6 } else { 0 };
    for (p_i, pane) in app.panes.iter().enumerate() {
        let chunk = pane_chunks[p_i];
        let mut current_x = chunk.x;

        if app.current_view == CurrentView::Editor {
            // SINGLE FILE TAB for Editor View
            if let Some(preview) = &pane.preview {
                let base_name = preview
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Editor".to_string());

                let is_focused_pane = p_i == app.focused_pane_index && !app.sidebar_focus;
                let base_style = if is_focused_pane {
                    Style::default()
                        .fg(THEME.accent_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME.accent_primary)
                };

                let mut spans = vec![Span::styled(format!(" {} ", base_name), base_style)];

                // Restrict git indicators to Git page to avoid cross-view bleed.
                if app.current_view == CurrentView::Git {
                    if let Some(tab) = pane.tabs.get(pane.active_tab_index) {
                        if let Some(branch) = &tab.git_branch {
                            let pending = tab.git_pending.len();
                            let ahead = tab.git_ahead;
                            let behind = tab.git_behind;

                            let branch_color = if pending > 0 {
                                Color::Red
                            } else if ahead > 0 || behind > 0 {
                                Color::Yellow
                            } else {
                                Color::Green
                            };

                            let mut branch_style = Style::default().fg(branch_color);
                            if is_focused_pane {
                                branch_style = branch_style.add_modifier(Modifier::BOLD);
                            }

                            spans.push(Span::styled(format!("({})", branch), branch_style));

                            if pending > 0 {
                                spans.push(Span::styled(
                                    format!("+{}", pending),
                                    Style::default().fg(Color::Red),
                                ));
                            }
                            if ahead > 0 {
                                spans.push(Span::styled(
                                    format!(" ↑{}", ahead),
                                    Style::default().fg(Color::Yellow),
                                ));
                            }
                            if behind > 0 {
                                spans.push(Span::styled(
                                    format!(" ↓{}", behind),
                                    Style::default().fg(Color::Yellow),
                                ));
                            }
                            spans.push(Span::raw(" "));
                        }
                    }
                }

                let line = Line::from(spans);
                let width = line.width() as u16;
                let rect = Rect::new(current_x, area.y, width, 1);
                f.render_widget(Paragraph::new(line), rect);
                // We'll still register it as a 'tab' so header-mode can highlight it
                app.tab_bounds.push((rect, p_i, pane.active_tab_index));
            }
            continue;
        }

        for (t_i, tab) in pane.tabs.iter().enumerate() {
            let mut spans = Vec::new();
            let base_name = tab
                .current_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "/".to_string());

            let is_active_tab = t_i == pane.active_tab_index;
            let is_focused_pane = p_i == app.focused_pane_index && !app.sidebar_focus;

            let mut base_style = if is_active_tab {
                if is_focused_pane {
                    Style::default()
                        .fg(THEME.accent_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME.accent_primary)
                }
            } else {
                Style::default().fg(Color::DarkGray)
            };

            if let AppMode::Header(idx) = app.mode {
                if idx == global_tab_idx {
                    base_style = base_style
                        .bg(THEME.accent_primary)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                }
            }

            spans.push(Span::styled(format!(" {} ", base_name), base_style));

            if app.current_view == CurrentView::Git {
                if let Some(branch) = &tab.git_branch {
                    let pending = tab.git_pending.len();
                    let ahead = tab.git_ahead;
                    let behind = tab.git_behind;

                    let branch_color = if pending > 0 {
                        Color::Red
                    } else if ahead > 0 || behind > 0 {
                        Color::Yellow
                    } else {
                        Color::Green
                    };

                    let mut branch_style = Style::default().fg(branch_color);
                    if is_active_tab && is_focused_pane {
                        branch_style = branch_style.add_modifier(Modifier::BOLD);
                    }

                    spans.push(Span::styled(format!("({})", branch), branch_style));

                    if pending > 0 {
                        spans.push(Span::styled(
                            format!("+{}", pending),
                            Style::default().fg(Color::Red),
                        ));
                    }
                    if ahead > 0 {
                        spans.push(Span::styled(
                            format!(" ↑{}", ahead),
                            Style::default().fg(Color::Yellow),
                        ));
                    }
                    if behind > 0 {
                        spans.push(Span::styled(
                            format!(" ↓{}", behind),
                            Style::default().fg(Color::Yellow),
                        ));
                    }
                    spans.push(Span::raw(" "));
                }
            }

            let line = Line::from(spans);
            let width = line.width() as u16;
            if current_x + width > chunk.x + chunk.width {
                break;
            }
            let rect = Rect::new(current_x, area.y, width, 1);
            f.render_widget(Paragraph::new(line), rect);
            app.tab_bounds.push((rect, p_i, t_i));
            current_x += width + 1;
            global_tab_idx += 1;
        }
    }
}

fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
    match app.current_view {
        CurrentView::Files => {
            let pane_count = app.panes.len();
            if pane_count == 0 {
                return;
            }

            let constraints = vec![Constraint::Fill(1); pane_count];
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints(constraints)
                .spacing(0)
                .split(area);
            for i in 0..pane_count {
                let is_focused = i == app.focused_pane_index && !app.sidebar_focus;
                let borders = Borders::ALL;
                draw_file_view(f, chunks[i], app, i, is_focused, borders);
            }
        }
        CurrentView::Editor => {
            crate::ui::panes::editor::draw_ide_editor(f, area, app);
        }
        _ => {}
    }
}

fn draw_git_page(f: &mut Frame, area: Rect, app: &mut App) {
    f.render_widget(Clear, area);
    let pane_idx = app.focused_pane_index;
    let tab_idx = if let Some(pane) = app.panes.get(pane_idx) {
        pane.active_tab_index
    } else {
        0
    };

    let (history, pending, _current_path, branch, summary, remotes, stashes) = if let Some(pane) = app.panes.get(pane_idx) {
        if let Some(tab) = pane.tabs.get(tab_idx) {
            (
                tab.git_history.clone(),
                tab.git_pending.clone(),
                tab.current_path.clone(),
                tab.git_branch.clone(),
                tab.git_summary.clone(),
                tab.git_remotes.clone(),
                tab.git_stashes.clone(),
            )
        } else {
            return;
        }
    } else {
        return;
    };

    let branch_name = branch.as_deref().unwrap_or("HEAD");
    let summary_text = summary.clone().unwrap_or_default();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_primary))
        .style(Style::default().bg(Color::Rgb(0, 0, 0)))
        .title_top(Line::from(vec![
            Span::styled(
                " GIT HUB ",
                Style::default()
                    .fg(Color::Black)
                    .bg(THEME.accent_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!(" [{}] ", branch_name), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(" {} ", _current_path.to_string_lossy()),
                Style::default().fg(THEME.accent_secondary),
            ),
        ]))
        .title_bottom(Line::from(format!(" {} ", summary_text)).alignment(Alignment::Right))
        .title_top(
            Line::from(vec![
                Span::styled(
                    " Esc ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Back ", Style::default().fg(Color::Red)),
            ])
            .alignment(Alignment::Right),
        );

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Main vertical split: [ Top Row (Active & Info) | Bottom Row (History) ]
    let top_h = if pending.is_empty() && remotes.is_empty() && stashes.is_empty() {
        0
    } else {
        let p_len = if pending.is_empty() { 0 } else { pending.len() as u16 + 2 };
        let i_len = if remotes.is_empty() && stashes.is_empty() { 0 } else { 6 };
        p_len.max(i_len).min(inner.height / 3)
    };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top_h),
            Constraint::Min(0),
        ])
        .split(inner);

    let top_area = main_chunks[0];
    let history_area = main_chunks[1];

    if top_h > 0 {
        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Fill(6), Constraint::Fill(4)])
            .split(top_area);

        let active_area = top_chunks[0];
        let info_area = top_chunks[1];

        // 1. ACTIVE Changes
        if !pending.is_empty() {
            let active_title = format!(" ACTIVE ({} Affected) ", pending.len());
            
            let pending_rows: Vec<_> = pending
                .iter()
                .map(|p| {
                    let status_color = match p.status.as_str() {
                        "M" => Color::Yellow,
                        "A" | "??" => Color::Green,
                        "D" => Color::Red,
                        "R" => Color::Cyan,
                        _ => Color::White,
                    };
                    
                    let mut stats_spans = Vec::new();
                    if p.insertions > 0 {
                        stats_spans.push(Span::styled(format!(" +{}", p.insertions), Style::default().fg(Color::Green)));
                    }
                    if p.deletions > 0 {
                        stats_spans.push(Span::styled(format!(" -{}", p.deletions), Style::default().fg(Color::Red)));
                    }

                    Row::new(vec![
                        Cell::from(format!(" {} ", p.status)).style(
                            Style::default()
                                .bg(status_color)
                                .fg(Color::Black)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Cell::from(p.path.clone()).style(Style::default().fg(THEME.fg)),
                        Cell::from(Line::from(stats_spans)),
                    ])
                })
                .collect();

            let pending_table = Table::new(pending_rows, [Constraint::Length(6), Constraint::Fill(1), Constraint::Length(15)])
                .block(
                    Block::default()
                        .title(active_title)
                        .border_style(Style::default().fg(Color::Rgb(40, 45, 55)))
                        .borders(Borders::RIGHT),
                )
                .row_highlight_style(
                    Style::default()
                        .bg(Color::Rgb(40, 40, 50))
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                );

            if let Some(pane) = app.panes.get_mut(pane_idx) {
                if let Some(tab) = pane.tabs.get_mut(tab_idx) {
                    f.render_stateful_widget(pending_table, active_area, &mut tab.git_pending_state);
                }
            }
        }

        // 2. Info Panel (Right Area) - DYNAMIC based on selection
        let mut info_items = Vec::new();
        
        let mut selected_history_item = None;
        if let Some(pane) = app.panes.get(pane_idx) {
            if let Some(tab) = pane.tabs.get(tab_idx) {
                if let Some(idx) = tab.git_history_state.selected() {
                    selected_history_item = tab.git_history.get(idx);
                }
            }
        }

        let mut selected_active_item = None;
        if let Some(pane) = app.panes.get(pane_idx) {
            if let Some(tab) = pane.tabs.get(tab_idx) {
                if let Some(idx) = tab.git_pending_state.selected() {
                    selected_active_item = tab.git_pending.get(idx);
                }
            }
        }

        if let Some(commit) = selected_history_item {
            info_items.push(ListItem::new(Line::from(vec![
                Span::styled(" COMMIT DETAILS ", Style::default().bg(THEME.accent_secondary).fg(Color::Black).add_modifier(Modifier::BOLD)),
            ])));
            info_items.push(ListItem::new(format!("  Hash: {}", commit.hash)));
            info_items.push(ListItem::new(format!("  Author: {}", commit.author)));
            info_items.push(ListItem::new(format!("  Date: {}", commit.date)));
            info_items.push(ListItem::new(""));
            info_items.push(ListItem::new(Span::styled(format!("  {}", commit.message), Style::default().fg(Color::White))));
            info_items.push(ListItem::new(""));
            info_items.push(ListItem::new(vec![
                Line::from(vec![
                    Span::raw("  Files: "),
                    Span::styled(format!("{}", commit.files_changed), Style::default().fg(Color::Cyan)),
                ]),
                Line::from(vec![
                    Span::raw("  Additions: "),
                    Span::styled(format!("+{}", commit.insertions), Style::default().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::raw("  Deletions: "),
                    Span::styled(format!("-{}", commit.deletions), Style::default().fg(Color::Red)),
                ]),
            ]));
        } else if let Some(change) = selected_active_item {
            info_items.push(ListItem::new(Line::from(vec![
                Span::styled(" CHANGE DETAILS ", Style::default().bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD)),
            ])));
            info_items.push(ListItem::new(format!("  Path: {}", change.path)));
            info_items.push(ListItem::new(format!("  Status: {}", change.status)));
            info_items.push(ListItem::new(""));
            info_items.push(ListItem::new(vec![
                Line::from(vec![
                    Span::raw("  Additions: "),
                    Span::styled(format!("+{}", change.insertions), Style::default().fg(Color::Green)),
                ]),
                Line::from(vec![
                    Span::raw("  Deletions: "),
                    Span::styled(format!("-{}", change.deletions), Style::default().fg(Color::Red)),
                ]),
            ]));
        } else {
            // Default: Remotes & Stashes
            if !remotes.is_empty() {
                info_items.push(ListItem::new(Line::from(vec![
                    Span::styled(" REMOTES ", Style::default().bg(Color::Rgb(40, 45, 55)).fg(Color::White).add_modifier(Modifier::BOLD)),
                ])));
                for r in remotes.iter().take(8) {
                    info_items.push(ListItem::new(Span::styled(format!("  {}", r), Style::default().fg(THEME.fg))));
                }
                info_items.push(ListItem::new(""));
            }

            if !stashes.is_empty() {
                info_items.push(ListItem::new(Line::from(vec![
                    Span::styled(" STASHES ", Style::default().bg(Color::Rgb(40, 45, 55)).fg(Color::White).add_modifier(Modifier::BOLD)),
                ])));
                for s in stashes.iter().take(8) {
                    info_items.push(ListItem::new(Span::styled(format!("  {}", s), Style::default().fg(THEME.fg))));
                }
            }

            if info_items.is_empty() {
                info_items.push(ListItem::new(Span::styled("  (No remotes or stashes)", Style::default().fg(Color::DarkGray))));
            }
        }

        f.render_widget(
            List::new(info_items).block(
                Block::default()
                    .title(" INFO ")
                    .border_style(Style::default().fg(Color::Rgb(40, 45, 55)))
                    .borders(Borders::NONE),
            ),
            info_area,
        );
    }

    // 3. HISTORY
    if history.is_empty() {
        f.render_widget(
            Paragraph::new("\n\n No git history found for this path or not a git repository.")
                .alignment(Alignment::Center),
            history_area,
        );
    } else {
        let rows: Vec<_> = history
            .iter()
            .map(|act| {
                let h_short = act.hash.chars().take(7).collect::<String>();
                let branch_info = act.decorations.trim().trim_matches(|c| c == '(' || c == ')');
                
                let mut stats_cells = Vec::new();
                if act.files_changed > 0 {
                    stats_cells.push(Cell::from(format!("{}", act.files_changed)).style(Style::default().fg(Color::Cyan)));
                    stats_cells.push(Cell::from(format!("+{}", act.insertions)).style(Style::default().fg(Color::Green)));
                    stats_cells.push(Cell::from(format!("-{}", act.deletions)).style(Style::default().fg(Color::Red)));
                } else {
                    stats_cells.push(Cell::from(""));
                    stats_cells.push(Cell::from(""));
                    stats_cells.push(Cell::from(""));
                }

                let mut row_cells = vec![
                    Cell::from(act.date.clone()).style(Style::default().fg(Color::DarkGray)),
                    Cell::from(h_short).style(
                        Style::default()
                            .fg(THEME.accent_secondary)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Cell::from(truncate_to_width(branch_info, 15, "..")).style(Style::default().fg(Color::Yellow)),
                    Cell::from(act.author.clone()).style(Style::default().fg(Color::Cyan)),
                    Cell::from(act.message.clone()).style(Style::default().fg(THEME.fg)),
                ];
                row_cells.extend(stats_cells);

                Row::new(row_cells)
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Length(15), // DATE
                Constraint::Length(8),  // HASH
                Constraint::Length(15), // BRANCH
                Constraint::Length(15), // AUTHOR
                Constraint::Fill(1),    // MESSAGE
                Constraint::Length(6),  // FILES
                Constraint::Length(6),  // ADD
                Constraint::Length(6),  // DEL
            ],
        )
        .header(
            Row::new(vec!["DATE", "HASH", "BRANCH", "AUTHOR", "MESSAGE", "FILES", "ADD", "DEL"])
                .style(
                    Style::default()
                        .fg(THEME.accent_secondary)
                        .add_modifier(Modifier::BOLD),
                )
                .bottom_margin(1),
        )
        .block(
            Block::default()
                .title(" HISTORY ")
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::Rgb(40, 45, 55))),
        )
        .row_highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 40, 50))
                .fg(THEME.accent_secondary)
                .add_modifier(Modifier::BOLD),
        );

        if let Some(pane) = app.panes.get_mut(pane_idx) {
            if let Some(tab) = pane.tabs.get_mut(tab_idx) {
                f.render_stateful_widget(table, history_area, &mut tab.git_history_state);
            }
        }
    }
}

fn draw_file_view(
    f: &mut Frame,
    area: Rect,
    app: &mut App,
    pane_idx: usize,
    is_focused: bool,
    borders: Borders,
) {
    if let Some(pane) = app.panes.get_mut(pane_idx) {
        if let Some(preview) = &mut pane.preview {
            let block = Block::default()
                .borders(borders)
                .border_type(BorderType::Rounded)
                .title(format!(" Preview: {} ", preview.path.display()))
                .border_style(if is_focused {
                    Style::default().fg(THEME.border_active)
                } else {
                    Style::default().fg(THEME.border_inactive)
                });

            let lines = if let Some(cached) = &preview.highlighted_lines {
                cached.clone()
            } else {
                let language = preview
                    .path
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");

                // PERFORMANCE OPTIMIZATION: Only highlight what's likely to be visible + some buffer
                // This is a PREVIEW, so full file highlighting is overkill for large files.
                let content_to_highlight = if preview.content.lines().count() > 500 {
                    preview
                        .content
                        .lines()
                        .take(500)
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    preview.content.clone()
                };

                let highlighted = terma::utils::highlight_code(&content_to_highlight, language);
                let mut lines = Vec::new();
                for (i, line) in highlighted.iter().enumerate() {
                    let mut spans = line
                        .spans
                        .iter()
                        .map(|s| Span::styled(s.content.to_string(), s.style))
                        .collect::<Vec<_>>();
                    // Prepend line number gutter
                    let num = format!("{:>3} │ ", i + 1);
                    spans.insert(
                        0,
                        Span::styled(num, Style::default().fg(Color::Rgb(60, 60, 70))),
                    );
                    lines.push(Line::from(spans));
                }
                preview.highlighted_lines = Some(lines.clone());
                lines
            };

            let text = Paragraph::new(lines)
                .wrap(ratatui::widgets::Wrap { trim: false })
                .block(block);

            f.render_widget(text, area);
            return;
        }
    }

    // --- BORDER & BACKGROUND (Rendered FIRST to create base) ---

    let mut border_style = if is_focused {
        let pulse = ((SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            % 1500) as f32
            / 1500.0
            * std::f32::consts::PI
            * 2.0)
            .sin()
            * 0.5
            + 0.5;

        let r = (255.0 * (0.7 + 0.3 * pulse)) as u8;

        let g = (0.0 * (0.7 + 0.3 * pulse)) as u8;

        let b = (85.0 * (0.7 + 0.3 * pulse)) as u8;

        Style::default()
            .fg(Color::Rgb(r, g, b))
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(THEME.border_inactive)
    };

    if matches!(app.hovered_drop_target, Some(DropTarget::Pane(idx)) if idx == pane_idx) {
        border_style = Style::default()
            .fg(Color::Rgb(0, 255, 200))
            .add_modifier(Modifier::BOLD);
    }

    let main_block = Block::default()
        .borders(borders)
        .border_type(BorderType::Rounded)
        .border_style(border_style);

    f.render_widget(main_block, area);

    draw_pane_breadcrumbs(f, area, app, pane_idx);

    if let Some(file_state) = app
        .panes
        .get_mut(pane_idx)
        .and_then(|p| p.current_state_mut())
    {
        file_state.view_height = area.height as usize;

        let mut render_state = TableState::default();

        if let Some(sel) = file_state.selection.selected {
            let offset = file_state.table_state.offset();

            let capacity = file_state.view_height.saturating_sub(3);

            if sel >= offset && sel < offset + capacity {
                render_state.select(Some(sel));
            }
        }

        *render_state.offset_mut() = file_state.table_state.offset();

        let mut display_columns = Vec::new();

        for col in &file_state.columns {
            match col {
                FileColumn::Name => display_columns.push(FileColumn::Name),
                FileColumn::Size if area.width > 40 => display_columns.push(FileColumn::Size),
                FileColumn::Modified if area.width > 70 => {
                    display_columns.push(FileColumn::Modified)
                }
                FileColumn::Created if area.width > 90 => display_columns.push(FileColumn::Created),
                FileColumn::Permissions if area.width > 110 => {
                    display_columns.push(FileColumn::Permissions)
                }
                _ => {}
            }
        }
        // Ensure Name is always there as a safety fallback
        if !display_columns.contains(&FileColumn::Name) {
            display_columns.insert(0, FileColumn::Name);
        }

        let constraints: Vec<Constraint> = display_columns
            .iter()
            .map(|c| match c {
                FileColumn::Name => Constraint::Fill(1),
                FileColumn::Size => Constraint::Length(12),
                FileColumn::Modified => Constraint::Length(20),
                FileColumn::Created => Constraint::Length(20),
                FileColumn::Permissions => Constraint::Length(12),
            })
            .collect();

        let dummy_block = Block::default().borders(borders);
        let inner_area = dummy_block.inner(area);
        let column_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints.clone())
            .spacing(0)
            .split(inner_area);

        let header_lines: Vec<Line> = display_columns
            .iter()
            .map(|c| {
                let base_name = match c {
                    FileColumn::Name => "Name",
                    FileColumn::Size => "Size",
                    FileColumn::Modified => "Modified",
                    FileColumn::Created => "Created",
                    FileColumn::Permissions => "Permissions",
                };
                let name = if *c == file_state.sort_column {
                    if file_state.sort_ascending {
                        format!("{} ▲", base_name)
                    } else {
                        format!("{} ▼", base_name)
                    }
                } else {
                    base_name.to_string()
                };
                Line::from(vec![Span::styled(
                    name,
                    Style::default()
                        .fg(THEME.header_fg)
                        .add_modifier(Modifier::BOLD),
                )])
            })
            .collect();

        // --- ABSOLUTE CELL ISOLATION RENDERING ---
        file_state.column_bounds.clear();
        let header_y = inner_area.y;
        let content_y = header_y + 1;
        let visible_height = inner_area.height.saturating_sub(1) as usize;

        // 1. Render Headers
        for (col_idx, rect) in column_layout.iter().enumerate() {
            if let Some(col_type) = display_columns.get(col_idx) {
                file_state.column_bounds.push((*rect, *col_type));
                let header_line = header_lines.get(col_idx).cloned().unwrap_or(Line::from(""));
                let header_rect = Rect::new(rect.x, header_y, rect.width, 1);
                let alignment = match col_type {
                    FileColumn::Name => ratatui::layout::Alignment::Left,
                    _ => ratatui::layout::Alignment::Right,
                };
                f.render_widget(
                    Paragraph::new(header_line).alignment(alignment),
                    header_rect,
                );
            }
        }

        // 2. Render Rows
        let offset_val = file_state.table_state.offset();
        let total_files = file_state.files.len();
        for i in 0..visible_height {
            let file_idx = offset_val + i;
            if file_idx >= total_files {
                break;
            }
            let row_y = content_y + i as u16;
            let path = &file_state.files[file_idx];
            let is_selected = file_state.selection.selected == Some(file_idx);
            let is_multi_selected = file_state.selection.multi.contains(&file_idx);

            let mut row_bg_style = Style::default();
            let is_hovered_drop =
                matches!(&app.hovered_drop_target, Some(DropTarget::Folder(p)) if p == path);

            if is_selected {
                row_bg_style = row_bg_style.bg(THEME.accent_primary);
            } else if is_multi_selected {
                row_bg_style = row_bg_style.bg(Color::Rgb(200, 0, 0));
            } else if is_hovered_drop {
                row_bg_style = row_bg_style.bg(THEME.accent_secondary);
            } else if let Some(&c) = app.path_colors.get(path) {
                let color = match c {
                    1 => Color::Red,
                    2 => Color::Green,
                    3 => Color::Yellow,
                    4 => Color::Blue,
                    5 => Color::Magenta,
                    6 => Color::Cyan,
                    _ => Color::Reset,
                };
                if color != Color::Reset {
                    row_bg_style = row_bg_style.bg(color);
                }
            }
            if row_bg_style.bg.is_some() {
                f.render_widget(
                    Block::default().style(row_bg_style),
                    Rect::new(inner_area.x, row_y, inner_area.width, 1),
                );
            }

            let metadata = file_state.metadata.get(path);
            for (col_idx, col_rect) in column_layout.iter().enumerate() {
                if let Some(col_type) = display_columns.get(col_idx) {
                    let cell_rect = Rect::new(col_rect.x, row_y, col_rect.width, 1);
                    let mut cell_style = if is_selected {
                        Style::default()
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    } else if is_multi_selected {
                        Style::default().fg(Color::Black)
                    } else if is_hovered_drop {
                        Style::default()
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    } else if app.path_colors.contains_key(path) {
                        Style::default()
                            .fg(Color::Black)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(THEME.fg)
                    };

                    let content = match col_type {
                        FileColumn::Name => {
                            if path.to_string_lossy() == "__DIVIDER__" {
                                cell_style = Style::default()
                                    .fg(Color::Yellow)
                                    .add_modifier(Modifier::BOLD);
                                "> Global results".to_string()
                            } else {
                                let name =
                                    path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
                                let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
                                let cat = crate::modules::files::get_file_category(path);
                                let icon_str = Icon::get_for_path(path, cat, is_dir, app.icon_mode);

                                let mut suffix = String::new();
                                if app.starred.contains(path) {
                                    suffix.push_str(" [*]");
                                }
                                if !is_selected
                                    && !is_multi_selected
                                    && !app.path_colors.contains_key(path)
                                    && !is_hovered_drop
                                {
                                    if app.semantic_coloring {
                                        if is_dir {
                                            cell_style = cell_style.fg(THEME.accent_secondary);
                                        } else {
                                            cell_style = cell_style.fg(cat.cyber_color());
                                        }
                                    }
                                }
                                let icon_w = icon_str.chars().map(get_visual_width).sum::<usize>();
                                // Super-Aggressive Hard-Cut: reservers generous safety for icons/status
                                let available_width =
                                    (col_rect.width as usize).saturating_sub(icon_w + 12);

                                let display_name = if file_idx > file_state.local_count {
                                    let full_str = path.to_string_lossy();
                                    let home = dirs::home_dir()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "/root".to_string());
                                    if full_str.starts_with(&home) {
                                        full_str.replacen(&home, "~", 1)
                                    } else {
                                        full_str.to_string()
                                    }
                                } else {
                                    name.to_string()
                                };
                                let display_name = squarify(&display_name);

                                let truncated_name =
                                    truncate_to_width(&display_name, available_width, "..");
                                let final_line =
                                    format!(" {} {}{}", icon_str, truncated_name, suffix);
                                truncate_to_width(
                                    &final_line,
                                    (col_rect.width as usize).saturating_sub(2),
                                    "",
                                )
                            }
                        }
                        FileColumn::Size => {
                            let size = metadata.map(|m| m.size).unwrap_or(0);
                            let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
                            let text = if is_dir && size == 0 {
                                "<DIR>".to_string()
                            } else {
                                format_size(size)
                            };
                            truncate_to_width(
                                &text,
                                (col_rect.width as usize).saturating_sub(2),
                                "",
                            )
                        }
                        FileColumn::Modified => {
                            let text = format_modified_time(
                                metadata
                                    .map(|m| m.modified)
                                    .unwrap_or(SystemTime::UNIX_EPOCH),
                            );
                            truncate_to_width(
                                &text,
                                (col_rect.width as usize).saturating_sub(2),
                                "",
                            )
                        }
                        FileColumn::Permissions => {
                            let text =
                                format_permissions(metadata.map(|m| m.permissions).unwrap_or(0));
                            truncate_to_width(
                                &text,
                                (col_rect.width as usize).saturating_sub(2),
                                "",
                            )
                        }
                        FileColumn::Created => {
                            let text = format_modified_time(
                                metadata
                                    .map(|m| m.created)
                                    .unwrap_or(SystemTime::UNIX_EPOCH),
                            );
                            truncate_to_width(
                                &text,
                                (col_rect.width as usize).saturating_sub(2),
                                "",
                            )
                        }
                    };
                    let alignment = match col_type {
                        FileColumn::Name => ratatui::layout::Alignment::Left,
                        _ => ratatui::layout::Alignment::Right,
                    };
                    f.render_widget(
                        Paragraph::new(Span::styled(content, cell_style)).alignment(alignment),
                        cell_rect,
                    );
                }
            }
        }

        if total_files > area.height.saturating_sub(4) as usize {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));

            let mut scroll_state = ScrollbarState::new(file_state.files.len())
                .position(file_state.table_state.offset())
                .viewport_content_length(inner_area.height as usize);

            f.render_stateful_widget(scrollbar, area, &mut scroll_state);
        }
    }
}

fn draw_stat_bar(label: &str, value: f32, max: f32) -> Line<'static> {
    let width = 10;
    let ratio = (value / max.max(1.0)).clamp(0.0, 1.0);
    let filled = (ratio * width as f32).round() as usize;

    let mut spans = vec![Span::styled(
        format!("{} ", label),
        Style::default().fg(Color::DarkGray),
    )];

    for i in 0..width {
        let symbol = if i < filled { "█" } else { "░" };
        let color = if ratio < 0.4 {
            THEME.accent_secondary // Cyber Green
        } else if ratio < 0.7 {
            Color::Rgb(255, 255, 0) // Yellow
        } else {
            Color::Rgb(255, 0, 85) // Neon Red
        };

        if i < filled {
            spans.push(Span::styled(symbol, Style::default().fg(color)));
        } else {
            spans.push(Span::styled(
                symbol,
                Style::default().fg(Color::Rgb(30, 30, 35)),
            ));
        }
    }

    spans.push(Span::styled(
        format!(" {:>3.0}%", ratio * 100.0),
        Style::default().fg(THEME.fg).add_modifier(Modifier::BOLD),
    ));
    Line::from(spans)
}

fn draw_footer(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Fill(1),    // Log, Clipboard & Shortcuts
            Constraint::Length(20), // Selection Info
            Constraint::Length(45), // Stats (CPU/MEM)
        ])
        .split(chunks[0]);

    // 1. Left Section: ^Q Quit, Activity Log, Clipboard & Essential Shortcuts
    let mut left_spans = vec![Span::raw(" ")];

    // Log - If present, hide other shortcuts on the left
    let mut showing_log = false;
    if let Some((msg, time)) = &app.last_action_msg {
        if time.elapsed().as_secs() < 5 {
            left_spans.push(Span::styled(
                format!(" [ SYSTEM ] {} ", msg),
                Style::default()
                    .fg(THEME.accent_secondary)
                    .bg(Color::Rgb(20, 25, 30)),
            ));
            showing_log = true;
        }
    }

    if app.is_dragging {
        if let Some(src) = &app.drag_source {
            let name = src.file_name().and_then(|n| n.to_str()).unwrap_or("...");
            left_spans.push(Span::styled(
                " DRAGGING ",
                Style::default()
                    .fg(Color::Black)
                    .bg(THEME.accent_primary)
                    .add_modifier(Modifier::BOLD),
            ));
            left_spans.push(Span::styled(
                format!(" {} ", name),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ));

            if let Some(target) = &app.hovered_drop_target {
                left_spans.push(Span::raw(" to "));
                let target_desc = match target {
                    DropTarget::Folder(p) => {
                        p.file_name().and_then(|n| n.to_str()).unwrap_or("Folder")
                    }
                    DropTarget::Pane(idx) => {
                        if *idx == 0 {
                            "Left Pane"
                        } else {
                            "Right Pane"
                        }
                    }
                    DropTarget::Favorites => "Favorites",
                    DropTarget::RemotesHeader => "Remotes",
                    DropTarget::ImportServers => "Import Servers",
                    DropTarget::ReorderFavorite(_) => "Favorites (Reorder)",
                    DropTarget::SidebarArea => "Sidebar",
                };
                left_spans.push(Span::styled(
                    format!(" {} ", target_desc),
                    Style::default()
                        .fg(Color::Rgb(0, 255, 200))
                        .add_modifier(Modifier::BOLD),
                ));
            }
            showing_log = true; // Use this to skip shortcuts
        }
    }

    if !showing_log {
        left_spans.extend(HotkeyHint::render("^Q", "Quit", Color::Red));

        let hidden_on = if let Some(fs) = app.current_file_state() {
            fs.show_hidden
        } else {
            app.default_show_hidden
        };

        let mut shortcuts = Vec::new();
        if app.current_view == CurrentView::Editor {
            shortcuts.extend(HotkeyHint::render("Esc", "Back", THEME.accent_primary));
            shortcuts.extend(HotkeyHint::render("^B", "Sidebar", THEME.accent_secondary));
            shortcuts.extend(HotkeyHint::render("^P", "Split", THEME.accent_secondary));
            shortcuts.extend(HotkeyHint::render("^F", "Find", THEME.accent_secondary));
            shortcuts.extend(HotkeyHint::render("^R", "Replace", THEME.accent_secondary));
            shortcuts.extend(HotkeyHint::render("^G", "GoTo", THEME.accent_secondary));
        } else {
            shortcuts.extend(HotkeyHint::render("^P", "Split", THEME.accent_secondary));
            shortcuts.extend(HotkeyHint::render("^T", "Tab", THEME.accent_secondary));
            shortcuts.extend(HotkeyHint::render("^N", "TermTab", THEME.accent_secondary));
            shortcuts.extend(HotkeyHint::render("^K", "TermWin", THEME.accent_secondary));
            shortcuts.extend(HotkeyHint::render(
                "^H",
                "Hidden",
                if hidden_on { Color::Green } else { Color::Red },
            ));
            shortcuts.extend(HotkeyHint::render(
                "Space",
                "Editor",
                Color::Rgb(88, 166, 255),
            )); // GitHub Blue
        }

        for s in shortcuts {
            left_spans.push(s);
        }

        // Add Remote Status Badge
        let is_remote = app.panes.iter().any(|p| {
            if let Some(fs) = p.current_state() {
                fs.remote_session.is_some()
            } else {
                false
            }
        });

        if is_remote {
            left_spans.push(Span::raw(" │ "));
            left_spans.push(Span::styled(
                " REMOTE ",
                Style::default()
                    .bg(THEME.accent_secondary)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(left_spans)).wrap(ratatui::widgets::Wrap { trim: false }),
        top_chunks[0],
    );

    // 2. Center Section: Selection Summary (Only in Files view)
    if app.current_view != CurrentView::Editor {
        if let Some(fs) = app.current_file_state() {
            let sel_count = if !fs.selection.is_empty() {
                fs.selection.multi.len()
            } else if fs.selection.selected.is_some() {
                1
            } else {
                0
            };
            let total_count = fs.files.len();
            let summary = format!(" SEL: {} / {} ", sel_count, total_count);
            f.render_widget(
                Paragraph::new(Span::styled(
                    summary,
                    Style::default()
                        .bg(THEME.accent_primary)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                ))
                .alignment(ratatui::layout::Alignment::Right),
                top_chunks[1],
            );
        }
    }

    // 3. Stats (CPU/MEM) - Far Right
    let cpu_bar = draw_stat_bar("CPU", app.system_state.cpu_usage, 100.0);
    let mem_usage =
        (app.system_state.mem_usage / app.system_state.total_mem.max(1.0)) as f32 * 100.0;
    let mem_bar = draw_stat_bar("MEM", mem_usage, 100.0);

    let stats_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Length(22),
            Constraint::Fill(1),
        ])
        .split(top_chunks[2]);

    f.render_widget(
        Paragraph::new(cpu_bar).alignment(ratatui::layout::Alignment::Right),
        stats_layout[0],
    );
    f.render_widget(
        Paragraph::new(mem_bar).alignment(ratatui::layout::Alignment::Right),
        stats_layout[1],
    );

    // 4. CYBER_PULSE (Animated Indicator)
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let pulse_frames = [
        " ", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂",
    ];
    let pulse_idx = (time / 80) % pulse_frames.len() as u128;
    let pulse_char = pulse_frames[pulse_idx as usize];

    let pulse_spans = vec![
        Span::styled(" PULSE ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            pulse_char.repeat(3),
            Style::default().fg(THEME.accent_primary),
        ),
    ];

    f.render_widget(
        Paragraph::new(Line::from(pulse_spans)).alignment(ratatui::layout::Alignment::Right),
        stats_layout[2],
    );

    // 5. Bottom Line: Background Tasks
    let mut task_spans = Vec::new();
    for task in &app.background_tasks {
        let pct = (task.progress * 100.0) as usize;
        let bar = "█".repeat(pct / 10) + &"░".repeat(10 - (pct / 10));
        task_spans.push(Span::styled(
            format!(" {} [{}%] ", task.name, pct),
            Style::default().fg(Color::Cyan),
        ));
        task_spans.push(Span::styled(
            format!("{} ", bar),
            Style::default().fg(Color::Cyan),
        ));
    }

    if !task_spans.is_empty() {
        f.render_widget(Paragraph::new(Line::from(task_spans)), chunks[1]);
    }
}

fn draw_context_menu(
    f: &mut Frame,
    x: u16,
    y: u16,
    target: &crate::app::ContextMenuTarget,
    app: &App,
) {
    use crate::app::ContextMenuAction;
    let mut items = Vec::new();

    let actions = if let AppMode::ContextMenu { actions, .. } = &app.mode {
        actions.clone()
    } else {
        vec![]
    };

    let selected_idx = if let AppMode::ContextMenu { selected_index, .. } = &app.mode {
        *selected_index
    } else {
        None
    };

    for (i, action) in actions.iter().enumerate() {
        let label = match action {
            ContextMenuAction::Open => format!(" {} Open", Icon::Folder.get(app.icon_mode)),
            ContextMenuAction::OpenNewTab => {
                format!(" {} Open in New Tab", Icon::Split.get(app.icon_mode))
            }
            ContextMenuAction::OpenWith => {
                format!(" {} Open With...", Icon::Split.get(app.icon_mode))
            }
            ContextMenuAction::Edit => format!(" {} Edit", Icon::Document.get(app.icon_mode)),
            ContextMenuAction::Run => format!(" {} Run", Icon::Video.get(app.icon_mode)),
            ContextMenuAction::RunTerminal => {
                format!(" {} Run in Terminal", Icon::Script.get(app.icon_mode))
            }
            ContextMenuAction::ExtractHere => {
                format!(" {} Extract Here", Icon::Archive.get(app.icon_mode))
            }
            ContextMenuAction::NewFolder => {
                format!(" {} New Folder", Icon::Folder.get(app.icon_mode))
            }
            ContextMenuAction::NewFile => format!(" {} New File", Icon::File.get(app.icon_mode)),
            ContextMenuAction::Cut => format!(" {} Cut", Icon::Cut.get(app.icon_mode)),
            ContextMenuAction::Copy => format!(" {} Copy", Icon::Copy.get(app.icon_mode)),
            ContextMenuAction::CopyPath => format!(" {} Copy Path", Icon::Copy.get(app.icon_mode)),
            ContextMenuAction::CopyName => format!(" {} Copy Name", Icon::Copy.get(app.icon_mode)),
            ContextMenuAction::Paste => format!(" {} Paste", Icon::Paste.get(app.icon_mode)),
            ContextMenuAction::Rename => format!(" {} Rename", Icon::Rename.get(app.icon_mode)),
            ContextMenuAction::Duplicate => {
                format!(" {} Duplicate", Icon::Duplicate.get(app.icon_mode))
            }
            ContextMenuAction::Compress => {
                format!(" {} Compress", Icon::Archive.get(app.icon_mode))
            }
            ContextMenuAction::Delete => format!(" {} Delete", Icon::Delete.get(app.icon_mode)),
            ContextMenuAction::AddToFavorites => {
                format!(" {} Add to Favorites", Icon::Star.get(app.icon_mode))
            }
            ContextMenuAction::RemoveFromFavorites => {
                format!(" {} Remove from Favorites", Icon::Star.get(app.icon_mode))
            }
            ContextMenuAction::Properties => {
                format!(" {} Properties", Icon::Document.get(app.icon_mode))
            }
            ContextMenuAction::TerminalWindow => {
                format!(" {} New Terminal Window", Icon::Script.get(app.icon_mode))
            }
            ContextMenuAction::TerminalTab => {
                format!(" {} New Terminal Tab", Icon::Script.get(app.icon_mode))
            }
            ContextMenuAction::Refresh => format!(" {} Refresh", Icon::Refresh.get(app.icon_mode)),
            ContextMenuAction::SelectAll => {
                format!(" {} Select All", Icon::SelectAll.get(app.icon_mode))
            }
            ContextMenuAction::ToggleHidden => {
                format!(" {} Toggle Hidden", Icon::ToggleHidden.get(app.icon_mode))
            }
            ContextMenuAction::ConnectRemote => {
                format!(" {} Connect", Icon::Remote.get(app.icon_mode))
            }
            ContextMenuAction::DeleteRemote => {
                format!(" {} Delete Bookmark", Icon::Delete.get(app.icon_mode))
            }
            ContextMenuAction::Mount => format!(" {} Mount", Icon::Storage.get(app.icon_mode)),
            ContextMenuAction::Unmount => format!(" {} Unmount", Icon::Storage.get(app.icon_mode)),
            ContextMenuAction::SetWallpaper => {
                format!(" {} Set as Wallpaper", Icon::Image.get(app.icon_mode))
            }
            ContextMenuAction::GitInit => format!(" {} Git Init", Icon::Git.get(app.icon_mode)),
            ContextMenuAction::GitStatus => format!(" {} Git Status", Icon::Git.get(app.icon_mode)),
            ContextMenuAction::SystemMonitor => {
                format!(" {} System Monitor", Icon::Monitor.get(app.icon_mode))
            }
            ContextMenuAction::Drag => {
                format!(" {} Drag...", Icon::Remote.get(app.icon_mode))
            }
            ContextMenuAction::SetColor(_) => {
                format!(" {} Highlight...", Icon::Image.get(app.icon_mode))
            }
            ContextMenuAction::SortBy(col) => {
                let name = match col {
                    crate::app::FileColumn::Name => "Name",
                    crate::app::FileColumn::Size => "Size",
                    crate::app::FileColumn::Modified => "Date",
                    _ => "Unknown",
                };
                let mut label = format!(" 󰒺 Sort by {}", name);
                if let Some(fs) = app.current_file_state() {
                    if fs.sort_column == *col {
                        label.push_str(if fs.sort_ascending {
                            " (▲)"
                        } else {
                            " (▼)"
                        });
                    }
                }
                label
            }
            ContextMenuAction::Separator => " ────────────────".to_string(),
        };

        let style = if Some(i) == selected_idx {
            Style::default()
                .bg(THEME.accent_primary)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(THEME.fg)
        };

        let mut item = ListItem::new(label).style(style);
        if (*action == ContextMenuAction::Paste) && app.clipboard.is_none() {
            item = item.style(Style::default().fg(Color::DarkGray));
        }
        if *action == ContextMenuAction::Separator {
            item = item.style(Style::default().fg(Color::DarkGray));
        }
        items.push(item);
    }

    let title = match target {
        crate::app::ContextMenuTarget::File(_) => " File ",
        crate::app::ContextMenuTarget::Folder(_) => " Folder ",
        crate::app::ContextMenuTarget::EmptySpace => " View ",
        crate::app::ContextMenuTarget::SidebarFavorite(_) => " Favorite ",
        crate::app::ContextMenuTarget::SidebarRemote(_) => " Remote ",
        crate::app::ContextMenuTarget::SidebarStorage(_) => " Storage ",
        crate::app::ContextMenuTarget::ProjectTree(_) => " Project ",
        crate::app::ContextMenuTarget::Process(_) => " Process ",
    };

    let menu_width = 30;
    let menu_height = items.len() as u16 + 2;
    let mut draw_x = x;
    let mut draw_y = y;
    if draw_x + menu_width > f.area().width {
        draw_x = f.area().width.saturating_sub(menu_width);
    }
    if draw_y + menu_height > f.area().height {
        draw_y = f.area().height.saturating_sub(menu_height);
    }

    let area = Rect::new(draw_x, draw_y, menu_width, menu_height);

    f.render_widget(Clear, area);
    let menu_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_secondary));

    // Use full width of inner area, just offset X by 1 for padding
    let inner_area = menu_block.inner(area);
    let padded_area = Rect::new(
        inner_area.x,
        inner_area.y,
        inner_area.width,
        inner_area.height,
    );

    f.render_widget(menu_block, area);
    f.render_widget(List::new(items), padded_area);
}

fn draw_import_servers_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Import Servers (TOML) ")
        .border_style(Style::default().fg(THEME.accent_primary));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .split(inner);

    f.render_widget(
        Paragraph::new("Enter path to server configuration file:"),
        chunks[0],
    );

    let input_area = chunks[1];
    f.render_widget(
        Paragraph::new("> ").style(Style::default().fg(THEME.accent_secondary)),
        Rect::new(input_area.x, input_area.y, 2, 1),
    );
    f.render_widget(
        &app.input,
        Rect::new(
            input_area.x + 2,
            input_area.y,
            input_area.width.saturating_sub(2),
            1,
        ),
    );

    let example_toml = r#"Example format:
[[servers]]
name = "Production"
host = "192.168.1.10"
user = "admin"
port = 22"#;

    f.render_widget(
        Paragraph::new(example_toml).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );

    let mut footer_text = Vec::new();
    footer_text.extend(HotkeyHint::render("Enter", "Import", Color::Green));
    footer_text.extend(HotkeyHint::render("Esc", "Cancel", Color::Red));

    f.render_widget(Paragraph::new(Line::from(footer_text)), chunks[3]);
}

fn draw_command_palette(f: &mut Frame, app: &mut App) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);
    let inner = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Command Palette ")
        .border_style(Style::default().fg(Color::Magenta))
        .inner(area);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(" Command Palette ")
            .border_style(Style::default().fg(Color::Magenta)),
        area,
    );

    f.render_widget(
        Paragraph::new("> ").style(Style::default().fg(Color::Yellow)),
        Rect::new(inner.x, inner.y, 2, 1),
    );
    f.render_widget(
        &app.input,
        Rect::new(inner.x + 2, inner.y, inner.width.saturating_sub(2), 1),
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
            ListItem::new(cmd.desc.clone()).style(style)
        })
        .collect();
    f.render_widget(
        List::new(items),
        Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1),
    );
}

fn draw_rename_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Rename ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.rename_selected {
        let text = if let Some(idx) = app.input.value.rfind('.') {
            if idx > 0 {
                let stem_part = &app.input.value[..idx];
                let ext_part = &app.input.value[idx..];
                Line::from(vec![
                    Span::styled(
                        stem_part,
                        Style::default().bg(THEME.accent_primary).fg(Color::Black),
                    ),
                    Span::raw(ext_part),
                ])
            } else {
                Line::from(vec![Span::styled(
                    &app.input.value,
                    Style::default().bg(THEME.accent_primary).fg(Color::Black),
                )])
            }
        } else {
            Line::from(vec![Span::styled(
                &app.input.value,
                Style::default().bg(THEME.accent_primary).fg(Color::Black),
            )])
        };
        f.render_widget(Paragraph::new(text), inner);
    } else {
        f.render_widget(&app.input, inner);
    }
}

fn draw_new_folder_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" New Folder ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);
}

fn draw_new_file_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" New File ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);
}

fn draw_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);

    let title = if let AppMode::DeleteFile(ref path) = app.mode {
        format!(
            " Delete {}? ",
            path.file_name().unwrap_or_default().to_string_lossy()
        )
    } else {
        " Delete selected items? ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Message
    f.render_widget(
        Paragraph::new(format!("Confirm deletion? [Y/n]: {}", app.input.value))
            .alignment(Alignment::Center),
        inner, // Use inner area, maybe offset y? Defaults to top.
    );

    // Buttons
    let (mx, my) = app.mouse_pos;
    let button_y = inner.y + inner.height.saturating_sub(2);

    let is_hover =
        |bx: u16, len: u16| mx >= inner.x + bx && mx < inner.x + bx + len && my == button_y;

    // [ YES ] at x=5 (width 9)
    // [ NO ]  at x=25 (width 8)

    let yes_style = if is_hover(5, 9) {
        Style::default()
            .bg(Color::Red)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    };

    let no_style = if is_hover(25, 8) {
        Style::default()
            .bg(Color::White)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    f.render_widget(
        Paragraph::new(" [ YES ] ").style(yes_style),
        Rect::new(inner.x + 5, button_y, 9, 1),
    );

    f.render_widget(
        Paragraph::new(" [ NO ] ").style(no_style),
        Rect::new(inner.x + 25, button_y, 8, 1),
    );
}

fn draw_properties_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 50, f.area());
    f.render_widget(Clear, area);

    let mut text = Vec::new();

    if let Some(fs) = app.current_file_state() {
        let target_path = fs
            .selection
            .selected
            .and_then(|idx| fs.files.get(idx))
            .unwrap_or(&fs.current_path);

        let name = target_path
            .file_name()
            .map(|n: &std::ffi::OsStr| n.to_string_lossy().to_string())
            .unwrap_or_else(|| target_path.to_string_lossy().to_string());
        let parent = target_path
            .parent()
            .map(|p: &std::path::Path| p.to_string_lossy().to_string())
            .unwrap_or_default();

        text.push(Line::from(vec![
            Span::styled("Name: ", Style::default().fg(THEME.accent_secondary)),
            Span::raw(name),
        ]));
        text.push(Line::from(vec![
            Span::styled("Location: ", Style::default().fg(THEME.accent_secondary)),
            Span::raw(parent),
        ]));
        text.push(Line::from(""));

        if let Some(meta) = fs.metadata.get(target_path) {
            let type_str = if meta.is_dir { "Folder" } else { "File" };
            text.push(Line::from(vec![
                Span::styled("Type: ", Style::default().fg(THEME.accent_secondary)),
                Span::raw(type_str),
            ]));
            text.push(Line::from(vec![
                Span::styled("Size: ", Style::default().fg(THEME.accent_secondary)),
                Span::raw(format_size(meta.size)),
            ]));
            text.push(Line::from(vec![
                Span::styled("Modified: ", Style::default().fg(THEME.accent_secondary)),
                Span::raw(format_time(meta.modified)),
            ]));
            text.push(Line::from(vec![
                Span::styled("Created: ", Style::default().fg(THEME.accent_secondary)),
                Span::raw(format_time(meta.created)),
            ]));
            text.push(Line::from(vec![
                Span::styled("Permissions: ", Style::default().fg(THEME.accent_secondary)),
                Span::raw(format_permissions(meta.permissions)),
            ]));
        } else {
            if fs.remote_session.is_none() {
                if let Ok(m) = std::fs::metadata(target_path) {
                    let is_dir = m.is_dir();
                    text.push(Line::from(vec![
                        Span::styled("Type: ", Style::default().fg(THEME.accent_secondary)),
                        Span::raw(if is_dir { "Folder" } else { "File" }),
                    ]));
                    text.push(Line::from(vec![
                        Span::styled("Size: ", Style::default().fg(THEME.accent_secondary)),
                        Span::raw(format_size(m.len())),
                    ]));
                    if let Ok(mod_time) = m.modified() {
                        text.push(Line::from(vec![
                            Span::styled("Modified: ", Style::default().fg(THEME.accent_secondary)),
                            Span::raw(format_time(mod_time)),
                        ]));
                    }
                } else {
                    text.push(Line::from(Span::styled(
                        "No metadata available",
                        Style::default().fg(Color::DarkGray),
                    )));
                }
            } else {
                text.push(Line::from(Span::styled(
                    "No metadata available (Remote)",
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }
    }

    let block = Block::default()
        .title(" Properties ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_primary));
    f.render_widget(Paragraph::new(text).block(block), area);
}

fn draw_settings_modal(f: &mut Frame, app: &App) {
    let area = f.area();

    f.render_widget(Clear, area);

    let block = Block::default()
        .title_top(Line::from(vec![Span::styled(
            " SETTINGS ",
            Style::default()
                .fg(Color::Black)
                .bg(THEME.accent_primary)
                .add_modifier(Modifier::BOLD),
        )]))
        .title_top(
            Line::from(vec![
                Span::styled(
                    " Esc ",
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Back ", Style::default().fg(Color::Red)),
            ])
            .alignment(Alignment::Right),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_primary))
        .style(Style::default().bg(Color::Rgb(0, 0, 0)));

    let inner = block.inner(area);

    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(0)])
        .split(inner);

    let sections = vec![
        ListItem::new(" 󰟜  Columns "),
        ListItem::new(" 󰓩  Tabs "),
        ListItem::new(" 󰒓  General "),
        ListItem::new(" 󰒍  Remotes "),
        ListItem::new(" 󰌌  Shortcuts "),
    ];

    let sel = match app.settings_section {
        SettingsSection::Columns => 0,
        SettingsSection::Tabs => 1,
        SettingsSection::General => 2,
        SettingsSection::Remotes => 3,
        SettingsSection::Shortcuts => 4,
    };
    let items: Vec<ListItem> = sections
        .into_iter()
        .enumerate()
        .map(|(i, item)| {
            if i == sel {
                item.style(
                    Style::default()
                        .bg(THEME.accent_primary)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                item
            }
        })
        .collect();
    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        chunks[0],
    );
    match app.settings_section {
        SettingsSection::Columns => draw_column_settings(f, chunks[1], app),
        SettingsSection::Tabs => draw_tab_settings(f, chunks[1], app),
        SettingsSection::General => draw_general_settings(f, chunks[1], app),
        SettingsSection::Remotes => draw_remote_settings(f, chunks[1], app),
        SettingsSection::Shortcuts => draw_shortcuts_settings(f, chunks[1], app),
    }
}

fn draw_shortcuts_settings(f: &mut Frame, area: Rect, _app: &App) {
    let shortcuts = vec![
        (
            "General",
            vec![
                ("Ctrl + q", "Quit Application"),
                ("Ctrl + g", "Open Settings"),
                ("Ctrl + Space", "Open Command Palette"),
                ("Ctrl + b", "Toggle Sidebar"),
                ("Ctrl + m", "Toggle Main Stage"),
                ("Ctrl + i", "Information"),
            ],
        ),
        (
            "Navigation",
            vec![
                ("↑ / ↓", "Move Selection"),
                ("Left / Right", "Change Pane / Enter/Leave Sidebar"),
                ("Enter", "Open Directory / File"),
                ("Shift + Enter", "Open Folder in New Tab"),
                ("Backspace", "Go to Parent Directory"),
                ("Alt + Left / Right", "Back / Forward in History"),
                ("~", "Go to Home Directory"),
                ("Middle Click / Space", "Editor"),
            ],
        ),
        (
            "View & Tabs",
            vec![
                ("Ctrl + s", "Toggle Split View"),
                ("Ctrl + t", "New Duplicate Tab"),
                ("Ctrl + h", "Toggle Hidden Files"),
                ("Ctrl + b", "Toggle Sidebar"),
                ("Ctrl + l / u", "Clear Search Filter"),
                ("Ctrl + z / y", "Undo / Redo (Rename/Move)"),
                ("?", "Show this Help"),
                ("Esc / Ctrl + [", "Back / Exit Mode"),
            ],
        ),
        (
            "File Operations",
            vec![
                ("Ctrl + c / Ins", "Copy Selected"),
                ("Ctrl + x / Shift+Del", "Cut Selected"),
                ("Ctrl + v / Shift+Ins", "Paste Selected"),
                ("Ctrl + a", "Select All"),
                ("F2", "Rename Selected"),
                ("Delete", "Delete Selected"),
                ("Shift + Delete", "Permanent Delete"),
                ("Alt + Enter", "Show Properties"),
            ],
        ),
        (
            "Editor",
            vec![
                ("Alt + Up/Down", "Move Line Up/Down"),
                ("Ctrl + Bksp / W", "Delete Word Backward"),
                ("Ctrl + Delete", "Delete Word Forward"),
                ("Ctrl + G", "Go to Line"),
                ("Ctrl + F", "Find in File"),
                ("Ctrl + R / F2", "Replace"),
                ("Double Click", "Select Word"),
                ("Triple Click", "Select Line"),
                ("Drag Selection", "Move Text Block"),
            ],
        ),
        (
            "Terminal",
            vec![
                ("Ctrl + n", "Open Terminal Tab"),
                ("Ctrl + . / Ctrl + k", "New Terminal Window"),
            ],
        ),
    ];

    let mut rows = Vec::new();
    for (category, items) in shortcuts {
        rows.push(Row::new(vec![
            Cell::from(Span::styled(
                category,
                Style::default()
                    .fg(THEME.accent_primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Cell::from(""),
        ]));
        for (key, desc) in items {
            rows.push(Row::new(vec![
                Cell::from(Span::styled(key, Style::default().fg(Color::Yellow))),
                Cell::from(desc),
            ]));
        }
        rows.push(Row::new(vec![Cell::from(""), Cell::from("")])); // Spacer
    }

    let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(0)]).block(
        Block::default()
            .title(" Keyboard Shortcuts ")
            .borders(Borders::NONE),
    );

    f.render_widget(table, area);
}

fn draw_column_settings(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);
    let titles = vec![" [Single] ", " [Split] "];
    let sel = match app.settings_target {
        SettingsTarget::SingleMode => 0,
        SettingsTarget::SplitMode => 1,
    };
    f.render_widget(
        Tabs::new(titles)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .title(" Configure Mode "),
            )
            .select(sel)
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        chunks[0],
    );
    let options = [
        (FileColumn::Size, "Size (s)"),
        (FileColumn::Modified, "Modified (m)"),
        (FileColumn::Created, "Created (c)"),
        (FileColumn::Permissions, "Permissions (p)"),
    ];
    let target = match app.settings_target {
        SettingsTarget::SingleMode => &app.single_columns,
        SettingsTarget::SplitMode => &app.split_columns,
    };
    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (col, label))| {
            let prefix = if target.contains(col) { "[x] " } else { "[ ] " };
            let mut style = Style::default().fg(THEME.fg);
            if i == app.settings_index && app.settings_section == SettingsSection::Columns {
                style = Style::default()
                    .bg(THEME.accent_primary)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD);
            }
            ListItem::new(format!("{}{}", prefix, label)).style(style)
        })
        .collect();
    f.render_widget(
        List::new(items).block(
            Block::default()
                .title(" Visible Columns ")
                .borders(Borders::NONE),
        ),
        chunks[1],
    );
}

fn draw_tab_settings(f: &mut Frame, area: Rect, app: &App) {
    let mut rows = Vec::new();
    let mut tab_counter = 0;

    for (p_idx, pane) in app.panes.iter().enumerate() {
        rows.push(Row::new(vec![
            Cell::from(Span::styled(
                format!("PANE {}", p_idx + 1),
                Style::default()
                    .fg(THEME.accent_secondary)
                    .add_modifier(Modifier::BOLD),
            )),
            Cell::from(""),
            Cell::from(""),
        ]));

        for (t_idx, tab) in pane.tabs.iter().enumerate() {
            let is_selected =
                tab_counter == app.settings_index && app.settings_section == SettingsSection::Tabs;
            let mut style = Style::default().fg(THEME.fg);
            if is_selected {
                style = style
                    .bg(THEME.accent_primary)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD);
            }

            let is_active = t_idx == pane.active_tab_index;
            let status = if is_active {
                " [ACTIVE] "
            } else {
                "          "
            };
            let status_style = if is_active {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            rows.push(Row::new(vec![
                Cell::from(format!("  Tab {}", t_idx + 1)).style(style),
                Cell::from(tab.current_path.to_string_lossy().to_string()).style(style),
                Cell::from(status).style(if is_selected { style } else { status_style }),
            ]));
            tab_counter += 1;
        }
        rows.push(Row::new(vec![
            Cell::from(""),
            Cell::from(""),
            Cell::from(""),
        ])); // Spacer
    }

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Fill(1),
            Constraint::Length(12),
        ],
    )
    .header(
        Row::new(vec![" TAB ", " PATH ", " STATUS "]).style(
            Style::default()
                .fg(THEME.accent_secondary)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(
        Block::default()
            .title(" OPEN TABS MANAGEMENT ")
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::Rgb(40, 45, 55))),
    )
    .column_spacing(2);

    f.render_widget(table, area);
}

fn draw_general_settings(f: &mut Frame, area: Rect, app: &App) {
    struct GeneralOption {
        label: &'static str,
        status: String,
        key: &'static str,
        bool_state: Option<bool>,
    }
    let options = [
        GeneralOption {
            label: "Show Hidden Files",
            status: if app.default_show_hidden {
                "ENABLED ".to_string()
            } else {
                "DISABLED".to_string()
            },
            key: "h",
            bool_state: Some(app.default_show_hidden),
        },
        GeneralOption {
            label: "Confirm Delete",
            status: if app.confirm_delete {
                "ENABLED ".to_string()
            } else {
                "DISABLED".to_string()
            },
            key: "d",
            bool_state: Some(app.confirm_delete),
        },
        GeneralOption {
            label: "Smart Date Formatting",
            status: if app.smart_date {
                "ENABLED ".to_string()
            } else {
                "DISABLED".to_string()
            },
            key: "t",
            bool_state: Some(app.smart_date),
        },
        GeneralOption {
            label: "Semantic Coloring",
            status: if app.semantic_coloring {
                "ENABLED ".to_string()
            } else {
                "DISABLED".to_string()
            },
            key: "s",
            bool_state: Some(app.semantic_coloring),
        },
        GeneralOption {
            label: "Auto Save",
            status: if app.auto_save {
                "ENABLED ".to_string()
            } else {
                "DISABLED".to_string()
            },
            key: "a",
            bool_state: Some(app.auto_save),
        },
        GeneralOption {
            label: "Preview Max Size",
            status: format!("{} MB", app.preview_max_mb),
            key: "p",
            bool_state: None,
        },
        GeneralOption {
            label: "Icon Mode",
            status: format!("{:?}", app.icon_mode),
            key: "i",
            bool_state: None,
        },
    ];

    let rows: Vec<_> = options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let is_selected =
                i == app.settings_index && app.settings_section == SettingsSection::General;
            let mut style = Style::default().fg(THEME.fg);
            let mut status_style = match opt.bool_state {
                Some(true) => Style::default().fg(Color::Green),
                Some(false) => Style::default().fg(Color::Red),
                None => Style::default().fg(Color::Cyan),
            };

            if is_selected {
                style = style
                    .bg(THEME.accent_primary)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD);
                status_style = status_style
                    .bg(THEME.accent_primary)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD);
            }

            Row::new(vec![
                Cell::from(format!("  {}", opt.label)).style(style),
                Cell::from(format!(" [ {} ] ", opt.status)).style(status_style),
                Cell::from(format!("({})", opt.key)).style(if is_selected {
                    style
                } else {
                    Style::default().fg(Color::DarkGray)
                }),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Fill(1),
            Constraint::Length(15),
            Constraint::Length(5),
        ],
    )
    .block(
        Block::default()
            .title(" SYSTEM PARAMETERS ")
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::Rgb(40, 45, 55))),
    )
    .column_spacing(2);

    f.render_widget(table, area);
}

fn draw_remote_settings(f: &mut Frame, area: Rect, app: &App) {
    let rows: Vec<_> = app
        .remote_bookmarks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let is_selected =
                i == app.settings_index && app.settings_section == SettingsSection::Remotes;
            let mut style = Style::default().fg(THEME.fg);
            if is_selected {
                style = style
                    .bg(THEME.accent_primary)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD);
            }

            let icon = Icon::Remote.get(app.icon_mode);
            Row::new(vec![
                Cell::from(format!(" {} {}", icon, b.name)).style(style),
                Cell::from(format!("{}@{}", b.user, b.host)).style(style),
                Cell::from(b.port.to_string()).style(style),
                Cell::from(b.last_path.to_string_lossy().to_string()).style(style),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Fill(1),
        ],
    )
    .header(
        Row::new(vec![" NAME ", " CONNECTION ", " PORT ", " LAST PATH "]).style(
            Style::default()
                .fg(THEME.accent_secondary)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(
        Block::default()
            .title(" REMOTE SERVER BOOKMARKS ")
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::Rgb(40, 45, 55))),
    )
    .column_spacing(2);

    let text = vec![
        Line::from("Manage your remote server bookmarks here."),
        Line::from(vec![
            Span::raw("Tip: Import servers by clicking "),
            Span::styled(
                " REMOTES [Import] ",
                Style::default()
                    .fg(THEME.accent_secondary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" in the sidebar."),
        ]),
        Line::from("Format (TOML): [[servers]] name=\"...\" host=\"...\" user=\"...\" port=22"),
        Line::from(""),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    f.render_widget(Paragraph::new(text), chunks[0]);

    if app.remote_bookmarks.is_empty() {
        f.render_widget(
            Paragraph::new("\n (No remote servers configured)")
                .style(Style::default().fg(Color::DarkGray)),
            chunks[1],
        );
    } else {
        f.render_widget(table, chunks[1]);
    }
}

fn draw_add_remote_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 50, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Add Remote Server ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Name
            Constraint::Length(3), // Host
            Constraint::Length(3), // User
            Constraint::Length(3), // Port
            Constraint::Length(3), // Key Path
            Constraint::Min(0),    // Help
        ])
        .split(inner);

    let active_idx = if let AppMode::AddRemote(idx) = app.mode {
        idx
    } else {
        0
    };

    let fields = [
        ("Name", &app.pending_remote.name),
        ("Host", &app.pending_remote.host),
        ("User", &app.pending_remote.user),
        ("Port", &app.pending_remote.port.to_string()),
        (
            "Key Path",
            &app.pending_remote
                .key_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        ),
    ];

    for (i, (label, value)) in fields.iter().enumerate() {
        let is_active = i == active_idx;
        let mut style = Style::default().fg(Color::DarkGray);
        if is_active {
            style = Style::default().fg(Color::Yellow);
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", label))
            .border_style(style);
        let field_area = chunks[i];

        if is_active {
            f.render_widget(
                Paragraph::new(app.input.value.as_str()).block(block),
                field_area,
            );
        } else {
            f.render_widget(Paragraph::new(value.as_str()).block(block), field_area);
        }
    }

    let help_text = vec![
        Line::from(vec![
            Span::styled(
                " [Tab/Enter] ",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("Next Field  "),
            Span::styled(
                " [Esc] ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("Cancel"),
        ]),
        Line::from("On the last field, [Enter] will save the bookmark."),
    ];
    f.render_widget(Paragraph::new(help_text), chunks[5]);
}

fn draw_highlight_modal(f: &mut Frame, _app: &App) {
    // Actually let's use absolute sizing for palette
    let area = Rect::new(
        (f.area().width.saturating_sub(34)) / 2,
        (f.area().height.saturating_sub(5)) / 2,
        34,
        5,
    );

    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Highlight ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_primary));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let colors = [
        (1, " R ", Color::Red),
        (2, " G ", Color::Green),
        (3, " Y ", Color::Yellow),
        (4, " B ", Color::Blue),
        (5, " M ", Color::Magenta),
        (6, " C ", Color::Cyan),
        (0, " X ", Color::Reset),
    ];

    let mut spans = Vec::new();
    for (i, (code, label, color)) in colors.iter().enumerate() {
        let style = if *code == 0 {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default().bg(*color).fg(Color::Black)
        };
        spans.push(Span::styled(*label, style));
        if i < colors.len() - 1 {
            spans.push(Span::raw(" "));
        }
    }

    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(ratatui::layout::Alignment::Center),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    f.render_widget(
        Paragraph::new("1   2   3   4   5   6   0")
            .alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );
}

fn format_modified_time(time: SystemTime) -> String {
    use chrono::{DateTime, Local};
    let dt: DateTime<Local> = time.into();
    let now = Local::now();

    if dt.date_naive() == now.date_naive() {
        dt.format("%H:%M:%S").to_string()
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

fn draw_drag_ghost(f: &mut Frame, app: &App) {
    if let Some(path) = &app.drag_source {
        let (col, row) = app.mouse_pos;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        // Truncate name if too long
        let max_len = 20;
        let display_name = if name.len() > max_len {
            format!("{}...", &name[..max_len])
        } else {
            name.to_string()
        };

        let text = format!(" {} ", display_name);
        let width = text.len() as u16;

        // Draw slightly offset from cursor
        let x = col
            .saturating_add(2)
            .min(f.area().width.saturating_sub(width));
        let y = row.saturating_add(1).min(f.area().height.saturating_sub(1));

        let area = Rect::new(x, y, width, 1);

        f.render_widget(Clear, area);
        f.render_widget(
            Paragraph::new(Span::styled(
                text,
                Style::default()
                    .bg(THEME.accent_primary)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            )),
            area,
        );
    }
}
