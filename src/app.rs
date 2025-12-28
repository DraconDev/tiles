use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use crate::modules::system::SystemModule;
use crate::modules::files::update_files;
use crate::license::check_license;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum AppMode {
    Normal,
    Zoomed,
    CommandPalette,
    Location, // Ctrl+L mode
    Rename,   // F2 mode
    Properties, // Alt+Enter mode
    NewFolder,  // Ctrl+Shift+N mode
    Delete,     // Delete key mode
    ColumnSetup, // Column configuration mode
    AddRemote,   // Add new SSH remote host
    ContextMenu(u16, u16), // x, y coordinates
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum CurrentView {
    Files,
    Docker,
    System,
}

pub enum LicenseStatus {
    FreeMode,
    Commercial(String),
}

#[derive(Clone)]
pub struct RemoteBookmark {
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub last_path: PathBuf,
}

pub struct App {
    pub running: bool,
    pub current_view: CurrentView,
    pub mode: AppMode,
    pub input: String,
    pub file_tabs: Vec<FileState>,
    pub tab_index: usize,
    pub docker_state: DockerState,
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
    StartContainer(String),
    StopContainer(String),
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

pub struct FileState {
    pub current_path: PathBuf,
    pub remote_session: Option<RemoteSession>, // None = Local, Some = SSH
    pub selected_index: Option<usize>,
    pub table_state: TableState,
    pub files: Vec<PathBuf>,
    pub show_hidden: bool,
    pub git_status: HashMap<PathBuf, String>,
    pub clipboard: Option<(PathBuf, ClipboardOp)>,
    pub search_filter: String,
    pub starred: HashSet<PathBuf>,
    pub columns: Vec<FileColumn>,
    pub history: Vec<PathBuf>,
    pub history_index: usize,
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
    pub used_space: f64, // GB
    pub total_space: f64, // GB
}

pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu: f32,
    pub mem: u64,
}

impl App {
    pub fn new() -> Self {
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
            show_hidden: false,
            git_status: HashMap::new(),
            clipboard: None,
            search_filter: String::new(),
            starred: HashSet::new(),
            columns: vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified],
            history: vec![initial_path],
            history_index: 0,
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
            docker_state: DockerState {
                containers: Vec::new(),
                selected_index: 0,
                filter: None,
            },
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
            CurrentView::System => CurrentView::Docker,
            CurrentView::Docker => CurrentView::Files,
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
                if self.sidebar_index == 5 { self.sidebar_index -= 2; }
            }
            return;
        }

        match self.current_view {
            CurrentView::Files => {
                if let Some(file_state) = self.current_file_state_mut() {
                    let mut new_index = match file_state.selected_index {
                        Some(i) => if i > 0 { i - 1 } else { 0 },
                        None => file_state.table_state.offset(),
                    };
                    
                    // Logic to ensure selection stays in view
                    let offset = file_state.table_state.offset();
                    if new_index < offset {
                        *file_state.table_state.offset_mut() = new_index;
                    }
                    
                    file_state.selected_index = Some(new_index);
                    file_state.table_state.select(Some(new_index));
                }
            }
            CurrentView::Docker => {
                if self.docker_state.selected_index > 0 {
                    self.docker_state.selected_index -= 1;
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
            let max_index = 4 + self.remote_bookmarks.len(); // 4 local items + gap + remote items
            if self.sidebar_index < max_index.saturating_sub(1) {
                self.sidebar_index += 1;
                // Skip the gap at index 4
                if self.sidebar_index == 4 { self.sidebar_index += 2; }
            }
            return;
        }

        match self.current_view {
            CurrentView::Files => {
                if let Some(file_state) = self.current_file_state_mut() {
                    let max_idx = file_state.files.len().saturating_sub(1);
                    let mut new_index = match file_state.selected_index {
                        Some(i) => if i < max_idx { i + 1 } else { max_idx },
                        None => file_state.table_state.offset(),
                    };

                    // Logic to ensure selection stays in view
                    // We don't have the height here, but we can detect if we moved past the current offset
                    // In draw() the Table will handle the actual scrolling, but we need to ensure 
                    // the state allows it.
                    file_state.selected_index = Some(new_index);
                    file_state.table_state.select(Some(new_index));
                }
            }
            CurrentView::Docker => {
                if self.docker_state.selected_index < self.docker_state.containers.len().saturating_sub(1) {
                    self.docker_state.selected_index += 1;
                }
            }
            CurrentView::System => {
                if self.system_state.selected_process_index < self.system_state.processes.len().saturating_sub(1) {
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
