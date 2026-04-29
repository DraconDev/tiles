use std::path::PathBuf;
use crate::app::{App, AppEvent, AppMode, ContextMenuAction, ContextMenuTarget, SettingsSection};
use crate::state::IconMode;
use dracon_terminal_engine::contracts::{
    InputEvent as Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind,
};
use serde::Deserialize;
use tokio::sync::mpsc;

const STYLE_PRESET_COUNT: usize = 6;
const STYLE_COLOR_FIELD_COUNT: usize = 6;
const STYLE_COLOR_START_INDEX: usize = 1 + STYLE_PRESET_COUNT;
const STYLE_MAX_INDEX: usize = STYLE_COLOR_START_INDEX + STYLE_COLOR_FIELD_COUNT - 1;

pub fn handle_modal_events(evt: &Event, app: &mut App, event_tx: &mpsc::Sender<AppEvent>) -> bool {
    match evt {
        Event::Key(key) => handle_modal_keys(key, app, event_tx, evt),
        Event::Mouse(me) => handle_modal_mouse(me, app, event_tx),
        _ => false,
    }
}

fn cycle_preview_max_mb(current: u16) -> u16 {
    match current {
        5 => 10,
        10 => 20,
        20 => 50,
        50 => 100,
        _ => 5,
    }
}

fn open_style_color_input(app: &mut App) {
    if app.settings_index < STYLE_COLOR_START_INDEX {
        return;
    }
    let style = crate::ui::theme::style_settings();
    let color = style_field_color(app.settings_index, &style);
    app.input.value = format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b);
    app.input.cursor_position = app.input.value.len();
    app.mode = AppMode::StyleColorInput;
}

fn style_preset_for_index(index: usize) -> Option<crate::ui::theme::ThemeStyle> {
    match index {
        1 => Some(crate::ui::theme::ThemeStyle::preset_warm()),
        2 => Some(crate::ui::theme::ThemeStyle::preset_cool()),
        3 => Some(crate::ui::theme::ThemeStyle::preset_forest()),
        4 => Some(crate::ui::theme::ThemeStyle::preset_sunset()),
        5 => Some(crate::ui::theme::ThemeStyle::preset_mono()),
        6 => Some(crate::ui::theme::ThemeStyle::preset_legacy_red()),
        _ => None,
    }
}

fn style_field_name(index: usize) -> &'static str {
    let idx = index.saturating_sub(STYLE_COLOR_START_INDEX);
    match idx {
        0 => "accent_primary",
        1 => "accent_secondary",
        2 => "selection_bg",
        3 => "border_active",
        4 => "border_inactive",
        5 => "header_fg",
        _ => "accent_primary",
    }
}

fn style_field_color(
    index: usize,
    style: &crate::ui::theme::ThemeStyle,
) -> crate::ui::theme::RgbColor {
    let idx = index.saturating_sub(STYLE_COLOR_START_INDEX);
    match idx {
        0 => style.accent_primary,
        1 => style.accent_secondary,
        2 => style.selection_bg,
        3 => style.border_active,
        4 => style.border_inactive,
        5 => style.header_fg,
        _ => style.accent_primary,
    }
}

fn set_style_field_color(
    index: usize,
    style: &mut crate::ui::theme::ThemeStyle,
    color: crate::ui::theme::RgbColor,
) {
    let idx = index.saturating_sub(STYLE_COLOR_START_INDEX);
    match idx {
        0 => style.accent_primary = color,
        1 => style.accent_secondary = color,
        2 => style.selection_bg = color,
        3 => style.border_active = color,
        4 => style.border_inactive = color,
        5 => style.header_fg = color,
        _ => {}
    }
}

fn parse_style_color_input(input: &str) -> Option<crate::ui::theme::RgbColor> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let hex = trimmed.trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        return Some(crate::ui::theme::RgbColor::new(r, g, b));
    }

    let parts: Vec<&str> = trimmed.split(',').map(|p| p.trim()).collect();
    if parts.len() == 3 {
        let r = parts[0].parse::<u8>().ok()?;
        let g = parts[1].parse::<u8>().ok()?;
        let b = parts[2].parse::<u8>().ok()?;
        return Some(crate::ui::theme::RgbColor::new(r, g, b));
    }

    None
}

fn handle_style_color_input_keys(key: &dracon_terminal_engine::contracts::KeyEvent, app: &mut App) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Settings;
            app.input.clear();
            true
        }
        KeyCode::Enter => {
            if let Some(color) = parse_style_color_input(&app.input.value) {
                let mut style = crate::ui::theme::style_settings();
                set_style_field_color(app.settings_index, &mut style, color);
                crate::ui::theme::set_style_settings(style);
                crate::config::save_state_quiet(app);
                app.mode = AppMode::Settings;
                app.input.clear();
            } else {
                app.last_action_msg = Some((
                    format!(
                        "Invalid color for {}. Use #RRGGBB or R,G,B",
                        style_field_name(app.settings_index)
                    ),
                    std::time::Instant::now(),
                ));
            }
            true
        }
        _ => app
            .input
            .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(&Event::Key(*key))),
    }
}

fn reset_all_settings_to_defaults(app: &mut App) {
    app.confirm_delete = true;
    app.smart_date = true;
    app.semantic_coloring = true;
    app.auto_save = true;
    app.default_show_hidden = false;
    app.preview_max_mb = 20;
    app.icon_mode = crate::state::IconMode::Nerd;
    app.show_sidebar = true;
    app.show_side_panel = true;
    app.show_main_stage = true;
    app.sidebar_width_percent = 15;
    app.single_columns = vec![
        crate::app::FileColumn::Name,
        crate::app::FileColumn::Size,
        crate::app::FileColumn::Modified,
        crate::app::FileColumn::Permissions,
    ];
    app.split_columns = vec![crate::app::FileColumn::Name, crate::app::FileColumn::Size];
    app.view_prefs.files.show_sidebar = true;
    app.view_prefs.files.is_split_mode = false;
    app.view_prefs.editor.show_sidebar = false;
    app.view_prefs.editor.is_split_mode = false;
    app.settings_section = SettingsSection::General;
    app.settings_index = 0;
    crate::ui::theme::set_style_settings(crate::ui::theme::ThemeStyle::preset_legacy_red());

    for pane in &mut app.panes {
        for tab in &mut pane.tabs {
            tab.show_hidden = app.default_show_hidden;
        }
    }
}

