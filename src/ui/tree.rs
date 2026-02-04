use crate::app::App;
use crate::state::TreeColumn;
use crate::ui::theme::THEME;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, List, ListItem, ListState, StatefulWidget, Widget},
    Frame,
};

pub fn draw_tree_view(f: &mut Frame, area: Rect, app: &mut App) {
    if app.tree_state.active_columns.is_empty() {
        crate::events::tree::refresh_tree(app);
    }

    if app.tree_state.active_columns.is_empty() {
        return;
    }

    // 1. Calculate Expanded Heights (Bottom-Up)
    let mut expanded_heights = vec![0; app.tree_state.active_columns.len()];
    // Iterate backwards
    for i in (0..app.tree_state.active_columns.len()).rev() {
        let col = &app.tree_state.active_columns[i];
        let mut base_height = col.items.len();
        if !col.sections.is_empty() {
            // Stacked column height = sum of section heights (calculated based on area height assumption?)
            // Actually, stacked cols in this mode probably behave like flat lists for height?
            // "new column on the right has 5 length".
            // If stacked, it's just total items + headers?
            // Let's assume calculated visual height.
            let (_, h) = app.terminal_size;
            let area_h = h.saturating_sub(2);
            let section_heights = col.calculate_section_heights(area_h);
            base_height = section_heights.iter().map(|&x| x as usize + 2).sum();
        }

        if i == app.tree_state.active_columns.len() - 1 {
            expanded_heights[i] = base_height;
        } else {
            let child_h = expanded_heights[i + 1];
            // Parent height = (items - 1) + child_h
            // Guard against empty items
            if base_height == 0 {
                expanded_heights[i] = 0; // Or child_h? No parent item to anchor.
            } else {
                expanded_heights[i] = (base_height - 1) + child_h;
            }
        }
    }

    // 2. Calculate Widths
    let col_widths: Vec<u16> = app
        .tree_state
        .active_columns
        .iter()
        .map(|col| col.width())
        .collect();

    // 3. Render Columns (Recursive Position)
    let mut current_x = area.x;
    let mut parent_focus_offset_y = 0;

    for i in 0..app.tree_state.active_columns.len() {
        let col = &app.tree_state.active_columns[i];
        let width = col_widths[i];

        // Check X visibility
        if current_x >= area.x + area.width {
            break;
        }
        let render_width = std::cmp::min(width, (area.x + area.width).saturating_sub(current_x));
        if render_width == 0 {
            break;
        }

        let col_height = expanded_heights[i];
        // Calculate child height (spacer size)
        let child_h = if i + 1 < expanded_heights.len() {
            expanded_heights[i + 1]
        } else {
            0
        };
        let spacer_size = child_h.saturating_sub(1);

        // Calculate Y Position relative to Area Top
        // Y = area.y + parent_focus_offset_y - global_scroll
        let abs_y =
            area.y as i32 + parent_focus_offset_y as i32 - app.tree_state.cascade_scroll as i32;

        // Prepare Items with Spacers
        // We only insert spacer if this column HAS a child (is not last) AND has a focus.
        let mut items_vec = Vec::new();
        let spacer_idx_in_parent = if i < app.tree_state.active_columns.len() - 1 {
            col.focus_index
        } else {
            usize::MAX
        };

        // If stacked, we can't easily insert spacers inside sections?
        // User request implied simple folders. Stacked view + Cascade usually means only the last column is stacked?
        // If an intermediate column is stacked, which "item" is the parent of the next column?
        // The one clicked. `focus_index` is global to the column.
        // So we insert spacers after `focus_index` item.

        // Note: For stacked columns, `items` are raw. `render_sectioned_column` handles display.
        // Inserting spacers into `items` for `render_sectioned_column` breaks its logic (indices shift).
        // Since we are rewriting rendering, we can handle it.
        // BUT `render_sectioned_column` is complex.
        // Assumption: Stacked columns calculate their own layout. If we must insert spacers, we must do it visually.

        if !col.sections.is_empty() {
            // For stacked columns, pass the rect.
            // But wait, if stacked column is not the last one, it pushes content down?
            // Yes. The focused item expands.
            // We need to pass `spacer_height` and `spacer_index` to render logic?
            // Let's modify `render_sectioned_column` or handle it here?
            // Since `render_sectioned_column` calculates layout based on heights, passing a "gap" is hard without modifying it.
            // Let's assume standard column logic for now for simplicity, or just render normally.
            // If user uses Stacks + Cascade, the expansion visual might be tricky.
            // We'll stick to rendering it normally for now, respecting Y pos.

            let rect = Rect {
                x: current_x,
                y: abs_y.max(area.y as i32) as u16,
                width: render_width,
                height: if abs_y < area.y as i32 {
                    col_height.saturating_sub((area.y as i32 - abs_y) as usize) as u16
                } else {
                    col_height as u16
                }
                .min(area.height),
            };
            // Clip rect
            if rect.height > 0 {
                render_sectioned_column(f, rect, col, i == app.tree_state.focus_col_idx);
            }
        } else {
            // Standard Column
            // Construct display items
            for (idx, item) in col.items.iter().enumerate() {
                // Create normal item
                let is_selected = col.selections.contains_key(&idx);
                let is_focused_item = idx == col.focus_index;
                let mut style = Style::default().fg(item.color);

                if is_selected {
                    if let Some(sel_color) = col.selections.get(&idx) {
                        style = style.bg(*sel_color).fg(Color::Black);
                    } else {
                        style = style.bg(Color::Rgb(60, 60, 60));
                    }
                }
                if is_focused_item && i == app.tree_state.focus_col_idx {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if item.is_dir {
                    style = style.add_modifier(Modifier::BOLD);
                }

                let icon = if item.is_dir { "" } else { "" };
                let content = format!("{} {}", icon, item.name);
                items_vec.push(ListItem::new(content).style(style));

                // Insert spacers
                if idx == spacer_idx_in_parent && spacer_size > 0 {
                    for _ in 0..spacer_size {
                        items_vec.push(ListItem::new(""));
                    }
                }
            }

            // Render List
            let effective_y = abs_y;
            let mut start_idx = 0;
            let mut render_y = area.y;
            let mut render_h = area.height;

            if effective_y < area.y as i32 {
                // Top is cut off
                start_idx = (area.y as i32 - effective_y) as usize;
                render_y = area.y;
                // How much height remains?
                // Total visual items = items_vec.len()
                let remaining = items_vec.len().saturating_sub(start_idx);
                render_h = (remaining as u16).min(area.height);
            } else {
                render_y = effective_y as u16;
                // Limit height
                let space_below = (area.y + area.height).saturating_sub(render_y);
                render_h = (items_vec.len() as u16).min(space_below);
            }

            if render_h > 0 && start_idx < items_vec.len() {
                let visible_items = items_vec.drain(start_idx..).collect::<Vec<_>>();
                let rect = Rect::new(current_x, render_y, render_width, render_h);

                let border_style = if i == app.tree_state.focus_col_idx {
                    Style::default().fg(THEME.accent_primary)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                let block = Block::default()
                    .borders(Borders::LEFT)
                    .border_style(border_style);

                f.render_widget(List::new(visible_items).block(block), rect);
            }
        }

        // Update offsets for next iteration
        current_x += width;
        // The next column starts aligned with the FOCUSED item of this column.
        // The focused item's Y position relative to THIS column's top is `focus_index`.
        // Wait, if spacers were inserted BEFORE? No, simple logic:
        // Items 0..focus take `focus` lines.
        // Focused item is at index `focus`.
        // So next column Y relative to THIS column Y is `focus`.
        // But what if `focus_index` accounts for sections/stacks?
        // For flat list, yes.
        parent_focus_offset_y += col.focus_index;

        // Correction: If this column was stacked, the `focus_index` determines Y too properly (since logic handles it?)
        // Yes, if `focus_index` is row index.
        // We accumulate offsets.
    }
}

fn ensure_focus_visible(app: &mut App, available_width: u16, widths: &[u16]) {
    let focus = app.tree_state.focus_col_idx;
    let mut start = app.tree_state.scroll_offset_col;

    // Ensure 1: Focus >= Start
    if focus < start {
        start = focus;
    }

    // Ensure 2: Focus is fully visible from Start?
    // Calculate total width from Start to Focus
    // If > available_width, increment Start until it fits (or Start == Focus)

    loop {
        let mut total_w = 0;
        let mut focus_visible = false;

        for i in start..widths.len() {
            total_w += widths[i];
            if i == focus {
                if total_w <= available_width {
                    focus_visible = true;
                }
                break;
            }
        }

        // If we reached focus and it fits, good.
        // Or if Start == Focus, we can't scroll further right (one column always shows, truncated if needed)
        if focus_visible || start == focus {
            break;
        }

        // Increment start (scroll right)
        start += 1;
        if start >= app.tree_state.active_columns.len() {
            start = app.tree_state.active_columns.len().saturating_sub(1);
            break;
        }
    }

    app.tree_state.scroll_offset_col = start;
}

fn render_column(f: &mut Frame, area: Rect, col: &TreeColumn, is_focused: bool) {
    // If column has sections, render as separate stacked boxes
    if !col.sections.is_empty() {
        render_sectioned_column(f, area, col, is_focused);
        return;
    }

    // Normal single-section column
    let items: Vec<ListItem> = col
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let is_selected = col.selections.contains_key(&i);
            let is_focused_item = i == col.focus_index;
            let mut style = Style::default().fg(item.color);

            if is_selected {
                if let Some(sel_color) = col.selections.get(&i) {
                    style = style.bg(*sel_color).fg(Color::Black);
                } else {
                    style = style.bg(Color::Rgb(60, 60, 60));
                }
            }
            if is_focused_item && is_focused {
                style = style.add_modifier(Modifier::UNDERLINED);
            }
            if item.is_dir {
                style = style.add_modifier(Modifier::BOLD);
            }

            let icon = if item.is_dir { "" } else { "" };
            let content = format!("{} {}", icon, item.name);
            ListItem::new(content).style(style)
        })
        .collect();

    let border_style = if is_focused {
        Style::default().fg(THEME.accent_primary)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);

    let highlight_style = if is_focused {
        Style::default()
            .bg(THEME.accent_primary)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(Color::Rgb(60, 60, 60)).fg(Color::White)
    };

    let mut state = ListState::default();
    state.select(Some(col.focus_index));

    f.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(highlight_style),
        area,
        &mut state,
    );
}

