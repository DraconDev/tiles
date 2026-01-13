fn format_datetime_smart(time: std::time::SystemTime) -> String {
    use chrono::{DateTime, Local, Datelike};
    let dt: DateTime<Local> = time.into();
    let now = Local::now();
    if dt.year() == now.year() && dt.month() == now.month() && dt.day() == now.day() {
        dt.format("%H:%M").to_string()
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}

fn draw_file_view(f: &mut Frame, area: Rect, app: &mut App, pane_idx: usize, is_focused: bool, borders: Borders) {
    if let Some(pane) = app.panes.get_mut(pane_idx) {
        if let Some(preview) = &pane.preview {
            let block = Block::default()
                .borders(borders)
                .border_type(BorderType::Rounded)
                .title(format!(" Preview: {} ", preview.path.display()))
                .border_style(if is_focused { Style::default().fg(THEME.border_active) } else { Style::default().fg(THEME.border_inactive) });
            
            let highlighted = highlight_code(&preview.content);
            let text = Paragraph::new(highlighted).block(block);
            f.render_widget(text, area);
            return;
        }
    }

    if let Some(file_state) = app.panes.get_mut(pane_idx).and_then(|p| p.current_state_mut()) {
        file_state.view_height = area.height as usize;
        let mut render_state = TableState::default();
        if let Some(sel) = file_state.selected_index {
            let offset = file_state.table_state.offset();
            let capacity = file_state.view_height.saturating_sub(3);
            if sel >= offset && sel < offset + capacity { render_state.select(Some(sel)); }
        }
        *render_state.offset_mut() = file_state.table_state.offset();

        let constraints: Vec<Constraint> = file_state.columns.iter().map(|c| match c {
            FileColumn::Name => Constraint::Min(20),
            FileColumn::Size => Constraint::Length(9),
            FileColumn::Modified => Constraint::Length(12),
            FileColumn::Created => Constraint::Length(12),
            FileColumn::Extension => Constraint::Length(5),
            FileColumn::Permissions => Constraint::Length(10),
        }).collect();

        let dummy_block = Block::default().borders(borders);
        let inner_area = dummy_block.inner(area);
        let column_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints.clone())
            .spacing(1)
            .split(inner_area);

        file_state.column_bounds.clear();
        for (i, col_type) in file_state.columns.iter().enumerate() {
            if i < column_layout.len() { file_state.column_bounds.push((column_layout[i], *col_type)); }
        }

        let name_col_width = column_layout.get(0).map(|r| r.width as usize).unwrap_or(20);

        let header_cells = file_state.columns.iter().enumerate().map(|(_i, c)| {
            let base_name = match c { 
                FileColumn::Name => "Name", FileColumn::Size => "Size", FileColumn::Modified => "Modified", 
                FileColumn::Created => "Created", FileColumn::Extension => "Ext", FileColumn::Permissions => "Perms" 
            };
            let name = if *c == file_state.sort_column { if file_state.sort_ascending { format!("{} ▲", base_name) } else { format!("{} ▼", base_name) } } else { base_name.to_string() };
            Cell::from(name).style(Style::default().fg(THEME.header_fg).add_modifier(Modifier::BOLD))
        });

        let rows = file_state.files.iter().enumerate().map(|(i, path)| {
            if path.to_string_lossy() == "__DIVIDER__" {
                let cells = file_state.columns.iter().enumerate().map(|(col_idx, _)| {
                    if col_idx == 0 { Cell::from("> Global results").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)) } 
                    else { Cell::from("──────────────────").style(Style::default().fg(Color::DarkGray)) }
                });
                return Row::new(cells);
            }
            let metadata = file_state.metadata.get(path);
            let is_multi_selected = file_state.multi_select.contains(&i) && is_focused;
            let cells = file_state.columns.iter().map(|c| match c {
                FileColumn::Name => {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
                    let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
                    let mut final_style = if is_dir { Style::default().fg(THEME.accent_secondary) } else { Style::default().fg(THEME.fg) };
                    if let Some(c) = app.path_colors.get(path) {
                        let color = match c { 1 => Color::Red, 2 => Color::Green, 3 => Color::Yellow, 4 => Color::Blue, 5 => Color::Magenta, 6 => Color::Cyan, _ => Color::White };
                        final_style = Style::default().fg(color).add_modifier(Modifier::BOLD);
                    }
                    if let Some((ref cb_path, op)) = app.clipboard { if op == crate::app::ClipboardOp::Cut && cb_path == path { final_style = final_style.add_modifier(Modifier::DIM); } }
                    let icon = if is_dir { Icon::Folder.get(app.icon_mode) } 
                              else { match crate::modules::files::get_file_category(path) { FileCategory::Archive => Icon::Archive.get(app.icon_mode), FileCategory::Image => Icon::Image.get(app.icon_mode), FileCategory::Audio => Icon::Audio.get(app.icon_mode), FileCategory::Video => Icon::Video.get(app.icon_mode), FileCategory::Script => Icon::Script.get(app.icon_mode), FileCategory::Document => Icon::Document.get(app.icon_mode), _ => Icon::File.get(app.icon_mode) } };
                    let mut dn = if i > file_state.local_count { let fs = path.to_string_lossy(); if fs.starts_with("/home/dracon") { fs.replacen("/home/dracon", "~", 1) } else { fs.to_string() } } else { name.to_string() };
                    if app.starred.contains(path) { dn.push_str(" [*]"); }
                    dn.push(' ');
                    if dn.len() > name_col_width && name_col_width > 5 { let kl = name_col_width - 3; dn = format!("...{}", &dn[dn.len() - kl..]); }
                    Cell::from(format!("{}{}", icon, dn)).style(final_style)
                }
                FileColumn::Size => { let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false); let style = if is_dir { Style::default().fg(THEME.accent_secondary) } else { Style::default().fg(THEME.fg) }; if is_dir { Cell::from("<DIR>").style(style) } else { Cell::from(format_size(metadata.map(|m| m.size).unwrap_or(0))).style(style) } }
                FileColumn::Modified => Cell::from(format_datetime_smart(metadata.map(|m| m.modified).unwrap_or(SystemTime::UNIX_EPOCH))).style(Style::default().fg(THEME.fg)),
                FileColumn::Created => Cell::from(format_datetime_smart(metadata.map(|m| m.created).unwrap_or(SystemTime::UNIX_EPOCH))).style(Style::default().fg(THEME.fg)),
                FileColumn::Extension => Cell::from(metadata.map(|m| m.extension.as_str()).unwrap_or("")).style(Style::default().fg(THEME.fg)),
                FileColumn::Permissions => Cell::from(format_permissions(metadata.map(|m| m.permissions).unwrap_or(0))).style(Style::default().fg(THEME.fg)),
            });
            let mut row_style = Style::default();
            if is_multi_selected { row_style = row_style.bg(Color::Rgb(100, 0, 0)).fg(Color::White); }
            Row::new(cells).style(row_style)
        });

        let mut breadcrumb_spans = Vec::new();
        file_state.breadcrumb_bounds.clear();
        let path = file_state.current_path.clone();
        let components: Vec<_> = path.components().collect();
        let mut cur_p = std::path::PathBuf::new();
        let mut cur_x = area.x + 2;
        let tc = components.len();
        for (i, comp) in components.iter().enumerate() {
            match comp { std::path::Component::RootDir => cur_p.push("/"), std::path::Component::Prefix(p) => cur_p.push(p.as_os_str()), std::path::Component::Normal(name) => cur_p.push(name), _ => continue }
            let d_name = if comp.as_os_str() == "/" { "/".to_string() } else { comp.as_os_str().to_string_lossy().to_string() };
            if !d_name.is_empty() {
                let sp = cur_p.clone();
                let is_hovered = file_state.hovered_breadcrumb.as_ref() == Some(&sp);
                let is_last = i == tc - 1;
                let fg = if is_hovered { Color::Rgb(255, 255, 0) } else if is_last { THEME.accent_secondary } else { Color::Rgb(100, 100, 110) };
                let mut style = Style::default().fg(fg);
                if is_last { style = style.add_modifier(Modifier::BOLD); }
                if is_hovered { style = style.add_modifier(Modifier::UNDERLINED); }
                let segment = if is_last { format!(" [ {} ] ", d_name) } else { format!(" {} ", d_name) };
                breadcrumb_spans.push(Span::styled(segment.clone(), style));
                file_state.breadcrumb_bounds.push((Rect::new(cur_x, area.y, segment.len() as u16, 1), sp));
                cur_x += segment.len() as u16;
                if !is_last { breadcrumb_spans.push(Span::styled("›", Style::default().fg(Color::Rgb(60, 60, 70)))); cur_x += 1; }
            }
        }
        if !file_state.search_filter.is_empty() { breadcrumb_spans.push(Span::styled(format!(" [ {} ]", file_state.search_filter), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))); }

        let mut border_style = if is_focused { 
            let pulse = ((SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis() % 1500) as f32 / 1500.0 * std::f32::consts::PI * 2.0).sin() * 0.5 + 0.5;
            let r = (255.0 * (0.7 + 0.3 * pulse)) as u8; let g = (0.0 * (0.7 + 0.3 * pulse)) as u8; let b = (85.0 * (0.7 + 0.3 * pulse)) as u8;
            Style::default().fg(Color::Rgb(r, g, b)).add_modifier(Modifier::BOLD)
        } else { Style::default().fg(THEME.border_inactive) };
        if matches!(app.hovered_drop_target, Some(DropTarget::Pane(idx)) if idx == pane_idx) { border_style = Style::default().fg(Color::Rgb(0, 255, 200)).add_modifier(Modifier::BOLD); }
        let block = Block::default().borders(borders).border_type(BorderType::Rounded).title(Line::from(breadcrumb_spans)).border_style(border_style);

        let table = Table::new(rows, constraints.clone()).header(Row::new(header_cells).height(1)).block(block.clone()).column_spacing(1)
            .row_highlight_style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD));

        f.render_stateful_widget(table, area, &mut render_state);
        *file_state.table_state.offset_mut() = render_state.offset();

        let vr = area.height.saturating_sub(3) as usize;
        if file_state.files.len() > vr {
            let sb = Scrollbar::default().orientation(ScrollbarOrientation::VerticalRight).begin_symbol(Some("▲")).end_symbol(Some("▼"));
            let mut ss = ScrollbarState::new(file_state.files.len()).position(file_state.table_state.offset()).viewport_content_length(vr);
            f.render_stateful_widget(sb, area, &mut ss);
        }
    }
}
