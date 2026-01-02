use terma::input::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MediaKeyCode, ModifierKeyCode, MouseButton, MouseEvent,
    MouseEventKind,
};
use terma::input::parser::{
    Event as ParserEvent, KeyCode as ParserKeyCode, KeyModifiers as ParserModifiers,
    MediaKeyCode as ParserMediaKeyCode, ModifierKeyCode as ParserModifierKeyCode,
    MouseButton as ParserMouseButton,
};

pub fn convert_event(p_evt: ParserEvent) -> Option<Event> {
    match p_evt {
        ParserEvent::Key(pk) => {
            let code = map_keycode(pk.code);
            let modifiers = map_modifiers(pk.modifiers);
            Some(Event::Key(KeyEvent { code, modifiers }))
        }
        ParserEvent::Mouse {
            button,
            line,
            column,
            is_press,
            is_drag,
            modifiers,
        } => {
            let kind = if is_drag {
                MouseEventKind::Drag(map_button(button))
            } else if is_press {
                match button {
                    ParserMouseButton::ScrollUp => MouseEventKind::ScrollUp,
                    ParserMouseButton::ScrollDown => MouseEventKind::ScrollDown,
                    ParserMouseButton::ScrollLeft => MouseEventKind::ScrollLeft,
                    ParserMouseButton::ScrollRight => MouseEventKind::ScrollRight,
                    _ => MouseEventKind::Down(map_button(button)),
                }
            } else {
                MouseEventKind::Up(map_button(button))
            };

            // Note: Parser returns bitflags for modifiers in Mouse (u8), need to map roughly or ignore
            // For now, defaulting or basic check
            let mut mods = KeyModifiers::empty();
            if modifiers & 4 != 0 {
                mods.insert(KeyModifiers::CONTROL);
            }
            if modifiers & 2 != 0 {
                mods.insert(KeyModifiers::ALT);
            }
            if modifiers & 1 != 0 {
                mods.insert(KeyModifiers::SHIFT);
            }

            Some(Event::Mouse(MouseEvent {
                kind,
                column,
                row: line,
                modifiers: mods,
            }))
        }
        ParserEvent::Focus(gained) => {
            if gained {
                Some(Event::FocusGained)
            } else {
                Some(Event::FocusLost)
            }
        }
        ParserEvent::Paste(s) => Some(Event::Paste(s)),
        _ => None,
    }
}

fn map_keycode(k: ParserKeyCode) -> KeyCode {
    match k {
        ParserKeyCode::Char(c) => KeyCode::Char(c),
        ParserKeyCode::Backspace => KeyCode::Backspace,
        ParserKeyCode::Enter => KeyCode::Enter,
        ParserKeyCode::Left => KeyCode::Left,
        ParserKeyCode::Right => KeyCode::Right,
        ParserKeyCode::Up => KeyCode::Up,
        ParserKeyCode::Down => KeyCode::Down,
        ParserKeyCode::Home => KeyCode::Home,
        ParserKeyCode::End => KeyCode::End,
        ParserKeyCode::PageUp => KeyCode::PageUp,
        ParserKeyCode::PageDown => KeyCode::PageDown,
        ParserKeyCode::Tab => KeyCode::Tab,
        ParserKeyCode::BackTab => KeyCode::BackTab,
        ParserKeyCode::Delete => KeyCode::Delete,
        ParserKeyCode::Insert => KeyCode::Insert,
        ParserKeyCode::F(n) => KeyCode::F(n),
        ParserKeyCode::Null => KeyCode::Null,
        ParserKeyCode::Esc => KeyCode::Esc,
        ParserKeyCode::CapsLock => KeyCode::CapsLock,
        ParserKeyCode::ScrollLock => KeyCode::ScrollLock,
        ParserKeyCode::NumLock => KeyCode::NumLock,
        ParserKeyCode::PrintScreen => KeyCode::PrintScreen,
        ParserKeyCode::Pause => KeyCode::Pause,
        ParserKeyCode::Menu => KeyCode::Menu,
        ParserKeyCode::KeypadBegin => KeyCode::KeypadBegin,
        ParserKeyCode::Media(m) => KeyCode::Media(map_media(m)),
        ParserKeyCode::Modifier(m) => KeyCode::Modifier(map_modifier_code(m)),
    }
}

