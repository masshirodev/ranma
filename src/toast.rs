//! Toasts: short notifications stacked in a corner, gone after a few seconds.
//!
//! They come from Lua (`ranma.toast`), from `ranma notify` run in any pane (see
//! `ipc`), and from ranma itself (a bell in a pane nobody can see). They never
//! take focus or keys; a click dismisses one.

use std::time::{Duration, Instant};

use unicode_width::UnicodeWidthChar;

use crate::layout::Rect;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
/// More than this and the oldest go first: a flood of toasts should not bury
/// the screen.
pub const MAX_SHOWN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Normal,
    Urgent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    pub level: Level,
    pub expires: Instant,
}

#[derive(Debug, Default)]
pub struct Toasts {
    list: Vec<Toast>,
    next_id: u64,
}

impl Toasts {
    pub fn push(&mut self, text: impl Into<String>, level: Level, timeout: Duration, now: Instant) {
        let text: String = text.into();
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        self.next_id += 1;
        self.list.push(Toast {
            id: self.next_id,
            text: text.to_string(),
            level,
            expires: now + timeout,
        });
        if self.list.len() > MAX_SHOWN {
            self.list.remove(0);
        }
    }

    /// Drop expired toasts; true if any went.
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.list.len();
        self.list.retain(|t| t.expires > now);
        self.list.len() != before
    }

    pub fn next_expiry(&self) -> Option<Instant> {
        self.list.iter().map(|t| t.expires).min()
    }

    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.list.len();
        self.list.retain(|t| t.id != id);
        self.list.len() != before
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Where each toast goes on a screen of `screen`, newest on top, stacked down
    /// the right edge from `top`. Returns each toast with its box and its lines.
    pub fn layout(&self, screen: Rect, top: u16) -> Vec<(&Toast, Rect, Vec<String>)> {
        let max_w = (screen.w * 2 / 5)
            .clamp(20, 60)
            .min(screen.w.saturating_sub(2));
        let inner_max = max_w.saturating_sub(4) as usize;
        let mut y = top;
        let mut out = Vec::new();
        for t in self.list.iter().rev() {
            let lines = wrap(&t.text, inner_max.max(1), 3);
            let text_w = lines.iter().map(|l| width(l)).max().unwrap_or(0) as u16;
            let w = (text_w + 4).max(20).min(max_w);
            let h = lines.len() as u16 + 2;
            if y + h > screen.bottom() {
                break;
            }
            let x = screen.right().saturating_sub(w + 1);
            out.push((t, Rect::new(x, y, w, h), lines));
            y += h;
        }
        out
    }
}

fn width(s: &str) -> usize {
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// Word-wrap to `cols`, at most `max_lines`, the last one cut with an ellipsis
/// if text remains. Words longer than a line are broken.
pub fn wrap(text: &str, cols: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0;
    let flush = |lines: &mut Vec<String>, cur: &mut String, cur_w: &mut usize| {
        lines.push(std::mem::take(cur));
        *cur_w = 0;
    };
    for word in text.split_whitespace() {
        let ww = width(word);
        if cur_w > 0 && cur_w + 1 + ww > cols {
            flush(&mut lines, &mut cur, &mut cur_w);
        }
        if cur_w > 0 {
            cur.push(' ');
            cur_w += 1;
        }
        for c in word.chars() {
            let cw = c.width().unwrap_or(0);
            if cur_w + cw > cols {
                flush(&mut lines, &mut cur, &mut cur_w);
            }
            cur.push(c);
            cur_w += cw;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        let last = lines.last_mut().expect("max_lines > 0");
        while width(last) + 1 > cols {
            last.pop();
        }
        last.push('…');
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect {
        x: 0,
        y: 0,
        w: 100,
        h: 30,
    };

    #[test]
    fn toasts_expire_and_report_the_next_expiry() {
        let now = Instant::now();
        let mut t = Toasts::default();
        t.push("a", Level::Normal, Duration::from_secs(1), now);
        t.push("b", Level::Urgent, Duration::from_secs(5), now);
        assert_eq!(t.next_expiry(), Some(now + Duration::from_secs(1)));
        assert!(t.expire(now + Duration::from_secs(2)));
        assert!(!t.expire(now + Duration::from_secs(2)));
        assert_eq!(t.layout(SCREEN, 0).len(), 1);
        assert!(t.expire(now + Duration::from_secs(6)));
        assert!(t.is_empty());
    }

    #[test]
    fn empty_text_is_not_a_toast_and_old_ones_make_room() {
        let now = Instant::now();
        let mut t = Toasts::default();
        t.push("   ", Level::Normal, DEFAULT_TIMEOUT, now);
        assert!(t.is_empty());
        for i in 0..8 {
            t.push(format!("t{i}"), Level::Normal, DEFAULT_TIMEOUT, now);
        }
        let texts: Vec<&str> = t
            .layout(SCREEN, 0)
            .iter()
            .map(|(t, _, _)| t.text.as_str())
            .collect();
        assert_eq!(texts, ["t7", "t6", "t5", "t4", "t3"]);
    }

    #[test]
    fn layout_stacks_newest_first_at_the_right_edge() {
        let now = Instant::now();
        let mut t = Toasts::default();
        t.push("first", Level::Normal, DEFAULT_TIMEOUT, now);
        t.push("second", Level::Normal, DEFAULT_TIMEOUT, now);
        let l = t.layout(SCREEN, 1);
        assert_eq!(l[0].0.text, "second");
        assert_eq!(l[0].1.y, 1);
        assert_eq!(l[1].1.y, 1 + l[0].1.h);
        assert_eq!(l[0].1.right(), SCREEN.right() - 1);
        let id = l[1].0.id;
        assert!(t.dismiss(id));
        assert_eq!(t.layout(SCREEN, 1).len(), 1);
    }

    #[test]
    fn wrapping() {
        assert_eq!(wrap("build done in 3s", 10, 3), ["build done", "in 3s"]);
        assert_eq!(wrap("abcdefghijkl", 5, 3), ["abcde", "fghij", "kl"]);
        let cut = wrap("one two three four five six seven", 9, 2);
        assert_eq!(cut.len(), 2);
        assert!(cut[1].ends_with('…'));
        assert_eq!(wrap("日本語テキスト", 6, 3), ["日本語", "テキス", "ト"]);
    }
}
