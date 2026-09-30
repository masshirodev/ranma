//! Chrome: how the screen's rows are shared between toolbars, the bar, the
//! monocle strip and the panes, and what gives way as the screen gets shorter
//! (DESIGN.md, "A mobile view"; the handoff's section 04).
//!
//! One pure function of the driving terminal's size and what the config asks
//! for, so the order things fold away in is one table, tested at the sizes the
//! handoff drew.

use crate::layout::Rect;
use crate::toolbar::{self, Position, Size};

/// Which side of the screen the bar is on, when it is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Top,
    Bottom,
}

/// A toolbar the config shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WantToolbar {
    pub position: Position,
    pub size: Size,
    /// How many buttons it has: what its narrowest layout needs.
    pub buttons: usize,
}

/// What the config asks for, at the driver's size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wants {
    pub cols: u16,
    pub rows: u16,
    pub bar: Option<(Side, Size)>,
    /// Monocle has more than one pane to switch between.
    pub strip: bool,
    pub toolbars: Vec<WantToolbar>,
}

/// Where the monocle strip went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strip {
    None,
    /// Its own row(s) over the pane.
    Row(Rect),
    /// Into the bar: as chips where they fit, else as one `2/4 nvim` chip.
    InBar,
}

/// A toolbar placed: its rows and the size it is drawn at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub rect: Rect,
    pub size: Size,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chrome {
    pub bar: Option<Rect>,
    pub bar_size: Size,
    /// The bar's columns left to it when a toolbar sits beside it.
    pub bar_room: u16,
    /// One per wanted toolbar, in order: `None` when it gave way.
    pub toolbars: Vec<Option<Placed>>,
    pub strip: Strip,
    /// The pane on screen goes without its border (the strip carries the title).
    pub borderless: bool,
    /// Workspaces other than the current one show only their number.
    pub compact_workspaces: bool,
    /// What is left for the panes.
    pub workspace: Rect,
}

/// The rows below which each step is taken, in order (the handoff's section
/// 04, measured on a phone with everything stacked; a toolbar beside the bar
/// counts its rows back, since it takes none of its own).
const FOLD_STRIP: u16 = 30;
const NORMAL_BAR: u16 = 24;
const NO_BORDER: u16 = 20;
const NORMAL_TOOLBAR: u16 = 12;
const NO_TOOLBAR: u16 = 8;

