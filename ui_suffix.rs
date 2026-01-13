
fn draw_stat_bar(label: &str, value: f32, max: f32) -> Line<'static> {
    let width = 10;
    let ratio = (value / max).clamp(0.0, 1.0);
    let filled = (ratio * width as f32).round() as usize;
    
    let mut spans = vec![
        Span::styled(format!("{} ", label), Style::default().fg(Color::DarkGray)),
    ];

    for i in 0..width {
        let symbol = if i < filled { "█" } else { "░" };
        let color = if ratio < 0.4 {
            Color::Rgb(0, 255, 150) // Cyber Green
        } else if ratio < 0.7 {
            Color::Rgb(255, 255, 0) // Yellow
        } else {
            Color::Rgb(255, 0, 85)  // Neon Red
        };
        
        if i < filled {
            spans.push(Span::styled(symbol, Style::default().fg(color)));
        } else {
            spans.push(Span::styled(symbol, Style::default().fg(Color::Rgb(30, 30, 35))));
        }
    }

    spans.push(Span::styled(format!(" {:>3.0}%", ratio * 100.0), Style::default().fg(THEME.fg).add_modifier(Modifier::BOLD)));
    Line::from(spans)
}

fn draw_footer(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),      // Log, Clipboard & Shortcuts
            Constraint::Length(20), // Selection Info
            Constraint::Length(45), // Stats (CPU/MEM)
        ])
        .split(area);

    // 1. Left Section: ^Q Quit, Activity Log, Clipboard & Essential Shortcuts
    let mut left_spans = vec![
        Span::styled(" ^Q ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw("Quit "),
    ];
    
    // Log
    if let Some((msg, time)) = &app.last_action_msg {
        if time.elapsed().as_secs() < 5 {
            left_spans.push(Span::styled(format!(" [ SYSTEM ] {} ", msg), Style::default().fg(THEME.accent_secondary).bg(Color::Rgb(20, 25, 30))));
        }
    }
    
    // Clipboard
    if let Some((ref path, op)) = app.clipboard {
        let op_str = match op { crate::app::ClipboardOp::Copy => "COPY", crate::app::ClipboardOp::Cut => "CUT" };
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string_lossy().to_string());
        left_spans.push(Span::styled(format!(" [ {} ] {} ", op_str, name), Style::default().fg(Color::Yellow).bg(Color::Rgb(30, 30, 20))));
    }

    // Spacing
    if left_spans.len() > 2 {
        left_spans.push(Span::raw("  "));
    }

    // Reduced Shortcuts
    let shortcuts = vec![
        Span::styled(" ^B ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Side "),
        Span::styled(" ^S ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Split "),
        Span::styled(" ^T ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Tab "),
    ];
    left_spans.extend(shortcuts);

    f.render_widget(Paragraph::new(Line::from(left_spans)), chunks[0]);

    // 2. Middle Section: Selection Summary
    if let Some(fs) = app.current_file_state() {
        let sel_count = if !fs.multi_select.is_empty() { fs.multi_select.len() } else if fs.selected_index.is_some() { 1 } else { 0 };
        let total_count = fs.files.len();
        let summary = format!(" SEL: {} / {} ", sel_count, total_count);
        f.render_widget(Paragraph::new(summary).style(Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)).alignment(ratatui::layout::Alignment::Right), chunks[1]);
    }

    // 3. Right Section: CPU/MEM Stats
    let cpu_bar = draw_stat_bar("CPU", app.system_state.cpu_usage, 100.0);
    let mem_usage = (app.system_state.mem_usage / app.system_state.total_mem.max(1.0)) as f32 * 100.0;
    let mem_bar = draw_stat_bar("MEM", mem_usage, 100.0);
    let stats_layout = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(chunks[2]);
    f.render_widget(Paragraph::new(cpu_bar).alignment(ratatui::layout::Alignment::Right), stats_layout[0]);
    f.render_widget(Paragraph::new(mem_bar).alignment(ratatui::layout::Alignment::Right), stats_layout[1]);
}

