//! A workspace: a tiled tree, a floating layer above it, and its own focus.

use crate::layout::{PaneId, Rect, Tree};

#[derive(Debug, Default)]
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
    fn raise_moves_to_the_top() {
        let mut ws = Workspace::default();
        ws.floating.push((1, Rect::default()));
        ws.floating.push((2, Rect::default()));
        ws.raise(1);
        assert_eq!(ws.floating.last().unwrap().0, 1);
    }
}