fn handle_reset_settings_confirm_keys(key: &dracon_terminal_engine::contracts::KeyEvent, app: &mut App) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Settings;
            app.input.clear();
            true
        }
        KeyCode::Enter => {
            if app.input.value.trim().eq_ignore_ascii_case("RESET") {
                reset_all_settings_to_defaults(app);
                crate::config::save_state_quiet(app);
                app.last_action_msg = Some((
                    "Settings reset to defaults".to_string(),
                    std::time::Instant::now(),
                ));
                app.mode = AppMode::Settings;
            } else {
                app.last_action_msg = Some((
                    "Type RESET to confirm".to_string(),
                    std::time::Instant::now(),
                ));
            }
            app.input.clear();
            true
        }
        _ => app
            .input
            .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(&Event::Key(*key))),
    }
}

fn handle_modal_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
    evt: &Event,
) -> bool {
    let mode = app.mode.clone();
    match mode {
        AppMode::ContextMenu {
            ref actions,
            ref target,
            selected_index,
            ..
        } => handle_context_menu_keys(key, app, event_tx, actions, target, selected_index),
        AppMode::DragDropMenu {
            ref sources,
            ref target,
        } => handle_drag_drop_keys(key, app, event_tx, sources, target),
        AppMode::EditorReplace => handle_editor_replace_keys(key, app, event_tx, evt),
        AppMode::EditorSearch => handle_editor_search_keys(key, app, event_tx, evt),
        AppMode::EditorGoToLine => handle_editor_goto_keys(key, app, event_tx, evt),
        AppMode::CommandPalette => handle_command_palette_keys(key, app, event_tx, evt),
        AppMode::AddRemote(idx) => handle_add_remote_keys(key, app, event_tx, idx, evt),
        AppMode::ImportServers => handle_import_servers_keys(key, app, event_tx, evt),
        AppMode::Highlight => handle_highlight_keys(key, app),
        AppMode::StyleColorInput => handle_style_color_input_keys(key, app),
        AppMode::ResetSettingsConfirm => handle_reset_settings_confirm_keys(key, app),
        AppMode::NewFile
        | AppMode::NewFolder
        | AppMode::Rename
        | AppMode::Delete
        | AppMode::DeleteFile(_) => handle_input_modals_keys(key, app, event_tx),
        AppMode::PathInput => handle_path_input_keys(key, app, event_tx),
        AppMode::SaveAs(_) => handle_save_as_keys(key, app, event_tx),
        AppMode::Header(idx) => handle_header_keys(key, app, event_tx, idx),
        AppMode::Hotkeys => {
            if let KeyCode::Esc | KeyCode::Enter | KeyCode::F(1) = key.code {
                app.mode = app.previous_mode.clone();
                true
            } else {
                true
            }
        }
        AppMode::Settings => handle_settings_keys(key, app, event_tx),
        AppMode::Properties => handle_properties_keys(key, app),
        AppMode::Search => handle_search_keys(key, app, event_tx),
        AppMode::OpenWith(path) => {
            match key.code {
                KeyCode::Esc => {
                    app.mode = AppMode::Normal;
                    true
                }
                KeyCode::Enter => {
                    if !app.input.value.is_empty() {
                        let _ = event_tx.try_send(AppEvent::SpawnDetached {
                            cmd: app.input.value.clone(),
                            args: vec![path.to_string_lossy().to_string()],
                        });
                    }
                    app.mode = AppMode::Normal;
                    app.input.clear();
                    true
                }
                _ => {
                    let res = app.input.handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt));
                    if app.input.value.is_empty() && res {
                        app.mode = AppMode::Normal;
                    }
                    res
                }
            }
        }
        _ => false,
    }
}

fn handle_search_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            if let Some(fs) = app.current_file_state_mut() {
                fs.search_filter.clear();
                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
            }
            app.mode = AppMode::Normal;
            app.input.clear();
            true
        }
        KeyCode::Enter => {
            let query = app.input.value.clone();
            if !query.is_empty() {
                if let Some(fs) = app.current_file_state_mut() {
                    fs.search_filter = query;
                    // Trigger refresh or filtering logic if needed
                    // Usually search filter is live, but if this is a modal, it applies on Enter?
                    // file_manager.rs uses fs.search_filter for live filtering.
                    // AppMode::Search is likely the "Find" modal mentioned in Ctrl+F.
                    // If we are in "Search" mode, we are typing into app.input.
                    // We should likely update fs.search_filter live OR on Enter.
                    // For now, let's update on Enter.
                }
                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
            }
            app.mode = AppMode::Normal;
            app.input.clear();
            true
        }
        _ => {
            // Live Update
            let handled = app
                .input
                .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(&Event::Key(*key)));
            if handled {
                let filter = app.input.value.clone();
                if let Some(fs) = app.current_file_state_mut() {
                    fs.search_filter = filter;
                }
                let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
            }
            handled
        }
    }
}

fn handle_path_input_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.input.clear();
            app.input.style = ratatui::style::Style::default().fg(ratatui::style::Color::White);
            app.input.cursor_style = ratatui::style::Style::default()
                .bg(ratatui::style::Color::White)
                .fg(ratatui::style::Color::Black);
            true
        }
        KeyCode::Enter => {
            match crate::event_helpers::submit_path_input(app, event_tx) {
                Ok(()) => {
                    app.mode = AppMode::Normal;
                    app.input.clear();
                    app.input.style =
                        ratatui::style::Style::default().fg(ratatui::style::Color::White);
                    app.input.cursor_style = ratatui::style::Style::default()
                        .bg(ratatui::style::Color::White)
                        .fg(ratatui::style::Color::Black);
                }
                Err(err) => {
                    app.last_action_msg = Some((err, std::time::Instant::now()));
                }
            }
            true
        }
        KeyCode::Char('c') | KeyCode::Char('C')
            if key.modifiers.contains(KeyModifiers::CONTROL) =>
        {
            match crate::event_helpers::copy_text_to_clipboard(&app.input.value) {
                Ok(()) => {
                    app.last_action_msg = Some((
                        "Copied current path to clipboard".to_string(),
                        std::time::Instant::now(),
                    ));
                }
                Err(err) => {
                    app.last_action_msg = Some((
                        format!("Clipboard failed: {}", err),
                        std::time::Instant::now(),
                    ));
                }
            }
            true
        }
        _ => app
            .input
            .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(&Event::Key(*key))),
    }
}

