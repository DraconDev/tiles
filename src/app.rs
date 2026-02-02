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
pub use terma::system::{DiskInfo, ProcessInfo, SystemData};
pub use terma::utils::{FileCategory, FileColumn, IconMode, SelectionState};
pub use terma::widgets::context_menu::ContextMenuAction;
use terma::widgets::{TextEditor, TextInput};
use uuid::Uuid;

pub mod state {
    pub use crate::state::*;
}

pub use crate::state::{
    AppEvent, AppMode, ClipboardOp, CommandAction, CommitInfo, ContextMenuTarget, CurrentView,
    DiskInfo as StateDiskInfo, DropTarget, FileMetadata, FileState, GitStatus, LicenseStatus,
    MonitorSubview, Pane, PreviewState, ProcessColumn, RemoteBookmark, RemoteSession,
    SettingsSection, SettingsTarget, SidebarBounds, SidebarTarget, SystemState, UndoAction,
    ViewPreferences, ViewStatePersistence,
};

#[derive(Clone, Debug)]
pub struct BackgroundTask {
    pub id: Uuid,
    pub name: String,
    pub progress: f32,
    pub status: String,
}

pub struct App {
    pub running: bool,
    pub current_view: CurrentView,
    pub mode: AppMode,
    pub previous_mode: AppMode,
    pub input: TextInput,
    pub icon_mode: IconMode,

    pub panes: Vec<Pane>,
    pub focused_pane_index: usize,
    pub is_split_mode: bool,

    pub terminal_size: (u16, u16),
    pub mouse_pos: (u16, u16),
    pub system_state: SystemState,
    pub license: LicenseStatus,
    pub sidebar_focus: bool,
    pub sidebar_index: usize,
    pub starred: Vec<PathBuf>,
    pub remote_bookmarks: Vec<RemoteBookmark>,
    pub pending_remote: RemoteBookmark,
    pub external_tools: HashMap<String, Vec<crate::config::ExternalTool>>,
    pub show_sidebar: bool,
    pub sidebar_width_percent: u16,
    pub sidebar_bounds: Vec<SidebarBounds>,
    pub drag_start_pos: Option<(u16, u16)>,
    pub drag_source: Option<PathBuf>,
    pub is_dragging: bool,
    pub hovered_drop_target: Option<DropTarget>,
    pub last_action_msg: Option<(String, std::time::Instant)>,
    pub folder_selections: HashMap<PathBuf, usize>,
    pub path_colors: HashMap<PathBuf, u8>,
    pub confirm_delete: bool,
    pub smart_date: bool,
    pub semantic_coloring: bool,
    pub auto_save: bool,
    pub default_show_hidden: bool,
    pub monitor_subview: MonitorSubview,
    pub monitor_subview_bounds: Vec<(Rect, MonitorSubview)>,
    pub process_sort_col: ProcessColumn,
    pub process_sort_asc: bool,
    pub process_column_bounds: Vec<(Rect, ProcessColumn)>,
    pub process_selected_idx: Option<usize>,
    pub process_table_state: TableState,
    pub process_search_filter: String,
    pub undo_stack: Vec<UndoAction>,
    pub redo_stack: Vec<UndoAction>,
    pub header_icon_bounds: Vec<(Rect, String)>,
    pub tab_bounds: Vec<(Rect, usize, usize)>,
    pub hovered_header_icon: Option<String>,
    pub expanded_folders: HashSet<PathBuf>,
    pub mouse_last_click: std::time::Instant,
    pub mouse_click_pos: (u16, u16),
    pub mouse_click_count: usize,
    pub is_resizing_sidebar: bool,
    pub editor_clipboard: Option<String>,
    pub clipboard: Option<(PathBuf, ClipboardOp)>,
    pub rename_selected: bool,
    pub editor_state: Option<PreviewState>,
    pub selection_mode: bool,
    pub prevent_mouse_up_selection_cleanup: bool,
    pub input_shield_until: Option<std::time::Instant>,
    pub command_index: usize,
    pub filtered_commands: Vec<CommandItem>,
    pub view_prefs: ViewStatePersistence,
    pub settings_index: usize,
    pub settings_section: SettingsSection,
    pub settings_target: SettingsTarget,
    pub settings_scroll: u16,
    pub open_with_index: usize,
    pub replace_buffer: String,
    pub background_tasks: Vec<BackgroundTask>,
    pub tile_queue: Arc<Mutex<Vec<TilePlacement>>>,
}

