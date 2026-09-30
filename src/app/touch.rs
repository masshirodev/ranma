//! The window manager's side of the mobile view (DESIGN.md, "A mobile view"):
//! the chrome at the driving terminal's size, toolbar taps, latched
//! modifiers and `send`. Layout is `chrome` and `toolbar`; this is the state
//! and the input.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::{App, Layout, Mode, chord_event};
use crate::action::{Action, Latch};
use crate::chrome::{self, Chrome, Side, WantToolbar};
use crate::config::BindAction;
use crate::input;
use crate::layout::Rect;
use crate::theme::BarPosition;
use crate::toolbar::{self, Face, Slot};

/// How long a pressed face stays drawn after the release: Termux sends a
/// tap's press and release together when the finger lifts, and a pressed
/// state never drawn is no feedback at all. A held frame, not an animation.
const PRESS_HOLD: Duration = Duration::from_millis(90);

/// Tapped again within this, a latch locks instead of letting go.
const DOUBLE_TAP: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hold {
    #[default]
    Off,
    /// For the next key only.
    Latched,
    /// Until tapped again.
    Locked,
}

/// Modifiers held for the next key. They belong to the terminal that latched
/// them: another terminal driving clears them (see `driven_by`).
#[derive(Debug, Default)]
pub struct Latches {
    ctrl: Hold,
    alt: Hold,
    shift: Hold,
    last_tap: Option<(Latch, Instant)>,
}

impl Latches {
    fn get(&self, l: Latch) -> Hold {
        match l {
            Latch::Ctrl => self.ctrl,
            Latch::Alt => self.alt,
            Latch::Shift => self.shift,
        }
    }

    fn set(&mut self, l: Latch, h: Hold) {
        match l {
            Latch::Ctrl => self.ctrl = h,
            Latch::Alt => self.alt = h,
            Latch::Shift => self.shift = h,
        }
    }

    /// A tap on a latch: off to latched, latched to locked when quick, else
    /// off; locked to off.
    pub fn tap(&mut self, l: Latch, now: Instant) {
        let quick = self
            .last_tap
            .is_some_and(|(p, t)| p == l && now.duration_since(t) <= DOUBLE_TAP);
        let next = match (self.get(l), quick) {
            (Hold::Off, _) => Hold::Latched,
            (Hold::Latched, true) => Hold::Locked,
            (Hold::Latched, false) | (Hold::Locked, _) => Hold::Off,
        };
        self.set(l, next);
        self.last_tap = Some((l, now));
    }

    /// Let go of what is latched for one key; `all` lets go of locks too.
    pub fn release(&mut self, all: bool) {
        for l in [Latch::Ctrl, Latch::Alt, Latch::Shift] {
            if all || self.get(l) == Hold::Latched {
                self.set(l, Hold::Off);
            }
        }
    }

    pub fn any(&self) -> bool {
        [self.ctrl, self.alt, self.shift]
            .iter()
            .any(|h| *h != Hold::Off)
    }

    /// Add what is held to a key, and let go of what was held for one key.
    pub fn apply(&mut self, mut key: KeyEvent) -> KeyEvent {
        for (l, m) in [
            (Latch::Ctrl, KeyModifiers::CONTROL),
            (Latch::Alt, KeyModifiers::ALT),
            (Latch::Shift, KeyModifiers::SHIFT),
        ] {
            if self.get(l) != Hold::Off {
                key.modifiers |= m;
                if l == Latch::Shift
                    && let KeyCode::Char(c) = key.code
                {
                    key.code = KeyCode::Char(c.to_uppercase().next().unwrap_or(c));
                }
            }
        }
        self.release(false);
        key
    }

    /// The bar's mode slot: ` CTRL `, ` CTRL LOCK `, ` CTRL ALT `.
    pub fn label(&self) -> Option<String> {
        let mut parts = Vec::new();
        let mut locked = false;
        for l in [Latch::Ctrl, Latch::Alt, Latch::Shift] {
            match self.get(l) {
                Hold::Off => {}
                h => {
                    parts.push(l.name().to_uppercase());
                    locked |= h == Hold::Locked;
                }
            }
        }
        if parts.is_empty() {
            return None;
        }
        if locked {
            parts.push("LOCK".into());
        }
        Some(parts.join(" "))
    }
}

