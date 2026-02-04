use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use terma::widgets::TextEditor;

pub use terma::system::{DiskInfo, ProcessInfo, SystemData};
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
    AddToFavorites(PathBuf),
    ConnectToRemote(usize, usize),
    RemoteConnected(usize, RemoteSession),
    SystemUpdated(SystemData),
    MountDisk(String),
    KillProcess(u32),
    GitHistoryUpdated(
        usize,
        usize,
        Vec<CommitInfo>,
        Vec<GitPendingChange>,
        Option<String>,
        usize,
        usize,
    ),
    TaskProgress(uuid::Uuid, f32, String),
    TaskFinished(uuid::Uuid),
    GlobalSearchUpdated(usize, Vec<PathBuf>, HashMap<PathBuf, FileMetadata>),
    SpawnTerminal {
        path: PathBuf,
        new_tab: bool,
        remote: Option<RemoteSession>,
        command: Option<String>,
    },
    SpawnDetached {
        cmd: String,
        args: Vec<String>,
    },
    Editor,
    Raw(terma::input::event::Event),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CurrentView {
    Files,
    Editor,
    Git,
    Processes,
    Tree,
    Galaxy,
}

#[derive(Clone, Debug)]
pub struct GalaxyNode {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub color: ratatui::style::Color,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub children: Vec<GalaxyNode>,
}

#[derive(Clone, Debug)]
pub struct GalaxyState {
    pub root: Option<GalaxyNode>,
    pub current_path: PathBuf,
    pub zoom: f32,
    pub pan: (f32, f32),
}

impl Default for GalaxyState {
    fn default() -> Self {
        Self {
            root: None,
            current_path: dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
            zoom: 1.0,
            pan: (0.0, 0.0),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TreeItem {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub expanded: bool,
    pub has_children: bool,
    pub color: ratatui::style::Color,
    pub children: Option<Vec<TreeItem>>,
}

#[derive(Clone, Debug)]
pub struct TreeState {
    pub root_items: Vec<TreeItem>,
    // Path of the "current selection/focus" in the flattened tree.
    // If easier, we can track indices, but Path is stable across re-renders.
    pub selected_path: Option<PathBuf>,
    pub scroll_offset: usize, // Vertical scroll line
    pub show_hidden: bool,
    pub column_width: u16,
}

impl Default for TreeState {
    fn default() -> Self {
        Self {
            root_items: Vec::new(),
            selected_path: None,
            scroll_offset: 0,
            show_hidden: false,
            column_width: 25,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    OpenWith(PathBuf),
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommandItem {
    pub key: String,
    pub desc: String,
    pub action: CommandAction,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SidebarTarget {
    Favorite(PathBuf),
    Remote(usize),
    Storage(usize),
    Project(PathBuf),
    Header(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize, Clone)]
pub struct RemoteSession {
    pub host: String,
    pub user: String,
    pub name: String,
    #[serde(skip)]
    pub session: Option<Arc<Mutex<ssh2::Session>>>,
}

impl std::fmt::Debug for RemoteSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteSession")
            .field("host", &self.host)
            .field("user", &self.user)
            .field("name", &self.name)
            .finish()
    }
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
    #[serde(skip)]
    pub git_history: Vec<CommitInfo>,
    #[serde(skip)]
    pub git_history_state: ratatui::widgets::TableState,
    #[serde(skip)]
    pub git_branch: Option<String>,
    #[serde(skip)]
    pub git_ahead: usize,
    #[serde(skip)]
    pub git_behind: usize,
    #[serde(skip)]
    pub git_pending: Vec<GitPendingChange>,
}

impl FileState {
    pub fn new(
        path: PathBuf,
        remote: Option<RemoteSession>,
        show_hidden: bool,
        columns: Vec<FileColumn>,
        sort_col: FileColumn,
        sort_asc: bool,
    ) -> Self {
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
            git_history: Vec::new(),
            git_history_state: ratatui::widgets::TableState::default(),
            git_branch: None,
            git_ahead: 0,
            git_behind: 0,
            git_pending: Vec::new(),
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
    pub mem_usage: f32,
    pub total_mem: f32,
    pub swap_usage: f32,
    pub total_swap: f32,
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
    pub highlighted_lines: Option<Vec<ratatui::text::Line<'static>>>,
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
    pub fn current_state(&self) -> Option<&FileState> {
        self.tabs.get(self.active_tab_index)
    }
    pub fn current_state_mut(&mut self) -> Option<&mut FileState> {
        self.tabs.get_mut(self.active_tab_index)
    }
    pub fn open_tab(&mut self, fs: FileState) {
        if self.tabs.len() >= 3 {
            return;
        }
        self.tabs.push(fs);
        self.active_tab_index = self.tabs.len() - 1;
    }
}

#[derive(Clone, Debug)]
pub struct BackgroundTask {
    pub id: uuid::Uuid,
    pub name: String,
    pub status: String,
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
    Commercial(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CommitInfo {
    pub hash: String,
    pub author: String,
    pub date: String,
    pub message: String,
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Staged,
    Conflict,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GitPendingChange {
    pub status: String,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum MonitorSubview {
    Overview,
    Cpu,
    Memory,
    Disk,
    Network,
    Processes,
    Applications,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ProcessColumn {
    Pid,
    Name,
    Cpu,
    Mem,
    User,
    Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClipboardOp {
    Copy,
    Cut,
}
