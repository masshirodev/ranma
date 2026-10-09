//! The bar: modules, how they fit in one row, and running `exec` modules.
//!
//! A module renders to pieces of styled text. Built-in modules are computed from
//! ranma's state at draw time; Lua and exec modules are computed on their own
//! schedule and cached, so drawing a frame never runs Lua or a process.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use unicode_width::UnicodeWidthStr;

use crate::pane::AppEvent;

/// How a piece of the bar is coloured. The theme decides what each one looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    Dim,
    Accent,
    Urgent,
    Mode,
    WsActive,
    WsOccupied,
    WsEmpty,
    WsUrgent,
    /// A workspace that printed while not shown (`monitor_activity`).
    WsActivity,
    /// A current workspace that is not the end of the path: bold, occupied.
    WsHolder,
    /// The deepest current workspace inside a nested ranma: bold text in that
    /// ranma's session accent (the theme's active colour without one).
    WsInner(Option<[u8; 3]>),
    /// A pane chip in the `pane_strip` module, drawn as a tab.
    TabActive,
    TabInactive,
    /// `bar.module_left` or `module_right`: the edge of a module's ground.
    Cap,
}

impl Style {
    /// The styles a Lua module may ask for by name.
    pub fn from_name(s: &str) -> Option<Style> {
        Some(match s {
            "normal" => Style::Normal,
            "dim" => Style::Dim,
            "accent" => Style::Accent,
            "urgent" => Style::Urgent,
            _ => return None,
        })
    }
}

/// The mode module's sign that keys go to a ranma inside the focused pane.
/// Four cells, the ⧉ second: kitty and Windows Terminal both draw it about
/// two cells wide, centred on the line after its own cell, so one blank
/// before and two after put it in the middle of its pill. One either side
/// (and a no-break space to keep kitty from widening it, 2026-10-07) left it
/// half a cell right in Windows Terminal, which widens it regardless.
pub const NESTED_SIGN: &str = " ⧉  ";

/// What clicking a piece of the bar does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// Go to this workspace (0 toggles the scratchpad).
    Workspace(u8),
    SessionSwitcher,
    /// Run the update.
    Update,
    /// Focus this pane (a `pane_strip` chip).
    Pane(crate::layout::PaneId),
    /// Open the pane switcher (the folded strip's `2/4 nvim` chip).
    Panes,
    /// A workspace inside the ranma in workspace `holder`'s focused pane,
    /// `depth` levels down: `path` holds each level's workspace (0 for its
    /// scratchpad).
    Nested {
        holder: u8,
        depth: u8,
        path: [u8; 4],
    },
    /// Workspace `n` (0 for its scratchpad) of the ranma in `pane`, from the
    /// label on that pane's border: focus the pane, then go there.
    InPane {
        pane: crate::layout::PaneId,
        n: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub style: Style,
    pub click: Option<Click>,
    /// Inside a module's caps: drawn on `colors.module_bg`.
    pub boxed: bool,
}

impl Piece {
    pub fn new(text: impl Into<String>, style: Style) -> Piece {
        Piece {
            text: text.into(),
            style,
            click: None,
            boxed: false,
        }
    }
    pub fn on_click(mut self, click: Click) -> Piece {
        self.click = Some(click);
        self
    }
}

/// One module's output. Empty modules take no room and get no separator.
pub type Segment = Vec<Piece>;

fn width(pieces: &[Piece]) -> usize {
    pieces.iter().map(|p| p.text.width()).sum()
}

/// A module's chips at the large size: each at least five columns with its
/// text centred, one column apart, so a thumb can hit them (the handoff's
/// section 06). A workspace's `[k]` count stays with its chip.
pub fn enlarge(seg: Segment) -> Segment {
    let mut out: Segment = Vec::new();
    for p in seg {
        // Its own gaps replace the module's.
        if p.text.trim().is_empty() {
            continue;
        }
        if p.text.starts_with('[') {
            out.push(p);
            continue;
        }
        if !out.is_empty() {
            out.push(Piece::new(" ", Style::Normal));
        }
        let t = p.text.trim();
        let w = t.width().max(3) + 2;
        let left = (w - t.width()) / 2;
        let text = format!(
            "{}{t}{}",
            " ".repeat(left),
            " ".repeat(w - t.width() - left)
        );
        out.push(Piece { text, ..p });
    }
    out
}