fn handle_save_as_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.input.clear();
            true
        }
        KeyCode::Enter => {
            let input = app.input.value.trim().to_string();
            if input.is_empty() {
                app.last_action_msg = Some(("Path is empty".to_string(), std::time::Instant::now()));
                return true;
            }
            if let AppMode::SaveAs(original_path) = app.mode.clone() {
                let target = if input.starts_with('/') {
                    PathBuf::from(&input)
                } else if let Some(parent) = original_path.parent() {
                    parent.join(&input)
                } else {
                    PathBuf::from(&input)
                };
                let content = if let Some(pane) = app.panes.get(app.focused_pane_index) {
                    pane.current_state().and_then(|fs| {
                        fs.preview.as_ref().and_then(|p| {
                            p.editor.as_ref().map(|e| e.get_content())
                        })
                    })
                } else {
                    None
                };
                if let Some(content) = content {
                    let _ = event_tx.try_send(AppEvent::SaveFile(target.clone(), content));
                    app.last_action_msg = Some((
                        format!("Saved as: {}", target.file_name().unwrap_or_default().to_string_lossy()),
                        std::time::Instant::now(),
                    ));
                } else {
                    app.last_action_msg = Some(("No content to save".to_string(), std::time::Instant::now()));
                }
            }
            app.mode = AppMode::Normal;
            app.input.clear();
            true
        }
        _ => app
            .input
            .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(&Event::Key(*key))),
    }
}

fn handle_properties_keys(key: &dracon_terminal_engine::contracts::KeyEvent, app: &mut App) -> bool {
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
            app.mode = AppMode::Normal;
            true
        }
        _ => true,
    }
}

fn handle_context_menu_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
    actions: &[ContextMenuAction],
    target: &ContextMenuTarget,
    selected_index: Option<usize>,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            true
        }
        KeyCode::Up => {
            let mut new_idx = match selected_index {
                Some(idx) => {
                    if idx > 0 {
                        idx - 1
                    } else {
                        actions.len().saturating_sub(1)
                    }
                }
                None => actions.len().saturating_sub(1),
            };
            if let Some(ContextMenuAction::Separator) = actions.get(new_idx) {
                new_idx = new_idx.saturating_sub(1);
            }
            if let AppMode::ContextMenu {
                selected_index: ref mut si,
                ..
            } = app.mode
            {
                *si = Some(new_idx);
            }
            true
        }
        KeyCode::Down => {
            let mut new_idx = match selected_index {
                Some(idx) => {
                    if idx < actions.len().saturating_sub(1) {
                        idx + 1
                    } else {
                        0
                    }
                }
                None => 0,
            };
            if let Some(ContextMenuAction::Separator) = actions.get(new_idx) {
                if new_idx < actions.len().saturating_sub(1) {
                    new_idx += 1;
                }
            }
            if let AppMode::ContextMenu {
                selected_index: ref mut si,
                ..
            } = app.mode
            {
                *si = Some(new_idx);
            }
            true
        }
        KeyCode::Enter => {
            if let Some(idx) = selected_index {
                if let Some(action) = actions.get(idx) {
                    if *action != ContextMenuAction::Separator {
                        let action = action.clone();
                        let target = target.clone();
                        let prev_mode = app.mode.clone();
                        crate::event_helpers::handle_context_menu_action(
                            &action,
                            &target,
                            app,
                            event_tx.clone(),
                        );
                        if matches!(prev_mode, AppMode::ContextMenu { .. }) {
                            if !matches!(app.mode, AppMode::NewFile | AppMode::NewFolder | AppMode::Rename | AppMode::Delete | AppMode::DeleteFile(_)) {
                                app.mode = AppMode::Normal;
                            }
                        }
                    }
                }
            }
            true
        }
        _ => true,
    }
}

fn handle_drag_drop_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
    sources: &[std::path::PathBuf],
    target: &std::path::Path,
) -> bool {
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C') => {
            for source in sources {
                let dest = target.join(
                    source
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("root")),
                );
                let _ = event_tx.try_send(AppEvent::Copy(source.clone(), dest));
            }
            app.mode = AppMode::Normal;
            true
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            for source in sources {
                let dest = target.join(
                    source
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("root")),
                );
                let _ = event_tx.try_send(AppEvent::Rename(source.clone(), dest));
            }
            if let Some(fs) = app.current_file_state_mut() {
                fs.selection.clear_multi();
                fs.selection.anchor = None;
            }
            app.mode = AppMode::Normal;
            true
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            for source in sources {
                let dest = target.join(
                    source
                        .file_name()
                        .unwrap_or_else(|| std::ffi::OsStr::new("root")),
                );
                let _ = event_tx.try_send(AppEvent::Symlink(source.clone(), dest));
            }
            app.mode = AppMode::Normal;
            true
        }
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            true
        }
        _ => true,
    }
}

fn handle_editor_replace_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
    evt: &Event,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = app.previous_mode.clone();
            app.input.clear();
            app.replace_buffer.clear();
            true
        }
        KeyCode::Tab | KeyCode::Enter => {
            if app.replace_buffer.is_empty() {
                app.replace_buffer = app.input.value.clone();
                app.input.clear();
                let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                    "Replace '{}' with: (Enter: next, ^Enter: all)",
                    app.replace_buffer
                )));
            } else {
                let replace_term = app.input.value.clone();
                let find_term = app.replace_buffer.clone();
                let is_all = key.modifiers.contains(KeyModifiers::CONTROL);

                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        editor.push_history();
                        if is_all {
                            editor.replace_all(&find_term, &replace_term);
                            let _ = event_tx.try_send(AppEvent::StatusMsg(format!(
                                "Replaced all '{}' with '{}'",
                                find_term, replace_term
                            )));
                        } else {
                            editor.replace_next(&find_term, &replace_term);
                            let (w, h) = app.terminal_size;
                            editor.ensure_cursor_centered(ratatui::layout::Rect::new(
                                1,
                                1,
                                w.saturating_sub(2),
                                h.saturating_sub(2),
                            ));
                        }
                    }
                }
                let focused_idx = app.focused_pane_index;
                if let Some(pane) = app.panes.get_mut(focused_idx) {
                    if let Some(fs) = pane.current_state_mut() {
                        if let Some(preview) = &mut fs.preview {
                            if let Some(editor) = &mut preview.editor {
                                editor.push_history();
                                if is_all {
                                    editor.replace_all(&find_term, &replace_term);
                                } else {
                                    editor.replace_next(&find_term, &replace_term);
                                }
                            }
                        }
                    }
                }
                app.mode = app.previous_mode.clone();
                app.input.clear();
                app.replace_buffer.clear();
            }
            true
        }
        _ => {
            let res = app
                .input
                .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt));
            if res && app.replace_buffer.is_empty() && app.input.value.is_empty() {
                app.mode = app.previous_mode.clone();
                app.input.clear();
                app.replace_buffer.clear();
            }
            res
        }
    }
}

