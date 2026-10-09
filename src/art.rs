//! Backgrounds: text art drawn behind the panes (the theme's `[background]`).
//!
//! Art is a text file, plain or coloured with SGR escapes, which is what
//! `chafa`, `jp2a --colors`, `lolcat -f` and `toilet --gay` write. It is
//! parsed once, when the theme loads, into cells; drawing copies cells and
//! reads nothing. Only SGR is understood (colours, bold, dim, italic,
//! underline, reverse); any other escape is dropped, so a cursor movement in
//! the file cannot move anything.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use unicode_width::UnicodeWidthChar;

use crate::theme::Color;

/// One cell of art. `None` colours are the background's own (`colors.
/// background_fg`, `background_bg`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cell {
    pub ch: char,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
}

impl Cell {
    /// Nothing to draw: a space on no ground of its own, which leaves
    /// whatever is under it (the terminal's ground, or the theme's).
    pub fn is_clear(&self) -> bool {
        self.ch == ' ' && self.bg.is_none() && !self.reverse
    }
}

/// Parsed art: rows of cells, as wide as its widest row. A wide character
/// takes its cell and the next, which is left as `'\0'`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Art {
    pub rows: Vec<Vec<Cell>>,
    pub width: usize,
}

/// Art that ships with ranma, by name. Files with the same name win.
pub const BUILTIN: [(&str, &str); 3] = [
    ("dots", "·   \n  · \n"),
    ("grid", "┼───\n│   \n"),
    ("waves", " ~^~  \n~   ~^\n"),
];

impl Art {
    pub fn parse(src: &str) -> Art {
        let mut rows: Vec<Vec<Cell>> = Vec::new();
        let mut row: Vec<Cell> = Vec::new();
        let mut pen = Cell::default();
        let mut chars = src.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\x1b' => {
                    if chars.peek() != Some(&'[') {
                        // Not a CSI: skip the next character and go on.
                        chars.next();
                        continue;
                    }
                    chars.next();
                    let mut params = String::new();
                    let mut end = None;
                    for ch in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&ch) {
                            end = Some(ch);
                            break;
                        }
                        params.push(ch);
                    }
                    if end == Some('m') {
                        sgr(&mut pen, &params);
                    }
                }
                '\n' => rows.push(std::mem::take(&mut row)),
                '\r' => {}
                '\t' => {
                    for _ in 0..(8 - row.len() % 8) {
                        row.push(Cell {
                            ch: ' ',
                            ..pen.clone()
                        });
                    }
                }
                c if c.is_control() => {}
                c => {
                    let w = c.width().unwrap_or(0);
                    if w == 0 {
                        continue;
                    }
                    row.push(Cell {
                        ch: c,
                        ..pen.clone()
                    });
                    if w == 2 {
                        row.push(Cell {
                            ch: '\0',
                            ..pen.clone()
                        });
                    }
                }
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
        // Blank rows at the end are the file's last newline, not art.
        while rows.last().is_some_and(|r| r.iter().all(Cell::is_clear)) {
            rows.pop();
        }
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        Art { rows, width }
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    /// The cell at (`x`, `y`) of the art, if it has one there.
    pub fn at(&self, x: usize, y: usize) -> Option<&Cell> {
        self.rows.get(y)?.get(x)
    }

    /// Art by name from `dirs` (each a `backgrounds` directory, looked in in
    /// order, `<name>.txt`), else a built-in; a name with a `/` or starting
    /// with `~` is a path.
    pub fn load(name: &str, dirs: &[PathBuf]) -> Result<Art> {
        let path = if name.contains('/') || name.starts_with('~') {
            Some(expand(name))
        } else {
            dirs.iter()
                .map(|d| d.join(format!("{name}.txt")))
                .find(|p| p.is_file())
        };
        let art = match path {
            Some(p) => Art::parse(
                &std::fs::read_to_string(&p)
                    .with_context(|| format!("background `{}`", p.display()))?,
            ),
            None => match BUILTIN.iter().find(|(n, _)| *n == name) {
                Some((_, src)) => Art::parse(src),
                None => bail!(
                    "background `{name}`: no {name}.txt in {} and no built-in of that name ({})",
                    dirs.iter()
                        .map(|d| d.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    BUILTIN.map(|(n, _)| n).join(", ")
                ),
            },
        };
        if art.width == 0 {
            bail!("background `{name}` is empty");
        }
        Ok(art)
    }
}

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => dirs::home_dir()
            .map(|h| h.join(rest))
            .unwrap_or_else(|| Path::new(p).to_path_buf()),
        None => PathBuf::from(p),
    }
}

