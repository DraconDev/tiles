use crate::app::App;
use crate::ui::theme::THEME;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, List, ListItem, ListState},
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

    let col_count = app.tree_state.active_columns.len();

    // Equal width columns for now
    let constraints = vec![Constraint::Ratio(1, col_count as u32); col_count];

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, col) in app.tree_state.active_columns.iter().enumerate() {
        // Prepare items
        let items: Vec<ListItem> = col
            .items
            .iter()
            .map(|item| {
                let mut style = Style::default().fg(item.color);
                if item.is_dir {
                    // Directories maybe distinct?
                    style = style.add_modifier(Modifier::BOLD);
                }
                // Selection happens via ListState, but we need to style manually or rely on highlight_style
                ListItem::new(Span::raw(format!(
                    "{} {}",
                    if item.is_dir { "" } else { "" },
                    item.name
                )))
                .style(style)
            })
            .collect();

        // Block styling
        let is_focused_col = i == app.tree_state.focus_col_idx;
        let border_style = if is_focused_col {
            Style::default().fg(Color::Yellow) // Highlight active column
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .borders(Borders::ALL) // Excel-like cells
            .border_style(border_style);
        // Title? Maybe path name?
        // .title(col.path.file_name()...)

        // We need a ListState to render selection
        let mut state = ListState::default();
        state.select(Some(col.selected));
        // Handle offset manually? List handles it if state is persistent usually,
        // but here we regenerate state.
        // Actually List handles scrolling if we pass offset?
        // state.select works. List widget handles view.
        // But we need to sync state.offset back if we want persistent scrolling?
        // App's TreeState stores offset. `state` should use it?
        // Ratatui ListState has `offset` field but it's private or read-only in some versions?
        // Actually `state.select()` just highlights.
        // To force scroll, we rely on `state`.
        // Wait, if we recreate `ListState` every frame with select(Some(x)), it might jump?
        // No, `render_stateful_widget` updates the state mutable ref.
        // We aren't passing `&mut app.tree_state` into a widget state directly because `TreeColumn` is custom.
        // We can construct a transient ListState and set its offset if possible?
        // Or better: `state.select(Some(col.selected))` usually ensures visibility (scroll to show).

        // Highlight style
        let highlight_style = Style::default()
            .bg(Color::Blue)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);

        f.render_stateful_widget(
            List::new(items)
                .block(block)
                .highlight_style(highlight_style),
            chunks[i],
            &mut state,
        );

        // Sync offset back?
        // `state.offset()` exists in recent ratatui.
        // app.tree_state.active_columns[i].offset = state.offset();
        // Since we iterate immutably `iter()`, we can't mutate app.
        // We need `iter_mut()` or index loop.
    }
}
