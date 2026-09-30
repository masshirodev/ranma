//! A workspace: a tiled tree, a floating layer above it, and its own focus.

use crate::layout::{PaneId, Rect, Tree};

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    pub tree: Tree,
    /// Floating panes and where they sit, bottom first: the last one is on top.
    pub floating: Vec<(PaneId, Rect)>,
    pub focused: Option<PaneId>,
    pub fullscreen: bool,
    /// A pane here rang the bell while the workspace was not shown.
    pub urgent: bool,
    /// A name given with rename_workspace; the bar shows it after the number.
    /// A named workspace stays even while empty: the name says it is wanted.
    pub name: Option<String>,
    /// Where tiled panes last floated, so floating one again puts it back there.
    pub float_memory: std::collections::HashMap<PaneId, Rect>,
    /// The tile last focused: the one `monocle` keeps on screen while a float
    /// has the focus. Defaulted, so an upgrade from a build without it reads.
    #[serde(default)]
    pub last_tile: Option<PaneId>,
}

impl Workspace {
    pub fn is_empty(&self) -> bool {
        self.tree.is_empty() && self.floating.is_empty()
    }

    pub fn contains(&self, id: PaneId) -> bool {
        self.is_floating(id) || self.tree.contains(id)
    }

    pub fn is_floating(&self, id: PaneId) -> bool {
        self.floating.iter().any(|(p, _)| *p == id)
    }

    pub fn panes(&self) -> Vec<PaneId> {
        let mut out = self.tree.panes();
        out.extend(self.floating.iter().map(|(p, _)| *p));
        out
    }

    pub fn len(&self) -> usize {
        self.tree.panes().len() + self.floating.len()
    }

    /// Take a pane out, wherever it is. `Some(Some(rect))` if it was floating,
    /// `Some(None)` if it was tiled, `None` if it is not here. Focus is left to
    /// the caller, which knows where it should go.
    pub fn take(&mut self, id: PaneId) -> Option<Option<Rect>> {
        if let Some(i) = self.floating.iter().position(|(p, _)| *p == id) {
            let (_, r) = self.floating.remove(i);
            return Some(Some(r));
        }
        if self.tree.remove(id) {
            return Some(None);
        }
        None
    }

    /// Put a floating pane on top of the others.
    pub fn raise(&mut self, id: PaneId) {
        if let Some(i) = self.floating.iter().position(|(p, _)| *p == id) {
            let f = self.floating.remove(i);
            self.floating.push(f);
        }
    }

    /// Where a new float goes: offset down and right from the topmost float, so
    /// a pile of them shows every title bar, or centred at `pw`% x `ph`% of
    /// `area` when there is none (or the cascade would run off the area).
    pub fn cascade(&self, area: Rect, pw: u16, ph: u16) -> Rect {
        let fresh = area.centered(pw, ph);
        match self.floating.last() {
            None => fresh,
            Some((_, last)) => {
                let next = Rect::new(last.x + 3, last.y + 1, last.w, last.h);
                if next.right() > area.right() || next.bottom() > area.bottom() {
                    fresh
                } else {
                    next
                }
            }
        }
    }

    /// Raise the bottom-most float and return it: called repeatedly, this
    /// brings each float of a pile to the top in turn.
    pub fn cycle_floats(&mut self) -> Option<PaneId> {
        if self.floating.is_empty() {
            return None;
        }
        let bottom = self.floating.remove(0);
        let id = bottom.0;
        self.floating.push(bottom);
        self.focused = Some(id);
        Some(id)
    }

    pub fn float_rect_mut(&mut self, id: PaneId) -> Option<&mut Rect> {
        self.floating
            .iter_mut()
            .find(|(p, _)| *p == id)
            .map(|(_, r)| r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Placement;

    #[test]
    fn take_reports_where_the_pane_was() {
        let mut ws = Workspace::default();
        ws.tree.insert(1, None, None, Placement::Dwindle);
        ws.floating.push((2, Rect::new(1, 1, 10, 5)));
        assert_eq!(ws.len(), 2);
        assert_eq!(ws.take(2), Some(Some(Rect::new(1, 1, 10, 5))));
        assert_eq!(ws.take(1), Some(None));
        assert_eq!(ws.take(1), None);
        assert!(ws.is_empty());
    }

    #[test]
    fn floats_cascade_and_wrap() {
        let area = Rect::new(0, 0, 100, 40);
        let mut ws = Workspace::default();
        let first = ws.cascade(area, 60, 60);
        assert_eq!(first, area.centered(60, 60));
        ws.floating.push((1, first));
        let second = ws.cascade(area, 60, 60);
        assert_eq!((second.x, second.y), (first.x + 3, first.y + 1));
        assert_eq!((second.w, second.h), (first.w, first.h));
        // Off the bottom-right edge: start again from the centre.
        ws.floating.push((2, Rect::new(60, 30, 40, 10)));
        assert_eq!(ws.cascade(area, 60, 60), area.centered(60, 60));
    }

    #[test]
    fn cycling_brings_each_float_to_the_top() {
        let mut ws = Workspace::default();
        for id in 1..=3 {
            ws.floating.push((id, Rect::default()));
        }
        assert_eq!(ws.cycle_floats(), Some(1));
        assert_eq!(ws.cycle_floats(), Some(2));
        assert_eq!(ws.cycle_floats(), Some(3));
        assert_eq!(ws.cycle_floats(), Some(1));
        assert_eq!(ws.floating.last().unwrap().0, 1);
        assert_eq!(Workspace::default().cycle_floats(), None);
    }

    #[test]
    fn raise_moves_to_the_top() {
        let mut ws = Workspace::default();
        ws.floating.push((1, Rect::default()));
        ws.floating.push((2, Rect::default()));
        ws.raise(1);
        assert_eq!(ws.floating.last().unwrap().0, 1);
    }
}
