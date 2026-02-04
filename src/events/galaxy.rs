use crate::app::{App, AppEvent};
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
    event_tx: &mpsc::Sender<AppEvent>,
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
            // We need to calculate layout positions and check distance to click
            // For now, let's just log click
            return true;
        }
        _ => {}
    }

    // Drag to Pan?
    // Requires drag state tracking in App or GalaxyState

    false
}
