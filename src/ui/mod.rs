pub mod theme;
use std::path::PathBuf;

use crate::app::{App, AppMode, CurrentView, DropTarget, FileColumn, SidebarBounds, SidebarTarget, SettingsSection, SettingsTarget};
use crate::ui::theme::THEME;
use ratatui::text::{Line, Span};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, List, ListItem, Paragraph, Row, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Table, Tabs
    },
    Frame,
};

use terma::utils::{format_permissions, format_size, format_time};

fn draw_sidebar(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Files ")
        .border_style(
            if app.sidebar_focus && app.current_view == CurrentView::Files {
                Style::default().fg(THEME.border_active)
            } else {
                Style::default().fg(THEME.border_inactive)
            },
        );
    f.render_widget(block, area);

    /*
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
    */

    let inner = area.inner(ratatui::layout::Margin {
        vertical: 1,
        horizontal: 1,
    });
    match app.current_view {
        CurrentView::Files => {
            let mut sidebar_items = Vec::new();
            app.sidebar_bounds.clear();
            let mut current_y = inner.y;
            let is_dragging_over_sidebar = app.is_dragging && app.drag_source.is_some() && app.mouse_pos.0 < area.width;

            if is_dragging_over_sidebar {
                let current_idx = sidebar_items.len();
                sidebar_items.push(
                    ListItem::new("> FAVORITES")
                        .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                );
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_idx, // Use 0-index
                    target: SidebarTarget::Header("FAVORITES".to_string()),
                });
                current_y += 1;
            } else {
                // Removed FAVORITES header - top section is implicitly favorites
            }

            // Render Starred Folders (No sorting to allow reordering)
            for path in &app.starred {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or("?".to_string());

                let current_idx = sidebar_items.len();
                let is_focused = app.sidebar_focus && app.sidebar_index == current_idx;
                let is_hovered =
                    matches!(&app.hovered_drop_target, Some(DropTarget::Folder(p)) if p == path);
                
                // Active highlighting: Is this path open in the focused pane?
                let is_active = if let Some(fs) = app.current_file_state() {
                    fs.current_path == *path
                } else {
                    false
                };

                let label = ListItem::new(name);
                let mut style = if is_active {
                    Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME.fg)
                };

                if is_focused {
                    style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
                } else if is_hovered && app.is_dragging {
                    style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
                } else if matches!(&app.drag_source, Some(s) if s == path) && app.is_dragging {
                    style = style.fg(Color::Green).add_modifier(Modifier::BOLD);
                }
                
                sidebar_items.push(label.style(style));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_idx,
                    target: SidebarTarget::Favorite(path.clone()),
                });
                current_y += 1;
            }

            // REMOTE Section
            sidebar_items.push(ListItem::new(""));
            current_y += 1;

            let current_header_idx = sidebar_items.len();
            sidebar_items.push(
                ListItem::new("󰒍 REMOTES").style(
                    Style::default()
                        .fg(THEME.accent_secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            );
            app.sidebar_bounds.push(SidebarBounds {
                y: current_y,
                index: current_header_idx,
                target: SidebarTarget::Header("REMOTES".to_string()),
            });
            current_y += 1;
            for (i, bookmark) in app.remote_bookmarks.iter().enumerate() {
                let current_bookmark_idx = sidebar_items.len();
                let is_focused = app.sidebar_focus && app.sidebar_index == current_bookmark_idx;
                
                let is_active = if let Some(fs) = app.current_file_state() {
                    fs.remote_session.as_ref().map(|s| s.host == bookmark.host).unwrap_or(false)
                } else {
                    false
                };

                let mut style = if is_active {
                    Style::default().fg(THEME.accent_primary).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(THEME.fg)
                };

                if is_focused {
                    style = style
                        .bg(THEME.accent_primary)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                }

                sidebar_items.push(ListItem::new(bookmark.name.clone()).style(style));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_bookmark_idx,
                    target: SidebarTarget::Remote(i),
                });
                current_y += 1;
            }
            if app.remote_bookmarks.is_empty() {
                sidebar_items.push(
                    ListItem::new("(No remotes)").style(Style::default().fg(Color::DarkGray)),
                );
                current_y += 1;
            }

            // STORAGE Section
            sidebar_items.push(ListItem::new(""));
            current_y += 1;
            let current_storage_header_idx = sidebar_items.len();
            sidebar_items.push(
                ListItem::new("󰋊 STORAGES").style(
                    Style::default()
                        .fg(THEME.accent_secondary)
                        .add_modifier(Modifier::BOLD),
                ),
            );
            app.sidebar_bounds.push(SidebarBounds {
                y: current_y,
                index: current_storage_header_idx, // elementary logic says index should match sidebar_items.len()
                target: SidebarTarget::Header("STORAGES".to_string()),
            });
            current_y += 1;
            
            // Collect current path from focused pane
            let active_path = app.current_file_state().map(|fs| fs.current_path.to_string_lossy().to_string());

            for (i, disk) in app.system_state.disks.iter().enumerate() {
                // Check if focused pane path starts with this disk's mount point
                let is_active = if let Some(ref path) = active_path {
                    path.starts_with(&disk.name)
                        || (disk.name == "/"
                            && !app
                                .system_state
                                .disks
                                .iter()
                                .any(|d| d.name != "/" && path.starts_with(&d.name)))
                } else {
                    false
                };

                let current_disk_idx = sidebar_items.len();
                let is_focused = app.sidebar_focus && app.sidebar_index == current_disk_idx;
                let mut name_style = if is_active {
                    Style::default()
                        .fg(THEME.accent_primary)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Green)
                };
                if is_focused {
                    name_style = name_style
                        .bg(THEME.accent_primary)
                        .fg(Color::Black)
                        .add_modifier(Modifier::BOLD);
                }

                let available = (disk.available_space as f64 / 1_073_741_824.0).round() as u64; // GB
                let label = format!("{}: {}G Free", disk.name, available);
                sidebar_items.push(ListItem::new(label.clone()).style(name_style));
                app.sidebar_bounds.push(SidebarBounds {
                    y: current_y,
                    index: current_disk_idx, // elementary logic says sidebar_index should be compared with current_disk_idx
                    target: SidebarTarget::Storage(i),
                });
                current_y += 1;
            }

            if app.system_state.disks.is_empty() {
                sidebar_items.push(ListItem::new("Root (/)"));
                sidebar_items.push(ListItem::new("Media"));
            }

            let items: Vec<ListItem> = sidebar_items
                .into_iter()
                .enumerate()
                .map(|(i, item): (usize, ListItem)| {
                    if i == app.sidebar_index && app.sidebar_focus {
                        item.style(
                            Style::default()
                                .fg(Color::Black)
                                .bg(THEME.accent_primary)
                                .add_modifier(Modifier::BOLD),
                        )
                    } else {
                        // Keep the style already set on the item (for active highlighting)
                        item
                    }
                })
                .collect();

            f.render_widget(List::new(items), inner);
        }
        CurrentView::Processes => {
            // Placeholder for Processes sidebar
        }
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Global Header
            Constraint::Min(0),    // Workspace
            Constraint::Length(1), // Footer
        ])
        .split(f.area());

    let workspace = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20), Constraint::Min(0)])
        .split(chunks[1]);

    let sidebar_area = workspace[0];
    let main_stage_area = workspace[1];

    draw_global_header(f, chunks[0], sidebar_area.width, app);
    crate::app::log_debug("Header done");
    draw_sidebar(f, sidebar_area, app);
    crate::app::log_debug("Sidebar done");
    draw_main_stage(f, main_stage_area, app);
    crate::app::log_debug("Main stage done");
    draw_footer(f, chunks[2], app);
    crate::app::log_debug("Draw complete");

    if let AppMode::ContextMenu { x, y, ref target } = app.mode {
        draw_context_menu(f, x, y, target, app);
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
    if matches!(app.mode, AppMode::Settings) {
        draw_settings_modal(f, app);
    }
    if matches!(app.mode, AppMode::CommandPalette) {
        draw_command_palette(f, app);
    }
    if matches!(app.mode, AppMode::AddRemote) {
        draw_add_remote_modal(f, app);
    }
}

