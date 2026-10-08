//! A plugin's screen in the app (`ranma.screen`): the slot it shares with
//! the settings panel, its keys and mouse, and the plugin's callbacks.
//!
//! The screen floats over the workspace's right side, where settings sits, and
//! never resizes a pane: it previews nothing about the layout, and the agents
//! list is opened and closed all day. One slot: opening a screen closes
//! settings (which asks first about unsaved edits) or the screen before it.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use mlua::Function;

use super::App;
use crate::luascreen::{Hooks, ScreenSpec, Update};
use crate::screen::{Outcome, Screen};

/// A plugin's screen while it is open.
pub struct PluginScreen {
    pub id: u64,
    pub screen: Screen,
    hooks: Hooks,
    /// When the plugin's `on_query` is next due (typing is debounced).
    query_due: Option<Instant>,
}

/// A plugin's screen is redrawn at most this often, however fast it updates.
const SCREEN_FRAME: Duration = Duration::from_millis(100);
/// `on_query` runs this long after the last key typed into the filter.
const QUERY_PAUSE: Duration = Duration::from_millis(150);

impl App {
    pub fn plugin_screen(&self) -> Option<&PluginScreen> {
        self.plugin_screen.as_ref()
    }

    /// `ranma.screen`: into the slot. Settings with unsaved edits asks first,
    /// and the screen opens once it is answered.
    pub(super) fn open_plugin_screen(&mut self, spec: ScreenSpec) {
        if let Some(st) = self.settings.as_mut() {
            if !st.panel.unsaved().is_empty() {
                st.panel.confirm = true;
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
        self.plugin_screen = Some(PluginScreen {
            id: spec.id,
            screen,
            hooks: spec.hooks,
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
                up.clone().apply(&mut ps.screen, &mut ps.hooks);
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
                    .group
                    .clone()
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
