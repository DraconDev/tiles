use ratatui::layout::Rect;
use ratatui::widgets::TableState;
use ratatui::text::Line;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use terma::input::event::Event as TermaEvent;
pub use terma::system::{DiskInfo, ProcessInfo, SystemData};
pub use terma::utils::{FileCategory, FileColumn, IconMode, SelectionState};
pub use terma::widgets::context_menu::ContextMenuAction;
use terma::widgets::{TextEditor, TextInput};

#[derive(Clone, Debug)]
pub enum AppEvent {
    FilesChangedOnDisk(PathBuf),
    RefreshFiles(usize),
    FilesUpdated(usize, Vec<PathBuf>, HashMap<PathBuf, FileMetadata>, HashMap<PathBuf, String>, Option<String>, usize),
    Tick,
    Raw(TermaEvent),
    SystemUpdated(SystemData),
    CreateFile(PathBuf),
    CreateFolder(PathBuf),
    Rename(PathBuf, PathBuf),
    Copy(PathBuf, PathBuf),
    Symlink(PathBuf, PathBuf),
    Delete(PathBuf),
    SaveFile(PathBuf, String),
    RemoteConnected(usize, RemoteSession),
    ConnectToRemote(usize, usize),
    PreviewRequested(usize, PathBuf),
    SpawnTerminal {
        path: PathBuf,
        new_tab: bool,
        remote: Option<RemoteSession>,
        command: Option<String>,
    },
    MountDisk(String),
    SpawnDetached { cmd: String, args: Vec<String> },
    StatusMsg(String),
    KillProcess(u32),
    SystemMonitor,
    GitHistory,
    Editor,
    GitHistoryUpdated(usize, usize, Vec<CommitInfo>, Vec<GitStatus>),
    TaskProgress(Uuid, f32, String),
    TaskFinished(Uuid),
    GlobalSearchUpdated(usize, Vec<PathBuf>, HashMap<PathBuf, FileMetadata>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatus {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
    pub hash: String,
    pub author: String,
    pub timestamp: u64,
    pub date: String,
    pub message: String,
    pub refs: String,
    pub insertions: usize,
    pub deletions: usize,
    pub files_changed: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum MonitorSubview {
    Overview,
    Applications,
    Processes,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ProcessColumn {
    Pid,
    Name,
    Cpu,
    Mem,
    User,
    Status,
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
pub enum SettingsSection {
    Columns,
    Tabs,
    General,
    Remotes,
    Shortcuts,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsTarget {
    SingleMode,
    SplitMode,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppMode {
    Normal,
    Editor,
    EditorSearch,
    EditorGoToLine,
    EditorReplace,
    Hotkeys,
    Rename,
    Delete,
    DeleteFile(PathBuf),
    NewFolder,
    NewFile,
    Search,
    Command,
    RemoteAdd,
    TabSearch,
    Properties,
    Highlight,
    Ide,
    ContextMenu {
        x: u16,
        y: u16,
        target: ContextMenuTarget,
        actions: Vec<ContextMenuAction>,
        selected_index: Option<usize>,
    },
    CommandPalette,
    Location,
    Settings,
    AddRemote(usize),
    ImportServers,
    Header(usize),
    OpenWith(PathBuf),
    Engage,
    Viewer,
    DragDropMenu {
        sources: Vec<PathBuf>,
        target: PathBuf,
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
    Disk(String),
    Project(PathBuf),
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandAction {
    Quit,
    ToggleZoom,
    SwitchView(CurrentView),
    AddRemote,
    ConnectToRemote(usize),
    ImportServers,
    CommandPalette,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ClipboardOp {
    Copy,
    Cut,
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
    pub key_path: Option<PathBuf>,
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
    pub selection: SelectionState,
    #[serde(skip)]
    pub table_state: TableState,
    #[serde(skip)]
    pub files: Vec<PathBuf>,
    #[serde(skip)]
    pub metadata: HashMap<PathBuf, FileMetadata>,
    #[serde(skip)]
    pub git_status: HashMap<PathBuf, String>,
    pub show_hidden: bool,
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
    pub git_ahead: usize,
    #[serde(skip)]
    pub git_behind: usize,
    #[serde(skip)]
    pub local_count: usize,
    #[serde(skip)]
    pub pending_select_path: Option<PathBuf>,
    #[serde(skip)]
    pub git_history: Vec<CommitInfo>,
    #[serde(skip)]
    pub git_pending: Vec<GitStatus>,
    #[serde(skip)]
    pub git_history_state: TableState,
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
            selection: SelectionState::default(),
            table_state: TableState::default(),
            files: Vec::new(),
            metadata: HashMap::new(),
            git_status: HashMap::new(),
            show_hidden,
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
            git_ahead: 0,
            git_behind: 0,
            local_count: 0,
            pending_select_path: None,
            git_history: Vec::new(),
            git_pending: Vec::new(),
            git_history_state: TableState::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SystemState {
    pub last_update: std::time::Instant,
    pub disks: Vec<DiskInfo>,
    pub processes: Vec<ProcessInfo>,
    pub cpu_usage: f32,
    pub cpu_cores: Vec<f32>,
    pub mem_usage: f64,
    pub total_mem: f64,
    pub swap_usage: f64,
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
    pub highlighted_lines: Option<Vec<Line<'static>>>,
}

#[derive(Serialize, Deserialize)]
pub struct Pane {
    pub tabs: Vec<FileState>,
    pub active_tab_index: usize,
    #[serde(skip)]
    pub preview: Option<PreviewState>,
}

impl Pane {
    pub fn new(initial_state: FileState) -> Self {
        Self {
            tabs: vec![initial_state],
            active_tab_index: 0,
            preview: None,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CurrentView {
    Files,
    Processes,
    Git,
    Editor,
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

#[derive(Clone, Debug)]
pub enum UndoAction {
    Rename(PathBuf, PathBuf),
    Move(PathBuf, PathBuf),
    Copy(PathBuf, PathBuf),
    Delete(PathBuf),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseStatus {
    Valid,
    Invalid,
    TrialExpired,
    Commercial,
    FreeMode,
}