/// Share `spare` columns out among a segment's chips (its clickable pieces),
/// as trailing space: the folded strip in a large bar stretches across the
/// room it has, the way the strip's own row does.
pub fn stretch(seg: Segment, spare: usize) -> Segment {
    let n = seg.iter().filter(|p| p.click.is_some()).count();
    if n == 0 || spare == 0 {
        return seg;
    }
    let mut i = 0;
    seg.into_iter()
        .map(|mut p| {
            if p.click.is_some() {
                let extra = (i + 1) * spare / n - i * spare / n;
                p.text.push_str(&" ".repeat(extra));
                i += 1;
            }
            p
        })
        .collect()
}

/// A module between its caps (`bar.module_left` / `module_right`), its pieces
/// on the module's ground. An empty module stays empty: it takes no room.
pub fn boxed(seg: Segment, left: &str, right: &str) -> Segment {
    if (left.is_empty() && right.is_empty()) || width(&seg) == 0 {
        return seg;
    }
    let cap = |t: &str| Piece::new(t, Style::Cap);
    let mut out = Vec::with_capacity(seg.len() + 2);
    if !left.is_empty() {
        out.push(cap(left));
    }
    out.extend(seg.into_iter().map(|p| Piece { boxed: true, ..p }));
    if !right.is_empty() {
        out.push(cap(right));
    }
    out
}

/// Join a side's modules with the separator, skipping empty ones.
fn join(segments: &[Segment], sep: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    for seg in segments.iter().filter(|s| width(s) > 0) {
        if !out.is_empty() && !sep.is_empty() {
            out.push(Piece::new(sep, Style::Dim));
        }
        out.extend(seg.iter().cloned());
    }
    out
}

/// Cut pieces down to `max` columns, marking the cut with an ellipsis.
fn truncate(pieces: Vec<Piece>, max: usize) -> Vec<Piece> {
    if width(&pieces) <= max {
        return pieces;
    }
    if max == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut used = 0;
    for p in pieces {
        let w = p.text.width();
        if used + w < max {
            used += w;
            out.push(p);
            continue;
        }
        let mut text = String::new();
        for c in p.text.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used + cw + 1 > max {
                break;
            }
            used += cw;
            text.push(c);
        }
        text.push('…');
        out.push(Piece { text, ..p });
        break;
    }
    out
}

/// The width of a side once joined.
pub fn joined_width(segments: &[Segment], sep: &str) -> usize {
    width(&join(segments, sep))
}

/// Place the three sides in `cols` columns. Returns pieces with their column.
///
/// Budgeting, in order: the right side is kept whole if it fits at all (it holds
/// the things you glance at: clock, load); the left side gets what remains; the
/// centre gets the gap between them, centred on the bar when it fits there and
/// pushed aside otherwise. Whatever does not fit is truncated with an ellipsis,
/// never silently dropped piece by piece.
pub fn fit(
    left: &[Segment],
    center: &[Segment],
    right: &[Segment],
    sep: &str,
    cols: u16,
) -> Vec<(u16, Piece)> {
    fit_floor(left, center, right, sep, cols, 0)
}

