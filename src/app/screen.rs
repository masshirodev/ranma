//! A plugin's screen in the app (`ranma.screen`): the slot it shares with
//! the settings panel, its keys and mouse, and the plugin's callbacks.
//!
//! The screen floats over the workspace's right side, where settings sits, and
//! never resizes a pane: it previews nothing about the layout, and the agents
//! list is opened and closed all day. One slot: opening a screen closes
//! settings (which asks first about unsaved edits) or the screen before it.
//!
//! The slot has the keyboard when it opens, and can hand it to the panes
//! while it stays open, so a card can be read beside the shell it is typed
//! into: a click on a pane, a focus move, or `settings` again (`leader p`)
//! hands it over; a click on the slot or `leader p` takes it back.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use mlua::Function;

use super::{App, Mode};
use crate::luascreen::{Hooks, ScreenSpec, Update};
use crate::screen::{Outcome, Screen};

/// A plugin's screen while it is open.
pub struct PluginScreen {
    pub id: u64,
    pub screen: Screen,
    hooks: Hooks,
    /// The callbacks of the view a `push` covered, for `esc` back.
    back_hooks: Option<Hooks>,
    /// When the plugin's `on_query` is next due (typing is debounced).
    query_due: Option<Instant>,
}

/// Where an input event goes while the slot is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Route {
    /// To the slot: settings or the plugin's screen.
    Slot,
    /// To the workspace, as with no slot open.
    Workspace,
    /// Taken already (the leader, from a slot with the keyboard).
    Done,
}

/// A plugin's screen is redrawn at most this often, however fast it updates.
const SCREEN_FRAME: Duration = Duration::from_millis(100);
/// `on_query` runs this long after the last key typed into the filter.
const QUERY_PAUSE: Duration = Duration::from_millis(150);

impl App {
    pub fn plugin_screen(&self) -> Option<&PluginScreen> {
        self.plugin_screen.as_ref()
    }

    /// Whether settings or a plugin's screen is open.
    pub fn slot_open(&self) -> bool {
        self.settings.is_some() || self.plugin_screen.is_some()
    }

    /// Whether the open slot has the keyboard (false with none open).
    pub fn slot_has_keys(&self) -> bool {
        self.slot_open() && self.slot_keys
    }

    /// Where the open slot is on screen: (x, y, w, h).
    pub fn slot_rect(&self) -> Option<(u16, u16, u16, u16)> {
        let bar_y = self.bar_rect().map(|b| b.y);
        let (w, h) = (self.screen.w, self.screen.h);
        if let Some(st) = self.settings.as_ref() {
            return Some(st.panel.rect(w, h, bar_y));
        }
        self.plugin_screen
            .as_ref()
            .map(|ps| ps.screen.rect(w, h, bar_y))
    }

    /// `settings` with the slot open: the keyboard moves between it and
    /// the panes.
    pub(super) fn toggle_slot_keys(&mut self) {
        self.slot_keys = !self.slot_keys;
        self.dirty = true;
    }

    /// Where an event goes. The slot gets the keys while it has the
    /// keyboard, except the leader, which enters WM mode here (never the
    /// focused pane's own ranma: the slot is this one's). The mouse goes
    /// by where it is: a press on the slot gives it the keyboard, a press
    /// outside gives the keyboard to the workspace and acts there as usual.
    pub(super) fn slot_route(&mut self, ev: &Event) -> Route {
        if !self.slot_open() {
            return Route::Workspace;
        }
        match ev {
            Event::Key(k) => {
                if !self.slot_keys || self.mode != Mode::Normal {
                    return Route::Workspace;
                }
                let leader = self.config.settings.leader;
                if k.kind != KeyEventKind::Release && crate::input::chord_of(k) == Some(leader) {
                    self.enter_wm(leader);
                    self.status = None;
                    return Route::Done;
                }
                Route::Slot
            }
            Event::Paste(_) if self.slot_keys && self.mode == Mode::Normal => Route::Slot,
            Event::Mouse(m) => {
                let Some((x, y, w, h)) = self.slot_rect() else {
                    return Route::Workspace;
                };
                let inside = m.column >= x && m.column < x + w && m.row >= y && m.row < y + h;
                if matches!(m.kind, MouseEventKind::Down(_)) && inside != self.slot_keys {
                    self.slot_keys = inside;
                    self.dirty = true;
                }
                if inside {
                    Route::Slot
                } else {
                    Route::Workspace
                }
            }
            _ => Route::Workspace,
        }
    }

