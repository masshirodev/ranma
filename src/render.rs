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
use ratatui::layout::Alignment;
use ratatui::layout::{Position, Rect as RRect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders, Clear};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Mode, PaneView};
use crate::bar;
use crate::layout::{Rect, TabBar};
use crate::picker::Picker;
use crate::theme::{self, Attrs, BorderStyle, Colors, Styles, TitlePosition};

pub fn color(c: theme::Color) -> Color {
    match c {
        theme::Color::Default => Color::Reset,
        theme::Color::Indexed(i) => Color::Indexed(i),
        theme::Color::Rgb(r, g, b) => Color::Rgb(r, g, b),
    }
}

/// A role's text attributes as ratatui's.
pub fn modifier(a: Attrs) -> Modifier {
    let mut m = Modifier::empty();
    for (on, bit) in [
        (a.bold, Modifier::BOLD),
        (a.dim, Modifier::DIM),
        (a.italic, Modifier::ITALIC),
        (a.underline, Modifier::UNDERLINED),
        (a.reverse, Modifier::REVERSED),
        (a.strikethrough, Modifier::CROSSED_OUT),
    ] {
        if on {
            m |= bit;
        }
    }
    m
}

/// The characters a border style draws with; `None` draws no border.
fn border_set(b: &theme::Border, style: BorderStyle) -> Option<border::Set<'_>> {
    Some(match style {
        BorderStyle::None => return None,
        BorderStyle::Rounded => border::ROUNDED,
        BorderStyle::Plain => border::PLAIN,
        BorderStyle::Thick => border::THICK,
        BorderStyle::Double => border::DOUBLE,
        BorderStyle::Ascii => border::Set {
            top_left: "+",
            top_right: "+",
            bottom_left: "+",
            bottom_right: "+",
            vertical_left: "|",
            vertical_right: "|",
            horizontal_top: "-",
            horizontal_bottom: "-",
        },
        BorderStyle::Custom => {
            let [tl, tr, bl, br, h, v] = b.custom_chars();
            border::Set {
                top_left: tl,
                top_right: tr,
                bottom_left: bl,
                bottom_right: br,
                vertical_left: v,
                vertical_right: v,
                horizontal_top: h,
                horizontal_bottom: h,
            }
        }
    })
}

/// A box that is always framed (a picker, a sheet, a toast): the theme's
/// border, rounded when it has none.
fn frame_set(b: &theme::Border) -> border::Set<'_> {
    border_set(b, b.style).unwrap_or(border::ROUNDED)
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
    let labels = app.nest_labels(&frame);
    let mut cursor = None;
    let mut overlay_cleared = false;

    // Under everything: a scratchpad shown over an empty workspace covers it.
    if let Some((area, keys)) = app.splash() {
        draw_splash(f, app, area, &keys);
    }

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
        // A pane whose ranma reports draws its own titles (and, filling the
        // workspace, its own frame: then there is no border here at all).
        if view.inner != view.outer {
            let title = if app.reports_from(view.id) {
                String::new()
            } else {
                app.border_title(view.id)
            };
            draw_border(f, app, view, &title);
        }
        if let Some(c) = draw_pane(f, app, view, pane) {
            cursor = Some(c);
        }
        if let Some(l) = labels.iter().find(|l| l.pane == view.id) {
            draw_nest_label(f, app, view, l);
        }
    }

    for tb in &frame.tab_bars {
        draw_tab_bar(f, app, tb);
    }
    if let Some(h) = app.hint_state()
        && let Some(v) = frame.views.iter().find(|v| v.id == h.pane)
    {
        draw_hints(f, app, v, h);
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
    draw_toolbars(f, app);
    if app.which_key_shown() {
        draw_which_key(f, app);
    }
    draw_toasts(f, app);
    if let (Some(p), Some(l)) = (app.picker(), app.sheet_layout()) {
        draw_sheet(f, app, p, &l);
        return None;
    }
    if let (Some(p), Some(_)) = (app.picker(), app.picker_layout()) {
        draw_picker(f, app, p);
        // The picker's query line has the cursor; nothing else does.
        return None;
    }
    cursor
}

