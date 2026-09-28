//! The window manager: state, input routing, actions, and the event loop.
//!
//! One thread owns all of this. PTY threads and the input thread only send it
//! messages, so nothing here is shared and nothing needs a lock except each
//! pane's `Term`, which alacritty_terminal's own thread also writes.

use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use alacritty_terminal::event::Event as TermEvent;
use alacritty_terminal::vte::ansi::CursorShape;
use anyhow::{Context, Result};
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange, Event,
    KeyEvent, KeyEventKind,
};
use crossterm::execute;

use crate::action::Action;
use crate::config::{self, BindAction, Config, Layout};
use crate::input;
use crate::layout::{self, PaneId, Placement, Rect, Split, Tree};
use crate::pane::{AppEvent, Pane, Size, SpawnOptions};
use crate::render::{self, CursorState};
use crate::theme::{BarPosition, BorderStyle};

/// The frame cap. Output arriving faster than this is coalesced: the pane is drawn
/// at its latest state once per interval, not once per read.
const FRAME: Duration = Duration::from_micros(8_333);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Keys go to the focused pane; only the leader is looked at.
    Normal,
    /// Keys are looked up in the bind table.
    Wm,
}

/// Where one pane is drawn this frame.
#[derive(Debug, Clone, Copy)]
pub struct PaneView {
    pub id: PaneId,
    /// Including the border.
    pub outer: Rect,
    /// The pane's own cells: its PTY size.
    pub inner: Rect,
    pub focused: bool,
}

pub struct App {
    pub config: Config,
    pub panes: HashMap<PaneId, Pane>,
    tree: Tree,
    focused: Option<PaneId>,
    fullscreen: bool,
    pub mode: Mode,
    /// A one-line message for the bar; cleared by the next key.
    pub status: Option<String>,
    screen: Rect,
    next_id: PaneId,
    tx: Sender<AppEvent>,
    dirty: bool,
    quit: bool,
}

impl App {
    pub fn new(config: Config, tx: Sender<AppEvent>, cols: u16, rows: u16) -> App {
        App {
            config,
            panes: HashMap::new(),
            tree: Tree::default(),
            focused: None,
            fullscreen: false,
            mode: Mode::Normal,
            status: None,
            screen: Rect::new(0, 0, cols, rows),
            next_id: 1,
            tx,
            dirty: true,
            quit: false,
        }
    }

    // ---- geometry ----------------------------------------------------------

    pub fn bar_rect(&self) -> Option<Rect> {
        let s = self.screen;
        match self.config.theme.bar.position {
            BarPosition::Hidden => None,
            _ if s.h == 0 => None,
            BarPosition::Top => Some(Rect::new(s.x, s.y, s.w, 1)),
            BarPosition::Bottom => Some(Rect::new(s.x, s.bottom() - 1, s.w, 1)),
        }
    }

    /// The area panes are laid out in: the screen minus the bar and outer gaps.
    fn workspace_area(&self) -> Rect {
        let s = self.screen;
        let mut a = match self.bar_rect() {
            None => s,
            Some(_) if self.config.theme.bar.position == BarPosition::Top => {
                Rect::new(s.x, s.y + 1, s.w, s.h.saturating_sub(1))
            }
            Some(_) => Rect::new(s.x, s.y, s.w, s.h.saturating_sub(1)),
        };
        let g = &self.config.theme.gaps;
        a = a.inset(g.outer_horizontal, g.outer_vertical);
        a
    }

    fn border(&self) -> u16 {
        match self.config.theme.border.style {
            BorderStyle::None => 0,
            _ => 1,
        }
    }

    /// Every visible pane and where it goes, focused one included.
    pub fn views(&self) -> Vec<PaneView> {
        let area = self.workspace_area();
        let b = self.border();
        let rects = match (self.fullscreen, self.focused) {
            (true, Some(f)) => vec![(f, area)],
            _ => self.tree.layout(area, self.config.theme.gaps.inner),
        };
        rects
            .into_iter()
            .map(|(id, outer)| PaneView {
                id,
                outer,
                inner: outer.inset(b, b),
                focused: Some(id) == self.focused,
            })
            .collect()
    }

    fn rects(&self) -> Vec<(PaneId, Rect)> {
        self.tree
            .layout(self.workspace_area(), self.config.theme.gaps.inner)
    }