    /// `ranma.screen`: into the slot. Settings with unsaved edits asks first,
    /// and the screen opens once it is answered.
    pub(super) fn open_plugin_screen(&mut self, spec: ScreenSpec) {
        if let Some(st) = self.settings.as_mut() {
            if !st.panel.unsaved().is_empty() {
                // The question needs an answer from the keyboard.
                st.panel.confirm = true;
                self.slot_keys = true;
                self.pending_screen = Some(Box::new(spec));
                self.dirty = true;
                return;
            }
            self.settings = None;
            self.stashed_screen = None;
            self.relayout();
        }
        self.close_plugin_screen(true);
        let mut screen = spec.screen;
        if screen.options && screen.group.is_none() {
            screen.group = Some(screen.title.clone());
        }
        self.slot_keys = true;
        self.plugin_screen = Some(PluginScreen {
            id: spec.id,
            screen,
            hooks: spec.hooks,
            back_hooks: None,
            query_due: None,
        });
        self.picker = None;
        self.dirty = true;
    }

    /// Settings closed: a screen waiting on it opens, or the one it was
    /// opened from (`o`) comes back.
    pub(super) fn after_settings_closed(&mut self, opened: bool) {
        if let Some(spec) = self.pending_screen.take()
            && opened
        {
            self.open_plugin_screen(*spec);
            return;
        }
        if let Some(back) = self.stashed_screen.take() {
            self.slot_keys = true;
            self.plugin_screen = Some(back);
            self.dirty = true;
        }
    }

    /// Close the slot's screen. `user`: by the user or another screen, so
    /// the plugin hears it (`on_close`); not when the plugin closed it.
    pub(super) fn close_plugin_screen(&mut self, user: bool) {
        let Some(ps) = self.plugin_screen.take() else {
            return;
        };
        self.dirty = true;
        if user {
            self.call_hook(&ps.hooks.on_close, ());
        }
    }

    pub(super) fn screen_update(&mut self, id: u64, up: Update) {
        let now = Instant::now();
        for ps in [self.plugin_screen.as_mut(), self.stashed_screen.as_mut()]
            .into_iter()
            .flatten()
        {
            if ps.id == id {
                up.clone()
                    .apply(&mut ps.screen, &mut ps.hooks, &mut ps.back_hooks);
            }
        }
        // Drawn at most every SCREEN_FRAME: a plugin updating a timer every
        // tick costs a frame a tenth of a second, not one per update.
        match self.screen_drawn {
            Some(t) if now < t + SCREEN_FRAME => {
                self.screen_redraw_due = Some(t + SCREEN_FRAME);
            }
            _ => self.dirty = true,
        }
    }

    pub(super) fn screen_close(&mut self, id: u64) {
        if self.plugin_screen.as_ref().is_some_and(|p| p.id == id) {
            self.close_plugin_screen(false);
        }
        if self.stashed_screen.as_ref().is_some_and(|p| p.id == id) {
            self.stashed_screen = None;
        }
    }

    /// Note that the screen was drawn (render calls this, through the run loop).
    pub fn screen_drawn_now(&mut self) {
        if self.plugin_screen.is_some() {
            self.screen_drawn = Some(Instant::now());
        }
    }

    pub(super) fn screen_timers(&mut self, now: Instant) {
        if self.screen_redraw_due.is_some_and(|t| t <= now) {
            self.screen_redraw_due = None;
            self.dirty = true;
        }
        let due = self
            .plugin_screen
            .as_ref()
            .and_then(|p| p.query_due)
            .is_some_and(|t| t <= now);
        if due {
            let (hook, q) = {
                let ps = self.plugin_screen.as_mut().expect("checked");
                ps.query_due = None;
                (ps.hooks.on_query.clone(), ps.screen.query.clone())
            };
            self.call_hook(&hook, q);
        }
    }

    pub(super) fn screen_deadline(&self) -> Option<Instant> {
        self.plugin_screen
            .as_ref()
            .and_then(|p| p.query_due)
            .into_iter()
            .chain(self.screen_redraw_due)
            .min()
    }

