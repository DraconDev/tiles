use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table},
    Frame,
};

use crate::app::{App, AppMode};
use crate::event_helpers::get_open_with_suggestions;
use crate::icons::Icon;
use crate::ui::theme::THEME;
use terma::layout::centered_rect;
use terma::utils::{format_permissions, format_size, format_time, truncate_to_width};
use terma::widgets::HotkeyHint;

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn draw_hotkeys_modal(f: &mut Frame, area: Rect) {
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" KEYBINDINGS ")
        .border_style(Style::default().fg(crate::ui::theme::accent_primary()));
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
            .alignment(Alignment::Center),
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
                    .fg(crate::ui::theme::accent_primary())
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

#[allow(dead_code)]
pub fn draw_open_with_modal(f: &mut Frame, app: &App, path: &std::path::Path) {
    let area = centered_rect(60, 60, f.area());
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
        .border_style(Style::default().fg(crate::ui::theme::accent_primary()));
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
    let mut suggestions = get_open_with_suggestions(app, &ext);

    // Filter suggestions based on input
    if !app.input.value.is_empty() {
        let query = app.input.value.to_lowercase();
        suggestions.retain(|s: &String| s.to_lowercase().contains(&query));
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
                    .bg(crate::ui::theme::accent_primary())
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

#[allow(dead_code)]
pub fn draw_import_servers_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Import Servers (TOML) ")
        .border_style(Style::default().fg(crate::ui::theme::accent_primary()));
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
        Paragraph::new("> ").style(Style::default().fg(crate::ui::theme::accent_secondary())),
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
                        Style::default()
                            .bg(crate::ui::theme::accent_primary())
                            .fg(Color::Black),
                    ),
                    Span::raw(ext_part),
                ])
            } else {
                Line::from(vec![Span::styled(
                    &app.input.value,
                    Style::default()
                        .bg(crate::ui::theme::accent_primary())
                        .fg(Color::Black),
                )])
            }
        } else {
            Line::from(vec![Span::styled(
                &app.input.value,
                Style::default()
                    .bg(crate::ui::theme::accent_primary())
                    .fg(Color::Black),
            )])
        };
        f.render_widget(Paragraph::new(text), inner);
    } else {
        f.render_widget(&app.input, inner);
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn draw_delete_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);

    let title = " Delete items? ".to_string();

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

#[allow(dead_code)]
pub fn draw_delete_file_modal(f: &mut Frame, app: &App, path: &std::path::Path) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    let title = format!(
        " Delete {}? ",
        path.file_name().unwrap_or_default().to_string_lossy()
    );
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

#[allow(dead_code)]
pub fn draw_properties_modal(f: &mut Frame, app: &App) {
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
            Span::styled(
                "Name: ",
                Style::default().fg(crate::ui::theme::accent_secondary()),
            ),
            Span::raw(name),
        ]));
        text.push(Line::from(vec![
            Span::styled(
                "Location: ",
                Style::default().fg(crate::ui::theme::accent_secondary()),
            ),
            Span::raw(parent),
        ]));
        text.push(Line::from(""));

        if let Some(meta) = fs.metadata.get(target_path) {
            let type_str = if meta.is_dir { "Folder" } else { "File" };
            text.push(Line::from(vec![
                Span::styled(
                    "Type: ",
                    Style::default().fg(crate::ui::theme::accent_secondary()),
                ),
                Span::raw(type_str),
            ]));
            text.push(Line::from(vec![
                Span::styled(
                    "Size: ",
                    Style::default().fg(crate::ui::theme::accent_secondary()),
                ),
                Span::raw(format_size(meta.size)),
            ]));
            text.push(Line::from(vec![
                Span::styled(
                    "Modified: ",
                    Style::default().fg(crate::ui::theme::accent_secondary()),
                ),
                Span::raw(format_time(meta.modified)),
            ]));
            text.push(Line::from(vec![
                Span::styled(
                    "Created: ",
                    Style::default().fg(crate::ui::theme::accent_secondary()),
                ),
                Span::raw(format_time(meta.created)),
            ]));
            text.push(Line::from(vec![
                Span::styled(
                    "Permissions: ",
                    Style::default().fg(crate::ui::theme::accent_secondary()),
                ),
                Span::raw(format_permissions(meta.permissions)),
            ]));
        }
    }

    let block = Block::default()
        .title(" Properties ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(crate::ui::theme::accent_primary()));
    f.render_widget(Paragraph::new(text).block(block), area);
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
        .border_style(Style::default().fg(crate::ui::theme::accent_primary()));
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
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        Rect::new(inner.x, inner.y + 1, inner.width, 1),
    );
    f.render_widget(
        Paragraph::new("1   2   3   4   5   6   0")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        Rect::new(inner.x, inner.y + 2, inner.width, 1),
    );
}

#[allow(dead_code)]
pub fn draw_confirm_reset_modal(f: &mut Frame, _area: Rect) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    let block = Block::default()
        .title(" Reset Column Widths? ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow));
    f.render_widget(
        Paragraph::new("Reset all columns to defaults? (y/Enter/n)").block(block),
        area,
    );
}

#[allow(dead_code)]
pub fn draw_context_menu(
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
                .bg(crate::ui::theme::accent_primary())
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

    let menu_width = 25;
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
        .border_style(Style::default().fg(crate::ui::theme::accent_secondary()));

    // Add 1 cell of padding on the left by using a nested layout or margin
    let inner_area = menu_block.inner(area);
    let padded_area = Rect::new(
        inner_area.x + 1,
        inner_area.y,
        inner_area.width.saturating_sub(1),
        inner_area.height,
    );

    f.render_widget(menu_block, area);
    f.render_widget(List::new(items), padded_area);
}