fn handle_editor_search_keys(
    _key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
    evt: &Event,
) -> bool {
    match _key.code {
        KeyCode::Esc | KeyCode::Enter => {
            let clear_filter = |ed: &mut dracon_terminal_engine::widgets::TextEditor| ed.set_filter("");
            if let Some(preview) = &mut app.editor_state {
                if let Some(editor) = &mut preview.editor {
                    clear_filter(editor);
                }
            }
            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                if let Some(fs) = pane.current_state_mut() {
                    if let Some(preview) = &mut fs.preview {
                        if let Some(editor) = &mut preview.editor {
                            clear_filter(editor);
                        }
                    }
                }
            }
            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                if let Some(fs) = pane.current_state_mut() {
                    if let Some(preview) = &mut fs.preview {
                        if let Some(editor) = &mut preview.editor {
                            editor.handle_event(
                                &dracon_terminal_engine::input::mapping::to_runtime_event(evt),
                                ratatui::layout::Rect::new(0, 0, 100, 100),
                            );
                        }
                    }
                }
            }
            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                if let Some(fs) = pane.current_state_mut() {
                    if let Some(preview) = &mut fs.preview {
                        if let Some(editor) = &mut preview.editor {
                            editor.set_filter(&filter);
                        }
                    }
                }
            }
            app.mode = app.previous_mode.clone();
            app.input.clear();
            true
        }
        KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
            if let Some(preview) = &mut app.editor_state {
                if let Some(editor) = &mut preview.editor {
                    editor.handle_event(
                        &dracon_terminal_engine::input::mapping::to_runtime_event(evt),
                        ratatui::layout::Rect::new(
                            1,
                            1,
                            app.terminal_size.0.saturating_sub(2),
                            app.terminal_size.1.saturating_sub(2),
                        ),
                    );
                }
            }
            if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                if let Some(fs) = pane.current_state_mut() {
                    if let Some(preview) = &mut fs.preview {
                        if let Some(editor) = &mut preview.editor {
                            editor.handle_event(
                                &dracon_terminal_engine::input::mapping::to_runtime_event(evt),
                                ratatui::layout::Rect::new(0, 0, 100, 100),
                            );
                        }
                    }
                }
            }
            true
        }
        _ => {
            let handled = app
                .input
                .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt));
            if handled {
                let filter = app.input.value.clone();
                if filter.is_empty() {
                    app.mode = app.previous_mode.clone();
                    app.input.clear();
                    return true;
                }
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        editor.set_filter(&filter);
                    }
                }
                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                    if let Some(fs) = pane.current_state_mut() {
                        if let Some(preview) = &mut fs.preview {
                            if let Some(editor) = &mut preview.editor {
                                editor.set_filter(&filter);
                            }
                        }
                    }
                }
            }
            handled
        }
    }
}

fn handle_editor_goto_keys(
    _key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
    evt: &Event,
) -> bool {
    match _key.code {
        KeyCode::Esc => {
            app.mode = app.previous_mode.clone();
            app.input.clear();
            true
        }
        KeyCode::Enter => {
            if let Ok(line_num) = app.input.value.parse::<usize>() {
                let target = line_num.saturating_sub(1);
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        editor.cursor_row =
                            std::cmp::min(target, editor.lines.len().saturating_sub(1));
                        editor.cursor_col = 0;
                    }
                }
                if let Some(pane) = app.panes.get_mut(app.focused_pane_index) {
                    if let Some(fs) = pane.current_state_mut() {
                        if let Some(preview) = &mut fs.preview {
                            if let Some(editor) = &mut preview.editor {
                                editor.cursor_row =
                                    std::cmp::min(target, editor.lines.len().saturating_sub(1));
                                editor.cursor_col = 0;
                            }
                        }
                    }
                }
            }
            app.mode = app.previous_mode.clone();
            app.input.clear();
            true
        }
        _ => app
            .input
            .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt)),
    }
}

fn handle_command_palette_keys(
    _key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
    evt: &Event,
) -> bool {
    match _key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            true
        }
        KeyCode::Enter => {
            if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() {
                crate::event_helpers::execute_command(cmd.action, app, event_tx.clone());
            }
            app.mode = AppMode::Normal;
            app.input.clear();
            true
        }
        _ => {
            let handled = app
                .input
                .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt));
            if handled {
                crate::event_helpers::update_commands(app);
            }
            handled
        }
    }
}

fn handle_add_remote_keys(
    _key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
    idx: usize,
    evt: &Event,
) -> bool {
    match _key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.input.clear();
            true
        }
        KeyCode::Tab | KeyCode::Enter => {
            let val = app.input.value.clone();
            match idx {
                0 => app.pending_remote.name = val,
                1 => app.pending_remote.host = val,
                2 => app.pending_remote.user = val,
                3 => app.pending_remote.port = val.parse().unwrap_or(22),
                4 => {
                    app.pending_remote.key_path = if val.is_empty() {
                        None
                    } else {
                        Some(std::path::PathBuf::from(val))
                    }
                }
                _ => {}
            }
            if idx < 4 {
                app.mode = AppMode::AddRemote(idx + 1);
                app.input.set_value(String::new());
            } else {
                app.remote_bookmarks.push(app.pending_remote.clone());
                crate::config::save_state_quiet(app);
                app.mode = AppMode::Normal;
                app.input.clear();
            }
            true
        }
        _ => app
            .input
            .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt)),
    }
}

#[derive(Deserialize)]
struct ImportServersToml {
    servers: Vec<ImportServerEntry>,
}

#[derive(Deserialize)]
struct ImportServerEntry {
    name: String,
    host: String,
    user: String,
    #[serde(default = "default_ssh_port")]
    port: u16,
    #[serde(default)]
    key_path: Option<std::path::PathBuf>,
}

const fn default_ssh_port() -> u16 {
    22
}

