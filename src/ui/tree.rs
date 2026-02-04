use crate::app::App;
use crate::state::TreeItem;
use crate::ui::theme::THEME;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Widget},
    Frame,
};

pub fn draw_tree_view(f: &mut Frame, area: Rect, app: &mut App) {
    if app.tree_state.root_items.is_empty() {
        crate::events::tree::refresh_tree(app);
    }

    if app.tree_state.root_items.is_empty() {
        return;
    }

    // Draw directly to buffer using calculated layout
    // First, generate the full layout to determine heights and positions
    let layout = calculate_layout(&app.tree_state.root_items);

    // Apply scrolling
    // If scroll_offset is too large, clamp it
    let total_rows = if let Some(last) = layout.last() {
        last.row + 1 // Approximation, or we can calculate max row
    } else {
        0
    };

    if app.tree_state.scroll_offset >= total_rows && total_rows > 0 {
        app.tree_state.scroll_offset = total_rows - 1;
    }
    let scroll_offset = app.tree_state.scroll_offset;

    // Filter visible items based on scroll area
    let area_height = area.height as usize;
    let visible_items: Vec<&LayoutItem> = layout
        .iter()
        .filter(|item| item.row >= scroll_offset && item.row < scroll_offset + area_height)
        .collect();

    for item in visible_items {
        // Map logical row to visual row
        let visual_row = (item.row - scroll_offset) as u16;
        let row_y = area.y + visual_row;

        let col_width = app.tree_state.column_width;
        let row_x = area.x + (item.col as u16 * col_width);

        let available_width = area.width.saturating_sub(item.col as u16 * col_width);
        if available_width == 0 {
            continue;
        }

        // Fix "Missing First Letter": Offset text rect by 1 to make room for border
        let text_rect = Rect::new(
            row_x + 1,
            row_y,
            available_width.saturating_sub(1).min(col_width),
            1,
        );
        let border_rect = Rect::new(row_x, row_y, 1, 1);
        let bg_rect = Rect::new(row_x, row_y, available_width.min(col_width), 1);

        let is_selected = if let Some(sel) = &app.tree_state.selected_path {
            sel == &item.item.path
        } else {
            false
        };

        let mut style = Style::default().fg(item.item.color);

        // Expanded Highlight (Dark Blue-Gray) for open folders
        if item.item.expanded {
            style = style.bg(Color::Rgb(30, 30, 45));
        }

        if is_selected {
            style = style
                .bg(Color::Rgb(60, 60, 60))
                .add_modifier(Modifier::BOLD);
        }
        // Dim empty folders
        if item.item.is_dir && !item.item.has_children && !is_selected {
            style = style.fg(Color::Rgb(100, 100, 100));
        }

        let icon = if item.item.is_dir {
            if item.item.has_children {
                " "
            } else {
                " ∅"
            }
        } else {
            ""
        };

        let span = Span::styled(format!("{}{}", icon, item.item.name), style);

        // Render Background
        f.render_widget(Block::default().style(style), bg_rect);

        // Render Text (Offset)
        f.render_widget(span, text_rect);

        // Draw border (Separate widget)
        if is_selected {
            f.render_widget(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(THEME.accent_primary)),
                border_rect, // Only draw border on first char
            );
        } else {
            f.render_widget(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(Color::DarkGray)),
                border_rect,
            );
        }

        // Draw Indentation Guides
        for d in 0..item.col {
            let guide_x = area.x + (d as u16 * col_width);
            f.render_widget(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(Color::Rgb(40, 40, 40))), // Very dim guide
                Rect::new(guide_x, row_y, 1, 1),
            );
        }
    }
}

pub struct LayoutItem<'a> {
    pub item: &'a TreeItem,
    pub col: usize,
    pub row: usize,
}

pub fn calculate_layout(roots: &[TreeItem]) -> Vec<LayoutItem<'_>> {
    let mut result = Vec::new();
    let mut current_row = 0;
    for root in roots {
        let height = layout_recursive(root, 0, current_row, &mut result);
        current_row += height;
    }
    result
}

fn layout_recursive<'a>(
    item: &'a TreeItem,
    col: usize,
    start_row: usize,
    result: &mut Vec<LayoutItem<'a>>,
) -> usize {
    // Parent is at (col, start_row)
    result.push(LayoutItem {
        item,
        col,
        row: start_row,
    });

    if !item.expanded {
        return 1;
    }

    if let Some(children) = &item.children {
        if children.is_empty() {
            return 1;
        }

        let mut child_y = start_row;
        // Optimization: Child 0 on same row?
        // Logic: All children drawn recursively.
        // We pass 'start_row' to the first child.
        // BUT, we passed (col + 1).

        for child in children {
            let height = layout_recursive(child, col + 1, child_y, result);
            child_y += height;
        }

        let children_height = child_y - start_row;
        return children_height.max(1);
    }

    1
}
