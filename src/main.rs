use std::time::Duration;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

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
    crate::app::log_debug("run_tty start");
    let backend = TermaBackend::new(std::io::stdout())?;
    let tile_queue = backend.tile_queue();
    let mut terminal = Terminal::new(backend)?;

    let (app, event_tx, mut event_rx) = setup_app(tile_queue);

    // 1. Input Loop (Thread)
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
                    Ok(false) => {
                        if let Some(evt) = parser.check_timeout() {
                             if let Some(converted) = crate::event::convert_event(evt) {
                                 let _ = tx.blocking_send(AppEvent::Raw(converted));
                             }
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // 2. System Stats Loop (Tokio)
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            let mut sys_mod = modules::system::SystemModule::new();
            loop {
                let data = sys_mod.get_data();
                let _ = tx.send(AppEvent::SystemUpdated(data)).await;
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
        });
    }

    // 3. Tick Loop (Tokio)
    {
        let tx = event_tx.clone();
        tokio::spawn(async move {
            loop {
                let _ = tx.send(AppEvent::Tick).await;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
    }

    // Initial State Setup
    {
        let mut app_guard = app.lock().unwrap();
        app_guard.running = true;
        if let Ok(size) = terminal.size() {
            app_guard.terminal_size = (size.width, size.height);
        }
        for i in 0..app_guard.panes.len() {
            let _ = event_tx.send(AppEvent::RefreshFiles(i)).await;
        }
    }

    crate::app::log_debug("Entering main loop");
    loop {
        let mut needs_draw = false;
        
        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Tick => {
                    needs_draw = true;
                }
                AppEvent::Raw(raw) => {
                    let mut app_guard = app.lock().unwrap();
                    if handle_event(raw, &mut app_guard, event_tx.clone()) {
                        needs_draw = true;
                    }
                }
                AppEvent::SystemUpdated(data) => {
                    let mut app_guard = app.lock().unwrap();
                    update_system_state(&mut app_guard, data);
                    needs_draw = true;
                }
                AppEvent::RefreshFiles(idx) => {
                    let mut app_guard = app.lock().unwrap();
                    if let Some(pane) = app_guard.panes.get_mut(idx) {
                        if let Some(fs) = pane.current_state_mut() {
                            crate::modules::files::update_files(fs, None);
                            needs_draw = true;
                        }
                    }
                }
                AppEvent::KillProcess(pid) => {
                    let _ = std::process::Command::new("kill").arg("-9").arg(pid.to_string()).status();
                }
                _ => {}
            }
        }

        {
            let mut app_guard = app.lock().unwrap();
            if !app_guard.running { break; }
            if needs_draw {
                app_guard.terminal_size = (terminal.size()?.width, terminal.size()?.height);
                terminal.draw(|f| ui::draw(f, &mut app_guard))?;
            }
        }

        tokio::time::sleep(Duration::from_millis(16)).await;
    }

    Ok(())
}

fn update_system_state(app: &mut App, data: terma::system::SystemData) {
    let s = &mut app.system_state;
    s.cpu_usage = data.cpu_usage;
    s.cpu_cores = data.cpu_cores.clone();
    s.mem_usage = data.mem_usage;
    s.total_mem = data.total_mem;
    s.swap_usage = data.swap_usage;
    s.total_swap = data.total_swap;
    s.disks = data.disks;
    s.processes = data.processes;
    s.os_name = data.os_name;
    s.os_version = data.os_version;
    s.kernel_version = data.kernel_version;
    s.hostname = data.hostname;
    s.uptime = data.uptime;

    s.cpu_history.push(data.cpu_usage as u64);
    if s.cpu_history.len() > 100 { s.cpu_history.remove(0); }

    if s.core_history.len() != data.cpu_cores.len() {
        s.core_history = vec![vec![0; 100]; data.cpu_cores.len()];
    }
    for (i, &usage) in data.cpu_cores.iter().enumerate() {
        s.core_history[i].push(usage as u64);
        if s.core_history[i].len() > 100 { s.core_history[i].remove(0); }
    }

    let mem_p = if data.total_mem > 0.0 { (data.mem_usage / data.total_mem) * 100.0 } else { 0.0 };
    s.mem_history.push(mem_p as u64);
    if s.mem_history.len() > 100 { s.mem_history.remove(0); }

    let swap_p = if data.total_swap > 0.0 { (data.swap_usage / data.total_swap) * 100.0 } else { 0.0 };
    s.swap_history.push(swap_p as u64);
    if s.swap_history.len() > 100 { s.swap_history.remove(0); }

    if s.last_net_in > 0 {
        let diff_in = data.net_in.saturating_sub(s.last_net_in);
        let diff_out = data.net_out.saturating_sub(s.last_net_out);
        s.net_in_history.push(diff_in);
        s.net_out_history.push(diff_out);
        if s.net_in_history.len() > 100 { s.net_in_history.remove(0); }
        if s.net_out_history.len() > 100 { s.net_out_history.remove(0); }
    }
    s.last_net_in = data.net_in;
    s.last_net_out = data.net_out;
    s.net_in = data.net_in;
    s.net_out = data.net_out;

    app.apply_process_sort();
}

