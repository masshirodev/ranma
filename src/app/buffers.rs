//! Paste buffers (tmux's `choose-buffer` and `paste-buffer`): what was copied
//! lately, newest first, to paste again. Every copy ranma passes to the
//! terminal's clipboard is one: copy mode, hints, `ranma.copy`, and a
//! program's own OSC 52. Kept in memory only: a clipboard holds passwords,
//! and nothing here is worth a file.

use super::App;
use crate::input;
use crate::picker::{Item, Kind, Picker, Target};

/// How many copies are kept.
pub const KEPT: usize = 50;

/// `text` as the newest of `list`: an earlier copy of the same text moves
/// up instead of being kept twice, and the oldest go past `KEPT`.
pub fn remember(list: &mut Vec<String>, text: &str) {
    if text.is_empty() {
        return;
    }
    list.retain(|t| t != text);
    list.insert(0, text.to_string());
    list.truncate(KEPT);
}

/// One line for the picker: the text with its line breaks as `↵` and other
/// control characters left out, cut to `width` with `…`.
pub fn preview(text: &str, width: usize) -> String {
    let flat: String = text
        .trim()
        .chars()
        .filter_map(|c| match c {
            '\n' => Some('↵'),
            '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        })
        .collect();
    if flat.chars().count() <= width {
        return flat;
    }
    let mut cut: String = flat.chars().take(width.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

impl App {
    /// A copy went to the clipboard: keep it as the newest buffer.
    pub(super) fn remember_copy(&mut self, text: &str) {
        remember(&mut self.buffers, text);
    }

    /// `choose_buffer`: the buffers, newest first; Enter pastes one.
    pub(super) fn open_buffer_picker(&mut self) {
        if self.buffers.is_empty() {
            self.status = Some("nothing copied yet".into());
            return;
        }
        let items = self
            .buffers
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let lines = t.lines().count().max(1);
                Item {
                    label: preview(t, 60),
                    detail: if lines == 1 {
                        format!("{} chars", t.chars().count())
                    } else {
                        format!("{lines} lines")
                    },
                    target: Target::Run(format!("paste_buffer {}", i + 1)),
                    current: false,
                }
            })
            .collect();
        self.picker = Some(Picker::new(Kind::Buffers, "paste a copy", items));
        self.dirty = true;
    }

    /// `paste_buffer`: the `n`th newest copy (1 is the last), pasted into the
    /// focused pane as a paste, so bracketed paste guards it as any other.
    pub(super) fn paste_buffer(&mut self, n: usize) {
        let Some(text) = self.buffers.get(n.saturating_sub(1)).cloned() else {
            self.status = Some(if self.buffers.is_empty() {
                "nothing copied yet".into()
            } else {
                format!("only {} copies kept", self.buffers.len())
            });
            return;
        };
        self.typed(|modes| Some(input::encode_paste(&text, modes)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_are_kept_newest_first_once_each() {
        let mut l = Vec::new();
        for t in ["a", "b", "", "a"] {
            remember(&mut l, t);
        }
        assert_eq!(l, vec!["a", "b"]);
        for i in 0..80 {
            remember(&mut l, &i.to_string());
        }
        assert_eq!(l.len(), KEPT);
        assert_eq!(l[0], "79");
    }

    #[test]
    fn a_preview_is_one_line() {
        assert_eq!(preview("  ls -la\nexit\n", 40), "ls -la↵exit");
        assert_eq!(preview("abcdef", 4), "abc…");
        assert_eq!(preview("a\x1b[31mb", 9), "a[31mb");
    }
}
