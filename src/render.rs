//! Drawing a frame: pane borders and contents, tab bars, floats, the bar.
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
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode, PaneView};
use crate::bar;
use crate::layout::{Rect, TabBar};
use crate::picker::Picker;
use crate::theme::{self, BorderStyle, Colors};

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
    let frame = app.frame();
    let mut cursor = None;
    let mut overlay_cleared = false;

    for view in &frame.views {
        // Floats and the scratchpad cover what is under them; without clearing,
        // the tiles' cells would show through wherever the float's are blank.
        if view.floating {
            match frame.overlay {
                Some(o) if o.contains(view.outer.x, view.outer.y) => {
                    if !overlay_cleared {
                        f.render_widget(Clear, rrect(o));
                        overlay_cleared = true;
                    }
                }
                _ => f.render_widget(Clear, rrect(view.outer)),
            }
        }
        let Some(pane) = app.panes.get(&view.id) else {
            continue;
        };
        draw_border(f, app, view, pane.label());
        if let Some(c) = draw_pane(f, app, view, pane) {
            cursor = Some(c);
        }
    }

    for tb in &frame.tab_bars {
        draw_tab_bar(f, app, tb);
    }
    // Where a dragged tile would land: an outline over that half of the target.
    if let Some((target, dir)) = app.drop_preview()
        && let Some(v) = frame.views.iter().find(|v| v.id == target)
    {
        let half = crate::app::drop_half(v.outer, dir);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Thick)
            .border_style(Style::default().fg(color(app.config.theme.colors.mode_bg)));
        f.render_widget(block, rrect(half));
    }
    if let Some(bar) = app.bar_rect() {
        draw_bar(f, app, bar);
    }
    draw_toasts(f, app);
    if let (Some(p), Some(_)) = (app.picker(), app.picker_layout()) {
        draw_picker(f, app, p);
        // The picker's query line has the cursor; nothing else does.
        return None;
    }
    cursor
}