/// Apply one SGR sequence's parameters to the pen.
fn sgr(pen: &mut Cell, params: &str) {
    let nums: Vec<u16> = if params.is_empty() {
        vec![0]
    } else {
        params
            .split([';', ':'])
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    };
    let mut i = 0;
    while i < nums.len() {
        let n = nums[i];
        match n {
            0 => *pen = Cell::default(),
            1 => pen.bold = true,
            2 => pen.dim = true,
            3 => pen.italic = true,
            4 => pen.underline = true,
            7 => pen.reverse = true,
            22 => (pen.bold, pen.dim) = (false, false),
            23 => pen.italic = false,
            24 => pen.underline = false,
            27 => pen.reverse = false,
            30..=37 => pen.fg = Some(Color::Indexed((n - 30) as u8)),
            90..=97 => pen.fg = Some(Color::Indexed((n - 90 + 8) as u8)),
            40..=47 => pen.bg = Some(Color::Indexed((n - 40) as u8)),
            100..=107 => pen.bg = Some(Color::Indexed((n - 100 + 8) as u8)),
            39 => pen.fg = None,
            49 => pen.bg = None,
            38 | 48 => {
                let (c, used) = extended(&nums[i + 1..]);
                if n == 38 {
                    pen.fg = c;
                } else {
                    pen.bg = c;
                }
                i += used;
            }
            _ => {}
        }
        i += 1;
    }
}

/// `5;n` or `2;r;g;b` after a 38 or 48: the colour, and how many numbers it took.
fn extended(rest: &[u16]) -> (Option<Color>, usize) {
    match rest {
        [5, n, ..] => (Some(Color::Indexed(*n as u8)), 2),
        [2, r, g, b, ..] => (Some(Color::Rgb(*r as u8, *g as u8, *b as u8)), 4),
        _ => (None, rest.len()),
    }
}

/// Where the art's cell for screen cell (`x`, `y`) of an area `w` x `h` is,
/// placed by `align`: `None` where the art does not reach.
pub fn place(
    art: &Art,
    align: crate::theme::BackgroundAlign,
    w: u16,
    h: u16,
    x: u16,
    y: u16,
) -> Option<(usize, usize)> {
    use crate::theme::BackgroundAlign as A;
    let (aw, ah) = (art.width as i32, art.height() as i32);
    if aw == 0 || ah == 0 {
        return None;
    }
    if align == A::Tile {
        return Some(((x as i32 % aw) as usize, (y as i32 % ah) as usize));
    }
    let (w, h) = (w as i32, h as i32);
    let left = match align {
        A::TopLeft | A::Left | A::BottomLeft => 0,
        A::TopRight | A::Right | A::BottomRight => w - aw,
        _ => (w - aw) / 2,
    };
    let top = match align {
        A::TopLeft | A::Top | A::TopRight => 0,
        A::BottomLeft | A::Bottom | A::BottomRight => h - ah,
        _ => (h - ah) / 2,
    };
    let (ax, ay) = (x as i32 - left, y as i32 - top);
    ((0..aw).contains(&ax) && (0..ah).contains(&ay)).then_some((ax as usize, ay as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::BackgroundAlign;

    #[test]
    fn colours_and_attributes_from_sgr() {
        let a = Art::parse("\x1b[1;31ma\x1b[0mb\x1b[38;2;1;2;3;48;5;200mc\x1b[39md\n");
        assert_eq!(a.width, 4);
        let r = &a.rows[0];
        assert_eq!(
            (r[0].ch, r[0].fg, r[0].bold),
            ('a', Some(Color::Indexed(1)), true)
        );
        assert_eq!((r[1].fg, r[1].bold), (None, false));
        assert_eq!(r[2].fg, Some(Color::Rgb(1, 2, 3)));
        assert_eq!(r[2].bg, Some(Color::Indexed(200)));
        assert_eq!((r[3].fg, r[3].bg), (None, Some(Color::Indexed(200))));
    }

    #[test]
    fn other_escapes_and_trailing_blank_rows_are_dropped() {
        let a = Art::parse("x\x1b[2Jy\x1b]0;title\x07z\n\n   \n");
        assert_eq!(a.height(), 1);
        let text: String = a.rows[0].iter().map(|c| c.ch).collect();
        assert!(text.starts_with("xy"), "{text}");
        let wide = Art::parse("日x");
        assert_eq!(wide.width, 3);
        assert_eq!(wide.rows[0][1].ch, '\0');
    }

    #[test]
    fn art_is_placed_or_tiled() {
        let a = Art::parse("ab\ncd\n");
        let at = |al, x, y| place(&a, al, 10, 6, x, y);
        assert_eq!(at(BackgroundAlign::Center, 4, 2), Some((0, 0)));
        assert_eq!(at(BackgroundAlign::Center, 3, 2), None);
        assert_eq!(at(BackgroundAlign::BottomRight, 9, 5), Some((1, 1)));
        assert_eq!(at(BackgroundAlign::TopLeft, 0, 0), Some((0, 0)));
        assert_eq!(at(BackgroundAlign::Tile, 5, 3), Some((1, 1)));
    }

    #[test]
    fn names_are_files_first_then_built_ins() {
        let dir = std::env::temp_dir().join(format!("ranma-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dots.txt"), "mine\n").unwrap();
        let mine = Art::load("dots", std::slice::from_ref(&dir)).unwrap();
        assert_eq!(mine.width, 4);
        assert_eq!(mine.rows[0][0].ch, 'm');
        assert!(Art::load("waves", std::slice::from_ref(&dir)).is_ok());
        let e = Art::load("nope", std::slice::from_ref(&dir)).unwrap_err();
        assert!(format!("{e:#}").contains("no nope.txt"), "{e:#}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
