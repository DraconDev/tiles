#![allow(dead_code)]
use crate::app::{App, FileState};
use serde::Serialize;

#[derive(Serialize)]
pub struct WorldState {
    pub mode: String,
    pub view: String,
    pub focus: String,
    pub active_tab: usize,
    pub tabs: Vec<TabState>,
    pub input_buffer: String,
}

#[derive(Serialize)]
pub struct TabState {
    pub path: String,
    pub selected_index: Option<usize>,
    pub item_count: usize,
    pub is_remote: bool,
}

impl WorldState {
    pub fn capture(app: &App) -> Self {
        WorldState {
            mode: format!("{:?}", app.mode),
            view: format!("{:?}", app.current_view),
            focus: if app.sidebar_focus {
                "Sidebar".to_string()
            } else {
                "Main".to_string()
            },
            active_tab: app.focused_pane_index,
            input_buffer: app.input.value.clone(),
            // Map the *active* state of each pane as the visible tabs
            tabs: app
                .panes
                .iter()
                .filter_map(|p| p.current_state().map(TabState::from))
                .collect(),
        }
    }
}

impl From<&FileState> for TabState {
    fn from(fs: &FileState) -> Self {
        Self {
            path: fs.current_path.to_string_lossy().to_string(),
            selected_index: fs.selection.selected,
            item_count: fs.files.len(),
            is_remote: fs.remote_session.is_some(),
        }
    }
}