/// `fit`, except a centre that would be cut below `floor` cells is left out
/// rather than shown as a stub (`✳Fea…`).
pub fn fit_floor(
    left: &[Segment],
    center: &[Segment],
    right: &[Segment],
    sep: &str,
    cols: u16,
    floor: usize,
) -> Vec<(u16, Piece)> {
    let cols = cols as usize;
    let right = truncate(join(right, sep), cols);
    let rw = width(&right);
    let left = truncate(
        join(left, sep),
        cols.saturating_sub(rw + usize::from(rw > 0)),
    );
    let lw = width(&left);

    let mut out = Vec::new();
    let mut x = 0;
    for p in left {
        let w = p.text.width();
        out.push((x as u16, p));
        x += w;
    }

    // One column of air on each side of the centre, when there is anything beside it.
    let gap_start = lw + usize::from(lw > 0);
    let gap_end = cols.saturating_sub(rw + usize::from(rw > 0));
    let room = gap_end.saturating_sub(gap_start);
    let mut center = join(center, sep);
    if width(&center) > room && room < floor {
        center.clear();
    }
    let center = truncate(center, room);
    let cw = width(&center);
    if cw > 0 {
        let ideal = cols.saturating_sub(cw) / 2;
        let mut cx = ideal.clamp(gap_start, gap_end.saturating_sub(cw).max(gap_start));
        for p in center {
            let w = p.text.width();
            out.push((cx as u16, p));
            cx += w;
        }
    }

    let mut rx = cols - rw;
    for p in right {
        let w = p.text.width();
        out.push((rx as u16, p));
        rx += w;
    }
    out
}

/// When a module with this interval should next run: aligned to the wall clock,
/// so a 60 s clock ticks on the minute rather than 60 s after ranma started.
pub fn next_due(now: Instant, interval: Duration) -> Instant {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let iv = interval.as_nanos().max(1);
    let into = since_epoch.as_nanos() % iv;
    // A few milliseconds late rather than early, so os.date() already sees the
    // new minute when the tick lands.
    now + Duration::from_nanos((iv - into) as u64) + Duration::from_millis(5)
}

/// How long an exec module may run before its whole process group is killed.
pub const EXEC_TIMEOUT: Duration = Duration::from_secs(5);

/// Run an exec module's command off the UI thread; the result arrives as
/// [`AppEvent::Module`].
///
/// The command runs in its own process group, and a timeout kills the group, not
/// just the shell. Killing only the shell leaves any child it started holding the
/// output pipe open, and the read never finishes (tuios #141).
pub fn spawn_exec(name: String, generation: u64, command: String, tx: Sender<AppEvent>) {
    std::thread::Builder::new()
        .name(format!("module {name}"))
        .spawn(move || {
            let text = run_command(&command, EXEC_TIMEOUT);
            let _ = tx.send(AppEvent::Module {
                name,
                generation,
                text,
            });
        })
        .expect("spawning a module thread");
}

