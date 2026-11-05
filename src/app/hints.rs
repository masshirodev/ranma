//! URL hints on the focused pane (`leader o`): labels over every link on
//! screen; typing a label copies that link, typing it in capitals opens it.
//! Finding the links is `crate::hints`; this is the state and the keys.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use crossterm::event::{KeyCode, KeyEvent};

use super::App;
use crate::hints::{self, Link, Row};
use crate::layout::PaneId;

pub struct HintState {
    pub pane: PaneId,
    /// Each link with its label.
    pub links: Vec<(String, Link)>,
    /// What has been typed of a label so far.
    pub typed: String,
    /// The label was typed in capitals: open the link instead of copying it.
    open: bool,
}

impl App {
    pub fn hint_state(&self) -> Option<&HintState> {
        self.hints.as_ref()
    }

    pub(super) fn enter_hints(&mut self) {
        let Some(id) = self.focused() else {
            return;
        };
        let Some(pane) = self.panes.get(&id) else {
            return;
        };
        let rows = visible_rows(pane);
        let s = &self.config.settings;
        let patterns: Vec<regex::Regex> = self
            .config
            .hint_patterns
            .iter()
            .map(|p| p.regex.clone())
            .collect();
        let links = hints::find_targets(
            &rows,
            &hints::Targets {
                urls: s.hint_urls,
                patterns: &patterns,
            },
        );
        if links.is_empty() {
            self.status = Some("no links on screen".into());
            return;
        }
        let labels = hints::labels_in(links.len(), &s.hint_alphabet);
        self.hints = Some(HintState {
            pane: id,
            links: labels.into_iter().zip(links).collect(),
            typed: String::new(),
            open: false,
        });
        self.dirty = true;
    }

    pub(super) fn exit_hints(&mut self) {
        if self.hints.take().is_some() {
            self.dirty = true;
        }
    }

    pub(super) fn hint_key(&mut self, key: &KeyEvent) {
        let Some(h) = self.hints.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => return self.exit_hints(),
            KeyCode::Backspace => {
                h.typed.pop();
                if h.typed.is_empty() {
                    h.open = false;
                }
            }
            KeyCode::Char(c) if c.is_alphabetic() => {
                if c.is_uppercase() {
                    h.open = true;
                }
                h.typed.extend(c.to_lowercase());
            }
            _ => return,
        }
        self.dirty = true;
        let h = self.hints.as_ref().expect("checked above");
        if let Some((_, link)) = h.links.iter().find(|(l, _)| *l == h.typed) {
            let (target, open, pane) = (link.target.clone(), h.open, h.pane);
            let pattern = link.pattern;
            self.hints = None;
            match pattern
                .and_then(|k| self.config.hint_patterns.get(k))
                .map(|p| p.action.clone())
            {
                Some(action) if !open => self.pick_pattern(&target, action, pane),
                _ => self.pick_link(&target, open, pane),
            }
        } else if !h.links.iter().any(|(l, _)| l.starts_with(h.typed.as_str())) {
            self.status = Some(format!("no link labelled {}", h.typed));
            self.hints = None;
        }
    }

    /// A pattern's hint, picked in lower case: what its `action` says.
    fn pick_pattern(&mut self, target: &str, action: crate::config::HintAction, pane: PaneId) {
        use crate::config::HintAction;
        match action {
            HintAction::Copy => self.pick_link(target, false, pane),
            HintAction::Open => self.pick_link(target, true, pane),
            HintAction::Paste => {
                self.focus(pane);
                self.paste(target.to_string());
            }
            HintAction::Lua(key) => {
                if let Ok(f) = self.config.lua.registry_value::<mlua::Function>(&key) {
                    let text = target.to_string();
                    self.call_lua(|lua| {
                        let t = lua.create_table()?;
                        t.set("text", text)?;
                        t.set("pane", pane)?;
                        f.call::<()>(t)
                    });
                }
            }
        }
    }

    /// Copy the link to the clipboard, or open it. Opening runs `xdg-open`
    /// (`open` on macOS) where the server runs, which is only where you are when the terminal
    /// did not come over SSH; from afar it copies instead and says why.
    fn pick_link(&mut self, target: &str, open: bool, pane: PaneId) {
        if open && !self.client_remote {
            let mut cmd = std::process::Command::new("sh");
            cmd.args(["-c", crate::os::OPEN_URL_SH, "sh", target]);
            crate::wslinterop::apply(&mut cmd);
            let spawned = cmd.status();
            self.status = Some(match spawned {
                Ok(s) if s.success() => format!("opening {target}"),
                _ => format!("could not run {} for {target}", crate::os::OPEN_URL_PROGRAM),
            });
            return;
        }
        self.set_host_clipboard(target, "hints", Some(pane));
        self.status = Some(if open {
            format!("copied {target} (opening would happen on the server, not here)")
        } else {
            format!("copied {target}")
        });
    }
}

/// The rows on screen as `hints` reads them: characters, OSC 8 targets, and
/// whether each row wraps into the next.
fn visible_rows(pane: &crate::pane::Pane) -> Vec<Row> {
    let term = pane.term.lock();
    let grid = term.grid();
    let offset = grid.display_offset() as i32;
    let cols = term.columns();
    (0..term.screen_lines() as i32)
        .map(|l| {
            let row = &grid[Line(l - offset)];
            // One entry per cell, so a link's column is its screen column. The
            // second half of a wide character is a NUL, which ends a URL.
            let cells = (0..cols)
                .map(|c| {
                    let cell = &row[Column(c)];
                    let ch = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                        '\0'
                    } else {
                        cell.c
                    };
                    let link = cell
                        .hyperlink()
                        .map(|h| h.uri().to_string())
                        .filter(|u| crate::osc::is_users_link(u));
                    (ch, link)
                })
                .collect();
            Row {
                cells,
                wrapped: row[Column(cols - 1)].flags.contains(Flags::WRAPLINE),
            }
        })
        .collect()
}
