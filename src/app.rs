use std::path::PathBuf;
use crate::modules::system::SystemModule;
use crate::modules::files::update_files;
use crate::license::check_license;

#[derive(Debug)]
pub enum AppMode {
    Normal,
    Zoomed,
    CommandPalette,
    Location, // Ctrl+L mode
    Rename,   // F2 mode
    Properties, // Alt+Enter mode
    NewFolder,  // Ctrl+Shift+N mode
    Delete,     // Delete key mode
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
    pub filtered_commands: Vec<CommandItem>,
    pub command_index: usize,
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
}

pub struct FileState {
    pub current_path: PathBuf,
    pub selected_index: usize,
    pub files: Vec<PathBuf>,
    pub show_hidden: bool,
    pub git_status: HashMap<PathBuf, String>,
    pub clipboard: Option<(PathBuf, ClipboardOp)>,
    pub search_filter: String,
    pub starred: HashSet<PathBuf>,
    pub columns: Vec<FileColumn>,
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

use crate::app::FileColumn;

// ...

        let mut file_state = FileState {
            current_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            selected_index: 0,
            files: Vec::new(),
            show_hidden: false,
            git_status: HashMap::new(),
            clipboard: None,
            search_filter: String::new(),
            starred: HashSet::new(),
            columns: vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified],
        };
        update_files(&mut file_state);

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
            filtered_commands: Vec::new(),
            command_index: 0,
        }
    }

    // Helper to get mutable reference to current file state
    pub fn current_file_state_mut(&mut self) -> Option<&mut FileState> {
        self.file_tabs.get_mut(self.tab_index)
    }

    // Helper to get reference to current file state
    pub fn current_file_state(&self) -> Option<&FileState> {
        self.file_tabs.get(self.tab_index)
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
            match self.current_view {
                CurrentView::Files => {
                    if self.sidebar_index > 0 {
                        self.sidebar_index -= 1;
                    }
                }
                _ => {
                    // In Dock, moving up cycles views backwards
                    self.current_view = match self.current_view {
                        CurrentView::Files => CurrentView::Docker,
                        CurrentView::Docker => CurrentView::System,
                        CurrentView::System => CurrentView::Files,
                    };
                }
            }
            return;
        }

        match self.current_view {
            CurrentView::Files => {
                if let Some(file_state) = self.current_file_state_mut() {
                    if file_state.selected_index > 0 {
                        file_state.selected_index -= 1;
                    }
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
            match self.current_view {
                CurrentView::Files => {
                    if self.sidebar_index < 3 { // Hardcoded for Home, Downloads, Documents, Pictures
                        self.sidebar_index += 1;
                    }
                }
                _ => {
                    // In Dock, moving down cycles views
                    self.switch_view();
                }
            }
            return;
        }

        match self.current_view {
            CurrentView::Files => {
                if let Some(file_state) = self.current_file_state_mut() {
                    if file_state.selected_index < file_state.files.len().saturating_sub(1) {
                        file_state.selected_index += 1;
                    }
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
        self.sidebar_focus = true;
    }

    pub fn move_right(&mut self) {
        self.sidebar_focus = false;
    }
}