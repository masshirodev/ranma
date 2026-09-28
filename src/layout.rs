//! The container tree of one workspace, and its geometry.
//!
//! i3's model: a container holds panes or other containers, split horizontally or
//! vertically, each child with a weight. Hyprland's dwindle is a placement policy
//! on top of it (split the focused pane along its longer side), not a different
//! structure, so both feels share this one tree.
//!
//! Everything here is pure: no PTYs, no terminal. Directional focus and movement
//! work on the computed rectangles, not on tree order, which is what makes focus
//! go where the eye expects.

use crate::action::Dir;

pub type PaneId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> u16 {
        self.x + self.w
    }
    pub fn bottom(&self) -> u16 {
        self.y + self.h
    }
    /// Shrink by `h` columns on the left and right and `v` rows on top and bottom.
    pub fn inset(&self, h: u16, v: u16) -> Rect {
        let w = self.w.saturating_sub(h * 2);
        let hh = self.h.saturating_sub(v * 2);
        Rect::new(
            self.x + h.min(self.w / 2),
            self.y + v.min(self.h / 2),
            w,
            hh,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    /// Children side by side, left to right.
    Horizontal,
    /// Children stacked, top to bottom.
    Vertical,
}

impl Split {
    fn flipped(self) -> Split {
        match self {
            Split::Horizontal => Split::Vertical,
            Split::Vertical => Split::Horizontal,
        }
    }
    fn of(dir: Dir) -> Split {
        match dir {
            Dir::Left | Dir::Right => Split::Horizontal,
            Dir::Up | Dir::Down => Split::Vertical,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Pane(PaneId),
    Container {
        split: Split,
        /// Children with their weights. Weights are relative, not fractions.
        children: Vec<(Node, f32)>,
    },
}

/// How a new pane enters the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Split the focused pane along its longer side.
    Dwindle,
    /// Split the focused pane in this direction.
    Manual(Split),
}

/// The tree of one workspace, possibly empty.
#[derive(Debug, Clone, Default)]
pub struct Tree {
    pub root: Option<Node>,
}

/// A terminal cell is about twice as tall as it is wide; dwindle compares
/// visual length, not cell counts, or every split of a wide pane is vertical.
const CELL_ASPECT: u32 = 2;

impl Tree {
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn panes(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        if let Some(r) = &self.root {
            collect(r, &mut out);
        }
        out
    }

    /// Insert `new` next to `focused` (or as the root when the tree is empty).
    /// `focused_rect` is where `focused` is on screen, which dwindle needs.
    pub fn insert(
        &mut self,
        new: PaneId,
        focused: Option<PaneId>,
        focused_rect: Option<Rect>,
        placement: Placement,
    ) {
        let Some(root) = self.root.as_mut() else {
            self.root = Some(Node::Pane(new));
            return;
        };
        let split = match placement {
            Placement::Manual(s) => s,
            Placement::Dwindle => match focused_rect {
                Some(r) if (r.h as u32) * CELL_ASPECT > r.w as u32 => Split::Vertical,
                _ => Split::Horizontal,
            },
        };
        let target = focused.filter(|f| find_path(root, *f).is_some());
        match target {
            Some(f) => insert_at(root, f, new, split),
            // No usable focus: append at the top level, the least surprising place.
            None => {
                let old = std::mem::replace(root, Node::Pane(new));
                *root = Node::Container {
                    split,
                    children: vec![(old, 1.0), (Node::Pane(new), 1.0)],
                };
            }
        }
    }

    /// Remove a pane, collapsing containers left with a single child.
    pub fn remove(&mut self, id: PaneId) -> bool {
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        if *root == Node::Pane(id) {
            self.root = None;
            return true;
        }
        let removed = remove_in(root, id);
        if removed {
            normalize(root);
        }
        removed
    }

    /// Flip the split of the container directly holding `id` (Hyprland's togglesplit).
    pub fn toggle_split(&mut self, id: PaneId) -> bool {
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        let Some(path) = find_path(root, id) else {
            return false;
        };
        if path.is_empty() {
            return false;
        }
        if let Node::Container { split, .. } = node_at_mut(root, &path[..path.len() - 1]) {
            *split = split.flipped();
            return true;
        }
        false
    }

    /// Swap two panes' places in the tree.
    pub fn swap(&mut self, a: PaneId, b: PaneId) -> bool {
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        let (Some(pa), Some(pb)) = (find_path(root, a), find_path(root, b)) else {
            return false;
        };
        *node_at_mut(root, &pa) = Node::Pane(b);
        *node_at_mut(root, &pb) = Node::Pane(a);
        true
    }

    /// Move the edge of `id` facing `dir` by `cells`, growing the pane.
    ///
    /// Adjusts the nearest ancestor split along that axis where the pane has a
    /// sibling on that side. Returns false when there is no such edge (the pane
    /// already touches the workspace border there).
    pub fn resize(&mut self, id: PaneId, dir: Dir, cells: u16, area: Rect, inner_gap: u16) -> bool {
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        let Some(path) = find_path(root, id) else {
            return false;
        };
        let want = Split::of(dir);
        let forward = matches!(dir, Dir::Right | Dir::Down);
        // Walk up from the pane to find the container to adjust, tracking the rect
        // of each container so the change can be converted from cells to weight.
        let rects = path_rects(root, &path, area, inner_gap);
        for depth in (0..path.len()).rev() {
            let idx = path[depth];
            let container = node_at_mut(root, &path[..depth]);
            let Node::Container { split, children } = container else {
                continue;
            };
            if *split != want {
                continue;
            }
            let neighbour = if forward {
                (idx + 1 < children.len()).then_some(idx + 1)
            } else {
                idx.checked_sub(1)
            };
            let Some(nb) = neighbour else {
                continue;
            };
            let crect = rects[depth];
            let span = match want {
                Split::Horizontal => crect.w,
                Split::Vertical => crect.h,
            } as f32;
            if span <= 0.0 {
                return false;
            }
            let total: f32 = children.iter().map(|(_, w)| *w).sum();
            let delta = cells as f32 / span * total;
            // Keep the neighbour at least two cells wide: one cell's worth of weight
            // can floor to zero columns once rounding is applied.
            let min = 2.0 * total / span;
            let give = delta.min(children[nb].1 - min).max(0.0);
            if give <= 0.0 {
                return false;
            }
            children[idx].1 += give;
            children[nb].1 -= give;
            return true;
        }
        false
    }

    /// Lay the tree out in `area`, `inner_gap` cells between siblings.
    pub fn layout(&self, area: Rect, inner_gap: u16) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        if let Some(r) = &self.root {
            layout_node(r, area, inner_gap, &mut out);
        }
        out
    }
}

/// The pane in direction `dir` from `from`, by geometry.
///
/// A candidate must lie entirely on that side and overlap `from` on the other
/// axis. The nearest wins; among equally near ones, the one sharing the most edge.
pub fn neighbour(rects: &[(PaneId, Rect)], from: PaneId, dir: Dir) -> Option<PaneId> {
    let cur = rects.iter().find(|(id, _)| *id == from)?.1;
    rects
        .iter()
        .filter(|(id, _)| *id != from)
        .filter_map(|(id, r)| {
            let (dist, overlap) = match dir {
                Dir::Left if r.right() <= cur.x => (
                    cur.x - r.right(),
                    span_overlap(r.y, r.bottom(), cur.y, cur.bottom()),
                ),
                Dir::Right if r.x >= cur.right() => (
                    r.x - cur.right(),
                    span_overlap(r.y, r.bottom(), cur.y, cur.bottom()),
                ),
                Dir::Up if r.bottom() <= cur.y => (
                    cur.y - r.bottom(),
                    span_overlap(r.x, r.right(), cur.x, cur.right()),
                ),
                Dir::Down if r.y >= cur.bottom() => (
                    r.y - cur.bottom(),
                    span_overlap(r.x, r.right(), cur.x, cur.right()),
                ),
                _ => return None,
            };
            (overlap > 0).then_some((*id, dist, overlap))
        })
        .min_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)))
        .map(|(id, _, _)| id)
}

