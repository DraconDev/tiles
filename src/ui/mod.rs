pub mod theme;
use std::path::PathBuf;

use ratatui::text::{Line, Span};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table,
    },
    Frame,
};

use crate::app::{App, AppMode, CurrentView, FileColumn};
use crate::ui::theme::THEME;
use terma::compositor::engine::TilePlacement;
use terma::utils::{format_permissions, format_size, format_time};
use terma::widgets::TermaButton;

fn draw_tabs(f: &mut Frame, area: Rect, app: &mut App) {
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
    // Files tab only
    let label = "Files";
    let width = (label.len() + 4) as u16;
    let tab_area = Rect::new(current_x, area.y, width, 1);
    f.render_widget(
        TermaButton::new(label, true, app.tile_queue.clone()),
        tab_area,
    );
    current_x += width + 1;

    // Settings and Split buttons on the right
    let settings_label = "[\u{2699}]";
    let split_label = "[\u{229e}]";
    let settings_width = 4;
    let split_width = 4;
    let right_x = area.x + area.width.saturating_sub(settings_width + split_width + 2);
    f.render_widget(
        Paragraph::new(split_label).style(Style::default().fg(Color::Cyan)),
        Rect::new(right_x, area.y, split_width, 1),
    );
    f.render_widget(
        Paragraph::new(settings_label).style(Style::default().fg(Color::Yellow)),
        Rect::new(right_x + split_width + 1, area.y, settings_width, 1),
    );

    let mut current_x_files = current_x;
    let sep = " | ";
    f.render_widget(
        Paragraph::new(sep),
        Rect::new(current_x_files, area.y, sep.len() as u16, 1),
    );
    current_x_files += sep.len() as u16;

    if app.current_view == CurrentView::Files {
        for (i, tab) in app.file_tabs.iter_mut().enumerate() {
            let is_active = i == app.tab_index;
            let style = if is_active {
                Style::default()
                    .fg(THEME.border_active)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(THEME.fg)
            };

            // Start bracket
            f.render_widget(
                Paragraph::new("[").style(style),
                Rect::new(current_x_files, area.y, 1, 1),
            );
            current_x_files += 1;

            let name = if is_active && !tab.search_filter.is_empty() {
                tab.search_filter.clone()
            } else {
                tab.current_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".to_string())
            };

            let width = name.len() as u16;
            f.render_widget(
                Paragraph::new(name.as_str()).style(style),
                Rect::new(current_x_files, area.y, width, 1),
            );
            current_x_files += width;

            // End bracket
            f.render_widget(
                Paragraph::new("]").style(style),
                Rect::new(current_x_files, area.y, 1, 1),
            );
            current_x_files += 2;
        }
    }
}