    /// Push the current layout down to every PTY.
    fn relayout(&mut self) {
        for view in self.views() {
            if let Some(p) = self.panes.get_mut(&view.id) {
                p.resize(Size {
                    cols: view.inner.w,
                    rows: view.inner.h,
                });
            }
        }
        self.dirty = true;
    }

    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    pub fn focused_title(&self) -> Option<&str> {
        self.focused
            .and_then(|f| self.panes.get(&f))
            .map(|p| p.title.as_str())
    }

    // ---- panes ---------------------------------------------------------------

    pub fn open_pane(&mut self, command: Option<&str>) -> Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        let focused_rect = self
            .focused
            .and_then(|f| self.rects().into_iter().find(|(p, _)| *p == f))
            .map(|(_, r)| r);
        let placement = match self.config.settings.layout {
            Layout::Dwindle => Placement::Dwindle,
            // i3's default for a fresh split is side by side.
            Layout::Manual => Placement::Manual(Split::Horizontal),
        };
        self.tree.insert(id, self.focused, focused_rect, placement);
        self.fullscreen = false;

        // Spawn at the size the layout gives it, so the program starts at its real
        // size instead of getting a resize immediately after starting.
        let view = self.views().into_iter().find(|v| v.id == id);
        let size = view.map_or(Size { cols: 80, rows: 24 }, |v| Size {
            cols: v.inner.w,
            rows: v.inner.h,
        });
        let s = &self.config.settings;
        let opts = SpawnOptions {
            shell: s.shell.as_deref(),
            command,
            scrollback_lines: s.scrollback_lines,
        };
        match Pane::spawn(id, size, &opts, self.tx.clone()) {
            Ok(pane) => {
                self.panes.insert(id, pane);
                self.set_focus(Some(id));
                self.relayout();
                Ok(())
            }
            Err(e) => {
                self.tree.remove(id);
                self.relayout();
                Err(e)
            }
        }
    }

    fn close_pane(&mut self, id: PaneId) {
        // Focus goes to the neighbour the eye lands on, not whatever is first in
        // the tree: left, then up, then right, then down.
        let rects = self.rects();
        let next = if self.focused == Some(id) {
            use crate::action::Dir::*;
            [Left, Up, Right, Down]
                .into_iter()
                .find_map(|d| layout::neighbour(&rects, id, d))
                .or_else(|| rects.iter().map(|(p, _)| *p).find(|p| *p != id))
        } else {
            self.focused
        };
        self.tree.remove(id);
        self.panes.remove(&id);
        if self.panes.is_empty() {
            self.quit = true;
            return;
        }
        self.fullscreen = false;
        self.set_focus(next);
        self.relayout();
    }

    fn set_focus(&mut self, id: Option<PaneId>) {
        if id == self.focused {
            return;
        }
        if let Some(old) = self.focused.and_then(|f| self.panes.get(&f))
            && let Some(b) = input::encode_focus(false, old.modes())
        {
            old.write(b);
        }
        self.focused = id;
        if let Some(new) = id.and_then(|f| self.panes.get(&f))
            && let Some(b) = input::encode_focus(true, new.modes())
        {
            new.write(b);
        }
        self.dirty = true;
    }

    fn focused_pane(&self) -> Option<&Pane> {
        self.focused.and_then(|f| self.panes.get(&f))
    }

    // ---- events --------------------------------------------------------------

    pub fn handle(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Input(ev) => self.handle_input(ev),
            AppEvent::InputClosed => self.quit = true,
            AppEvent::Pane(id, ev) => self.handle_pane_event(id, ev),
        }
    }

    fn handle_pane_event(&mut self, id: PaneId, ev: TermEvent) {
        let Some(pane) = self.panes.get_mut(&id) else {
            return;
        };
        match ev {
            TermEvent::Wakeup => self.dirty = true,
            TermEvent::Title(t) => {
                pane.title = t;
                self.dirty = true;
            }
            TermEvent::ResetTitle => {
                pane.title.clear();
                self.dirty = true;
            }
            // Replies to queries the program made (device attributes, cursor
            // position): they go back to the program, not to the host.
            TermEvent::PtyWrite(s) => pane.write(s.into_bytes()),
            TermEvent::TextAreaSizeRequest(fmt) => {
                let size = pane.size;
                let reply = fmt(alacritty_terminal::event::WindowSize {
                    num_lines: size.rows,
                    num_cols: size.cols,
                    cell_width: 0,
                    cell_height: 0,
                });
                pane.write(reply.into_bytes());
            }
            TermEvent::ChildExit(_) | TermEvent::Exit => self.close_pane(id),
            // Clipboard (OSC 52), colour queries, bell: milestone 3.
            _ => {}
        }
    }

    fn handle_input(&mut self, ev: Event) {
        match ev {
            Event::Key(key) => self.handle_key(key),
            Event::Paste(text) => {
                if let Some(p) = self.focused_pane() {
                    p.write(input::encode_paste(&text, p.modes()));
                }
            }
            Event::FocusGained | Event::FocusLost => {
                if let Some(p) = self.focused_pane()
                    && let Some(b) = input::encode_focus(ev == Event::FocusGained, p.modes())
                {
                    p.write(b);
                }
            }
            Event::Resize(w, h) => {
                self.screen = Rect::new(0, 0, w, h);
                self.relayout();
            }
            Event::Mouse(_) => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        let chord = input::chord_of(&key);
        let leader = self.config.settings.leader;

        if self.mode == Mode::Normal {
            if chord == Some(leader) {
                self.set_mode(Mode::Wm);
                self.status = None;
            } else if let Some(p) = self.focused_pane()
                && let Some(bytes) = input::encode_key(&key, p.modes())
            {
                p.write(bytes);
            }
            return;
        }

        self.status = None;
        self.dirty = true;
        let Some(chord) = chord else {
            return;
        };
        let (action, exits) = match self.config.binds.get(&chord) {
            Some(bind) => (
                match &bind.action {
                    BindAction::Builtin(a) => Some(a.clone()),
                    BindAction::Lua(_) => None,
                },
                bind.exits_mode,
            ),
            // The leader pressed again goes through to the program, tmux style.
            None if chord == leader => (Some(Action::SendLeader), true),
            // Unbound keys are swallowed: WM mode is a mode, and typing into a pane
            // by accident while in it is worse than a dead key.
            None => {
                self.status = Some(format!("{chord} is not bound"));
                return;
            }
        };
        match action {
            Some(a) => self.run_action(a),
            None => self.status = Some("Lua binds run from milestone 2".into()),
        }
        if exits || !self.config.settings.wm_mode_sticky {
            self.set_mode(Mode::Normal);
        }
    }

    fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.dirty = true;
    }

    fn run_action(&mut self, action: Action) {
        let focused = self.focused;
        match action {
            Action::NewPane => {
                if let Err(e) = self.open_pane(None) {
                    self.status = Some(format!("new pane failed: {e:#}"));
                }
            }
            Action::Exec(cmd) => {
                if let Err(e) = self.open_pane(Some(&cmd)) {
                    self.status = Some(format!("exec failed: {e:#}"));
                }
            }
            Action::ClosePane => {
                if let Some(id) = focused {
                    self.close_pane(id);
                }
            }
            Action::Focus(dir) => {
                if let Some(id) = focused
                    && let Some(n) = layout::neighbour(&self.rects(), id, dir)
                {
                    self.fullscreen = false;
                    self.set_focus(Some(n));
                    self.relayout();
                }
            }
            Action::Move(dir) => {
                if let Some(id) = focused
                    && let Some(n) = layout::neighbour(&self.rects(), id, dir)
                {
                    self.tree.swap(id, n);
                    self.relayout();
                }
            }
            Action::Resize(dir, cells) => {
                if let Some(id) = focused {
                    let area = self.workspace_area();
                    let gap = self.config.theme.gaps.inner;
                    if self.tree.resize(id, dir, cells, area, gap) {
                        self.relayout();
                    }
                }
            }
            Action::ToggleSplit => {
                if let Some(id) = focused
                    && self.tree.toggle_split(id)
                {
                    self.relayout();
                }
            }
            Action::Fullscreen => {
                if focused.is_some() {
                    self.fullscreen = !self.fullscreen;
                    self.relayout();
                }
            }
            Action::ExitMode => {}
            Action::SendLeader => {
                let leader = self.config.settings.leader;
                if let Some(p) = self.focused_pane()
                    && let Some(bytes) = chord_bytes(leader, p.modes())
                {
                    p.write(bytes);
                }
            }
            Action::ReloadConfig => self.reload_config(),
            Action::Quit => self.quit = true,
            other => self.status = Some(format!("`{other}` arrives in a later milestone")),
        }
    }

    fn reload_config(&mut self) {
        match config::load(config::config_dir().as_deref()) {
            Ok(cfg) => {
                self.config = cfg;
                self.status = Some("config reloaded".into());
                self.relayout();
            }
            // Keep running on the old config; a typo must never take the session down.
            Err(e) => self.status = Some(format!("config error: {e:#}")),
        }
    }

    /// Clear per-pane wakeup flags once a frame has been drawn.
    fn drawn(&mut self) {
        for p in self.panes.values() {
            p.drawn();
        }
        self.dirty = false;
    }
}

