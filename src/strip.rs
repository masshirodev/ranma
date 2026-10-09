//! The scrolling layout's arithmetic (`layout = "scrolling"`, niri's): a
//! workspace as a strip of columns wider than the screen, and a view onto
//! it that follows the focused column. Designed from
//! `doc/briefs/done/SCROLLING_LAYOUT.md`; the handoff is
//! `doc/handoffs/done/SCROLLING_LAYOUT.html`.
//!
//! Pure: widths, the stops `column_width next` walks, and where the view
//! goes. The tree's shape lives in `layout` (`Tree::arrange_strip` and the
//! other strip operations); a column's width is its weight in the tree's
//! root row, which keeps it with the column through every tree operation.
//! A weight up to 1 is a fraction of the screen, above it whole cells: a
//! column is never narrower than `scroll_min`, at least 20 cells, so the two
//! never meet.

use serde::Deserialize;

/// A column width as configured: a fraction of the screen or whole cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    /// `a/b` of the screen's width.
    Frac(u16, u16),
    Cells(u16),
}

/// niri's presets: a third, a half, two thirds.
pub const DEFAULT_WIDTHS: [Width; 3] = [Width::Frac(1, 3), Width::Frac(1, 2), Width::Frac(2, 3)];

impl std::fmt::Display for Width {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Width::Frac(a, b) => write!(f, "{a}/{b}"),
            Width::Cells(c) => write!(f, "{c}"),
        }
    }
}

impl Width {
    /// The weight a column of this width carries in the tree.
    pub fn weight(self) -> f32 {
        match self {
            Width::Frac(a, b) => a as f32 / b as f32,
            Width::Cells(c) => c as f32,
        }
    }
}

/// A width as `ranma.set` takes it: `"1/2"` or a whole number of cells.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WidthSetting {
    Cells(i64),
    Text(String),
}

impl WidthSetting {
    /// Checked: a fraction in (0, 1], or cells at least `min`.
    pub fn parse(&self, min: u16, who: &str) -> Result<Width, String> {
        match self {
            WidthSetting::Cells(c) => {
                if *c < min as i64 || *c > 10_000 {
                    return Err(format!(
                        "{who}: {c} cells is narrower than scroll_min ({min}) or absurdly wide"
                    ));
                }
                Ok(Width::Cells(*c as u16))
            }
            WidthSetting::Text(t) => {
                let frac = t.split_once('/').and_then(|(a, b)| {
                    let (a, b): (u16, u16) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
                    (a > 0 && b > 0 && a <= b).then_some(Width::Frac(a, b))
                });
                frac.ok_or_else(|| {
                    format!("{who}: `{t}` is not a width (a fraction such as \"1/2\", or cells)")
                })
            }
        }
    }
}

/// When the view centres the focused column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Center {
    /// Move the view the least that shows the focused column whole.
    Never,
    Always,
    /// Only when the focused column and the one focus came from do not fit
    /// on screen together (niri's `on-overflow`).
    Overflow,
}

/// A column's width in cells on a `screen`-wide workspace: its weight
/// resolved, never under `min`, never wider than the screen. A screen
/// narrower than `min` makes every column its full width.
pub fn cells(weight: f32, screen: u16, min: u16) -> u16 {
    if screen <= min {
        return screen;
    }
    let c = if weight <= 1.0 {
        (weight * screen as f32).round() as u16
    } else {
        weight as u16
    };
    c.clamp(min, screen)
}

/// The widths `column_width next` steps through, in cells, narrowest first;
/// presets landing on the same width are one stop. Full width is not one:
/// that is `column_width full`.
pub fn stops(widths: &[Width], screen: u16, min: u16) -> Vec<(u16, f32)> {
    let mut out: Vec<(u16, f32)> = widths
        .iter()
        .map(|w| (cells(w.weight(), screen, min), w.weight()))
        .collect();
    out.sort_by_key(|(c, _)| *c);
    out.dedup_by_key(|(c, _)| *c);
    out
}

/// The weight `column_width next` gives a column now `current` cells wide:
/// the first stop wider than it, else the first (it wraps).
pub fn next_stop(widths: &[Width], current: u16, screen: u16, min: u16) -> Option<f32> {
    let s = stops(widths, screen, min);
    s.iter()
        .find(|(c, _)| *c > current)
        .or(s.first())
        .map(|(_, w)| *w)
}

/// The previous stop: the last narrower than `current`, else the last.
pub fn prev_stop(widths: &[Width], current: u16, screen: u16, min: u16) -> Option<f32> {
    let s = stops(widths, screen, min);
    s.iter()
        .rev()
        .find(|(c, _)| *c < current)
        .or(s.last())
        .map(|(_, w)| *w)
}

/// Each column's left edge on the strip, from their widths and the gap
/// between them; and the strip's whole width.
pub fn offsets(widths: &[u16], gap: u16) -> (Vec<u32>, u32) {
    let mut x = 0u32;
    let mut xs = Vec::with_capacity(widths.len());
    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            x += gap as u32;
        }
        xs.push(x);
        x += *w as u32;
    }
    (xs, x)
}

