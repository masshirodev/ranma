//! A pane's screen as text, for handing a server over to a new build (see
//! DESIGN.md, "Upgrading a server in place").
//!
//! Not the emulator's own structures: those are large and belong to a crate
//! that may change between the two builds. The main screen and its history are
//! written out as what a program would have printed to draw them: characters,
//! colour and attribute changes, rows that wrapped left to wrap again, a line
//! break where one ended. Fed to a fresh emulator of the same size, that draws
//! the same thing, and the history ends up in its scrollback as it was. Then
//! the cursor, the modes the program asked for and its palette changes, as the
//! escape sequences that set them.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// The bytes that draw the main screen and its history, cursor last.
    pub screen: String,
    /// The bytes that put the modes and palette back (and the alternate
    /// screen, empty, for the program there to draw again).
    pub modes: String,
}

/// DEC private modes a program sets, with the terminal mode each one is.
const DEC_MODES: [(u16, TermMode); 11] = [
    (1, TermMode::APP_CURSOR),
    (6, TermMode::ORIGIN),
    (7, TermMode::LINE_WRAP),
    (25, TermMode::SHOW_CURSOR),
    (1000, TermMode::MOUSE_REPORT_CLICK),
    (1002, TermMode::MOUSE_DRAG),
    (1003, TermMode::MOUSE_MOTION),
    (1004, TermMode::FOCUS_IN_OUT),
    (1006, TermMode::SGR_MOUSE),
    (1007, TermMode::ALTERNATE_SCROLL),
    (2004, TermMode::BRACKETED_PASTE),
];

/// Take a pane's screen. A program on the alternate screen is left on it, but
/// the alternate screen comes back blank: a server that does not leave after
/// all asks that program to draw again (`pty::redraw`).
pub fn take<T: alacritty_terminal::event::EventListener>(term: &mut Term<T>) -> Snapshot {
    let mode = *term.mode();
    let alt = mode.contains(TermMode::ALT_SCREEN);
    if alt {
        // The shell's screen is behind the program's; what the program had
        // drawn it draws again (SIGWINCH), the shell's is only here.
        term.swap_alt();
    }
    let screen = draw_grid(term);
    let mut modes = String::new();
    for (n, m) in DEC_MODES {
        // Only what differs from a fresh terminal: line wrap and the cursor
        // are on there, the rest off.
        let on = mode.contains(m);
        let default_on = matches!(n, 7 | 25);
        if on != default_on {
            modes.push_str(&format!("\x1b[?{n}{}", if on { 'h' } else { 'l' }));
        }
    }
    if mode.contains(TermMode::APP_KEYPAD) {
        modes.push_str("\x1b=");
    }
    let colors = term.colors();
    for i in 0..256usize {
        if let Some(c) = colors[i] {
            modes.push_str(&format!(
                "\x1b]4;{i};rgb:{:02x}/{:02x}/{:02x}\x1b\\",
                c.r, c.g, c.b
            ));
        }
    }
    for (osc, named) in [
        (10, NamedColor::Foreground),
        (11, NamedColor::Background),
        (12, NamedColor::Cursor),
    ] {
        if let Some(c) = colors[named] {
            modes.push_str(&format!(
                "\x1b]{osc};rgb:{:02x}/{:02x}/{:02x}\x1b\\",
                c.r, c.g, c.b
            ));
        }
    }
    if alt {
        modes.push_str("\x1b[?1049h");
        term.swap_alt();
    }
    Snapshot { screen, modes }
}

