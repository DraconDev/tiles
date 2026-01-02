use crate::license::check_license;
use crate::modules::files::update_files;
use crate::modules::system::SystemModule;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use terma::compositor::engine::TilePlacement;
use terma::input::event::Event as TermaEvent;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AppMode {
    Normal,
    Zoomed,
    CommandPalette,
    Location,    // Ctrl+L mode
    Rename,      // F2 mode
    Properties,  // Alt+Enter mode
    NewFolder,   // Ctrl+Shift+N mode
    NewFile,     // New File mode
    Delete,      // Delete key mode
    ColumnSetup, // Column configuration mode
    AddRemote,   // Add new SSH remote host
    ContextMenu {
        x: u16,
        y: u16,
        item_index: Option<usize>,
    },
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum CurrentView {
    Files,
    System,
}
pub enum LicenseStatus {
    FreeMode,
    Commercial(String),
}

#[derive(Debug)]
pub enum AppEvent {
    RefreshFiles(usize), // tab_index
    CreateFile(String),  // filename
    FilesUpdated(
        usize,
        Vec<PathBuf>,
        HashMap<PathBuf, FileMetadata>,
        HashMap<PathBuf, String>,
    ), // tab_idx, files, metadata, git
    Tick,
    Raw(TermaEvent),
}

pub struct App {
    pub running: bool,
    pub current_view: CurrentView,
    pub mode: AppMode,
    pub input: String,
    pub file_tabs: Vec<FileState>,
    pub tab_index: usize,
    pub system_state: SystemState,
    pub license: LicenseStatus,
    pub system_module: SystemModule,
    pub sidebar_focus: bool, // true = focus is on sidebar/dock, false = focus is on main stage
    pub sidebar_index: usize,
    pub remote_bookmarks: Vec<RemoteBookmark>,
    pub active_sessions: HashMap<String, Arc<Mutex<ssh2::Session>>>, // host:port -> session
    pub filtered_commands: Vec<CommandItem>,
    pub command_index: usize,
    pub last_click: Option<(std::time::Instant, u16, u16)>, // time, row, col
    pub tile_queue: Arc<Mutex<Vec<TilePlacement>>>,
}

#[derive(Clone, Debug)]
pub struct CommandItem {
    pub label: String,
    pub action: CommandAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandAction {
    Quit,
    ToggleZoom,
    SwitchView(CurrentView),
    AddRemote,
    ConnectToRemote(usize), // index into remote_bookmarks
}

use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipboardOp {
    Copy,
    Cut,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FileColumn {
    Name,
    Size,
    Modified,
    Created,
    Permissions,
    Extension,
}

use ratatui::widgets::TableState;

#[derive(Clone)]
pub struct RemoteSession {
    pub name: String,
    pub host: String,
    pub user: String,
    pub session: Arc<Mutex<ssh2::Session>>,
}

#[derive(Clone, Debug)]
pub struct RemoteBookmark {
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub last_path: PathBuf,
}

#[derive(Clone, Debug)]
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

pub struct FileState {
    pub current_path: PathBuf,
    pub remote_session: Option<RemoteSession>, // None = Local, Some = SSH
    pub selected_index: Option<usize>,
    pub table_state: TableState,
    pub files: Vec<PathBuf>,
    pub metadata: HashMap<PathBuf, FileMetadata>, // PRE-FETCHED CACHE
    pub show_hidden: bool,
    pub git_status: HashMap<PathBuf, String>,
    pub clipboard: Option<(PathBuf, ClipboardOp)>,
    pub search_filter: String,
    pub starred: HashSet<PathBuf>,
    pub columns: Vec<FileColumn>,
    pub history: Vec<PathBuf>,
    pub history_index: usize,
    pub view_height: usize,
}

use bollard::models::ContainerSummary;

pub struct DockerState {
    pub containers: Vec<ContainerSummary>,
    pub selected_index: usize,
    pub filter: Option<String>,
}

pub struct SystemState {
    pub cpu_usage: f32,
    pub mem_usage: f64,
    pub total_mem: f64,
    pub disks: Vec<DiskInfo>,
    pub processes: Vec<ProcessInfo>,
    pub selected_process_index: usize,
}

pub struct DiskInfo {
    pub name: String,
    pub used_space: f64,  // GB
    pub total_space: f64, // GB
}

pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem: u64,
}

impl App {
    pub fn new(tile_queue: Arc<Mutex<Vec<TilePlacement>>>) -> Self {
        let mut system_module = SystemModule::new();
        let mut system_state = SystemState {
            cpu_usage: 0.0,
            mem_usage: 0.0,
            total_mem: 0.0,
            disks: Vec::new(),
            processes: Vec::new(),
            selected_process_index: 0,
        };
        system_module.update(&mut system_state);

        let initial_path = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut file_state = FileState {
            current_path: initial_path.clone(),
            remote_session: None,
            selected_index: Some(0),
            table_state: TableState::default(),
            files: Vec::new(),
            metadata: HashMap::new(),
            show_hidden: false,
            git_status: HashMap::new(),
            clipboard: None,
            search_filter: String::new(),
            starred: HashSet::new(),
            columns: vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified],
            history: vec![initial_path],
            history_index: 0,
            view_height: 0,
        };
        file_state.table_state.select(Some(0));
        update_files(&mut file_state, None);

        let license = check_license();

        Self {
            running: true,
            current_view: CurrentView::Files,
            mode: AppMode::Normal,
            input: String::new(),
            file_tabs: vec![file_state],
            tab_index: 0,

            system_state,
            system_module,
            license,
            sidebar_focus: false,
            sidebar_index: 0,
            remote_bookmarks: Vec::new(),
            active_sessions: HashMap::new(),
            filtered_commands: Vec::new(),
            command_index: 0,
            last_click: None,
            tile_queue,
        }
    }

    pub fn current_file_state_mut(&mut self) -> Option<&mut FileState> {
        self.file_tabs.get_mut(self.tab_index)
    }

    pub fn current_file_state(&self) -> Option<&FileState> {
        self.file_tabs.get(self.tab_index)
    }

    pub fn update_files_for_state(&mut self, tab_idx: usize) {
        if let Some(fs) = self.file_tabs.get_mut(tab_idx) {
            if let Some(rs) = &fs.remote_session {
                let key = format!("{}:{}", rs.host, 22);
                if let Some(sess_mutex) = self.active_sessions.get(&key) {
                    if let Ok(sess) = sess_mutex.lock() {
                        update_files(fs, Some(&sess));
                        return;
                    }
                }
            }
            update_files(fs, None);
        }
    }

    pub fn switch_view(&mut self) {
        self.current_view = match self.current_view {
            CurrentView::Files => CurrentView::System,
            CurrentView::System => CurrentView::Files,
        };
    }

    pub fn toggle_zoom(&mut self) {
        self.mode = match self.mode {
            AppMode::Zoomed => AppMode::Normal,
            _ => AppMode::Zoomed,
        };
    }

    pub fn move_up(&mut self) {
        if self.sidebar_focus {
            if self.sidebar_index > 0 {
                self.sidebar_index -= 1;
                // Skip the gap at index 4
                if self.sidebar_index == 5 {
                    self.sidebar_index -= 2;
                }
            }
            return;
        }

        match self.current_view {
            CurrentView::Files => {
                if let Some(file_state) = self.current_file_state_mut() {
                    let new_index = match file_state.selected_index {
                        Some(i) => {
                            if i > 0 {
                                i - 1
                            } else {
                                0
                            }
                        }
                        None => file_state.table_state.offset(),
                    };

                    file_state.selected_index = Some(new_index);
                    file_state.table_state.select(Some(new_index));

                    // Manual Auto-Scroll (Keep Selection in View)
                    let offset = file_state.table_state.offset();
                    if new_index < offset {
                        *file_state.table_state.offset_mut() = new_index;
                    }
                }
            }
            CurrentView::System => {
                if self.system_state.selected_process_index > 0 {
                    self.system_state.selected_process_index -= 1;
                }
            }
        }
    }

    pub fn move_down(&mut self) {
        if self.sidebar_focus {
            let max_index: usize = 4 + self.remote_bookmarks.len(); // 4 local items + gap + remote items
            if self.sidebar_index < max_index.saturating_sub(1) {
                self.sidebar_index += 1;
                // Skip the gap at index 4
                if self.sidebar_index == 4 {
                    self.sidebar_index += 2;
                }
            }
            return;
        }

        match self.current_view {
            CurrentView::Files => {
                if let Some(file_state) = self.current_file_state_mut() {
                    let max_idx = file_state.files.len().saturating_sub(1);
                    let new_index = match file_state.selected_index {
                        Some(i) => {
                            if i < max_idx {
                                i + 1
                            } else {
                                max_idx
                            }
                        }
                        None => file_state.table_state.offset(),
                    };

                    file_state.selected_index = Some(new_index);
                    file_state.table_state.select(Some(new_index));

                    // Manual Auto-Scroll (Keep Selection in View)
                    if file_state.view_height > 2 {
                        let offset = file_state.table_state.offset();
                        let capacity = file_state.view_height.saturating_sub(2);
                        if new_index >= offset + capacity {
                            *file_state.table_state.offset_mut() =
                                new_index.saturating_sub(capacity).saturating_add(1);
                        }
                    }
                }
            }

            CurrentView::System => {
                if self.system_state.selected_process_index
                    < self.system_state.processes.len().saturating_sub(1)
                {
                    self.system_state.selected_process_index += 1;
                }
            }
        }
    }

    pub fn move_left(&mut self) {
        if !self.sidebar_focus {
            self.sidebar_focus = true;
        }
    }

    pub fn move_right(&mut self) {
        if self.sidebar_focus {
            self.sidebar_focus = false;
        }
    }
}

pub fn log_debug(msg: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("debug_tiles.log")
    {
        let _ = writeln!(file, "{}", msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_scroll_logic() {
        let mut fs = FileState {
            current_path: PathBuf::from("/"),
            remote_session: None,
            selected_index: None,
            table_state: ratatui::widgets::TableState::default(),
            files: (0..100)
                .map(|i| PathBuf::from(format!("/file_{}", i)))
                .collect(),
            metadata: std::collections::HashMap::new(),
            show_hidden: false,
            git_status: std::collections::HashMap::new(),
            clipboard: None,
            search_filter: String::new(),
            starred: std::collections::HashSet::new(),
            columns: vec![],
            history: vec![],
            history_index: 0,
            view_height: 20,
        };

        // Initial Selection at 0
        fs.selected_index = Some(0);
        fs.table_state.select(Some(0));

        // Capacity = 18. Effective = 15. Max = 85.
        let capacity = fs.view_height.saturating_sub(2);
        let effective_capacity = capacity.saturating_sub(3);
        let max_offset = fs.files.len().saturating_sub(effective_capacity);

        assert_eq!(max_offset, 85);

        // Scroll Down 1 (offset 0 -> 1)
        // Selection should be preserved
        let new_offset = (fs.table_state.offset() + 1).min(max_offset);
        *fs.table_state.offset_mut() = new_offset;

        assert_eq!(fs.table_state.offset(), 1);
        assert_eq!(fs.selected_index, Some(0));

        // Scroll Down to limit
        for _ in 0..100 {
            let n = (fs.table_state.offset() + 1).min(max_offset);
            *fs.table_state.offset_mut() = n;
        }

        assert_eq!(fs.table_state.offset(), 85);
        assert_eq!(fs.selected_index, Some(0)); // Still preserved
    }

    #[test]
    fn test_scroll_logic_small_files() {
        let mut fs = FileState {
            current_path: PathBuf::from("/"),
            remote_session: None,
            selected_index: None,
            table_state: ratatui::widgets::TableState::default(),
            files: (0..10)
                .map(|i| PathBuf::from(format!("/file_{}", i)))
                .collect(),
            metadata: std::collections::HashMap::new(),
            show_hidden: false,
            git_status: std::collections::HashMap::new(),
            clipboard: None,
            search_filter: String::new(),
            starred: std::collections::HashSet::new(),
            columns: vec![],
            history: vec![],
            history_index: 0,
            view_height: 20,
        };

        let capacity = fs.view_height.saturating_sub(2);
        let effective_capacity = capacity.saturating_sub(3);
        let max_offset = fs.files.len().saturating_sub(effective_capacity);

        assert_eq!(max_offset, 0);

        // Scroll Down
        let new_offset = (fs.table_state.offset() + 1).min(max_offset);
        *fs.table_state.offset_mut() = new_offset;
        assert_eq!(fs.table_state.offset(), 0);
    }
}
