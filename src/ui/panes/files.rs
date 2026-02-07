use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Paragraph, TableState,
    },
    Frame,
};
use std::time::SystemTime;

use crate::app::{
    App, DropTarget, FileColumn,
};
use crate::icons::Icon;
use crate::ui::theme::THEME;
use terma::utils::{
    format_permissions, format_size, truncate_to_width,
};

pub fn draw_pane(f: &mut Frame, area: Rect, pane_idx: usize, app: &mut App) {
    let is_focused = app.focused_pane_index == pane_idx;
    let borders = if app.is_split_mode {
        if pane_idx == 0 { Borders::ALL } else { Borders::TOP | Borders::BOTTOM | Borders::RIGHT }
    } else {
        Borders::ALL
    };
    draw_file_view(f, area, app, pane_idx, is_focused, borders);
}

pub fn draw_file_view(
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
                let language = preview.path.extension().and_then(|s| s.to_str()).unwrap_or("");
                
                // PERFORMANCE OPTIMIZATION: Only highlight what's likely to be visible + some buffer
                // This is a PREVIEW, so full file highlighting is overkill for large files.
                let content_to_highlight = if preview.content.lines().count() > 500 {
                    preview.content.lines().take(500).collect::<Vec<_>>().join("
")
                } else {
                    preview.content.clone()
                };

                let highlighted = terma::utils::highlight_code(&content_to_highlight, language);
                let mut lines = Vec::new();
                for (i, line) in highlighted.iter().enumerate() {
                    let mut spans = line.spans.iter().map(|s| Span::styled(s.content.to_string(), s.style)).collect::<Vec<_>>();
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

    crate::ui::panes::breadcrumbs::draw_pane_breadcrumbs(f, area, app, pane_idx);

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
            
            if path.to_string_lossy() == "__DIVIDER__" {
                let divider_style = Style::default().fg(Color::Rgb(60, 60, 70));
                let label_style = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
                let line_char = "─";
                let label = " GLOBAL SEARCH ";
                let total_w = inner_area.width as usize;
                let left_w = 4;
                let right_w = total_w.saturating_sub(left_w + label.len() + 2);
                
                f.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled(line_char.repeat(left_w), divider_style),
                        Span::styled(label, label_style),
                        Span::styled(line_char.repeat(right_w), divider_style),
                    ])),
                    Rect::new(inner_area.x, row_y, inner_area.width, 1),
                );
                continue;
            }

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

                    let alignment = match col_type {
                        FileColumn::Name => ratatui::layout::Alignment::Left,
                        _ => ratatui::layout::Alignment::Right,
                    };

                    match col_type {
                        FileColumn::Name => {
                            let is_global = file_state.local_count > 0 && file_idx > file_state.local_count && file_state.search_filter.len() > 3;
                            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
                            let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
                            let cat = crate::modules::files::get_file_category(path);
                            let icon_str = Icon::get_for_path(path, cat, is_dir, app.icon_mode);

                            if !is_selected && !is_multi_selected && !app.path_colors.contains_key(path) && !is_hovered_drop {
                                if app.semantic_coloring {
                                    if is_dir {
                                        cell_style = cell_style.fg(THEME.accent_secondary);
                                    } else {
                                        let semantic_color = match cat {
                                            crate::app::FileCategory::Script => THEME.file_code,
                                            crate::app::FileCategory::Text => THEME.file_config,
                                            crate::app::FileCategory::Image | crate::app::FileCategory::Video | crate::app::FileCategory::Audio => THEME.file_media,
                                            crate::app::FileCategory::Archive => THEME.file_archive,
                                            crate::app::FileCategory::Document => THEME.fg,
                                            _ => THEME.fg,
                                        };
                                        cell_style = cell_style.fg(semantic_color);
                                    }
                                } else if is_dir {
                                    cell_style = cell_style.fg(THEME.accent_secondary);
                                }
                            }

                            let mut spans = vec![
                                Span::styled(icon_str, cell_style),
                                Span::styled(name, cell_style),
                            ];

                            if is_global {
                                if let Some(parent) = path.parent() {
                                    let mut p_str = parent.to_string_lossy().to_string();
                                    if let Some(home) = dirs::home_dir() {
                                        if let Ok(rel) = parent.strip_prefix(&home) {
                                            p_str = format!("~/{}", rel.display());
                                        }
                                    }
                                    let path_style = if is_selected || is_multi_selected {
                                        Style::default().fg(Color::Rgb(40, 40, 40))
                                    } else {
                                        Style::default().fg(Color::Rgb(80, 80, 90))
                                    };
                                    spans.push(Span::styled(format!("  ({})", p_str), path_style));
                                }
                            }

                            if app.starred.contains(path) {
                                spans.push(Span::styled(" [*]", Style::default().fg(Color::Yellow)));
                            }

                            f.render_widget(
                                Paragraph::new(Line::from(spans)),
                                cell_rect,
                            );
                        }
                        FileColumn::Size => {
                            let size = metadata.map(|m| m.size).unwrap_or(0);
                            let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
                            let content = if is_dir && size == 0 {
                                "<DIR>".to_string()
                            } else {
                                format_size(size)
                            };
                            f.render_widget(
                                Paragraph::new(Span::styled(content, cell_style)).alignment(alignment),
                                cell_rect,
                            );
                        }
                        FileColumn::Modified => {
                            let content = format_modified_time(metadata.map(|m| m.modified).unwrap_or(SystemTime::UNIX_EPOCH));
                            f.render_widget(
                                Paragraph::new(Span::styled(content, cell_style)).alignment(alignment),
                                cell_rect,
                            );
                        }
                        FileColumn::Created => {
                            let content = format_modified_time(metadata.map(|m| m.created).unwrap_or(SystemTime::UNIX_EPOCH));
                            f.render_widget(
                                Paragraph::new(Span::styled(content, cell_style)).alignment(alignment),
                                cell_rect,
                            );
                        }
                        FileColumn::Permissions => {
                            let content = format_permissions(metadata.map(|m| m.permissions).unwrap_or(0));
                            f.render_widget(
                                Paragraph::new(Span::styled(content, cell_style)).alignment(alignment),
                                cell_rect,
                            );
                        }
                        _ => {}
                    }
                }
            }
        }

        // 3. Render Scrollbar
        if total_files > visible_height {
            let scrollbar = ratatui::widgets::Scrollbar::default()
                .orientation(ratatui::widgets::ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));

            let mut scrollbar_state = ratatui::widgets::ScrollbarState::new(total_files)
                .position(offset_val)
                .viewport_content_length(visible_height);

            f.render_stateful_widget(scrollbar, inner_area, &mut scrollbar_state);
        }
    }
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
