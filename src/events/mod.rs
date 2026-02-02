use terma::input::event::{Event, KeyCode, KeyModifiers, KeyEventKind};
use tokio::sync::mpsc;
use crate::app::{App, AppEvent, AppMode, CurrentView};
use std::collections::HashSet;

pub mod editor;
pub mod file_manager;
pub mod monitor;
pub mod git;
pub mod input;
pub mod modals;

pub fn handle_event(
    evt: Event,
    app: &mut App,
    event_tx: mpsc::Sender<AppEvent>,
    _panes_needing_refresh: &mut HashSet<usize>,
) -> bool {
    // 1. Input Shield / Cooldown
    if let Some(until) = app.input_shield_until {
        if std::time::Instant::now() < until {
            if let Event::Resize(w, h) = evt {
                app.terminal_size = (w, h);
            }
            return true; 
        }
    }

    // 2. Global Resize
    if let Event::Resize(w, h) = evt {
        app.terminal_size = (w, h);
        return true;
    }

    // 3. Mode-specific logic (Modals, Overlays)
    if !matches!(app.mode, AppMode::Normal) {
        if modals::handle_modal_events(&evt, app, &event_tx) {
            return true;
        }
    }

    // 4. View-specific logic
    match app.current_view {
        CurrentView::Editor => {
            if editor::handle_editor_events(&evt, app, &event_tx) {
                return true;
            }
        }
        CurrentView::Processes => {
            if monitor::handle_monitor_events(&evt, app, &event_tx) {
                return true;
            }
        }
        CurrentView::Git => {
            if git::handle_git_events(&evt, app, &event_tx) {
                return true;
            }
        }
        CurrentView::Files => {
            if file_manager::handle_file_events(&evt, app, &event_tx) {
                return true;
            }
        }
    }

    // 5. Global Hotkeys (if not handled by mode/view)
    if let Event::Key(key) = evt {
        if key.kind == KeyEventKind::Press {
            let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
            
            if has_control {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') => {
                        app.running = false;
                        return true;
                    }
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        app.toggle_split();
                        app.save_current_view_prefs();
                        let _ = crate::config::save_state(app);
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(0));
                        let _ = event_tx.try_send(AppEvent::RefreshFiles(1));
                        return true;
                    }
                    KeyCode::Char('b') | KeyCode::Char('B') => {
                        app.show_sidebar = !app.show_sidebar;
                        app.save_current_view_prefs();
                        return true;
                    }
                    KeyCode::Char('e') | KeyCode::Char('E') => {
                        let _ = event_tx.try_send(AppEvent::Editor);
                        return true;
                    }
                    KeyCode::Char('l') | KeyCode::Char('L') => {
                        let _ = event_tx.try_send(AppEvent::GitHistory);
                        return true;
                    }
                    _ => {}
                }
            }
        }
    }

    false
}