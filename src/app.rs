use std::path::PathBuf;
use crate::modules::system::SystemModule;
use crate::modules::files::update_files;
use crate::license::check_license;

#[derive(Debug, PartialEq)]
pub enum AppMode {
    Normal,
    Input,
    Zoomed,
    CommandPalette,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TileType {
    Files,
    Docker,
    System,
    Logs,
}

pub enum LicenseStatus {
    FreeMode,
    Commercial(String),
}

pub struct App {
    pub running: bool,
    pub active_tile: TileType,
    pub mode: AppMode,
    pub input: String,
    pub file_state: FileState,
    pub docker_state: DockerState,
    pub system_state: SystemState,
    pub license: LicenseStatus,
    pub system_module: SystemModule,
}

pub struct FileState {
    pub current_path: PathBuf,
    pub selected_index: usize,
    pub files: Vec<PathBuf>,
}

pub struct DockerState {
    pub containers: Vec<String>,
    pub selected_index: usize,
    pub filter: Option<String>,
}

pub struct SystemState {
    pub cpu_usage: f32,
    pub mem_usage: f64,
    pub total_mem: f64,
}

impl App {
    pub fn new() -> Self {
        let mut system_module = SystemModule::new();
        let mut system_state = SystemState {
            cpu_usage: 0.0,
            mem_usage: 0.0,
            total_mem: 0.0,
        };
        system_module.update(&mut system_state);

        let mut file_state = FileState {
            current_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            selected_index: 0,
            files: Vec::new(),
        };
        update_files(&mut file_state);

        let license = check_license();

        Self {
            running: true,
            active_tile: TileType::Files,
            mode: AppMode::Normal,
            input: String::new(),
            file_state,
            docker_state: DockerState {
                containers: Vec::new(),
                selected_index: 0,
                filter: None,
            },
            system_state,
            system_module,
            license,
        }
    }

    pub fn next_tile(&mut self) {
        self.active_tile = match self.active_tile {
            TileType::Files => TileType::System,
            TileType::System => TileType::Docker,
            TileType::Docker => TileType::Files,
            _ => TileType::Files,
        };
    }

    pub fn toggle_zoom(&mut self) {
        self.mode = match self.mode {
            AppMode::Zoomed => AppMode::Normal,
            _ => AppMode::Zoomed,
        };
    }

    pub fn move_up(&mut self) {
        match self.active_tile {
            TileType::Files => {
                if self.file_state.selected_index > 0 {
                    self.file_state.selected_index -= 1;
                }
            }
            TileType::Docker => {
                if self.docker_state.selected_index > 0 {
                    self.docker_state.selected_index -= 1;
                } else {
                    self.active_tile = TileType::System;
                }
            }
            TileType::System => {}
            _ => {}
        }
    }

    pub fn move_down(&mut self) {
        match self.active_tile {
            TileType::Files => {
                if self.file_state.selected_index < self.file_state.files.len().saturating_sub(1) {
                    self.file_state.selected_index += 1;
                }
            }
            TileType::System => {
                self.active_tile = TileType::Docker;
            }
            TileType::Docker => {
                if self.docker_state.selected_index < self.docker_state.containers.len().saturating_sub(1) {
                    self.docker_state.selected_index += 1;
                }
            }
            _ => {}
        }
    }

    pub fn move_left(&mut self) {
        match self.active_tile {
            TileType::System | TileType::Docker => {
                self.active_tile = TileType::Files;
            }
            _ => {}
        }
    }

    pub fn move_right(&mut self) {
        match self.active_tile {
            TileType::Files => {
                self.active_tile = TileType::System;
            }
            _ => {}
        }
    }
}