fn map_media(m: ParserMediaKeyCode) -> MediaKeyCode {
    match m {
        ParserMediaKeyCode::Play => MediaKeyCode::Play,
        ParserMediaKeyCode::Pause => MediaKeyCode::Pause,
        ParserMediaKeyCode::PlayPause => MediaKeyCode::PlayPause,
        ParserMediaKeyCode::Reverse => MediaKeyCode::Reverse,
        ParserMediaKeyCode::Stop => MediaKeyCode::Stop,
        ParserMediaKeyCode::FastForward => MediaKeyCode::FastForward,
        ParserMediaKeyCode::Rewind => MediaKeyCode::Rewind,
        ParserMediaKeyCode::TrackNext => MediaKeyCode::TrackNext,
        ParserMediaKeyCode::TrackPrevious => MediaKeyCode::TrackPrevious,
        ParserMediaKeyCode::Record => MediaKeyCode::Record,
        ParserMediaKeyCode::LowerVolume => MediaKeyCode::LowerVolume,
        ParserMediaKeyCode::RaiseVolume => MediaKeyCode::RaiseVolume,
        ParserMediaKeyCode::MuteVolume => MediaKeyCode::MuteVolume,
    }
}

fn map_modifier_code(m: ParserModifierKeyCode) -> ModifierKeyCode {
    match m {
        ParserModifierKeyCode::LeftShift => ModifierKeyCode::LeftShift,
        ParserModifierKeyCode::LeftControl => ModifierKeyCode::LeftControl,
        ParserModifierKeyCode::LeftAlt => ModifierKeyCode::LeftAlt,
        ParserModifierKeyCode::LeftSuper => ModifierKeyCode::LeftSuper,
        ParserModifierKeyCode::LeftHyper => ModifierKeyCode::LeftHyper,
        ParserModifierKeyCode::LeftMeta => ModifierKeyCode::LeftMeta,
        ParserModifierKeyCode::RightShift => ModifierKeyCode::RightShift,
        ParserModifierKeyCode::RightControl => ModifierKeyCode::RightControl,
        ParserModifierKeyCode::RightAlt => ModifierKeyCode::RightAlt,
        ParserModifierKeyCode::RightSuper => ModifierKeyCode::RightSuper,
        ParserModifierKeyCode::RightHyper => ModifierKeyCode::RightHyper,
        ParserModifierKeyCode::RightMeta => ModifierKeyCode::RightMeta,
        ParserModifierKeyCode::IsoLevel3Shift => ModifierKeyCode::IsoLevel3Shift,
        ParserModifierKeyCode::IsoLevel5Shift => ModifierKeyCode::IsoLevel5Shift,
    }
}

fn map_modifiers(m: ParserModifiers) -> KeyModifiers {
    let mut out = KeyModifiers::empty();
    if m.shift {
        out.insert(KeyModifiers::SHIFT);
    }
    if m.ctrl {
        out.insert(KeyModifiers::CONTROL);
    }
    if m.alt {
        out.insert(KeyModifiers::ALT);
    }
    if m.super_key {
        out.insert(KeyModifiers::SUPER);
    }
    if m.hyper {
        out.insert(KeyModifiers::HYPER);
    }
    if m.meta {
        out.insert(KeyModifiers::META);
    }
    out
}

fn map_button(b: ParserMouseButton) -> MouseButton {
    match b {
        ParserMouseButton::Left => MouseButton::Left,
        ParserMouseButton::Right => MouseButton::Right,
        ParserMouseButton::Middle => MouseButton::Middle,
        ParserMouseButton::Back => MouseButton::Back,
        ParserMouseButton::Forward => MouseButton::Forward,
        ParserMouseButton::Other(u) => MouseButton::Other(u),
        _ => MouseButton::Left, // Fallback for scroll buttons if passed as button type
    }
}
