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
use terma::widgets::TextInput;

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
    Copy(PathBuf, PathBuf),
    Delete(PathBuf),
    RemoteConnected(usize, RemoteSession), // pane_idx, session
    PreviewRequested(usize, PathBuf), // target_pane_idx, path
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
    Remotes,
    Shortcuts,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FileCategory {
    Archive,
    Image,
    Script,
    Text,
    Document,
    Audio,
    Video,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ContextMenuAction {
    Open,
    OpenNewTab,
    Edit,
    Run,
    RunTerminal,
    ExtractHere,
    NewFolder,
    NewFile,
    Cut,
    Copy,
    Paste,
    Rename,
    Duplicate,
    Compress,
    Delete,
    TerminalTab,
    TerminalWindow,
    SetColor(Option<u8>),
    Properties,
    GitStatus,
    AddToFavorites,
    RemoveFromFavorites,
    Refresh,
    SelectAll,
    ToggleHidden,
    ConnectRemote,
    DeleteRemote,
    Mount,
    Unmount,
    SetWallpaper,
    GitInit,
    SortBy(crate::app::FileColumn),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FileColumn {
    Name,
    Size,
    Modified,
    Permissions,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileMetadata {
    pub size: u64,
    pub modified: u64,
    pub created: u64,
    pub permissions: u32,
    pub is_dir: bool,
    pub is_symlink: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSession {
    pub name: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub auth_method: AuthMethod,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AuthMethod {
    Password,
    Key(PathBuf),
}

#[derive(Clone, Debug)]
pub struct SystemData {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub uptime: u64,
    pub memory_total: u64,
    pub memory_used: u64,
    pub cpu_usage: f32,
    pub disks: Vec<DiskInfo>,
}

#[derive(Clone, Debug)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total: u64,
    pub used: u64,
    pub device: String,
    pub is_mounted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppMode {
    Normal,
    CommandPalette,
    Settings,
    ContextMenu {
        x: u16,
        y: u16,
        target: ContextMenuTarget,
        actions: Vec<ContextMenuAction>,
    },
    NewFile,
    NewFolder,
    Rename,
    Delete,
    ImportServers,
    Properties,
    AddRemote,
    Highlight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SidebarTarget {
    Favorite(usize),
    Remote(usize),
    Storage(usize),
    Header(&'static str),
}

#[derive(Clone, Debug)]
pub struct SidebarBound {
    pub y: u16,
    pub index: usize,
    pub target: crate::main::SidebarTarget,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DropTarget {
    Favorites,
    RemotesHeader,
    ImportServers,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SettingsTarget {
    SingleMode,
    SplitMode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistentState {
    pub panes: Vec<PaneState>,
    pub focused_pane_index: usize,
    pub starred: Vec<PathBuf>,
    pub remote_bookmarks: Vec<RemoteSession>,
    pub current_view: CurrentView,
    pub window_size: (u16, u16),
    pub path_colors: HashMap<PathBuf, u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaneState {
    pub tabs: Vec<FileState>,
    pub active_tab_index: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileState {
    pub current_path: PathBuf,
    pub files: Vec<PathBuf>,
    pub metadata: HashMap<PathBuf, FileMetadata>,
    pub git_status: HashMap<PathBuf, String>,
    pub git_branch: Option<String>,
    pub selected_index: Option<usize>,
    pub multi_select: HashSet<usize>,
    pub selection_anchor: Option<usize>,
    pub search_filter: String,
    #[serde(skip)]
    pub table_state: TableState,
    pub history: Vec<PathBuf>,
    pub history_index: usize,
    pub remote_session: Option<RemoteSession>,
    pub view_height: usize,
    pub local_count: usize,
}

pub struct App {
    pub running: bool,
    pub panes: Vec<PaneState>,
    pub focused_pane_index: usize,
    pub terminal_size: (u16, u16),
    pub mode: AppMode,
    pub input: TextInput,
    pub command_index: usize,
    pub filtered_commands: Vec<CommandItem>,
    pub starred: Vec<PathBuf>,
    pub remote_bookmarks: Vec<RemoteSession>,
    pub system_state: SystemData,
    pub current_view: CurrentView,
    pub sidebar_width_percent: u16,
    pub show_sidebar: bool,
    pub sidebar_focus: bool,
    pub sidebar_index: usize,
    pub sidebar_bounds: Vec<SidebarBound>,
    pub is_resizing_sidebar: bool,
    pub is_dragging: bool,
    pub drag_start_pos: Option<(u16, u16)>,
    pub drag_source: Option<PathBuf>,
    pub hovered_drop_target: Option<DropTarget>,
    pub mouse_pos: (u16, u16),
    pub mouse_last_click: std::time::Instant,
    pub mouse_click_pos: (u16, u16),
    pub tab_bounds: Vec<(Rect, usize, usize)>,
    pub rename_selected: bool,
    pub settings_section: SettingsSection,
    pub settings_target: SettingsTarget,
    pub column_settings_single: Vec<FileColumn>,
    pub column_settings_split: Vec<FileColumn>,
    pub icon_mode: IconMode,
    pub confirm_delete: bool,
    pub default_show_hidden: bool,
    pub path_colors: HashMap<PathBuf, u8>,
    pub preferred_terminal: Option<String>,
    pub clipboard: Option<ClipboardItem>,
    pub ignore_resize_until: Option<std::time::Instant>,
}

#[derive(Clone, Debug)]
pub struct CommandItem {
    pub key: String,
    pub desc: String,
    pub action: CommandAction,
}

#[derive(Clone, Debug)]
pub enum CommandAction {
    Quit,
    ToggleZoom,
    SwitchView(CurrentView),
    AddRemote,
    ImportServers,
    ConnectToRemote(usize),
}

#[derive(Clone, Debug)]
pub enum ClipboardItem {
    Copy(Vec<PathBuf>),
    Cut(Vec<PathBuf>),
}

impl App {
    pub fn new() -> Self {
        let mut app = Self {
            running: true,
            panes: vec![
                PaneState {
                    tabs: vec![FileState::default()],
                    active_tab_index: 0,
                },
                PaneState {
                    tabs: vec![FileState::default()],
                    active_tab_index: 0,
                },
            ],
            focused_pane_index: 0,
            terminal_size: (0, 0),
            mode: AppMode::Normal,
            input: TextInput::new("> ".to_string()),
            command_index: 0,
            filtered_commands: Vec::new(),
            starred: Vec::new(),
            remote_bookmarks: Vec::new(),
            system_state: SystemData::default(),
            current_view: CurrentView::Files,
            sidebar_width_percent: 20,
            show_sidebar: true,
            sidebar_focus: false,
            sidebar_index: 0,
            sidebar_bounds: Vec::new(),
            is_resizing_sidebar: false,
            is_dragging: false,
            drag_start_pos: None,
            drag_source: None,
            hovered_drop_target: None,
            mouse_pos: (0, 0),
            mouse_last_click: std::time::Instant::now(),
            mouse_click_pos: (0, 0),
            tab_bounds: Vec::new(),
            rename_selected: false,
            settings_section: SettingsSection::Columns,
            settings_target: SettingsTarget::SingleMode,
            column_settings_single: vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified],
            column_settings_split: vec![FileColumn::Name, FileColumn::Size],
            icon_mode: IconMode::Nerd,
            confirm_delete: true,
            default_show_hidden: false,
            path_colors: HashMap::new(),
            preferred_terminal: None,
            clipboard: None,
            ignore_resize_until: None,
        };

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        app.panes[0].tabs[0].current_path = PathBuf::from(&home);
        app.panes[0].tabs[0].history = vec![PathBuf::from(&home)];
        app.panes[1].tabs[0].current_path = PathBuf::from(&home);
        app.panes[1].tabs[0].history = vec![PathBuf::from(&home)];

        app
    }

    pub fn current_file_state(&self) -> Option<&FileState> {
        self.panes.get(self.focused_pane_index).and_then(|p| p.tabs.get(p.active_tab_index))
    }

    pub fn current_file_state_mut(&mut self) -> Option<&mut FileState> {
        let idx = self.focused_pane_index;
        self.panes.get_mut(idx).and_then(|p| {
            let t_idx = p.active_tab_index;
            p.tabs.get_mut(t_idx)
        })
    }

    pub fn toggle_split(&mut self) {
        if self.panes.len() > 1 {
            self.panes.truncate(1);
            self.focused_pane_index = 0;
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            self.panes.push(PaneState {
                tabs: vec![FileState {
                    current_path: PathBuf::from(&home),
                    history: vec![PathBuf::from(&home)],
                    ..FileState::default()
                }],
                active_tab_index: 0,
            });
        }
    }

    pub fn move_left(&mut self) {
        if self.sidebar_focus { return; }
        if self.focused_pane_index > 0 {
            self.focused_pane_index -= 1;
        } else if self.show_sidebar {
            self.sidebar_focus = true;
        }
    }

    pub fn move_right(&mut self) {
        if self.sidebar_focus {
            self.sidebar_focus = false;
            self.focused_pane_index = 0;
        } else if self.focused_pane_index + 1 < self.panes.len() {
            self.focused_pane_index += 1;
        }
    }

    pub fn move_up(&mut self, shift: bool) {
        if self.sidebar_focus {
            if self.sidebar_index > 0 { self.sidebar_index -= 1; }
        } else if let Some(fs) = self.current_file_state_mut() {
            let current = fs.selected_index.unwrap_or(0);
            if current > 0 {
                fs.selected_index = Some(current - 1);
                fs.table_state.select(Some(current - 1));
                if shift {
                    let anchor = fs.selection_anchor.unwrap_or(current);
                    fs.selection_anchor = Some(anchor);
                    fs.multi_select.clear();
                    let start = std::cmp::min(anchor, current - 1);
                    let end = std::cmp::max(anchor, current - 1);
                    for i in start..=end { fs.multi_select.insert(i); }
                } else {
                    fs.multi_select.clear();
                    fs.selection_anchor = Some(current - 1);
                }
            }
        }
    }

    pub fn move_down(&mut self, shift: bool) {
        if self.sidebar_focus {
            if self.sidebar_index + 1 < self.sidebar_bounds.len() { self.sidebar_index += 1; }
        } else if let Some(fs) = self.current_file_state_mut() {
            let current = fs.selected_index.unwrap_or(0);
            if current + 1 < fs.files.len() {
                fs.selected_index = Some(current + 1);
                fs.table_state.select(Some(current + 1));
                 if shift {
                    let anchor = fs.selection_anchor.unwrap_or(current);
                    fs.selection_anchor = Some(anchor);
                    fs.multi_select.clear();
                    let start = std::cmp::min(anchor, current + 1);
                    let end = std::cmp::max(anchor, current + 1);
                    for i in start..=end { fs.multi_select.insert(i); }
                } else {
                    fs.multi_select.clear();
                    fs.selection_anchor = Some(current + 1);
                }
            }
        }
    }

    pub fn sidebar_width(&self) -> u16 {
        if !self.show_sidebar { return 0; }
        (self.terminal_size.0 as f32 * (self.sidebar_width_percent as f32 / 100.0)) as u16
    }

    pub fn toggle_column(&mut self, col: FileColumn) {
        let cols = if self.panes.len() > 1 { &mut self.column_settings_split } else { &mut self.column_settings_single };
        if let Some(pos) = cols.iter().position(|c| c == &col) {
            if cols.len() > 1 { cols.remove(pos); }
        } else {
            cols.push(col);
        }
    }

    pub fn toggle_hidden(&mut self) -> usize {
        if let Some(fs) = self.current_file_state_mut() {
            // This is just a toggle, the actual filtering happens in update_files
            // But we need to signal it. We'll use a hidden field in FileState.
            // For now, let's just toggle a global or per-pane hidden flag.
        }
        self.focused_pane_index
    }

    pub fn toggle_zoom(&mut self) {
        // Implement zoom/maximize pane logic
    }

    pub fn move_to_other_pane(&mut self) {
        if self.panes.len() > 1 {
            self.focused_pane_index = if self.focused_pane_index == 0 { 1 } else { 0 };
        }
    }

    pub fn copy_to_other_pane(&mut self) {
        // Implement copy selection to other pane logic
    }

    pub fn import_servers(&mut self, _path: PathBuf) -> std::io::Result<()> {
        // Implement import servers from toml logic
        Ok(())
    }
}

impl PaneState {
    pub fn current_state(&self) -> Option<&FileState> {
        self.tabs.get(self.active_tab_index)
    }

    pub fn current_state_mut(&mut self) -> Option<&mut FileState> {
        self.tabs.get_mut(self.active_tab_index)
    }

    pub fn open_tab(&mut self, state: FileState) {
        self.tabs.push(state);
        self.active_tab_index = self.tabs.len() - 1;
    }
}

impl Default for FileState {
    fn default() -> Self {
        Self {
            current_path: PathBuf::from("."),
            files: Vec::new(),
            metadata: HashMap::new(),
            git_status: HashMap::new(),
            git_branch: None,
            selected_index: Some(0),
            multi_select: HashSet::new(),
            selection_anchor: None,
            search_filter: String::new(),
            table_state: TableState::default(),
            history: Vec::new(),
            history_index: 0,
            remote_session: None,
            view_height: 0,
            local_count: 0,
        }
    }
}

impl SystemData {
    pub fn default() -> Self {
        Self {
            hostname: String::new(),
            os: String::new(),
            kernel: String::new(),
            uptime: 0,
            memory_total: 0,
            memory_used: 0,
            cpu_usage: 0.0,
            disks: Vec::new(),
        }
    }
}