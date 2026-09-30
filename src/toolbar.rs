//! Toolbars: named rows of buttons, each a label and an action (DESIGN.md, "A
//! mobile view"). This is the pure part: where each face goes in a row, which
//! label it shows, and which face a cell belongs to. The handoff
//! (`doc/handoffs/done/MOBILE_VIEW_MOCK.txt`, sections 01-02) is the spec.

use unicode_width::UnicodeWidthStr;

use crate::layout::Rect;

/// Where a toolbar sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    Top,
    Bottom,
    /// In the bar's rows, on the right, when it fits there; else at the bottom.
    Beside,
}

/// Normal is one row, packed at natural width; large is three rows, stretched
/// across the row, big enough for a thumb.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Size {
    #[default]
    Normal,
    Large,
}

impl Size {
    pub fn rows(self) -> u16 {
        match self {
            Size::Normal => 1,
            Size::Large => 3,
        }
    }

    /// The narrowest face: the label and a column on each side, and at large
    /// size at least five columns (the handoff's minimum touch target).
    fn min_face(self) -> u16 {
        match self {
            Size::Normal => 3,
            Size::Large => 5,
        }
    }
}

/// A button's two labels: `label` always (a symbol, or short text), `text`
/// beside it when the face has room (`≡ menu`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub label: String,
    pub text: Option<String>,
}

impl Label {
    fn both(&self) -> Option<String> {
        self.text.as_ref().map(|t| format!("{} {t}", self.label))
    }

    /// What a face `w` wide shows: both labels if they fit with a column each
    /// side, else the first.
    pub fn fit(&self, w: u16) -> String {
        match self.both() {
            Some(b) if b.width() as u16 + 2 <= w => b,
            _ => self.label.clone(),
        }
    }

    /// The face width wanted at natural size: both labels when `full`.
    fn natural(&self, size: Size, full: bool) -> u16 {
        let text = if full {
            self.both().unwrap_or_else(|| self.label.clone())
        } else {
            self.label.clone()
        };
        (text.width() as u16 + 2).max(size.min_face())
    }
}

/// What a face stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The button at this index of the toolbar's list.
    Button(usize),
    /// `⋯`: the buttons from this index on, which did not fit.
    More(usize),
}

/// One face, laid out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Face {
    pub slot: Slot,
    pub rect: Rect,
    pub label: String,
    /// The last column this face answers a tap in: the gap after it belongs to
    /// it, so every cell of a stretched row does something.
    pub hit_right: u16,
}

pub const MORE: &str = "⋯";

/// Lay out `labels` across `area` (its height is the size's rows). Past what
/// fits, the last face becomes `⋯` for the rest. Large faces stretch across the
/// row and share out the spare columns; normal ones pack at natural width, and
/// drop their text before anything is cut.
pub fn layout(labels: &[Label], area: Rect, size: Size) -> Vec<Face> {
    if labels.is_empty() || area.w == 0 || area.h == 0 {
        return Vec::new();
    }
    let min = size.min_face();
    let max_faces = ((area.w + 1) / (min + 1)).max(1) as usize;
    let mut slots: Vec<Slot> = (0..labels.len()).map(Slot::Button).collect();
    if slots.len() > max_faces {
        slots.truncate(max_faces - 1);
        slots.push(Slot::More(max_faces - 1));
    }
    let more = Label {
        label: MORE.into(),
        text: Some("more".into()),
    };
    let label_of = |s: &Slot| match s {
        Slot::Button(i) => &labels[*i],
        Slot::More(_) => &more,
    };
    let n = slots.len() as u16;
    let widths: Vec<u16> = match size {
        Size::Large => {
            let room = area.w.saturating_sub(n - 1);
            (0..n).map(|i| (i + 1) * room / n - i * room / n).collect()
        }
        Size::Normal => {
            let natural = |full| -> Vec<u16> {
                slots
                    .iter()
                    .map(|s| label_of(s).natural(size, full))
                    .collect()
            };
            let total = |w: &[u16]| w.iter().sum::<u16>() + n - 1;
            let full = natural(true);
            if total(&full) <= area.w {
                full
            } else {
                natural(false)
            }
        }
    };
    let mut x = area.x;
    let mut faces = Vec::new();
    for (i, (slot, w)) in slots.iter().zip(&widths).enumerate() {
        let w = (*w).min(area.right().saturating_sub(x));
        if w == 0 {
            break;
        }
        let last = i + 1 == slots.len();
        faces.push(Face {
            slot: *slot,
            rect: Rect::new(x, area.y, w, area.h),
            label: label_of(slot).fit(w),
            hit_right: if last {
                x + w - 1
            } else {
                (x + w).min(area.right() - 1)
            },
        });
        x += w + 1;
    }
    faces
}

