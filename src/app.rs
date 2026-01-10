#![allow(dead_code, unused)]
use crate::license::check_license;
use crate::modules::files::update_files;

use ratatui::layout::Rect;
use ratatui::widgets::TableState;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use terma::compositor::engine::TilePlacement;
use terma::input::event::Event as TermaEvent;

#[derive(Clone, Debug)]
pub enum AppEvent {
    RefreshFiles(usize), // pane_index
    FilesUpdated(
        usize,
        Vec<PathBuf>,
        HashMap<PathBuf, FileMetadata>,
        HashMap<PathBuf, String>,
        Option<String>,
        usize, // local_count
    ), // tab_idx, files, metadata, git, branch, local_count
    Tick,
    Raw(TermaEvent),
    SystemUpdated(SystemData),
    CreateFile(PathBuf),
    CreateFolder(PathBuf),
    Rename(PathBuf, PathBuf),
    Delete(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CurrentView {
    Files,
    Processes,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ContextMenuTarget {
    File(usize),
    Folder(usize),
    EmptySpace,
    SidebarFavorite(PathBuf),
    SidebarRemote(usize),
    SidebarStorage(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsSection {
    Columns,
    Tabs,
    General,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppMode {
    Normal,
    Rename,
    Delete,
    NewFolder,
    NewFile,
    Search,
    Command,
    RemoteAdd,
    TabSearch,
    Properties,
    ContextMenu {
        x: u16,
        y: u16,
        target: ContextMenuTarget,
    },
    CommandPalette,
    Location,
    Settings,
    AddRemote,
}

#[derive(Clone, Debug)]
pub enum DropTarget {
    Favorites,
    SidebarArea,
    Folder(PathBuf),
}

#[derive(Clone, Debug)]
pub struct SidebarBounds {
    pub y: u16,
    pub index: usize,
    pub target: SidebarTarget,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SidebarTarget {
    Header(String),
    Favorite(PathBuf),
    Remote(usize),
    Storage(usize),
}

#[derive(Clone, Debug)]
pub struct CommandItem {
    pub key: String,
    pub desc: String,
    pub action: CommandAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandAction {
    Quit,
    ToggleZoom,
    SwitchView(CurrentView),
    AddRemote,
    ConnectToRemote(usize), // index into remote_bookmarks
    CommandPalette,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipboardOp {
    Copy,
    Cut,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum FileColumn {
    Name,
    Size,
    Modified,
    Permissions,
}

#[derive(Clone)]
pub struct RemoteSession {
    pub name: String,
    pub host: String,
    pub user: String,
    pub session: Arc<Mutex<ssh2::Session>>,
}

impl std::fmt::Debug for RemoteSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteSession")
            .field("name", &self.name)
            .field("host", &self.host)
            .field("user", &self.user)
            .field("session", &"<ssh2 session>")
            .finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteBookmark {
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub last_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct DiskInfo {
    pub name: String,
    pub used_space: f64,
    pub available_space: f64,
    pub total_space: f64,
}

#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileMetadata {
    pub size: u64,
    pub modified: std::time::SystemTime,
    pub created: std::time::SystemTime,
    pub permissions: u32,
    pub extension: String,
    pub is_dir: bool,
}

impl Default for FileMetadata {
    fn default() -> Self {
        Self {
            size: 0,
            modified: std::time::SystemTime::UNIX_EPOCH,
            created: std::time::SystemTime::UNIX_EPOCH,
            permissions: 0,
            extension: String::new(),
            is_dir: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileState {
    pub current_path: PathBuf,
    #[serde(skip)]
    pub remote_session: Option<RemoteSession>,
    pub selected_index: Option<usize>,
    pub selection_anchor: Option<usize>,
    pub multi_select: std::collections::HashSet<usize>,
    #[serde(skip)]
    pub table_state: TableState,
    #[serde(skip)]
    pub files: Vec<PathBuf>,
    #[serde(skip)]
    pub metadata: HashMap<PathBuf, FileMetadata>,
    #[serde(skip)]
    pub git_status: HashMap<PathBuf, String>,
    pub show_hidden: bool,
    #[serde(skip)]
    pub clipboard: Option<(PathBuf, ClipboardOp)>,
    pub search_filter: String,
    pub columns: Vec<FileColumn>,
    pub history: Vec<PathBuf>,
    pub history_index: usize,
    #[serde(skip)]
    pub view_height: usize,
    pub sort_column: FileColumn,
    pub sort_ascending: bool,
    #[serde(skip)]
    pub breadcrumb_bounds: Vec<(Rect, PathBuf)>,
    #[serde(skip)]
    pub column_bounds: Vec<(Rect, FileColumn)>,
    #[serde(skip)]
    pub hovered_breadcrumb: Option<PathBuf>,
    #[serde(skip)]
    pub git_branch: Option<String>,
    #[serde(skip)]
    pub local_count: usize,
}

impl FileState {
    pub fn new(
        path: PathBuf,
        remote: Option<RemoteSession>,
        show_hidden: bool,
        columns: Vec<FileColumn>,
        sort_column: FileColumn,
        sort_ascending: bool,
    ) -> Self {
        Self {
            current_path: path.clone(),
            remote_session: remote,
            selected_index: None,
            selection_anchor: None,
            multi_select: std::collections::HashSet::new(),
            table_state: TableState::default(),
            files: Vec::new(),
            metadata: HashMap::new(),
            git_status: HashMap::new(),
            show_hidden,
            clipboard: None,
            search_filter: String::new(),
            columns,
            history: vec![path],
            history_index: 0,
            view_height: 0,
            sort_column,
            sort_ascending,
            breadcrumb_bounds: Vec::new(),
            column_bounds: Vec::new(),
            hovered_breadcrumb: None,
            git_branch: None,
            local_count: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SystemData {
    pub cpu_usage: f32,
    pub mem_usage: f64,
    pub total_mem: f64,
    pub disks: Vec<DiskInfo>,
    pub processes: Vec<ProcessInfo>,
}

#[derive(Clone, Debug)]
pub struct SystemState {
    pub last_update: std::time::Instant,
    pub disks: Vec<DiskInfo>,
    pub processes: Vec<ProcessInfo>,
    pub cpu_usage: f32,
    pub mem_usage: f64,
    pub total_mem: f64,
}

#[derive(Debug)]
pub enum LicenseStatus {
    Valid,
    Invalid(String),
    TrialExpired,
    Commercial(String),
    FreeMode,
}

#[derive(Serialize, Deserialize)]
pub struct Pane {
    pub tabs: Vec<FileState>,
    pub active_tab_index: usize,
}

impl Pane {
    pub fn new(initial_state: FileState) -> Self {
        Self {
            tabs: vec![initial_state],
            active_tab_index: 0,
        }
    }

    pub fn current_state(&self) -> Option<&FileState> {
        self.tabs.get(self.active_tab_index)
    }

    pub fn current_state_mut(&mut self) -> Option<&mut FileState> {
        self.tabs.get_mut(self.active_tab_index)
    }

    pub fn open_tab(&mut self, state: FileState) {
        if self.tabs.len() >= 3 {
            self.tabs.remove(0);
        }
        self.tabs.push(state);
        self.active_tab_index = self.tabs.len() - 1;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsTarget {
    SingleMode,
    SplitMode,
}

pub struct App {
    pub running: bool,
    pub current_view: CurrentView,
    pub mode: AppMode,
    pub input: String,

    pub panes: Vec<Pane>,
    pub focused_pane_index: usize,

    pub terminal_size: (u16, u16),
    pub mouse_pos: (u16, u16),
    pub system_state: SystemState,
    pub license: LicenseStatus,
    pub sidebar_focus: bool,
    pub sidebar_index: usize,
    pub remote_bookmarks: Vec<RemoteBookmark>,
    pub active_sessions: HashMap<String, Arc<Mutex<ssh2::Session>>>,
    pub filtered_commands: Vec<CommandItem>,
    pub command_index: usize,
    pub last_click: Option<(std::time::Instant, u16, u16)>,
    pub tile_queue: Arc<Mutex<Vec<TilePlacement>>>,
    pub git_status_check_in_progress: bool,

    pub drag_source: Option<PathBuf>,
    pub is_dragging: bool,
    pub drag_start_pos: Option<(u16, u16)>,
    pub hovered_drop_target: Option<DropTarget>,

    pub starred: Vec<PathBuf>,
    pub sidebar_bounds: Vec<SidebarBounds>,
    pub tab_bounds: Vec<(Rect, usize, usize)>, // (Rect, pane_idx, tab_idx)

    pub mouse_last_click: std::time::Instant,
    pub mouse_click_pos: (u16, u16),
    pub settings_section: SettingsSection,
    pub settings_target: SettingsTarget,
    
    // Global Preferences
    pub default_show_hidden: bool,
    pub confirm_delete: bool,
    pub preferred_terminal: Option<String>,

    pub single_columns: Vec<FileColumn>,
    pub split_columns: Vec<FileColumn>,
}

impl App {
    pub fn new(tile_queue: Arc<Mutex<Vec<TilePlacement>>>) -> Self {
        log_debug("App::new start");
        let system_state = SystemState {
            last_update: std::time::Instant::now(),
            disks: Vec::new(),
            processes: Vec::new(),
            cpu_usage: 0.0,
            mem_usage: 0.0,
            total_mem: 0.0,
        };

        let license = check_license();
        log_debug("License checked");

        if let Some(mut state) = crate::config::load_state() {
            log_debug("State loaded from config");
            if !state.panes.is_empty() {
                // FORCE RESTORE mandatory columns and sensible defaults if missing or corrupted in saved state
                for pane in &mut state.panes {
                    for tab in &mut pane.tabs {
                        // Clear any search state that might have been saved
                        tab.search_filter.clear();
                        tab.local_count = 0;

                        // Ensure Name is always there
                        if !tab.columns.contains(&FileColumn::Name) {
                            tab.columns.insert(0, FileColumn::Name);
                        }
                        
                        // If columns list became empty for some reason, restore defaults
                        if tab.columns.len() <= 1 { // Only has Name or is empty
                             tab.columns = vec![
                                 FileColumn::Name,
                                 FileColumn::Size,
                                 FileColumn::Modified,
                             ];
                        }
                    }
                }

                log_debug("Returning early with loaded state");
                return Self {
                    running: true,
                    current_view: state.current_view,
                    mode: AppMode::Normal,
                    input: String::new(),
                    panes: state.panes,
                    focused_pane_index: state.focused_pane_index,
                    terminal_size: (0, 0),
                    mouse_pos: (0, 0),
                    system_state,
                    license,
                    sidebar_focus: false,
                    sidebar_index: 0,
                    remote_bookmarks: state.remote_bookmarks,
                    active_sessions: HashMap::new(),
                    filtered_commands: Vec::new(),
                    command_index: 0,
                    last_click: None,
                    tile_queue,
                    git_status_check_in_progress: false,
                    drag_source: None,
                    is_dragging: false,
                    drag_start_pos: None,
                    hovered_drop_target: None,
                    starred: state.starred,
                    sidebar_bounds: Vec::new(),
                    mouse_last_click: std::time::Instant::now(),
                    mouse_click_pos: (0, 0),
                    settings_section: SettingsSection::Columns,
                    settings_target: SettingsTarget::SingleMode,
                    default_show_hidden: false,
                    confirm_delete: true,
                    preferred_terminal: None,
                    single_columns: vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified, FileColumn::Permissions],
                    split_columns: vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified],
                };
            }
        }

        log_debug("No valid state found, starting fresh");
        let initial_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        log_debug(&format!("Initial path: {:?}", initial_path));
        let mut file_state = FileState::new(
            initial_path.clone(),
            None,
            false,
            vec![
                FileColumn::Name,
                FileColumn::Size,
                FileColumn::Modified,
                FileColumn::Permissions,
            ],
            FileColumn::Name,
            true,
        );
        file_state.table_state.select(Some(0));
        log_debug("Initial file state created");
        update_files(&mut file_state, None);
        log_debug("Initial files updated");

        let license = check_license();

        let app = Self {
            running: true,
            current_view: CurrentView::Files,
            mode: AppMode::Normal,
            input: String::new(),

            panes: vec![Pane::new(file_state)],
            focused_pane_index: 0,

            terminal_size: (0, 0),
            mouse_pos: (0, 0),
            system_state,
            license,
            sidebar_focus: false,
            sidebar_index: 0,
            remote_bookmarks: Vec::new(),
            active_sessions: HashMap::new(),
            filtered_commands: Vec::new(),
            command_index: 0,
            last_click: None,
            tile_queue,
            git_status_check_in_progress: false,

            drag_source: None,
            is_dragging: false,
            drag_start_pos: None,
            hovered_drop_target: None,

            starred: {
                let mut s = Vec::new();
                if let Some(p) = dirs::home_dir() {
                    if !s.contains(&p) {
                        s.push(p);
                    }
                }
                if let Some(p) = dirs::download_dir() {
                    if !s.contains(&p) {
                        s.push(p);
                    }
                }
                if let Some(p) = dirs::document_dir() {
                    if !s.contains(&p) {
                        s.push(p);
                    }
                }
                if let Some(p) = dirs::picture_dir() {
                    if !s.contains(&p) {
                        s.push(p);
                    }
                }
                s
            },
            sidebar_bounds: Vec::new(),

            mouse_last_click: std::time::Instant::now(),
            mouse_click_pos: (0, 0),
            settings_section: SettingsSection::Columns,
            settings_target: SettingsTarget::SingleMode,
            default_show_hidden: false,
            confirm_delete: true,
            preferred_terminal: None,
            single_columns: vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified, FileColumn::Permissions],
            split_columns: vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified],
        };
        log_debug("App::new finished successfully");
        app
    }

    pub fn current_file_state_mut(&mut self) -> Option<&mut FileState> {
        self.panes
            .get_mut(self.focused_pane_index)
            .and_then(|p| p.current_state_mut())
    }

    pub fn current_file_state(&self) -> Option<&FileState> {
        self.panes
            .get(self.focused_pane_index)
            .and_then(|p| p.current_state())
    }

    pub fn toggle_split(&mut self) {
        if self.panes.len() == 1 {
            // Entering Split Mode
            if let Some(fs) = self.current_file_state() {
                let mut new_fs = fs.clone();
                self.panes.push(Pane::new(new_fs));
            }
            
            // Apply Split Mode columns to all panes/tabs
            for pane in &mut self.panes {
                for tab in &mut pane.tabs {
                    tab.columns = self.split_columns.clone();
                }
            }
        } else {
            // Entering Single Mode
            self.panes.pop();
            self.focused_pane_index = 0;
            
            // Apply Single Mode columns to the remaining pane/tabs
            if let Some(pane) = self.panes.get_mut(0) {
                for tab in &mut pane.tabs {
                    tab.columns = self.single_columns.clone();
                }
            }
        }
    }

    pub fn update_files_for_active_tab(&mut self, pane_idx: usize) {
        if let Some(pane) = self.panes.get_mut(pane_idx) {
            if let Some(fs) = pane.current_state_mut() {
                update_files(fs, None);
            }
        }
    }

    pub fn switch_view(&mut self) {
        self.current_view = match self.current_view {
            CurrentView::Files => CurrentView::Processes,
            CurrentView::Processes => CurrentView::Files,
        };
    }

    pub fn toggle_hidden(&mut self) -> usize {
        if let Some(fs) = self.current_file_state_mut() {
            fs.show_hidden = !fs.show_hidden;
        }
        self.focused_pane_index
    }

    pub fn toggle_zoom(&mut self) {
        // Implementation here if needed
    }

    pub fn toggle_column(&mut self, col: FileColumn) {
        // Name is mandatory
        if col == FileColumn::Name {
            return;
        }

        let target_cols = match self.settings_target {
            SettingsTarget::SingleMode => &mut self.single_columns,
            SettingsTarget::SplitMode => &mut self.split_columns,
        };

        if target_cols.contains(&col) {
            target_cols.retain(|c| c != &col);
        } else {
            target_cols.push(col);
        }

                        // Maintain a consistent default order
                        let order = [
                            FileColumn::Name,
                            FileColumn::Size,
                            FileColumn::Modified,
                            FileColumn::Permissions,
                        ];
        let mut sorted = Vec::new();
        for &c in &order {
            if target_cols.contains(&c) {
                sorted.push(c);
            }
        }
        *target_cols = sorted;

        // Apply to active panes immediately if the target matches the current view mode
        let current_mode = if self.panes.len() == 1 { SettingsTarget::SingleMode } else { SettingsTarget::SplitMode };
        if self.settings_target == current_mode {
            for pane in &mut self.panes {
                for tab in &mut pane.tabs {
                    tab.columns = target_cols.clone();
                }
            }
        }
    }

    pub fn move_up(&mut self, shift: bool) {
        if self.sidebar_focus {
            if self.sidebar_index > 0 {
                self.sidebar_index -= 1;
            }
            return;
        }
        if let Some(fs) = self.current_file_state_mut() {
            let old_idx = fs.selected_index.unwrap_or(0);
            let mut i = if old_idx == 0 {
                fs.files.len().saturating_sub(1)
            } else {
                old_idx - 1
            };

            // Skip divider
            if fs.files[i].to_string_lossy() == "__DIVIDER__" {
                i = if i == 0 { fs.files.len().saturating_sub(1) } else { i - 1 };
            }

            fs.selected_index = Some(i);
            fs.table_state.select(Some(i));

            if shift {
                if fs.selection_anchor.is_none() {
                    fs.selection_anchor = Some(old_idx);
                }
                let anchor = fs.selection_anchor.unwrap();
                fs.multi_select.clear();
                let start = std::cmp::min(anchor, i);
                let end = std::cmp::max(anchor, i);
                for idx in start..=end {
                    fs.multi_select.insert(idx);
                }
            } else {
                fs.selection_anchor = None;
                fs.multi_select.clear();
            }
        }
    }

    pub fn move_down(&mut self, shift: bool) {
        if self.sidebar_focus {
            if self.sidebar_index < self.sidebar_bounds.len().saturating_sub(1) {
                self.sidebar_index += 1;
            }
            return;
        }
        if let Some(fs) = self.current_file_state_mut() {
            let old_idx = fs.selected_index.unwrap_or(0);
            let mut i = if old_idx >= fs.files.len().saturating_sub(1) {
                0
            } else {
                old_idx + 1
            };

            // Skip divider
            if i < fs.files.len() && fs.files[i].to_string_lossy() == "__DIVIDER__" {
                i = if i >= fs.files.len().saturating_sub(1) { 0 } else { i + 1 };
            }

            fs.selected_index = Some(i);
            fs.table_state.select(Some(i));

            if shift {
                if fs.selection_anchor.is_none() {
                    fs.selection_anchor = Some(old_idx);
                }
                let anchor = fs.selection_anchor.unwrap();
                fs.multi_select.clear();
                let start = std::cmp::min(anchor, i);
                let end = std::cmp::max(anchor, i);
                for idx in start..=end {
                    fs.multi_select.insert(idx);
                }
            } else {
                fs.selection_anchor = None;
                fs.multi_select.clear();
            }
        }
    }

    pub fn move_left(&mut self) {
        if self.focused_pane_index == 0 && !self.sidebar_focus {
            self.sidebar_focus = true;
        } else if self.focused_pane_index > 0 {
            self.focused_pane_index -= 1;
            self.sidebar_focus = false;
        }
    }

    pub fn move_right(&mut self) {
        if self.sidebar_focus {
            self.sidebar_focus = false;
            self.focused_pane_index = 0;
        } else if self.focused_pane_index < self.panes.len().saturating_sub(1) {
            self.focused_pane_index += 1;
        }
    }

    pub fn sidebar_width(&self) -> u16 {
        use ratatui::layout::{Constraint, Direction, Layout, Rect};
        let layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(20), Constraint::Min(0)])
            .split(Rect::new(0, 0, self.terminal_size.0, self.terminal_size.1));
        layout[0].width
    }

    pub fn copy_to_other_pane(&mut self) {
        if self.panes.len() < 2 {
            return;
        }
        let other_pane_idx = if self.focused_pane_index == 0 { 1 } else { 0 };

        let dest_path = if let Some(other_fs) = self.panes[other_pane_idx].current_state() {
            other_fs.current_path.clone()
        } else {
            return;
        };

        if let Some(fs) = self.current_file_state_mut() {
            let mut paths_to_copy = Vec::new();
            if !fs.multi_select.is_empty() {
                for &idx in &fs.multi_select {
                    if let Some(p) = fs.files.get(idx) {
                        paths_to_copy.push(p.clone());
                    }
                }
            } else if let Some(idx) = fs.selected_index {
                if let Some(p) = fs.files.get(idx) {
                    paths_to_copy.push(p.clone());
                }
            }

            for src in paths_to_copy {
                if let Some(filename) = src.file_name() {
                    let dest = dest_path.join(filename);
                    let _ = crate::modules::files::copy_recursive(&src, &dest);
                }
            }
        }
    }

    pub fn move_to_other_pane(&mut self) {
        log_debug("move_to_other_pane start");
        if self.panes.len() < 2 {
            log_debug("Not enough panes for move");
            return;
        }
        let other_pane_idx = if self.focused_pane_index == 0 { 1 } else { 0 };

        let dest_path = if let Some(other_fs) = self.panes[other_pane_idx].current_state() {
            other_fs.current_path.clone()
        } else {
            log_debug("Target pane has no state");
            return;
        };

        if let Some(fs) = self.current_file_state_mut() {
            let mut paths_to_move = Vec::new();
            if !fs.multi_select.is_empty() {
                for &idx in &fs.multi_select {
                    if let Some(p) = fs.files.get(idx) {
                        paths_to_move.push(p.clone());
                    }
                }
            } else if let Some(idx) = fs.selected_index {
                if let Some(p) = fs.files.get(idx) {
                    paths_to_move.push(p.clone());
                }
            }

            log_debug(&format!("Found {} paths to move to {:?}", paths_to_move.len(), dest_path));

            for src in paths_to_move {
                if let Some(filename) = src.file_name() {
                    let dest = dest_path.join(filename);
                    log_debug(&format!("Moving {:?} to {:?}", src, dest));
                    if let Err(e) = crate::modules::files::move_recursive(&src, &dest) {
                        log_debug(&format!("Move failed from {:?} to {:?}: {}", src, dest, e));
                    }
                }
            }
            // Clear multi-select after move
            fs.multi_select.clear();
            fs.selection_anchor = None;
        } else {
            log_debug("Current pane has no state");
        }
    }
}

pub fn log_debug(msg: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open("debug.log")
    {
        let _ = writeln!(file, "[{}] {}", chrono::Local::now(), msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_logic() {
        let mut fs = FileState::new(
            PathBuf::from("/"),
            None,
            false,
            vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified],
            FileColumn::Name,
            true,
        );

        fs.files = (0..100)
            .map(|i| PathBuf::from(format!("/file_{}", i)))
            .collect();
        fs.view_height = 20;

        fs.selected_index = Some(0);
        fs.table_state.select(Some(0));
        assert_eq!(fs.table_state.offset(), 0);
    }

    #[test]
    fn test_scroll_logic_small_files() {
        let mut fs = FileState::new(
            PathBuf::from("/"),
            None,
            false,
            vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified],
            FileColumn::Name,
            true,
        );

        fs.files = (0..10)
            .map(|i| PathBuf::from(format!("/file_{}", i)))
            .collect();
        fs.view_height = 20;
        assert_eq!(fs.table_state.offset(), 0);
    }
}
