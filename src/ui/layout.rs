use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Paragraph,
    },
    Frame,
};
use std::time::SystemTime;

use crate::app::{
    App, AppMode, CurrentView,
};
use crate::icons::Icon;
use crate::ui::theme::THEME;
use terma::utils::{
    format_size,
};

pub fn draw_global_header(f: &mut Frame, area: Rect, sidebar_width: u16, app: &mut App) {
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
    let project_icon = Icon::Folder.get(app.icon_mode); // Use Folder icon for IDE/Project

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
            (project_icon, "project"),
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
    let mut global_tab_idx = if show_icons { 7 } else { 0 }; 
    for (p_i, pane) in app.panes.iter().enumerate() {
        let chunk = pane_chunks[p_i];
        let mut current_x = chunk.x;

        if app.current_view == CurrentView::Editor {
            // SINGLE FILE TAB for Editor View
            if let Some(preview) = &pane.preview {
                let base_name = preview.path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Editor".to_string());
                
                let is_focused_pane = p_i == app.focused_pane_index && !app.sidebar_focus;
                let base_style = if is_focused_pane {
                    Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME.accent_primary)
                };

                let mut spans = vec![Span::styled(format!(" {} ", base_name), base_style)];

                // Add git info if available from active tab
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
                            spans.push(Span::styled(format!("+{}", pending), Style::default().fg(Color::Red)));
                        }
                        if ahead > 0 {
                            spans.push(Span::styled(format!(" ↑{}", ahead), Style::default().fg(Color::Yellow)));
                        }
                        if behind > 0 {
                            spans.push(Span::styled(format!(" ↓{}", behind), Style::default().fg(Color::Yellow)));
                        }
                        spans.push(Span::raw(" "));
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
            let base_name = tab.current_path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".to_string());
            
            let is_active_tab = t_i == pane.active_tab_index;
            let is_focused_pane = p_i == app.focused_pane_index && !app.sidebar_focus;

            let mut base_style = if is_active_tab {
                if is_focused_pane {
                    Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME.accent_primary)
                }
            } else {
                Style::default().fg(Color::DarkGray)
            };

            if let AppMode::Header(idx) = app.mode {
                if idx == global_tab_idx {
                    base_style = base_style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
                }
            }

            spans.push(Span::styled(format!(" {} ", base_name), base_style));

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
                    spans.push(Span::styled(format!("+{}", pending), Style::default().fg(Color::Red)));
                }
                if ahead > 0 {
                    spans.push(Span::styled(format!(" ↑{}", ahead), Style::default().fg(Color::Yellow)));
                }
                if behind > 0 {
                    spans.push(Span::styled(format!(" ↓{}", behind), Style::default().fg(Color::Yellow)));
                }
                spans.push(Span::raw(" "));
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

pub fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
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
                let borders = if pane_count > 1 {
                    if i == 0 {
                        Borders::ALL
                    } else {
                        Borders::ALL
                    }
                } else {
                    Borders::ALL
                };
                crate::ui::panes::files::draw_file_view(f, chunks[i], app, i, is_focused, borders);
            }
        }
        CurrentView::Editor => {
            crate::ui::panes::editor::draw_editor_stage(f, area, app);
        }
        _ => {}
    }
}

pub fn draw_footer(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),      // Log, Clipboard & Shortcuts
            Constraint::Length(25), // Selection Info
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
        Span::styled(" F1 ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Help "),
        Span::styled(" ^N ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Terminal "),
        Span::styled(" Space ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)), Span::raw("Preview/Edit "),
    ];
    left_spans.extend(shortcuts);

    f.render_widget(Paragraph::new(Line::from(left_spans)), chunks[0]);

    // 2. Middle Section: Selection Summary
    if let Some(fs) = app.current_file_state() {
        let sel_count = if !fs.selection.multi.is_empty() { fs.selection.multi.len() } else if fs.selection.selected.is_some() { 1 } else { 0 };
        let total_count = fs.files.len();
        let summary = format!(" SEL: {} / {} ", sel_count, total_count);
        f.render_widget(Paragraph::new(summary).style(Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)).alignment(ratatui::layout::Alignment::Right), chunks[1]);
    }

    // 3. Right Section: CPU/MEM Stats
    let cpu_bar = draw_stat_bar("CPU", app.system_state.cpu_usage, 100.0);
    let mem_usage = (app.system_state.mem_usage as f32 / app.system_state.total_mem.max(1.0) as f32) * 100.0;
    let mem_bar = draw_stat_bar("MEM", mem_usage, 100.0);
    let stats_layout = Layout::default().direction(Direction::Horizontal).constraints([Constraint::Percentage(50), Constraint::Percentage(50)]).split(chunks[2]);
    f.render_widget(Paragraph::new(cpu_bar).alignment(ratatui::layout::Alignment::Right), stats_layout[0]);
    f.render_widget(Paragraph::new(mem_bar).alignment(ratatui::layout::Alignment::Right), stats_layout[1]);
}

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