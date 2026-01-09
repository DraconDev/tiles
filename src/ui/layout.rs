fn draw_global_header(f: &mut Frame, area: Rect, app: &mut App) {
    let pane_count = app.panes.len();
    
    // Burger Menu (Settings) at Top-Left
    let menu_label = " ≡ ";
    let menu_width = 3;
    let menu_rect = Rect::new(area.x, area.y, menu_width, 1);
    
    f.render_widget(
        Paragraph::new(menu_label).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        menu_rect,
    );

    // Split Button (Keep at Right for now, or move next to menu? User just said settings to left)
    // Let's keep Split at right to balance it.
    let split_label = "[+]";
    let split_width = 3;
    let split_rect = Rect::new(
        area.x + area.width.saturating_sub(split_width),
        area.y,
        split_width,
        1,
    );
     f.render_widget(
        Paragraph::new(split_label).style(Style::default().fg(Color::Cyan)),
        split_rect,
    );

    if pane_count == 0 {
        return;
    }

    // Tabs Area
    // Start after Menu, End before Split
    let tabs_x = area.x + menu_width + 1;
    let tabs_width = area.width.saturating_sub(menu_width + 1 + split_width + 1);
    
    if tabs_width > 0 {
        let tabs_area = Rect::new(tabs_x, area.y, tabs_width, 1);
        
        // Split tabs area if multiple panes
        let pane_constraints = vec![Constraint::Ratio(1, pane_count as u32); pane_count];
        let pane_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(pane_constraints)
            .split(tabs_area);

        for (p_i, pane) in app.panes.iter().enumerate() {
            let chunk = pane_chunks[p_i];
            let mut current_x = chunk.x;

            for (t_i, tab) in pane.tabs.iter().enumerate() {
                let mut name = if !tab.search_filter.is_empty() {
                    format!("Search: {}", tab.search_filter)
                } else {
                    tab.current_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or("/".to_string())
                };

                if let Some(branch) = &tab.git_branch {
                    name = format!("{} ({})", name, branch);
                }

                let is_active_tab = t_i == pane.active_tab_index;
                let is_focused_pane = p_i == app.focused_pane_index && !app.sidebar_focus;

                let style = if is_active_tab {
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

                let text = format!(" {} ", name);
                let width = text.len() as u16;

                if current_x + width > chunk.x + chunk.width {
                    break;
                }

                f.render_widget(
                    Paragraph::new(text).style(style),
                    Rect::new(current_x, chunk.y, width, 1),
                );
                current_x += width + 1;
            }
        }
    }
}