fn draw_global_header(f: &mut Frame, area: Rect, sidebar_width: u16, app: &mut App) {
    let pane_count = app.panes.len();
    
    // Settings Button (Top-Left)
    let menu_label = " Settings ";
    let menu_width = 10;
    let menu_rect = Rect::new(area.x, area.y, menu_width, 1);
    
    f.render_widget(
        Paragraph::new(menu_label).style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        menu_rect,
    );

    // Split Button (Dynamic Icon)
    let split_label = if pane_count > 1 { "[□]" } else { "[◫]" };
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
    // Align Tabs with the Panes (skip Sidebar)
    // Ensure tabs start AFTER sidebar, but also don't overlap menu if sidebar is tiny (unlikely)
    // Also account for the wider Settings button
    let start_x = std::cmp::max(area.x + sidebar_width, area.x + menu_width + 1);
    
    // Use full width to ensure alignment with panes
    let end_x = area.x + area.width;
    
    let tabs_width = end_x.saturating_sub(start_x);
    
    if tabs_width > 0 {
        let tabs_area = Rect::new(start_x, area.y, tabs_width, 1);
        
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

                f.render_widget(Paragraph::new(text).style(style), Rect::new(current_x, area.y, width, 1));
                current_x += width + 1;
            }
        }
    }
}


