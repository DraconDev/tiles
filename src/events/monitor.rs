use crate::app::{App, AppEvent, CurrentView, MonitorSubview};
use terma::input::event::{Event, KeyCode};
use tokio::sync::mpsc;

pub fn handle_monitor_events(
    evt: &Event,
    app: &mut App,
    event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    if let Event::Key(key) = evt {
        if key.modifiers.is_empty() {
            match key.code {
                KeyCode::Char('1') => {
                    app.monitor_subview = MonitorSubview::Overview;
                    return true;
                }
                KeyCode::Char('2') => {
                    app.monitor_subview = MonitorSubview::Processes;
                    return true;
                }
                KeyCode::Char('3') => {
                    app.monitor_subview = MonitorSubview::Cpu;
                    return true;
                }
                KeyCode::Char('4') => {
                    app.monitor_subview = MonitorSubview::Memory;
                    return true;
                }
                KeyCode::Char('5') => {
                    app.monitor_subview = MonitorSubview::Disk;
                    return true;
                }
                KeyCode::Char('6') => {
                    app.monitor_subview = MonitorSubview::Network;
                    return true;
                }
                KeyCode::Up => {
                    if app.monitor_subview == MonitorSubview::Processes {
                        app.process_table_state.select(
                            app.process_table_state
                                .selected()
                                .map(|s| s.saturating_sub(1))
                                .or(Some(0)),
                        );
                        return true;
                    }
                }
                KeyCode::Down => {
                    if app.monitor_subview == MonitorSubview::Processes {
                        let len = app.system_state.processes.len();
                        app.process_table_state.select(
                            app.process_table_state
                                .selected()
                                .map(|s| (s + 1).min(len.saturating_sub(1)))
                                .or(Some(0)),
                        );
                        return true;
                    }
                }
                KeyCode::Char('k') => {
                    if app.monitor_subview == MonitorSubview::Processes {
                        if let Some(idx) = app.process_table_state.selected() {
                            if let Some(p) = app.system_state.processes.get(idx) {
                                let _ = event_tx.try_send(AppEvent::KillProcess(p.pid));
                            }
                        }
                        return true;
                    }
                }
                _ => {}
            }
        }
    }
    false
}

pub fn handle_monitor_mouse(
    me: &terma::input::event::MouseEvent,
    app: &mut App,
    _event_tx: &mpsc::Sender<AppEvent>,
) -> bool {
    if let terma::input::event::MouseEventKind::Down(_) = me.kind {
        for (rect, sv) in &app.monitor_subview_bounds {
            if rect.contains(ratatui::layout::Position {
                x: me.column,
                y: me.row,
            }) {
                app.monitor_subview = *sv;
                return true;
            }
        }
    }
    false
}
