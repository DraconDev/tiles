use terma::input::event::{Event, KeyCode, MouseEventKind, MouseButton};
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

pub fn handle_git_mouse(me: &terma::input::event::MouseEvent, app: &mut App, _event_tx: &mpsc::Sender<AppEvent>) -> bool {
    let row = me.row;
    if let MouseEventKind::Down(MouseButton::Left) = me.kind {
        if row >= 2 {
            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                if let Some(tab) = pane.tabs.get_mut(pane.active_tab_index) {
                    let scroll_offset = tab.git_history_state.offset();
                    let rel_row = (row - 2) as usize + scroll_offset;
                    if rel_row < tab.git_history.len() {
                        tab.git_history_state.select(Some(rel_row));
                        return true;
                    }
                }
            }
        }
    }
    true // Trap clicks
}