fn draw_main_stage(f: &mut Frame, area: Rect, app: &mut App) {
    if app.current_view == CurrentView::Files {
        let pane_count = app.panes.len();
        if pane_count == 0 {
            return;
        }

        // Content area is the full area passed in (workspace[1])
        let content_area = area;
        let constraints = vec![Constraint::Ratio(1, pane_count as u32); pane_count];
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(content_area);

        for i in 0..pane_count {
            let is_focused = i == app.focused_pane_index && !app.sidebar_focus;
            // User requested left side for right panel too. 
            // Using Borders::ALL for all panels ensures they each have their own box.
            let borders = Borders::ALL;
            draw_file_view(f, chunks[i], app, i, is_focused, borders);
        }
    }
}

use std::time::SystemTime;

fn draw_file_view(
    f: &mut Frame,
    area: Rect,
    app: &mut App,
    pane_idx: usize,
    is_focused: bool,
    borders: Borders,
) {
    // REMOVED Local Tab Strip. Use full area for content.
    let content_area = area;

    if let Some(file_state) = app
        .panes
        .get_mut(pane_idx)
        .and_then(|p| p.current_state_mut())
    {
        file_state.view_height = content_area.height as usize;
        let mut render_state = ratatui::widgets::TableState::default();
        if let Some(sel) = file_state.selected_index {
            let offset = file_state.table_state.offset();
            let capacity = file_state.view_height.saturating_sub(3);

            if sel >= offset && sel < offset + capacity {
                render_state.select(Some(sel));
            } else {
                render_state.select(None);
            }
        }
        // Force the render state offset to match our manual offset
        *render_state.offset_mut() = file_state.table_state.offset();

        // 1. Calculate Constraints and Layout first to get column widths
        let constraints: Vec<Constraint> = file_state
            .columns
            .iter()
            .map(|c| match c {
                FileColumn::Name => Constraint::Min(20),
                FileColumn::Size => Constraint::Length(10),
                FileColumn::Modified => Constraint::Percentage(20),
                FileColumn::Permissions => Constraint::Length(12),
            })
            .collect();

        // Need a dummy block to calculate inner area
        let dummy_block = Block::default().borders(borders);
        let column_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints.clone())
            .spacing(0)
            .split(dummy_block.inner(content_area));
        
        let name_col_width = column_layout.get(0).map(|r| r.width as usize).unwrap_or(20);

        let sort_col = file_state.sort_column;
        let sort_asc = file_state.sort_ascending;
        let header_cells = file_state.columns.iter().map(|c| {
            let base_name = match c {
                FileColumn::Name => "Name",
                FileColumn::Size => "Size",
                FileColumn::Modified => "Modified",
                FileColumn::Created => "Created",
                FileColumn::Permissions => "Permissions",
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
            if path.to_string_lossy() == "__DIVIDER__" {
                let cells = file_state.columns.iter().enumerate().map(|(col_idx, _)| {
                    if col_idx == 0 {
                        Cell::from("> Global results").style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
                    } else {
                        Cell::from("──────────────────").style(Style::default().fg(Color::DarkGray))
                    }
                });
                return Row::new(cells);
            }

            let metadata = file_state.metadata.get(path);
            let is_multi_selected = file_state.multi_select.contains(&i) && is_focused;

            let cells = file_state.columns.iter().map(|c| match c {
                FileColumn::Name => {
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("..");
                    let is_dir = metadata.map(|m| m.is_dir).unwrap_or(false);
                    let mut final_style = if is_dir {
                        Style::default().fg(THEME.accent_secondary)
                    } else {
                        Style::default().fg(THEME.fg)
                    };

                    let mut suffix = String::new();
                    if app.starred.contains(path) {
                        suffix.push_str(" [*]");
                        final_style = final_style
                            .fg(THEME.accent_primary)
                            .add_modifier(Modifier::BOLD);
                    }

                    // For global results, use smart path display
                    if i > file_state.local_count {
                        let full_str = path.to_string_lossy();
                        let mut display_path = if full_str.starts_with("/home/dracon") {
                            full_str.replacen("/home/dracon", "~", 1)
                        } else {
                            full_str.to_string()
                        };
                        
                        display_path.push_str(&suffix);

                        // Smart truncation: show the END of the path if it's too long
                        if display_path.len() > name_col_width && name_col_width > 5 {
                            let keep_len = name_col_width - 3;
                            let start_idx = display_path.len() - keep_len;
                            display_path = format!("...{}", &display_path[start_idx..]);
                        }
                        
                        Cell::from(display_path).style(final_style)
                    } else {
                        Cell::from(format!("{}{}", name, suffix)).style(final_style)
                    }
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
                FileColumn::Permissions => Cell::from(format_permissions(
                    metadata.map(|m| m.permissions).unwrap_or(0),
                ))
                .style(Style::default().fg(THEME.fg)),
            });

            let mut row_style = Style::default();
            if is_multi_selected {
                row_style = row_style.bg(Color::Rgb(100, 0, 0)).fg(Color::White);
            }
            Row::new(cells).style(row_style)
        });

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
            // Fix: RootDir should be displayed as "/" and have width 1.
            let display_name = if name == "/" { "/".to_string() } else { name };

            if !display_name.is_empty() {
                let segment_path = current_path.clone();
                let is_hovered = file_state.hovered_breadcrumb.as_ref() == Some(&segment_path);

                let style = if is_hovered {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                } else {
                    Style::default().fg(THEME.fg)
                };

                let text = display_name.clone();
                let width = text.len() as u16;
                breadcrumb_spans.push(Span::styled(text.clone(), style));

                // Store absolute screen bounds (Rect, path)
                file_state.breadcrumb_bounds.push((
                    Rect::new(current_pos_x, area.y, width, 1), // Exact row of the border title
                    segment_path,
                ));
                current_pos_x += width;

                // Add separator only if NOT last and NOT RootDir (since RootDir is its own separator visually)
                if i < components.len() - 1 && text != "/" {
                    breadcrumb_spans.push(Span::styled("/", Style::default().fg(Color::DarkGray)));
                    current_pos_x += 1;
                }
            }
        }

        // Add git branch if available
        if let Some(branch) = &file_state.git_branch {
            breadcrumb_spans.push(Span::styled(
                format!(" ({})", branch),
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ));
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
            .borders(borders)
            .border_type(BorderType::Rounded)
            .title(Line::from(breadcrumb_spans))
            .border_style(if is_focused {
                Style::default().fg(THEME.border_active)
            } else {
                Style::default().fg(THEME.border_inactive)
            });

        let table = Table::new(rows, constraints.clone())
            .header(header)
            .block(block.clone())
            .column_spacing(0) // Fix alignment and gaps
            .row_highlight_style(
                Style::default()
                    .bg(THEME.accent_primary)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ); // Disable default teal highlighting
               // Fix: Use content_area instead of area to avoid overlapping with Tabs!
               // Also update height calculation to use content_area.
        let height = content_area.height.saturating_sub(2) as usize; // Account for borders
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

        // --- POPULATE COLUMN BOUNDS FOR CLICK DETECTION ---
        file_state.column_bounds.clear();
        let column_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints.clone())
            .spacing(0) // Match table spacing
            .split(block.inner(content_area));

        for (i, col_type) in file_state.columns.iter().enumerate() {
            let rect = column_layout[i];
            file_state.column_bounds.push((rect, *col_type));
        }
        // --------------------------------------------------

        f.render_stateful_widget(table, content_area, &mut display_state);

        // Write back the offset to the persistent state, in case Table adjusted it (e.g. bottom clamp)
        *file_state.table_state.offset_mut() = display_state.offset();

        // Scrollbar logic:
        // Use Safety Margin (sub(4)) to match scrolling logic.
        if file_state.files.len() > area.height.saturating_sub(4) as usize {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));
            let mut scroll_state = ScrollbarState::new(file_state.files.len())
                .position(file_state.table_state.offset());
            f.render_stateful_widget(scrollbar, area, &mut scroll_state);
        }
    }
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);

    // Left: Shortcuts
    let shortcuts = vec![
        Span::styled(" ^Q ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Quit "),
        Span::styled(" ^S ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Split "),
        Span::styled(" ^T ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Term "),
        Span::styled(" ^Spc ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Cmd "),
        Span::styled(" ^H ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Hidden "),
        Span::styled(" Esc ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Back "),
    ];
    f.render_widget(Paragraph::new(Line::from(shortcuts)), chunks[0]);

    // Right: System Stats
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
        chunks[1],
    );
}