fn draw_pane(
    f: &mut Frame,
    app: &App,
    view: &PaneView,
    pane: &crate::pane::Pane,
) -> Option<CursorState> {
    let term = pane.term.lock();
    let content = term.renderable_content();
    let rows = term.screen_lines() as i32;
    let inner = view.inner;
    let colors = &app.config.theme.colors;
    let copy = app.copy_state().filter(|c| c.pane == view.id);
    let hit_style = Style::default()
        .fg(color(colors.search_fg))
        .bg(color(colors.search_bg));
    let current_style = Style::default()
        .fg(color(colors.search_current_fg))
        .bg(color(colors.search_current_bg));
    let selection = content.selection;
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
        let Some(out) = buf.cell_mut(Position::new(inner.x + col, inner.y + line as u16)) else {
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
        let mut style = Style {
            fg: Some(term_color(cell.fg, content.colors)),
            bg: Some(term_color(cell.bg, content.colors)),
            add_modifier: modifiers(cell.flags),
            ..Style::default()
        };
        let point = indexed.point;
        if let Some(c) = copy {
            if c.current.as_ref().is_some_and(|m| m.contains(&point)) {
                style = style.patch(current_style);
            } else if c.hits.iter().any(|m| m.contains(&point)) {
                style = style.patch(hit_style);
            }
        }
        if selection.is_some_and(|sel| sel.contains(point)) {
            style = style.add_modifier(Modifier::REVERSED);
        }
        out.set_style(style);
    }

    // The search prompt takes the pane's bottom row while it is open.
    if let Some(s) = copy.and_then(|c| c.search.as_ref()).filter(|s| s.editing)
        && inner.h > 0
    {
        let y = inner.y + inner.h - 1;
        let prompt = format!(
            "{}{}{}",
            if s.backward { "?" } else { "/" },
            s.query,
            if s.failed && !s.query.is_empty() {
                "   (no match)"
            } else {
                ""
            }
        );
        let style = Style::default()
            .fg(color(colors.mode_fg))
            .bg(color(colors.mode_bg));
        buf.set_style(RRect::new(inner.x, y, inner.w, 1), style);
        buf.set_stringn(inner.x, y, &prompt, inner.w as usize, style);
        let cx = inner.x + (1 + s.query.width() as u16).min(inner.w.saturating_sub(1));
        f.set_cursor_position(Position::new(cx, y));
        return Some(CursorState {
            shape: CursorShape::Beam,
            blinking: false,
        });
    }

    if !(view.focused && matches!(app.mode, Mode::Normal | Mode::Copy)) {
        return None;
    }
    let c = content.cursor;
    let line = c.point.line.0 + content.display_offset as i32;
    // In copy mode the cursor is alacritty's vi cursor, shown even when the
    // program hides its own.
    let visible = (app.mode == Mode::Copy || content.mode.contains(TermMode::SHOW_CURSOR))
        && c.shape != CursorShape::Hidden
        && line >= 0
        && (line as u16) < inner.h
        && (c.point.column.0 as u16) < inner.w;
    if !visible {
        return None;
    }
    f.set_cursor_position(Position::new(
        inner.x + c.point.column.0 as u16,
        inner.y + line as u16,
    ));
    Some(CursorState {
        shape: c.shape,
        blinking: term.cursor_style().blinking,
    })
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
    let border = match (view.focused, app.mode, view.floating) {
        (true, Mode::Wm | Mode::Copy, _) => c.mode_bg,
        (true, Mode::Normal, _) => c.border_active,
        (false, _, true) => c.border_floating,
        (false, _, false) => c.border_inactive,
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

/// Tabs share the row equally; a click maps back the same way (see App's mouse).
fn draw_tab_bar(f: &mut Frame, app: &App, tb: &TabBar) {
    let c: &Colors = &app.config.theme.colors;
    let n = tb.tabs.len().max(1) as u16;
    let buf = f.buffer_mut();
    for (i, id) in tb.tabs.iter().enumerate() {
        let i = i as u16;
        let x0 = tb.rect.x + i * tb.rect.w / n;
        let x1 = tb.rect.x + (i + 1) * tb.rect.w / n;
        let active = i as usize == tb.active;
        let style = if active {
            Style::default()
                .fg(color(c.tab_active_fg))
                .bg(color(c.tab_active_bg))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(color(c.tab_inactive_fg))
                .bg(color(c.tab_inactive_bg))
        };
        let title = app
            .panes
            .get(id)
            .map(|p| p.label())
            .filter(|t| !t.is_empty())
            .unwrap_or("shell");
        let width = x1.saturating_sub(x0) as usize;
        let label: String = format!(" {title} ").chars().take(width).collect();
        let padded = format!("{label:<width$}");
        buf.set_stringn(x0, tb.rect.y, &padded, width, style);
    }
}

fn draw_bar(f: &mut Frame, app: &App, area: Rect) {
    let c = &app.config.theme.colors;
    let base = Style::default().fg(color(c.bar_fg)).bg(color(c.bar_bg));
    let buf = f.buffer_mut();
    buf.set_style(rrect(area), base);
    for (x, piece) in app.bar_pieces(area.w) {
        let style = piece_style(c, piece.style).patch(Style::default().bg(color(c.bar_bg)));
        let style = match piece.style {
            // These carry their own background.
            bar::Style::Mode | bar::Style::WsActive => piece_style(c, piece.style),
            _ => style,
        };
        buf.set_stringn(
            area.x + x,
            area.y,
            &piece.text,
            area.w.saturating_sub(x) as usize,
            style,
        );
    }
}

fn draw_toasts(f: &mut Frame, app: &App) {
    let c = &app.config.theme.colors;
    let body = Style::default().fg(color(c.toast_fg)).bg(color(c.toast_bg));
    for (toast, r, lines) in app.toast_layout() {
        let edge = match toast.level {
            crate::toast::Level::Normal => c.bar_accent,
            crate::toast::Level::Urgent => c.bar_urgent,
        };
        f.render_widget(Clear, rrect(r));
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(body.fg(color(edge)))
            .style(body);
        f.render_widget(block, rrect(r));
        let buf = f.buffer_mut();
        for (i, line) in lines.iter().enumerate() {
            buf.set_stringn(
                r.x + 2,
                r.y + 1 + i as u16,
                line,
                r.w.saturating_sub(4) as usize,
                body,
            );
        }
    }
}

fn draw_picker(f: &mut Frame, app: &App, p: &Picker) {
    let Some(l) = app.picker_layout() else {
        return;
    };
    let c = &app.config.theme.colors;
    f.render_widget(Clear, rrect(l.outer));
    let border_type = match app.config.theme.border.style {
        BorderStyle::Rounded | BorderStyle::None => BorderType::Rounded,
        BorderStyle::Plain => BorderType::Plain,
        BorderStyle::Thick => BorderType::Thick,
        BorderStyle::Double => BorderType::Double,
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(Style::default().fg(color(c.mode_bg)))
        .title(Line::from(format!(" {} ", p.heading())));
    f.render_widget(block, rrect(l.outer));

    let buf = f.buffer_mut();
    let q = l.query;
    // A question shows its message and takes one key: no input line, no cursor.
    if let Some(msg) = &p.message {
        buf.set_stringn(
            q.x + 1,
            q.y,
            msg,
            q.w.saturating_sub(1) as usize,
            Style::default().fg(color(c.bar_fg)),
        );
        return;
    }
    let prompt = format!("> {}", p.query);
    buf.set_stringn(
        q.x,
        q.y,
        &prompt,
        q.w as usize,
        Style::default().fg(color(c.bar_fg)),
    );
    let cx = q.x + (prompt.width() as u16).min(q.w.saturating_sub(1));

    let items = p.visible();
    let marks = items.iter().any(|it| it.current);
    if items.is_empty() && !p.is_prompt() {
        buf.set_stringn(
            l.list.x,
            l.list.y,
            "  nothing matches",
            l.list.w as usize,
            Style::default().fg(color(c.bar_dim)),
        );
    }
    for (row, (i, item)) in items
        .iter()
        .enumerate()
        .skip(l.offset)
        .take(l.list.h as usize)
        .enumerate()
    {
        let y = l.list.y + row as u16;
        let w = l.list.w as usize;
        let selected = i == p.selected;
        let (fg, bg) = if selected {
            (color(c.picker_selected_fg), color(c.picker_selected_bg))
        } else {
            (color(c.bar_fg), Color::Reset)
        };
        let base = Style::default().fg(fg).bg(bg);
        buf.set_style(RRect::new(l.list.x, y, l.list.w, 1), base);
        // Where you are is marked, not only described in the dim detail: the
        // selection can sit elsewhere, and then nothing else says it.
        let label = match (marks, item.current) {
            (true, true) => format!(" ● {}", item.label),
            (true, false) => format!("   {}", item.label),
            (false, _) => format!(" {}", item.label),
        };
        let style = if item.current {
            base.add_modifier(Modifier::BOLD)
        } else {
            base
        };
        buf.set_stringn(l.list.x, y, &label, w, style);
        if !item.detail.is_empty() {
            let d = format!("{} ", item.detail);
            let dw = d.width();
            if label.width() + dw + 2 <= w {
                let dim = if selected {
                    base
                } else {
                    base.fg(color(c.bar_dim))
                };
                buf.set_stringn(l.list.x + (w - dw) as u16, y, &d, dw, dim);
            }
        }
    }
    f.set_cursor_position(Position::new(cx, q.y));
}

fn piece_style(c: &Colors, s: bar::Style) -> Style {
    let fg = |col| Style::default().fg(color(col));
    match s {
        bar::Style::Normal => fg(c.bar_fg),
        bar::Style::Dim => fg(c.bar_dim),
        bar::Style::Accent => fg(c.bar_accent),
        bar::Style::Urgent => fg(c.bar_urgent).add_modifier(Modifier::BOLD),
        bar::Style::Mode => Style::default()
            .fg(color(c.mode_fg))
            .bg(color(c.mode_bg))
            .add_modifier(Modifier::BOLD),
        bar::Style::WsActive => Style::default()
            .fg(color(c.ws_active_fg))
            .bg(color(c.ws_active_bg))
            .add_modifier(Modifier::BOLD),
        bar::Style::WsOccupied => fg(c.ws_occupied),
        bar::Style::WsEmpty => fg(c.ws_empty),
        bar::Style::WsUrgent => fg(c.ws_urgent).add_modifier(Modifier::BOLD),
    }
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