/// The main grid and its history as the bytes that draw them.
fn draw_grid<T>(term: &Term<T>) -> String {
    let grid = term.grid();
    let (rows, cols) = (term.screen_lines() as i32, term.columns());
    let history = grid.history_size() as i32;
    let mut out = String::from("\x1b[0m");
    let mut pen = Pen::default();
    for l in -history..rows {
        let row = &grid[Line(l)];
        let wrapped = row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
        // Trailing blank cells (default colours, no attributes) are not
        // drawn, unless the row wraps: then the next row must start where it
        // did, at the next column.
        let end = if wrapped {
            cols
        } else {
            (0..cols)
                .rev()
                .find(|c| !blank(&row[Column(*c)]))
                .map_or(0, |c| c + 1)
        };
        for c in 0..end {
            let cell = &row[Column(c)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            pen.change_to(cell, &mut out);
            out.push(if cell.c.is_control() { ' ' } else { cell.c });
            if let Some(extra) = cell.zerowidth() {
                out.extend(extra);
            }
        }
        if !wrapped && l + 1 < rows {
            // Colours off before the break, so nothing is painted by it.
            pen.change_to(&Cell::default(), &mut out);
            out.push_str("\r\n");
        }
    }
    pen.change_to(&Cell::default(), &mut out);
    let cursor = grid.cursor.point;
    out.push_str(&format!(
        "\x1b[{};{}H",
        cursor.line.0 + 1,
        cursor.column.0 + 1
    ));
    out
}

fn blank(cell: &Cell) -> bool {
    cell.c == ' '
        && cell.bg == Color::Named(NamedColor::Background)
        && cell.fg == Color::Named(NamedColor::Foreground)
        && (cell.flags - Flags::WRAPLINE).is_empty()
        && cell.zerowidth().is_none()
}

/// The attributes last written, so only changes are.
#[derive(Default)]
struct Pen {
    fg: Option<Color>,
    bg: Option<Color>,
    flags: Option<Flags>,
}

const STYLE_FLAGS: [(Flags, &str); 7] = [
    (Flags::BOLD, "1"),
    (Flags::DIM, "2"),
    (Flags::ITALIC, "3"),
    (Flags::UNDERLINE, "4"),
    (Flags::INVERSE, "7"),
    (Flags::HIDDEN, "8"),
    (Flags::STRIKEOUT, "9"),
];

impl Pen {
    fn change_to(&mut self, cell: &Cell, out: &mut String) {
        let flags = cell.flags
            & (Flags::BOLD
                | Flags::DIM
                | Flags::ITALIC
                | Flags::UNDERLINE
                | Flags::INVERSE
                | Flags::HIDDEN
                | Flags::STRIKEOUT);
        if self.fg == Some(cell.fg) && self.bg == Some(cell.bg) && self.flags == Some(flags) {
            return;
        }
        // One full SGR per change: a reset, then everything that is on.
        let mut sgr = vec!["0".to_string()];
        for (f, code) in STYLE_FLAGS {
            if flags.contains(f) {
                sgr.push(code.into());
            }
        }
        if let Some(s) = color_sgr(cell.fg, false) {
            sgr.push(s);
        }
        if let Some(s) = color_sgr(cell.bg, true) {
            sgr.push(s);
        }
        out.push_str(&format!("\x1b[{}m", sgr.join(";")));
        self.fg = Some(cell.fg);
        self.bg = Some(cell.bg);
        self.flags = Some(flags);
    }
}

fn color_sgr(c: Color, bg: bool) -> Option<String> {
    let base = if bg { 40 } else { 30 };
    Some(match c {
        Color::Spec(rgb) => format!("{};2;{};{};{}", base + 8, rgb.r, rgb.g, rgb.b),
        Color::Indexed(i) => format!("{};5;{i}", base + 8),
        Color::Named(n) => {
            let i = n as usize;
            if i < 8 {
                format!("{}", base + i)
            } else if i < 16 {
                format!("{}", base + 60 + i - 8)
            } else {
                match n {
                    NamedColor::DimBlack
                    | NamedColor::DimRed
                    | NamedColor::DimGreen
                    | NamedColor::DimYellow
                    | NamedColor::DimBlue
                    | NamedColor::DimMagenta
                    | NamedColor::DimCyan
                    | NamedColor::DimWhite => {
                        format!("{}", base + (n as usize - NamedColor::DimBlack as usize))
                    }
                    // The default foreground or background: what reset gives.
                    _ => return None,
                }
            }
        }
    })
}

/// Put a snapshot into a fresh terminal of the same size.
pub fn restore<T: alacritty_terminal::event::EventListener>(term: &mut Term<T>, s: &Snapshot) {
    let mut parser: alacritty_terminal::vte::ansi::Processor = Default::default();
    parser.advance(term, s.screen.as_bytes());
    parser.advance(term, s.modes.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::event::VoidListener;
    use alacritty_terminal::term::Config;

    struct Size(usize, usize);
    impl Dimensions for Size {
        fn total_lines(&self) -> usize {
            self.1
        }
        fn screen_lines(&self) -> usize {
            self.1
        }
        fn columns(&self) -> usize {
            self.0
        }
    }

    fn term(cols: usize, rows: usize) -> Term<VoidListener> {
        let config = Config {
            scrolling_history: 1000,
            ..Default::default()
        };
        Term::new(config, &Size(cols, rows), VoidListener)
    }

    fn feed(t: &mut Term<VoidListener>, bytes: &str) {
        let mut p: alacritty_terminal::vte::ansi::Processor = Default::default();
        p.advance(t, bytes.as_bytes());
    }

    /// Every cell of history and screen: character, colours, attributes.
    fn cells(t: &Term<VoidListener>) -> Vec<(char, Color, Color, Flags)> {
        let g = t.grid();
        let mut out = Vec::new();
        for l in -(g.history_size() as i32)..t.screen_lines() as i32 {
            for c in 0..t.columns() {
                let cell = &g[Line(l)][Column(c)];
                out.push((cell.c, cell.fg, cell.bg, cell.flags));
            }
        }
        out
    }

    fn round_trip(t: &mut Term<VoidListener>) -> Term<VoidListener> {
        let before = cells(t);
        let cursor = t.grid().cursor.point;
        let snap = take(t);
        let mut back = term(t.columns(), t.screen_lines());
        restore(&mut back, &snap);
        assert_eq!(cells(&back), before, "cells differ");
        assert_eq!(back.grid().cursor.point, cursor, "cursor");
        back
    }

    #[test]
    fn history_colours_and_wraps_come_back_exactly() {
        let mut t = term(20, 5);
        let mut s = String::new();
        for i in 0..30 {
            s.push_str(&format!("line {i}\r\n"));
        }
        s.push_str("\x1b[1;31mbold red\x1b[0m plain \x1b[48;5;202mbg\x1b[0m\r\n");
        s.push_str("\x1b[38;2;1;2;3mtruecolor\x1b[7m inverse \x1b[0m\r\n");
        // Longer than a row: it wraps, and must wrap again the same way.
        s.push_str("abcdefghijklmnopqrstuvwxyz0123456789\r\n");
        s.push_str("wide 日本 and a tab\there");
        feed(&mut t, &s);
        round_trip(&mut t);
    }

    #[test]
    fn the_shell_behind_a_full_screen_program_is_kept_and_modes_come_back() {
        let mut t = term(30, 6);
        feed(&mut t, "$ ls\r\nfile1  file2\r\n$ nvim\r\n");
        let shell = cells(&t);
        // nvim: the alternate screen, application cursor, bracketed paste,
        // SGR mouse; it draws something there.
        feed(
            &mut t,
            "\x1b[?1049h\x1b[?1h\x1b[?2004h\x1b[?1000h\x1b[?1006h\x1b[Hnvim screen",
        );
        let snap = take(&mut t);
        let mut back = term(30, 6);
        restore(&mut back, &snap);
        let m = *back.mode();
        assert!(m.contains(TermMode::ALT_SCREEN));
        assert!(m.contains(TermMode::APP_CURSOR | TermMode::BRACKETED_PASTE));
        assert!(m.contains(TermMode::MOUSE_REPORT_CLICK | TermMode::SGR_MOUSE));
        // The program's own screen is empty (it redraws on SIGWINCH); the
        // shell's comes back when it leaves.
        feed(&mut back, "\x1b[?1049l");
        assert_eq!(cells(&back), shell);
    }

    #[test]
    fn a_palette_change_comes_back() {
        let mut t = term(10, 3);
        feed(
            &mut t,
            "\x1b]4;1;rgb:ff/00/80\x1b\\\x1b]11;rgb:10/20/30\x1b\\x",
        );
        let snap = take(&mut t);
        let mut back = term(10, 3);
        restore(&mut back, &snap);
        let c = back.colors()[1].unwrap();
        assert_eq!((c.r, c.g, c.b), (0xff, 0, 0x80));
        let bg = back.colors()[NamedColor::Background].unwrap();
        assert_eq!((bg.r, bg.g, bg.b), (0x10, 0x20, 0x30));
    }
}