fn draw_context_menu(f: &mut Frame, x: u16, y: u16, target: &crate::app::ContextMenuTarget, app: &App) {
    use crate::app::ContextMenuAction;
    let mut items = Vec::new();
    
    let actions = if let AppMode::ContextMenu { actions, .. } = &app.mode {
        actions.clone()
    } else {
        vec![] 
    };

    for action in &actions {
        let label = match action {
            ContextMenuAction::Open => format!(" {} Open", Icon::Folder.get(app.icon_mode)),
            ContextMenuAction::OpenNewTab => format!(" {} Open in New Tab", Icon::Split.get(app.icon_mode)),
            ContextMenuAction::OpenWith => format!(" {} Open With...", Icon::Split.get(app.icon_mode)),
            ContextMenuAction::Edit => format!(" {} Edit", Icon::Document.get(app.icon_mode)),
            ContextMenuAction::Run => format!(" {} Run", Icon::Video.get(app.icon_mode)),
            ContextMenuAction::RunTerminal => format!(" {} Run in Terminal", Icon::Script.get(app.icon_mode)),
            ContextMenuAction::ExtractHere => format!(" {} Extract Here", Icon::Archive.get(app.icon_mode)),
            ContextMenuAction::NewFolder => format!(" {} New Folder", Icon::Folder.get(app.icon_mode)),
            ContextMenuAction::NewFile => format!(" {} New File", Icon::File.get(app.icon_mode)),
            ContextMenuAction::Cut => " 󰆐 Cut".to_string(), // Keep some standard ones or update all
            ContextMenuAction::Copy => " 󰆏 Copy".to_string(),
            ContextMenuAction::CopyPath => " 󰆏 Copy Path".to_string(),
            ContextMenuAction::CopyName => " 󰆏 Copy Name".to_string(),
            ContextMenuAction::Paste => " 󰆒 Paste".to_string(),
            ContextMenuAction::Rename => " 󰏫 Rename".to_string(),
            ContextMenuAction::Duplicate => " 󰆏 Duplicate".to_string(),
            ContextMenuAction::Compress => format!(" {} Compress", Icon::Archive.get(app.icon_mode)),
            ContextMenuAction::Delete => " 󰆴 Delete".to_string(),
            ContextMenuAction::AddToFavorites => format!(" {} Add to Favorites", Icon::Star.get(app.icon_mode)),
            ContextMenuAction::RemoveFromFavorites => format!(" {} Remove from Favorites", Icon::Star.get(app.icon_mode)),
            ContextMenuAction::Properties => format!(" {} Properties", Icon::Document.get(app.icon_mode)),
        ContextMenuAction::TerminalWindow => format!(" {} New Terminal Window", Icon::Script.get(app.icon_mode)),
            ContextMenuAction::Refresh => " 󰑓 Refresh".to_string(),
            ContextMenuAction::SelectAll => " 󰒆 Select All".to_string(),
            ContextMenuAction::ToggleHidden => " 󰈈 Toggle Hidden".to_string(),
            ContextMenuAction::ConnectRemote => format!(" {} Connect", Icon::Remote.get(app.icon_mode)),
            ContextMenuAction::DeleteRemote => " 󰆴 Delete Bookmark".to_string(),
            ContextMenuAction::Mount => " 󰃭 Mount".to_string(),
            ContextMenuAction::Unmount => " 󰃭 Unmount".to_string(),
            ContextMenuAction::SetWallpaper => format!(" {} Set as Wallpaper", Icon::Image.get(app.icon_mode)),
            ContextMenuAction::GitInit => format!(" {} Git Init", Icon::Git.get(app.icon_mode)),
            ContextMenuAction::GitStatus => format!(" {} Git Status", Icon::Git.get(app.icon_mode)),
            ContextMenuAction::SetColor(_) => format!(" {} Highlight...", Icon::Image.get(app.icon_mode)),
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
                        label.push_str(if fs.sort_ascending { " (▲)" } else { " (▼)" });
                    }
                }
                label
            },
        };
        
        let mut item = ListItem::new(label);
        if (*action == ContextMenuAction::Paste) && app.clipboard.is_none() {
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
    };
    
    let menu_width = 25;
    let menu_height = items.len() as u16 + 2;
    let mut draw_x = x;
    let mut draw_y = y;
    if draw_x + menu_width > f.area().width { draw_x = f.area().width.saturating_sub(menu_width); }
    if draw_y + menu_height > f.area().height { draw_y = f.area().height.saturating_sub(menu_height); }

    let area = Rect::new(draw_x, draw_y, menu_width, menu_height);

    f.render_widget(Clear, area);
    let menu_block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(THEME.accent_secondary));
    
    // Add 1 cell of padding on the left by using a nested layout or margin
    let inner_area = menu_block.inner(area);
    let padded_area = Rect::new(inner_area.x + 1, inner_area.y, inner_area.width.saturating_sub(1), inner_area.height);
    
    f.render_widget(menu_block, area);
    f.render_widget(List::new(items), padded_area);
}