fn draw_context_menu(f: &mut Frame, x: u16, y: u16, target: &crate::app::ContextMenuTarget, _app: &App) {
    let mut items = Vec::new();
    let title;
    
    match target {
        crate::app::ContextMenuTarget::File(_) => {
            title = " File ";
            items.push(ListItem::new(" 󰚩 Edit (Demon)"));
            items.push(ListItem::new(" 󰓎 Star"));
            items.push(ListItem::new(" 󰏫 Rename"));
            items.push(ListItem::new(" 󰆴 Delete"));
            items.push(ListItem::new(" 󰈙 Properties"));
        }
        crate::app::ContextMenuTarget::Folder(_) => {
            title = " Folder ";
            items.push(ListItem::new(" 󰉋 Open"));
            items.push(ListItem::new(" 󰓎 Star"));
            items.push(ListItem::new(" 󰏫 Rename"));
            items.push(ListItem::new(" 󰆴 Delete"));
        }
        crate::app::ContextMenuTarget::EmptySpace => {
            title = " Actions ";
            items.push(ListItem::new(" 󰉋 New Folder"));
            items.push(ListItem::new(" 󰈔 New File"));
            items.push(ListItem::new(" 󰑐 Refresh"));
            items.push(ListItem::new(" 󰆍 Terminal Here"));
        }
        crate::app::ContextMenuTarget::SidebarFavorite(_) => {
            title = " Favorite ";
            items.push(ListItem::new(" 󰓏 Unstar"));
            items.push(ListItem::new(" 󰉋 Open in new tab"));
        }
        crate::app::ContextMenuTarget::SidebarRemote(_) => {
            title = " Remote ";
            items.push(ListItem::new(" 󰒍 Connect"));
            items.push(ListItem::new(" 󰆴 Remove"));
        }
        crate::app::ContextMenuTarget::SidebarStorage(_) => {
            title = " Storage ";
            items.push(ListItem::new(" 󰋊 Mount"));
            items.push(ListItem::new(" 󰋊 Unmount"));
        }
    }
    let width = 20;
    let height = items.len() as u16 + 2;

    // Boundary check to prevent crash near edges
    let mut safe_x = x;
    let mut safe_y = y;

    if safe_x + width > f.area().width {
        safe_x = f.area().width.saturating_sub(width);
    }
    if safe_y + height > f.area().height {
        safe_y = f.area().height.saturating_sub(height);
    }

    let area = Rect::new(safe_x, safe_y, width, height);
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
            ListItem::new(cmd.desc.clone()).style(style)
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
        CurrentView::Processes => "Kill Process? (y/n)".to_string(),
    };
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title(" Confirm Delete ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Red)),
        ),
        area,
    );
}