/// Where the view's left edge goes so the focused column shows: the least
/// move from `view` (`Center::Never`), or centred. `from` is the column focus
/// came from, which `Center::Overflow` asks about. Clamped so the view never
/// runs past either end of the strip.
pub fn follow(
    view: u32,
    xs: &[u32],
    widths: &[u16],
    focus: usize,
    from: Option<usize>,
    screen: u16,
    center: Center,
) -> u32 {
    let (Some(&x), Some(&w)) = (xs.get(focus), widths.get(focus)) else {
        return view;
    };
    let (w, sw) = (w as u32, screen as u32);
    let total = xs
        .last()
        .zip(widths.last())
        .map_or(0, |(x, w)| x + *w as u32);
    let centred = (x + w / 2).saturating_sub(sw / 2);
    let least = if x < view {
        x
    } else if x + w > view + sw {
        x + w - sw
    } else {
        view
    };
    let to = match center {
        Center::Never => least,
        Center::Always => centred,
        Center::Overflow => {
            let together = from.filter(|f| *f != focus).and_then(|f| {
                let (fx, fw) = (*xs.get(f)?, *widths.get(f)? as u32);
                Some((x + w).max(fx + fw) - x.min(fx) <= sw)
            });
            match together {
                Some(false) => centred,
                _ => least,
            }
        }
    };
    to.min(total.saturating_sub(sw))
}

/// How many columns are not wholly on screen to the left and to the right
/// of a view at `view`: what the edges count.
pub fn beyond(xs: &[u32], widths: &[u16], view: u32, screen: u16) -> (usize, usize) {
    let right_edge = view + screen as u32;
    let left = xs.iter().filter(|x| **x < view).count();
    let right = xs
        .iter()
        .zip(widths)
        .filter(|(x, w)| **x + **w as u32 > right_edge)
        .count();
    (left, right)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Board 04: the stops for e and E on each screen width.
    #[test]
    fn widths_round_to_cells_with_a_floor() {
        let w = DEFAULT_WIDTHS;
        let cells_of = |sw| {
            stops(&w, sw, 40)
                .iter()
                .map(|(c, _)| *c)
                .collect::<Vec<_>>()
        };
        assert_eq!(cells_of(52), [40]);
        assert_eq!(cells_of(80), [40, 53]);
        assert_eq!(cells_of(110), [40, 55, 73]);
        assert_eq!(cells_of(120), [40, 60, 80]);
        assert_eq!(cells_of(200), [67, 100, 133]);
        assert_eq!(
            cells(0.5, 30, 40),
            30,
            "narrower than scroll_min: full width"
        );
        assert_eq!(cells(72.0, 60, 40), 60, "hand-sized, clamped to the screen");
    }

    #[test]
    fn next_and_prev_stop_wrap() {
        let w = DEFAULT_WIDTHS;
        assert_eq!(next_stop(&w, 40, 120, 40), Some(0.5));
        assert_eq!(next_stop(&w, 80, 120, 40), Some(1.0 / 3.0), "wraps");
        assert_eq!(
            next_stop(&w, 45, 120, 40),
            Some(0.5),
            "hand-sized: first wider"
        );
        assert_eq!(prev_stop(&w, 40, 120, 40), Some(2.0 / 3.0), "wraps");
    }

    #[test]
    fn settings_parse_strictly() {
        let p = |s: WidthSetting| s.parse(40, "scroll_width");
        assert_eq!(p(WidthSetting::Text("1/2".into())), Ok(Width::Frac(1, 2)));
        assert_eq!(p(WidthSetting::Cells(72)), Ok(Width::Cells(72)));
        assert!(p(WidthSetting::Cells(30)).is_err());
        assert!(p(WidthSetting::Text("3/2".into())).is_err());
        assert!(p(WidthSetting::Text("half".into())).is_err());
        assert!(p(WidthSetting::Text("0/2".into())).is_err());
    }

    /// Board 01: zsh 40, nvim 60, btop · logs 40, claude 60 on 120 cells.
    #[test]
    fn the_view_moves_the_least_and_stops_at_the_ends() {
        let widths = [40, 60, 40, 60];
        let (xs, total) = offsets(&widths, 0);
        assert_eq!((xs.as_slice(), total), ([0, 40, 100, 140].as_slice(), 200));
        let f = |view, focus, center| follow(view, &xs, &widths, focus, None, 120, center);
        assert_eq!(f(0, 1, Center::Never), 0, "nvim already shows");
        assert_eq!(
            f(0, 2, Center::Never),
            20,
            "btop: 20 cells along, no further"
        );
        assert_eq!(f(20, 3, Center::Never), 80, "claude: the end of the strip");
        assert_eq!(f(80, 0, Center::Never), 0);
        assert_eq!(f(0, 1, Center::Always), 10);
        assert_eq!(f(0, 3, Center::Always), 80, "clamped at the end");
        assert_eq!(beyond(&xs, &widths, 0, 120), (0, 2));
        assert_eq!(beyond(&xs, &widths, 80, 120), (2, 0));
    }

    #[test]
    fn overflow_centres_only_when_the_two_do_not_fit() {
        let widths = [40, 40, 40, 100];
        let (xs, _) = offsets(&widths, 0);
        let f = |view, focus, from| follow(view, &xs, &widths, focus, from, 120, Center::Overflow);
        assert_eq!(f(0, 2, Some(1)), 0, "both fit: the least move");
        assert_eq!(f(0, 3, Some(1)), 100, "40 + 100 do not: centred, clamped");
    }
}
