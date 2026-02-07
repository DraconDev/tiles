use crate::app::{App, AppEvent, AppMode, CurrentView};
use terma::input::event::{Event, KeyCode, MouseButton, MouseEventKind};
use tokio::sync::mpsc;

pub fn handle_git_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let CurrentView::Git = app.current_view {
        if let Event::Key(key) = evt {
            match key.code {
                KeyCode::Esc if matches!(app.mode, AppMode::Normal) => {
                    app.current_view = CurrentView::Files;
                    return true;
                }
                KeyCode::Enter if matches!(app.mode, AppMode::Normal) => {
                    if let Some(fs) = app.current_file_state() {
                        if let Some(idx) = fs.git_history_state.selected() {
                            if let Some(commit) = fs.git_history.get(idx) {
                                let hash = commit.hash.clone();
                                let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                    app.focused_pane_index,
                                    std::path::PathBuf::from(format!("git://{}", hash)),
                                ));
                                return true;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    false
}

pub fn handle_git_mouse(
    me: &terma::input::event::MouseEvent,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    let row = me.row;
    if let MouseEventKind::Down(MouseButton::Left) = me.kind {
        if let Some(fs) = app.current_file_state() {
            let pending = &fs.git_pending;
            
            // Replicate draw_git_page layout logic
            let inner_y = 1; // Top border
            let header_h = 1;
            let inner_h = app.terminal_size.1.saturating_sub(2);
            let pending_h = if pending.is_empty() { 
                0 
            } else { 
                (pending.len() as u16 + 2).min(inner_h / 3) 
            };
            
            // Table starts at inner_y + header_h + pending_h
            // Data rows start after table header (1 row)
            let table_data_start_y = inner_y + header_h + pending_h + 1;

            if row >= table_data_start_y {
                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                    if let Some(tab) = pane.tabs.get_mut(pane.active_tab_index) {
                        let scroll_offset = tab.git_history_state.offset();
                        let rel_row = (row - table_data_start_y) as usize + scroll_offset;
                        if rel_row < tab.git_history.len() {
                            tab.git_history_state.select(Some(rel_row));
                            return true;
                        }
                    }
                }
            }
        }
    }
    true // Trap clicks
}
