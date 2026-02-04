use crate::app::App;
use crate::state::GalaxyNode;
use ratatui::{
    layout::Rect,
    style::{Color, Style},
    Frame,
};

// Render Items
struct RenderPoint {
    x: f32,
    y: f32,
    char: char,
    color: Color,
    label: Option<String>,
}

// A palette of distinct colors for top-level sectors
const SECTOR_COLORS: [Color; 8] = [
    Color::Rgb(255, 100, 100), // Red
    Color::Rgb(100, 255, 100), // Green
    Color::Rgb(100, 180, 255), // Blue
    Color::Rgb(255, 200, 100), // Orange
    Color::Rgb(200, 100, 255), // Purple
    Color::Rgb(100, 255, 200), // Cyan
    Color::Rgb(255, 100, 200), // Pink
    Color::Rgb(200, 200, 100), // Yellow-ish
];

pub fn draw_galaxy_view(f: &mut Frame, area: Rect, app: &mut App) {
    if app.galaxy_state.root.is_none() {
        crate::events::galaxy::refresh_galaxy(app);
    }

    if let Some(root) = &app.galaxy_state.root {
        let center_x = area.x as f32 + (area.width as f32 / 2.0);
        let center_y = area.y as f32 + (area.height as f32 / 2.0);

        // Flatten the tree into layers, tracking parent sector color
        let mut layers: Vec<Vec<(&GalaxyNode, Color)>> = Vec::new();

        // Assign colors to top-level (orbit 1) items
        for (i, child) in root.children.iter().enumerate() {
            let sector_color = SECTOR_COLORS[i % SECTOR_COLORS.len()];
            collect_with_color(child, 1, sector_color, &mut layers);
        }

        let mut points = Vec::new();

        // Draw Root (The Sun)
        points.push(RenderPoint {
            x: 0.0,
            y: 0.0,
            char: '◉',
            color: Color::Yellow,
            label: Some(root.name.clone()),
        });

        // Draw Concentric Orbits
        for (depth, layer) in layers.iter().enumerate() {
            let real_depth = depth + 1; // layers[0] = depth 1

            // Each depth level gets its own fixed orbit radius
            let orbit_radius = (real_depth as f32) * 10.0 * app.galaxy_state.zoom;

            let count = layer.len();
            if count == 0 {
                continue;
            }

            // Distribute nodes evenly around this orbit
            let angle_step = 2.0 * std::f32::consts::PI / count as f32;

            for (i, (node, color)) in layer.iter().enumerate() {
                let angle = i as f32 * angle_step;
                let nx = orbit_radius * angle.cos();
                let ny = orbit_radius * angle.sin();

                // Node
                points.push(RenderPoint {
                    x: nx,
                    y: ny,
                    char: if node.is_dir { 'O' } else { '•' },
                    color: *color,
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
                    color: Color::Rgb(40, 40, 50),
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

/// Collect nodes by depth, propagating the sector color from parent.
fn collect_with_color<'a>(
    node: &'a GalaxyNode,
    depth: usize,
    color: Color,
    layers: &mut Vec<Vec<(&'a GalaxyNode, Color)>>,
) {
    // Ensure we have enough layers (depth 1 = index 0)
    while layers.len() < depth {
        layers.push(Vec::new());
    }
    layers[depth - 1].push((node, color));

    // Children inherit the same sector color
    for child in &node.children {
        collect_with_color(child, depth + 1, color, layers);
    }
}
