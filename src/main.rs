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
    let docker_module = DockerModule::new().ok();

    if let Some(docker) = docker_module {
        tokio::spawn(async move {
            loop {
                if let Ok(containers) = docker.get_containers().await {
                    let _ = tx.send(containers).await;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    let res = run_app(&mut terminal, &mut app, rx).await;

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

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>, 
    app: &mut App, 
    mut rx: mpsc::Receiver<Vec<String>>
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
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Tab => app.next_tile(),
                    KeyCode::Char('j') | KeyCode::Down => {
                        if app.active_tile == crate::app::TileType::Files {
                            if app.file_state.selected_index < app.file_state.files.len().saturating_sub(1) {
                                app.file_state.selected_index += 1;
                            }
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if app.active_tile == crate::app::TileType::Files {
                            if app.file_state.selected_index > 0 {
                                app.file_state.selected_index -= 1;
                            }
                        }
                    }
                    KeyCode::Enter => app.toggle_zoom(),
                    KeyCode::Esc => {
                        if matches!(app.mode, AppMode::Zoomed) {
                            app.mode = AppMode::Normal;
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