use crate::app::{App, CurrentView, Pane, RemoteBookmark};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;
use parking_lot::Mutex;
use std::time::{Duration, Instant};

static LAST_SAVE: LazyLock<Mutex<Option<(Instant, String)>>> = LazyLock::new(|| Mutex::new(None));
const SAVE_DEBOUNCE_MS: u64 = 350;

#[derive(Serialize, Deserialize, Clone)]
pub struct ExternalTool {
    pub name: String,
    pub command: String,
}

#[derive(Serialize, Deserialize)]
pub struct PersistentState {
    pub panes: Vec<Pane>,
    pub focused_pane_index: usize,
    pub starred: Vec<PathBuf>,
    pub remote_bookmarks: Vec<RemoteBookmark>,
    pub current_view: CurrentView,
    pub window_size: Option<(u16, u16)>,
    pub path_colors: HashMap<PathBuf, u8>,
    #[serde(default)]
    pub external_tools: HashMap<String, Vec<ExternalTool>>, // ext -> tools
    #[serde(default)]
    pub icon_mode: Option<crate::icons::IconMode>,
    #[serde(default)]
    pub is_split_mode: bool,
    #[serde(default = "default_true")]
    pub semantic_coloring: bool,
    #[serde(default = "default_true")]
    pub show_sidebar: bool,
    #[serde(default)]
    pub show_side_panel: bool,
    #[serde(default = "default_true")]
    pub default_show_hidden: bool,
    #[serde(default = "default_preview_max_mb")]
    pub preview_max_mb: u16,
    #[serde(default)]
    pub theme_style: Option<crate::ui::theme::ThemeStyle>,
    #[serde(default)]
    pub sidebar_scope: crate::state::SidebarScope,
    #[serde(default)]
    pub recent_folders: Vec<PathBuf>,
}

fn default_true() -> bool {
    true
}

fn default_preview_max_mb() -> u16 {
    20
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
                    tab_clone.files.clear();
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
        window_size: if app.terminal_size.0 > 0 && app.terminal_size.1 > 0 {
            Some(app.terminal_size)
        } else {
            None
        },
        path_colors: app.path_colors.clone(),
        external_tools: app.external_tools.clone(),
        icon_mode: Some(app.icon_mode),
        is_split_mode: app.is_split_mode,
        semantic_coloring: app.semantic_coloring,
        show_sidebar: app.show_sidebar,
        show_side_panel: app.show_side_panel,
        default_show_hidden: app.default_show_hidden,
        preview_max_mb: app.preview_max_mb,
        theme_style: Some(crate::ui::theme::style_settings()),
        sidebar_scope: app.sidebar_scope.clone(),
        recent_folders: app.recent_folders.clone(),
    };

    let config_dir = dirs::config_dir()
        .ok_or("Could not find config dir")?
        .join("tiles");
    fs::create_dir_all(&config_dir)?;
    let state_path = config_dir.join("state.json");
    let json = serde_json::to_string_pretty(&state)?;

    {
        let mut last = LAST_SAVE.lock();
        let now = Instant::now();
        if let Some((last_at, last_json)) = last.as_ref() {
            // Avoid repeated writes of identical content.
            if *last_json == json {
                return Ok(());
            }
            // Debounce bursts while app is active/autosaving.
            if app.running
                && app.auto_save
                && now.duration_since(*last_at) < Duration::from_millis(SAVE_DEBOUNCE_MS)
            {
                return Ok(());
            }
        }
        *last = Some((now, json.clone()));
    }

    fs::write(state_path, json)?;
    Ok(())
}

pub fn save_state_quiet(app: &App) {
    if let Err(e) = save_state(app) {
        crate::app::log_debug(&format!("save_state failed: {}", e));
    }
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
