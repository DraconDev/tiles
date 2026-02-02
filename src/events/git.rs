use terma::input::event::{Event, KeyCode};
use tokio::sync::mpsc;
use crate::app::{App, AppEvent, AppMode, CurrentView};

pub fn handle_git_events(evt: &Event, app: &mut App, _event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let CurrentView::Git = app.current_view {
        if let Event::Key(key) = evt {
            if key.code == KeyCode::Esc && matches!(app.mode, AppMode::Normal) {
                app.current_view = CurrentView::Files;
                return true;
            }
        }
    }
    false
}
