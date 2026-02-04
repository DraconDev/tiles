use crate::app::App;
use crate::state::GalaxyNode;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::Span,
    widgets::{Block, Borders, Widget},
    Frame,
};
use std::f64::consts::PI;

pub fn draw_galaxy_view(f: &mut Frame, area: Rect, app: &mut App) {
    if app.galaxy_state.root.is_none() {
        crate::events::galaxy::refresh_galaxy(app);
    }

    if let Some(root) = &app.galaxy_state.root {
        // Use Canvas for vector graphics feel? Or char grid?
        // Char grid gives us text easily.
        // Let's iterate and draw manual points.

        let center_x = area.width as f32 / 2.0 + app.galaxy_state.pan.0;
        let center_y = area.height as f32 / 2.0 + app.galaxy_state.pan.1;

        // Recursively draw
        draw_node_recursive(f, root, center_x, center_y, 0.0, 0, app.galaxy_state.zoom);
    }
}

fn draw_node_recursive(
    f: &mut Frame,
    node: &GalaxyNode,
    cx: f32,
    cy: f32,
    parent_angle: f32,
    depth: usize,
    zoom: f32,
) {
    // Determine screen coordinates
    // Base radius of orbit
    let base_radius = if depth == 0 {
        0.0
    } else {
        12.0 * zoom * (0.8f32).powi(depth as i32)
    };
    // Wait, radius is distance FROM PARENT.
    // So (cx, cy) is PARENT's position. We need to find THIS node's position.

    // Actually, we should propagate Absolute Position, but layout is relative.
    // Let's compute position:
    // This function receives the node's computed center (passed by parent).

    let x = cx as u16;
    let y = cy as u16;

    let (w, h) = f.area().as_ref().into(); // Correction: Frame area? No, need dimension.
                                           // Basic bounds check
                                           // if x < area.right... etc.

    // Draw Node
    let symbol = if node.is_dir { "O" } else { "•" };
    let color = node.color;
    // Draw label if zoomed enough or nearby?
    let label = &node.name;

    // Simple drawing: directly set buffer cell?
    // Ratatui doesn't expose buffer easily in draw function without a widget.
    // We can use a Widget impl or iterate.
    // Or render many small Paragraphs? (Expensive).
    // Canvas is best for dots/lines.

    // Let's use Canvas widget approach in the main draw function instead of recursion here?
    // No, Canvas doesn't draw Text well.
    // Let's use a custom Buffer-writing widget or just many small Widgets.
    // Optimization: Collect all "DrawCalls" into a list, then render.
}

// Better Approach: Calculate all absolute positions first, then render using one loop.
// Layout Engine separate from Render.

pub fn calculate_galaxy_layout(
    root: &GalaxyNode,
    zoom: f32,
    pan: (f32, f32),
    screen_center: (f32, f32),
) -> Vec<RenderItem> {
    let mut items = Vec::new();
    // Root at center
    // Let's just traverse and push items with calculated screen X/Y

    // Queue: (Node, parent_x, parent_y, start_angle, available_sector)
    // This is complex.
    // Simpler: Just Orbit 1 for now?

    items
}

pub struct RenderItem {
    pub x: u16,
    pub y: u16,
    pub symbol: String,
    pub color: Color,
    pub label: String,
}
