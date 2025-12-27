use std::path::PathBuf;

pub enum AppMode {
    Normal,
    Input,
    Zoomed,
}

#[derive(PartialEq, Eq, Clone, Copy)]
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
}

pub struct FileState {
    pub current_path: PathBuf,
    pub selected_index: usize,
    pub files: Vec<PathBuf>,
}

pub struct DockerState {
    pub containers: Vec<String>, // Placeholder
}

pub struct SystemState {
    pub cpu_usage: Vec<f32>,
    pub mem_usage: f32,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: true,
            active_tile: TileType::Files,
            mode: AppMode::Normal,
            file_state: FileState {
                current_path: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                selected_index: 0,
                files: Vec::new(),
            },
            docker_state: DockerState {
                containers: Vec::new(),
            },
            system_state: SystemState {
                cpu_usage: Vec::new(),
                mem_usage: 0.0,
            },
            license: LicenseStatus::FreeMode,
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
