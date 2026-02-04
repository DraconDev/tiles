use crate::app::App;
use crate::state::GalaxyNode;
use ratatui::{layout::Rect, style::Style, widgets::Widget, Frame};

// Render Items
struct RenderPoint {
    x: f32,
    y: f32,
    char: char,
    color: ratatui::style::Color,
    is_node: bool,
    label: Option<String>,
}

pub fn draw_galaxy_view(f: &mut Frame, area: Rect, app: &mut App) {
    if app.galaxy_state.root.is_none() {
        crate::events::galaxy::refresh_galaxy(app);
    }

    if let Some(root) = &app.galaxy_state.root {
        let center_x = area.x as f32 + (area.width as f32 / 2.0);
        let center_y = area.y as f32 + (area.height as f32 / 2.0);

        let mut points = Vec::new();

        // Root is at logical (0,0)
        collect_points_recursive(root, 0.0, 0.0, 0, app.galaxy_state.zoom, &mut points);

        // Render to Buffer
        let buf = f.buffer_mut();

        for p in points {
            // Apply Camera Transform
            // Logical position (p.x, p.y) -> Screen Position
            // Pan is Logical offset.

            let logical_x = p.x + app.galaxy_state.pan.0;
            let logical_y = p.y + app.galaxy_state.pan.1;

            // Project to Screen (Aspect Ratio: x * 2.0)
            let screen_x = center_x + (logical_x * 2.0);
            let screen_y = center_y + logical_y;

            // Bounds Check
            if screen_x >= area.left() as f32
                && screen_x < area.right() as f32
                && screen_y >= area.top() as f32
                && screen_y < area.bottom() as f32
            {
                let sx = screen_x as u16;
                let sy = screen_y as u16;

                // Draw Symbol
                buf.get_mut(sx, sy).set_char(p.char).set_fg(p.color);

                // Draw Label (if node and zoomed enough)
                if let Some(label) = p.label {
                    if app.galaxy_state.zoom > 0.5 {
                        let label_x = sx + 2;
                        if (label_x as f32 + label.len() as f32) < area.right() as f32 {
                            let cell = buf.get_mut(label_x, sy); // Check first char pos
                                                                 // Simply setting string:
                            buf.set_string(label_x, sy, label, Style::default().fg(p.color));
                        }
                    }
                }
            }
        }
    }
}

fn collect_points_recursive(
    node: &GalaxyNode,
    cx: f32, // Logical X
    cy: f32, // Logical Y
    depth: usize,
    zoom: f32,
    points: &mut Vec<RenderPoint>,
) {
    // Add Self
    points.push(RenderPoint {
        x: cx,
        y: cy,
        char: if node.is_dir { 'O' } else { '•' },
        color: node.color,
        is_node: true,
        label: Some(node.name.clone()),
    });

    // Children
    let count = node.children.len();
    if count == 0 {
        return;
    }

    // Layout Ring
    // Radius depends on depth. Inner = tighter.
    // Base radius starts large and shrinks? Or grows?
    // Orbit 1: Radius 15. Orbit 2: Radius 8 (relative).
    // Let's degrade radius by 0.7 per level.
    // Dynamic Radius Calculation
    // We need enough circumference to fit all children with spacing.
    // min_spacing = 3.0 (node + space).
    let min_spacing = 4.0;
    let circumference_needed = count as f32 * min_spacing;
    let radius_needed = circumference_needed / (2.0 * std::f32::consts::PI);

    // Base radius: Starts at 20.0 or calculated need.
    // Shrink with depth, but respect the need.
    let depth_factor = (0.7f32).powi(depth as i32);
    let radius = radius_needed.max(15.0) * zoom * depth_factor;

    // Distribute children evenly around circle
    let angle_step = 2.0 * std::f64::consts::PI as f32 / count as f32;

    for (i, child) in node.children.iter().enumerate() {
        let angle = i as f32 * angle_step + (depth as f32 * 0.5); // Offset per depth to avoid straight lines overlap
        let nx = cx + radius * angle.cos();
        let ny = cy + radius * angle.sin();

        // Draw Line (Spoke) - interpolation
        let steps = (radius * 1.0) as usize;
        for s in 1..steps {
            let t = s as f32 / steps as f32;
            let lx = cx + (nx - cx) * t;
            let ly = cy + (ny - cy) * t;
            points.push(RenderPoint {
                x: lx,
                y: ly,
                char: '·',
                color: ratatui::style::Color::DarkGray,
                is_node: false,
                label: None,
            });
        }

        collect_points_recursive(child, nx, ny, depth + 1, zoom, points);
    }
}
