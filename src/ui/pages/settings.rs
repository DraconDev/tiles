use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Table, Tabs,
    },
    Frame,
};

use crate::app::{
    App, FileColumn, SettingsSection, SettingsTarget,
};
use crate::icons::Icon;
use crate::ui::theme::THEME;

pub fn draw_settings_modal(f: &mut Frame, app: &App) {
    let area = f.area();
    f.render_widget(Clear, area);
    let block = Block::default()
        .title_top(Line::from(vec![
            Span::styled(" SETTINGS ", Style::default().fg(Color::Black).bg(crate::ui::theme::accent_primary()).add_modifier(Modifier::BOLD)),
        ]))
        .title_top(Line::from(vec![
            Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled(" Back ", Style::default().fg(Color::Red)),
        ]).alignment(ratatui::layout::Alignment::Right))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(crate::ui::theme::accent_primary()))
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
                        .bg(crate::ui::theme::accent_primary())
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
                    .fg(crate::ui::theme::accent_primary())
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
                    .bg(crate::ui::theme::accent_primary())
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
            Cell::from(Span::styled(format!("PANE {}", p_idx + 1), Style::default().fg(crate::ui::theme::accent_secondary()).add_modifier(Modifier::BOLD))),
            Cell::from(""),
            Cell::from(""),
        ]));

        for (t_idx, tab) in pane.tabs.iter().enumerate() {
            let is_selected = tab_counter == app.settings_index && app.settings_section == SettingsSection::Tabs;
            let mut style = Style::default().fg(THEME.fg);
            if is_selected {
                style = style.bg(crate::ui::theme::accent_primary()).fg(Color::Black).add_modifier(Modifier::BOLD);
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
        .style(Style::default().fg(crate::ui::theme::accent_secondary()).add_modifier(Modifier::BOLD)))
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
                style = style.bg(crate::ui::theme::accent_primary()).fg(Color::Black).add_modifier(Modifier::BOLD);
                status_style = status_style.bg(crate::ui::theme::accent_primary()).fg(Color::Black).add_modifier(Modifier::BOLD);
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
            style = style.bg(crate::ui::theme::accent_primary()).fg(Color::Black).add_modifier(Modifier::BOLD);
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
        .style(Style::default().fg(crate::ui::theme::accent_secondary()).add_modifier(Modifier::BOLD)))
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
            Span::styled(" REMOTES [Import] ", Style::default().fg(crate::ui::theme::accent_secondary()).add_modifier(Modifier::BOLD)),
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
