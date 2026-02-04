use ratatui::widgets::TableState;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use terma::compositor::engine::TilePlacement;
use terma::widgets::TextInput;

pub use crate::state::{
    AppEvent, AppMode, ClipboardOp, CommandAction, CommandItem, CommitInfo, ContextMenuAction,
    ContextMenuTarget, CurrentView, DropTarget, FileCategory, FileColumn, FileMetadata, FileState,
    GitPendingChange, GitStatus, LicenseStatus, MonitorSubview, Pane, PreviewState, ProcessColumn,
    RemoteBookmark, RemoteSession, SelectionState, SettingsSection, SettingsTarget, SidebarBounds,
    SidebarTarget, SystemState, TreeState, UndoAction, ViewPreferences, ViewStatePersistence,
};

pub struct BackgroundTask {
    pub id: uuid::Uuid,
    pub name: String,
    pub status: String,
    pub progress: f32,
}

pub struct App {
    pub running: bool,
    pub current_view: CurrentView,
    pub mode: AppMode,
    pub previous_mode: AppMode,
    pub input: TextInput,
    pub icon_mode: crate::icons::IconMode,
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
    pub show_side_panel: bool,
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
    pub single_columns: Vec<FileColumn>,
    pub split_columns: Vec<FileColumn>,
    pub monitor_subview: MonitorSubview,
    pub monitor_subview_bounds: Vec<(ratatui::layout::Rect, MonitorSubview)>,
    pub process_sort_col: ProcessColumn,
    pub process_sort_asc: bool,
    pub process_column_bounds: Vec<(ratatui::layout::Rect, ProcessColumn)>,
    pub process_selected_idx: Option<usize>,
    pub process_table_state: TableState,
    pub process_search_filter: String,
    pub undo_stack: Vec<UndoAction>,
    pub redo_stack: Vec<UndoAction>,
    pub header_icon_bounds: Vec<(ratatui::layout::Rect, String)>,
    pub tab_bounds: Vec<(ratatui::layout::Rect, usize, usize)>,
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
    pub tree_state: TreeState,
    pub saved_pane: Option<Pane>,
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
            icon_mode: crate::icons::IconMode::Nerd,
            panes: vec![Pane::new(initial_fs)],
            focused_pane_index: 0,
            is_split_mode: false,
            terminal_size: (80, 24),
            mouse_pos: (0, 0),
            system_state,
            license: LicenseStatus::FreeMode,
            sidebar_focus: false,
            sidebar_index: 0,
            starred: vec![
                dirs::home_dir(),
                dirs::desktop_dir(),
                dirs::document_dir(),
                dirs::download_dir(),
                dirs::picture_dir(),
                dirs::video_dir(),
            ]
            .into_iter()
            .flatten()
            .collect(),
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
            show_side_panel: true,
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
            single_columns: vec![
                FileColumn::Name,
                FileColumn::Size,
                FileColumn::Modified,
                FileColumn::Permissions,
            ],
            split_columns: vec![FileColumn::Name, FileColumn::Size],
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
                files: ViewPreferences {
                    show_sidebar: true,
                    is_split_mode: false,
                },
                editor: ViewPreferences {
                    show_sidebar: false,
                    is_split_mode: true,
                },
            },
            settings_index: 0,
            settings_section: SettingsSection::General,
            settings_target: SettingsTarget::SingleMode,
            settings_scroll: 0,
            open_with_index: 0,
            replace_buffer: String::new(),
            background_tasks: Vec::new(),
            tile_queue,
            tree_state: TreeState::default(),
            galaxy_state: crate::state::GalaxyState::default(),
            saved_pane: None,
        }
    }

    pub fn current_file_state(&self) -> Option<&FileState> {
        self.panes
            .get(self.focused_pane_index)
            .and_then(|p| p.current_state())
    }

    pub fn current_file_state_mut(&mut self) -> Option<&mut FileState> {
        self.panes
            .get_mut(self.focused_pane_index)
            .and_then(|p| p.current_state_mut())
    }

    pub fn sidebar_width(&self) -> u16 {
        if !self.show_sidebar {
            return 0;
        }
        (self.terminal_size.0 as f32 * (self.sidebar_width_percent as f32 / 100.0)) as u16
    }

    pub fn toggle_split(&mut self) {
        self.is_split_mode = !self.is_split_mode;
        if self.is_split_mode && self.panes.len() == 1 {
            let initial_fs = self.panes[0].tabs[0].clone();
            self.panes.push(Pane::new(initial_fs));
        } else if !self.is_split_mode && self.panes.len() > 1 {
            self.panes.truncate(1);
            self.focused_pane_index = 0;
        }
    }

    pub fn save_current_view_prefs(&mut self) {
        match self.current_view {
            CurrentView::Files => {
                self.view_prefs.files.show_sidebar = self.show_sidebar;
                self.view_prefs.files.is_split_mode = self.is_split_mode;
            }
            CurrentView::Editor => {
                self.view_prefs.editor.show_sidebar = self.show_sidebar;
                self.view_prefs.editor.is_split_mode = self.is_split_mode;
            }
            _ => {}
        }
    }

    pub fn load_view_prefs(&mut self, view: CurrentView) {
        let prefs = match view {
            CurrentView::Files => &self.view_prefs.files,
            CurrentView::Editor => &self.view_prefs.editor,
            _ => return,
        };
        self.show_sidebar = prefs.show_sidebar;
        self.apply_split_mode(prefs.is_split_mode);
    }

    pub fn apply_split_mode(&mut self, split: bool) {
        if split && self.panes.len() == 1 {
            // Restore saved pane if available, otherwise clone the current one
            if let Some(saved) = self.saved_pane.take() {
                self.panes.push(saved);
            } else {
                let initial_fs = self.panes[0].tabs[0].clone();
                self.panes.push(Pane::new(initial_fs));
            }
        } else if !split && self.panes.len() > 1 {
            // Save the second pane before removing it
            self.saved_pane = self.panes.pop();
            self.focused_pane_index = 0;
        }
        self.is_split_mode = split;
    }

    pub fn toggle_hidden(&mut self) -> usize {
        if let Some(fs) = self.current_file_state_mut() {
            fs.show_hidden = !fs.show_hidden;
        }
        self.focused_pane_index
    }

    pub fn move_to_other_pane(&mut self) {
        if self.panes.len() > 1 {
            self.focused_pane_index = if self.focused_pane_index == 0 { 1 } else { 0 };
        }
    }

    pub fn resize_sidebar(&mut self, delta: i16) {
        let mut val = self.sidebar_width_percent as i16 + delta;
        val = val.clamp(5, 50);
        self.sidebar_width_percent = val as u16;
    }

    pub fn move_up(&mut self, shift: bool) {
        if let Some(fs) = self.current_file_state_mut() {
            if let Some(sel) = fs.selection.selected {
                if sel > 0 {
                    let next = sel - 1;
                    fs.selection.handle_move(next, shift);
                    fs.table_state.select(fs.selection.selected);
                    if next < fs.table_state.offset() {
                        *fs.table_state.offset_mut() = next;
                    }
                }
            }
        }
    }

    pub fn move_down(&mut self, shift: bool) {
        if let Some(fs) = self.current_file_state_mut() {
            let capacity = fs.view_height.saturating_sub(3);
            if let Some(sel) = fs.selection.selected {
                if sel + 1 < fs.files.len() {
                    let next = sel + 1;
                    fs.selection.handle_move(next, shift);
                    fs.table_state.select(fs.selection.selected);
                    if next >= fs.table_state.offset() + capacity {
                        *fs.table_state.offset_mut() = next.saturating_sub(capacity - 1);
                    }
                }
            }
        }
    }

    pub fn apply_process_sort(&mut self) {
        // Implementation in modules/system.rs handles the actual sorting of the vector
    }
}

pub fn log_debug(msg: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open("debug.log")
    {
        let _ = writeln!(file, "[{}] DEBUG: {}", chrono::Utc::now(), msg);
    }
}
