use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Tabs,
    },
    Frame,
};
use std::time::SystemTime;

use crate::app::{
    App, AppMode, FileColumn, MonitorSubview, SettingsSection, SettingsTarget,
};
use crate::icons::Icon;
use crate::ui::theme::THEME;
use terma::layout::centered_rect;
use terma::widgets::HotkeyHint;
use terma::utils::{
    format_permissions, format_size, format_time, truncate_to_width,
};

pub fn draw_drag_drop_modal(
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
        for i in 0..std::cmp::min(sources.len(), 3) {
            let name = sources[i].file_name().unwrap_or_default().to_string_lossy();
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

pub fn draw_hotkeys_modal(f: &mut Frame, _area: Rect) {
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
                ("Space", "Preview File / Folder Props"),
                ("Backspace", "Go Up Directory"),
                ("Home / ~", "Go Home"),
                ("Alt + Left/Right", "Resize Sidebar"),
                ("F6", "Rename File"),
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

pub fn draw_open_with_modal(f: &mut Frame, app: &App, path: &std::path::Path) {
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
    let mut suggestions = crate::get_open_with_suggestions(app, &ext);

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

pub fn draw_import_servers_modal(f: &mut Frame, app: &App) {
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
    footer_text.extend(HotkeyHint::new("Enter", "Import", Color::Green));
    footer_text.extend(HotkeyHint::new("Esc", "Cancel", Color::Red));

    f.render_widget(Paragraph::new(Line::from(footer_text)), chunks[3]);
}

pub fn draw_command_palette(f: &mut Frame, app: &mut App) {
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

pub fn draw_rename_modal(f: &mut Frame, app: &App) {
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

pub fn draw_new_folder_modal(f: &mut Frame, app: &App) {
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

pub fn draw_new_file_modal(f: &mut Frame, app: &App) {
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

pub fn draw_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    
    let title = if let AppMode::DeleteFile(ref path) = app.mode {
        format!(" Delete {}? ", path.file_name().unwrap_or_default().to_string_lossy())
    } else {
        " Delete selected items? ".to_string()
    };

    f.render_widget(
        Paragraph::new(format!("Confirm deletion? [Y/n]: {}", app.input.value)).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Red)),
        ),
        area,
    );
}

pub fn draw_properties_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 50, f.area());
    f.render_widget(Clear, area);

    let mut text = Vec::new();

    if let Some(fs) = app.current_file_state() {
        let target_path = fs
            .selection.selected
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

pub fn draw_settings_modal(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Clear, area);
    let block = Block::default()
        .title_top(Line::from(vec![
            Span::styled(" SETTINGS ", Style::default().fg(Color::Black).bg(THEME.accent_primary).add_modifier(Modifier::BOLD)),
        ]))
        .title_top(Line::from(vec![
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Back ", Style::default().fg(Color::Red)),
        ]).alignment(Alignment::Right))
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
                ("Ctrl + i", "AI Introspect (State Dump)"),
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
                ("Middle Click / Space", "Preview File in Other Pane"),
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
                ("r", "Quick Rename"),
                ("F6", "Rename Selected"),
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
    let options = vec![
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
            Cell::from(Span::styled(format!("PANE {}", p_idx + 1), Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD))),
            Cell::from(""),
            Cell::from(""),
        ]));

        for (t_idx, tab) in pane.tabs.iter().enumerate() {
            let is_selected = tab_counter == app.settings_index && app.settings_section == SettingsSection::Tabs;
            let mut style = Style::default().fg(THEME.fg);
            if is_selected {
                style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
            }

            let is_active = t_idx == pane.active_tab_index;
            let status = if is_active { " [ACTIVE] " } else { "          " };
            let status_style = if is_active { Style::default().fg(Color::Green) } else { Style::default() };

            rows.push(Row::new(vec![
                Cell::from(format!("  Tab {}", t_idx + 1)).style(style),
                Cell::from(tab.current_path.to_string_lossy().to_string()).style(style),
                Cell::from(status).style(if is_selected { style } else { status_style }),
            ]));
            tab_counter += 1;
        }
        rows.push(Row::new(vec![Cell::from(""), Cell::from(""), Cell::from("")])); // Spacer
    }

    let table = Table::new(
        rows,
        [
            Constraint::Length(10),
            Constraint::Fill(1),
            Constraint::Length(12),
        ],
    )
    .header(Row::new(vec![" TAB ", " PATH ", " STATUS "])
        .style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)))
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
    let icon_mode_str = format!("{:?}", app.icon_mode);
    let options = vec![
        ("Show Hidden Files", if app.default_show_hidden { "ENABLED " } else { "DISABLED" }, "h"),
        ("Confirm Delete", if app.confirm_delete { "ENABLED " } else { "DISABLED" }, "d"),
        ("Smart Date Formatting", if app.smart_date { "ENABLED " } else { "DISABLED" }, "t"),
        ("Semantic Coloring", if app.semantic_coloring { "ENABLED " } else { "DISABLED" }, "s"),
        ("Auto Save", if app.auto_save { "ENABLED " } else { "DISABLED" }, "a"),
        ("Icon Mode", &icon_mode_str, "i"),
    ];

    let rows: Vec<_> = options
        .iter()
        .enumerate()
        .map(|(i, (label, status, key))| {
            let is_selected = i == app.settings_index && app.settings_section == SettingsSection::General;
            let mut style = Style::default().fg(THEME.fg);
            let mut status_style = if status.contains("ENABLED") {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            };

            if is_selected {
                style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
                status_style = status_style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
            }

            Row::new(vec![
                Cell::from(format!("  {}", label)).style(style),
                Cell::from(format!(" [ {} ] ", status)).style(status_style),
                Cell::from(format!("({})", key)).style(if is_selected { style } else { Style::default().fg(Color::DarkGray) }),
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
    let rows: Vec<_> = app.remote_bookmarks.iter().enumerate().map(|(i, b)| {
        let is_selected = i == app.settings_index && app.settings_section == SettingsSection::Remotes;
        let mut style = Style::default().fg(THEME.fg);
        if is_selected {
            style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
        }

        let icon = Icon::Remote.get(app.icon_mode);
        Row::new(vec![
            Cell::from(format!(" {} {}", icon, b.name)).style(style),
            Cell::from(format!("{}@{}", b.user, b.host)).style(style),
            Cell::from(b.port.to_string()).style(style),
            Cell::from(b.last_path.to_string_lossy().to_string()).style(style),
        ])
    }).collect();

    let table = Table::new(
        rows,
        [
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Fill(1),
        ],
    )
    .header(Row::new(vec![" NAME ", " CONNECTION ", " PORT ", " LAST PATH "])
        .style(Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)))
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
            Span::styled(" REMOTES [Import] ", Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)),
            Span::raw(" in the sidebar."),
        ]),
        Line::from(r#"Format (TOML): [[servers]] name="..." host="..." user="..." port=22"#),
        Line::from(""),
    ];

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    f.render_widget(Paragraph::new(text), chunks[0]);

    if app.remote_bookmarks.is_empty() {
        f.render_widget(Paragraph::new("
 (No remote servers configured)").style(Style::default().fg(Color::DarkGray)), chunks[1]);
    } else {
        f.render_widget(table, chunks[1]);
    }
}

pub fn draw_add_remote_modal(f: &mut Frame, app: &App) {
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

pub fn draw_highlight_modal(f: &mut Frame, _app: &App) {
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

    let colors = vec![
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

pub fn draw_confirm_reset_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Reset Column Widths? ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Red));
    f.render_widget(Paragraph::new("Reset all columns to defaults? (y/Enter/n)").block(block), area);
}