fn draw_properties_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 50, f.area());
    f.render_widget(Clear, area);
    if let Some(fs) = app.current_file_state() {
        if let Some(idx) = fs.selected_index {
            if let Some(path) = fs.files.get(idx) {
                let meta = fs.metadata.get(path);
                let mut lines = Vec::new();
                lines.push(Line::from(format!("Name: {}", path.display())));
                if let Some(m) = meta {
                    lines.push(Line::from(format!("Size: {}", format_size(m.size))));
                    lines.push(Line::from(format!("Modified: {}", format_time(m.modified))));
                    lines.push(Line::from(format!("Created: {}", format_time(m.created))));
                    lines.push(Line::from(format!(
                        "Permissions: {}",
                        format_permissions(m.permissions)
                    )));
                }
                f.render_widget(
                    Paragraph::new(lines).block(
                        Block::default()
                            .title(" Properties ")
                            .borders(Borders::ALL)
                            .border_type(BorderType::Rounded)
                            .border_style(Style::default().fg(Color::Blue)),
                    ),
                    area,
                );
            }
        }
    }
}

fn draw_settings_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(80, 80, f.area());
    f.render_widget(Clear, area);
    
    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan));
    
    let inner_area = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(15), Constraint::Min(0)])
        .split(inner_area);
    
    // Left: Sections
    let sections = vec![
        ListItem::new(" 󰟜 Columns "),
        ListItem::new(" 󰓩 Tabs "),
        ListItem::new(" 󰒓 General "),
    ];
    
    use ratatui::widgets::ListState;
    let mut section_state = ListState::default();
    section_state.select(Some(match app.settings_section {
        SettingsSection::Columns => 0,
        SettingsSection::Tabs => 1,
        SettingsSection::General => 2,
    }));

    let section_list = List::new(sections)
        .block(Block::default().borders(Borders::RIGHT).border_style(Style::default().fg(Color::DarkGray)))
        .highlight_style(Style::default().bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD));
    
    f.render_stateful_widget(section_list, chunks[0], &mut section_state);

    // Right: Content
    match app.settings_section {
        SettingsSection::Columns => draw_column_settings(f, chunks[1], app),
        SettingsSection::Tabs => draw_tab_settings(f, chunks[1], app),
        SettingsSection::General => draw_general_settings(f, chunks[1], app),
    }
}

