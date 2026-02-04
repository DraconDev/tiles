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

    // 1. Calculate ideal widths for ALL columns (or at least relevant ones around focus)
    //    We need to know widths to determining scrolling.
    let col_widths: Vec<u16> = app
        .tree_state
        .active_columns
        .iter()
        .map(|col| measure_column_width(col))
        .collect();

    // 2. Adjust scrolling to ensure `focus_col_idx` is visible
    ensure_focus_visible(app, area.width, &col_widths);

    // 3. Determine visible columns
    let mut visible_cols = Vec::new();
    let mut current_x = area.x;
    let mut x_offset = 0;

    // Skip columns before scroll_offset
    for i in 0..app.tree_state.scroll_offset_col {
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

fn measure_column_width(col: &TreeColumn) -> u16 {
    let max_len = col
        .items
        .iter()
        .map(|it| it.name.chars().count())
        .max()
        .unwrap_or(0);

    // Padding: Border (2) + Icon (2) + Text + Padding (2)
    let width = (max_len + 6) as u16;

    // Clamp
    width.clamp(15, 50)
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
    let items: Vec<ListItem> = col
        .items
        .iter()
        .enumerate()
        .map(|(_, item)| {
            let is_selected = i == col.selected;
            let mut style = Style::default().fg(item.color);

            if is_selected && !is_focused {
                style = style.bg(Color::Rgb(40, 40, 40)); // Dim selection for inactive col
            }
            if item.is_dir {
                style = style.add_modifier(Modifier::BOLD);
            }

            let icon = if item.is_dir { "" } else { "" };
            let content = format!("{} {}", icon, item.name);

            ListItem::new(Span::styled(content, style))
        })
        .collect();

    let border_style = if is_focused {
        Style::default().fg(THEME.accent_primary)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL) // Or Borders::RIGHT for cleaner "stacked" look?
        // Borders::ALL gives clear separation cell-like.
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
    state.select(Some(col.selected));

    f.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(highlight_style),
        area,
        &mut state,
    );
}
