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
use crate::app::{App, AppMode};
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

    loop {
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
                if matches!(app.mode, AppMode::CommandPalette) {
                    match key.code {
                        KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Enter => {
                            // TODO: Execute command
                            app.mode = AppMode::Normal;
                            app.input.clear();
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Char('p') if key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL) => {
                        app.mode = AppMode::CommandPalette;
                        app.input.clear();
                    }
                    KeyCode::Tab => app.next_tile(),
                    KeyCode::Char('j') | KeyCode::Down => {
                        if app.active_tile == crate::app::TileType::Files {
                            if app.file_state.selected_index < app.file_state.files.len().saturating_sub(1) {
                                app.file_state.selected_index += 1;
                                update_docker_filter(app);
                            }
                        } else if app.active_tile == crate::app::TileType::Docker {
                            if app.docker_state.selected_index < app.docker_state.containers.len().saturating_sub(1) {
                                app.docker_state.selected_index += 1;
                            }
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if app.active_tile == crate::app::TileType::Files {
                            if app.file_state.selected_index > 0 {
                                app.file_state.selected_index -= 1;
                                update_docker_filter(app);
                            }
                        } else if app.active_tile == crate::app::TileType::Docker {
                            if app.docker_state.selected_index > 0 {
                                app.docker_state.selected_index -= 1;
                            }
                        }
                    }
                    KeyCode::Char('s') => {
                        if app.active_tile == crate::app::TileType::Docker {
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
                        if app.active_tile == crate::app::TileType::Docker {
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
                        if app.active_tile == crate::app::TileType::Files {
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
                        if app.active_tile == crate::app::TileType::Files {
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
}