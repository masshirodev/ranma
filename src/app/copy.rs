//! Copy mode and search: vi keys over a pane's scrollback, selections, `/`.
//!
//! alacritty_terminal has a vi mode of its own: a cursor independent of the
//! program's, motions (words, brackets, screen positions), selections that follow
//! that cursor, and a regex search over the whole grid. Copy mode is a keymap over
//! it plus the clipboard: a yank goes to the host terminal as OSC 52, which kitty,
//! foot, wezterm, alacritty and tmux all accept.

use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vi_mode::ViMotion;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{App, Mode};
use crate::layout::PaneId;
use crate::pane::Proxy;

pub struct CopyState {
    pub pane: PaneId,
    pub search: Option<Search>,
    /// Copy mode was entered by `search`: cancelling the search leaves it entirely.
    entered_by_search: bool,
    /// Matches in the visible rows, for highlighting; the current one separately.
    pub hits: Vec<Match>,
    pub current: Option<Match>,
}

pub struct Search {
    pub query: String,
    regex: Option<RegexSearch>,
    /// The prompt is open and keys edit the query.
    pub editing: bool,
    /// Searching up, towards older output.
    pub backward: bool,
    /// Where the search started; each edit of the query searches from here again.
    origin: Point,
    /// The query compiled but matched nothing, or did not compile.
    pub failed: bool,
}

impl App {
    pub fn copy_state(&self) -> Option<&CopyState> {
        self.copy.as_ref()
    }

    /// Enter copy mode on the focused pane; with `search`, open the search prompt
    /// (`true` searches backward, towards older output).
    pub(super) fn enter_copy_mode(&mut self, search: Option<bool>) {
        let Some(id) = self.focused() else {
            return;
        };
        let Some(pane) = self.panes.get(&id) else {
            return;
        };
        let origin = {
            let mut term = pane.term.lock();
            if !term.mode().contains(TermMode::VI) {
                term.toggle_vi_mode();
            }
            term.vi_mode_cursor.point
        };
        let already = self.copy.as_ref().is_some_and(|c| c.pane == id);
        if !already {
            self.copy = Some(CopyState {
                pane: id,
                search: None,
                entered_by_search: search.is_some(),
                hits: Vec::new(),
                current: None,
            });
        }
        if let (Some(backward), Some(c)) = (search, self.copy.as_mut()) {
            c.search = Some(Search {
                query: String::new(),
                regex: None,
                editing: true,
                backward,
                origin,
                failed: false,
            });
            c.current = None;
            c.hits.clear();
        }
        self.mode = Mode::Copy;
        self.dirty = true;
    }

    /// Leave copy mode: drop the vi cursor and selection, back to the live screen.
    pub(super) fn exit_copy_mode(&mut self) {
        let Some(c) = self.copy.take() else {
            return;
        };
        if let Some(pane) = self.panes.get(&c.pane) {
            let mut term = pane.term.lock();
            if term.mode().contains(TermMode::VI) {
                term.toggle_vi_mode();
            }
            term.selection = None;
            term.scroll_display(Scroll::Bottom);
        }
        if self.mode == Mode::Copy {
            self.mode = Mode::Normal;
        }
        self.dirty = true;
    }

    pub(super) fn copy_key(&mut self, key: &KeyEvent) {
        let Some(c) = self.copy.as_ref() else {
            self.mode = Mode::Normal;
            return;
        };
        let (id, editing) = (c.pane, c.search.as_ref().is_some_and(|s| s.editing));
        if !self.panes.contains_key(&id) {
            self.exit_copy_mode();
            return;
        }
        self.dirty = true;
        if editing {
            self.search_prompt_key(id, key);
        } else {
            self.vi_key(id, key);
        }
        self.refresh_hits();
    }

    fn search_prompt_key(&mut self, id: PaneId, key: &KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let c = self.copy.as_mut().expect("checked");
        let s = c.search.as_mut().expect("editing");
        match key.code {
            KeyCode::Esc => {
                if c.entered_by_search {
                    self.exit_copy_mode();
                } else {
                    // Back to where the search started, still in copy mode.
                    let origin = s.origin;
                    c.search = None;
                    c.current = None;
                    if let Some(p) = self.panes.get(&id) {
                        p.term.lock().vi_goto_point(origin);
                    }
                }
                return;
            }
            KeyCode::Char('c' | 'g') if ctrl => {
                self.exit_copy_mode();
                return;
            }
            KeyCode::Enter => {
                s.editing = false;
                // An empty or failed search has nothing to stay on.
                if s.failed || s.query.is_empty() {
                    if c.entered_by_search {
                        self.exit_copy_mode();
                    } else {
                        c.search = None;
                    }
                }
                return;
            }
            KeyCode::Backspace => {
                s.query.pop();
            }
            KeyCode::Char('u') if ctrl => s.query.clear(),
            KeyCode::Char(ch) if !ctrl => s.query.push(ch),
            _ => return,
        }
        self.search_from_origin(id);
    }

