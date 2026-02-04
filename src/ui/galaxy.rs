use crate::app::App;
use crate::state::GalaxyNode;
use ratatui::{layout::Rect, style::Style, Frame};

// Render Items
struct RenderPoint {
    x: f32,
    y: f32,
    char: char,
    color: ratatui::style::Color,
    label: Option<String>,
}

pub fn draw_galaxy_view(f: &mut Frame, area: Rect, app: &mut App) {
    if app.galaxy_state.root.is_none() {
        crate::events::galaxy::refresh_galaxy(app);
    }

    if let Some(root) = &app.galaxy_state.root {
        let center_x = area.x as f32 + (area.width as f32 / 2.0);
        let center_y = area.y as f32 + (area.height as f32 / 2.0);

        // Flatten the tree into layers (by depth)
        let mut layers: Vec<Vec<&GalaxyNode>> = Vec::new();
        collect_by_depth(root, 0, &mut layers);

        let mut points = Vec::new();

        // Draw Root (The Sun)
        points.push(RenderPoint {
            x: 0.0,
            y: 0.0,
            char: '◉',
            color: ratatui::style::Color::Yellow,
            label: Some(root.name.clone()),
        });

        // Draw Concentric Orbits
        for (depth, layer) in layers.iter().enumerate() {
            if depth == 0 {
                continue; // Skip root, already drawn
            }

            // Each depth level gets its own fixed orbit radius
            // Orbit 1 = 8 units, Orbit 2 = 16, Orbit 3 = 24, etc.
            let orbit_radius = (depth as f32) * 10.0 * app.galaxy_state.zoom;

            let count = layer.len();
            if count == 0 {
                continue;
            }

            // Distribute nodes evenly around this orbit
            let angle_step = 2.0 * std::f32::consts::PI / count as f32;

            for (i, node) in layer.iter().enumerate() {
                let angle = i as f32 * angle_step;
                let nx = orbit_radius * angle.cos();
                let ny = orbit_radius * angle.sin();

                // Node
                points.push(RenderPoint {
                    x: nx,
                    y: ny,
                    char: if node.is_dir { 'O' } else { '•' },
                    color: node.color,
                    label: Some(node.name.clone()),
                });
            }

            // Draw Orbit Ring (using dots)
            let ring_steps = (orbit_radius * 4.0) as usize;
            for s in 0..ring_steps {
                let t = s as f32 / ring_steps as f32;
                let ring_angle = t * 2.0 * std::f32::consts::PI;
                points.push(RenderPoint {
                    x: orbit_radius * ring_angle.cos(),
                    y: orbit_radius * ring_angle.sin(),
                    char: '·',
                    color: ratatui::style::Color::Rgb(40, 40, 50),
                    label: None,
                });
            }
        }

        // Render to Buffer
        let buf = f.buffer_mut();

        for p in points {
            let logical_x = p.x + app.galaxy_state.pan.0;
            let logical_y = p.y + app.galaxy_state.pan.1;

            // Aspect ratio correction (x * 2.0)
            let screen_x = center_x + (logical_x * 2.0);
            let screen_y = center_y + logical_y;

            if screen_x >= area.left() as f32
                && screen_x < area.right() as f32
                && screen_y >= area.top() as f32
                && screen_y < area.bottom() as f32
            {
                let sx = screen_x as u16;
                let sy = screen_y as u16;

                buf.get_mut(sx, sy).set_char(p.char).set_fg(p.color);

                if let Some(label) = p.label {
                    if app.galaxy_state.zoom > 0.4 {
                        let label_x = sx + 2;
                        if (label_x as usize + label.len()) < area.right() as usize {
                            buf.set_string(label_x, sy, label, Style::default().fg(p.color));
                        }
                    }
                }
            }
        }
    }
}

/// Collect all nodes by their depth level into a flat layer list.
fn collect_by_depth<'a>(node: &'a GalaxyNode, depth: usize, layers: &mut Vec<Vec<&'a GalaxyNode>>) {
    // Ensure we have enough layers
    while layers.len() <= depth {
        layers.push(Vec::new());
    }
    layers[depth].push(node);

    for child in &node.children {
        collect_by_depth(child, depth + 1, layers);
    }
}
