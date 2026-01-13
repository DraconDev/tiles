use std::time::Duration;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use std::os::unix::process::CommandExt;

// Terma Imports
use terma::integration::ratatui::TermaBackend;
use terma::input::event::{Event, KeyCode, MouseEventKind, KeyModifiers};

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{App, CurrentView, AppEvent, MonitorSubview};

mod app;
mod ui;
mod modules;
mod config;
mod license;
mod icons;
mod event;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    std::panic::set_hook(Box::new(|panic_info| {
        let msg = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic".to_string()
        };
        let location = panic_info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_else(|| "unknown location".to_string());
        crate::app::log_debug(&format!("PANIC at {}: {}", location, msg));
    }));
    
    run_tty().await
}

async fn run_tty() -> color_eyre::Result<()> {
    let backend = TermaBackend::new(std::io::stdout())?;
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    let (app, event_tx, mut event_rx) = setup_app(tile_queue);

    // 1. Input Loop
    {
        let tx = event_tx.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            use std::os::fd::AsRawFd;
            let mut parser = terma::input::parser::Parser::new();
            let mut stdin = std::io::stdin();
            let fd = stdin.as_raw_fd();
            let mut buffer = [0; 1024];
            loop {
                let polled = unsafe { terma::backend::tty::poll_input(std::os::fd::BorrowedFd::borrow_raw(fd), 20) };
                match polled {
                    Ok(true) => {
                         match stdin.read(&mut buffer) {
                            Ok(0) => break,
                            Ok(n) => {
                                for i in 0..n {
                                    if let Some(evt) = parser.advance(buffer[i]) {
                                         if let Some(converted) = crate::event::convert_event(evt) { let _ = tx.blocking_send(AppEvent::Raw(converted)); }
                                    }
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    Ok(false) => { if let Some(evt) = parser.check_timeout() { if let Some(converted) = crate::event::convert_event(evt) { let _ = tx.blocking_send(AppEvent::Raw(converted)); } } }
                    Err(_) => break,
                }
            }
        });
    }

    // 2. System Stats
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut sys_mod = modules::system::SystemModule::new();
            loop {
                let data = sys_mod.get_data();
                let _ = tx.send(AppEvent::SystemUpdated(data)).await;
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        });
    }

    // 3. Tick
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            loop {
                let _ = tx.send(AppEvent::Tick).await;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    loop {
        let mut needs_draw = false;
        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Raw(raw) => {
                    let mut app_guard = app.lock().unwrap();
                    if handle_event(raw, &mut app_guard, event_tx.clone()) { needs_draw = true; }
                }
                AppEvent::Tick => { needs_draw = true; }
                AppEvent::SystemUpdated(data) => {
                    let mut app_guard = app.lock().unwrap();
                    app_guard.system_state.cpu_usage = data.cpu_usage;
                    app_guard.system_state.cpu_cores = data.cpu_cores.clone();
                    app_guard.system_state.mem_usage = data.mem_usage;
                    app_guard.system_state.total_mem = data.total_mem;
                    app_guard.system_state.swap_usage = data.swap_usage;
                    app_guard.system_state.total_swap = data.total_swap;
                    app_guard.system_state.disks = data.disks;
                    app_guard.system_state.processes = data.processes;
                    app_guard.system_state.os_name = data.os_name;
                    app_guard.system_state.os_version = data.os_version;
                    app_guard.system_state.kernel_version = data.kernel_version;
                    app_guard.system_state.hostname = data.hostname;
                    app_guard.system_state.uptime = data.uptime;
                    
                    app_guard.system_state.cpu_history.push(data.cpu_usage as u64);
                    if app_guard.system_state.cpu_history.len() > 100 { app_guard.system_state.cpu_history.remove(0); }
                    
                    if app_guard.system_state.core_history.len() != data.cpu_cores.len() {
                        app_guard.system_state.core_history = vec![vec![0; 100]; data.cpu_cores.len()];
                    }
                    for (i, &usage) in data.cpu_cores.iter().enumerate() {
                        app_guard.system_state.core_history[i].push(usage as u64);
                        if app_guard.system_state.core_history[i].len() > 100 { app_guard.system_state.core_history[i].remove(0); }
                    }

                    if app_guard.system_state.last_net_in > 0 {
                        let diff_in = data.net_in.saturating_sub(app_guard.system_state.last_net_in);
                        let diff_out = data.net_out.saturating_sub(app_guard.system_state.last_net_out);
                        app_guard.system_state.net_in_history.push(diff_in);
                        app_guard.system_state.net_out_history.push(diff_out);
                        if app_guard.system_state.net_in_history.len() > 100 { app_guard.system_state.net_in_history.remove(0); }
                        if app_guard.system_state.net_out_history.len() > 100 { app_guard.system_state.net_out_history.remove(0); }
                    }
                    app_guard.system_state.last_net_in = data.net_in;
                    app_guard.system_state.last_net_out = data.net_out;
                    app_guard.system_state.net_in = data.net_in;
                    app_guard.system_state.net_out = data.net_out;

                    let mem_p = if data.total_mem > 0.0 { (data.mem_usage / data.total_mem) * 100.0 } else { 0.0 };
                    app_guard.system_state.mem_history.push(mem_p as u64);
                    if app_guard.system_state.mem_history.len() > 100 { app_guard.system_state.mem_history.remove(0); }

                    let swap_p = if data.total_swap > 0.0 { (data.swap_usage / data.total_swap) * 100.0 } else { 0.0 };
                    app_guard.system_state.swap_history.push(swap_p as u64);
                    if app_guard.system_state.swap_history.len() > 100 { app_guard.system_state.swap_history.remove(0); }

                    app_guard.apply_process_sort();
                    needs_draw = true;
                }
                AppEvent::RefreshFiles(idx) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(fs) = app_guard.panes.get_mut(idx).and_then(|p| p.current_state_mut()) {
                        crate::modules::files::update_files(fs, None);
                        needs_draw = true;
                    }
                }
                _ => {}
            }
        }

        if needs_draw {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running { break; }
            terminal.draw(|f| crate::ui::draw(f, &mut app_guard))?;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(())
}

