//! One bar for nested ranmas, the window manager's side (the bar itself is
//! `nestbar`). Two directions:
//!
//! - **Inward**, as the outer ranma: a ranma in a pane asks at start whether a
//!   ranma draws around it (answered here), then reports its workspaces, which
//!   are kept per pane and drawn in this ranma's bar. A pane whose ranma
//!   reports loses its border's title, and its border when it fills the
//!   workspace: the ranma inside draws both.
//! - **Outward**, as the inner ranma: when the client said a ranma answered its
//!   question, this ranma reports its own workspaces (and what it heard from
//!   the ranmas inside it) on every change, and draws no bar while its terminal
//!   has focus, which is exactly while the outer bar shows its workspaces.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Frame, Mode, SCRATCHPAD, chord_bytes};
use crate::action::{Action, WorkspaceTarget};
use crate::bar::Segment;
use crate::config::{BindAction, ModuleNested, NestedMode};
use crate::layout::PaneId;
use crate::nestbar::{self, Report, Ws};

type PlaceFn = fn(&Report, &str, u16, PaneId) -> Option<(u16, usize, Segment)>;

/// A report as it goes into this ranma's own, under a workspace: without its
/// colours, which only the top level carries (`Report::colors`).
fn nest_copy(r: &Report) -> Box<Report> {
    Box::new(Report {
        colors: None,
        bar: None,
        ..r.clone()
    })
}

/// This ranma's own bar while another's is drawn (`follow_inner_bar`).
pub(super) struct OwnBar {
    layout: crate::config::BarLayout,
    fits: std::collections::HashMap<String, crate::config::ModuleFit>,
    styles: std::collections::HashMap<String, String>,
    look: Option<(
        crate::theme::Bar,
        std::collections::BTreeMap<String, crate::theme::RoleDef>,
        Option<toml::Table>,
    )>,
}

impl OwnBar {
    /// Put `w` in use in `config`, returning what it replaced.
    fn lend(config: &mut crate::config::Config, w: &nestbar::WireBar) -> OwnBar {
        use crate::config::{BarWidth, ModuleFit, TitleEntry};
        use nestbar::{WireEntry, WireWidth};
        let side = |e: &[WireEntry]| -> Vec<TitleEntry> {
            e.iter()
                .map(|e| match e {
                    WireEntry::Name(n) => TitleEntry::Piece(n.clone()),
                    WireEntry::Group {
                        pieces,
                        style,
                        priority,
                    } => TitleEntry::Group {
                        pieces: pieces.clone(),
                        style: style.clone(),
                        priority: *priority,
                    },
                })
                .collect()
        };
        let width = |w: WireWidth| match w {
            WireWidth::Cells(n) => BarWidth::Cells(n),
            WireWidth::Percent(p) => BarWidth::Percent(p),
        };
        let layout = crate::config::BarLayout {
            left: side(&w.left),
            center: side(&w.center),
            right: side(&w.right),
            // The bar's size is this screen's, as its position is.
            size: config.bar.size,
            center_min: w.center_min,
        };
        let mut fits = config.module_fits.clone();
        for (n, f) in &w.fits {
            fits.insert(
                n.clone(),
                ModuleFit {
                    priority: f.priority,
                    min_width: f.min.map(width),
                    max_width: f.max.map(width),
                },
            );
        }
        let mut styles = config.module_styles.clone();
        styles.extend(w.styles.iter().map(|(n, s)| (n.clone(), s.clone())));
        // The look, when this build reads it: else this ranma's own.
        let theirs = w
            .look
            .as_ref()
            .and_then(|v| serde_json::from_value::<toml::Table>(v.clone()).ok())
            .and_then(|t| crate::theme::bar_look(&t).map(|l| (l, t)));
        let look = theirs.map(|((mut bar, roles), source)| {
            bar.position = config.theme.bar.position;
            let mut merged = config.theme.roles.clone();
            merged.extend(roles);
            (
                std::mem::replace(&mut config.theme.bar, bar),
                std::mem::replace(&mut config.theme.roles, merged),
                config.theme.bar_source.replace(source),
            )
        });
        OwnBar {
            layout: std::mem::replace(&mut config.bar, layout),
            fits: std::mem::replace(&mut config.module_fits, fits),
            styles: std::mem::replace(&mut config.module_styles, styles),
            look,
        }
    }

