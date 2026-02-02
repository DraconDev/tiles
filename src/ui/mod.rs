use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear,
    },
    Frame,
};
use std::time::SystemTime;

use crate::app::{
    App, AppMode, CurrentView,
};
use crate::ui::theme::THEME;
use terma::widgets::HotkeyHint;

pub mod layout;
pub mod theme;
pub mod modals;
pub mod pages;
pub mod panes;

pub fn draw(f: &mut Frame, app: &mut App) {
    // Check if we are in any Editor-related mode or Viewer
    let is_editor_mode = (matches!(
        app.mode,
        AppMode::Editor
            | AppMode::EditorSearch
            | AppMode::EditorGoToLine
            | AppMode::EditorReplace
            | AppMode::Viewer
    ) || (matches!(app.mode, AppMode::Hotkeys)
        && matches!(
            app.previous_mode,
            AppMode::Editor
                | AppMode::EditorSearch
                | AppMode::EditorGoToLine
                | AppMode::EditorReplace
                | AppMode::Viewer
        ))) && app.current_view != CurrentView::Editor;

    if is_editor_mode {
        let (border_color, status_text) = if let AppMode::Viewer = app.mode {
            (Color::Red, " Read Only ")
        } else if let Some(preview) = &app.editor_state {
            if let Some(editor) = &preview.editor {
                if let Some(last) = preview.last_saved {
                    if last.elapsed().as_secs() < 2 {
                        (Color::Green, " Saved ")
                    } else if editor.modified {
                        (Color::Yellow, " Modified ")
                    } else {
                        (Color::White, " Clean ")
                    }
                } else if editor.modified {
                    (Color::Yellow, " Modified ")
                } else {
                    (Color::White, " Clean ")
                }
            } else {
                (Color::White, " Clean ")
            }
        } else {
            (Color::White, " Clean ")
        };

        let mut header_left = Vec::new();
        header_left.push(Span::styled(
            if let AppMode::Viewer = app.mode { " VIEWER " } else { " EDITOR " },
            Style::default().bg(border_color).fg(Color::Black).add_modifier(Modifier::BOLD),
        ));
        header_left.push(Span::styled(format!(" {} ", status_text), Style::default().fg(border_color)));

        match app.mode {
            AppMode::EditorSearch => {
                header_left.push(Span::styled("FIND: ", Style::default().fg(border_color).add_modifier(Modifier::BOLD)));
                header_left.push(Span::styled(&app.input.value, Style::default().fg(Color::White)));
            }
            AppMode::EditorGoToLine => {
                header_left.push(Span::styled("LINE: ", Style::default().fg(border_color).add_modifier(Modifier::BOLD)));
                header_left.push(Span::styled(&app.input.value, Style::default().fg(Color::White)));
            }
            AppMode::EditorReplace => {
                if app.replace_buffer.is_empty() {
                    header_left.push(Span::styled("REPLACE [FIND]: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)));
                    header_left.push(Span::styled(&app.input.value, Style::default().fg(Color::White)));
                } else {
                    header_left.push(Span::styled("REPLACE [WITH]: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)));
                    header_left.push(Span::styled(&app.input.value, Style::default().fg(Color::White)));
                }
            }
            AppMode::Editor | AppMode::Viewer => {
                header_left.extend(HotkeyHint::new("^F", "Find", THEME.accent_secondary));
                header_left.extend(HotkeyHint::new("^R/F2", "Replace", THEME.accent_secondary));
                header_left.extend(HotkeyHint::new("^G", "Line", THEME.accent_secondary));
            }
            _ => {}
        }

        let mut header_right = Vec::new();
        header_right.extend(HotkeyHint::new("Esc", "Back", Color::Red));
        header_right.extend(HotkeyHint::new("^Q", "Quit", Color::Red));

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title_top(Line::from(header_left))
            .title_top(Line::from(header_right).alignment(ratatui::layout::Alignment::Right))
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(Color::Rgb(0, 0, 0)));

        f.render_widget(block.clone(), f.area());

        let inner_area = block.inner(f.area());
        // Fix for line number border overlap: add 1 column of padding on left
        let inner_area = ratatui::layout::Rect {
            x: inner_area.x + 1,
            width: inner_area.width.saturating_sub(1),
            ..inner_area
        };

        if let Some(preview) = &app.editor_state {
            if let Some(editor) = &preview.editor {
                f.render_widget(editor, inner_area);
            }
        }
    } else if matches!(app.mode, AppMode::Settings) {
        f.render_widget(Block::default().style(Style::default().bg(Color::Black)), f.area());
        pages::settings::draw_settings_modal(f, app);
    } else if matches!(app.current_view, CurrentView::Processes | CurrentView::Git) {
        f.render_widget(Block::default().style(Style::default().bg(Color::Black)), f.area());
        match app.current_view {
            CurrentView::Processes => pages::monitor::draw_monitor_page(f, f.area(), app),
            CurrentView::Git => pages::git::draw_git_page(f, f.area(), app),
            _ => {}
        }
    } else {
        // Normal File Manager Background
        f.render_widget(
            Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0))),
            f.area(),
        );

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Fill(1),
                Constraint::Length(2),
            ])
            .split(f.area());

        let workspace_constraints = if app.show_sidebar {
            [Constraint::Length(app.sidebar_width()), Constraint::Fill(1)]
        } else {
            [Constraint::Length(0), Constraint::Fill(1)]
        };

        let workspace = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(workspace_constraints)
            .split(chunks[1]);

        layout::draw_global_header(f, chunks[0], workspace[0].width, app);
        
        if app.show_sidebar {
            panes::sidebar::draw_sidebar(f, workspace[0], app);
        }

        layout::draw_main_stage(f, workspace[1], app);

        layout::draw_footer(f, chunks[2], app);
    }

    // --- OVERLAYS ---
    if let AppMode::Hotkeys = app.mode {
        modals::draw_hotkeys_modal(f, f.area());
    }
    if matches!(app.mode, AppMode::ContextMenu { .. }) {
        if let AppMode::ContextMenu { x, y, ref target, .. } = app.mode {
            modals::draw_context_menu(f, x, y, target, app);
        }
    }
    if matches!(app.mode, AppMode::Highlight) {
        modals::draw_highlight_modal(f, app);
    }
    if matches!(app.mode, AppMode::Rename) {
        modals::draw_rename_modal(f, app);
    }
    if matches!(app.mode, AppMode::Delete | AppMode::DeleteFile(_)) {
        modals::draw_delete_modal(f, app);
    }
    if matches!(app.mode, AppMode::Properties) {
        modals::draw_properties_modal(f, app);
    }
    if matches!(app.mode, AppMode::NewFolder) {
        modals::draw_new_folder_modal(f, app);
    }
    if matches!(app.mode, AppMode::NewFile) {
        modals::draw_new_file_modal(f, app);
    }
    if matches!(app.mode, AppMode::CommandPalette) {
        modals::draw_command_palette(f, app);
    }
    if matches!(app.mode, AppMode::AddRemote(_)) {
        modals::draw_add_remote_modal(f, app);
    }
    if matches!(app.mode, AppMode::ImportServers) {
        modals::draw_import_servers_modal(f, app);
    }
    if let AppMode::OpenWith(ref path) = app.mode {
        modals::draw_open_with_modal(f, app, path);
    }
    if let AppMode::DragDropMenu {
        ref sources,
        ref target,
    } = app.mode
    {
        modals::draw_drag_drop_modal(f, app, sources, target);
    }
}

pub fn format_modified_time(time: SystemTime) -> String {
    use chrono::{DateTime, Local};
    let dt: DateTime<Local> = time.into();
    let now = Local::now();

    if dt.date_naive() == now.date_naive() {
        dt.format("%H:%M:%S").to_string()
    } else {
        dt.format("%Y-%m-%d").to_string()
    }
}