fn span_overlap(a0: u16, a1: u16, b0: u16, b1: u16) -> u16 {
    a1.min(b1).saturating_sub(a0.max(b0))
}

fn collect(n: &Node, out: &mut Vec<PaneId>) {
    match n {
        Node::Pane(id) => out.push(*id),
        Node::Container { children, .. } => children.iter().for_each(|(c, _)| collect(c, out)),
    }
}

/// Child indices from the root to the pane.
fn find_path(n: &Node, id: PaneId) -> Option<Vec<usize>> {
    match n {
        Node::Pane(p) => (*p == id).then(Vec::new),
        Node::Container { children, .. } => children.iter().enumerate().find_map(|(i, (c, _))| {
            find_path(c, id).map(|mut p| {
                p.insert(0, i);
                p
            })
        }),
    }
}

fn node_at_mut<'a>(mut n: &'a mut Node, path: &[usize]) -> &'a mut Node {
    for &i in path {
        n = match n {
            Node::Container { children, .. } => &mut children[i].0,
            Node::Pane(_) => unreachable!("path runs through a pane"),
        };
    }
    n
}

fn insert_at(root: &mut Node, focused: PaneId, new: PaneId, split: Split) {
    let path = find_path(root, focused).expect("caller checked");
    // Joining the parent keeps i3's flat containers: three panes side by side are
    // one container of three, not a container nested in a container.
    if let Some((&idx, parent_path)) = path.split_last()
        && let Node::Container {
            split: psplit,
            children,
        } = node_at_mut(root, parent_path)
        && *psplit == split
    {
        // Split the focused pane's share instead of adding a full share, so the
        // rest of the row keeps its size.
        let w = children[idx].1 / 2.0;
        children[idx].1 = w;
        children.insert(idx + 1, (Node::Pane(new), w));
        return;
    }
    let leaf = node_at_mut(root, &path);
    let old = std::mem::replace(leaf, Node::Pane(new));
    *leaf = Node::Container {
        split,
        children: vec![(old, 1.0), (Node::Pane(new), 1.0)],
    };
}