    fn put_back(self, config: &mut crate::config::Config) {
        config.bar = self.layout;
        config.module_fits = self.fits;
        config.module_styles = self.styles;
        if let Some((bar, roles, source)) = self.look {
            config.theme.bar = bar;
            config.theme.roles = roles;
            config.theme.bar_source = source;
        }
    }
}

/// A pane's compact label, placed (see `App::nest_labels`).
#[derive(Debug, Clone, PartialEq)]
pub struct NestLabel {
    pub pane: PaneId,
    /// The blank cell before the label; the label starts one to the right.
    pub x: u16,
    pub y: u16,
    /// The step of the ladder it took.
    pub step: usize,
    pub pieces: Segment,
    /// On the border's edge, rather than over the pane's own row.
    pub on_border: bool,
}

impl App {
    /// The report of the ranma in a pane: only while one runs there (its
    /// title carries the mark), so a ranma that ended leaves nothing behind.
    pub(super) fn report_of(&self, id: PaneId) -> Option<&Report> {
        let p = self.panes.get(&id)?;
        if !p.hosts_ranma() {
            return None;
        }
        self.reports.get(&id)
    }

    /// How many workspaces the ranma in workspace `n`'s focused pane has in
    /// use, for the workspaces module to put after a collapsed holder. None
    /// with `nested = "off"`, where the bar says nothing of inner ranmas.
    pub(super) fn holder_in_use(&self, n: u8) -> Option<usize> {
        if self.config.workspaces_nested == crate::config::NestedWorkspaces::Off {
            return None;
        }
        let focused = self.workspaces.get(&n)?.focused?;
        nestbar::in_use(self.report_of(focused)?)
    }

    /// The same for the scratchpad, whose `S` holds a ranma when an `ssh`
    /// started there reaches one.
    pub(super) fn scratch_in_use(&self) -> Option<usize> {
        if self.config.workspaces_nested == crate::config::NestedWorkspaces::Off {
            return None;
        }
        nestbar::in_use(self.report_of(self.scratch.focused?)?)
    }

    /// Whether the ranma in this pane reports: its border then carries no
    /// title, since the ranma inside draws its own.
    pub fn reports_from(&self, id: PaneId) -> bool {
        self.report_of(id).is_some()
    }

