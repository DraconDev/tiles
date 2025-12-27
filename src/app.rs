use std::path::PathBuf;
use crate::modules::system::SystemModule;
use crate::modules::files::update_files;
use crate::license::check_license;

#[derive(Debug)]
pub enum AppMode {
    Normal,
    Input,
    Zoomed,
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
            file_state,
            docker_state: DockerState {
                containers: Vec::new(),
                selected_index: 0,
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
}