fn handle_import_servers_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
    evt: &Event,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.input.clear();
            true
        }
        KeyCode::Enter => {
            let path = app.input.value.trim().to_string();
            if path.is_empty() {
                app.last_action_msg = Some((
                    "Import path is empty".to_string(),
                    std::time::Instant::now(),
                ));
                return true;
            }

            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed reading {}: {}", path, e))
                .and_then(|content| {
                    toml::from_str::<ImportServersToml>(&content)
                        .map_err(|e| format!("Invalid TOML: {}", e))
                });

            match parsed {
                Ok(data) => {
                    let mut imported = 0usize;
                    for s in data.servers {
                        let candidate = crate::state::RemoteBookmark {
                            name: s.name,
                            host: s.host,
                            user: s.user,
                            port: s.port,
                            last_path: std::path::PathBuf::from("/"),
                            key_path: s.key_path,
                        };
                        let exists = app.remote_bookmarks.iter().any(|b| {
                            b.name == candidate.name
                                && b.host == candidate.host
                                && b.user == candidate.user
                                && b.port == candidate.port
                        });
                        if !exists {
                            app.remote_bookmarks.push(candidate);
                            imported += 1;
                        }
                    }
                    crate::config::save_state_quiet(app);
                    app.last_action_msg = Some((
                        format!("Imported {} server(s)", imported),
                        std::time::Instant::now(),
                    ));
                    app.mode = AppMode::Normal;
                    app.input.clear();
                }
                Err(msg) => {
                    app.last_action_msg = Some((msg, std::time::Instant::now()));
                }
            }
            true
        }
        _ => app
            .input
            .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(evt)),
    }
}

fn handle_highlight_keys(key: &dracon_terminal_engine::contracts::KeyEvent, app: &mut App) -> bool {
    if let KeyCode::Char(c) = key.code {
        if let Some(digit) = c.to_digit(10) {
            if digit <= 6 {
                let color = if digit == 0 { None } else { Some(digit as u8) };
                if let Some(fs) = app.current_file_state() {
                    let mut paths = Vec::new();
                    if !fs.selection.is_empty() {
                        for &idx in fs.selection.multi_selected_indices() {
                            if let Some(p) = fs.files.get(idx) {
                                paths.push(p.clone());
                            }
                        }
                    } else if let Some(idx) = fs.selection.selected {
                        if let Some(p) = fs.files.get(idx) {
                            paths.push(p.clone());
                        }
                    }
                    for p in paths {
                        if let Some(col) = color {
                            app.path_colors.insert(p, col);
                        } else {
                            app.path_colors.remove(&p);
                        }
                    }
                    crate::config::save_state_quiet(app);
                }
                app.mode = AppMode::Normal;
                true
            } else {
                false
            }
        } else {
            false
        }
    } else if key.code == KeyCode::Esc {
        app.mode = AppMode::Normal;
        true
    } else {
        false
    }
}