/// The face a tap at (`x`, `y`) lands on.
pub fn hit(faces: &[Face], x: u16, y: u16) -> Option<&Face> {
    faces
        .iter()
        .find(|f| y >= f.rect.y && y < f.rect.bottom() && x >= f.rect.x && x <= f.hit_right)
}

/// The width a toolbar needs at its smallest: every face at the minimum, as
/// many as there are (up to what `⋯` stands in for, which is never less).
pub fn min_width(buttons: usize, size: Size) -> u16 {
    let n = buttons.max(1) as u16;
    n * size.min_face() + (n - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch() -> Vec<Label> {
        [
            ("≡", "menu"),
            ("+", "new"),
            ("◀", "prev"),
            ("▶", "next"),
            ("⌃", "ctrl"),
            ("⎋", "esc"),
            ("⊞", "spaces"),
            ("✕", "close"),
        ]
        .iter()
        .map(|(l, t)| Label {
            label: (*l).into(),
            text: Some((*t).into()),
        })
        .collect()
    }

    /// The handoff's section 02: eight large faces stretched across 52
    /// columns are 5 or 6 wide, one column apart, filling the row.
    #[test]
    fn eight_large_buttons_fill_a_phone_row() {
        let faces = layout(&touch(), Rect::new(0, 31, 52, 3), Size::Large);
        assert_eq!(faces.len(), 8);
        assert!(faces.iter().all(|f| f.rect.w == 5 || f.rect.w == 6));
        assert!(faces.iter().all(|f| f.rect.h == 3 && f.rect.y == 31));
        assert_eq!(faces[0].rect.x, 0);
        assert_eq!(faces[7].rect.right(), 52);
        for w in faces.windows(2) {
            assert_eq!(w[1].rect.x, w[0].rect.right() + 1, "one column apart");
        }
        // Too narrow for "≡ menu": the symbol alone.
        assert_eq!(faces[0].label, "≡");
        // The mock row, cell for cell (MOBILE_VIEW_MOCK.txt, tbspec).
        let mut row = vec![' '; 52];
        for f in &faces {
            let lw = f.label.width() as u16;
            let x = f.rect.x + (f.rect.w - lw) / 2;
            row[x as usize] = f.label.chars().next().unwrap();
        }
        let row: String = row.into_iter().collect();
        assert_eq!(
            row.trim_end(),
            "  ≡     +      ◀     ▶      ⌃      ⎋     ⊞      ✕"
        );
    }

    #[test]
    fn past_what_fits_the_last_face_is_more() {
        let mut ten = touch();
        ten.extend(touch().into_iter().take(2));
        let faces = layout(&ten, Rect::new(0, 0, 52, 3), Size::Large);
        assert_eq!(faces.len(), 8);
        assert_eq!(faces[7].slot, Slot::More(7));
        assert_eq!(faces[7].label, "⋯");
        assert_eq!(faces[6].slot, Slot::Button(6));
    }

    #[test]
    fn normal_faces_pack_and_drop_their_text_before_anything_is_cut() {
        // Room for both labels: "≡ menu" and the rest at natural width.
        let faces = layout(&touch(), Rect::new(0, 0, 200, 1), Size::Normal);
        assert_eq!(faces[0].label, "≡ menu");
        assert_eq!(faces[0].rect.w, 8);
        assert_eq!(faces[1].rect.x, 9);
        // At 52 columns only the symbols fit: 8 faces of 3, 7 gaps.
        let faces = layout(&touch(), Rect::new(0, 0, 52, 1), Size::Normal);
        assert_eq!(faces.len(), 8);
        assert!(faces.iter().all(|f| f.rect.w == 3 && f.label.width() == 1));
        assert_eq!(faces[7].rect.right(), 31, "packed, not stretched");
    }

    #[test]
    fn a_gap_answers_for_the_face_on_its_left() {
        let faces = layout(&touch(), Rect::new(0, 31, 52, 3), Size::Large);
        let gap = faces[0].rect.right();
        assert_eq!(hit(&faces, gap, 32).map(|f| f.slot), Some(Slot::Button(0)));
        assert_eq!(
            hit(&faces, gap + 1, 32).map(|f| f.slot),
            Some(Slot::Button(1))
        );
        assert_eq!(hit(&faces, 51, 33).map(|f| f.slot), Some(Slot::Button(7)));
        assert_eq!(hit(&faces, 10, 30), None, "above the row");
        // Packed: the cells after the last face do nothing.
        let faces = layout(&touch(), Rect::new(0, 0, 52, 1), Size::Normal);
        assert_eq!(hit(&faces, 40, 0), None);
    }
}
