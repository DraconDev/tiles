use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use std::path::PathBuf;

// Terma Imports
use terma::integration::ratatui::TermaBackend;
use terma::input::event::{Event, KeyCode, MouseButton, MouseEventKind, KeyModifiers};

// Ratatui Imports
use ratatui::Terminal;

use crate::app::{App, AppMode, CurrentView, CommandItem, AppEvent, DropTarget, SidebarTarget, ContextMenuTarget, MonitorSubview, ProcessColumn};
use crate::icons::IconMode;

mod app;
mod ui;
mod modules;
mod event;
mod config;
mod license;

fn main() -> color_eyre::Result<()> {
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
    
    run_tty()
}

fn run_tty() -> color_eyre::Result<()> {
    let backend = TermaBackend::new(std::io::stdout())?;
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    let (app, event_tx, mut event_rx) = setup_app(tile_queue);

    // TTY Event Loop
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
                                         if let Some(converted) = crate::event::convert_event(evt) {
                                             let _ = tx.blocking_send(AppEvent::Raw(converted));
                                         }
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

    // System Stats Loop
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

    // Tick Loop
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            loop {
                let _ = tx.send(AppEvent::Tick).await;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    // Initial Refresh
    {
        let mut app_guard = app.lock().unwrap();
        for i in 0..app_guard.panes.len() { let _ = event_tx.blocking_send(AppEvent::RefreshFiles(i)); }
        if let Ok(size) = terminal.size() { app_guard.terminal_size = (size.width, size.height); }
    }

    loop {
        let mut needs_draw = false;
        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Raw(raw) => {
                    let mut app_guard = app.lock().unwrap();
                    if handle_event(raw, &mut app_guard, event_tx.clone()) { needs_draw = true; }
                }
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
                AppEvent::Tick => { needs_draw = true; }
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
            terminal.draw(|f| crate::ui::draw(f, &mut app_guard))?;
        }

        if !app.lock().unwrap().running { break; }
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
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') if has_control => { app.running = false; return true; }
                KeyCode::Char('m') | KeyCode::Char('M') if has_control => { app.current_view = if app.current_view == CurrentView::Processes { CurrentView::Files } else { CurrentView::Processes }; return true; }
                KeyCode::Char('1') if app.current_view == CurrentView::Processes => { app.monitor_subview = MonitorSubview::Overview; return true; }
                KeyCode::Char('2') if app.current_view == CurrentView::Processes => { app.monitor_subview = MonitorSubview::Applications; return true; }
                KeyCode::Char('3') if app.current_view == CurrentView::Processes => { app.monitor_subview = MonitorSubview::Processes; return true; }
                KeyCode::Esc => {
                    if app.current_view == CurrentView::Processes { app.current_view = CurrentView::Files; }
                    app.mode = AppMode::Normal;
                    return true;
                }
                KeyCode::Up => { if app.current_view == CurrentView::Processes { app.move_process_up(); } else { app.move_up(false); } return true; }
                KeyCode::Down => { if app.current_view == CurrentView::Processes { app.move_process_down(); } else { app.move_down(false); } return true; }
                _ => {}
            }
        }
        Event::Mouse(me) => {
            let (col, row) = (me.column, me.row);
            if let MouseEventKind::Down(_) = me.kind {
                if app.current_view == CurrentView::Processes {
                    for (rect, view) in &app.monitor_subview_bounds {
                        if rect.contains(ratatui::layout::Position { x: col, y: row }) {
                            app.monitor_subview = *view;
                            return true;
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

fn push_history(fs: &mut crate::app::FileState, path: std::path::PathBuf) {
    if fs.history.get(fs.history_index) == Some(&path) { return; }
    fs.history.truncate(fs.history_index + 1);
    fs.history.push(path);
    fs.history_index = fs.history.len() - 1;
}

fn navigate_back(fs: &mut crate::app::FileState) {
    if fs.history_index > 0 {
        fs.history_index -= 1;
        fs.current_path = fs.history[fs.history_index].clone();
    }
}

fn navigate_forward(fs: &mut crate::app::FileState) {
    if fs.history_index + 1 < fs.history.len() {
        fs.history_index += 1;
        fs.current_path = fs.history[fs.history_index].clone();
    }
}

fn spawn_detached(cmd: &str, args: Vec<&str>) {
    let mut command = std::process::Command::new(cmd);
    command.args(args);
    unsafe {
        let _ = command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .pre_exec(|| { libc::setsid(); Ok(()) })
            .spawn();
    }
}

fn handle_context_menu_action(_action: &crate::app::ContextMenuAction, _target: &crate::app::ContextMenuTarget, _app: &mut App, _tx: mpsc::Sender<AppEvent>) {}
fn fs_mouse_index(_row: u16, _app: &App) -> usize { 0 }
fn update_commands(_app: &mut App) {}
fn execute_command(_action: crate::app::CommandAction, _app: &mut App, _tx: mpsc::Sender<AppEvent>) {}