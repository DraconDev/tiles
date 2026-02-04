use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};
use crate::app::App;
use crate::ui::theme::THEME;

pub fn draw_tree_view(f: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Tree View ")
        .style(Style::default().fg(THEME.border_inactive).bg(Color::Rgb(0, 0, 0))); // Full black bg

    let inner_area = block.inner(area);
    f.render_widget(block, area);

    // Columns: Name, Permission, Modified (Maybe just Name for now to keep it simple, or mimicking Files)
    // User requested "columns".
    let header_cells = ["Name", "Size", "Date"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(THEME.accent_secondary)));
    let header = Row::new(header_cells)
        .style(Style::default().bg(Color::Rgb(20, 20, 20)))
        .height(1);

    let rows = app.tree_state.flat_items.iter().enumerate().map(|(i, item)| {
        let is_selected = i == app.tree_state.selected;
        
        // Name Indentation
        let indent = "  ".repeat(item.depth);
        let prefix = if item.is_dir {
            if item.expanded { "▼ " } else { "▶ " }
        } else {
            "  " // File alignment
        };
        let icon = if item.is_dir { " " } else { " " }; // Basic icons for now
        
        let name_span = format!("{}{}{}{}", indent, prefix, icon, item.name);
        
        let mut style = Style::default().fg(item.color);
        if is_selected {
            style = style.bg(THEME.accent_primary).fg(Color::Black).add_modifier(Modifier::BOLD);
        }

        Row::new(vec![
            Cell::from(name_span),
            Cell::from(""), // Placeholder Size
            Cell::from(""), // Placeholder Date
        ])
        .style(style)
    });

    let widths = [
        ratatui::layout::Constraint::Percentage(60),
        ratatui::layout::Constraint::Length(10),
        ratatui::layout::Constraint::Length(20),
    ];

    let mut state = TableState::default();
    state.select(Some(app.tree_state.selected));
    // Apply scrolling offset manually if needed, but TableState handles select scrolling if we pass it correctly?
    // Ratatui TableState tracks offset.
    // However, app.tree_state has `offset`. We might need to sync them or just rely on TableState?
    // If we rely on TableState, we need to persist it in app.tree_state.table_state?
    // app.tree_state.offset is used in navigation logic. 
    // Let's use `*state.offset_mut() = app.tree_state.offset;` if accessible, or just rely on Table widget with manually sliced iter?
    // Standard approach: TableState manages offset if we don't slice.
    
    // We already calculated offset in `move_selection`.
    // Let's manually slice the rows? No, let Table handle it if we pass full rows.
    // But we need to sync offset back to app state if Table changes it?
    // Actually, `handle_tree_events` sets `offset`. Let's just use `offset` to slice rows?
    // Or simpler: Just render `Table` with `state` having `selected`.
    // We didn't add `TableState` to `TreeState`. We added `offset`.
    // I will add `offset` handling to the Table widget rendering by skipping items?
    // Ratatui Table calculates visible area based on `state.selected`.
    
    f.render_stateful_widget(
        Table::new(rows, widths)
            .header(header)
            .block(Block::default())
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        inner_area,
        &mut state
    );
}