fn draw_sidebar(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Sidebar ")
        .border_style(
            if app.sidebar_focus && app.current_view == CurrentView::Files {
                Style::default().fg(THEME.border_active)
            } else {
                Style::default().fg(THEME.border_inactive)
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
            let mut sidebar_items = Vec::new();

            // FILES Section
            sidebar_items.push(
                ListItem::new("[FILES]").style(
                    Style::default()
                        .fg(THEME.accent_secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            );
            sidebar_items.push(ListItem::new("Home"));
            sidebar_items.push(ListItem::new("Downloads"));
            sidebar_items.push(ListItem::new("Documents"));
            sidebar_items.push(ListItem::new("Pictures"));

            // REMOTE Section
            sidebar_items.push(ListItem::new(""));
            sidebar_items.push(
                ListItem::new("[REMOTE]").style(
                    Style::default()
                        .fg(THEME.accent_secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            );
            for bookmark in &app.remote_bookmarks {
                sidebar_items.push(ListItem::new(bookmark.name.clone()));
            }
            if app.remote_bookmarks.is_empty() {
                sidebar_items.push(
                    ListItem::new("(No remotes)").style(Style::default().fg(Color::DarkGray)),
                );
            }

            // STORAGE Section
            sidebar_items.push(ListItem::new(""));
            sidebar_items.push(
                ListItem::new("[STORAGE]").style(
                    Style::default()
                        .fg(THEME.accent_secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            );
            for disk in &app.system_state.disks {
                let free = disk.total_space - disk.used_space;
                let disk_item = ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw(format!("{} - ", disk.name)),
                    ratatui::text::Span::styled(
                        format!("{:.0}GB Free", free),
                        Style::default().fg(Color::Green),
                    ),
                ]);
                sidebar_items.push(ListItem::new(disk_item));
            }
            if app.system_state.disks.is_empty() {
                sidebar_items.push(ListItem::new("Root (/)"));
                sidebar_items.push(ListItem::new("Media"));
            }

            let items: Vec<ListItem> = sidebar_items
                .into_iter()
                .enumerate()
                .map(|(i, item): (usize, ListItem)| {
                    // Check if this row is actually selectable (not a header or empty)
                    let is_selectable = i > 0 && i < 5
                        || (i > 7 && i < 7 + app.remote_bookmarks.len())
                        || i >= 9 + app.remote_bookmarks.len().max(1);

                    if !is_selectable {
                        return item.clone().style(Style::default().fg(Color::DarkGray));
                    }

                    if i == app.sidebar_index && app.sidebar_focus {
                        item.clone().style(
                            Style::default()
                                .fg(THEME.border_active)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else if i == app.sidebar_index && !app.sidebar_focus {
                        item.clone()
                            .style(Style::default().fg(THEME.fg).add_modifier(Modifier::BOLD))
                    } else {
                        item.clone().style(Style::default().fg(THEME.fg))
                    }
                })
                .collect();

            f.render_widget(List::new(items), inner);
        }
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    // draw_tabs(f, chunks[0], app); // Removed

    let workspace = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Min(0)])
        .split(chunks[0]); // Was chunks[1]

    draw_sidebar(f, workspace[0], app);
    draw_main_stage(f, workspace[1], app);

    draw_footer(f, chunks[1], app); // Was chunks[2]

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
        if let Some(split_idx) = app.split_index {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(area);

            let left_focused = !app.focus_right && !app.sidebar_focus;
            let right_focused = app.focus_right && !app.sidebar_focus;

            draw_file_view(f, chunks[0], app, app.tab_index, left_focused);
            draw_file_view(f, chunks[1], app, split_idx, right_focused);
        } else {
            let is_focused = !app.sidebar_focus;
            draw_file_view(f, area, app, app.tab_index, is_focused);
        }
    }
}

use std::time::SystemTime;

fn draw_file_view(f: &mut Frame, area: Rect, app: &mut App, tab_idx: usize, is_focused: bool) {
    // Split area into Tabs (Top) and Content (Bottom)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Local Tab Strip
            Constraint::Min(0),    // Content
        ])
        .split(area);

    let tabs_area = chunks[0];
    let content_area = chunks[1];

    // Draw Local Tabs
    let titles: Vec<Line> = app
        .file_tabs
        .iter()
        .enumerate()
        .map(|(i, tab)| {
            // Simplified title: Folder Name or Search
            let name = if i == tab_idx && !tab.search_filter.is_empty() {
                format!("Search: {}", tab.search_filter)
            } else {
                tab.current_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".to_string())
            };

            // Highlight active tab
            if i == tab_idx {
                Line::from(Span::styled(
                    format!(" {} ", name),
                    Style::default()
                        .fg(THEME.bg)
                        .bg(THEME.accent_primary)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(
                    format!(" {} ", name),
                    Style::default().fg(Color::DarkGray),
                ))
            }
        })
        .collect();

    // For now, simple horizontal joining with spaces
    // Since Ratatui Tabs widget expects selected_index to match resizing, we might just render spans manually or use Tabs
    // Let's use customized manual rendering for tight control or the Tabs widget
    let tabs = ratatui::widgets::Tabs::new(titles)
        .select(tab_idx)
        .highlight_style(Style::default().fg(THEME.accent_primary))
        .divider(" ");
    f.render_widget(tabs, tabs_area);

    if let Some(file_state) = app.file_tabs.get_mut(tab_idx) {
        file_state.view_height = content_area.height as usize;
        let mut render_state = ratatui::widgets::TableState::default();
        if let Some(sel) = file_state.selected_index {
            let offset = file_state.table_state.offset();
            // Capacity = Height - 2 (Borders) - 1 (Header) - 1 (Safety Margin)
            let capacity = file_state.view_height.saturating_sub(4);

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

        let sort_col = file_state.sort_column;
        let sort_asc = file_state.sort_ascending;
        let header_cells = file_state.columns.iter().map(|c| {
            let base_name = match c {
                FileColumn::Name => "Name",
                FileColumn::Size => "Size",
                FileColumn::Modified => "Modified",
                FileColumn::Created => "Created",
                FileColumn::Permissions => "Permissions",
                FileColumn::Extension => "Ext",
            };
            let name = if *c == sort_col {
                if sort_asc {
                    format!("{} ▲", base_name)
                } else {
                    format!("{} ▼", base_name)
                }
            } else {
                base_name.to_string()
            };
            Cell::from(name).style(
                Style::default()
                    .fg(THEME.header_fg)
                    .add_modifier(Modifier::BOLD),
            )
        });
        let header = Row::new(header_cells).height(1).bottom_margin(0);

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
                        // Extension-based color coding
                        let ext = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("")
                            .to_lowercase();
                        let ext_color = match ext.as_str() {
                            "rs" | "py" | "c" | "cpp" | "h" | "hpp" | "js" | "ts" | "go"
                            | "java" | "rb" | "php" | "sh" => THEME.file_code,
                            "toml" | "json" | "yaml" | "yml" | "xml" | "ini" | "conf" | "cfg" => {
                                THEME.file_config
                            }
                            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "svg" | "mp4" | "mkv"
                            | "avi" | "mp3" | "wav" => THEME.file_media,
                            "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar" => {
                                THEME.file_archive
                            }
                            "exe" | "bin" | "elf" => THEME.file_exec,
                            _ => {
                                // Check for executable permissions if available
                                if let Some(meta) = metadata {
                                    if meta.permissions & 0o100 != 0 {
                                        THEME.file_exec
                                    } else {
                                        THEME.fg
                                    }
                                } else {
                                    THEME.fg
                                }
                            }
                        };
                        Style::default().fg(ext_color)
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

                    // No indentation - display name directly
                    Cell::from(display_name).style(style)
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
            let style = if Some(i) == file_state.selected_index && is_focused {
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
        let mut breadcrumb_spans = Vec::new();
        file_state.breadcrumb_bounds.clear();

        let path = file_state.current_path.clone();
        let components: Vec<_> = path.components().collect();
        let mut current_path = PathBuf::new();

        // Calculate actual screen coordinates for segments to enable hover/click
        // Each segment is "name" (no spaces)
        let mut current_pos_x = area.x + 2; // Approximate start offset inside block title " [breadcrumb] "

        for (i, comp) in components.iter().enumerate() {
            match comp {
                std::path::Component::RootDir => {
                    current_path.push("/");
                }
                std::path::Component::Prefix(p) => {
                    current_path.push(p.as_os_str());
                }
                std::path::Component::Normal(name) => {
                    current_path.push(name);
                }
                _ => continue,
            }

            let name = comp.as_os_str().to_string_lossy().to_string();
            let display_name = if name == "/" { "".to_string() } else { name };

            if !display_name.is_empty() || i == 0 {
                let segment_path = current_path.clone();
                let is_hovered =
                    file_state.hovered_breadcrumb == Some(file_state.breadcrumb_bounds.len());

                let style = if is_hovered {
                    Style::default()
                        .fg(THEME.accent_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME.fg)
                };

                let text = display_name;
                let width = text.len() as u16;
                breadcrumb_spans.push(Span::styled(text.clone(), style));

                // Store absolute screen bounds (start_x, end_x, path)
                file_state.breadcrumb_bounds.push((
                    current_pos_x,
                    current_pos_x + width,
                    segment_path,
                ));
                current_pos_x += width;

                if i < components.len() - 1 {
                    let sep_color = if is_focused {
                        THEME.accent_primary
                    } else {
                        Color::DarkGray
                    };
                    breadcrumb_spans.push(Span::styled("/", Style::default().fg(sep_color)));
                    current_pos_x += 1;
                }
            }
        }

        // Add search filter if active
        if !file_state.search_filter.is_empty() {
            breadcrumb_spans.push(Span::styled(
                format!(" [ {} ]", file_state.search_filter),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(Line::from(breadcrumb_spans))
            .border_style(if is_focused {
                Style::default().fg(THEME.border_active)
            } else {
                Style::default().fg(THEME.border_inactive)
            });

        let table = Table::new(rows, constraints).header(header).block(block);

        let height = area.height.saturating_sub(2) as usize; // Account for borders
        let offset = render_state.offset();
        let selected = render_state.selected();

        // Fix for "Scroll Glitch":
        // If the selected item is NOT in the current view range (offset..offset+height),
        // we must effectively "hide" the selection from the Table widget during this render pass.
        // Otherwise, Table will forcibly snap the offset back to bring the selection into view,
        // undoing any manual mouse scrolling.
        let mut display_state = render_state.clone();
        if let Some(sel) = selected {
            if sel < offset || sel >= offset + height {
                display_state.select(None);
            }
        }

        f.render_stateful_widget(table, area, &mut display_state);

        // Write back the offset to the persistent state, in case Table adjusted it (e.g. bottom clamp)
        *file_state.table_state.offset_mut() = display_state.offset();

        // Scrollbar logic:
        // Use Safety Margin (sub(4)) to match scrolling logic.
        if file_state.files.len() > area.height.saturating_sub(4) as usize {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .style(Style::default().fg(Color::Yellow));

            let mut scrollbar_state = ScrollbarState::new(file_state.files.len())
                .position(file_state.table_state.offset());

            // Render with 1-char gutter from border (width-3)
            // This ensures it is always visible and doesn't clash with borders.
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(3),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };
            f.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
        }
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let mut spans = Vec::new();

    // CPU
    spans.push(ratatui::text::Span::styled(
        format!("CPU: {:.0}%", app.system_state.cpu_usage),
        Style::default().fg(Color::Green),
    ));
    spans.push(ratatui::text::Span::raw(" | "));

    // Memory
    if app.system_state.total_mem > 0.0 {
        let mem_percent = (app.system_state.mem_usage / app.system_state.total_mem) * 100.0;
        spans.push(ratatui::text::Span::styled(
            format!("Mem: {:.0}%", mem_percent),
            Style::default().fg(Color::Yellow),
        ));
        spans.push(ratatui::text::Span::raw(" | "));
    }

    // Storage
    let mut total_used = 0.0;
    let mut total_space = 0.0;
    for disk in &app.system_state.disks {
        total_used += disk.used_space;
        total_space += disk.total_space;
    }

    if total_space > 0.0 {
        let storage_percent = (total_used / total_space) * 100.0;
        spans.push(ratatui::text::Span::styled(
            format!("Storage: {:.0}%", storage_percent),
            Style::default().fg(Color::Cyan),
        ));
    } else if let Some(disk) = app.system_state.disks.first() {
        let free = disk.total_space - disk.used_space;
        spans.push(ratatui::text::Span::styled(
            format!("Storage: {:.1}GB", free),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Right-align the footer content
    let line = ratatui::text::Line::from(spans);
    f.render_widget(
        Paragraph::new(line).alignment(ratatui::layout::Alignment::Right),
        area,
    );
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
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
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Yellow)),
        ),
        area,
    );
}

fn draw_new_folder_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(40, 10, f.area());
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(app.input.as_str()).block(
            Block::default()
                .title(" New Folder ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
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
                .border_type(BorderType::Rounded)
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
    };
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title(" Confirm Action ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
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
    };
    f.render_widget(
        Paragraph::new(info).block(
            Block::default()
                .title(" Properties ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
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
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            area,
        );
    }
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
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Green)),
        ),
        area,
    );
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
