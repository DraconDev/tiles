use terma::input::event::{Event, KeyCode, KeyModifiers, KeyEventKind, MouseEventKind, MouseButton};
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

    // 4. View-specific logic (Keyboard)
    match &evt {
        Event::Key(_) => {
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
        }
        Event::Mouse(me) => {
            // General Mouse Handling
            if handle_general_mouse(me, app, &event_tx) {
                return true;
            }
        }
        Event::Paste(text) => {
            if let AppMode::Editor = app.mode {
                if let Some(preview) = &mut app.editor_state {
                    if let Some(editor) = &mut preview.editor {
                        editor.insert_string(text);
                        if app.auto_save {
                            let _ = event_tx.try_send(AppEvent::SaveFile(
                                preview.path.clone(),
                                editor.get_content(),
                            ));
                            editor.modified = false;
                        }
                        return true;
                    }
                }
            }
        }
        _ => {}
    }

    // 5. Global Hotkeys (Keyboard fallback)
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

fn handle_general_mouse(me: &terma::input::event::MouseEvent, app: &mut App, _event_tx: &mpsc::Sender<AppEvent>) -> bool {
    // This will eventually delegate to file_manager::handle_file_mouse, etc.
    // For now, I'll implement the core routing logic.
    app.mouse_pos = (me.column, me.row);
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use terma::input::event::{KeyEvent, KeyModifiers, KeyEventKind};
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_global_hotkeys_routing() {
        let tile_queue = Arc::new(Mutex::new(Vec::new()));
        let mut app = App::new(tile_queue);
        let (tx, _rx) = mpsc::channel(100);
        let mut refresh_set = HashSet::new();

        // Test Sidebar Toggle (Ctrl+B)
        let evt = Event::Key(KeyEvent {
            code: KeyCode::Char('b'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: terma::input::event::KeyEventState::empty(),
        });
        
        let initial_sidebar = app.show_sidebar;
        handle_event(evt, &mut app, tx.clone(), &mut refresh_set);
        assert_ne!(app.show_sidebar, initial_sidebar);

        // Test Split Toggle (Ctrl+P)
        let evt_split = Event::Key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: terma::input::event::KeyEventState::empty(),
        });
        
        let initial_split = app.is_split_mode;
        handle_event(evt_split, &mut app, tx.clone(), &mut refresh_set);
        assert_ne!(app.is_split_mode, initial_split);
    }
}