    /// Re-run the search from where it started: typing narrows the match in place
    /// instead of walking away from it.
    fn search_from_origin(&mut self, id: PaneId) {
        let Some(pane) = self.panes.get(&id) else {
            return;
        };
        let c = self.copy.as_mut().expect("in copy mode");
        let s = c.search.as_mut().expect("searching");
        s.regex = if s.query.is_empty() {
            None
        } else {
            RegexSearch::new(&s.query).ok()
        };
        let mut term = pane.term.lock();
        let Some(regex) = s.regex.as_mut() else {
            s.failed = !s.query.is_empty();
            c.current = None;
            term.vi_goto_point(s.origin);
            return;
        };
        let (dir, side) = direction(s.backward);
        c.current = term.search_next(regex, s.origin, dir, side, None);
        s.failed = c.current.is_none();
        match &c.current {
            Some(m) => term.vi_goto_point(*m.start()),
            None => term.vi_goto_point(s.origin),
        }
    }

    /// `n` / `N`: the next match in the search's direction, or the other way.
    fn search_again(&mut self, id: PaneId, reverse: bool) {
        let Some(pane) = self.panes.get(&id) else {
            return;
        };
        let c = self.copy.as_mut().expect("in copy mode");
        let Some(s) = c.search.as_mut() else {
            return;
        };
        let Some(regex) = s.regex.as_mut() else {
            return;
        };
        let backward = s.backward != reverse;
        let mut term = pane.term.lock();
        let from = term.vi_mode_cursor.point;
        // Step off the current match first, or the search finds it again.
        let origin = if backward {
            from.sub(&*term, Boundary::None, 1)
        } else {
            from.add(&*term, Boundary::None, 1)
        };
        let (dir, side) = direction(backward);
        if let Some(m) = term.search_next(regex, origin, dir, side, None) {
            term.vi_goto_point(*m.start());
            c.current = Some(m);
            s.failed = false;
        } else {
            s.failed = true;
        }
    }

    fn vi_key(&mut self, id: PaneId, key: &KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let motion = match key.code {
            KeyCode::Char('h') | KeyCode::Left => Some(ViMotion::Left),
            KeyCode::Char('j') | KeyCode::Down => Some(ViMotion::Down),
            KeyCode::Char('k') | KeyCode::Up => Some(ViMotion::Up),
            KeyCode::Char('l') | KeyCode::Right => Some(ViMotion::Right),
            KeyCode::Char('w') if !ctrl => Some(ViMotion::SemanticRight),
            KeyCode::Char('b') if !ctrl => Some(ViMotion::SemanticLeft),
            KeyCode::Char('e') => Some(ViMotion::SemanticRightEnd),
            KeyCode::Char('W') => Some(ViMotion::WordRight),
            KeyCode::Char('B') => Some(ViMotion::WordLeft),
            KeyCode::Char('E') => Some(ViMotion::WordRightEnd),
            KeyCode::Char('0') | KeyCode::Home => Some(ViMotion::First),
            KeyCode::Char('$') | KeyCode::End => Some(ViMotion::Last),
            KeyCode::Char('^') => Some(ViMotion::FirstOccupied),
            KeyCode::Char('H') => Some(ViMotion::High),
            KeyCode::Char('M') => Some(ViMotion::Middle),
            KeyCode::Char('L') => Some(ViMotion::Low),
            KeyCode::Char('%') => Some(ViMotion::Bracket),
            _ => None,
        };
        let pane = self.panes.get(&id).expect("checked");
        if let Some(m) = motion {
            let mut term = pane.term.lock();
            term.vi_motion(m);
            let p = term.vi_mode_cursor.point;
            term.scroll_to_point(p);
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.exit_copy_mode(),
            KeyCode::Char('c') if ctrl => self.exit_copy_mode(),
            KeyCode::Char('y') | KeyCode::Enter => self.yank(id),
            KeyCode::Char('/') => self.enter_copy_mode(Some(false)),
            KeyCode::Char('?') => self.enter_copy_mode(Some(true)),
            KeyCode::Char('n') if !ctrl => self.search_again(id, false),
            KeyCode::Char('N') => self.search_again(id, true),
            KeyCode::Char('v') if ctrl => {
                toggle_selection(&mut pane.term.lock(), SelectionType::Block)
            }
            KeyCode::Char('v') => toggle_selection(&mut pane.term.lock(), SelectionType::Simple),
            KeyCode::Char('V') => toggle_selection(&mut pane.term.lock(), SelectionType::Lines),
            KeyCode::Char('g') => {
                let mut term = pane.term.lock();
                let top = Point::new(term.topmost_line(), Column(0));
                term.vi_goto_point(top);
            }
            KeyCode::Char('G') => {
                let mut term = pane.term.lock();
                let bottom = Point::new(term.bottommost_line(), Column(0));
                term.vi_goto_point(bottom);
            }
            KeyCode::Char('u' | 'b') | KeyCode::PageUp if ctrl || key.code == KeyCode::PageUp => {
                page(&mut pane.term.lock(), key, true)
            }
            KeyCode::Char('d' | 'f') | KeyCode::PageDown
                if ctrl || key.code == KeyCode::PageDown =>
            {
                page(&mut pane.term.lock(), key, false)
            }
            _ => {}
        }
    }

