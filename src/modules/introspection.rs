use crate::app::{App, AppMode, CurrentView, FileState};
use serde::{Serialize, Serializer};

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
            active_tab: app.tab_index,
            input_buffer: app.input.clone(),
            tabs: app.file_tabs.iter().map(|t| TabState::from(t)).collect(),
        }
    }
}

impl From<&FileState> for TabState {
    fn from(fs: &FileState) -> Self {
        Self {
            path: fs.current_path.to_string_lossy().to_string(),
            selected_index: fs.selected_index,
            item_count: fs.files.len(),
            is_remote: fs.remote_session.is_some(),
        }
    }
}
