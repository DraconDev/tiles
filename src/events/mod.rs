use terma::input::event::{Event, KeyCode, KeyModifiers, KeyEventKind};
use tokio::sync::mpsc;
use crate::app::{App, AppEvent, AppMode, CurrentView};
use std::collections::HashSet;

pub mod editor;
pub mod file_manager;
pub mod monitor;
pub mod git;
pub mod input;

pub fn handle_event(
    evt: Event,
    app: &mut App,
    event_tx: mpsc::Sender<AppEvent>,
    panes_needing_refresh: &mut HashSet<usize>,
) -> bool {
    // SHIELD: Global input cooldown to prevent artifact leakage (e.g. from Escape sequences)
    if let Some(until) = app.input_shield_until {
        if std::time::Instant::now() < until {
            // Still ignore resize events normally, but consume others
            match evt {
                Event::Resize(w, h) => {
                    app.terminal_size = (w, h);
                }
                _ => {}
            }
            return true; 
        }
    }

    match evt {
        Event::Resize(w, h) => {
            app.terminal_size = (w, h);
            return true;
        }
        Event::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return false;
            }
            
            let has_control = key.modifiers.contains(KeyModifiers::CONTROL);
            let has_alt = key.modifiers.contains(KeyModifiers::ALT);
            let has_shift = key.modifiers.contains(KeyModifiers::SHIFT);

            if (key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q')) && has_control {
                app.running = false;
                return true;
            }

            // Global Escape (Ctrl+[)
            if has_control && key.code == KeyCode::Char('[') {
                return handle_global_escape(app);
            }

            // --- GLOBAL OVERRIDES (High Priority) ---
            if has_control {
                match key.code {
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

            // Route based on mode/view
            if editor::handle_editor_events(&evt, app, &event_tx) {
                return true;
            }

            // Add other routers here as we implement them
            // if monitor::handle_monitor_events(&evt, app, &event_tx) { return true; }
            
            // Fallback to legacy or other modules
            false
        }
        Event::Mouse(_) => {
            // Mouse handling will also be moved
            false
        }
    }
}

fn handle_global_escape(app: &mut App) -> bool {
    if matches!(app.mode, AppMode::Normal) {
        match app.current_view {
            CurrentView::Git | CurrentView::Processes => {
                app.current_view = CurrentView::Files;
                return true;
            }
            CurrentView::Editor => {
                app.save_current_view_prefs();
                app.current_view = CurrentView::Files;
                app.load_view_prefs(CurrentView::Files);
                for pane in &mut app.panes {
                    pane.preview = None;
                }
                app.input_shield_until = Some(std::time::Instant::now() + std::time::Duration::from_millis(50));
                return true;
            }
            _ => {}
        }
    } else {
        app.mode = AppMode::Normal;
        app.input.clear();
        app.rename_selected = false;
        return true;
    }
    false
}