fn draw_column_settings(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // Target Selection
    let titles = vec![" [Single] ", " [Split] "];
    let sel = match app.settings_target {
        SettingsTarget::SingleMode => 0,
        SettingsTarget::SplitMode => 1,
    };
    
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::BOTTOM).title(" Configure Mode "))
        .select(sel)
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, chunks[0]);

    let options = vec![
        (FileColumn::Size, "Size (s)"),
        (FileColumn::Modified, "Modified (m)"),
        (FileColumn::Permissions, "Permissions (p)"),
    ];
    
    let target_cols = match app.settings_target {
        SettingsTarget::SingleMode => &app.single_columns,
        SettingsTarget::SplitMode => &app.split_columns,
    };

    let items: Vec<ListItem> = options
        .iter()
        .map(|(col, label)| {
            let prefix = if target_cols.contains(col) {
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
                .title(" Visible Columns ")
                .borders(Borders::NONE),
        ),
        chunks[1],
    );
}

fn draw_tab_settings(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("Tab/Pane Persistence", Style::default().add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from("Each pane and tab maintains its own state (Nautilus-style)."));
    lines.push(Line::from("Current Pane Count: ".to_string() + &app.panes.len().to_string()));
    
    f.render_widget(
        Paragraph::new(lines).block(Block::default().title(" Tabs & Panes ").borders(Borders::NONE)),
        area,
    );
}

fn draw_general_settings(f: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    
    let hidden_state = if app.default_show_hidden { "[x]" } else { "[ ]" };
    lines.push(Line::from(format!("{} Show Hidden by Default (h)", hidden_state)));
    
    let confirm_state = if app.confirm_delete { "[x]" } else { "[ ]" };
    lines.push(Line::from(format!("{} Confirm on Delete (d)", confirm_state)));
    
    lines.push(Line::from(""));
    lines.push(Line::from("Terminal: ".to_string() + app.preferred_terminal.as_deref().unwrap_or("System Default")));
    
    f.render_widget(
        Paragraph::new(lines).block(Block::default().title(" General Preferences ").borders(Borders::NONE)),
        area,
    );
}

fn draw_add_remote_modal(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 40, f.area());
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(app.input.as_str()).block(
            Block::default()
                .title(" Add Remote Host (user@host:port) ")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::Cyan)),
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