#[derive(Clone, Debug)]
pub struct CommandItem {
    pub key: String,
    pub desc: String,
    pub action: CommandAction,
}

impl App {
    pub fn new(tile_queue: Arc<Mutex<Vec<TilePlacement>>>) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let initial_fs = FileState::new(
            home,
            None,
            false,
            vec![
                FileColumn::Name,
                FileColumn::Size,
                FileColumn::Modified,
                FileColumn::Permissions,
            ],
            FileColumn::Name,
            true,
        );

        let system_state = SystemState {
            last_update: std::time::Instant::now(),
            disks: Vec::new(),
            processes: Vec::new(),
            cpu_usage: 0.0,
            cpu_cores: Vec::new(),
            mem_usage: 0.0,
            total_mem: 0.0,
            swap_usage: 0.0,
            total_swap: 0.0,
            cpu_history: vec![0; 100],
            core_history: Vec::new(),
            mem_history: vec![0; 100],
            swap_history: vec![0; 100],
            net_in: 0,
            net_out: 0,
            net_in_history: vec![0; 100],
            net_out_history: vec![0; 100],
            last_net_in: 0,
            last_net_out: 0,
            uptime: 0,
            os_name: String::new(),
            os_version: String::new(),
            kernel_version: String::new(),
            hostname: String::new(),
        };