    /// Copy the selection (or the current search match, when nothing is
    /// selected) to the host clipboard, and leave copy mode.
    fn yank(&mut self, id: PaneId) {
        let text = {
            let pane = self.panes.get(&id).expect("checked");
            let mut term = pane.term.lock();
            if term.selection.is_none()
                && let Some(m) = self.copy.as_ref().and_then(|c| c.current.clone())
            {
                let mut sel = Selection::new(SelectionType::Simple, *m.start(), Side::Left);
                sel.update(*m.end(), Side::Right);
                term.selection = Some(sel);
            }
            term.selection_to_string()
        };
        match text.filter(|t| !t.is_empty()) {
            Some(t) => {
                let n = t.chars().count();
                self.set_host_clipboard(&t);
                self.status = Some(format!(
                    "copied {n} character{}",
                    if n == 1 { "" } else { "s" }
                ));
            }
            None => self.status = Some("nothing selected (v starts a selection)".into()),
        }
        self.exit_copy_mode();
    }

    /// Recompute the matches in the visible rows, for highlighting.
    pub(super) fn refresh_hits(&mut self) {
        let Some(c) = self.copy.as_mut() else {
            return;
        };
        c.hits.clear();
        let Some(pane) = self.panes.get(&c.pane) else {
            return;
        };
        let Some(regex) = c.search.as_mut().and_then(|s| s.regex.as_mut()) else {
            return;
        };
        let term = pane.term.lock();
        let offset = term.grid().display_offset() as i32;
        let start = Point::new(Line(-offset), Column(0));
        let end = Point::new(
            Line(-offset + term.screen_lines() as i32 - 1),
            Column(term.columns().saturating_sub(1)),
        );
        // A screenful of matches is plenty; a pathological regex should not be
        // able to stall a keypress.
        c.hits = RegexIter::new(start, end, Direction::Right, &term, regex)
            .take(500)
            .collect();
    }

    /// The wheel in copy mode moves the view and keeps the vi cursor in it.
    pub(super) fn copy_scroll(&mut self, lines: i32) {
        let Some(c) = self.copy.as_ref() else {
            return;
        };
        if let Some(p) = self.panes.get(&c.pane) {
            let mut term = p.term.lock();
            term.scroll_display(Scroll::Delta(lines));
            let motion = if lines > 0 {
                ViMotion::High
            } else {
                ViMotion::Low
            };
            let offset = term.grid().display_offset() as i32;
            let cur = term.vi_mode_cursor.point.line.0;
            let (top, bottom) = (-offset, -offset + term.screen_lines() as i32 - 1);
            if cur < top || cur > bottom {
                term.vi_motion(motion);
            }
        }
        self.refresh_hits();
        self.dirty = true;
    }

    /// Queue an OSC 52 write setting the host's clipboard.
    pub(super) fn set_host_clipboard(&mut self, text: &str) {
        let mut seq = b"\x1b]52;c;".to_vec();
        seq.extend_from_slice(base64(text.as_bytes()).as_bytes());
        seq.push(0x07);
        self.host_out.push(seq);
    }
}

fn direction(backward: bool) -> (Direction, Side) {
    if backward {
        (Direction::Left, Side::Right)
    } else {
        (Direction::Right, Side::Left)
    }
}

/// v / V / Ctrl+v: start a selection of that type at the vi cursor, or drop it if
/// one of that type is already there (as in vim).
fn toggle_selection(term: &mut Term<Proxy>, ty: SelectionType) {
    if term.selection.as_ref().is_some_and(|s| s.ty == ty) {
        term.selection = None;
        return;
    }
    let p = term.vi_mode_cursor.point;
    term.selection = Some(Selection::new(ty, p, Side::Left));
    // Include the cell under the cursor, as vim does.
    if let Some(sel) = term.selection.as_mut() {
        sel.update(p, Side::Right);
    }
}

/// Half a page with Ctrl+u / Ctrl+d, a whole one with Ctrl+b / Ctrl+f and the
/// page keys: the view and the cursor move together.
fn page(term: &mut Term<Proxy>, key: &KeyEvent, up: bool) {
    let rows = term.screen_lines() as i32;
    let n = match key.code {
        KeyCode::Char('u' | 'd') => rows / 2,
        _ => rows.saturating_sub(1),
    }
    .max(1);
    term.scroll_display(Scroll::Delta(if up { n } else { -n }));
    let mut p = term.vi_mode_cursor.point;
    p.line = Line(
        (p.line.0 + if up { -n } else { n }).clamp(term.topmost_line().0, term.bottommost_line().0),
    );
    term.vi_goto_point(p);
}

/// Standard base64 with padding, for OSC 52.
pub fn base64(input: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64("日本".as_bytes()), "5pel5pys");
    }
}