pub fn run_command(command: &str, timeout: Duration) -> Result<String, String> {
    use std::os::unix::process::CommandExt;
    let mut child = Command::new("/bin/sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| format!("could not run: {e}"))?;
    let pgid = child.id() as i32;
    let done = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    {
        let (done, timed_out) = (done.clone(), timed_out.clone());
        std::thread::spawn(move || {
            let start = Instant::now();
            while start.elapsed() < timeout {
                if done.load(Ordering::Acquire) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            if !done.load(Ordering::Acquire) {
                timed_out.store(true, Ordering::Release);
                // SAFETY: kill(2) with a negative pid signals a process group we
                // created; no memory is involved.
                unsafe {
                    libc::kill(-pgid, libc::SIGKILL);
                }
            }
        });
    }
    let mut out = Vec::new();
    let read = child.stdout.take().map(|mut s| s.read_to_end(&mut out));
    let status = child.wait();
    done.store(true, Ordering::Release);
    if timed_out.load(Ordering::Acquire) {
        return Err(format!("timed out after {}s", timeout.as_secs()));
    }
    if let Some(Err(e)) = read {
        return Err(format!("reading output: {e}"));
    }
    match status {
        Ok(s) if s.success() => Ok(String::from_utf8_lossy(&out)
            .lines()
            .next()
            .unwrap_or("")
            .trim_end()
            .to_string()),
        Ok(s) => Err(format!("exited with {s}")),
        Err(e) => Err(format!("waiting: {e}")),
    }
}

/// Apply an exec module's `format`: `%s` is the output line.
pub fn format_output(format: Option<&str>, line: &str) -> String {
    match format {
        Some(f) => f.replace("%s", line),
        None => line.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(s: &str) -> Segment {
        vec![Piece::new(s, Style::Normal)]
    }
    fn text_at(placed: &[(u16, Piece)], cols: usize) -> String {
        let mut line = vec![' '; cols];
        for (x, p) in placed {
            for (i, c) in p.text.chars().enumerate() {
                if (*x as usize + i) < cols {
                    line[*x as usize + i] = c;
                }
            }
        }
        line.into_iter().collect()
    }

    #[test]
    fn sides_land_where_they_belong() {
        let placed = fit(&[seg("L")], &[seg("mid")], &[seg("R")], " | ", 21);
        assert_eq!(text_at(&placed, 21), "L        mid        R");
    }

    #[test]
    fn separators_only_between_non_empty_modules() {
        let placed = fit(&[seg("a"), seg(""), seg("b")], &[], &[], "|", 10);
        assert_eq!(text_at(&placed, 10).trim_end(), "a|b");
    }

    #[test]
    fn centre_moves_aside_rather_than_overlap() {
        let placed = fit(&[seg("a-long-left")], &[seg("ctr")], &[], " ", 20);
        let line = text_at(&placed, 20);
        assert!(line.starts_with("a-long-left ctr"), "{line:?}");
    }

    #[test]
    fn right_wins_then_left_then_centre_truncates() {
        let placed = fit(
            &[seg("leftleft")],
            &[seg("centre")],
            &[seg("12:00")],
            " ",
            16,
        );
        let line = text_at(&placed, 16);
        assert!(line.ends_with("12:00"), "{line:?}");
        assert!(line.starts_with("leftleft"), "{line:?}");
        // Two columns remain between them after the air; the centre is cut to fit.
        assert!(line.contains('…'), "{line:?}");
    }

    #[test]
    fn a_left_side_too_long_is_truncated_not_dropped() {
        let placed = fit(&[seg("abcdefghij")], &[], &[seg("R")], " ", 8);
        assert_eq!(text_at(&placed, 8), "abcde… R");
    }

    #[test]
    fn wide_characters_count_as_two() {
        let placed = fit(&[], &[], &[seg("日本")], " ", 6);
        assert_eq!(placed[0].0, 2);
    }

    #[test]
    fn a_boxed_module_is_its_caps_around_its_ground() {
        let b = boxed(seg("cpu 3%"), "(", ")");
        let texts: Vec<(&str, Style, bool)> = b
            .iter()
            .map(|p| (p.text.as_str(), p.style, p.boxed))
            .collect();
        assert_eq!(
            texts,
            vec![
                ("(", Style::Cap, false),
                ("cpu 3%", Style::Normal, true),
                (")", Style::Cap, false)
            ]
        );
        assert!(
            boxed(Vec::new(), "(", ")").is_empty(),
            "an empty module takes no room"
        );
        assert_eq!(boxed(seg("x"), "", ""), seg("x"), "no caps, no ground");
    }

    #[test]
    fn the_nested_sign_is_centred_in_its_cells() {
        // Even, with the glyph's two cells in the middle two.
        assert_eq!(NESTED_SIGN.width(), 4);
        assert_eq!(NESTED_SIGN.chars().position(|c| c == '⧉'), Some(1));
    }

    #[test]
    fn exec_returns_the_first_line() {
        assert_eq!(
            run_command("printf 'one\\ntwo\\n'", EXEC_TIMEOUT),
            Ok("one".into())
        );
        assert!(run_command("exit 3", EXEC_TIMEOUT).is_err());
    }

    #[test]
    fn exec_timeout_kills_orphaned_children_too() {
        // The grandchild keeps stdout open; killing only the shell would hang here.
        let start = Instant::now();
        let r = run_command("sleep 30 & echo started", Duration::from_millis(300));
        assert!(r.is_err(), "{r:?}");
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn format_substitutes() {
        assert_eq!(format_output(Some("load %s"), "0.5"), "load 0.5");
        assert_eq!(format_output(None, "x"), "x");
    }

    #[test]
    fn due_times_are_in_the_future_and_bounded() {
        let now = Instant::now();
        let d = next_due(now, Duration::from_secs(60));
        assert!(d > now && d <= now + Duration::from_secs(61));
    }
}
