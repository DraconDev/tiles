use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    widgets::{
        Block, BorderType, Borders,
    },
    Frame,
};

use crate::app::{
    App, AppMode, CurrentView,
};
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
    ) || matches!(app.mode, AppMode::Viewer))
        && app.current_view != CurrentView::Editor;

    if is_editor_mode {
        draw_editor_view(f, app);
        return;
    }

    if let AppMode::Settings = app.mode {
        pages::settings::draw_settings_modal(f, app);
        return;
    }

    if let AppMode::Hotkeys = app.mode {
        modals::draw_hotkeys_modal(f, f.area());
        return;
    }

    match app.current_view {
        CurrentView::Processes => {
            pages::monitor::draw_monitor_page(f, f.area(), app);
        }
        CurrentView::Git => {
            pages::git::draw_git_page(f, f.area(), app);
        }
        _ => {
            let sw = app.sidebar_width();
            layout::draw_global_header(f, Rect::new(0, 0, f.area().width, 1), sw, app);

            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Header
                    Constraint::Fill(1),   // Main Stage
                    Constraint::Length(2), // Footer
                ])
                .split(f.area());

            let stage_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(sw),
                    Constraint::Fill(1),
                ])
                .split(main_chunks[1]);

            if app.show_sidebar {
                panes::sidebar::draw_sidebar(f, stage_chunks[0], app);
            }

            layout::draw_main_stage(f, stage_chunks[1], app);
            layout::draw_footer(f, main_chunks[2], app);
        }
    }

    // Overlays
    if let AppMode::ContextMenu {
        x,
        y,
        ref target,
        ..
    } = app.mode
    {
        modals::draw_context_menu(f, x, y, target, app);
    }
    if matches!(app.mode, AppMode::Rename) {
        modals::draw_rename_modal(f, app);
    }
    if matches!(app.mode, AppMode::Delete) {
        modals::draw_delete_modal(f, app);
    }
    if let AppMode::DeleteFile(ref path) = app.mode {
        modals::draw_delete_file_modal(f, app, path); 
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

fn draw_editor_view(f: &mut Frame, app: &mut App) {
    use ratatui::style::Modifier;
    use ratatui::text::Span;
    use ratatui::style::Color;
    use ratatui::style::Style;
    use ratatui::text::Line;

    let (border_color, status_text) = if let AppMode::Viewer = app.mode {
        (Color::White, " Viewer ")
    } else if let Some(preview) = &app.editor_state {
        if let Some(last) = preview.last_saved {
            if last.elapsed().as_secs() < 2 {
                (Color::Green, " Saved ")
            } else {
                (Color::Yellow, " Modified ")
            }
        } else {
            (Color::White, " Clean ")
        }
    } else {
        (Color::White, " Clean ")
    };

    let mut header_left = vec![
        Span::styled(
            if let AppMode::Viewer = app.mode { " VIEWER " } else { " EDITOR " },
            Style::default().bg(border_color).fg(Color::Black).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {} ", status_text), Style::default().fg(border_color)),
    ];

    if let Some(preview) = &app.editor_state {
        header_left.push(Span::raw(" "));
        header_left.push(Span::styled(
            preview.path.to_string_lossy().to_string(),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
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

    let inner = block.inner(f.area());
    f.render_widget(block, f.area());

    if let Some(preview) = &mut app.editor_state {
        if let Some(editor) = &preview.editor {
            f.render_widget(editor, inner);
        }
    }
}
