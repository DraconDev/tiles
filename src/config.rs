use crate::app::{App, Pane, RemoteBookmark, CurrentView};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;
use std::fs;

#[derive(Serialize, Deserialize)]
pub struct PersistentState {
    pub panes: Vec<Pane>,
    pub focused_pane_index: usize,
    pub starred: Vec<PathBuf>,
    pub remote_bookmarks: Vec<RemoteBookmark>,
    pub current_view: CurrentView,
    pub window_size: Option<(u16, u16)>,
    pub path_colors: HashMap<PathBuf, u8>,
}

pub fn save_state(app: &App) -> Result<(), Box<dyn std::error::Error>> {
    let state = PersistentState {
        panes: {
            // We need to clone the panes but some fields are skipped by serde anyway
            // but we need to make sure we don't save ephemeral data if we can avoid it.
            // Actually Pane and FileState already have #[serde(skip)] on ephemeral fields.
            let mut panes = Vec::new();
            for p in &app.panes {
                let mut tabs = Vec::new();
                for t in &p.tabs {
                    let mut tab_clone = t.clone();
                    tab_clone.search_filter.clear();
                    tab_clone.local_count = 0;
                    tabs.push(tab_clone);
                }
                panes.push(Pane {
                    tabs,
                    active_tab_index: p.active_tab_index,
                    preview: None,
                });
            }
            panes
        },
        focused_pane_index: app.focused_pane_index,
        starred: app.starred.clone(),
        remote_bookmarks: app.remote_bookmarks.clone(),
        current_view: app.current_view.clone(),
        window_size: if app.terminal_size.0 > 0 && app.terminal_size.1 > 0 { Some(app.terminal_size) } else { None },
        path_colors: app.path_colors.clone(),
    };

    let config_dir = dirs::config_dir().ok_or("Could not find config dir")?.join("tiles");
    fs::create_dir_all(&config_dir)?;
    let state_path = config_dir.join("state.json");
    let json = serde_json::to_string_pretty(&state)?;
    fs::write(state_path, json)?;
    Ok(())
}

pub fn load_state() -> Option<PersistentState> {
    let config_dir = dirs::config_dir()?.join("tiles");
    let state_path = config_dir.join("state.json");
    if !state_path.exists() {
        return None;
    }
    let json = fs::read_to_string(state_path).ok()?;
    serde_json::from_str(&json).ok()
}