/// The bytes a chord would have sent had it not been the leader.
fn chord_bytes(chord: crate::keys::Chord, modes: input::PaneModes) -> Option<Vec<u8>> {
    use crate::keys::Key;
    use crossterm::event::{KeyCode, KeyModifiers};
    let code = match chord.key {
        Key::Char(c) => KeyCode::Char(c),
        Key::Space => KeyCode::Char(' '),
        Key::Left => KeyCode::Left,
        Key::Right => KeyCode::Right,
        Key::Up => KeyCode::Up,
        Key::Down => KeyCode::Down,
        Key::Return => KeyCode::Enter,
        Key::Tab => KeyCode::Tab,
        Key::Backspace => KeyCode::Backspace,
        Key::Escape => KeyCode::Esc,
        Key::Delete => KeyCode::Delete,
        Key::Home => KeyCode::Home,
        Key::End => KeyCode::End,
        Key::PageUp => KeyCode::PageUp,
        Key::PageDown => KeyCode::PageDown,
        Key::F(n) => KeyCode::F(n),
    };
    let mut mods = KeyModifiers::NONE;
    mods.set(KeyModifiers::CONTROL, chord.mods.ctrl);
    mods.set(KeyModifiers::ALT, chord.mods.alt);
    mods.set(KeyModifiers::SHIFT, chord.mods.shift);
    input::encode_key(&KeyEvent::new(code, mods), modes)
}