fn handle_input_modals_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            app.input.clear();
            app.rename_selected = false;
            true
        }
        KeyCode::Enter => {
            let input = app.input.value.clone();
            if let AppMode::DeleteFile(ref path) = app.mode {
                if input.trim().to_lowercase() == "y" || !app.confirm_delete {
                    let _ = event_tx.try_send(AppEvent::Delete(path.clone()));
                    app.mode = AppMode::Normal;
                } else {
                    app.mode = AppMode::Normal;
                }
                app.input.clear();
                return true;
            }
            let mode = app.mode.clone();
            if let Some(fs) = app.current_file_state() {
                let path = fs.current_path.join(&input);
                match mode {
                    AppMode::NewFile => {
                        let _ = event_tx.try_send(AppEvent::CreateFile(path));
                    }
                    AppMode::NewFolder => {
                        let _ = event_tx.try_send(AppEvent::CreateFolder(path));
                    }
                    AppMode::Rename => {
                        if let Some(idx) = fs.selection.selected {
                            if let Some(old) = fs.files.get(idx) {
                                if let Some(parent) = old.parent() {
                                    let _ = event_tx.try_send(AppEvent::Rename(
                                        old.clone(),
                                        parent.join(&input),
                                    ));
                                } else {
                                    let _ = event_tx.try_send(AppEvent::StatusMsg(
                                        "Cannot rename root path".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    AppMode::Delete => {
                        if input.trim().to_lowercase() == "y" || input.is_empty() {
                            // Collect paths to delete
                            let mut paths = Vec::new();
                            if !fs.selection.is_empty() {
                                for &idx in fs.selection.multi_selected_indices() {
                                    if let Some(p) = fs.files.get(idx) {
                                        paths.push(p.clone());
                                    }
                                }
                            } else if let Some(idx) = fs.selection.selected {
                                if let Some(p) = fs.files.get(idx) {
                                    paths.push(p.clone());
                                }
                            }
                            for p in paths {
                                let _ = event_tx.try_send(AppEvent::Delete(p));
                            }
                        }
                    }
                    _ => {}
                }
            }
            app.mode = AppMode::Normal;
            app.input.clear();
            true
        }
        _ => app
            .input
            .handle_event(&dracon_terminal_engine::input::mapping::to_runtime_event(&Event::Key(*key))),
    }
}

fn handle_header_keys(
    _key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
    _idx: usize,
) -> bool {
    match _key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            true
        }
        KeyCode::Enter => {
            // Header icon logic
            app.mode = AppMode::Normal;
            true
        }
        KeyCode::Left => {
            if _idx > 0 {
                app.mode = AppMode::Header(_idx - 1);
            }
            true
        }
        KeyCode::Right => {
            app.mode = AppMode::Header(_idx + 1);
            true
        }
        _ => true,
    }
}

fn handle_settings_keys(
    key: &dracon_terminal_engine::contracts::KeyEvent,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    match key.code {
        KeyCode::Esc => {
            app.mode = AppMode::Normal;
            true
        }
        KeyCode::Char('1') => {
            app.settings_section = SettingsSection::Columns;
            app.settings_index = 0;
            true
        }
        KeyCode::Char('2') => {
            app.settings_section = SettingsSection::Tabs;
            app.settings_index = 0;
            true
        }
        KeyCode::Char('3') => {
            app.settings_section = SettingsSection::General;
            app.settings_index = 0;
            true
        }
        KeyCode::Char('4') => {
            app.settings_section = SettingsSection::Style;
            app.settings_index = 0;
            true
        }
        KeyCode::Char('5') => {
            app.settings_section = SettingsSection::Remotes;
            app.settings_index = 0;
            true
        }
        KeyCode::Char('6') => {
            app.settings_section = SettingsSection::Shortcuts;
            app.settings_index = 0;
            true
        }
        KeyCode::Char('r') | KeyCode::Char('R')
            if app.settings_section == SettingsSection::Style =>
        {
            crate::ui::theme::set_style_settings(crate::ui::theme::ThemeStyle::preset_legacy_red());
            crate::config::save_state_quiet(app);
            true
        }
        KeyCode::Char('e') | KeyCode::Char('E')
            if app.settings_section == SettingsSection::Style =>
        {
            if app.settings_index == 0 {
                crate::ui::theme::set_style_settings(
                    crate::ui::theme::ThemeStyle::preset_legacy_red(),
                );
                crate::config::save_state_quiet(app);
            } else if let Some(preset) = style_preset_for_index(app.settings_index) {
                crate::ui::theme::set_style_settings(preset);
                crate::config::save_state_quiet(app);
            } else {
                open_style_color_input(app);
            }
            true
        }
        KeyCode::Up => {
            app.settings_index = app.settings_index.saturating_sub(1);
            true
        }
        KeyCode::Down => {
            let max = match app.settings_section {
                SettingsSection::General => 7,
                SettingsSection::Columns => 3,
                SettingsSection::Style => STYLE_MAX_INDEX,
                _ => 0,
            };
            if app.settings_index < max {
                app.settings_index += 1;
            }
            true
        }
        KeyCode::Enter => {
            match app.settings_section {
                SettingsSection::General => {
                    match app.settings_index {
                        0 => app.default_show_hidden = !app.default_show_hidden,
                        1 => app.confirm_delete = !app.confirm_delete,
                        2 => app.smart_date = !app.smart_date,
                        3 => app.semantic_coloring = !app.semantic_coloring,
                        4 => app.auto_save = !app.auto_save,
                        5 => app.preview_max_mb = cycle_preview_max_mb(app.preview_max_mb),
                        6 => {
                            app.icon_mode = match app.icon_mode {
                                IconMode::Nerd => IconMode::Unicode,
                                IconMode::Unicode => IconMode::ASCII,
                                IconMode::ASCII => IconMode::Nerd,
                            }
                        }
                        7 => {
                            app.mode = AppMode::ResetSettingsConfirm;
                            app.input.clear();
                        }
                        _ => {}
                    }
                    if app.settings_index != 7 {
                        crate::config::save_state_quiet(app);
                    }
                }
                SettingsSection::Columns => {
                    let col = match app.settings_index {
                        0 => crate::app::FileColumn::Size,
                        1 => crate::app::FileColumn::Modified,
                        2 => crate::app::FileColumn::Created,
                        3 => crate::app::FileColumn::Permissions,
                        _ => crate::app::FileColumn::Size,
                    };
                    let target_set = match app.settings_target {
                        crate::app::SettingsTarget::SingleMode => &mut app.single_columns,
                        crate::app::SettingsTarget::SplitMode => &mut app.split_columns,
                    };
                    if let Some(pos) = target_set.iter().position(|c| c == &col) {
                        target_set.remove(pos);
                    } else {
                        target_set.push(col);
                    }
                    crate::config::save_state_quiet(app);
                }
                SettingsSection::Style => {
                    if app.settings_index == 0 {
                        crate::ui::theme::set_style_settings(
                            crate::ui::theme::ThemeStyle::preset_legacy_red(),
                        );
                        crate::config::save_state_quiet(app);
                    } else if let Some(preset) = style_preset_for_index(app.settings_index) {
                        crate::ui::theme::set_style_settings(preset);
                        crate::config::save_state_quiet(app);
                    } else {
                        open_style_color_input(app);
                    }
                }
                _ => {}
            }
            true
        }
        _ => false,
    }
}

pub fn handle_modal_mouse(
    me: &dracon_terminal_engine::contracts::MouseEvent,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    let (w, h) = app.terminal_size;
    let column = me.column;
    let row = me.row;

    // Middle-click paste for input modals
    if let MouseEventKind::Down(MouseButton::Middle) = me.kind {
        if matches!(
            app.mode,
            AppMode::Rename
                | AppMode::NewFile
                | AppMode::NewFolder
                | AppMode::AddRemote(_)
                | AppMode::ImportServers
                | AppMode::EditorSearch
                | AppMode::EditorReplace
                | AppMode::EditorGoToLine
                | AppMode::StyleColorInput
                | AppMode::ResetSettingsConfirm
        ) {
            if let Some(text) = dracon_terminal_engine::utils::get_primary_selection_text() {
                let pos = app.input.cursor_position;
                if pos >= app.input.value.len() {
                    app.input.value.push_str(&text);
                } else {
                    app.input.value.insert_str(pos, &text);
                }
                app.input.cursor_position += text.len();
                app.rename_selected = false;
                return true;
            }
        }
    }

    match app.mode.clone() {
        AppMode::Highlight => {
            if let MouseEventKind::Down(_) = me.kind {
                let area_w = 34;
                let area_h = 5;
                let area_x = (w.saturating_sub(area_w)) / 2;
                let area_y = (h.saturating_sub(area_h)) / 2;
                if column >= area_x
                    && column < area_x + area_w
                    && row >= area_y
                    && row < area_y + area_h
                {
                    let rel_x = column.saturating_sub(area_x + 3);
                    if row >= area_y + 2 && row <= area_y + 3 {
                        let colors = [1, 2, 3, 4, 5, 6, 0];
                        if let Some(&color_code) = colors.get((rel_x / 4) as usize) {
                            let color = if color_code == 0 {
                                None
                            } else {
                                Some(color_code as u8)
                            };
                            if let Some(fs) = app.current_file_state() {
                                let mut paths = Vec::new();
                                if !fs.selection.is_empty() {
                                    for &idx in fs.selection.multi_selected_indices() {
                                        if let Some(p) = fs.files.get(idx) {
                                            paths.push(p.clone());
                                        }
                                    }
                                } else if let Some(idx) = fs.selection.selected {
                                    if let Some(p) = fs.files.get(idx) {
                                        paths.push(p.clone());
                                    }
                                }
                                for p in paths {
                                    if let Some(col) = color {
                                        app.path_colors.insert(p, col);
                                    } else {
                                        app.path_colors.remove(&p);
                                    }
                                }
                                crate::config::save_state_quiet(app);
                            }
                            app.mode = AppMode::Normal;
                        }
                    }
                } else {
                    app.mode = AppMode::Normal;
                }
                return true;
            }
        }
        AppMode::ContextMenu {
            x,
            y,
            ref actions,
            ref target,
            ..
        } => {
            let (mw, mh) = (25, actions.len() as u16 + 2);
            let (mut dx, mut dy) = (x, y);
            if dx + mw > w {
                dx = w.saturating_sub(mw);
            }
            if dy + mh > h {
                dy = h.saturating_sub(mh);
            }

            if let MouseEventKind::Down(_) = me.kind {
                if column >= dx && column < dx + mw && row >= dy && row < dy + mh {
                    let rel_y = row.saturating_sub(dy + 1);
                    if row > dy && row < dy + mh - 1 {
                        if let Some(action) = actions.get(rel_y as usize) {
                            if *action != ContextMenuAction::Separator {
                                let prev_mode = app.mode.clone();
                                crate::event_helpers::handle_context_menu_action(
                                    action,
                                    target,
                                    app,
                                    event_tx.clone(),
                                );
                                if matches!(app.mode, AppMode::Normal) {
                                    // Menu was closed, check if action changed mode
                                } else if matches!(prev_mode, AppMode::ContextMenu { .. }) {
                                    // Action changed mode (like NewFile/NewFolder), keep it
                                } else {
                                    app.mode = AppMode::Normal;
                                }
                            }
                        }
                    }
                } else {
                    app.mode = AppMode::Normal;
                }
                return true;
            }
            return true; // Consume all mouse events while context menu is open
        }
        AppMode::StyleColorInput => {
            if let MouseEventKind::Down(_) = me.kind {
                let area_w = 64;
                let area_h = 9;
                let area_x = (w.saturating_sub(area_w)) / 2;
                let area_y = (h.saturating_sub(area_h)) / 2;
                let inside = column >= area_x
                    && column < area_x + area_w
                    && row >= area_y
                    && row < area_y + area_h;
                if !inside {
                    app.mode = AppMode::Settings;
                    app.input.clear();
                }
                return true;
            }
        }
        AppMode::ResetSettingsConfirm => {
            if let MouseEventKind::Down(_) = me.kind {
                let area_w = 56;
                let area_h = 12;
                let area_x = (w.saturating_sub(area_w)) / 2;
                let area_y = (h.saturating_sub(area_h)) / 2;
                let inside = column >= area_x
                    && column < area_x + area_w
                    && row >= area_y
                    && row < area_y + area_h;
                if !inside {
                    app.mode = AppMode::Settings;
                    app.input.clear();
                }
                return true;
            }
        }
        AppMode::Settings => {
            if let MouseEventKind::Down(_) = me.kind {
                if row == 0 && column >= w.saturating_sub(10) {
                    app.mode = AppMode::Normal;
                    return true;
                }
                let inner_x = 1;
                let inner_y = 1;
                if column < inner_x + 20 {
                    let rel_y = row.saturating_sub(inner_y);
                    match rel_y {
                        0 => app.settings_section = SettingsSection::Columns,
                        1 => app.settings_section = SettingsSection::Tabs,
                        2 => app.settings_section = SettingsSection::General,
                        3 => app.settings_section = SettingsSection::Style,
                        4 => app.settings_section = SettingsSection::Remotes,
                        5 => app.settings_section = SettingsSection::Shortcuts,
                        _ => {}
                    }
                    app.settings_index = 0;
                } else {
                    // Right Side Interactions
                    let rel_col = column.saturating_sub(inner_x + 20);
                    let rel_y = row.saturating_sub(inner_y + 1); // +1 assuming block top border

                    match app.settings_section {
                        SettingsSection::General => {
                            if rel_y < 8 {
                                app.settings_index = rel_y as usize;
                                match app.settings_index {
                                    0 => app.default_show_hidden = !app.default_show_hidden,
                                    1 => app.confirm_delete = !app.confirm_delete,
                                    2 => app.smart_date = !app.smart_date,
                                    3 => app.semantic_coloring = !app.semantic_coloring,
                                    4 => app.auto_save = !app.auto_save,
                                    5 => {
                                        app.preview_max_mb =
                                            cycle_preview_max_mb(app.preview_max_mb)
                                    }
                                    6 => {
                                        app.icon_mode = match app.icon_mode {
                                            IconMode::Nerd => IconMode::Unicode,
                                            IconMode::Unicode => IconMode::ASCII,
                                            IconMode::ASCII => IconMode::Nerd,
                                        }
                                    }
                                    7 => {
                                        app.mode = AppMode::ResetSettingsConfirm;
                                        app.input.clear();
                                    }
                                    _ => {}
                                }
                                if app.settings_index != 7 {
                                    crate::config::save_state_quiet(app);
                                }
                            }
                        }
                        SettingsSection::Columns => {
                            if rel_y < 3 {
                                // Toggle Mode Tabs
                                if rel_col < 15 {
                                    app.settings_target = crate::app::SettingsTarget::SingleMode;
                                } else {
                                    app.settings_target = crate::app::SettingsTarget::SplitMode;
                                }
                            } else {
                                let idx = rel_y.saturating_sub(3) as usize;
                                if idx < 4 {
                                    app.settings_index = idx;
                                    let col = match idx {
                                        0 => crate::app::FileColumn::Size,
                                        1 => crate::app::FileColumn::Modified,
                                        2 => crate::app::FileColumn::Created,
                                        3 => crate::app::FileColumn::Permissions,
                                        _ => crate::app::FileColumn::Size,
                                    };
                                    let target_set = match app.settings_target {
                                        crate::app::SettingsTarget::SingleMode => {
                                            &mut app.single_columns
                                        }
                                        crate::app::SettingsTarget::SplitMode => {
                                            &mut app.split_columns
                                        }
                                    };
                                    if let Some(pos) = target_set.iter().position(|c| c == &col) {
                                        target_set.remove(pos);
                                    } else {
                                        target_set.push(col);
                                    }
                                    crate::config::save_state_quiet(app);
                                }
                            }
                        }
                        SettingsSection::Style => {
                            if rel_y < (STYLE_MAX_INDEX as u16 + 1) {
                                app.settings_index = rel_y as usize;
                                if app.settings_index == 0 {
                                    crate::ui::theme::set_style_settings(
                                        crate::ui::theme::ThemeStyle::preset_legacy_red(),
                                    );
                                    crate::config::save_state_quiet(app);
                                } else if let Some(preset) =
                                    style_preset_for_index(app.settings_index)
                                {
                                    crate::ui::theme::set_style_settings(preset);
                                    crate::config::save_state_quiet(app);
                                } else {
                                    open_style_color_input(app);
                                }
                            }
                        }
                        _ => {}
                    }
                }
                return true;
            }
        }
        AppMode::Delete | AppMode::DeleteFile(_) => {
            if let MouseEventKind::Down(MouseButton::Left) = me.kind {
                let area_w = 40;
                let area_h = 10;
                let area_x = (w.saturating_sub(area_w)) / 2;
                let area_y = (h.saturating_sub(area_h)) / 2;

                let inner_x = area_x + 1;
                let inner_y = area_y + 1;
                let inner_h = area_h - 2;
                let button_y = inner_y + inner_h.saturating_sub(2);

                let is_hit = |bx: u16, len: u16| {
                    column >= inner_x + bx && column < inner_x + bx + len && row == button_y
                };

                if is_hit(5, 9) {
                    // Collect paths to delete
                    if let Some(fs) = app.current_file_state() {
                        let mut paths = Vec::new();
                        if !fs.selection.is_empty() {
                            for &idx in fs.selection.multi_selected_indices() {
                                if let Some(p) = fs.files.get(idx) {
                                    paths.push(p.clone());
                                }
                            }
                        } else if let Some(idx) = fs.selection.selected {
                            if let Some(p) = fs.files.get(idx) {
                                paths.push(p.clone());
                            }
                        }
                        for p in paths {
                            let _ = event_tx.try_send(AppEvent::Delete(p));
                        }
                    }
                    app.mode = AppMode::Normal;
                    app.input.clear();
                    return true;
                }

                if is_hit(25, 8) {
                    app.mode = AppMode::Normal;
                    app.input.clear();
                    return true;
                }
            }
        }
        AppMode::DragDropMenu {
            ref sources,
            ref target,
        } => {
            if let MouseEventKind::Down(MouseButton::Left) = me.kind {
                // centered_rect(60, 20, ...) uses percentages
                let area_w = w * 60 / 100;
                let area_h = h * 20 / 100;
                let area_x = (w.saturating_sub(area_w)) / 2;
                let area_y = (h.saturating_sub(area_h)) / 2;

                // Block borders take 1 cell each side
                let inner_x = area_x + 1;
                let inner_y = area_y + 1;

                let button_y_offset = if sources.len() == 1 {
                    3
                } else {
                    let display_count = std::cmp::min(sources.len(), 3);
                    let mut offset = 1 + display_count;
                    if sources.len() > 3 {
                        offset += 1;
                    }
                    offset + 2
                };
                let button_y = inner_y + button_y_offset as u16;

                let is_hit = |bx: u16, len: u16| {
                    column >= inner_x + bx && column < inner_x + bx + len && row == button_y
                };

                let sources = sources.clone();
                let target = target.to_path_buf();

                // Button layout: " [C] Copy " (10) + "  " (2) + " [M] Move " (10) + "  " (2) + " [L] Link " (10) + "  " (2) + " [Esc] Cancel " (14)
                if is_hit(0, 10) {
                    for src in &sources {
                        let dest = target.join(src.file_name().unwrap_or_default());
                        let _ = event_tx.try_send(AppEvent::Copy(src.clone(), dest));
                    }
                    app.mode = AppMode::Normal;
                    return true;
                }
                if is_hit(12, 10) {
                    for src in &sources {
                        let dest = target.join(src.file_name().unwrap_or_default());
                        let _ = event_tx.try_send(AppEvent::Rename(src.clone(), dest));
                    }
                    app.mode = AppMode::Normal;
                    return true;
                }
                if is_hit(24, 10) {
                    for src in &sources {
                        let dest = target.join(src.file_name().unwrap_or_default());
                        let _ = event_tx.try_send(AppEvent::Symlink(src.clone(), dest));
                    }
                    app.mode = AppMode::Normal;
                    return true;
                }
                if is_hit(36, 14) {
                    app.mode = AppMode::Normal;
                    return true;
                }
            }
        }
        _ => {}
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, AppMode};
    use dracon_terminal_engine::contracts::{
        KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use dracon_terminal_engine::compositor::engine::TilePlacement;
    use tokio::sync::mpsc;

    fn test_app() -> App {
        let queue: Arc<Mutex<Vec<TilePlacement>>> = Arc::new(Mutex::new(Vec::new()));
        App::new(queue)
    }

    #[test]
    fn drag_drop_link_click_emits_symlink_and_closes_modal() {
        let (tx, mut rx) = mpsc::channel(8);
        let mut app = test_app();
        app.terminal_size = (100, 40);

        let src = PathBuf::from("/tmp/src-file.txt");
        let target = PathBuf::from("/tmp/dest-dir");
        app.mode = AppMode::DragDropMenu {
            sources: vec![src.clone()],
            target: target.clone(),
        };

        let handled = handle_modal_mouse(
            &MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 46, // Link button region for 100x40 terminal.
                row: 20,
                modifiers: KeyModifiers::empty(),
            },
            &mut app,
            &tx,
        );

        assert!(handled);
        assert!(matches!(app.mode, AppMode::Normal));

        match rx.try_recv() {
            Ok(AppEvent::Symlink(from, to)) => {
                assert_eq!(from, src);
                assert_eq!(to, target.join("src-file.txt"));
            }
            other => panic!("expected Symlink event, got {:?}", other),
        }
    }

    #[test]
    fn style_preset_and_custom_color_apply() {
        let (tx, _rx) = mpsc::channel(8);
        let mut app = test_app();
        app.mode = AppMode::Settings;
        app.settings_section = SettingsSection::Style;

        // Apply Cool preset row.
        app.settings_index = 2;
        let _ = handle_settings_keys(
            &KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
            },
            &mut app,
            &tx,
        );
        let style_after_preset = crate::ui::theme::style_settings();
        assert_eq!(style_after_preset.accent_primary.r, 160);

        // Open first custom color row and apply explicit color.
        app.settings_index = STYLE_COLOR_START_INDEX;
        let _ = handle_settings_keys(
            &KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
            },
            &mut app,
            &tx,
        );
        assert!(matches!(app.mode, AppMode::StyleColorInput));
        app.input.set_value("#112233".to_string());
        let _ = handle_style_color_input_keys(
            &KeyEvent {
                code: KeyCode::Enter,
                modifiers: KeyModifiers::empty(),
                kind: KeyEventKind::Press,
            },
            &mut app,
        );
        let style_after_custom = crate::ui::theme::style_settings();
        assert_eq!(style_after_custom.accent_primary.r, 0x11);
        assert_eq!(style_after_custom.accent_primary.g, 0x22);
        assert_eq!(style_after_custom.accent_primary.b, 0x33);
    }
}
