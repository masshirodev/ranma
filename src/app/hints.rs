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
        let links = hints::find_links(&rows);
        if links.is_empty() {
            self.status = Some("no links on screen".into());
            return;
        }
        let labels = hints::labels(links.len());
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
            let (target, open) = (link.target.clone(), h.open);
            self.hints = None;
            self.pick_link(&target, open);
        } else if !h.links.iter().any(|(l, _)| l.starts_with(h.typed.as_str())) {
            self.status = Some(format!("no link labelled {}", h.typed));
            self.hints = None;
        }
    }

    /// Copy the link to the clipboard, or open it. Opening runs `xdg-open`
    /// where the server runs, which is only where you are when the terminal
    /// did not come over SSH; from afar it copies instead and says why.
    fn pick_link(&mut self, target: &str, open: bool) {
        if open && !self.client_remote {
            let spawned = std::process::Command::new("sh")
                .args([
                    "-c",
                    "setsid xdg-open \"$1\" >/dev/null 2>&1 </dev/null &",
                    "sh",
                    target,
                ])
                .status();
            self.status = Some(match spawned {
                Ok(s) if s.success() => format!("opening {target}"),
                _ => format!("could not run xdg-open for {target}"),
            });
            return;
        }
        self.set_host_clipboard(target);
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
                    (ch, cell.hyperlink().map(|h| h.uri().to_string()))
                })
                .collect();
            Row {
                cells,
                wrapped: row[Column(cols - 1)].flags.contains(Flags::WRAPLINE),
            }
        })
        .collect()
}
