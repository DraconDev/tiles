use terma::input::event::{Event, KeyCode, KeyModifiers};
use tokio::sync::mpsc;
use crate::app::{App, AppEvent, AppMode, CurrentView, MonitorSubview};

pub fn handle_monitor_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let CurrentView::Processes = app.current_view {
        if let Event::Key(key) = evt {
            if key.code == KeyCode::Esc && matches!(app.mode, AppMode::Normal) {
                app.current_view = CurrentView::Files;
                return true;
            }
            
            // Subview switching
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('1') => { app.monitor_subview = MonitorSubview::Overview; return true; }
                    KeyCode::Char('2') => { app.monitor_subview = MonitorSubview::Applications; return true; }
                    KeyCode::Char('3') => { app.monitor_subview = MonitorSubview::Processes; return true; }
                    _ => {}
                }
            }
        }
    }
    false
}
