use crate::app::{App, AppEvent, AppMode, CurrentView};
use dracon_terminal_engine::contracts::{InputEvent as Event, KeyCode, MouseButton, MouseEventKind};
use tokio::sync::mpsc;

pub fn handle_git_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let CurrentView::Git = app.current_view {
        if let Event::Key(key) = evt {
            match key.code {
                KeyCode::Enter if matches!(app.mode, AppMode::Normal) => {
                    let mut open_preview: Option<std::path::PathBuf> = None;
                    let mut open_commit_view = false;
                    if let Some(fs) = app.current_file_state() {
                        // Priority 1: Pending changes (Diff)
                        if let Some(idx) = fs.git_pending_state.selected() {
                            if let Some(change) = fs.git_pending.get(idx) {
                                open_preview = Some(std::path::PathBuf::from(format!(
                                    "git-diff://{}",
                                    change.path
                                )));
                            }
                        }
                        // Priority 2: History (Commit)
                        if open_preview.is_none() {
                            if let Some(idx) = fs.git_history_state.selected() {
                                if let Some(commit) = fs.git_history.get(idx) {
                                    let hash = commit.hash.clone();
                                    open_preview =
                                        Some(std::path::PathBuf::from(format!("git://{}", hash)));
                                    open_commit_view = true;
                                }
                            }
                        }
                    }
                    if let Some(path) = open_preview {
                        let _ = event_tx
                            .try_send(AppEvent::PreviewRequested(app.focused_pane_index, path));
                        if open_commit_view {
                            app.current_view = CurrentView::Commit;
                            app.mode = AppMode::Viewer;
                            app.sidebar_focus = false;
                        }
                        return true;
                    }
                }
                _ => {}
            }
        }
    }
    false
}

pub fn handle_git_mouse(
    me: &dracon_terminal_engine::contracts::MouseEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    let row = me.row;
    if let MouseEventKind::Down(MouseButton::Left) = me.kind {
        if let Some(fs) = app.current_file_state() {
            let pending = &fs.git_pending;
            let remotes = &fs.git_remotes;
            let stashes = &fs.git_stashes;
            let inner_h = app.terminal_size.1.saturating_sub(2);
            let top_h = if pending.is_empty() && remotes.is_empty() && stashes.is_empty() {
                0
            } else {
                let p_len = if pending.is_empty() {
                    0
                } else {
                    pending.len() as u16 + 2
                };
                let i_len = if remotes.is_empty() && stashes.is_empty() {
                    0
                } else {
                    6
                };
                p_len.max(i_len).min(inner_h / 3)
            };

            let inner_y = 1; // Top border
            let active_data_start_y = inner_y + 1;

            // 1. Check if click is in ACTIVE section
            if !pending.is_empty()
                && row >= active_data_start_y
                && row < active_data_start_y + top_h.saturating_sub(1)
            {
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
            let history_area_y = inner_y + top_h;
            let table_data_start_y = history_area_y + 3;

            if row >= table_data_start_y {
                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                    if let Some(tab) = pane.tabs.get_mut(pane.active_tab_index) {
                        let scroll_offset = tab.git_history_state.offset();
                        let rel_row = (row - table_data_start_y) as usize + scroll_offset;
                        if rel_row < tab.git_history.len() {
                            tab.git_history_state.select(Some(rel_row));
                            tab.git_pending_state.select(None);
                            if let Some(commit) = tab.git_history.get(rel_row) {
                                let _ = event_tx.try_send(AppEvent::PreviewRequested(
                                    app.focused_pane_index,
                                    std::path::PathBuf::from(format!("git://{}", commit.hash)),
                                ));
                                app.current_view = CurrentView::Commit;
                                app.mode = AppMode::Viewer;
                                app.sidebar_focus = false;
                            }
                            return true;
                        }
                    }
                }
            }
        }
    }
    true // Trap clicks
}
