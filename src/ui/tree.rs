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

    // Flatten the tree for rendering
    let mut visible_items = Vec::new();
    for item in &app.tree_state.root_items {
        flatten_item_recursive(item, 0, &mut visible_items);
    }

    // Apply scrolling
    let scroll_offset = app.tree_state.scroll_offset;
    if scroll_offset >= visible_items.len() && !visible_items.is_empty() {
        app.tree_state.scroll_offset = visible_items.len() - 1;
    }

    let render_items: Vec<RenderItem> = visible_items
        .into_iter()
        .skip(app.tree_state.scroll_offset)
        .take(area.height as usize)
        .collect();

    // Draw directly to buffer to allow custom positioning
    for (i, r_item) in render_items.iter().enumerate() {
        let row_y = area.y + i as u16;
        if row_y >= area.y + area.height {
            break;
        }

        // Calculate X position
        // Simple approach: Fixed column width of 25?
        let col_width = 25;
        let row_x = area.x + (r_item.depth as u16 * col_width);

        let available_width = area.width.saturating_sub(r_item.depth as u16 * col_width);
        if available_width == 0 {
            continue;
        }

        let item_rect = Rect::new(row_x, row_y, available_width.min(col_width), 1);

        let is_selected = if let Some(sel) = &app.tree_state.selected_path {
            sel == &r_item.item.path
        } else {
            false
        };

        let mut style = Style::default().fg(r_item.item.color);
        if is_selected {
            style = style
                .bg(Color::Rgb(60, 60, 60))
                .add_modifier(Modifier::BOLD);
        }
        // Dim empty folders
        if r_item.item.is_dir && !r_item.item.has_children && !is_selected {
            style = style.fg(Color::Rgb(100, 100, 100));
        }

        let icon = if r_item.item.is_dir {
            if r_item.item.has_children {
                " "
            } else {
                " ∅"
            }
        } else {
            ""
        };

        let span = Span::styled(format!("{}{}", icon, r_item.item.name), style);

        // Render
        f.render_widget(Block::default().style(style), item_rect); // Background
        f.render_widget(span, item_rect);

        // Draw vertical connector from parent?
        // Cascade style implies boxes/columns.
        // Let's draw a border on the left?
        if is_selected {
            f.render_widget(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(THEME.accent_primary)),
                item_rect,
            );
        } else {
            f.render_widget(
                Block::default()
                    .borders(Borders::LEFT)
                    .border_style(Style::default().fg(Color::DarkGray)),
                item_rect,
            );
        }
    }
}

struct RenderItem<'a> {
    item: &'a TreeItem,
    depth: usize,
}

fn flatten_item_recursive<'a>(item: &'a TreeItem, depth: usize, result: &mut Vec<RenderItem<'a>>) {
    result.push(RenderItem { item, depth });
    if item.expanded {
        if let Some(children) = &item.children {
            for child in children {
                flatten_item_recursive(child, depth + 1, result);
            }
        }
    }
}