/// Render a column divided into multiple colored section boxes
fn render_sectioned_column(f: &mut Frame, area: Rect, col: &TreeColumn, is_focused: bool) {
    use ratatui::layout::{Constraint, Direction, Layout};

    // Calculate heights for each section (proportional to item count, but also leave room for headers)
    let section_heights = col.calculate_section_heights(area.height);

    let constraints: Vec<Constraint> = section_heights
        .iter()
        .map(|&h| Constraint::Length(h + 2))
        .collect();

    let section_rects = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    for (sec_idx, section) in col.sections.iter().enumerate() {
        if sec_idx >= section_rects.len() {
            break;
        }
        let sec_area = section_rects[sec_idx];

        // Build items for this section
        let items: Vec<ListItem> = col.items[section.start_index..section.end_index]
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let global_idx = section.start_index + i;
                let is_focused_item = global_idx == col.focus_index;
                let mut style = Style::default().fg(item.color);

                if is_focused_item && is_focused {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                if item.is_dir {
                    style = style.add_modifier(Modifier::BOLD);
                }

                let icon = if item.is_dir { "" } else { "" };
                let content = format!("{} {}", icon, item.name);
                ListItem::new(content).style(style)
            })
            .collect();

        // Block with section color and title
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(section.color))
            .title(Span::styled(
                format!(" {} ", section.title),
                Style::default()
                    .fg(section.color)
                    .add_modifier(Modifier::BOLD),
            ));

        // Highlight style for focus within section
        let highlight_style = Style::default()
            .bg(section.color)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD);

        // State: select the item within the section if focus is in this section
        let mut state = ListState::default();
        if col.focus_index >= section.start_index && col.focus_index < section.end_index {
            state.select(Some(col.focus_index - section.start_index));
        }

        f.render_stateful_widget(
            List::new(items)
                .block(block)
                .highlight_style(highlight_style),
            sec_area,
            &mut state,
        );
    }
}