fn spawn_input_thread(tx: Sender<AppEvent>) {
    std::thread::Builder::new()
        .name("input".into())
        .spawn(move || {
            loop {
                match crossterm::event::read() {
                    Ok(ev) => {
                        if tx.send(AppEvent::Input(ev)).is_err() {
                            return;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(AppEvent::InputClosed);
                        return;
                    }
                }
            }
        })
        .expect("spawning the input thread");
}

/// Run the window manager until the last pane closes or `quit`.
pub fn run(config: Config) -> Result<()> {
    let (tx, rx): (Sender<AppEvent>, Receiver<AppEvent>) = mpsc::channel();
    // ratatui::init sets raw mode and the alternate screen, and installs a panic
    // hook that restores the terminal before the panic message prints.
    let mut terminal = ratatui::try_init().context("setting up the terminal")?;
    let result = (|| -> Result<()> {
        execute!(
            terminal.backend_mut(),
            EnableBracketedPaste,
            EnableFocusChange
        )?;
        let size = terminal.size()?;
        let mut app = App::new(config, tx.clone(), size.width, size.height);
        app.open_pane(None).context("starting the first pane")?;
        spawn_input_thread(tx);

        let mut last_draw = Instant::now() - FRAME;
        let mut last_cursor: Option<CursorState> = None;
        loop {
            // Idle means blocked here with no timeout: zero frames, zero wakeups.
            let first = if app.dirty {
                match rx.recv_timeout(FRAME.saturating_sub(last_draw.elapsed())) {
                    Ok(ev) => Some(ev),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match rx.recv() {
                    Ok(ev) => Some(ev),
                    Err(_) => break,
                }
            };
            if let Some(ev) = first {
                app.handle(ev);
                while let Ok(ev) = rx.try_recv() {
                    app.handle(ev);
                }
            }
            if app.quit {
                break;
            }
            if app.dirty && last_draw.elapsed() >= FRAME {
                let mut cursor = None;
                terminal.draw(|f| cursor = render::draw(f, &app))?;
                if cursor != last_cursor {
                    if let Some(c) = cursor {
                        execute!(terminal.backend_mut(), cursor_style(c))?;
                    }
                    last_cursor = cursor;
                }
                app.drawn();
                last_draw = Instant::now();
            }
        }
        Ok(())
    })();

    let _ = execute!(
        terminal.backend_mut(),
        DisableBracketedPaste,
        DisableFocusChange,
        SetCursorStyle::DefaultUserShape
    );
    let _ = terminal.backend_mut().flush();
    ratatui::restore();
    result
}

fn cursor_style(c: CursorState) -> SetCursorStyle {
    match (c.shape, c.blinking) {
        (CursorShape::Beam, true) => SetCursorStyle::BlinkingBar,
        (CursorShape::Beam, false) => SetCursorStyle::SteadyBar,
        (CursorShape::Underline, true) => SetCursorStyle::BlinkingUnderScore,
        (CursorShape::Underline, false) => SetCursorStyle::SteadyUnderScore,
        (_, true) => SetCursorStyle::BlinkingBlock,
        (_, false) => SetCursorStyle::SteadyBlock,
    }
}