/// The logo and the keys, centred as one block on the empty workspace.
fn draw_splash(f: &mut Frame, app: &App, area: Rect, keys: &[(String, &str)]) {
    use crate::splash::{self, Part};
    let lines = splash::lines(area.w, area.h, keys);
    if lines.is_empty() {
        return;
    }
    let c = &app.config.theme.colors;
    let style = |p: Part| match p {
        Part::Logo => Style::default().fg(color(c.bar_accent)),
        Part::Key => Style::default()
            .fg(color(c.bar_fg))
            .add_modifier(Modifier::BOLD),
        Part::Text => Style::default().fg(color(c.bar_dim)),
    };
    // The logo and the keys are each centred as a block, so the keys keep
    // one column for what they do.
    let block_x = |logo: bool| {
        let w = lines
            .iter()
            .filter(|l| l.iter().any(|(_, p)| (*p == Part::Logo) == logo))
            .map(splash::line_width)
            .max()
            .unwrap_or(0) as u16;
        area.x + area.w.saturating_sub(w) / 2
    };
    let (logo_x, keys_x) = (block_x(true), block_x(false));
    let y = area.y + area.h.saturating_sub(lines.len() as u16) / 2;
    let buf = f.buffer_mut();
    for (i, line) in lines.iter().enumerate() {
        let logo = line.iter().any(|(_, p)| *p == Part::Logo);
        let mut x = if logo { logo_x } else { keys_x };
        for (text, part) in line {
            let room = area.right().saturating_sub(x) as usize;
            let (nx, _) = buf.set_stringn(x, y + i as u16, text, room, style(*part));
            x = nx;
        }
    }
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
    let panes = &app.config.theme.panes;
    let ground = if view.focused {
        panes.active_bg
    } else {
        panes.inactive_bg
    };
    let dim = Some(panes.dim_unfocused)
        .filter(|d| *d > 0.0 && !view.focused)
        .map(|amount| Dim {
            amount,
            host: &app.host_colors,
            ground,
        });
    let selected_style = match (colors.selection_fg, colors.selection_bg) {
        (None, None) => None,
        (fg, bg) => Some(Style {
            fg: fg.map(color),
            bg: bg.map(color),
            ..Style::default()
        }),
    };
    let buf = f.buffer_mut();
    // The pane's ground under everything, so cells the grid does not reach
    // (a resize on its way) show it too.
    if let Some(g) = ground {
        buf.set_style(rrect(inner), Style::default().bg(color(g)));
    }

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
        let mut bg = term_color(cell.bg, content.colors);
        // The program left the default background: the theme's ground, if any.
        if let (Color::Reset, Some(g)) = (bg, ground) {
            bg = color(g);
        }
        let mut style = Style {
            fg: Some(term_color(cell.fg, content.colors)),
            bg: Some(bg),
            add_modifier: modifiers(cell.flags),
            ..Style::default()
        };
        if let Some(d) = &dim {
            style = d.apply(style, cell, content.colors);
        }
        let point = indexed.point;
        if let Some(c) = copy {
            if c.current.as_ref().is_some_and(|m| m.contains(&point)) {
                style = style.patch(current_style);
            } else if c.hits.iter().any(|m| m.contains(&point)) {
                style = style.patch(hit_style);
            }
        }
        if selection.is_some_and(|sel| sel.contains(point)) {
            style = match selected_style {
                Some(sel) => style.patch(sel),
                None => style.add_modifier(Modifier::REVERSED),
            };
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
    let b = &theme.border;
    let style = if view.floating { b.floating() } else { b.style };
    let Some(set) = border_set(b, style) else {
        return;
    };
    let c = &app.colors();
    // In WM mode the focused border takes the mode colour, so it is obvious which
    // pane the next action applies to.
    let border = match (view.focused, app.mode, view.floating) {
        (true, Mode::Wm | Mode::Copy, _) => c.mode_bg,
        // A latched modifier takes the next key, as WM mode does.
        (true, Mode::Normal, _) if app.latched() => c.mode_bg,
        (true, Mode::Normal, _) => c.border_active,
        (false, _, true) => c.border_floating,
        (false, _, false) => c.border_inactive,
    };
    let edge = Style::default().fg(color(border));
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_set(set)
        .border_style(edge);
    let mark = if app.is_synced(view.id) { "⇉ " } else { "" };
    if b.title != TitlePosition::Off && (!title.trim().is_empty() || !mark.is_empty()) {
        // The sync mark goes in front of the text, inside the format's padding.
        let text = if title.trim().is_empty() {
            format!(" {mark}")
        } else {
            let lead = title.len() - title.trim_start().len();
            format!("{}{mark}{}", &title[..lead], &title[lead..])
        };
        let attrs = if view.focused {
            theme.styles.title_active
        } else {
            theme.styles.title
        };
        let line =
            Line::styled(text, edge.add_modifier(modifier(attrs))).alignment(match b.title_align {
                theme::Align::Left => Alignment::Left,
                theme::Align::Center => Alignment::Center,
                theme::Align::Right => Alignment::Right,
            });
        block = match b.title {
            TitlePosition::Bottom => block.title_bottom(line),
            _ => block.title_top(line),
        };
    }
    let r = view.outer;
    f.render_widget(block, rrect(r));
    if b.indicator == theme::Indicator::Arrows && view.focused && r.w >= 3 && r.h >= 3 {
        let buf = f.buffer_mut();
        let (mx, my) = (r.x + r.w / 2, r.y + r.h / 2);
        let mut arrows = vec![(r.x, my, "▶"), (r.right() - 1, my, "◀")];
        // Not over the title's edge.
        if b.title != TitlePosition::Top {
            arrows.push((mx, r.y, "▼"));
        }
        if b.title != TitlePosition::Bottom {
            arrows.push((mx, r.bottom() - 1, "▲"));
        }
        for (x, y, a) in arrows {
            buf.set_string(x, y, a, edge);
        }
    }
}

/// The compact label of a ranma this bar is not showing (see
/// `App::nest_labels`): on its pane's border, in the border's colour around
/// it, or, with no border, over the end of the pane's row on the bar's ground.
fn draw_nest_label(f: &mut Frame, app: &App, view: &PaneView, l: &crate::app::NestLabel) {
    let c = &app.config.theme.colors;
    let ground = if l.on_border {
        let edge = if view.floating {
            c.border_floating
        } else {
            c.border_inactive
        };
        Style::default().fg(color(edge))
    } else {
        Style::default().fg(color(c.bar_fg)).bg(color(c.bar_bg))
    };
    let buf = f.buffer_mut();
    let area = buf.area;
    if l.y >= area.bottom() {
        return;
    }
    let mut x = l.x;
    let mut put = |text: &str, style: Style, x: &mut u16| {
        let w = text.width() as u16;
        if *x + w <= area.right() {
            buf.set_stringn(*x, l.y, text, w as usize, style);
        }
        *x += w;
    };
    put(" ", ground, &mut x);
    for p in &l.pieces {
        let mut style = piece_style(c, &app.config.theme.styles, p.style);
        if !l.on_border && style.bg.is_none() {
            style = style.bg(color(c.bar_bg));
        }
        put(&p.text, style, &mut x);
    }
    put(" ", ground, &mut x);
}

/// Each link's label over its first cells, in the mode colours; labels that
/// no longer match what was typed are left out, and the typed part is dimmed.
fn draw_hints(f: &mut Frame, app: &App, view: &PaneView, h: &crate::app::HintState) {
    let c = &app.config.theme.colors;
    let label = Style::default()
        .fg(color(c.mode_fg))
        .bg(color(c.mode_bg))
        .add_modifier(Modifier::BOLD);
    let typed = label.add_modifier(Modifier::DIM);
    let inner = view.inner;
    let buf = f.buffer_mut();
    for (text, link) in &h.links {
        let Some(rest) = text.strip_prefix(h.typed.as_str()) else {
            continue;
        };
        let (row, col) = (link.at.0 as u16, link.at.1 as u16);
        if row >= inner.h || col >= inner.w {
            continue;
        }
        let (x, y) = (inner.x + col, inner.y + row);
        let room = (inner.w - col) as usize;
        buf.set_stringn(x, y, &h.typed, room, typed);
        let done = h.typed.chars().count() as u16;
        if done < inner.w - col {
            buf.set_stringn(x + done, y, rest, room - done as usize, label);
        }
    }
}

/// Tabs share the row equally; a click maps back the same way (see App's mouse).
fn draw_tab_bar(f: &mut Frame, app: &App, tb: &TabBar) {
    let c: &Colors = &app.config.theme.colors;
    let buf = f.buffer_mut();
    // A tall (touch-sized) strip: its row's background under the gaps, the
    // chips filled top to bottom, the titles on the middle row.
    if tb.rect.h > 1 {
        buf.set_style(rrect(tb.rect), Style::default().bg(color(c.bar_bg)));
    }
    let mid = tb.rect.y + tb.rect.h / 2;
    for (i, (id, (x0, x1))) in tb.tabs.iter().zip(tb.spans()).enumerate() {
        let active = i == tb.active;
        let style = piece_style(
            c,
            &app.config.theme.styles,
            if active {
                bar::Style::TabActive
            } else {
                bar::Style::TabInactive
            },
        );
        let width = x1.saturating_sub(x0) as usize;
        let label: String = format!(" {} ", app.chip_label(*id))
            .chars()
            .take(width)
            .collect();
        let padded = format!("{label:<width$}");
        let face = RRect::new(x0, tb.rect.y, x1.saturating_sub(x0), tb.rect.h);
        buf.set_style(face, style);
        buf.set_stringn(x0, mid, &padded, width, style);
    }
}

/// The toolbars: faces filled with their state's colours, the label centred
/// on the middle row (the handoff's section 01).
fn draw_toolbars(f: &mut Frame, app: &App) {
    use crate::app::ButtonState;
    let c = app.colors();
    let buf = f.buffer_mut();
    for t in app.shown_toolbars() {
        buf.set_style(
            rrect(t.placed.rect),
            Style::default().bg(color(c.toolbar_bg())),
        );
        for face in &t.faces {
            let state = app.button_state(&t.name, face.slot);
            let (fg, bg, bold) = match state {
                ButtonState::Normal => (c.button_fg(), c.button_bg(), false),
                ButtonState::Pressed => (c.button_pressed_fg(), c.button_pressed_bg(), true),
                ButtonState::Latched | ButtonState::Locked => {
                    (c.button_latched_fg(), c.button_latched_bg(), true)
                }
                ButtonState::Active => (c.button_active_fg(), c.button_active_bg(), true),
                ButtonState::Disabled => (c.button_disabled_fg(), c.button_bg(), false),
            };
            let mut style = Style::default().fg(color(fg)).bg(color(bg));
            if bold {
                style = style.add_modifier(Modifier::BOLD);
            }
            let r = face.rect;
            buf.set_style(rrect(r), style);
            let inner = r.w.saturating_sub(2) as usize;
            let label: String = if face.label.width() > inner {
                face.label
                    .chars()
                    .take(inner.saturating_sub(1))
                    .collect::<String>()
                    + "…"
            } else {
                face.label.clone()
            };
            let lw = label.width() as u16;
            let mid = r.y + r.h / 2;
            let locked = state == ButtonState::Locked;
            let label_style = if locked && r.h == 1 {
                style.add_modifier(Modifier::UNDERLINED)
            } else {
                style
            };
            buf.set_stringn(r.x + (r.w - lw) / 2, mid, &label, inner, label_style);
            if locked && r.h >= 3 && r.w >= 4 {
                buf.set_stringn(r.x + (r.w - 4) / 2, r.bottom() - 1, "lock", 4, style);
            }
        }
    }
}

fn draw_bar(f: &mut Frame, app: &App, area: Rect) {
    let c = &app.colors();
    let st = &app.config.theme.styles;
    let base = Style::default().fg(color(c.bar_fg)).bg(color(c.bar_bg));
    // The bar owns its row: drawn over a pane (a nested ranma's bar, while an
    // outer one shows its workspaces elsewhere), nothing underneath shows.
    f.render_widget(Clear, rrect(area));
    let buf = f.buffer_mut();
    buf.set_style(rrect(area), base);
    // A large bar is three rows: the chips fill all of them, the text sits on
    // the middle one.
    let mid = area.y + area.h / 2;
    for (x, piece) in app.bar_pieces(area.w) {
        let filled = matches!(
            piece.style,
            bar::Style::Mode
                | bar::Style::WsActive
                | bar::Style::TabActive
                | bar::Style::TabInactive
                | bar::Style::Cap
        );
        let mut style = piece_style(c, st, piece.style);
        // These carry their own background; the rest sit on the module's
        // ground (between its caps) or the bar's.
        if !filled {
            match (piece.boxed, c.module_bg) {
                (true, Some(ground)) => {
                    style = style.bg(color(ground));
                    if piece.style == bar::Style::Normal
                        && let Some(fg) = c.module_fg
                    {
                        style = style.fg(color(fg));
                    }
                }
                _ => style = style.bg(color(c.bar_bg)),
            }
        }
        let room = area.w.saturating_sub(x);
        if filled && area.h > 1 {
            let w = (piece.text.width() as u16).min(room);
            buf.set_style(RRect::new(area.x + x, area.y, w, area.h), style);
        }
        buf.set_stringn(area.x + x, mid, &piece.text, room as usize, style);
    }
}

/// The which-key hint: a panel standing on the bar from its left end, where
/// ` WM ` is (hanging under a top bar; on the bottom row with none). Laid out
/// by `whichkey`, from the WM-mode binds; drawn in the theme's roles, the
/// frame in the mode colour on the toast surface (the design's handoff).
fn draw_which_key(f: &mut Frame, app: &App) {
    use crate::whichkey::{self, BindKind, Role};
    let screen = f.area();
    let groups = whichkey::groups(app.config.binds.iter().map(|(c, b)| {
        (
            *c,
            match &b.action {
                crate::config::BindAction::Builtin(a) => BindKind::Action(a),
                crate::config::BindAction::Lua(_) => BindKind::Lua(b.desc.as_deref()),
            },
        )
    }));
    let help = app
        .config
        .binds
        .iter()
        .find(|(_, b)| {
            matches!(
                b.action,
                crate::config::BindAction::Builtin(crate::action::Action::Help)
            )
        })
        .map(|(c, _)| whichkey::short(c));
    let title = whichkey::short(&app.wm_chord());
    let Some(p) = whichkey::layout(
        &groups,
        screen.width,
        screen.height,
        &title,
        help.as_deref(),
    ) else {
        return;
    };
    let y = match app.bar_rect() {
        Some(bar) if bar.y == screen.y => bar.bottom(),
        Some(bar) => bar.y.saturating_sub(p.h),
        None => screen.bottom().saturating_sub(p.h),
    };
    let area = RRect::new(screen.x, y, p.w.min(screen.width), p.h.min(screen.height));
    let c = &app.colors();
    let surface = Style::default().fg(color(c.toast_fg)).bg(color(c.toast_bg));
    f.render_widget(Clear, area);
    let mut block = Block::default().style(surface);
    let b = &app.config.theme.border;
    if let Some(set) = border_set(b, b.style) {
        block = block
            .borders(Borders::ALL)
            .border_set(set)
            .border_style(surface.fg(color(c.mode_bg)));
    }
    f.render_widget(block, area);
    let buf = f.buffer_mut();
    for (x, y, text, role) in &p.pieces {
        let style = match role {
            Role::Title | Role::KeyBase | Role::FooterKey => {
                surface.fg(color(c.mode_bg)).add_modifier(Modifier::BOLD)
            }
            Role::KeyMods => surface.fg(color(c.mode_bg)),
            Role::Heading => surface.fg(color(c.bar_accent)).add_modifier(Modifier::BOLD),
            Role::Name | Role::FooterText => surface,
        };
        let (x, y) = (area.x + x, area.y + y);
        if y < area.bottom() && x < area.right() {
            buf.set_stringn(x, y, text, (area.right() - x) as usize, style);
        }
    }
}

fn draw_toasts(f: &mut Frame, app: &App) {
    let theme = &app.config.theme;
    let c = &theme.colors;
    let body = Style::default().fg(color(c.toast_fg)).bg(color(c.toast_bg));
    let text = body.add_modifier(modifier(theme.styles.toast));
    for (toast, r, lines) in app.toast_layout() {
        let edge = match toast.level {
            crate::toast::Level::Normal => c.bar_accent,
            crate::toast::Level::Urgent => c.bar_urgent,
        };
        f.render_widget(Clear, rrect(r));
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(frame_set(&theme.border))
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
                text,
            );
        }
    }
}

