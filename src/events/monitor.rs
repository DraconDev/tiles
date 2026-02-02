use terma::input::event::{Event, KeyCode, KeyModifiers, MouseEventKind, MouseButton};
use tokio::sync::mpsc;
use crate::app::{App, AppEvent, AppMode, CurrentView, MonitorSubview, ProcessColumn};

pub fn handle_monitor_events(evt: &Event, app: &mut App, _event_tx: &mpsc::Sender<AppEvent>) -> bool {
    if let CurrentView::Processes = app.current_view {
        if let Event::Key(key) = evt {
            if key.code == KeyCode::Esc && matches!(app.mode, AppMode::Normal) {
                app.current_view = CurrentView::Files;
                return true;
            }
            
            // Subview switching
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('1') => { app.monitor_subview = MonitorSubview::Overview; return true; }
                    KeyCode::Char('2') => { app.monitor_subview = MonitorSubview::Applications; return true; }
                    KeyCode::Char('3') => { app.monitor_subview = MonitorSubview::Processes; return true; }
                    _ => {}
                }
            }
        }
    }
    false
}

pub fn handle_monitor_mouse(me: &terma::input::event::MouseEvent, app: &mut App, _event_tx: &mpsc::Sender<AppEvent>) -> bool {
    let column = me.column;
    let row = me.row;

    if let MouseEventKind::Down(button) = me.kind {
        if button == MouseButton::Left {
            // Tab Clicks
            if let Some((_, subview)) = app.monitor_subview_bounds.iter().find(|(r, _)| column >= r.x && column < r.x + r.width && row == r.y) {
                app.monitor_subview = *subview;
                return true;
            }

            // Column Sorting Clicks
            if app.monitor_subview == MonitorSubview::Processes || app.monitor_subview == MonitorSubview::Applications {
                if let Some((_, col)) = app.process_column_bounds.iter().find(|(r, _)| column >= r.x && column < r.x + r.width && row == r.y) {
                    if app.process_sort_col == *col {
                        app.process_sort_asc = !app.process_sort_asc;
                    } else {
                        app.process_sort_col = *col;
                        app.process_sort_asc = true;
                    }
                    return true;
                }
            }

            // Selection Clicks
            if row >= 6 {
                let scroll_offset = app.process_table_state.offset();
                let rel_row = (row - 6) as usize + scroll_offset;
                if app.monitor_subview == MonitorSubview::Processes || app.monitor_subview == MonitorSubview::Applications {
                    app.process_selected_idx = Some(rel_row);
                    app.process_table_state.select(Some(rel_row));
                    return true;
                }
            }
        }
    }
    true // Trap all clicks in full screen monitor
}