    /// The screen has the keyboard. The mouse over it selects and scrolls;
    /// elsewhere it is the workspace's as usual (returns false).
    pub(super) fn screen_input(&mut self, ev: &Event) -> bool {
        match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                let Some(ps) = self.plugin_screen.as_mut() else {
                    return false;
                };
                let outcome = ps.screen.key(k);
                self.dirty = true;
                self.screen_outcome(outcome);
                true
            }
            Event::Key(_) | Event::Paste(_) => true,
            Event::Mouse(m) => self.screen_mouse(*m),
            _ => false,
        }
    }

    fn screen_mouse(&mut self, m: MouseEvent) -> bool {
        let bar_y = self.bar_rect().map(|b| b.y);
        let (w, h) = (self.screen.w, self.screen.h);
        let Some(ps) = self.plugin_screen.as_mut() else {
            return false;
        };
        let (x, y, pw, ph) = ps.screen.rect(w, h, bar_y);
        let inside = m.column >= x && m.column < x + pw && m.row >= y && m.row < y + ph;
        if !inside {
            return false;
        }
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let hit = ps
                    .screen
                    .rows_at
                    .borrow()
                    .iter()
                    .find(|(row, _)| *row == m.row)
                    .map(|(_, id)| id.clone());
                if let Some(id) = hit {
                    ps.screen.sel = Some(id);
                }
            }
            MouseEventKind::ScrollUp => {
                ps.screen.key(&crossterm::event::KeyEvent::from(
                    crossterm::event::KeyCode::Up,
                ));
            }
            MouseEventKind::ScrollDown => {
                ps.screen.key(&crossterm::event::KeyEvent::from(
                    crossterm::event::KeyCode::Down,
                ));
            }
            _ => {}
        }
        self.dirty = true;
        true
    }

    fn screen_outcome(&mut self, outcome: Outcome) {
        match outcome {
            Outcome::Stay | Outcome::Peek => {}
            Outcome::Close => self.close_plugin_screen(true),
            Outcome::Options => {
                let Some(ps) = self.plugin_screen.take() else {
                    return;
                };
                let group = ps
                    .screen
                    .options_group()
                    .unwrap_or_else(|| ps.screen.title.clone());
                let title = ps.screen.title.clone();
                self.stashed_screen = Some(ps);
                self.open_settings_scoped(&group, &title);
            }
            Outcome::Key(k, row) => {
                let Some(ps) = self.plugin_screen.as_ref() else {
                    return;
                };
                let f = row
                    .as_ref()
                    .and_then(|r| ps.hooks.row_keys.get(&(r.clone(), k.clone())))
                    .or_else(|| ps.hooks.keys.get(&k))
                    .cloned();
                self.call_hook(&f, row);
            }
            Outcome::Step(id, forward) => {
                let f = self
                    .plugin_screen
                    .as_ref()
                    .and_then(|p| p.hooks.on_change.get(&id))
                    .cloned();
                self.call_hook(&f, if forward { 1 } else { -1 });
            }
            Outcome::Edit(id) => {
                let Some(ps) = self.plugin_screen.as_ref() else {
                    return;
                };
                let Some(f) = ps.hooks.on_edit.get(&id).cloned() else {
                    return;
                };
                let text = match ps.screen.selected().and_then(|r| r.value.clone()) {
                    Some(crate::screen::Value::Field(t)) => t,
                    _ => String::new(),
                };
                let name = ps
                    .screen
                    .selected()
                    .map(|r| r.name.clone())
                    .unwrap_or_default();
                self.open_lua_input(crate::luaui::InputSpec {
                    title: name,
                    text,
                    hooks: crate::luaui::Hooks {
                        on_select: Some(f),
                        ..Default::default()
                    },
                });
            }
            Outcome::Ask(k, text, row) => {
                let f = self
                    .plugin_screen
                    .as_ref()
                    .and_then(|p| p.hooks.ask_keys.get(&k))
                    .cloned();
                self.call_hook(&f, (row, text));
            }
            Outcome::Pop => {
                let mut back = None;
                if let Some(ps) = self.plugin_screen.as_mut()
                    && let Some(h) = ps.back_hooks.take()
                {
                    back = std::mem::replace(&mut ps.hooks, h).on_back;
                }
                self.call_hook(&back, ());
            }
            Outcome::Query(_) => {
                if let Some(ps) = self.plugin_screen.as_mut() {
                    ps.query_due = Some(Instant::now() + QUERY_PAUSE);
                }
            }
        }
    }

    /// Call a plugin's function, if it gave one, as a bind would be called.
    fn call_hook<A: mlua::IntoLuaMulti>(
        &mut self,
        f: &Option<std::rc::Rc<mlua::RegistryKey>>,
        args: A,
    ) {
        let Some(k) = f else {
            return;
        };
        let Ok(f) = self.config.lua.registry_value::<Function>(k) else {
            return;
        };
        self.call_lua(|_| f.call::<()>(args));
    }
}
