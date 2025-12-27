use std::{io, time::{Duration, Instant}};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
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
use crate::app::{App, AppMode, CommandItem, CommandAction, CurrentView};
use crate::modules::docker::DockerModule;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let mut app = App::new();

    // Setup Docker channel
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

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn update_docker_filter(app: &mut App) {
    if let Some(path) = app.file_state.files.get(app.file_state.selected_index) {
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            app.docker_state.filter = Some(name.to_string());
        } else {
            app.docker_state.filter = None;
        }
    } else {
        app.docker_state.filter = None;
    }
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>, 
    app: &mut App, 
    mut rx: mpsc::Receiver<Vec<String>>,
    docker_module: Option<Arc<DockerModule>>,
) -> io::Result<()> {
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    while app.running {
        terminal.draw(|f| ui::draw(f, app))?;

        // Handle async updates
        while let Ok(containers) = rx.try_recv() {
            app.docker_state.containers = containers;
        }

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = crossterm::event::read()? {
                if matches!(app.mode, AppMode::Location) {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => {
                            let path = std::path::PathBuf::from(&app.input);
                            if path.exists() {
                                app.file_state.current_path = path;
                                app.file_state.selected_index = 0;
                                crate::modules::files::update_files(&mut app.file_state);
                            }
                            app.mode = AppMode::Normal;
                        }
                        _ => {}
                    }
                    continue;
                }

                if matches!(app.mode, AppMode::Rename) {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => {
                            if let Some(old_path) = app.file_state.files.get(app.file_state.selected_index) {
                                let mut new_path = old_path.clone();
                                new_path.set_file_name(&app.input);
                                if let Ok(_) = std::fs::rename(old_path, new_path) {
                                    crate::modules::files::update_files(&mut app.file_state);
                                }
                            }
                            app.mode = AppMode::Normal;
                        }
                        _ => {}
                    }
                    continue;
                }

                if matches!(app.mode, AppMode::Properties) {
                    if key.code == KeyCode::Esc || key.code == KeyCode::Enter {
                        app.mode = AppMode::Normal;
                    }
                    continue;
                }

                if matches!(app.mode, AppMode::NewFolder) {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => {
                            let mut path = app.file_state.current_path.clone();
                            path.push(&app.input);
                            if let Ok(_) = std::fs::create_dir_all(path) {
                                crate::modules::files::update_files(&mut app.file_state);
                            }
                            app.mode = AppMode::Normal;
                        }
                        _ => {}
                    }
                    continue;
                }

                if matches!(app.mode, AppMode::Delete) {
                    match key.code {
                        KeyCode::Char('y') | KeyCode::Enter => {
                            match app.current_view {
                                CurrentView::Files => {
                                    if let Some(path) = app.file_state.files.get(app.file_state.selected_index) {
                                        let _ = if path.is_dir() {
                                            std::fs::remove_dir_all(path)
                                        } else {
                                            std::fs::remove_file(path)
                                        };
                                        crate::modules::files::update_files(&mut app.file_state);
                                    }
                                }
                                CurrentView::System => {
                                     // Kill process logic
                                     if let Some(p) = app.system_state.processes.get(app.system_state.selected_process_index) {
                                         // In real app, we'd use kill(pid)
                                         // For now, placeholder
                                         // app.system_module.kill_process(p.pid);
                                     }
                                }
                                CurrentView::Docker => {
                                     if let Some(name) = app.docker_state.containers.get(app.docker_state.selected_index) {
                                         if let Some(docker) = &docker_module {
                                             let docker = docker.clone();
                                             let name = name.clone();
                                             tokio::spawn(async move {
                                                 // remove container with force=true?
                                                 // docker.remove_container(&name, ...).await
                                             });
                                         }
                                     }
                                }
                            }
                            app.mode = AppMode::Normal;
                        }
                        _ => {
                            app.mode = AppMode::Normal;
                        }
                    }
                    continue;
                }

                if matches!(app.mode, AppMode::CommandPalette) {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => {
                            app.input.push(c);
                            update_commands(app);
                        }
                        KeyCode::Backspace => { 
                            app.input.pop();
                            update_commands(app);
                        }
                        KeyCode::Up => {
                            if app.command_index > 0 {
                                app.command_index -= 1;
                            }
                        }
                        KeyCode::Down => {
                            if app.command_index < app.filtered_commands.len().saturating_sub(1) {
                                app.command_index += 1;
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(cmd) = app.filtered_commands.get(app.command_index).cloned() {
                                execute_command(cmd.action, app, &docker_module);
                            }
                            app.mode = AppMode::Normal;
                            app.input.clear();
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => app.running = false,
                    
                    // Ctrl+Key Modifiers (Must come before single chars)
                    KeyCode::Char('P') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        app.mode = AppMode::CommandPalette;
                        app.input.clear();
                        update_commands(app);
                    }
                    KeyCode::Char('p') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        app.mode = AppMode::CommandPalette;
                        app.input.clear();
                        update_commands(app);
                    }
                    KeyCode::Char('N') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        if app.current_view == CurrentView::Files {
                            app.mode = AppMode::NewFolder;
                            app.input = "New Folder".to_string();
                        }
                    }
                    KeyCode::Char('l') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                         if app.current_view == CurrentView::Files {
                            app.mode = AppMode::Location;
                            app.input = app.file_state.current_path.to_string_lossy().to_string();
                        }
                    }
                    KeyCode::Char('h') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        app.file_state.show_hidden = !app.file_state.show_hidden;
                        crate::modules::files::update_files(&mut app.file_state);
                    }

                    // View Switching Shortcuts
                    KeyCode::Char('f') => app.current_view = CurrentView::Files,
                    KeyCode::Char('p') => app.current_view = CurrentView::System,
                    KeyCode::Char('d') => app.current_view = CurrentView::Docker,
                    KeyCode::Char('c') => {
                        app.mode = AppMode::CommandPalette;
                        app.input.clear();
                        update_commands(app);
                    KeyCode::F(5) => {
                         crate::modules::files::update_files(&mut app.file_state);
                    }
                    KeyCode::Delete => {
                        match app.current_view {
                            CurrentView::Files | CurrentView::System | CurrentView::Docker => {
                                app.mode = AppMode::Delete;
                            }
                        }
                    }
                    KeyCode::F(2) => {
                        if app.current_view == CurrentView::Files {
                            if let Some(path) = app.file_state.files.get(app.file_state.selected_index) {
                                app.mode = AppMode::Rename;
                                app.input = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                            }
                        }
                    }
                    KeyCode::Enter if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                         match app.current_view {
                             CurrentView::Files | CurrentView::System | CurrentView::Docker => {
                                 app.mode = AppMode::Properties;
                             }
                         }
                    }
                    KeyCode::Up if key.modifiers.contains(crossterm::event::KeyModifiers::ALT) => {
                         if app.current_view == crate::app::CurrentView::Files {
                            if let Some(parent) = app.file_state.current_path.parent() {
                                app.file_state.current_path = parent.to_path_buf();
                                app.file_state.selected_index = 0;
                                crate::modules::files::update_files(&mut app.file_state);
                            }
                        }
                    }
                    KeyCode::Tab => app.switch_view(),
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.move_down();
                        update_docker_filter(app);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.move_up();
                        update_docker_filter(app);
                    }
                    KeyCode::Left | KeyCode::Char('h') => {
                        app.move_left();
                        update_docker_filter(app);
                    }
                    KeyCode::Right | KeyCode::Char('l') => {
                        app.move_right();
                        update_docker_filter(app);
                    }
                    KeyCode::Char('s') => {
                        if app.current_view == crate::app::CurrentView::Docker {
                            if let Some(name) = app.docker_state.containers.get(app.docker_state.selected_index) {
                                if let Some(docker) = &docker_module {
                                    let docker = docker.clone();
                                    let name = name.clone();
                                    tokio::spawn(async move {
                                        let _ = docker.start_container(&name).await;
                                    });
                                }
                            }
                        }
                    }
                    KeyCode::Char('x') => {
                        if app.current_view == crate::app::CurrentView::Docker {
                            if let Some(name) = app.docker_state.containers.get(app.docker_state.selected_index) {
                                if let Some(docker) = &docker_module {
                                    let docker = docker.clone();
                                    let name = name.clone();
                                    tokio::spawn(async move {
                                        let _ = docker.stop_container(&name).await;
                                    });
                                }
                            }
                        }
                    }
                    KeyCode::Enter => {
                        if app.sidebar_focus && app.current_view == CurrentView::Files {
                            let path = match app.sidebar_index {
                                0 => dirs::home_dir(),
                                1 => dirs::download_dir(),
                                2 => dirs::document_dir(),
                                3 => dirs::picture_dir(),
                                _ => None,
                            };
                            if let Some(p) = path {
                                app.file_state.current_path = p;
                                app.file_state.selected_index = 0;
                                crate::modules::files::update_files(&mut app.file_state);
                                app.sidebar_focus = false; // Jump to file list
                            }
                        } else if app.current_view == CurrentView::Files {
                            if let Some(path) = app.file_state.files.get(app.file_state.selected_index) {
                                if path.is_dir() {
                                    app.file_state.current_path = path.clone();
                                    app.file_state.selected_index = 0;
                                    crate::modules::files::update_files(&mut app.file_state);
                                }
                            }
                        } else {
                            app.toggle_zoom();
                        }
                    }
                    KeyCode::Backspace => {
                        if app.current_view == crate::app::CurrentView::Files {
                            if let Some(parent) = app.file_state.current_path.parent() {
                                app.file_state.current_path = parent.to_path_buf();
                                app.file_state.selected_index = 0;
                                crate::modules::files::update_files(&mut app.file_state);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.system_module.update(&mut app.system_state);
            last_tick = Instant::now();
        }
    }
    Ok(())
}