/// A face being pressed: drawn reversed from the press until a moment after
/// the release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pressed {
    pub toolbar: String,
    pub slot: Slot,
    pub until: Option<Instant>,
}

/// How a face is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Normal,
    Pressed,
    Latched,
    Locked,
    Active,
    Disabled,
}

/// A toolbar as drawn: its name, where it is and its faces.
pub struct ShownToolbar {
    pub name: String,
    pub placed: chrome::Placed,
    pub faces: Vec<Face>,
}

impl App {
    /// The chrome at the driving terminal's size. A yielded bar (a ranma inside
    /// another, whose bar shows this one's workspaces) keeps no rows.
    pub fn chrome(&self) -> Chrome {
        let bar = match self.config.theme.bar.position {
            _ if self.bar_yielded() => None,
            BarPosition::Hidden => None,
            BarPosition::Top => Some((Side::Top, self.config.bar.size)),
            BarPosition::Bottom => Some((Side::Bottom, self.config.bar.size)),
        };
        let ws = self.active();
        let strip = self.config.settings.layout == Layout::Monocle
            && !self.scratch_shown
            && !ws.fullscreen
            && ws.len() > 1;
        let toolbars = self
            .config
            .toolbars_shown
            .iter()
            .filter_map(|n| self.config.toolbar(n))
            .map(|t| WantToolbar {
                position: t.position,
                size: t.size,
                buttons: t.buttons.len(),
            })
            .collect();
        chrome::plan(&chrome::Wants {
            cols: self.screen.w,
            rows: self.screen.h,
            bar,
            strip,
            toolbars,
        })
    }

    /// The toolbars on screen, laid out.
    pub fn shown_toolbars(&self) -> Vec<ShownToolbar> {
        let chrome = self.chrome();
        self.config
            .toolbars_shown
            .iter()
            .zip(chrome.toolbars)
            .filter_map(|(name, placed)| {
                let placed = placed?;
                let def = self.config.toolbar(name)?;
                let labels: Vec<_> = def.buttons.iter().map(|b| b.label.clone()).collect();
                Some(ShownToolbar {
                    name: name.clone(),
                    faces: toolbar::layout(&labels, placed.rect, placed.size),
                    placed,
                })
            })
            .collect()
    }

    fn toolbar_face_at(&self, x: u16, y: u16) -> Option<(String, Slot)> {
        self.shown_toolbars()
            .into_iter()
            .find_map(|t| toolbar::hit(&t.faces, x, y).map(|f| (t.name.clone(), f.slot)))
    }