fn draw_import_servers_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let block = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Import Servers (TOML) ").border_style(Style::default().fg(THEME.accent_primary));
    let inner = block.inner(area);
    f.render_widget(block, area);
    
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(2), Constraint::Length(1), Constraint::Length(1), Constraint::Length(1)]).split(inner);
    
    f.render_widget(Paragraph::new("Enter path to server configuration file:"), chunks[0]);
    
    let input_area = chunks[1];
    f.render_widget(Paragraph::new("> ").style(Style::default().fg(THEME.accent_secondary)), Rect::new(input_area.x, input_area.y, 2, 1));
    f.render_widget(&app.input, Rect::new(input_area.x + 2, input_area.y, input_area.width.saturating_sub(2), 1));
    
    let footer_text = vec![Span::styled(" [Enter] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)), Span::raw("Import "), Span::styled(" [Esc] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)), Span::raw("Cancel")];
    f.render_widget(Paragraph::new(Line::from(footer_text)), chunks[3]);
}

fn draw_command_palette(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());
    f.render_widget(Clear, area);
    let inner = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Command Palette ").border_style(Style::default().fg(Color::Magenta)).inner(area);
    f.render_widget(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Command Palette ").border_style(Style::default().fg(Color::Magenta)), area);
    
    f.render_widget(Paragraph::new("> ").style(Style::default().fg(Color::Yellow)), Rect::new(inner.x, inner.y, 2, 1));
    f.render_widget(&app.input, Rect::new(inner.x + 2, inner.y, inner.width.saturating_sub(2), 1));
    
    let items: Vec<ListItem> = app.filtered_commands.iter().enumerate().map(|(i, cmd)| {
        let style = if i == app.command_index { Style::default().bg(Color::DarkGray).fg(Color::White) } else { Style::default() };
        ListItem::new(cmd.desc.clone()).style(style)
    }).collect();
    f.render_widget(List::new(items), Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1));
}

fn draw_rename_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); 
    f.render_widget(Clear, area);
    let block = Block::default().title(" Rename ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    f.render_widget(block, area);
    
    if app.rename_selected {
        let text = if let Some(idx) = app.input.value.rfind('.') {
             if idx > 0 {
                 let stem_part = &app.input.value[..idx];
                 let ext_part = &app.input.value[idx..];
                 Line::from(vec![
                     Span::styled(stem_part, Style::default().bg(Color::Blue).fg(Color::White)),
                     Span::raw(ext_part)
                 ])
             } else {
                 Line::from(vec![Span::styled(&app.input.value, Style::default().bg(Color::Blue).fg(Color::White))])
             }
        } else {
             Line::from(vec![Span::styled(&app.input.value, Style::default().bg(Color::Blue).fg(Color::White))])
        };
        f.render_widget(Paragraph::new(text), inner);
    } else {
        f.render_widget(&app.input, inner);
    }
}

fn draw_new_folder_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); 
    f.render_widget(Clear, area);
    let block = Block::default().title(" New Folder ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);
}

fn draw_new_file_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area()); 
    f.render_widget(Clear, area);
    let block = Block::default().title(" New File ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(&app.input, inner);
}

