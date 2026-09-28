//! Drawing a frame: pane borders, pane contents, the bar.
//!
//! ratatui keeps the previous frame and writes only the cells that differ, so a
//! frame where one pane changed costs one pane's worth of output. What it does not
//! do is decide *when* to draw; that is the event loop's job (draw on change only).

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color as AColor, CursorShape, NamedColor};
use ratatui::Frame;
use ratatui::layout::{Position, Rect as RRect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

use crate::app::{App, Mode, PaneView};
use crate::layout::Rect;
use crate::theme::{self, BorderStyle};

pub fn color(c: theme::Color) -> Color {
    match c {
        theme::Color::Default => Color::Reset,
        theme::Color::Indexed(i) => Color::Indexed(i),
        theme::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

fn rrect(r: Rect) -> RRect {
    RRect::new(r.x, r.y, r.w, r.h)
}

/// The cursor the host terminal should show after this frame, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorState {
    pub shape: CursorShape,
    pub blinking: bool,
}

pub fn draw(f: &mut Frame, app: &App) -> Option<CursorState> {
    let mut cursor = None;

    for view in app.views() {
        let Some(pane) = app.panes.get(&view.id) else {
            continue;
        };
        draw_border(f, app, &view, &pane.title);

        let term = pane.term.lock();
        let content = term.renderable_content();
        let rows = term.screen_lines() as i32;
        let inner = view.inner;
        let buf = f.buffer_mut();

        for indexed in content.display_iter {
            let line = indexed.point.line.0 + content.display_offset as i32;
            let col = indexed.point.column.0 as u16;
            if line < 0 || line >= rows || col >= inner.w || line as u16 >= inner.h {
                continue;
            }
            let cell = indexed.cell;
            // The second half of a wide character belongs to the first; ratatui
            // skips it when diffing because the first cell's symbol is two wide.
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let Some(out) = buf.cell_mut(Position::new(inner.x + col, inner.y + line as u16))
            else {
                continue;
            };
            if cell
                .flags
                .intersects(Flags::HIDDEN | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                out.set_char(' ');
            } else if let Some(extra) = cell.zerowidth() {
                let mut s = String::with_capacity(8);
                s.push(display_char(cell.c));
                s.extend(extra);
                out.set_symbol(&s);
            } else {
                out.set_char(display_char(cell.c));
            }
            out.set_style(Style {
                fg: Some(term_color(cell.fg, content.colors)),
                bg: Some(term_color(cell.bg, content.colors)),
                add_modifier: modifiers(cell.flags),
                ..Style::default()
            });
        }

        if view.focused && app.mode == Mode::Normal {
            let c = content.cursor;
            let line = c.point.line.0 + content.display_offset as i32;
            let visible = content.mode.contains(TermMode::SHOW_CURSOR)
                && c.shape != CursorShape::Hidden
                && line >= 0
                && (line as u16) < inner.h
                && (c.point.column.0 as u16) < inner.w;
            if visible {
                f.set_cursor_position(Position::new(
                    inner.x + c.point.column.0 as u16,
                    inner.y + line as u16,
                ));
                cursor = Some(CursorState {
                    shape: c.shape,
                    blinking: term.cursor_style().blinking,
                });
            }
        }
    }

    if let Some(bar) = app.bar_rect() {
        draw_bar(f, app, bar);
    }
    cursor
}

fn draw_border(f: &mut Frame, app: &App, view: &PaneView, title: &str) {
    let theme = &app.config.theme;
    let border_type = match theme.border.style {
        BorderStyle::None => return,
        BorderStyle::Rounded => BorderType::Rounded,
        BorderStyle::Plain => BorderType::Plain,
        BorderStyle::Thick => BorderType::Thick,
        BorderStyle::Double => BorderType::Double,
    };
    let c = &theme.colors;
    // In WM mode the focused border takes the mode colour, so it is obvious which
    // pane the next action applies to.
    let border = match (view.focused, app.mode) {
        (true, Mode::Wm) => c.mode_bg,
        (true, Mode::Normal) => c.border_active,
        (false, _) => c.border_inactive,
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(color(border)));
    if !title.is_empty() {
        block = block.title(Line::from(format!(" {title} ")));
    }
    f.render_widget(block, rrect(view.outer));
}

fn draw_bar(f: &mut Frame, app: &App, area: Rect) {
    let c = &app.config.theme.colors;
    let base = Style::default().fg(color(c.bar_fg)).bg(color(c.bar_bg));
    let mut spans = Vec::new();
    match app.mode {
        Mode::Wm => spans.push(Span::styled(
            " WM ",
            Style::default()
                .fg(color(c.mode_fg))
                .bg(color(c.mode_bg))
                .add_modifier(Modifier::BOLD),
        )),
        Mode::Normal => spans.push(Span::styled(
            " ranma ",
            Style::default().fg(color(c.bar_dim)),
        )),
    }
    if let Some(msg) = &app.status {
        spans.push(Span::styled(
            format!(" {msg}"),
            Style::default().fg(color(c.bar_accent)),
        ));
    } else if let Some(title) = app.focused_title().filter(|t| !t.is_empty()) {
        spans.push(Span::raw(format!(" {title}")));
    }
    let right = format!(
        "{}{} ",
        if app.fullscreen() { "[full] " } else { "" },
        app.panes.len()
    );
    let used: usize = spans.iter().map(|s| s.width()).sum();
    let pad = (area.w as usize).saturating_sub(used + right.chars().count());
    spans.push(Span::raw(" ".repeat(pad)));
    spans.push(Span::styled(right, Style::default().fg(color(c.bar_dim))));
    f.render_widget(Paragraph::new(Line::from(spans)).style(base), rrect(area));
}

/// What a grid cell's character looks like on screen.
///
/// alacritty_terminal stores a literal `\t` in the cell a tab starts from, so
/// that copying the text keeps the tab. Sent to the host as is, it moves the host's
/// cursor instead of drawing a cell, and whatever was on screen there before stays
/// visible. Every control character is a blank cell here.
fn display_char(c: char) -> char {
    if c.is_control() { ' ' } else { c }
}

fn modifiers(flags: Flags) -> Modifier {
    let mut m = Modifier::empty();
    if flags.contains(Flags::BOLD) {
        m |= Modifier::BOLD;
    }
    if flags.contains(Flags::ITALIC) {
        m |= Modifier::ITALIC;
    }
    if flags.contains(Flags::DIM) {
        m |= Modifier::DIM;
    }
    if flags.intersects(Flags::ALL_UNDERLINES) {
        m |= Modifier::UNDERLINED;
    }
    if flags.contains(Flags::STRIKEOUT) {
        m |= Modifier::CROSSED_OUT;
    }
    if flags.contains(Flags::INVERSE) {
        m |= Modifier::REVERSED;
    }
    m
}

/// A cell colour in host terms. Palette entries a program redefined (OSC 4/10/11)
/// are honoured; everything else is left to the host's palette, so panes look
/// like the rest of the user's terminal.
fn term_color(c: AColor, overrides: &alacritty_terminal::term::color::Colors) -> Color {
    match c {
        AColor::Spec(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
        AColor::Indexed(i) => match overrides[i as usize] {
            Some(rgb) => Color::Rgb(rgb.r, rgb.g, rgb.b),
            None => Color::Indexed(i),
        },
        AColor::Named(n) => {
            if let Some(rgb) = overrides[n] {
                return Color::Rgb(rgb.r, rgb.g, rgb.b);
            }
            let idx = n as usize;
            if idx < 16 {
                return Color::Indexed(idx as u8);
            }
            match n {
                NamedColor::DimBlack => Color::Indexed(0),
                NamedColor::DimRed => Color::Indexed(1),
                NamedColor::DimGreen => Color::Indexed(2),
                NamedColor::DimYellow => Color::Indexed(3),
                NamedColor::DimBlue => Color::Indexed(4),
                NamedColor::DimMagenta => Color::Indexed(5),
                NamedColor::DimCyan => Color::Indexed(6),
                NamedColor::DimWhite => Color::Indexed(7),
                // Foreground, Background, Cursor, and their bright/dim variants:
                // the host's own defaults.
                _ => Color::Reset,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_characters_draw_as_blanks() {
        assert_eq!(display_char('\t'), ' ');
        assert_eq!(display_char('\x1b'), ' ');
        assert_eq!(display_char('\u{7f}'), ' ');
        assert_eq!(display_char('a'), 'a');
        assert_eq!(display_char('日'), '日');
    }
}