    /// A tap on a toolbar, in any mode and over any picker: pressed on the
    /// press, run on a release over the same face. Returns whether the event
    /// was the toolbar's.
    pub(super) fn toolbar_mouse(&mut self, m: MouseEvent) -> bool {
        let (x, y) = (m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let Some((toolbar, slot)) = self.toolbar_face_at(x, y) else {
                    return false;
                };
                if self.button_state(&toolbar, slot) == ButtonState::Disabled {
                    return true;
                }
                self.pressed = Some(Pressed {
                    toolbar,
                    slot,
                    until: None,
                });
                self.dirty = true;
                true
            }
            MouseEventKind::Drag(_) | MouseEventKind::Moved
                if self.pressed.as_ref().is_some_and(|p| p.until.is_none()) =>
            {
                true
            }
            MouseEventKind::Up(_) => {
                let Some(p) = self.pressed.clone().filter(|p| p.until.is_none()) else {
                    return false;
                };
                self.pressed = Some(Pressed {
                    until: Some(Instant::now() + PRESS_HOLD),
                    ..p.clone()
                });
                self.dirty = true;
                if self.toolbar_face_at(x, y) == Some((p.toolbar.clone(), p.slot)) {
                    self.press_button(&p.toolbar, p.slot);
                }
                true
            }
            // Anything else on a toolbar does nothing, and reaches no pane.
            _ => self.toolbar_face_at(x, y).is_some(),
        }
    }

    /// A face was tapped.
    fn press_button(&mut self, toolbar: &str, slot: Slot) {
        // The button whose sheet is open closes it; any other closes it and
        // runs.
        if self.picker.is_some() {
            let same = self.sheet_from.as_ref() == Some(&(toolbar.to_string(), slot));
            self.close_picker();
            if same {
                return;
            }
        }
        match slot {
            Slot::More(from) => self.open_toolbar_more(toolbar, from),
            Slot::Button(i) => self.run_button(toolbar, i),
        }
        if self.picker.is_some() {
            self.sheet_from = Some((toolbar.to_string(), slot));
        }
    }

    pub(super) fn run_button(&mut self, toolbar: &str, i: usize) {
        let Some(action) = self
            .config
            .toolbar(toolbar)
            .and_then(|t| t.buttons.get(i))
            .map(|b| match &b.action {
                BindAction::Builtin(a) => Ok(a.clone()),
                BindAction::Lua(k) => Err(self.config.lua.registry_value::<mlua::Function>(k)),
            })
        else {
            return;
        };
        // A ranma action lets go of what is latched for one key; a key sent
        // or another latch takes it.
        match action {
            Ok(a @ (Action::Send(_) | Action::Latch(_))) => self.run_action(a),
            Ok(a) => {
                self.latches.release(false);
                self.run_action(a);
            }
            Err(Ok(f)) => {
                self.latches.release(false);
                self.call_lua(|_| f.call::<()>(()));
            }
            Err(Err(_)) => self.status = Some("lua: button function is gone".into()),
        }
        self.dirty = true;
    }

    /// How a face is drawn now, from what its action would do.
    pub fn button_state(&self, toolbar: &str, slot: Slot) -> ButtonState {
        if self
            .pressed
            .as_ref()
            .is_some_and(|p| p.toolbar == toolbar && p.slot == slot)
        {
            return ButtonState::Pressed;
        }
        if self.picker.is_some() && self.sheet_from.as_ref() == Some(&(toolbar.to_string(), slot)) {
            return ButtonState::Active;
        }
        let Slot::Button(i) = slot else {
            return ButtonState::Normal;
        };
        let Some(action) = self
            .config
            .toolbar(toolbar)
            .and_then(|t| t.buttons.get(i))
            .and_then(|b| b.builtin())
        else {
            return ButtonState::Normal;
        };
        let focused = self.focused();
        let ws = self.active();
        match action {
            Action::Latch(l) => match self.latches.get(*l) {
                Hold::Off => ButtonState::Normal,
                Hold::Latched => ButtonState::Latched,
                Hold::Locked => ButtonState::Locked,
            },
            Action::SyncToggle if focused.is_some_and(|f| self.synced.contains(&f)) => {
                ButtonState::Active
            }
            Action::Fullscreen if ws.fullscreen => ButtonState::Active,
            Action::Profile(p) if self.config.profile == *p => ButtonState::Active,
            Action::Toolbar(n, _) if self.config.toolbars_shown.contains(n) => ButtonState::Active,
            Action::ClosePane
            | Action::Send(_)
            | Action::SyncToggle
            | Action::Fullscreen
            | Action::ToggleFloating
            | Action::CopyMode
            | Action::Hints
            | Action::Search
                if focused.is_none() =>
            {
                ButtonState::Disabled
            }
            Action::FocusCycle { .. } if ws.len() < 2 => ButtonState::Disabled,
            _ => ButtonState::Normal,
        }
    }

    pub(super) fn tap_latch(&mut self, l: Latch) {
        self.latches.tap(l, Instant::now());
        self.dirty = true;
    }

    /// A key typed at the keyboard, with what is latched added.
    pub(super) fn latched_key(&mut self, key: KeyEvent) -> KeyEvent {
        if !self.latches.any() {
            return key;
        }
        self.dirty = true;
        self.latches.apply(key)
    }

    /// `send`: type the chord into the focused pane, with what is latched, as
    /// if pressed there. Binds are not looked up: this is for the program.
    pub(super) fn send_chord(&mut self, chord: crate::keys::Chord) {
        let key = self.latched_key(chord_event(chord));
        self.typed(|modes| input::encode_key(&key, modes));
    }

    pub fn latch_label(&self) -> Option<String> {
        self.latches.label()
    }

    /// While a modifier is held the focused border is in the mode colour, as
    /// in WM mode: the next key is taken.
    pub fn latched(&self) -> bool {
        self.mode == Mode::Normal && self.latches.any()
    }

    pub(super) fn press_due(&self) -> Option<Instant> {
        self.pressed.as_ref().and_then(|p| p.until)
    }

    pub(super) fn expire_press(&mut self, now: Instant) {
        if self.press_due().is_some_and(|t| t <= now) {
            self.pressed = None;
            self.dirty = true;
        }
    }

    /// The one tile on screen: monocle's, or the only tile there is.
    pub(super) fn lone_tile(
        &self,
        ws: &crate::workspace::Workspace,
    ) -> Option<crate::layout::PaneId> {
        let tiles = ws.tree.panes();
        if self.config.settings.layout == Layout::Monocle {
            ws.focused
                .filter(|f| ws.tree.contains(*f))
                .or(ws.last_tile.filter(|t| ws.tree.contains(*t)))
                .or(tiles.first().copied())
        } else {
            (tiles.len() == 1).then(|| tiles[0])
        }
    }

    /// Close whatever picker is open, and forget which button opened it.
    pub(super) fn close_picker(&mut self) {
        self.picker = None;
        self.sheet_from = None;
        self.dirty = true;
    }

    /// `⋯`: the buttons that did not fit on the toolbar, as a picker (a sheet
    /// at large size), each run as if tapped.
    fn open_toolbar_more(&mut self, toolbar: &str, from: usize) {
        let Some(def) = self.config.toolbar(toolbar) else {
            return;
        };
        let items = def
            .buttons
            .iter()
            .enumerate()
            .skip(from)
            .map(|(i, b)| crate::picker::Item {
                label: b.label.text.as_ref().map_or_else(
                    || b.label.label.clone(),
                    |t| format!("{} {t}", b.label.label),
                ),
                detail: String::new(),
                target: crate::picker::Target::Button(toolbar.to_string(), i),
                current: false,
            })
            .collect();
        self.picker = Some(crate::picker::Picker::new(
            crate::picker::Kind::ToolbarMore,
            toolbar.to_string(),
            items,
        ));
        self.dirty = true;
    }

    /// The monocle strip's row, when it has one of its own.
    pub fn strip_row(&self) -> Option<Rect> {
        match self.chrome().strip {
            chrome::Strip::Row(r) => Some(r),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_latch_holds_for_one_key_and_locks_on_a_double_tap() {
        let t0 = Instant::now();
        let mut l = Latches::default();
        l.tap(Latch::Ctrl, t0);
        assert_eq!(l.label().as_deref(), Some("CTRL"));
        let k = l.apply(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(k.modifiers, KeyModifiers::CONTROL);
        assert!(!l.any(), "one key only");
        // Twice quickly: locked, for every key until tapped again.
        l.tap(Latch::Ctrl, t0);
        l.tap(Latch::Ctrl, t0 + Duration::from_millis(200));
        assert_eq!(l.label().as_deref(), Some("CTRL LOCK"));
        l.apply(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(l.label().as_deref(), Some("CTRL LOCK"));
        l.tap(Latch::Ctrl, t0 + Duration::from_secs(5));
        assert!(!l.any());
        // Twice slowly: on, then off.
        l.tap(Latch::Alt, t0);
        l.tap(Latch::Alt, t0 + Duration::from_secs(1));
        assert!(!l.any());
    }

    #[test]
    fn latches_stack() {
        let t0 = Instant::now();
        let mut l = Latches::default();
        l.tap(Latch::Ctrl, t0);
        l.tap(Latch::Alt, t0);
        assert_eq!(l.label().as_deref(), Some("CTRL ALT"));
        let k = l.apply(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(k.modifiers, KeyModifiers::CONTROL | KeyModifiers::ALT);
        l.tap(Latch::Shift, t0);
        let k = l.apply(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(k.code, KeyCode::Char('A'));
    }
}