        Self {
            running: true,
            current_view: CurrentView::Files,
            mode: AppMode::Normal,
            previous_mode: AppMode::Normal,
            input: TextInput::default(),
            icon_mode: IconMode::Nerd,
            panes: vec![Pane::new(initial_fs)],
            focused_pane_index: 0,
            is_split_mode: false,
            terminal_size: (80, 24),
            mouse_pos: (0, 0),
            system_state,
            license: LicenseStatus::FreeMode,
            sidebar_focus: false,
            sidebar_index: 0,
            starred: Vec::new(),
            remote_bookmarks: Vec::new(),
            pending_remote: RemoteBookmark {
                name: String::new(),
                host: String::new(),
                user: String::new(),
                port: 22,
                last_path: PathBuf::from("/"),
                key_path: None,
            },
            external_tools: HashMap::new(),
            show_sidebar: true,
            sidebar_width_percent: 15,
            sidebar_bounds: Vec::new(),
            drag_start_pos: None,
            drag_source: None,
            is_dragging: false,
            hovered_drop_target: None,
            last_action_msg: None,
            folder_selections: HashMap::new(),
            path_colors: HashMap::new(),
            confirm_delete: true,
            smart_date: true,
            semantic_coloring: true,
            auto_save: false,
            default_show_hidden: false,
            monitor_subview: MonitorSubview::Overview,
            monitor_subview_bounds: Vec::new(),
            process_sort_col: ProcessColumn::Cpu,
            process_sort_asc: false,
            process_column_bounds: Vec::new(),
            process_selected_idx: None,
            process_table_state: TableState::default(),
            process_search_filter: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            header_icon_bounds: Vec::new(),
            tab_bounds: Vec::new(),
            hovered_header_icon: None,
            expanded_folders: HashSet::new(),
            mouse_last_click: std::time::Instant::now(),
            mouse_click_pos: (0, 0),
            mouse_click_count: 0,
            is_resizing_sidebar: false,
            editor_clipboard: None,
            clipboard: None,
            rename_selected: false,
            editor_state: None,
            selection_mode: false,
            prevent_mouse_up_selection_cleanup: false,
            input_shield_until: None,
            command_index: 0,
            filtered_commands: Vec::new(),
            view_prefs: ViewStatePersistence {
                files: ViewPreferences { show_sidebar: true, is_split_mode: false },
                editor: ViewPreferences { show_sidebar: false, is_split_mode: true },
            },
            settings_index: 0,
            settings_section: SettingsSection::General,
            settings_target: SettingsTarget::SingleMode,
            settings_scroll: 0,
            open_with_index: 0,
            replace_buffer: String::new(),
            background_tasks: Vec::new(),
            tile_queue,
        }
    }

    pub fn current_file_state(&self) -> Option<&FileState> {
        self.panes.get(self.focused_pane_index).and_then(|p| p.current_state())
    }

    pub fn current_file_state_mut(&mut self) -> Option<&mut FileState> {
        self.panes.get_mut(self.focused_pane_index).and_then(|p| p.current_state_mut())
    }

    pub fn toggle_split(&mut self) {
        let current_split = self.is_split_mode;
        self.apply_split_mode(!current_split);
    }

    pub fn apply_split_mode(&mut self, split: bool) {
        self.is_split_mode = split;
        if self.is_split_mode && self.panes.len() == 1 {
            let mut new_fs = self.panes[0].tabs[0].clone();
            new_fs.selection.clear();
            self.panes.push(Pane::new(new_fs));
        } else if !self.is_split_mode && self.panes.len() > 1 {
            self.panes.truncate(1);
            if self.focused_pane_index > 0 {
                self.focused_pane_index = 0;
            }
        }
    }

    pub fn sidebar_width(&self) -> u16 {
        if !self.show_sidebar { 0 }
        else { (self.terminal_size.0 as f32 * (self.sidebar_width_percent as f32 / 100.0)) as u16 }
    }

    pub fn resize_sidebar(&mut self, delta: i16) {
        let current_w = self.sidebar_width();
        let new_w = (current_w as i16 + delta).clamp(10, (self.terminal_size.0 / 2) as i16) as u16;
        self.sidebar_width_percent = (new_w as f32 / self.terminal_size.0 as f32 * 100.0) as u16;
    }

    pub fn toggle_hidden(&mut self) -> usize {
        let idx = self.focused_pane_index;
        if let Some(fs) = self.current_file_state_mut() {
            fs.show_hidden = !fs.show_hidden;
        }
        idx
    }

    pub fn move_to_other_pane(&mut self) {
        if self.panes.len() > 1 {
            self.focused_pane_index = if self.focused_pane_index == 0 { 1 } else { 0 };
            self.sidebar_focus = false;
        }
    }

    pub fn move_up(&mut self, shift: bool) {
        if self.sidebar_focus {
            if self.sidebar_index > 0 { self.sidebar_index -= 1; }
        } else if let Some(fs) = self.current_file_state_mut() {
            let current = fs.selection.selected.unwrap_or(0);
            if current > 0 {
                let next = current - 1;
                fs.selection.handle_move(next, shift);
                fs.table_state.select(fs.selection.selected);
                if next < fs.table_state.offset() {
                    *fs.table_state.offset_mut() = next;
                }
            }
        }
    }

    pub fn move_down(&mut self, shift: bool) {
        if self.sidebar_focus {
            if self.sidebar_index < self.sidebar_bounds.len().saturating_sub(1) { self.sidebar_index += 1; }
        } else if let Some(fs) = self.current_file_state_mut() {
            let current = fs.selection.selected.unwrap_or(0);
            if current < fs.files.len().saturating_sub(1) {
                let next = current + 1;
                fs.selection.handle_move(next, shift);
                fs.table_state.select(fs.selection.selected);
                if next >= fs.table_state.offset() + fs.view_height.saturating_sub(1) {
                    *fs.table_state.offset_mut() = next.saturating_sub(fs.view_height.saturating_sub(2));
                }
            }
        }
    }

    pub fn toggle_column(&mut self, col: FileColumn) {
        if let Some(fs) = self.current_file_state_mut() {
            if fs.columns.contains(&col) { fs.columns.retain(|c| *c != col); }
            else { fs.columns.push(col); }
        }
    }

    pub fn import_servers(&mut self, path: PathBuf) -> color_eyre::Result<()> {
        let content = std::fs::read_to_string(path)?;
        let bookmarks: Vec<RemoteBookmark> = toml::from_str(&content)?;
        for b in bookmarks {
            if !self.remote_bookmarks.iter().any(|existing| existing.host == b.host) {
                self.remote_bookmarks.push(b);
            }
        }
        Ok(())
    }

    pub fn apply_process_sort(&mut self) {
        let col = self.process_sort_col;
        let asc = self.process_sort_asc;
        match col {
            ProcessColumn::Pid => self.system_state.processes.sort_by(|a, b| if asc { a.pid.cmp(&b.pid) } else { b.pid.cmp(&a.pid) }),
            ProcessColumn::Name => self.system_state.processes.sort_by(|a, b| if asc { a.name.to_lowercase().cmp(&b.name.to_lowercase()) } else { b.name.to_lowercase().cmp(&a.name.to_lowercase()) }),
            ProcessColumn::Cpu => self.system_state.processes.sort_by(|a, b| if asc { a.cpu.partial_cmp(&b.cpu).unwrap_or(std::cmp::Ordering::Equal) } else { b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal) }),
            ProcessColumn::Mem => self.system_state.processes.sort_by(|a, b| if asc { a.mem.partial_cmp(&b.mem).unwrap_or(std::cmp::Ordering::Equal) } else { b.mem.partial_cmp(&a.mem).unwrap_or(std::cmp::Ordering::Equal) }),
            ProcessColumn::User => self.system_state.processes.sort_by(|a, b| if asc { a.user.cmp(&b.user) } else { b.user.cmp(&a.user) }),
            ProcessColumn::Status => self.system_state.processes.sort_by(|a, b| if asc { a.status.cmp(&b.status) } else { b.status.cmp(&a.status) }),
        }
    }

    pub fn save_current_view_prefs(&mut self) {
        let prefs = ViewPreferences { show_sidebar: self.show_sidebar, is_split_mode: self.is_split_mode };
        match self.current_view {
            CurrentView::Files => self.view_prefs.files = prefs,
            CurrentView::Editor => self.view_prefs.editor = prefs,
            _ => {}
        }
    }

    pub fn load_view_prefs(&mut self, target: CurrentView) {
        let (show_sidebar, is_split_mode) = match target {
            CurrentView::Files => (self.view_prefs.files.show_sidebar, self.view_prefs.files.is_split_mode),
            CurrentView::Editor => (self.view_prefs.editor.show_sidebar, self.view_prefs.editor.is_split_mode),
            _ => (self.show_sidebar, self.is_split_mode),
        };
        self.show_sidebar = show_sidebar;
        self.apply_split_mode(is_split_mode);
    }
}