pub fn plan(w: &Wants) -> Chrome {
    let large =
        matches!(w.bar, Some((_, Size::Large))) || w.toolbars.iter().any(|t| t.size == Size::Large);
    let beside_fits = |t: &WantToolbar| {
        w.bar.is_some()
            && t.position == Position::Beside
            && toolbar::min_width(t.buttons, t.size) <= w.cols / 2
    };
    let saved: u16 = w
        .toolbars
        .iter()
        .filter(|t| beside_fits(t))
        .map(|t| t.size.rows())
        .sum();
    let eff = if large { w.rows + saved } else { u16::MAX };

    let mut bar_size = w.bar.map_or(Size::Normal, |(_, s)| s);
    let strip_in_bar = w.strip && w.bar.is_some() && eff < FOLD_STRIP;
    if eff < NORMAL_BAR {
        bar_size = Size::Normal;
    }
    let compact = eff < NORMAL_BAR;
    let borderless = eff < NO_BORDER;
    let toolbar_size = |s: Size| {
        if eff < NORMAL_TOOLBAR {
            Size::Normal
        } else {
            s
        }
    };
    let toolbars_shown = eff >= NO_TOOLBAR;

    let mut area = Rect::new(0, 0, w.cols, w.rows);
    let mut placed: Vec<Option<Placed>> = vec![None; w.toolbars.len()];
    // Toolbars are outermost: nearest the thumb, or the top edge.
    if toolbars_shown {
        for (i, t) in w.toolbars.iter().enumerate() {
            if beside_fits(t) {
                continue;
            }
            let size = toolbar_size(t.size);
            let h = size.rows().min(area.h);
            let rect = match t.position {
                Position::Top => {
                    let r = Rect::new(area.x, area.y, area.w, h);
                    area = Rect::new(area.x, area.y + h, area.w, area.h - h);
                    r
                }
                Position::Bottom | Position::Beside => {
                    let r = Rect::new(area.x, area.bottom() - h, area.w, h);
                    area = Rect::new(area.x, area.y, area.w, area.h - h);
                    r
                }
            };
            placed[i] = Some(Placed { rect, size });
        }
    }
    let mut bar = None;
    let mut bar_room = w.cols;
    if let Some((side, _)) = w.bar {
        let h = bar_size.rows().min(area.h);
        let r = match side {
            Side::Top => {
                let r = Rect::new(area.x, area.y, area.w, h);
                area = Rect::new(area.x, area.y + h, area.w, area.h - h);
                r
            }
            Side::Bottom => {
                let r = Rect::new(area.x, area.bottom() - h, area.w, h);
                area = Rect::new(area.x, area.y, area.w, area.h - h);
                r
            }
        };
        // Beside: in the bar's rows, from the right end, at the bar's height.
        let mut right = r.right();
        if toolbars_shown {
            for (i, t) in w.toolbars.iter().enumerate() {
                if !beside_fits(t) {
                    continue;
                }
                let size = if r.h >= Size::Large.rows() {
                    toolbar_size(t.size)
                } else {
                    Size::Normal
                };
                let tw = toolbar::min_width(t.buttons, size);
                let x = right.saturating_sub(tw).max(r.x);
                placed[i] = Some(Placed {
                    rect: Rect::new(x, r.y, right - x, r.h),
                    size,
                });
                right = x.saturating_sub(1);
            }
        }
        bar_room = right.saturating_sub(r.x);
        bar = Some(r);
    }
    let strip = if !w.strip {
        Strip::None
    } else if strip_in_bar {
        Strip::InBar
    } else {
        let h = if bar_size == Size::Large || (w.bar.is_none() && large) {
            3
        } else {
            1
        };
        let h = h.min(area.h.saturating_sub(1));
        if h == 0 {
            Strip::None
        } else {
            let r = Rect::new(area.x, area.y, area.w, h);
            area = Rect::new(area.x, area.y + h, area.w, area.h - h);
            Strip::Row(r)
        }
    };
    Chrome {
        bar,
        bar_size,
        bar_room,
        toolbars: placed,
        strip,
        borderless,
        compact_workspaces: compact,
        workspace: area,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(position: Position) -> WantToolbar {
        WantToolbar {
            position,
            size: Size::Large,
            buttons: 8,
        }
    }

    fn phone(rows: u16) -> Wants {
        Wants {
            cols: 52,
            rows,
            bar: Some((Side::Bottom, Size::Large)),
            strip: true,
            toolbars: vec![touch(Position::Bottom)],
        }
    }

    /// Section 03, keyboard closed: strip (3), pane, large bar (3), large
    /// toolbar (3) at the bottom.
    #[test]
    fn a_phone_with_room_shows_everything() {
        let c = plan(&phone(34));
        assert_eq!(c.toolbars[0].unwrap().rect, Rect::new(0, 31, 52, 3));
        assert_eq!(c.bar, Some(Rect::new(0, 28, 52, 3)));
        assert_eq!(c.strip, Strip::Row(Rect::new(0, 0, 52, 3)));
        assert_eq!(c.workspace, Rect::new(0, 3, 52, 25));
        assert!(!c.borderless && !c.compact_workspaces);
    }

    /// Section 04, in order: the strip folds into the bar, the bar goes to
    /// one row, the border goes, the toolbar goes to one row, then hides.
    #[test]
    fn what_gives_way_as_the_keyboard_opens() {
        let c = plan(&phone(29));
        assert_eq!(c.strip, Strip::InBar);
        assert_eq!(c.bar_size, Size::Large);
        assert_eq!(c.workspace, Rect::new(0, 0, 52, 23));

        let c = plan(&phone(23));
        assert_eq!((c.strip, c.bar_size), (Strip::InBar, Size::Normal));
        assert!(c.compact_workspaces && !c.borderless);

        // Gboard open (the mock's 52x18): one bar row, no border, the
        // toolbar still large.
        let c = plan(&phone(18));
        assert!(c.borderless);
        assert_eq!(c.bar, Some(Rect::new(0, 14, 52, 1)));
        assert_eq!(c.toolbars[0].unwrap().rect, Rect::new(0, 15, 52, 3));
        assert_eq!(c.workspace, Rect::new(0, 0, 52, 14));

        let c = plan(&phone(11));
        assert_eq!(c.toolbars[0].unwrap().size, Size::Normal);
        assert_eq!(c.toolbars[0].unwrap().rect.h, 1);

        let c = plan(&phone(7));
        assert_eq!(c.toolbars[0], None);
        assert_eq!(c.bar.unwrap().h, 1, "only the bar is left");
    }

    /// Section 05's alternative: the toolbar at the top, the strip under it.
    #[test]
    fn a_toolbar_on_top_keeps_the_strip_over_the_pane() {
        let mut w = phone(34);
        w.toolbars = vec![touch(Position::Top)];
        let c = plan(&w);
        assert_eq!(c.toolbars[0].unwrap().rect, Rect::new(0, 0, 52, 3));
        assert_eq!(c.strip, Strip::Row(Rect::new(0, 3, 52, 3)));
        assert_eq!(c.bar, Some(Rect::new(0, 31, 52, 3)));
    }

    /// Section 09: phone landscape, 110x22. The toolbar fits beside the bar,
    /// so all the chrome takes 3 rows and the strip goes into the bar.
    #[test]
    fn landscape_puts_the_toolbar_beside_the_bar() {
        let w = Wants {
            cols: 110,
            rows: 22,
            bar: Some((Side::Bottom, Size::Large)),
            strip: true,
            toolbars: vec![touch(Position::Beside)],
        };
        let c = plan(&w);
        assert_eq!(c.bar, Some(Rect::new(0, 19, 110, 3)));
        let t = c.toolbars[0].unwrap();
        assert_eq!(t.rect, Rect::new(63, 19, 47, 3));
        assert_eq!(c.bar_room, 62);
        assert_eq!(c.strip, Strip::InBar);
        assert_eq!(c.workspace, Rect::new(0, 0, 110, 19));
        assert!(!c.borderless);
        // On the phone upright it does not fit beside, and goes to the bottom.
        let c = plan(&Wants {
            cols: 52,
            rows: 34,
            ..w
        });
        assert_eq!(c.toolbars[0].unwrap().rect, Rect::new(0, 31, 52, 3));
    }

    /// Section 10: the tablet, dwindle (no strip), large bar and toolbar.
    #[test]
    fn a_tablet_keeps_both_large() {
        let w = Wants {
            cols: 120,
            rows: 40,
            bar: Some((Side::Bottom, Size::Large)),
            strip: false,
            toolbars: vec![touch(Position::Bottom)],
        };
        let c = plan(&w);
        assert_eq!(c.bar, Some(Rect::new(0, 34, 120, 3)));
        assert_eq!(c.toolbars[0].unwrap().rect, Rect::new(0, 37, 120, 3));
        assert_eq!(c.workspace, Rect::new(0, 0, 120, 34));
    }

    /// Section 11: the desktop, all normal size. Nothing folds, however few
    /// the rows: the ladder is for large chrome.
    #[test]
    fn normal_chrome_never_folds() {
        let w = Wants {
            cols: 200,
            rows: 50,
            bar: Some((Side::Bottom, Size::Normal)),
            strip: true,
            toolbars: vec![WantToolbar {
                position: Position::Top,
                size: Size::Normal,
                buttons: 7,
            }],
        };
        let c = plan(&w);
        assert_eq!(c.toolbars[0].unwrap().rect, Rect::new(0, 0, 200, 1));
        assert_eq!(c.strip, Strip::Row(Rect::new(0, 1, 200, 1)));
        assert_eq!(c.bar, Some(Rect::new(0, 49, 200, 1)));
        assert_eq!(c.workspace, Rect::new(0, 2, 200, 47));
        let c = plan(&Wants { rows: 10, ..w });
        assert_eq!(c.strip, Strip::Row(Rect::new(0, 1, 200, 1)));
        assert!(!c.borderless && c.toolbars[0].is_some());
    }

    #[test]
    fn nothing_asked_for_is_the_whole_screen() {
        let c = plan(&Wants {
            cols: 80,
            rows: 24,
            bar: None,
            strip: false,
            toolbars: Vec::new(),
        });
        assert_eq!(c.workspace, Rect::new(0, 0, 80, 24));
        assert_eq!(c.bar, None);
    }
}
