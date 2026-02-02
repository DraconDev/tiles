use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use terma::compositor::engine::TilePlacement;
use terma::widgets::TextEditor;
use std::sync::{Arc, Mutex};

pub use terma::utils::{FileCategory, FileColumn, IconMode, SelectionState};

#[derive(Clone, Debug)]
pub enum AppEvent {
    Tick,
    RefreshFiles(usize),
    CreateFile(PathBuf),
    CreateFolder(PathBuf),
    Rename(PathBuf, PathBuf),
    Delete(PathBuf),
    Copy(PathBuf, PathBuf),
    Symlink(PathBuf, PathBuf),
    StatusMsg(String),
    FilesChangedOnDisk(PathBuf),
    PreviewRequested(usize, PathBuf),
    SaveFile(PathBuf, String),
    GitHistory,
    SystemMonitor,
    ConnectToRemote(usize, usize),
    SystemUpdated(crate::modules::system::SystemData),
    Editor,
    Raw(terma::input::event::Event),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CurrentView {
    Files,
    Editor,
    Git,
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
    ProjectTree(PathBuf),
    Process(u32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ContextMenuAction {
    Open,
    OpenNewTab,
    OpenWith,
    Edit,
    Run,
    RunTerminal,
    ExtractHere,
    NewFolder,
    NewFile,
    Cut,
    Copy,
    CopyPath,
    CopyName,
    Paste,
    Rename,
    Duplicate,
    Compress,
    Delete,
    AddToFavorites,
    RemoveFromFavorites,
    Properties,
    TerminalWindow,
    TerminalTab,
    Refresh,
    SelectAll,
    ToggleHidden,
    ConnectRemote,
    DeleteRemote,
    Mount,
    Unmount,
    SetWallpaper,
    GitInit,
    GitStatus,
    SystemMonitor,
    Drag,
    SetColor(Option<u8>),
    SortBy(FileColumn),
    Separator,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SettingsSection {
    General,
    Columns,
    Tabs,
    Remotes,
    Shortcuts,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SettingsTarget {
    SingleMode,
    SplitMode,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AppMode {
    Normal,
    Editor,
    EditorSearch,
    EditorGoToLine,
    EditorReplace,
    Settings,
    Properties,
    Rename,
    NewFile,
    NewFolder,
    Delete,
    DeleteFile(PathBuf),
    Search,
    CommandPalette,
    AddRemote(usize),
    ImportServers,
    Viewer,
    Hotkeys,
    Header(usize),
    Highlight,
    DragDropMenu {
        sources: Vec<PathBuf>,
        target: PathBuf,
    },
    ContextMenu {
        x: u16,
        y: u16,
        target: ContextMenuTarget,
        actions: Vec<ContextMenuAction>,
        selected_index: Option<usize>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum DropTarget {
    Favorites,
    SidebarArea,
    Folder(PathBuf),
    ImportServers,
    RemotesHeader,
    Pane(usize),
    ReorderFavorite(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SidebarBounds {
    pub y: u16,
    pub index: usize,
    pub target: SidebarTarget,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SidebarTarget {
    Favorite(PathBuf),
    Remote(usize),
    Storage(usize),
    Project(PathBuf),
    Header(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandAction {
    Quit,
    ToggleZoom,
    SwitchView(CurrentView),
    AddRemote,
    ConnectToRemote(usize),
    CommandPalette,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileMetadata {
    pub size: u64,
    pub modified: std::time::SystemTime,
    pub created: std::time::SystemTime,
    pub permissions: u32,
    pub is_dir: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RemoteBookmark {
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub last_path: PathBuf,
    pub key_path: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileState {
    pub current_path: PathBuf,
    pub remote_session: Option<RemoteSession>,
    pub files: Vec<PathBuf>,
    pub selection: SelectionState,
    pub show_hidden: bool,
    pub search_filter: String,
    pub columns: Vec<FileColumn>,
    pub history: Vec<PathBuf>,
    pub history_index: usize,
    pub sort_column: FileColumn,
    pub sort_ascending: bool,
    #[serde(skip)]
    pub metadata: HashMap<PathBuf, FileMetadata>,
    #[serde(skip)]
    pub path_colors: HashMap<PathBuf, u8>,
    #[serde(skip)]
    pub preview: Option<String>,
    #[serde(skip)]
    pub view_height: usize,
    #[serde(skip)]
    pub table_state: ratatui::widgets::TableState,
    #[serde(skip)]
    pub column_bounds: Vec<(ratatui::layout::Rect, FileColumn)>,
    #[serde(skip)]
    pub breadcrumb_bounds: Vec<(ratatui::layout::Rect, PathBuf)>,
    #[serde(skip)]
    pub local_count: usize,
    #[serde(skip)]
    pub pending_select_path: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct RemoteSession {
    pub host: String,
    pub user: String,
}

impl FileState {
    pub fn new(path: PathBuf, remote: Option<RemoteSession>, show_hidden: bool, columns: Vec<FileColumn>, sort_col: FileColumn, sort_asc: bool) -> Self {
        Self {
            current_path: path.clone(),
            remote_session: remote,
            files: Vec::new(),
            selection: SelectionState::default(),
            show_hidden,
            search_filter: String::new(),
            columns,
            history: vec![path],
            history_index: 0,
            sort_column: sort_col,
            sort_ascending: sort_asc,
            metadata: HashMap::new(),
            path_colors: HashMap::new(),
            preview: None,
            view_height: 0,
            table_state: ratatui::widgets::TableState::default(),
            column_bounds: Vec::new(),
            breadcrumb_bounds: Vec::new(),
            local_count: 0,
            pending_select_path: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SystemState {
    pub last_update: std::time::Instant,
    pub disks: Vec<crate::modules::system::DiskData>,
    pub processes: Vec<crate::modules::system::ProcessData>,
    pub cpu_usage: f32,
    pub cpu_cores: Vec<f32>,
    pub mem_usage: f32,
    pub total_mem: f64,
    pub swap_usage: f32,
    pub total_swap: f64,
    pub cpu_history: Vec<u64>,
    pub core_history: Vec<Vec<u64>>,
    pub mem_history: Vec<u64>,
    pub swap_history: Vec<u64>,
    pub net_in: u64,
    pub net_out: u64,
    pub net_in_history: Vec<u64>,
    pub net_out_history: Vec<u64>,
    pub last_net_in: u64,
    pub last_net_out: u64,
    pub uptime: u64,
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub hostname: String,
}

#[derive(Clone, Debug)]
pub struct PreviewState {
    pub path: PathBuf,
    pub content: String,
    pub scroll: usize,
    pub editor: Option<TextEditor>,
    pub last_saved: Option<std::time::Instant>,
    pub image_data: Option<(Vec<u8>, u32, u32)>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewPreferences {
    pub show_sidebar: bool,
    pub is_split_mode: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewStatePersistence {
    pub files: ViewPreferences,
    pub editor: ViewPreferences,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Pane {
    pub tabs: Vec<FileState>,
    pub active_tab_index: usize,
    #[serde(skip)]
    pub preview: Option<PreviewState>,
}

impl Pane {
    pub fn new(initial_fs: FileState) -> Self {
        Self {
            tabs: vec![initial_fs],
            active_tab_index: 0,
            preview: None,
        }
    }
    pub fn current_state(&self) -> Option<&FileState> { self.tabs.get(self.active_tab_index) }
    pub fn current_state_mut(&mut self) -> Option<&mut FileState> { self.tabs.get_mut(self.active_tab_index) }
    pub fn open_tab(&mut self, fs: FileState) { self.tabs.push(fs); self.active_tab_index = self.tabs.len() - 1; }
}

#[derive(Clone, Debug)]
pub struct BackgroundTask {
    pub id: uuid::Uuid,
    pub description: String,
    pub progress: f32,
}

#[derive(Clone, Debug)]
pub enum UndoAction {
    Rename(PathBuf, PathBuf),
    Move(PathBuf, PathBuf),
    Copy(PathBuf, PathBuf),
    Delete(PathBuf),
}

#[derive(Clone, Debug)]
pub enum LicenseStatus {
    Valid,
    Invalid(String),
    TrialExpired,
    FreeMode,
}