fn setup_app(tile_queue: Arc<Mutex<Vec<terma::compositor::engine::TilePlacement>>>) -> (Arc<Mutex<App>>, mpsc::Sender<AppEvent>, mpsc::Receiver<AppEvent>) {
    let (tx, rx) = mpsc::channel(1000);
    let app = Arc::new(Mutex::new(App::new(tile_queue)));
    (app, tx, rx)
}

fn handle_event(evt: Event, app: &mut App, event_tx: mpsc::Sender<AppEvent>) -> bool {
    match evt {
        Event::Resize(w, h) => { app.terminal_size = (w, h); return true; }
        Event::Key(key) => {
            let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
            if app.current_view == CurrentView::Processes {
                match key.code {
                    KeyCode::Char('1') => { app.monitor_subview = MonitorSubview::Overview; app.process_search_filter.clear(); return true; }
                    KeyCode::Char('2') => { app.monitor_subview = MonitorSubview::Applications; app.process_search_filter.clear(); return true; }
                    KeyCode::Char('3') => { app.monitor_subview = MonitorSubview::Processes; app.process_search_filter.clear(); return true; }
                    KeyCode::Up => { app.move_process_up(); return true; }
                    KeyCode::Down => { app.move_process_down(); return true; }
                    KeyCode::Left => { 
                        app.monitor_subview = match app.monitor_subview {
                            MonitorSubview::Overview => MonitorSubview::Processes,
                            MonitorSubview::Applications => MonitorSubview::Overview,
                            MonitorSubview::Processes => MonitorSubview::Applications,
                        };
                        app.process_search_filter.clear();
                        return true;
                    }
                    KeyCode::Right => { 
                        app.monitor_subview = match app.monitor_subview {
                            MonitorSubview::Overview => MonitorSubview::Applications,
                            MonitorSubview::Applications => MonitorSubview::Processes,
                            MonitorSubview::Processes => MonitorSubview::Overview,
                        };
                        app.process_search_filter.clear();
                        return true;
                    }
                    KeyCode::Backspace => {
                        app.process_search_filter.pop();
                        return true;
                    }
                    KeyCode::Char(c) if !has_control => {
                        app.process_search_filter.push(c);
                        app.apply_process_sort();
                        return true;
                    }
                    KeyCode::Esc => { app.current_view = CurrentView::Files; return true; }
                    _ => {}
                }
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') if has_control => { app.running = false; return true; }
                KeyCode::Char('m') | KeyCode::Char('M') if has_control => { app.current_view = if app.current_view == CurrentView::Processes { CurrentView::Files } else { CurrentView::Processes }; return true; }
                KeyCode::Char('p') | KeyCode::Char('P') if has_control => { app.toggle_split(); return true; }
                KeyCode::Char('b') | KeyCode::Char('B') if has_control => { app.show_sidebar = !app.show_sidebar; return true; }
                KeyCode::Up => { app.move_up(false); return true; }
                KeyCode::Down => { app.move_down(false); return true; }
                KeyCode::Left => { app.move_left(); return true; }
                KeyCode::Right => { app.move_right(); return true; }
                KeyCode::Enter => {
                    if let Some(fs) = app.current_file_state_mut() {
                        if let Some(idx) = fs.selected_index {
                            if let Some(path) = fs.files.get(idx).cloned() {
                                if path.is_dir() {
                                    fs.current_path = path.clone();
                                    fs.selected_index = Some(0);
                                    let _ = event_tx.try_send(AppEvent::RefreshFiles(app.focused_pane_index));
                                } else {
                                    let _ = std::process::Command::new("xdg-open").arg(&path).spawn();
                                }
                            }
                        }
                    }
                    return true;
                }
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
                            app.process_search_filter.clear();
                            return true;
                        }
                    }
                    if row >= 6 {
                        let table_row = (row as usize).saturating_sub(6) + app.process_table_state.offset();
                        let proc_count = if app.monitor_subview == MonitorSubview::Processes {
                            app.system_state.processes.len()
                        } else {
                            let user = std::env::var("USER").unwrap_or_default();
                            app.system_state.processes.iter().filter(|p| p.user == user && !p.name.starts_with('[')).count()
                        };
                        if table_row < proc_count {
                            app.process_selected_idx = Some(table_row);
                            app.process_table_state.select(app.process_selected_idx);
                        }
                        return true;
                    }
                }
                if row == 0 {
                    if let Some((_, id)) = app.header_icon_bounds.iter().find(|(r, _)| r.contains(ratatui::layout::Position { x: col, y: row })) {
                        if id == "monitor" {
                            app.current_view = if app.current_view == CurrentView::Processes { CurrentView::Files } else { CurrentView::Processes };
                            return true;
                        }
                    }
                }
            }
        }
        _ => {}
    }
    false
}