    pub(super) fn nested_mark(&mut self, id: PaneId, m: crate::osc::Mark) {
        match m {
            crate::osc::Mark::RanmaHello => {
                // Answered only by a ranma that will show the workspaces: one
                // with a bar of its own, or one that passes them further out.
                let shows = self.config.theme.bar.position != crate::theme::BarPosition::Hidden
                    || self.client_outer;
                if self.config.settings.nested == NestedMode::Auto
                    && shows
                    && let Some(p) = self.panes.get(&id)
                {
                    // The colours first: they are read only up to the answer's
                    // DA1, and whatever it carries goes on to inner ranmas.
                    let mut answer = nestbar::colors_osc(&self.config.theme.colors);
                    answer.extend(nestbar::hello_reply());
                    p.write(answer);
                }
            }
            crate::osc::Mark::RanmaReport(json) => {
                match Report::parse(&json) {
                    Some(r) => {
                        if self.reports.get(&id) != Some(&r) {
                            self.reports.insert(id, r);
                            self.dirty = true;
                        }
                    }
                    // A protocol this build does not speak: as if none came.
                    None => {
                        if self.reports.remove(&id).is_some() {
                            self.dirty = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// This ranma as a report: what an outer ranma draws, and the top level
    /// of this ranma's own nested bar.
    pub(super) fn own_report(&self) -> Report {
        let keys = self.workspace_keys();
        let ws = self
            .workspace_list()
            .into_iter()
            .map(|(n, _current, occ, urgent, activity, name)| {
                let focused = self.workspaces.get(&n).and_then(|w| w.focused);
                Ws {
                    n,
                    name,
                    occ,
                    urgent,
                    activity,
                    key: keys.get(&n).cloned(),
                    nest: focused.and_then(|f| self.report_of(f)).map(nest_copy),
                }
            })
            .collect();
        let (scratch, scratch_shown) = self.scratch_state();
        Report {
            // In the outer's version when it is older: a version-1 outer
            // drops a report in any other.
            v: match self.outer_v {
                0 => nestbar::PROTOCOL,
                v => v.min(nestbar::PROTOCOL),
            },
            session: self.session_name().to_string(),
            sessions: self.session_count(),
            current: if scratch_shown {
                SCRATCHPAD
            } else {
                self.current
            },
            accent: self.session_accent().and_then(|c| self.rgb_of(c)),
            scratch,
            scratch_shown,
            scratch_key: self.bound_key(|a| *a == Action::ScratchpadToggle),
            scratch_nest: self
                .scratch
                .focused
                .and_then(|f| self.report_of(f))
                .map(nest_copy),
            mode: match (self.mode, self.hints.is_some()) {
                (Mode::Wm, _) => "wm",
                (Mode::Copy, _) => {
                    if self
                        .copy
                        .as_ref()
                        .and_then(|c| c.search.as_ref())
                        .is_some_and(|s| s.editing)
                    {
                        "search"
                    } else {
                        "copy"
                    }
                }
                (Mode::Normal, true) => "link",
                (Mode::Normal, false) => "normal",
            }
            .into(),
            mode_label: match self.mode {
                Mode::Wm => self.user_mode_label().map(str::to_string),
                _ => None,
            },
            status: self.status.clone(),
            status_error: self.status.is_some() && self.status == self.error_status,
            bar: Some(self.wire_bar()),
            outer_leader: Some(self.config.settings.outer_leader.to_string()),
            sticky: self.config.settings.wm_mode_sticky,
            ws,
            panes: self
                .strip_panes()
                .into_iter()
                .map(|(_, label, focused)| nestbar::Chip { label, focused })
                .collect(),
            modules: self
                .config
                .bar
                .all()
                .filter(|n| self.config.modules.contains_key(*n))
                .map(|n| {
                    let seg = self.module_value(n);
                    (n.clone(), nestbar::to_wire(&seg, &self.config.theme.roles))
                })
                .collect(),
            // A module showing an error takes a click too: it opens the log.
            clicks: {
                let mut c: Vec<String> = self
                    .click_modules()
                    .into_iter()
                    .chain(self.erred_modules())
                    .filter(|n| self.config.bar.all().any(|b| b == n))
                    .collect();
                c.sort();
                c.dedup();
                c
            },
            click_token: self.click_token.clone(),
            actions: crate::action::FORWARDED
                .iter()
                .map(|a| a.to_string())
                .collect(),
            colors: self
                .focused()
                .and_then(|f| self.report_of(f))
                .and_then(|r| r.colors.clone())
                .or_else(|| {
                    let own = self
                        .own_colors
                        .as_ref()
                        .unwrap_or(&self.config.theme.colors);
                    serde_json::to_value(own).ok()?.as_object().cloned()
                }),
        }
    }

    /// This ranma's bar as it travels outward (card c464). What is in use
    /// is what goes: a ranma drawing the bar of one inside it passes that
    /// one's on, so a chain carries the innermost's.
    fn wire_bar(&self) -> nestbar::WireBar {
        use crate::config::{BarWidth, TitleEntry};
        use nestbar::{WireEntry, WireFit, WireWidth};
        let side = |entries: &[TitleEntry]| -> Vec<WireEntry> {
            entries
                .iter()
                .map(|e| match e {
                    TitleEntry::Piece(n) => WireEntry::Name(n.clone()),
                    TitleEntry::Group {
                        pieces,
                        style,
                        priority,
                    } => WireEntry::Group {
                        pieces: pieces.clone(),
                        style: style.clone(),
                        priority: *priority,
                    },
                })
                .collect()
        };
        let width = |w: BarWidth| match w {
            BarWidth::Cells(n) => WireWidth::Cells(n),
            BarWidth::Percent(p) => WireWidth::Percent(p),
        };
        let bar = &self.config.bar;
        nestbar::WireBar {
            left: side(&bar.left),
            center: side(&bar.center),
            right: side(&bar.right),
            center_min: bar.center_min,
            fits: self
                .config
                .module_fits
                .iter()
                .map(|(n, f)| {
                    let fit = WireFit {
                        priority: f.priority,
                        min: f.min_width.map(width),
                        max: f.max_width.map(width),
                    };
                    (n.clone(), fit)
                })
                .collect(),
            styles: self
                .config
                .module_styles
                .iter()
                .map(|(n, s)| (n.clone(), s.clone()))
                .collect(),
            look: self
                .config
                .theme
                .bar_source
                .as_ref()
                .and_then(|t| serde_json::to_value(t).ok()),
            own: Some({
                let mut own: Vec<String> = bar
                    .all()
                    .filter(|n| {
                        self.config
                            .modules
                            .get(*n)
                            .is_some_and(|m| m.nested == ModuleNested::Own)
                    })
                    .cloned()
                    .collect();
                own.sort();
                own.dedup();
                own
            }),
        }
    }

    /// Put this ranma's own bar back before its theme or settings are
    /// replaced, so nothing is restored over the new ones later; the next
    /// round lends the inner's again if it is still asked for.
    pub(super) fn return_bar(&mut self) {
        if let Some(own) = self.own_bar.take() {
            own.put_back(&mut self.config);
            self.bar_changed();
        }
        self.inner_bar = None;
    }

    /// Another bar is in use: the modules it shows tick now (a timed one is
    /// scheduled only while a bar shows it, and the one that came in was
    /// never scheduled: the desk's clock in a host's bar stayed blank), and
    /// the state-driven ones render (card c468).
    fn bar_changed(&mut self) {
        self.schedule_modules(std::time::Instant::now());
        self.render_state_modules();
        self.dirty = true;
    }

    /// Under `nested_bar = "inner"`, draw the bar of the ranma the focus is
    /// in (the innermost on the focus path that sends one), this one's own
    /// again once the focus leaves it (card c464). Kept: this ranma's bar
    /// position and size, its workspaces (that ranma's in brackets), its
    /// mode chip, and its modules' values where they are `nested = "own"`.
    /// Called once per round of events, beside `follow_inner_colors`.
    pub fn follow_inner_bar(&mut self) {
        let want = (self.config.settings.nested_bar == crate::config::NestedBar::Inner)
            .then(|| self.nested_path().iter().rev().find_map(|r| r.bar.clone()))
            .flatten();
        if want == self.inner_bar {
            return;
        }
        if let Some(own) = self.own_bar.take() {
            own.put_back(&mut self.config);
        }
        if let Some(w) = &want {
            self.own_bar = Some(OwnBar::lend(&mut self.config, w));
        }
        self.inner_bar = want;
        self.bar_changed();
    }

    /// The report of the ranma a toolbar button acts on instead of this one:
    /// the one in the focused pane, while it takes forwarded actions and the
    /// toolbar follows the focus (`toolbar_target`).
    pub(super) fn toolbar_inner(&self) -> Option<&Report> {
        if self.toolbar_here {
            return None;
        }
        self.report_of(self.focused()?)
            .filter(|r| !r.click_token.is_empty() && !r.actions.is_empty())
    }

    /// What to write into the focused pane so the ranma there runs a
    /// toolbar button's `action`, or `None` when this one runs it: the
    /// toolbar acts here, the action is not one a toolbar sends in, or the
    /// ranma there does not take it (older, or not listing it).
    pub(super) fn toolbar_action_bytes(&self, action: &Action) -> Option<Vec<u8>> {
        if !action.forwarded() {
            return None;
        }
        let r = self.toolbar_inner()?;
        let name = action.name();
        r.actions
            .contains(&name)
            .then(|| nestbar::action_paste(&r.click_token, &action.to_string()))
    }

    /// A toolbar button's action: sent into the ranma in the focused pane
    /// when the toolbar follows the focus there, else run here.
    pub(super) fn run_toolbar_action(&mut self, action: Action) {
        if let Some(bytes) = self.toolbar_action_bytes(&action)
            && let Some(p) = self.focused().and_then(|f| self.panes.get(&f))
        {
            p.write(bytes);
            return;
        }
        self.run_action(action);
    }

    /// A click on a bar module, or `module_click`: the ranma whose value it
    /// shows acts on it. While the value comes from the ranma in the focused
    /// pane, and that ranma says it takes the click, it is sent through as a
    /// paste carrying its token (`nestbar::click_paste`), and it does the
    /// same for a ranma inside it in turn. Otherwise this one's `on_click`
    /// runs: an older ranma inside, or a module it does not click.
    pub(super) fn click_module(&mut self, name: &str) {
        if let Some(bytes) = self.module_click_bytes(name)
            && let Some(p) = self.focused().and_then(|f| self.panes.get(&f))
        {
            p.write(bytes);
            return;
        }
        // Showing an error: the click opens the log, where it is in full,
        // as it would on this ranma's own bar (a click from an outer one).
        if self.erred_modules().iter().any(|m| m == name) {
            self.run_action(Action::Logs);
            return;
        }
        let hook = self
            .config
            .modules
            .get(name)
            .and_then(|m| m.on_click.clone());
        if let Some(k) = hook
            && let Ok(f) = self.config.lua.registry_value::<mlua::Function>(&k)
        {
            self.call_lua(|_| f.call::<()>(()));
        }
    }

    /// The modules whose value here is an error (`Click::Logs` on it).
    pub(super) fn erred_modules(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .module_values
            .iter()
            .filter(|(_, seg)| seg.iter().any(|p| p.click == Some(crate::bar::Click::Logs)))
            .map(|(n, _)| n.clone())
            .collect();
        names.sort();
        names
    }

    /// What to write into the focused pane so the ranma there runs `action`,
    /// when it reports one that takes it; else `None`.
    pub(super) fn inner_action_bytes(&self, action: &Action) -> Option<Vec<u8>> {
        let r = self.report_of(self.focused()?)?;
        (!r.click_token.is_empty() && r.actions.contains(&action.name()))
            .then(|| nestbar::action_paste(&r.click_token, &action.to_string()))
    }

    /// What to write into the focused pane so the ranma there clicks `name`,
    /// or `None` when this ranma clicks it itself.
    pub(super) fn module_click_bytes(&self, name: &str) -> Option<Vec<u8>> {
        if self.config.modules.get(name)?.nested == ModuleNested::Own {
            return None;
        }
        let r = self.report_of(self.focused()?)?;
        if r.click_token.is_empty()
            || !r.modules.contains_key(name)
            || !r.clicks.iter().any(|c| c == name)
        {
            return None;
        }
        Some(nestbar::click_paste(&r.click_token, name))
    }

    /// A module's value as this ranma draws it: from the innermost ranma on
    /// the focus path that reports one of that name, unless the module keeps
    /// its own (`nested = "own"`); blank for `"hide"` when none does. What
    /// is drawn is also what this ranma reports, so a chain of three carries
    /// the innermost's value all the way out.
    pub(super) fn module_value(&self, name: &str) -> Segment {
        let own = || self.module_values.get(name).cloned().unwrap_or_default();
        let nested = match self.inner_bar.as_ref().and_then(|b| b.own.as_ref()) {
            // A host's bar in use: its marks decide (card c472). A desk
            // value it asks for that this ranma has no module for is the
            // host's own.
            Some(own)
                if own.iter().any(|n| n == name) && self.config.modules.contains_key(name) =>
            {
                ModuleNested::Own
            }
            Some(_) => ModuleNested::Follow,
            None => self
                .config
                .modules
                .get(name)
                .map_or(ModuleNested::Follow, |m| m.nested),
        };
        if nested == ModuleNested::Own {
            return own();
        }
        let path = self.nested_path();
        if path.is_empty() {
            return own();
        }
        match path.iter().rev().find_map(|r| r.modules.get(name)) {
            Some(w) => nestbar::from_wire(w, &self.config.theme.roles),
            None if nested == ModuleNested::Hide => Vec::new(),
            None => own(),
        }
    }

    /// The key each workspace is bound to in WM mode, as a bind spells it.
    fn workspace_keys(&self) -> std::collections::HashMap<u8, String> {
        let mut keys = std::collections::HashMap::new();
        for (chord, b) in &self.config.binds {
            if let BindAction::Builtin(Action::Workspace(WorkspaceTarget::Index(n))) = &b.action {
                keys.entry(*n)
                    .and_modify(|k: &mut String| {
                        let c = chord.to_string();
                        if c.len() < k.len() {
                            *k = c;
                        }
                    })
                    .or_insert_with(|| chord.to_string());
            }
        }
        keys
    }

    fn bound_key(&self, is: impl Fn(&Action) -> bool) -> Option<String> {
        self.config
            .binds
            .iter()
            .filter(|(_, b)| matches!(&b.action, BindAction::Builtin(a) if is(a)))
            .map(|(c, _)| c.to_string())
            .min_by_key(|k| k.len())
    }

    /// A theme colour as RGB, through the host's palette for an indexed one.
    fn rgb_of(&self, c: crate::theme::Color) -> Option<[u8; 3]> {
        match c {
            crate::theme::Color::Rgb(r, g, b) => Some([r, g, b]),
            crate::theme::Color::Indexed(i) => {
                self.host_colors.get(i as usize).map(|c| [c.r, c.g, c.b])
            }
            crate::theme::Color::Default => None,
        }
    }

    /// Send this ranma's report outward when it changed, if a ranma around
    /// it answered. Called once per round of events, before output is flushed.
    pub fn report_outward(&mut self) {
        if !self.client_outer || self.config.settings.nested != NestedMode::Auto {
            return;
        }
        let mut r = self.own_report();
        // An outer drops an OSC past its reader's limit whole, workspaces and
        // all; the modules are the part a bar can do without.
        if nestbar::report_osc(&r).len() > crate::osc::MAX {
            r.modules.clear();
        }
        if nestbar::report_osc(&r).len() > crate::osc::MAX {
            r.colors = None;
        }
        // Then the bar's look, then the bar: the outer draws its own.
        if nestbar::report_osc(&r).len() > crate::osc::MAX
            && let Some(b) = r.bar.as_mut()
        {
            b.look = None;
        }
        if nestbar::report_osc(&r).len() > crate::osc::MAX {
            r.bar = None;
        }
        if self.last_report.as_ref() != Some(&r) {
            // The title goes again with every report: the outer takes a
            // report only while the pane's title carries the mark, and the
            // title is otherwise sent only when it changes, so one the outer
            // lost (overwritten there) left every report after it ignored
            // until the focused pane's title happened to change. Seen
            // 2026-11-05 over ssh: the workspaces gone from the outer bar
            // until another terminal attached and the redraw re-sent it.
            if !self.host_title.is_empty() {
                self.host_out
                    .push(format!("\x1b]2;{}\x07", self.host_title).into_bytes());
            }
            self.host_out.push(nestbar::report_osc(&r));
            self.last_report = Some(r);
        }
    }

    /// A (new) terminal is showing this ranma: whether a ranma around it
    /// answered, in a protocol this build speaks. The report goes out afresh.
    pub fn set_outer(&mut self, outer: Option<u32>) {
        let was = self.bar_yielded();
        self.client_outer = outer.is_some_and(nestbar::speaks);
        self.outer_v = outer.filter(|v| nestbar::speaks(*v)).unwrap_or(0);
        // A terminal attaching is one being typed in: focused until it says.
        if !self.host_focused {
            self.host_focused = true;
            self.emit(crate::config::Event::ClientFocus, |t| {
                t.set("focused", true)
            });
        }
        self.last_report = None;
        if self.bar_yielded() != was {
            self.relayout();
        }
    }

    /// A (new) terminal is showing this ranma, and the ranma around it sent
    /// these colours (or none, for a plain terminal or an older ranma).
    pub fn set_outer_colors(&mut self, colors: Option<serde_json::Map<String, serde_json::Value>>) {
        self.outer_colors = colors;
        self.adopt_colors();
    }

    /// Draw with another ranma's colours when the setting asks for them and
    /// that ranma sent some (the outer's at attach, the focused inner's in
    /// its report), else with the theme's own: laid over them, so a role the
    /// other did not send keeps this theme's colour.
    pub(super) fn adopt_colors(&mut self) {
        use crate::config::ThemeColors;
        if let Some(own) = self.own_colors.take() {
            self.config.theme.colors = own;
        }
        let wire = match self.config.settings.theme_colors {
            ThemeColors::Own => None,
            ThemeColors::Outer => self.outer_colors.as_ref(),
            ThemeColors::Inner => self.inner_colors.as_ref(),
        };
        if let Some(theirs) = wire.map(|w| self.config.theme.colors.overlay(w)) {
            self.own_colors = Some(std::mem::replace(&mut self.config.theme.colors, theirs));
        }
        self.dirty = true;
    }

    /// Under `theme_colors = "inner"`, take the colours of the ranma in the
    /// focused pane when they changed: focus moved onto or off a pane
    /// holding one, or its report brought others (its own focus moved
    /// deeper). Called once per round of events, beside `report_outward`.
    pub fn follow_inner_colors(&mut self) {
        if self.config.settings.theme_colors != crate::config::ThemeColors::Inner {
            self.inner_colors = None;
            return;
        }
        let now = self
            .focused()
            .and_then(|f| self.report_of(f))
            .and_then(|r| r.colors.as_ref());
        if now == self.inner_colors.as_ref() {
            return;
        }
        self.inner_colors = now.cloned();
        self.adopt_colors();
    }

    /// This ranma draws no bar of its own: an outer one shows its workspaces,
    /// and does so right now (this terminal has focus there).
    pub fn bar_yielded(&self) -> bool {
        self.client_outer && self.config.settings.nested == NestedMode::Auto
    }

    /// The bar is drawn over the bottom row instead of beside the panes: a
    /// ranma around this one shows the workspaces while this terminal has
    /// focus, and keeping the row free means focus coming and going never
    /// resizes the panes. Only under an outer of protocol 1: a newer one
    /// labels this pane's border while it is not focused, so this ranma draws
    /// no bar at all.
    pub fn bar_overlaid(&self) -> bool {
        self.bar_yielded() && !self.host_focused && self.outer_v < nestbar::EDGE_SINCE
    }

    /// The compact labels of the panes whose ranma reports but is not on the
    /// focus path, so this bar does not show its workspaces: on the edge of
    /// the pane's border nearest the bar, or, with no border, over the end of
    /// the pane's own row there. Drawing and clicks both read this.
    pub fn nest_labels(&self, frame: &Frame) -> Vec<NestLabel> {
        let top = self.config.theme.bar.position == crate::theme::BarPosition::Top;
        frame
            .views
            .iter()
            .filter(|v| !v.focused)
            .filter_map(|v| {
                let r = self
                    .report_of(v.id)
                    .filter(|r| r.v >= nestbar::EDGE_SINCE)?;
                let host = self.nest_host(v.id);
                let d = v.drawn();
                let on_border = if top { d.top } else { d.bottom };
                let (rect, place) = if on_border {
                    (v.outer, nestbar::on_edge as PlaceFn)
                } else {
                    (v.inner, nestbar::in_row as PlaceFn)
                };
                if rect.h == 0 {
                    return None;
                }
                let (dx, step, pieces) = place(r, &host, rect.w, v.id)?;
                Some(NestLabel {
                    pane: v.id,
                    x: rect.x + dx,
                    y: if top { rect.y } else { rect.bottom() - 1 },
                    step,
                    pieces,
                    on_border,
                })
            })
            .collect()
    }

    /// What a pane's label calls the ranma in it: the connection's name, as
    /// its workspace would be named (`ssh pc` is `pc`), else the host that
    /// ranma says it is on.
    fn nest_host(&self, id: PaneId) -> String {
        self.programs
            .get(&id)
            .cloned()
            .or_else(|| self.panes.get(&id)?.inner_host().map(str::to_string))
            .unwrap_or_else(|| "ranma".into())
    }

    /// What a click at `x`, `y` hits on a label: its pane, and the workspace
    /// for a workspace's piece.
    pub(super) fn nest_label_at(
        &self,
        frame: &Frame,
        x: u16,
        y: u16,
    ) -> Option<(PaneId, Option<u8>)> {
        // Side by side, two labels share a row; a float over one hides it.
        let top = self.pane_at(frame, x, y)?.id;
        let l = self
            .nest_labels(frame)
            .into_iter()
            .find(|l| l.y == y && l.pane == top)?;
        let mut px = l.x + 1;
        for p in &l.pieces {
            let w = unicode_width::UnicodeWidthStr::width(p.text.as_str()) as u16;
            if x >= px && x < px + w {
                return match p.click {
                    Some(crate::bar::Click::InPane { pane, n }) => Some((pane, Some(n))),
                    _ => Some((l.pane, None)),
                };
            }
            px += w;
        }
        None
    }

    /// A click on a label: focus its pane, then go to the workspace (or
    /// just focus, for anything but a workspace).
    pub(super) fn click_in_pane(&mut self, pane: PaneId, n: Option<u8>) {
        self.active_mut().fullscreen = false;
        self.focus(pane);
        self.relayout();
        if let Some(n) = n {
            self.reach_nested(pane, &[n]);
        }
    }

    /// The pane that is drawn without a border: one whose ranma reports and
    /// that fills the workspace shown (alone there, or fullscreen), so the
    /// ranma inside draws the only frame.
    /// The scratchpad shown over it changes nothing: framing the pane then
    /// resized the ranma inside, and the layout under the scratchpad jumped
    /// as it opened and again as it closed.
    pub(super) fn frameless(&self) -> Option<PaneId> {
        let ws = self.workspaces.get(&self.current)?;
        // A short touch screen gives the one tile on screen no border: the
        // strip's chips carry its title (see `chrome`).
        if self.chrome().borderless
            && let Some(id) = self.lone_tile(ws)
        {
            return Some(id);
        }
        let id = ws.focused?;
        self.report_of(id)?;
        let alone = ws.fullscreen
            || (ws.floating.is_empty() && ws.tree.panes().len() == 1 && ws.tree.contains(id));
        alone.then_some(id)
    }

    /// The deepest ranma on the path of the focused pane, and every one
    /// between: whose mode and messages the bar says.
    pub(super) fn nested_path(&self) -> Vec<&Report> {
        let mut out = Vec::new();
        let mut r = self.focused().and_then(|f| self.report_of(f));
        while let Some(rep) = r {
            out.push(rep);
            r = rep.on_path();
        }
        out
    }

    /// A click on a workspace inside a nested ranma: show its holder, then
    /// type what reaches it. The outer leader once puts the ranma in the pane
    /// in WM mode; each time more goes one level down (DESIGN.md, "ranma
    /// inside ranma"); then that level's key for the workspace, then Esc.
    /// The holder may be the scratchpad (0): it is shown, never toggled away.
    pub(super) fn click_nested(&mut self, holder: u8, path: &[u8]) {
        let pane = if holder == SCRATCHPAD {
            if !self.scratch_shown {
                self.run_action(Action::ScratchpadToggle);
            }
            self.scratch.focused
        } else {
            self.run_action(Action::Workspace(WorkspaceTarget::Index(holder)));
            self.workspaces.get(&holder).and_then(|w| w.focused)
        };
        let Some(pane) = pane else {
            return;
        };
        self.reach_nested(pane, path);
    }

    /// Type into `pane` what takes the ranma there (and the ones inside it)
    /// down `path` to a workspace.
    fn reach_nested(&mut self, pane: PaneId, path: &[u8]) {
        let mut level = self.report_of(pane).cloned();
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let mut bytes = Vec::new();
        let modes = match self.panes.get(&pane) {
            Some(p) => p.modes(),
            None => return,
        };
        for (depth, n) in path.iter().enumerate() {
            let Some(r) = level else { break };
            let key = if *n == SCRATCHPAD {
                r.scratch_key.clone()
            } else {
                r.ws.iter().find(|w| w.n == *n).and_then(|w| w.key.clone())
            };
            let outer = r
                .outer_leader
                .as_deref()
                .and_then(|k| k.parse::<crate::keys::Chord>().ok());
            let (Some(key), Some(outer)) = (key.and_then(|k| k.parse().ok()), outer) else {
                break;
            };
            for _ in 0..=depth {
                bytes.extend(chord_bytes(outer, modes).unwrap_or_default());
            }
            bytes.extend(chord_bytes(key, modes).unwrap_or_default());
            if r.sticky {
                bytes.extend(crate::input::encode_key(&esc, modes).unwrap_or_default());
            }
            level = r.nest_of(*n).cloned();
        }
        if let Some(p) = self.panes.get(&pane)
            && !bytes.is_empty()
        {
            p.write(bytes);
        }
    }
}