/// A picker as a touch sheet (the handoff's section 07): entries are faces a
/// thumb can hit, nothing is highlighted until a key moves the selection, and
/// what does not fit is counted on the bottom border.
fn draw_sheet(f: &mut Frame, app: &App, p: &Picker, l: &crate::app::SheetLayout) {
    let c = app.colors();
    f.render_widget(Clear, rrect(l.outer));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(frame_set(&app.config.theme.border))
        .border_style(Style::default().fg(color(c.mode_bg)))
        .title(Line::from(format!(" {} ", p.heading())));
    f.render_widget(block, rrect(l.outer));
    let buf = f.buffer_mut();
    let text = Style::default().fg(color(c.bar_fg));
    let dim = Style::default().fg(color(c.bar_dim));
    if let Some(q) = l.query {
        let prompt = format!("> {}", p.query);
        buf.set_stringn(q.x, q.y, &prompt, q.w as usize, text);
        if p.query.is_empty() {
            let x = q.x + prompt.width() as u16 + 1;
            buf.set_stringn(
                x,
                q.y,
                "type to filter",
                q.right().saturating_sub(x) as usize,
                dim,
            );
        }
    }
    let items = p.visible();
    let marks = items.iter().any(|it| it.current);
    let highlight = p.touched || !p.query.is_empty();
    for (i, r) in &l.faces {
        let Some(item) = items.get(*i) else {
            continue;
        };
        let selected = highlight && *i == p.selected;
        let urgent = matches!(&item.target, crate::picker::Target::Run(a) if a == "close_pane");
        let (fg, bg) = if selected {
            (c.picker_selected_fg, c.picker_selected_bg)
        } else if urgent {
            (c.bar_urgent, c.button_bg())
        } else {
            (c.button_fg(), c.button_bg())
        };
        let mut face = Style::default().fg(color(fg)).bg(color(bg));
        if selected {
            face = face.add_modifier(modifier(app.config.theme.styles.picker_selected));
        }
        buf.set_style(rrect(*r), face);
        let mid = r.y + r.h / 2;
        let detail = if l.details { item.detail.as_str() } else { "" };
        let dw = if detail.is_empty() {
            0
        } else {
            detail.width() + 1
        };
        let mark = match (marks, item.current) {
            (true, true) => "● ",
            (true, false) => "  ",
            (false, _) => "",
        };
        let label = format!("{mark}{}", item.label);
        let room = (r.w as usize).saturating_sub(2 + dw);
        let label: String = if label.width() > room {
            label
                .chars()
                .take(room.saturating_sub(1))
                .collect::<String>()
                + "…"
        } else {
            label
        };
        let style = if item.current {
            face.add_modifier(Modifier::BOLD)
        } else {
            face
        };
        buf.set_stringn(r.x + 1, mid, &label, room, style);
        if !detail.is_empty() {
            let hint = if selected {
                face
            } else {
                Style::default().fg(color(c.bar_dim)).bg(color(bg))
            };
            let x = r.right().saturating_sub(1 + detail.width() as u16);
            buf.set_stringn(x, mid, detail, detail.width(), hint);
        }
    }
    if l.more > 0 {
        let s = format!(" ▾ {} more ", l.more);
        let x = l.outer.right().saturating_sub(2 + s.width() as u16);
        buf.set_stringn(x, l.outer.bottom() - 1, &s, s.width(), text);
    }
}

