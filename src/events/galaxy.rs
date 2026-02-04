use crate::app::{App, AppEvent};
use crate::state::GalaxyNode;
use ratatui::style::Color;
use std::path::{Path, PathBuf};
use terma::input::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use tokio::sync::mpsc;

pub fn handle_galaxy_events(
    evt: &Event,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    if let Event::Key(key) = evt {
        if key.kind != KeyEventKind::Press {
            return false;
        }
        match key.code {
            KeyCode::Esc => {
                app.current_view = crate::app::CurrentView::Files;
                app.mode = crate::app::AppMode::Normal;
                return true;
            }
            // Pan with Arrow Keys or WASD
            KeyCode::Char('w') | KeyCode::Up => {
                app.galaxy_state.pan.1 += 5.0 / app.galaxy_state.zoom;
                return true;
            }
            KeyCode::Char('s') | KeyCode::Down => {
                app.galaxy_state.pan.1 -= 5.0 / app.galaxy_state.zoom;
                return true;
            }
            KeyCode::Char('a') | KeyCode::Left => {
                app.galaxy_state.pan.0 += 10.0 / app.galaxy_state.zoom;
                return true;
            }
            KeyCode::Char('d') | KeyCode::Right => {
                app.galaxy_state.pan.0 -= 10.0 / app.galaxy_state.zoom;
                return true;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                app.galaxy_state.zoom *= 1.1;
                return true;
            }
            KeyCode::Char('-') => {
                app.galaxy_state.zoom *= 0.9;
                return true;
            }
            _ => {}
        }
    }
    false
}

pub fn handle_galaxy_mouse(
    me: &MouseEvent,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    // Basic hit testing logic will be tricky without shared layout state.
    // For now, implement Zoom and Pan.

    match me.kind {
        MouseEventKind::ScrollDown => {
            app.galaxy_state.zoom *= 0.9;
            return true;
        }
        MouseEventKind::ScrollUp => {
            app.galaxy_state.zoom *= 1.1;
            return true;
        }
        MouseEventKind::Down(MouseButton::Left) => {
            // To be implemented: Hit testing for navigation
            return true;
        }
        _ => {}
    }
    false
}

pub fn refresh_galaxy(app: &mut App) {
    let path = app
        .current_file_state()
        .map(|fs| fs.current_path.clone())
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default());

    app.galaxy_state.current_path = path.clone();
    app.galaxy_state.root = Some(load_galaxy_recursive(&path, 0));
}

fn load_galaxy_recursive(path: &Path, depth: usize) -> GalaxyNode {
    let is_dir = path.is_dir();
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut children = Vec::new();

    // Depth Limit
    if is_dir && depth < 3 {
        if let Ok(entries) = std::fs::read_dir(path) {
            let mut entries: Vec<_> = entries
                .filter_map(|e| e.ok())
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .collect();

            entries.truncate(20);

            for e in entries {
                children.push(load_galaxy_recursive(&e.path(), depth + 1));
            }
        }
    }

    let color = if is_dir {
        Color::Blue
    } else {
        if let Some(ext) = path.extension() {
            match ext.to_string_lossy().to_lowercase().as_str() {
                "rs" => Color::Red,
                "toml" | "json" | "yaml" | "yml" => Color::Yellow,
                "md" | "txt" => Color::Green,
                "png" | "jpg" | "jpeg" => Color::Magenta,
                _ => Color::White,
            }
        } else {
            Color::White
        }
    };

    GalaxyNode {
        path: path.to_path_buf(),
        name,
        is_dir,
        color,
        x: 0.0,
        y: 0.0,
        size: 1.0,
        children,
    }
}
