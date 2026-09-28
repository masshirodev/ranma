//! Host input: key events to chords (WM mode) and to bytes (everything else).
//!
//! crossterm decodes what the host terminal sends; the focused pane gets it
//! re-encoded for the modes *that pane* asked for. The host's modes are ranma's,
//! not the pane's — a program in application-cursor mode expects `ESC O A` for Up
//! even though the host sent `ESC [ A` — so passing host bytes through verbatim
//! would be wrong, and this is how tmux does it too.
//!
//! There is no paste detection here on purpose: pastes arrive as bracketed-paste
//! events from the host and nothing else is guessed at (see DESIGN.md, tuios #89/#113).

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};

use crate::keys::{Chord, Key, Mods};

/// The chord a key event spells, if it is one a bind could name.
pub fn chord_of(ev: &KeyEvent) -> Option<Chord> {
    let m = ev.modifiers;
    let mut mods = Mods {
        ctrl: m.contains(KeyModifiers::CONTROL),
        alt: m.contains(KeyModifiers::ALT),
        shift: m.contains(KeyModifiers::SHIFT),
        super_: m.contains(KeyModifiers::SUPER),
    };
    let key = match ev.code {
        KeyCode::Char(' ') => Key::Space,
        KeyCode::Char(c) => {
            // crossterm reports Shift+a as 'A' with SHIFT; binds spell it "shift+a".
            if c.is_uppercase() {
                mods.shift = true;
            }
            Key::Char(c.to_lowercase().next().unwrap_or(c))
        }
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Enter => Key::Return,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => {
            mods.shift = true;
            Key::Tab
        }
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Esc => Key::Escape,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    };
    Some(Chord { mods, key })
}

/// What the pane asked for that changes how input is encoded.
#[derive(Debug, Clone, Copy, Default)]
pub struct PaneModes {
    pub app_cursor: bool,
    pub bracketed_paste: bool,
    pub focus_events: bool,
    /// The program asked for clicks (1000), drags (1002) or all motion (1003).
    pub mouse_click: bool,
    pub mouse_drag: bool,
    pub mouse_motion: bool,
    /// SGR encoding (1006), which has no coordinate limit.
    pub mouse_sgr: bool,
    pub alt_screen: bool,
    /// On the alternate screen, turn the wheel into arrow keys (1007).
    pub alternate_scroll: bool,
}

impl PaneModes {
    pub fn wants_mouse(&self) -> bool {
        self.mouse_click || self.mouse_drag || self.mouse_motion
    }
}

/// xterm's modifier parameter: 1 + shift + 2*alt + 4*ctrl.
fn mod_param(m: KeyModifiers) -> u8 {
    1 + m.contains(KeyModifiers::SHIFT) as u8
        + 2 * m.contains(KeyModifiers::ALT) as u8
        + 4 * m.contains(KeyModifiers::CONTROL) as u8
}

/// Encode a key event for a pane, xterm style. `None` for events that produce
/// nothing (releases, bare modifiers, keys legacy encoding cannot express).
pub fn encode_key(ev: &KeyEvent, modes: PaneModes) -> Option<Vec<u8>> {
    if ev.kind == KeyEventKind::Release {
        return None;
    }
    let m = ev.modifiers;
    let ctrl = m.contains(KeyModifiers::CONTROL);
    let alt = m.contains(KeyModifiers::ALT);
    let modded = m.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT | KeyModifiers::CONTROL);

    let mut out = Vec::new();
    let esc_if_alt = |out: &mut Vec<u8>| {
        if alt {
            out.push(0x1b);
        }
    };

    // CSI letter keys: arrows, Home, End, F1-F4.
    let csi_letter = |letter: u8, ss3_ok: bool| -> Vec<u8> {
        if modded {
            format!("\x1b[1;{}{}", mod_param(m), letter as char).into_bytes()
        } else if ss3_ok {
            vec![0x1b, b'O', letter]
        } else {
            vec![0x1b, b'[', letter]
        }
    };
    // CSI number ~ keys: Insert, Delete, PageUp/Down, F5-F12.
    let csi_tilde = |n: u8| -> Vec<u8> {
        if modded {
            format!("\x1b[{n};{}~", mod_param(m)).into_bytes()
        } else {
            format!("\x1b[{n}~").into_bytes()
        }
    };

    match ev.code {
        KeyCode::Char(c) => {
            esc_if_alt(&mut out);
            if ctrl {
                out.push(ctrl_byte(c)?);
            } else {
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
        }
        KeyCode::Enter => {
            esc_if_alt(&mut out);
            out.push(b'\r');
        }
        KeyCode::Tab => {
            esc_if_alt(&mut out);
            out.push(b'\t');
        }
        KeyCode::BackTab => out.extend_from_slice(b"\x1b[Z"),
        KeyCode::Backspace => {
            esc_if_alt(&mut out);
            out.push(if ctrl { 0x08 } else { 0x7f });
        }
        KeyCode::Esc => {
            esc_if_alt(&mut out);
            out.push(0x1b);
        }
        KeyCode::Up => out = csi_letter(b'A', modes.app_cursor),
        KeyCode::Down => out = csi_letter(b'B', modes.app_cursor),
        KeyCode::Right => out = csi_letter(b'C', modes.app_cursor),
        KeyCode::Left => out = csi_letter(b'D', modes.app_cursor),
        KeyCode::Home => out = csi_letter(b'H', modes.app_cursor),
        KeyCode::End => out = csi_letter(b'F', modes.app_cursor),
        KeyCode::Insert => out = csi_tilde(2),
        KeyCode::Delete => out = csi_tilde(3),
        KeyCode::PageUp => out = csi_tilde(5),
        KeyCode::PageDown => out = csi_tilde(6),
        KeyCode::F(n @ 1..=4) => out = csi_letter(b'P' + (n - 1), true),
        KeyCode::F(n @ 5..=12) => {
            const CODES: [u8; 8] = [15, 17, 18, 19, 20, 21, 23, 24];
            out = csi_tilde(CODES[(n - 5) as usize]);
        }
        _ => return None,
    }
    Some(out)
}

