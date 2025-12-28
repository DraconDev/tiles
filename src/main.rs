use std::{io, time::{Duration, Instant}};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind, MouseButton},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};

mod app;
mod ui;
mod modules;
mod event;
mod config;
mod license;

use std::sync::Arc;
use tokio::sync::mpsc;
use crate::app::{App, AppMode, CurrentView, CommandItem};
use crate::modules::docker::DockerModule;
use bollard::models::ContainerSummary;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let (tx, rx) = mpsc::channel(10);
    let docker_module = DockerModule::new().ok().map(Arc::new);

    if let Some(docker) = docker_module.clone() {
        tokio::spawn(async move {
            loop {
                if let Ok(containers) = docker.get_containers().await {
                    let _ = tx.send(containers).await;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    let res = run_app(&mut terminal, &mut app, rx, docker_module).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    if let Err(err) = res { println!("{err:?}"); }
    Ok(())
}

fn push_history(fs: &mut crate::app::FileState, path: std::path::PathBuf) {
    if fs.current_path == path { return; }
    fs.history.truncate(fs.history_index + 1);
    fs.history.push(path);
    fs.history_index = fs.history.len() - 1;
}

fn navigate_back(fs: &mut crate::app::FileState) {
    if fs.history_index > 0 {
        fs.history_index -= 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = 0;
        fs.search_filter.clear();
        crate::modules::files::update_files(fs);
    }
}

fn navigate_forward(fs: &mut crate::app::FileState) {
    if fs.history_index + 1 < fs.history.len() {
        fs.history_index += 1;
        fs.current_path = fs.history[fs.history_index].clone();
        fs.selected_index = 0;
        fs.search_filter.clear();
        crate::modules::files::update_files(fs);
    }
}

fn update_docker_filter(app: &mut App) {
    if let Some(fs) = app.current_file_state() {
        if let Some(path) = fs.files.get(fs.selected_index) {
            if path.is_dir() {
                if path.join("Dockerfile").exists() || path.join("docker-compose.yml").exists() || path.join("docker-compose.yaml").exists() {
                    app.docker_state.filter = Some(path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string());
                    return;
                }
            }
        }
    }
    app.docker_state.filter = None;
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>, 
    app: &mut App, 
    mut rx: mpsc::Receiver<Vec<ContainerSummary>>,
    docker_module: Option<Arc<DockerModule>>,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    while app.running {
        terminal.draw(|f| ui::draw(f, app))?;
        while let Ok(containers) = rx.try_recv() { app.docker_state.containers = containers; }
        let timeout = tick_rate.checked_sub(last_tick.elapsed()).unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            match crossterm::event::read()? {
                Event::Mouse(mouse) => {
                    let (cols, rows) = terminal.size().map(|s| (s.width, s.height)).unwrap_or((0, 0));
                    match mouse.kind {
                        MouseEventKind::Down(btn) => {
                            if let AppMode::ContextMenu(x, y) = app.mode {
                                if mouse.column >= x && mouse.column < x + 15 {
                                    match mouse.row.saturating_sub(y) as usize {
                                        0 => { if let Some(name) = app.current_file_state().and_then(|fs| fs.files.get(fs.selected_index).map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string())) { app.mode = AppMode::Rename; app.input = name; } }
                                        1 => { if let Some(fs) = app.current_file_state_mut() { if let Some(path) = fs.files.get(fs.selected_index).cloned() { if !fs.starred.insert(path.clone()) { fs.starred.remove(&path); } } } app.mode = AppMode::Normal; }
                                        2 => { app.mode = AppMode::Delete; }
                                        _ => { app.mode = AppMode::Normal; }
                                    }
                                } else { app.mode = AppMode::Normal; }
                                continue;
                            }
                            if btn == MouseButton::Right { app.mode = AppMode::ContextMenu(mouse.column, mouse.row); continue; }
                            
                            if btn == MouseButton::Middle {
                                if app.current_view == CurrentView::Files {
                                    let index = fs_mouse_index(mouse.row, app);
                                    if let Some(fs) = app.current_file_state() {
                                        if let Some(path) = fs.files.get(index).cloned() {
                                            if path.is_dir() {
                                                let mut new_fs = crate::app::FileState {
                                                    current_path: path.clone(), selected_index: 0, table_state: ratatui::widgets::TableState::default(),
                                                    files: Vec::new(), show_hidden: fs.show_hidden, git_status: std::collections::HashMap::new(),
                                                    clipboard: None, search_filter: String::new(), starred: fs.starred.clone(),
                                                    columns: fs.columns.clone(), history: vec![path], history_index: 0,
                                                };
                                                crate::modules::files::update_files(&mut new_fs);
                                                app.file_tabs.push(new_fs);
                                            }
                                        }
                                    }
                                }
                                continue;
                            }

                            // Mouse 4/5 logic (Back/Forward)
                            if format!("{:?}", btn).contains("Back") || format!("{:?}", btn) == "Other(4)" {
                                if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); }
                                continue;
                            }
                            if format!("{:?}", btn).contains("Forward") || format!("{:?}", btn) == "Other(5)" {
                                if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); }
                                continue;
                            }

                            if btn == MouseButton::Left {
                                let mut is_double_click = false;
                                if let Some((last_time, last_row, last_col)) = app.last_click {
                                    if last_time.elapsed() < Duration::from_millis(500) && last_row == mouse.row && last_col == mouse.column { is_double_click = true; }
                                }
                                app.last_click = Some((Instant::now(), mouse.row, mouse.column));

                                if mouse.row == 0 {
                                    if mouse.column < 11 { app.current_view = CurrentView::Files; }
                                    else if mouse.column < 22 { app.current_view = CurrentView::System; }
                                    else if mouse.column < 33 { app.current_view = CurrentView::Docker; }
                                } else if mouse.row == rows.saturating_sub(1) {
                                    if mouse.column < 13 { app.mode = AppMode::CommandPalette; app.input.clear(); update_commands(app); }
                                } else {
                                    let sidebar_width = (cols as f32 * 0.2) as u16;
                                    if mouse.column < sidebar_width {
                                        app.sidebar_focus = true;
                                        let sidebar_row = mouse.row.saturating_sub(2) as usize;
                                        match sidebar_row {
                                            1..=4 => {
                                                app.sidebar_index = sidebar_row - 1;
                                                if is_double_click {
                                                    let path = match app.sidebar_index { 0 => dirs::home_dir(), 1 => dirs::download_dir(), 2 => dirs::document_dir(), 3 => dirs::picture_dir(), _ => None };
                                                    if let Some(p) = path {
                                                        if let Some(fs) = app.current_file_state_mut() {
                                                            fs.current_path = p.clone(); fs.selected_index = 0; fs.search_filter.clear();
                                                            *fs.table_state.offset_mut() = 0;
                                                            push_history(fs, p); crate::modules::files::update_files(fs); app.sidebar_focus = false;
                                                        }
                                                    }
                                                }
                                            },
                                            r if r >= 7 => {
                                                let bookmark_idx = r - 7;
                                                if bookmark_idx < app.remote_bookmarks.len() {
                                                    app.sidebar_index = r - 1;
                                                    if is_double_click {
                                                        execute_command(crate::app::CommandAction::ConnectToRemote(bookmark_idx), app, &docker_module);
                                                    }
                                                }
                                            },
                                            _ => {}
                                        }
                                    } else {
                                        app.sidebar_focus = false;
                                        if app.current_view == CurrentView::Files {
                                            let index = fs_mouse_index(mouse.row, app);
                                            if let Some(fs) = app.current_file_state_mut() {
                                                if index < fs.files.len() {
                                                    fs.selected_index = index; fs.table_state.select(Some(index));
                                                    if is_double_click {
                                                        if let Some(path) = fs.files.get(index).cloned() {
                                                            if path.is_dir() {
                                                                fs.current_path = path.clone(); fs.selected_index = 0; fs.search_filter.clear();
                                                                *fs.table_state.offset_mut() = 0;
                                                                push_history(fs, path); crate::modules::files::update_files(fs);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        MouseEventKind::ScrollUp => {
                            if app.current_view == CurrentView::Files {
                                if let Some(fs) = app.current_file_state_mut() {
                                    let new_offset = fs.table_state.offset().saturating_sub(3);
                                    *fs.table_state.offset_mut() = new_offset;
                                }
                            } else { app.move_up(); update_docker_filter(app); }
                        }
                        MouseEventKind::ScrollDown => {
                            if app.current_view == CurrentView::Files {
                                if let Some(fs) = app.current_file_state_mut() {
                                    let max_files = fs.files.len();
                                    let new_offset = (fs.table_state.offset() + 3).min(max_files.saturating_sub(1));
                                    *fs.table_state.offset_mut() = new_offset;
                                }
                            } else { app.move_down(); update_docker_filter(app); }
                        }
                        _ => {}
                    }
                }
                Event::Key(key) => {
                    match app.mode {
                        AppMode::Location => {
                            match key.code {
                                KeyCode::Esc => app.mode = AppMode::Normal,
                                KeyCode::Char(c) => app.input.push(c),
                                KeyCode::Backspace => { app.input.pop(); }
                                KeyCode::Enter => {
                                    let path = std::path::PathBuf::from(&app.input);
                                    if path.exists() {
                                        if let Some(fs) = app.current_file_state_mut() {
                                            fs.current_path = path.clone(); fs.selected_index = 0; *fs.table_state.offset_mut() = 0;
                                            push_history(fs, path); crate::modules::files::update_files(fs);
                                        }
                                    }
                                    app.mode = AppMode::Normal;
                                }
                                _ => {}
                            }
                        }
                        AppMode::Rename => {
                            match key.code {
                                KeyCode::Esc => app.mode = AppMode::Normal,
                                KeyCode::Char(c) => app.input.push(c),
                                KeyCode::Backspace => { app.input.pop(); }
                                KeyCode::Enter => {
                                    let new_name = app.input.clone();
                                    if let Some(fs) = app.current_file_state_mut() {
                                        if let Some(old_path) = fs.files.get(fs.selected_index) {
                                            let mut new_path = old_path.clone();
                                            new_path.set_file_name(&new_name);
                                            let _ = std::fs::rename(old_path, new_path);
                                            crate::modules::files::update_files(fs);
                                        }
                                    }
                                    app.mode = AppMode::Normal;
                                }
                                _ => {}
                            }
                        }
                        AppMode::NewFolder => {
                            match key.code {
                                KeyCode::Esc => app.mode = AppMode::Normal,
                                KeyCode::Char(c) => app.input.push(c),
                                KeyCode::Backspace => { app.input.pop(); }
                                KeyCode::Enter => {
                                    let folder_name = app.input.clone();
                                    if let Some(fs) = app.current_file_state_mut() {
                                        let path = fs.current_path.join(folder_name);
                                        let _ = std::fs::create_dir_all(path);
                                        crate::modules::files::update_files(fs);
                                    }
                                    app.mode = AppMode::Normal;
                                }
                                _ => {}
                            }
                        }
                        AppMode::AddRemote => {
                            match key.code {
                                KeyCode::Esc => app.mode = AppMode::Normal,
                                KeyCode::Char(c) => app.input.push(c),
                                KeyCode::Backspace => { app.input.pop(); }
                                KeyCode::Enter => {
                                    let input = app.input.clone();
                                    let parts: Vec<&str> = input.split('@').collect();
                                    if parts.len() == 2 {
                                        let user = parts[0].to_string();
                                        let host_port: Vec<&str> = parts[1].split(':').collect();
                                        let host = host_port[0].to_string();
                                        let port = host_port.get(1).and_then(|p| p.parse().ok()).unwrap_or(22);
                                        app.remote_bookmarks.push(crate::app::RemoteBookmark {
                                            name: host.clone(), host, user, port, last_path: std::path::PathBuf::from("/"),
                                        });
                                    }
                                    app.mode = AppMode::Normal;
                                }
                                _ => {}
                            }
                        }
                        AppMode::ColumnSetup => {
                            if key.code == KeyCode::Esc || key.code == KeyCode::Enter { app.mode = AppMode::Normal; }
                            else if let Some(fs) = app.current_file_state_mut() {
                                match key.code {
                                    KeyCode::Char('n') => toggle_column(fs, crate::app::FileColumn::Name),
                                    KeyCode::Char('s') => toggle_column(fs, crate::app::FileColumn::Size),
                                    KeyCode::Char('m') => toggle_column(fs, crate::app::FileColumn::Modified),
                                    KeyCode::Char('c') => toggle_column(fs, crate::app::FileColumn::Created),
                                    KeyCode::Char('p') => toggle_column(fs, crate::app::FileColumn::Permissions),
                                    KeyCode::Char('e') => toggle_column(fs, crate::app::FileColumn::Extension),
                                    _ => {}
                                }
                            }
                        }
                        AppMode::Delete => {
                            if key.code == KeyCode::Char('y') || key.code == KeyCode::Enter {
                                if let Some(fs) = app.current_file_state_mut() {
                                    if let Some(path) = fs.files.get(fs.selected_index) {
                                        let _ = if path.is_dir() { std::fs::remove_dir_all(path) } else { std::fs::remove_file(path) };
                                        crate::modules::files::update_files(fs);
                                    }
                                }
                            }
                            app.mode = AppMode::Normal;
                        }
                        AppMode::CommandPalette => {
                            match key.code {
                                KeyCode::Esc => app.mode = AppMode::Normal,
                                KeyCode::Char(c) => { app.input.push(c); update_commands(app); }
                                KeyCode::Backspace => { app.input.pop(); update_commands(app); }
                                KeyCode::Up => { if app.command_index > 0 { app.command_index -= 1; } }
                                KeyCode::Down => { if app.command_index < app.filtered_commands.len().saturating_sub(1) { app.command_index += 1; } }
                                KeyCode::Enter => {
                                    if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() {
                                        execute_command(cmd.action, app, &docker_module);
                                    }
                                    app.mode = AppMode::Normal; app.input.clear();
                                }
                                _ => {}
                            }
                        }
                        AppMode::Normal | AppMode::Zoomed | AppMode::Properties | AppMode::ContextMenu(_, _) => {
                            match key.code {
                                KeyCode::Char('q') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => app.running = false,
                                KeyCode::Char('f') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => app.current_view = CurrentView::Files,
                                KeyCode::Char('p') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => app.current_view = CurrentView::System,
                                KeyCode::Char('d') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => app.current_view = CurrentView::Docker,
                                KeyCode::Char('l') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                    let path_opt = app.current_file_state().map(|fs| fs.current_path.to_string_lossy().to_string());
                                    if let Some(p) = path_opt { app.mode = AppMode::Location; app.input = p; }
                                }
                                KeyCode::Char('h') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                    if let Some(fs) = app.current_file_state_mut() { fs.show_hidden = !fs.show_hidden; crate::modules::files::update_files(fs); }
                                }
                                KeyCode::Char('b') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        if let Some(path) = fs.files.get(fs.selected_index).cloned() {
                                            if !fs.starred.insert(path.clone()) { fs.starred.remove(&path); }
                                        }
                                    }
                                }
                                KeyCode::Char('t') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                    if let Some(curr) = app.current_file_state() {
                                        let mut new_fs = crate::app::FileState {
                                            current_path: curr.current_path.clone(), selected_index: 0, table_state: ratatui::widgets::TableState::default(),
                                            files: Vec::new(), show_hidden: curr.show_hidden, git_status: std::collections::HashMap::new(),
                                            clipboard: None, search_filter: String::new(), starred: curr.starred.clone(),
                                            columns: curr.columns.clone(), history: vec![curr.current_path.clone()], history_index: 0,
                                        };
                                        crate::modules::files::update_files(&mut new_fs);
                                        app.file_tabs.push(new_fs); app.tab_index = app.file_tabs.len() - 1;
                                    }
                                }
                                KeyCode::Char('w') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                    if app.file_tabs.len() > 1 { app.file_tabs.remove(app.tab_index); app.tab_index = app.tab_index.min(app.file_tabs.len() - 1); }
                                    else { app.running = false; }
                                }
                                KeyCode::Tab if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                                    app.tab_index = (app.tab_index + 1) % app.file_tabs.len();
                                }
                                KeyCode::Char('C') if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                                    if app.current_view == CurrentView::Files { app.mode = AppMode::ColumnSetup; }
                                }
                                KeyCode::Left if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                                    if let Some(fs) = app.current_file_state_mut() { navigate_back(fs); }
                                }
                                KeyCode::Right if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                                    if let Some(fs) = app.current_file_state_mut() { navigate_forward(fs); }
                                }
                                KeyCode::Up if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        if let Some(p) = fs.current_path.parent() {
                                            let path = p.to_path_buf(); fs.current_path = path.clone(); fs.selected_index = 0;
                                            *fs.table_state.offset_mut() = 0;
                                            push_history(fs, path); crate::modules::files::update_files(fs);
                                        }
                                    }
                                }
                                KeyCode::Down => { app.move_down(); update_docker_filter(app); }
                                KeyCode::Up => { app.move_up(); update_docker_filter(app); }
                                KeyCode::Left => { 
                                    if !app.sidebar_focus { app.sidebar_focus = true; }
                                    else { if !app.current_file_state().map(|s| !s.search_filter.is_empty()).unwrap_or(false) { app.move_left(); } }
                                }
                                KeyCode::Right => {
                                    if app.sidebar_focus { app.sidebar_focus = false; }
                                    else { if !app.current_file_state().map(|s| !s.search_filter.is_empty()).unwrap_or(false) { app.move_right(); } }
                                }
                                KeyCode::F(5) => { if let Some(fs) = app.current_file_state_mut() { crate::modules::files::update_files(fs); } }
                                KeyCode::F(2) => {
                                    let name_opt = app.current_file_state().and_then(|fs| fs.files.get(fs.selected_index).map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()));
                                    if let Some(n) = name_opt { app.mode = AppMode::Rename; app.input = n; }
                                }
                                KeyCode::Delete => { app.mode = AppMode::Delete; }
                                KeyCode::Enter if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => { app.mode = AppMode::Properties; }
                                KeyCode::Enter => {
                                    if app.sidebar_focus {
                                        let path = match app.sidebar_index { 0 => dirs::home_dir(), 1 => dirs::download_dir(), 2 => dirs::document_dir(), 3 => dirs::picture_dir(), _ => None };
                                        if let Some(p) = path {
                                            if let Some(fs) = app.current_file_state_mut() {
                                                fs.current_path = p.clone(); fs.selected_index = 0; fs.search_filter.clear();
                                                *fs.table_state.offset_mut() = 0;
                                                push_history(fs, p); crate::modules::files::update_files(fs); app.sidebar_focus = false;
                                            }
                                        }
                                    } else if let Some(fs) = app.current_file_state_mut() {
                                        if let Some(path) = fs.files.get(fs.selected_index).cloned() {
                                            if path.is_dir() {
                                                fs.current_path = path.clone(); fs.selected_index = 0; fs.search_filter.clear();
                                                *fs.table_state.offset_mut() = 0;
                                                push_history(fs, path); crate::modules::files::update_files(fs);
                                            }
                                        }
                                    }
                                }
                                KeyCode::Backspace => {
                                    if let Some(fs) = app.current_file_state_mut() {
                                        if !fs.search_filter.is_empty() { fs.search_filter.pop(); crate::modules::files::update_files(fs); }
                                        else if let Some(p) = fs.current_path.parent() {
                                            let path = p.to_path_buf(); fs.current_path = path.clone(); fs.selected_index = 0;
                                            *fs.table_state.offset_mut() = 0;
                                            push_history(fs, path); crate::modules::files::update_files(fs);
                                        }
                                    }
                                }
                                KeyCode::Esc => { if let Some(fs) = app.current_file_state_mut() { if !fs.search_filter.is_empty() { fs.search_filter.clear(); crate::modules::files::update_files(fs); } } }
                                KeyCode::Char(c) => {
                                    if app.current_view == CurrentView::Files {
                                        if let Some(fs) = app.current_file_state_mut() { fs.search_filter.push(c); fs.selected_index = 0; crate::modules::files::update_files(fs); }
                                    } else if app.current_view == CurrentView::Docker {
                                        if c == 's' || c == 'x' {
                                            if let Some(container) = app.docker_state.containers.get(app.docker_state.selected_index) {
                                                let name = container.names.as_ref().map(|n| n.first().map(|s| s.as_str()).unwrap_or("")).unwrap_or("").trim_start_matches('/').to_string();
                                                if !name.is_empty() { if let Some(docker) = &docker_module { let docker = docker.clone(); tokio::spawn(async move { if c == 's' { let _ = docker.start_container(&name).await; } else { let _ = docker.stop_container(&name).await; } }); } }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if last_tick.elapsed() >= tick_rate { app.system_module.update(&mut app.system_state); last_tick = Instant::now(); }
    }
    Ok(())
}

fn fs_mouse_index(row: u16, app: &App) -> usize {
    let mouse_row_offset = row.saturating_sub(7) as usize;
    if let Some(fs) = app.current_file_state() { fs.table_state.offset() + mouse_row_offset }
    else { 0 }
}

fn update_commands(app: &mut App) {
    let mut commands = vec![
        CommandItem { label: "Quit".to_string(), action: crate::app::CommandAction::Quit },
        CommandItem { label: "Toggle Zoom".to_string(), action: crate::app::CommandAction::ToggleZoom },
        CommandItem { label: "View: Files".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::Files) },
        CommandItem { label: "View: Docker".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::Docker) },
        CommandItem { label: "View: System".to_string(), action: crate::app::CommandAction::SwitchView(CurrentView::System) },
        CommandItem { label: "Add Remote Host".to_string(), action: crate::app::CommandAction::AddRemote },
    ];
    for container in &app.docker_state.containers {
         let name = container.names.as_ref().map(|n| n.first().map(|s| s.as_str()).unwrap_or("")).unwrap_or("").trim_start_matches('/');
         if !name.is_empty() {
             commands.push(CommandItem { label: format!("Start Container: {}", name), action: crate::app::CommandAction::StartContainer(name.to_string()) });
             commands.push(CommandItem { label: format!("Stop Container: {}", name), action: crate::app::CommandAction::StopContainer(name.to_string()) });
         }
    }
    app.filtered_commands = commands.into_iter().filter(|cmd| cmd.label.to_lowercase().contains(&app.input.to_lowercase())).collect();
    app.command_index = app.command_index.min(app.filtered_commands.len().saturating_sub(1));
}

fn execute_command(action: crate::app::CommandAction, app: &mut App, docker_module: &Option<Arc<DockerModule>>) {
    match action {
        crate::app::CommandAction::Quit => { app.running = false; },
        crate::app::CommandAction::ToggleZoom => app.toggle_zoom(),
        crate::app::CommandAction::SwitchView(view) => app.current_view = view,
        crate::app::CommandAction::StartContainer(name) => { if let Some(docker) = docker_module { let docker = docker.clone(); tokio::spawn(async move { let _ = docker.start_container(&name).await; }); } },
        crate::app::CommandAction::StopContainer(name) => { if let Some(docker) = docker_module { let docker = docker.clone(); tokio::spawn(async move { let _ = docker.stop_container(&name).await; }); } },
        crate::app::CommandAction::AddRemote => { app.mode = AppMode::AddRemote; app.input.clear(); }
        crate::app::CommandAction::ConnectToRemote(idx) => {
            if let Some(bookmark) = app.remote_bookmarks.get(idx) {
                let host = bookmark.host.clone();
                let port = bookmark.port;
                let user = bookmark.user.clone();
                let key = format!("{}:{}", host, port);
                
                if !app.active_sessions.contains_key(&key) {
                    // Start SSH connection attempt
                    let addr = format!("{}:{}", host, port);
                    if let Ok(tcp) = std::net::TcpStream::connect(&addr) {
                        if let Ok(mut sess) = ssh2::Session::new() {
                            sess.set_tcp_stream(tcp);
                            if sess.handshake().is_ok() {
                                // Try agent auth first
                                if sess.userauth_agent(&user).is_ok() {
                                    app.active_sessions.insert(key.clone(), Arc::new(sess));
                                }
                            }
                        }
                    }
                }

                if app.active_sessions.contains_key(&key) {
                    if let Some(fs) = app.current_file_state_mut() {
                        fs.remote_session = Some(crate::app::RemoteSession {
                            name: bookmark.name.clone(),
                            host: host.clone(),
                            user: user.clone(),
                        });
                        fs.current_path = std::path::PathBuf::from("/");
                        crate::modules::files::update_files(fs);
                    }
                }
            }
        }
    }
}

fn toggle_column(file_state: &mut crate::app::FileState, col: crate::app::FileColumn) {
    if file_state.columns.contains(&col) { file_state.columns.retain(|c| *c != col); }
    else { file_state.columns.push(col); }
}