fn draw_picker(f: &mut Frame, app: &App, p: &Picker) {
    let Some(l) = app.picker_layout() else {
        return;
    };
    let c = &app.config.theme.colors;
    f.render_widget(Clear, rrect(l.outer));
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(frame_set(&app.config.theme.border))
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
        let mut base = Style::default().fg(fg).bg(bg);
        if selected {
            base = base.add_modifier(modifier(app.config.theme.styles.picker_selected));
        }
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

fn piece_style(c: &Colors, st: &Styles, s: bar::Style) -> Style {
    let fg = |col, a| Style::default().fg(color(col)).add_modifier(modifier(a));
    let filled = |f, b, a| {
        Style::default()
            .fg(color(f))
            .bg(color(b))
            .add_modifier(modifier(a))
    };
    match s {
        bar::Style::Normal => fg(c.bar_fg, st.bar),
        bar::Style::Dim => fg(c.bar_dim, st.dim),
        bar::Style::Accent => fg(c.bar_accent, st.accent),
        bar::Style::Urgent => fg(c.bar_urgent, st.urgent),
        bar::Style::Mode => filled(c.mode_fg, c.mode_bg, st.mode),
        bar::Style::WsActive => filled(c.ws_active_fg, c.ws_active_bg, st.ws_active),
        bar::Style::WsOccupied => fg(c.ws_occupied, st.ws_occupied),
        bar::Style::WsEmpty => fg(c.ws_empty, st.ws_empty),
        bar::Style::WsUrgent => fg(c.ws_urgent, st.ws_urgent),
        // A current workspace that is not the end of the path: bold, occupied.
        bar::Style::WsHolder => fg(c.ws_occupied, st.ws_occupied).add_modifier(Modifier::BOLD),
        bar::Style::TabActive => filled(c.tab_active_fg, c.tab_active_bg, st.tab_active),
        bar::Style::TabInactive => filled(c.tab_inactive_fg, c.tab_inactive_bg, st.tab_inactive),
        bar::Style::WsInner(accent) => match accent {
            Some([r, g, b]) => Style::default()
                .fg(Color::Rgb(r, g, b))
                .add_modifier(modifier(st.ws_active)),
            None => fg(c.ws_active_bg, st.ws_active),
        },
        // A module's edge: its ground's colour on the bar's.
        bar::Style::Cap => Style::default()
            .fg(color(c.module_bg.unwrap_or(c.bar_bg)))
            .bg(color(c.bar_bg)),
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

/// Fading an unfocused pane's text toward its background (`panes.dim_unfocused`).
struct Dim<'a> {
    amount: f32,
    host: &'a crate::hostcolors::HostColors,
    /// The pane's ground (`panes.inactive_bg`), where the program left the
    /// default background: text fades toward it, not toward the host's.
    ground: Option<theme::Color>,
}

impl Dim<'_> {
    /// The cell's foreground mixed toward its background. Both are resolved to
    /// RGB through the pane's own palette changes and then the host's; when
    /// either is unknown (a host that did not report its colours), the
    /// terminal's faint attribute stands in.
    fn apply(
        &self,
        style: Style,
        cell: &alacritty_terminal::term::cell::Cell,
        overrides: &alacritty_terminal::term::color::Colors,
    ) -> Style {
        let (fg, bg) = if cell.flags.contains(Flags::INVERSE) {
            (cell.bg, cell.fg)
        } else {
            (cell.fg, cell.bg)
        };
        let ground = |c: AColor| -> Option<crate::hostcolors::Rgb> {
            let default_bg = matches!(c, AColor::Named(NamedColor::Background))
                && overrides[NamedColor::Background].is_none();
            match self.ground.filter(|_| default_bg)? {
                theme::Color::Rgb(r, g, b) => Some(crate::hostcolors::Rgb { r, g, b }),
                theme::Color::Indexed(i) => self.host.get(i as usize),
                theme::Color::Default => None,
            }
        };
        match (
            rgb_of(fg, overrides, self.host),
            ground(bg).or_else(|| rgb_of(bg, overrides, self.host)),
        ) {
            (Some(f), Some(b)) => {
                let mix =
                    |f: u8, b: u8| (f as f32 + (b as f32 - f as f32) * self.amount).round() as u8;
                let c = Color::Rgb(mix(f.r, b.r), mix(f.g, b.g), mix(f.b, b.b));
                if cell.flags.contains(Flags::INVERSE) {
                    style.bg(c)
                } else {
                    style.fg(c)
                }
            }
            _ => style.add_modifier(Modifier::DIM),
        }
    }
}

/// A cell colour as RGB, if it can be known: a program's own palette change
/// first, then the host's colours as asked at startup.
fn rgb_of(
    c: AColor,
    overrides: &alacritty_terminal::term::color::Colors,
    host: &crate::hostcolors::HostColors,
) -> Option<crate::hostcolors::Rgb> {
    let from = |v: alacritty_terminal::vte::ansi::Rgb| crate::hostcolors::Rgb {
        r: v.r,
        g: v.g,
        b: v.b,
    };
    let index = match c {
        AColor::Spec(rgb) => return Some(from(rgb)),
        AColor::Indexed(i) => i as usize,
        AColor::Named(n) => match n {
            NamedColor::DimBlack
            | NamedColor::DimRed
            | NamedColor::DimGreen
            | NamedColor::DimYellow
            | NamedColor::DimBlue
            | NamedColor::DimMagenta
            | NamedColor::DimCyan
            | NamedColor::DimWhite => n as usize - NamedColor::DimBlack as usize,
            NamedColor::BrightForeground | NamedColor::DimForeground => {
                NamedColor::Foreground as usize
            }
            _ => n as usize,
        },
    };
    overrides[index].map(from).or_else(|| host.get(index))
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
    fn colours_resolve_through_the_programs_palette_then_the_hosts() {
        use crate::hostcolors::{HostColors, Rgb};
        let host = HostColors {
            fg: Some(Rgb {
                r: 200,
                g: 200,
                b: 200,
            }),
            bg: Some(Rgb { r: 0, g: 0, b: 0 }),
            palette: [Some(Rgb {
                r: 10,
                g: 20,
                b: 30,
            }); 16],
            ..Default::default()
        };
        let none = alacritty_terminal::term::color::Colors::default();
        let rgb = |c| rgb_of(c, &none, &host).map(|v| (v.r, v.g, v.b));
        assert_eq!(
            rgb(AColor::Named(NamedColor::Foreground)),
            Some((200, 200, 200))
        );
        assert_eq!(rgb(AColor::Named(NamedColor::Background)), Some((0, 0, 0)));
        assert_eq!(rgb(AColor::Named(NamedColor::DimRed)), Some((10, 20, 30)));
        assert_eq!(rgb(AColor::Indexed(196)), Some((255, 0, 0)));
        let mut own = alacritty_terminal::term::color::Colors::default();
        own[NamedColor::Foreground] = Some(alacritty_terminal::vte::ansi::Rgb { r: 1, g: 2, b: 3 });
        assert_eq!(
            rgb_of(AColor::Named(NamedColor::Foreground), &own, &host).map(|v| (v.r, v.g, v.b)),
            Some((1, 2, 3))
        );
        // A host that said nothing: unknown, so the faint attribute is used.
        assert!(
            rgb_of(
                AColor::Named(NamedColor::Foreground),
                &none,
                &HostColors::default()
            )
            .is_none()
        );
    }

    #[test]
    fn control_characters_draw_as_blanks() {
        assert_eq!(display_char('\t'), ' ');
        assert_eq!(display_char('\x1b'), ' ');
        assert_eq!(display_char('\u{7f}'), ' ');
        assert_eq!(display_char('a'), 'a');
        assert_eq!(display_char('日'), '日');
    }
}