/// The C0 byte Ctrl produces with this key, as xterm maps it.
fn ctrl_byte(c: char) -> Option<u8> {
    Some(match c.to_ascii_lowercase() {
        c @ 'a'..='z' => c as u8 - b'a' + 1,
        '@' | ' ' | '2' => 0,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '-' | '7' => 0x1f,
        '?' | '8' => 0x7f,
        _ => return None,
    })
}

pub fn encode_paste(text: &str, modes: PaneModes) -> Vec<u8> {
    if modes.bracketed_paste {
        // A paste containing the end marker could otherwise break out of the
        // bracket and have the rest run as typed input.
        let clean = text.replace("\x1b[201~", "");
        let mut out = b"\x1b[200~".to_vec();
        out.extend_from_slice(clean.as_bytes());
        out.extend_from_slice(b"\x1b[201~");
        out
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

pub fn encode_focus(gained: bool, modes: PaneModes) -> Option<&'static [u8]> {
    modes
        .focus_events
        .then_some(if gained { b"\x1b[I" } else { b"\x1b[O" })
}

/// Encode a mouse event at `col`, `row` (0-based, inside the pane) for a program
/// that enabled mouse reporting. `None` when the program did not ask for this kind
/// of event (motion without 1003, drags without 1002).
pub fn encode_mouse(
    kind: MouseEventKind,
    mods: KeyModifiers,
    col: u16,
    row: u16,
    m: PaneModes,
) -> Option<Vec<u8>> {
    if !m.wants_mouse() {
        return None;
    }
    let button = |b: MouseButton| match b {
        MouseButton::Left => 0u8,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    };
    let (code, release) = match kind {
        MouseEventKind::Down(b) => (button(b), false),
        MouseEventKind::Up(b) => (button(b), true),
        MouseEventKind::Drag(b) if m.mouse_drag || m.mouse_motion => (button(b) + 32, false),
        MouseEventKind::Moved if m.mouse_motion => (3 + 32, false),
        MouseEventKind::ScrollUp => (64, false),
        MouseEventKind::ScrollDown => (65, false),
        MouseEventKind::ScrollLeft => (66, false),
        MouseEventKind::ScrollRight => (67, false),
        _ => return None,
    };
    let code = code
        + 4 * mods.contains(KeyModifiers::SHIFT) as u8
        + 8 * mods.contains(KeyModifiers::ALT) as u8
        + 16 * mods.contains(KeyModifiers::CONTROL) as u8;
    let (x, y) = (col as u32 + 1, row as u32 + 1);
    if m.mouse_sgr {
        let end = if release { 'm' } else { 'M' };
        return Some(format!("\x1b[<{code};{x};{y}{end}").into_bytes());
    }
    // X10 encoding: a release is button 3, and coordinates past 223 cannot be sent.
    let code = if release { 3 + (code & !3) } else { code };
    if x > 223 || y > 223 {
        return None;
    }
    Some(vec![
        0x1b,
        b'[',
        b'M',
        32 + code,
        32 + x as u8,
        32 + y as u8,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }
    fn enc(code: KeyCode, mods: KeyModifiers) -> Vec<u8> {
        encode_key(&key(code, mods), PaneModes::default()).unwrap()
    }
    const NONE: KeyModifiers = KeyModifiers::NONE;

    #[test]
    fn chords_match_config_spelling() {
        let c = chord_of(&key(KeyCode::Char('b'), KeyModifiers::CONTROL)).unwrap();
        assert_eq!(c, "ctrl+b".parse().unwrap());
        let c = chord_of(&key(KeyCode::Char('A'), KeyModifiers::SHIFT)).unwrap();
        assert_eq!(c, "shift+a".parse().unwrap());
        let c = chord_of(&key(KeyCode::Right, KeyModifiers::SHIFT)).unwrap();
        assert_eq!(c, "shift+right".parse().unwrap());
        let c = chord_of(&key(KeyCode::BackTab, KeyModifiers::SHIFT)).unwrap();
        assert_eq!(c, "shift+tab".parse().unwrap());
        let c = chord_of(&key(KeyCode::Enter, KeyModifiers::ALT)).unwrap();
        assert_eq!(c, "alt+return".parse().unwrap());
    }

    #[test]
    fn plain_and_control_characters() {
        assert_eq!(enc(KeyCode::Char('a'), NONE), b"a");
        assert_eq!(enc(KeyCode::Char('ç'), NONE), "ç".as_bytes());
        assert_eq!(enc(KeyCode::Char('c'), KeyModifiers::CONTROL), [3]);
        assert_eq!(enc(KeyCode::Char('b'), KeyModifiers::CONTROL), [2]);
        assert_eq!(enc(KeyCode::Char(' '), KeyModifiers::CONTROL), [0]);
        assert_eq!(enc(KeyCode::Char('x'), KeyModifiers::ALT), b"\x1bx");
        assert_eq!(enc(KeyCode::Enter, NONE), b"\r");
        assert_eq!(enc(KeyCode::Backspace, NONE), [0x7f]);
        assert_eq!(enc(KeyCode::Esc, NONE), [0x1b]);
    }

    #[test]
    fn cursor_keys_follow_the_panes_mode() {
        assert_eq!(enc(KeyCode::Up, NONE), b"\x1b[A");
        let app = PaneModes {
            app_cursor: true,
            ..Default::default()
        };
        assert_eq!(encode_key(&key(KeyCode::Up, NONE), app).unwrap(), b"\x1bOA");
        // Modified keys use the CSI form regardless of mode.
        assert_eq!(
            encode_key(&key(KeyCode::Right, KeyModifiers::CONTROL), app).unwrap(),
            b"\x1b[1;5C"
        );
    }

    #[test]
    fn tilde_and_function_keys() {
        assert_eq!(enc(KeyCode::Delete, NONE), b"\x1b[3~");
        assert_eq!(enc(KeyCode::PageUp, KeyModifiers::SHIFT), b"\x1b[5;2~");
        assert_eq!(enc(KeyCode::F(1), NONE), b"\x1bOP");
        assert_eq!(enc(KeyCode::F(5), NONE), b"\x1b[15~");
        assert_eq!(enc(KeyCode::F(12), NONE), b"\x1b[24~");
    }

    #[test]
    fn mouse_is_encoded_only_when_asked_for() {
        use crossterm::event::{MouseButton, MouseEventKind};
        let down = MouseEventKind::Down(MouseButton::Left);
        assert_eq!(encode_mouse(down, NONE, 0, 0, PaneModes::default()), None);
        let click = PaneModes {
            mouse_click: true,
            ..Default::default()
        };
        assert_eq!(
            encode_mouse(down, NONE, 4, 9, click).unwrap(),
            vec![0x1b, b'[', b'M', 32, 37, 42]
        );
        // Drags and motion need their own modes.
        let drag = MouseEventKind::Drag(MouseButton::Left);
        assert_eq!(encode_mouse(drag, NONE, 1, 1, click), None);
        assert_eq!(encode_mouse(MouseEventKind::Moved, NONE, 1, 1, click), None);
        let sgr = PaneModes {
            mouse_click: true,
            mouse_drag: true,
            mouse_sgr: true,
            ..Default::default()
        };
        assert_eq!(
            encode_mouse(drag, KeyModifiers::CONTROL, 299, 0, sgr).unwrap(),
            b"\x1b[<48;300;1M"
        );
        assert_eq!(
            encode_mouse(MouseEventKind::Up(MouseButton::Right), NONE, 0, 0, sgr).unwrap(),
            b"\x1b[<2;1;1m"
        );
        assert_eq!(
            encode_mouse(MouseEventKind::ScrollUp, NONE, 0, 0, sgr).unwrap(),
            b"\x1b[<64;1;1M"
        );
        // Past column 223 the old encoding cannot say where the click was.
        assert_eq!(encode_mouse(down, NONE, 300, 0, click), None);
    }

    #[test]
    fn releases_produce_nothing() {
        let mut ev = key(KeyCode::Char('a'), NONE);
        ev.kind = KeyEventKind::Release;
        assert_eq!(encode_key(&ev, PaneModes::default()), None);
    }

    #[test]
    fn paste_is_bracketed_only_when_asked_and_cannot_escape() {
        let plain = encode_paste("a\nb", PaneModes::default());
        assert_eq!(plain, b"a\rb");
        let modes = PaneModes {
            bracketed_paste: true,
            ..Default::default()
        };
        let out = encode_paste("x\x1b[201~rm -rf", modes);
        assert_eq!(out, b"\x1b[200~xrm -rf\x1b[201~");
    }
}
