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

    // Safety check again
    if app.tree_state.active_columns.is_empty() {
        return;
    }

    // 2. Calculate ideal widths for ALL columns (or at least relevant ones around focus)
    //    We need to know widths to determining scrolling.
    let col_widths: Vec<u16> = app
        .tree_state
        .active_columns
        .iter()
        .map(|col| col.width())
        .collect();

    // 2. Adjust scrolling to ensure `focus_col_idx` is visible
    ensure_focus_visible(app, area.width, &col_widths);

    // 3. Determine visible columns
    let mut visible_cols = Vec::new();
    let mut current_x = area.x;
    let mut x_offset = 0;

    // Skip columns before scroll_offset
    for _ in 0..app.tree_state.scroll_offset_col {
        // Just skip their widths logic?
        // Actually we render from scroll_offset
    }

    for i in app.tree_state.scroll_offset_col..app.tree_state.active_columns.len() {
        let width = col_widths[i];

        // If this column fits (mostly), render it
        // We limit to area width.
        if x_offset >= area.width {
            break;
        }

        // Last visible column might be truncated, that's fine for Miller Columns often.
        // But we try to show full.
        let render_width = std::cmp::min(width, area.width - x_offset);

        if render_width > 0 {
            visible_cols.push((i, Rect::new(current_x, area.y, render_width, area.height)));
            current_x += render_width;
            x_offset += render_width;
        }
    }

    // 4. Render visible columns
    for (i, rect) in visible_cols {
        let col = &app.tree_state.active_columns[i];
        render_column(f, rect, col, i == app.tree_state.focus_col_idx);
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
    let total_items: usize = col
        .sections
        .iter()
        .map(|s| s.end_index - s.start_index)
        .sum();
    let available_height = area.height.saturating_sub(col.sections.len() as u16 * 2); // 2 lines per section for border

    let section_heights: Vec<u16> = col
        .sections
        .iter()
        .map(|s| {
            let item_count = s.end_index - s.start_index;
            let proportion = item_count as f32 / total_items.max(1) as f32;
            let height = (proportion * available_height as f32).round() as u16;
            height.max(3) // Minimum height of 3 (border + 1 item + border)
        })
        .collect();

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