pub fn log_debug(msg: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new().append(true).create(true).open("debug.log") {
        let _ = writeln!(file, "[{}] {}", chrono::Local::now(), msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_logic() {
        let mut fs = FileState::new(PathBuf::from("/"), None, false, vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified], FileColumn::Name, true);
        fs.files = (0..100).map(|i| PathBuf::from(format!("/file_{}", i))).collect();
        fs.view_height = 20;
        fs.selection.selected = Some(0);
        fs.table_state.select(Some(0));
        assert_eq!(fs.table_state.offset(), 0);
    }

    #[test]
    fn test_scroll_logic_small_files() {
        let mut fs = FileState::new(PathBuf::from("/"), None, false, vec![FileColumn::Name, FileColumn::Size, FileColumn::Modified], FileColumn::Name, true);
        fs.files = (0..10).map(|i| PathBuf::from(format!("/file_{}", i))).collect();
        fs.view_height = 20;
        assert_eq!(fs.table_state.offset(), 0);
    }

    #[test]
    fn test_view_preferences_swap() {
        let mut app = App::new(Arc::new(Mutex::new(Vec::new())));
        app.current_view = CurrentView::Files;
        app.show_sidebar = true;
        app.is_split_mode = false;
        app.save_current_view_prefs();
        app.current_view = CurrentView::Editor;
        app.show_sidebar = false;
        app.is_split_mode = true;
        app.save_current_view_prefs();
        app.load_view_prefs(CurrentView::Files);
        assert_eq!(app.show_sidebar, true);
        assert_eq!(app.is_split_mode, false);
        app.load_view_prefs(CurrentView::Editor);
        assert_eq!(app.show_sidebar, false);
        assert_eq!(app.is_split_mode, true);
    }

    #[test]
    fn test_selection_state_toggle() {
        let mut sel = SelectionState::default();
        sel.toggle(5);
        assert!(sel.multi.contains(&5));
        sel.toggle(5);
        assert!(!sel.multi.contains(&5));
    }
}