fn update_commands(app: &mut App) {
    let mut commands = vec![
        CommandItem { label: "Quit".to_string(), action: CommandAction::Quit },
        CommandItem { label: "Toggle Zoom".to_string(), action: CommandAction::ToggleZoom },
        CommandItem { label: "View: Files".to_string(), action: CommandAction::SwitchView(CurrentView::Files) },
        CommandItem { label: "View: Docker".to_string(), action: CommandAction::SwitchView(CurrentView::Docker) },
        CommandItem { label: "View: System".to_string(), action: CommandAction::SwitchView(CurrentView::System) },
    ];
    
    // Add dynamic commands (Docker containers)
    for name in &app.docker_state.containers {
         commands.push(CommandItem { 
             label: format!("Start Container: {}", name), 
             action: CommandAction::StartContainer(name.clone()) 
         });
         commands.push(CommandItem { 
             label: format!("Stop Container: {}", name), 
             action: CommandAction::StopContainer(name.clone()) 
         });
    }

    app.filtered_commands = commands.into_iter()
        .filter(|cmd| cmd.label.to_lowercase().contains(&app.input.to_lowercase()))
        .collect();
    
    // Ensure index is valid
    if app.filtered_commands.is_empty() {
        app.command_index = 0;
    } else if app.command_index >= app.filtered_commands.len() {
        app.command_index = app.filtered_commands.len() - 1;
    }
}

fn execute_command(action: CommandAction, app: &mut App, docker_module: &Option<Arc<DockerModule>>) {
    match action {
        CommandAction::Quit => {
            app.running = false;
        },
        CommandAction::ToggleZoom => app.toggle_zoom(),
        CommandAction::SwitchView(view) => app.current_view = view,
        CommandAction::StartContainer(name) => {
             if let Some(docker) = docker_module {
                let docker = docker.clone();
                tokio::spawn(async move {
                    let _ = docker.start_container(&name).await;
                });
            }
        },
        CommandAction::StopContainer(name) => {
            if let Some(docker) = docker_module {
                let docker = docker.clone();
                tokio::spawn(async move {
                    let _ = docker.stop_container(&name).await;
                });
            }
        },
    }
}