fn setup_app(tile_queue: Arc<Mutex<Vec<terma::compositor::engine::TilePlacement>>>) -> (Arc<Mutex<App>>, mpsc::Sender<AppEvent>, mpsc::Receiver<AppEvent>) {
    let (logic_tx, event_rx) = mpsc::channel(100);
    let app = Arc::new(Mutex::new(App::new(tile_queue)));
    (app, logic_tx, event_rx)
}

fn handle_event(evt: Event, app: &mut App, event_tx: mpsc::Sender<AppEvent>) -> bool {
    match evt {
        Event::Resize(w, h) => { app.terminal_size = (w, h); return true; }
        Event::Key(key) => {
            let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
            if app.current_view == CurrentView::Processes {
                match key.code {
                    KeyCode::Char('1') => { app.monitor_subview = MonitorSubview::Overview; return true; }
                    KeyCode::Char('2') => { app.monitor_subview = MonitorSubview::Applications; return true; }
                    KeyCode::Char('3') => { app.monitor_subview = MonitorSubview::Processes; return true; }
                    KeyCode::Up => { app.move_process_up(); return true; }
                    KeyCode::Down => { app.move_process_down(); return true; }
                    KeyCode::Esc => { app.current_view = CurrentView::Files; return true; }
                    _ => {}
                }
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') if has_control => { app.running = false; return true; }
                KeyCode::Char('m') | KeyCode::Char('M') if has_control => { app.current_view = if app.current_view == CurrentView::Processes { CurrentView::Files } else { CurrentView::Processes }; return true; }
                KeyCode::Up => { app.move_up(false); return true; }
                KeyCode::Down => { app.move_down(false); return true; }
                _ => {}
            }
        }
        Event::Mouse(me) => {
            let (col, row) = (me.column, me.row);
            if let MouseEventKind::Down(_) = me.kind {
                if app.current_view == CurrentView::Processes {
                    for (rect, view) in &app.monitor_subview_bounds {
                        if rect.contains(ratatui::layout::Position { x: col, y: row }) {
                            app.monitor_subview = *view; return true;
                        }
                    }
                    if row >= 6 {
                        let table_row = (row as usize).saturating_sub(6) + app.process_table_state.offset();
                        app.process_selected_idx = Some(table_row);
                        app.process_table_state.select(app.process_selected_idx);
                        return true;
                    }
                }
            }
        }
        _ => {}
    }
    false
}

pub fn get_context_menu_actions(_target: &crate::app::ContextMenuTarget, _app: &App) -> Vec<crate::app::ContextMenuAction> { vec![] }
fn push_history(_fs: &mut crate::app::FileState, _path: std::path::PathBuf) {}
fn spawn_detached(_cmd: &str, _args: Vec<&str>) {
    let mut command = std::process::Command::new(_cmd);
    command.args(_args);
    unsafe {
        let _ = command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
            .pre_exec(|| { libc::setsid(); Ok(()) }).spawn();
    }
}