fn remove_in(n: &mut Node, id: PaneId) -> bool {
    let Node::Container { children, .. } = n else {
        return false;
    };
    if let Some(i) = children.iter().position(|(c, _)| *c == Node::Pane(id)) {
        children.remove(i);
        return true;
    }
    children.iter_mut().any(|(c, _)| remove_in(c, id))
}

fn normalize(n: &mut Node) {
    if let Node::Container { children, .. } = n {
        children.iter_mut().for_each(|(c, _)| normalize(c));
        if children.len() == 1 {
            let (only, _) = children.pop().unwrap();
            *n = only;
        }
    }
}

/// Split `len` cells among weights, minus gaps, with the rounding remainder
/// handed out left to right so the parts always sum exactly.
fn distribute(len: u16, weights: &[f32], gap: u16) -> Vec<u16> {
    let n = weights.len() as u16;
    let gaps = gap.saturating_mul(n.saturating_sub(1)).min(len);
    let avail = (len - gaps) as f32;
    let total: f32 = weights.iter().sum::<f32>().max(f32::EPSILON);
    let mut sizes: Vec<u16> = weights
        .iter()
        .map(|w| (avail * w / total).floor() as u16)
        .collect();
    let mut rest = (len - gaps).saturating_sub(sizes.iter().sum());
    for s in sizes.iter_mut() {
        if rest == 0 {
            break;
        }
        *s += 1;
        rest -= 1;
    }
    sizes
}

