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
                        // Priority 1: Pending changes (Diff)
                        if let Some(idx) = fs.git_pending_state.selected() {
                            if let Some(change) = fs.git_pending.get(idx) {
                                let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                    app.focused_pane_index,
                                    std::path::PathBuf::from(format!("git-diff://{}", change.path)),
                                ));
                                return true;
                            }
                        }
                        // Priority 2: History (Commit)
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
            let inner_h = app.terminal_size.1.saturating_sub(2);
            let pending_h = if pending.is_empty() { 
                0 
            } else { 
                (pending.len() as u16 + 2).min(inner_h / 2) 
            };
            
            let inner_y = 1; // Top border
            let active_data_start_y = inner_y + 1;
            
            // 1. Check if click is in ACTIVE section
            if !pending.is_empty() && row >= active_data_start_y && row < active_data_start_y + pending_h.saturating_sub(1) {
                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                    if let Some(tab) = pane.tabs.get_mut(pane.active_tab_index) {
                        let rel_row = (row - active_data_start_y) as usize;
                        if rel_row < tab.git_pending.len() {
                            tab.git_pending_state.select(Some(rel_row));
                            tab.git_history_state.select(None);
                            return true;
                        }
                    }
                }
            }

            // 2. Check if click is in HISTORY section
            let history_area_y = inner_y + pending_h + 1;
            let table_data_start_y = history_area_y + 3;

            if row >= table_data_start_y {
                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                    if let Some(tab) = pane.tabs.get_mut(pane.active_tab_index) {
                        let scroll_offset = tab.git_history_state.offset();
                        let rel_row = (row - table_data_start_y) as usize + scroll_offset;
                        if rel_row < tab.git_history.len() {
                            tab.git_history_state.select(Some(rel_row));
                            tab.git_pending_state.select(None);
                            return true;
                        }
                    }
                }
            }
        }
    }
    true // Trap clicks
}