fn draw_delete_modal(f: &mut Frame, _app: &App) {
    let area = centered_rect(40, 10, f.area()); 
    f.render_widget(Clear, area);
    f.render_widget(Paragraph::new("Delete selected item(s)? (y/n)").block(Block::default().title(" Delete ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Red))), area);
}

fn draw_properties_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 50, f.area()); 
    f.render_widget(Clear, area);
    
    let mut text = Vec::new();
    
    if let Some(fs) = app.current_file_state() {
        let target_path = fs.selected_index.and_then(|idx| fs.files.get(idx)).unwrap_or(&fs.current_path);
        
        let name = target_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| target_path.to_string_lossy().to_string());
        let parent = target_path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
        
        text.push(Line::from(vec![Span::styled("Name: ", Style::default().fg(THEME.accent_secondary)), Span::raw(name)]));
        text.push(Line::from(vec![Span::styled("Location: ", Style::default().fg(THEME.accent_secondary)), Span::raw(parent)]));
        text.push(Line::from(""));
        
        if let Some(meta) = fs.metadata.get(target_path) {
            let type_str = if meta.is_dir { "Folder" } else { "File" };
            text.push(Line::from(vec![Span::styled("Type: ", Style::default().fg(THEME.accent_secondary)), Span::raw(type_str)]));
            text.push(Line::from(vec![Span::styled("Size: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_size(meta.size))]));
            text.push(Line::from(vec![Span::styled("Modified: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_time(meta.modified))]));
            text.push(Line::from(vec![Span::styled("Created: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_time(meta.created))]));
            text.push(Line::from(vec![Span::styled("Permissions: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_permissions(meta.permissions))]));
        } else {
             if fs.remote_session.is_none() {
                 if let Ok(m) = std::fs::metadata(target_path) {
                     let is_dir = m.is_dir();
                     text.push(Line::from(vec![Span::styled("Type: ", Style::default().fg(THEME.accent_secondary)), Span::raw(if is_dir { "Folder" } else { "File" })]));
                     text.push(Line::from(vec![Span::styled("Size: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_size(m.len()))]));
                     if let Ok(mod_time) = m.modified() {
                         text.push(Line::from(vec![Span::styled("Modified: ", Style::default().fg(THEME.accent_secondary)), Span::raw(format_time(mod_time))]));
                     }
                 } else {
                     text.push(Line::from(Span::styled("No metadata available", Style::default().fg(Color::DarkGray))));
                 }
             } else {
                 text.push(Line::from(Span::styled("No metadata available (Remote)", Style::default().fg(Color::DarkGray))));
             }
        }
    }

    let block = Block::default().title(" Properties ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Cyan));
    f.render_widget(Paragraph::new(text).block(block), area);
}

fn draw_settings_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 80, f.area()); 
    f.render_widget(Clear, area);
    let block = Block::default().title(" Settings ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area); f.render_widget(block, area);
    let chunks = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Length(15), Constraint::Min(0)]).split(inner);
    let sections = vec![ListItem::new(" 󰟜 Columns "), ListItem::new(" 󰓩 Tabs "), ListItem::new(" 󰒓 General "), ListItem::new(" 󰒍 Remotes "), ListItem::new(" 󰌌 Shortcuts ")];
    let sel = match app.settings_section { SettingsSection::Columns => 0, SettingsSection::Tabs => 1, SettingsSection::General => 2, SettingsSection::Remotes => 3, SettingsSection::Shortcuts => 4 };
    let items: Vec<ListItem> = sections.into_iter().enumerate().map(|(i, item)| {
        if i == sel { item.style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD)) } else { item }
    }).collect();
    f.render_widget(List::new(items).block(Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(Color::DarkGray))), chunks[0]);
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
        ("General", vec![
            ("Ctrl + q", "Quit Application"),
            ("Ctrl + g", "Open Settings"),
            ("Ctrl + Space", "Open Command Palette"),
            ("Ctrl + b", "Toggle Sidebar"),
            ("Ctrl + i", "AI Introspect (State Dump)"),
        ]),
        ("Navigation", vec![
            ("↑ / ↓", "Move Selection"),
            ("Left / Right", "Change Pane / Enter/Leave Sidebar"),
            ("Enter", "Open Directory / File"),
            ("Shift + Enter", "Open Folder in New Tab"),
            ("Backspace", "Go to Parent Directory"),
            ("Alt + Left / Right", "Back / Forward in History"),
            ("~", "Go to Home Directory"),
            ("Middle Click / Space", "Preview File in Other Pane"),
        ]),
        ("View & Tabs", vec![
            ("Ctrl + s", "Toggle Split View"),
            ("Ctrl + t", "New Duplicate Tab"),
            ("Ctrl + h", "Toggle Hidden Files"),
            ("Ctrl + b", "Toggle Sidebar"),
            ("Ctrl + l / u", "Clear Search Filter"),
            ("Ctrl + z / y", "Undo / Redo (Rename/Move)"),
            ("F1", "Show this Help"),
        ]),
        ("File Operations", vec![
            ("Ctrl + c", "Copy Selected"),
            ("Ctrl + x", "Cut Selected"),
            ("Ctrl + v", "Paste Selected"),
            ("Ctrl + a", "Select All"),
            ("F6", "Rename Selected"),
            ("Delete", "Delete Selected"),
            ("Alt + Enter", "Show Properties"),
        ]),
        ("Terminal", vec![
            ("Ctrl + n", "Open External Terminal"),
        ]),
    ];

    let mut rows = Vec::new();
    for (category, items) in shortcuts {
        rows.push(Row::new(vec![Cell::from(Span::styled(category, Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD))), Cell::from("")]));
        for (key, desc) in items {
            rows.push(Row::new(vec![
                Cell::from(Span::styled(key, Style::default().fg(Color::Yellow))),
                Cell::from(desc),
            ]));
        }
        rows.push(Row::new(vec![Cell::from(""), Cell::from("")])); // Spacer
    }

    let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(0)])
        .block(Block::default().title(" Keyboard Shortcuts ").borders(Borders::NONE));
    
    f.render_widget(table, area);
}

fn draw_column_settings(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(0)]).split(area);
    let titles = vec![" [Single] ", " [Split] "];
    let sel = match app.settings_target { SettingsTarget::SingleMode => 0, SettingsTarget::SplitMode => 1 };
    f.render_widget(Tabs::new(titles).block(Block::default().borders(Borders::BOTTOM).title(" Configure Mode ")).select(sel).highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), chunks[0]);
    let options = vec![
        (FileColumn::Extension, "Extension (e)"),
        (FileColumn::Size, "Size (s)"), 
        (FileColumn::Modified, "Modified (m)"), 
        (FileColumn::Created, "Created (c)"),
        (FileColumn::Permissions, "Permissions (p)")
    ];
    let target = match app.settings_target { SettingsTarget::SingleMode => &app.single_columns, SettingsTarget::SplitMode => &app.split_columns };
    let items: Vec<ListItem> = options.iter().map(|(col, label)| {
        let prefix = if target.contains(col) { "[x] " } else { "[ ] " };
        ListItem::new(format!("{}{}", prefix, label))
    }).collect();
    f.render_widget(List::new(items).block(Block::default().title(" Visible Columns ").borders(Borders::NONE)), chunks[1]);
}

fn draw_tab_settings(f: &mut Frame, area: Rect, _app: &App) {
    f.render_widget(Paragraph::new("Tab settings placeholder"), area);
}

fn draw_general_settings(f: &mut Frame, area: Rect, app: &App) {
    let items = vec![
        ListItem::new(format!("[{}] Show Hidden Files (h)", if app.default_show_hidden { "x" } else { " " })),
        ListItem::new(format!("[{}] Confirm Delete (d)", if app.confirm_delete { "x" } else { " " })),
        ListItem::new(format!("Icon Mode: {:?} (i)", app.icon_mode)),
    ];
    f.render_widget(List::new(items).block(Block::default().title(" General Preferences ").borders(Borders::NONE)), area);
}

fn draw_remote_settings(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app.remote_bookmarks.iter().map(|b| ListItem::new(format!("󰒍 {} ({}@{})", b.name, b.user, b.host))).collect();
    let list = if items.is_empty() { List::new(vec![ListItem::new("(No remote servers configured)").style(Style::default().fg(Color::DarkGray))]) } else { List::new(items) };
    let text = vec![Line::from("Manage your remote server bookmarks here."), Line::from(""), Line::from("Tip: You can bulk import servers by clicking the"), Line::from(vec![Span::styled("REMOTES [Import] ", Style::default().fg(THEME.accent_secondary).add_modifier(Modifier::BOLD)), Span::raw("header in the sidebar.")]), Line::from(""), Line::from("Current Servers:")];
    let chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(7), Constraint::Min(0)]).split(area);
    f.render_widget(Paragraph::new(text), chunks[0]);
    f.render_widget(list.block(Block::default().borders(Borders::TOP).title(" Bookmarks ")), chunks[1]);
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

    let active_idx = if let AppMode::AddRemote(idx) = app.mode { idx } else { 0 };

    let fields = [
        ("Name", &app.pending_remote.name),
        ("Host", &app.pending_remote.host),
        ("User", &app.pending_remote.user),
        ("Port", &app.pending_remote.port.to_string()),
        ("Key Path", &app.pending_remote.key_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()),
    ];

    for (i, (label, value)) in fields.iter().enumerate() {
        let is_active = i == active_idx;
        let mut style = Style::default().fg(Color::DarkGray);
        if is_active { style = Style::default().fg(Color::Yellow); }

        let block = Block::default().borders(Borders::ALL).title(format!(" {} ", label)).border_style(style);
        let field_area = chunks[i];
        
        if is_active {
            f.render_widget(Paragraph::new(app.input.value.as_str()).block(block), field_area);
        } else {
            f.render_widget(Paragraph::new(value.as_str()).block(block), field_area);
        }
    }

    let help_text = vec![
        Line::from(vec![
            Span::styled(" [Tab/Enter] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Next Field  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
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
        5
    );
    
    f.render_widget(Clear, area);
    let block = Block::default().title(" Highlight ").borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(Color::Cyan));
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

    f.render_widget(Paragraph::new(Line::from(spans)).alignment(ratatui::layout::Alignment::Center), Rect::new(inner.x, inner.y + 1, inner.width, 1));
    f.render_widget(Paragraph::new("1   2   3   4   5   6   0").alignment(ratatui::layout::Alignment::Center).style(Style::default().fg(Color::DarkGray)), Rect::new(inner.x, inner.y + 2, inner.width, 1));
}