fn child_rects(split: Split, children: &[(Node, f32)], area: Rect, gap: u16) -> Vec<Rect> {
    let weights: Vec<f32> = children.iter().map(|(_, w)| *w).collect();
    let (len, start) = match split {
        Split::Horizontal => (area.w, area.x),
        Split::Vertical => (area.h, area.y),
    };
    let mut pos = start;
    distribute(len, &weights, gap)
        .into_iter()
        .map(|size| {
            let r = match split {
                Split::Horizontal => Rect::new(pos, area.y, size, area.h),
                Split::Vertical => Rect::new(area.x, pos, area.w, size),
            };
            pos += size + gap;
            r
        })
        .collect()
}

fn layout_node(n: &Node, area: Rect, gap: u16, out: &mut Vec<(PaneId, Rect)>) {
    match n {
        Node::Pane(id) => out.push((*id, area)),
        Node::Container { split, children } => {
            for ((c, _), r) in children
                .iter()
                .zip(child_rects(*split, children, area, gap))
            {
                layout_node(c, r, gap, out);
            }
        }
    }
}

/// The rect of every container along `path`, root first.
fn path_rects(root: &Node, path: &[usize], area: Rect, gap: u16) -> Vec<Rect> {
    let mut rects = vec![area];
    let mut n = root;
    let mut r = area;
    for &i in path {
        let Node::Container { split, children } = n else {
            break;
        };
        r = child_rects(*split, children, r, gap)[i];
        n = &children[i].0;
        rects.push(r);
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        w: 200,
        h: 50,
    };

    fn rect_of(t: &Tree, id: PaneId) -> Rect {
        t.layout(AREA, 0)
            .into_iter()
            .find(|(p, _)| *p == id)
            .unwrap()
            .1
    }

    /// Open panes 1..=n the way new_pane does: each next to the last one.
    fn dwindle(n: PaneId) -> Tree {
        let mut t = Tree::default();
        for id in 1..=n {
            let focused = (id > 1).then_some(id - 1);
            let fr = focused.map(|f| rect_of(&t, f));
            t.insert(id, focused, fr, Placement::Dwindle);
        }
        t
    }

    #[test]
    fn first_pane_fills_the_area() {
        let t = dwindle(1);
        assert_eq!(t.layout(AREA, 0), vec![(1, AREA)]);
    }

    #[test]
    fn dwindle_splits_along_the_longer_visual_side() {
        // 200x50 cells is 200 wide by ~100 tall visually: split side by side.
        let t = dwindle(2);
        assert_eq!(rect_of(&t, 1), Rect::new(0, 0, 100, 50));
        assert_eq!(rect_of(&t, 2), Rect::new(100, 0, 100, 50));
        // Pane 2 is 100x50, visually square; a tie splits side by side.
        let t = dwindle(3);
        let r3 = rect_of(&t, 3);
        assert!(r3.w < 100, "{r3:?}");
    }

    #[test]
    fn dwindle_goes_vertical_in_a_tall_pane() {
        let mut t = Tree::default();
        let tall = Rect::new(0, 0, 40, 50);
        t.insert(1, None, None, Placement::Dwindle);
        t.insert(2, Some(1), Some(tall), Placement::Dwindle);
        match &t.root {
            Some(Node::Container { split, .. }) => assert_eq!(*split, Split::Vertical),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn layout_covers_the_area_exactly_with_gaps() {
        let t = dwindle(5);
        let rects = t.layout(AREA, 1);
        let area: u32 = rects.iter().map(|(_, r)| r.w as u32 * r.h as u32).sum();
        assert!(area < AREA.w as u32 * AREA.h as u32);
        for (_, r) in &rects {
            assert!(r.right() <= AREA.right() && r.bottom() <= AREA.bottom());
        }
        // No two panes overlap.
        for (i, (_, a)) in rects.iter().enumerate() {
            for (_, b) in &rects[i + 1..] {
                let ox = span_overlap(a.x, a.right(), b.x, b.right());
                let oy = span_overlap(a.y, a.bottom(), b.y, b.bottom());
                assert!(ox == 0 || oy == 0, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn distribute_sums_exactly() {
        for len in [0u16, 1, 7, 99, 200] {
            let parts = distribute(len, &[1.0, 1.0, 1.0], 1);
            let gaps = (2u16).min(len);
            assert_eq!(parts.iter().sum::<u16>() + gaps, len, "len {len}");
        }
    }

    #[test]
    fn manual_split_joins_a_matching_parent() {
        let mut t = Tree::default();
        let h = Placement::Manual(Split::Horizontal);
        t.insert(1, None, None, h);
        t.insert(2, Some(1), None, h);
        t.insert(3, Some(2), None, h);
        match &t.root {
            Some(Node::Container { children, .. }) => assert_eq!(children.len(), 3),
            other => panic!("{other:?}"),
        }
        // Pane 1 keeps its half; 2 and 3 share the other.
        assert_eq!(rect_of(&t, 1).w, 100);
        assert_eq!(rect_of(&t, 2).w, 50);
    }

    #[test]
    fn remove_collapses_single_child_containers() {
        let mut t = dwindle(3);
        assert!(t.remove(3));
        assert!(t.remove(2));
        assert_eq!(t.root, Some(Node::Pane(1)));
        assert!(t.remove(1));
        assert!(t.is_empty());
        assert!(!t.remove(1));
    }

    #[test]
    fn neighbour_uses_geometry() {
        let mut t = Tree::default();
        // 1 | 2
        //   | -
        //   | 3
        t.insert(1, None, None, Placement::Manual(Split::Horizontal));
        t.insert(2, Some(1), None, Placement::Manual(Split::Horizontal));
        t.insert(3, Some(2), None, Placement::Manual(Split::Vertical));
        let rects = t.layout(AREA, 0);
        assert_eq!(neighbour(&rects, 1, Dir::Right), Some(2));
        assert_eq!(neighbour(&rects, 3, Dir::Left), Some(1));
        assert_eq!(neighbour(&rects, 2, Dir::Down), Some(3));
        assert_eq!(neighbour(&rects, 3, Dir::Up), Some(2));
        assert_eq!(neighbour(&rects, 1, Dir::Left), None);
        assert_eq!(neighbour(&rects, 1, Dir::Up), None);
    }

    #[test]
    fn toggle_split_flips_the_parent() {
        let mut t = dwindle(2);
        assert_eq!(rect_of(&t, 2), Rect::new(100, 0, 100, 50));
        assert!(t.toggle_split(2));
        assert_eq!(rect_of(&t, 2), Rect::new(0, 25, 200, 25));
        assert!(!dwindle(1).toggle_split(1));
    }

    #[test]
    fn swap_exchanges_places() {
        let mut t = dwindle(2);
        let (r1, r2) = (rect_of(&t, 1), rect_of(&t, 2));
        assert!(t.swap(1, 2));
        assert_eq!(rect_of(&t, 1), r2);
        assert_eq!(rect_of(&t, 2), r1);
    }

    #[test]
    fn resize_moves_the_shared_edge() {
        let mut t = dwindle(2);
        assert!(t.resize(1, Dir::Right, 10, AREA, 0));
        assert_eq!(rect_of(&t, 1).w, 110);
        assert_eq!(rect_of(&t, 2).w, 90);
        assert!(t.resize(2, Dir::Left, 20, AREA, 0));
        assert_eq!(rect_of(&t, 2).w, 110);
        // Nothing to the left of pane 1: no edge to move.
        assert!(!t.resize(1, Dir::Left, 5, AREA, 0));
        // Nothing above in a side-by-side layout either.
        assert!(!t.resize(1, Dir::Up, 5, AREA, 0));
    }

    #[test]
    fn resize_never_crushes_the_neighbour() {
        let mut t = dwindle(2);
        assert!(t.resize(1, Dir::Right, 500, AREA, 0));
        assert!(rect_of(&t, 2).w >= 1);
    }

    #[test]
    fn inset_saturates() {
        assert_eq!(Rect::new(0, 0, 3, 3).inset(5, 5).w, 0);
        assert_eq!(Rect::new(2, 2, 10, 6).inset(1, 1), Rect::new(3, 3, 8, 4));
    }
}
