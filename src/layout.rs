//! The container tree of one workspace, and its geometry.
//!
//! i3's model: a container holds panes or other containers, split horizontally or
//! vertically, each child with a weight. Hyprland's dwindle is a placement policy
//! on top of it (split the focused pane along its longer side), not a different
//! structure, so both feels share this one tree.
//!
//! A container can also be *tabbed* (a group, in Hyprland's words): one child is
//! shown at a time under a one-row tab bar. It keeps its split, so untabbing it
//! puts things back the way they were.
//!
//! Everything here is pure: no PTYs, no terminal. Directional focus and movement
//! work on the computed rectangles, not on tree order, which is what makes focus
//! go where the eye expects.

use serde::{Deserialize, Serialize};

use crate::action::{Dir, Snap};

pub type PaneId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
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
    /// Inset by each side's own amount: top, right, bottom, left. Each axis
    /// gives way to zero size rather than past it.
    pub fn inset_sides(&self, [top, right, bottom, left]: [u16; 4]) -> Rect {
        let w = self.w.saturating_sub(left.saturating_add(right));
        let h = self.h.saturating_sub(top.saturating_add(bottom));
        Rect::new(self.x + left.min(self.w), self.y + top.min(self.h), w, h)
    }
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }
    /// A `pw`% by `ph`% rect centred in this one.
    pub fn centered(&self, pw: u16, ph: u16) -> Rect {
        let w = (self.w as u32 * pw as u32 / 100) as u16;
        let h = (self.h as u32 * ph as u32 / 100) as u16;
        Rect::new(self.x + (self.w - w) / 2, self.y + (self.h - h) / 2, w, h)
    }
    /// This rect resized to `pw`% by `ph`% of `area`, around the same centre,
    /// then kept inside `area`.
    pub fn resized_in(&self, area: Rect, pw: u8, ph: u8) -> Rect {
        let w = (area.w as u32 * pw as u32 / 100) as u16;
        let h = (area.h as u32 * ph as u32 / 100) as u16;
        let cx = self.x + self.w / 2;
        let cy = self.y + self.h / 2;
        Rect::new(cx.saturating_sub(w / 2), cy.saturating_sub(h / 2), w, h).clamp_into(area)
    }
    /// Where `snap` puts this rect in `area`. Halves and quarters take the odd
    /// cell on the right and bottom, so two opposite snaps tile `area` exactly;
    /// `Center` keeps this rect's size.
    pub fn snapped(&self, area: Rect, to: Snap) -> Rect {
        let (lw, lh) = (area.w / 2, area.h / 2);
        let (rw, rh) = (area.w - lw, area.h - lh);
        let (x0, x1) = (area.x, area.x + lw);
        let (y0, y1) = (area.y, area.y + lh);
        match to {
            Snap::Left => Rect::new(x0, y0, lw, area.h),
            Snap::Right => Rect::new(x1, y0, rw, area.h),
            Snap::Top => Rect::new(x0, y0, area.w, lh),
            Snap::Bottom => Rect::new(x0, y1, area.w, rh),
            Snap::TopLeft => Rect::new(x0, y0, lw, lh),
            Snap::TopRight => Rect::new(x1, y0, rw, lh),
            Snap::BottomLeft => Rect::new(x0, y1, lw, rh),
            Snap::BottomRight => Rect::new(x1, y1, rw, rh),
            Snap::Center => {
                let r = self.clamp_into(area);
                Rect::new(
                    area.x + (area.w - r.w) / 2,
                    area.y + (area.h - r.h) / 2,
                    r.w,
                    r.h,
                )
            }
        }
    }
    /// Move and clip this rect so it lies inside `area`, keeping its size if it fits.
    pub fn clamp_into(&self, area: Rect) -> Rect {
        let w = self.w.min(area.w).max(1);
        let h = self.h.min(area.h).max(1);
        let x = self
            .x
            .clamp(area.x, area.right().saturating_sub(w).max(area.x));
        let y = self
            .y
            .clamp(area.y, area.bottom().saturating_sub(h).max(area.y));
        Rect::new(x, y, w, h)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Node {
    Pane(PaneId),
    Container {
        split: Split,
        /// `Some(active child)` when the container is tabbed.
        tabbed: Option<usize>,
        /// Children with their weights. Weights are relative, not fractions.
        children: Vec<(Node, f32)>,
    },
}

impl Node {
    fn split(split: Split, children: Vec<(Node, f32)>) -> Node {
        Node::Container {
            split,
            tabbed: None,
            children,
        }
    }
    /// The pane a tab is labelled and focused by: its first pane.
    fn first_pane(&self) -> PaneId {
        match self {
            Node::Pane(id) => *id,
            Node::Container { children, .. } => children[0].0.first_pane(),
        }
    }
}

/// How a new pane enters the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Split the focused pane along its longer side.
    Dwindle,
    /// Split the focused pane in this direction.
    Manual(Split),
}

/// tmux's preset layouts: a shape applied once to the tiles a workspace has,
/// not a policy kept as panes open (that is the `layout` setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Preset {
    /// Side by side, equal widths.
    EvenHorizontal,
    /// Stacked, equal heights.
    EvenVertical,
    /// The first pane on top, the rest side by side below it.
    MainHorizontal,
    /// The first pane on the left, the rest stacked on the right.
    MainVertical,
    /// A grid, as square as the count allows.
    Tiled,
}

impl Preset {
    /// In tmux's order, which `next_layout` steps through.
    pub const ALL: [Preset; 5] = [
        Preset::EvenHorizontal,
        Preset::EvenVertical,
        Preset::MainHorizontal,
        Preset::MainVertical,
        Preset::Tiled,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Preset::EvenHorizontal => "even-horizontal",
            Preset::EvenVertical => "even-vertical",
            Preset::MainHorizontal => "main-horizontal",
            Preset::MainVertical => "main-vertical",
            Preset::Tiled => "tiled",
        }
    }

    pub fn from_name(s: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.name() == s)
    }

    /// The one after this in tmux's order, wrapping; the first after none.
    pub fn after(this: Option<Preset>) -> Preset {
        match this {
            None => Preset::ALL[0],
            Some(p) => {
                let i = Preset::ALL.iter().position(|q| *q == p).unwrap_or(0);
                Preset::ALL[(i + 1) % Preset::ALL.len()]
            }
        }
    }

    /// The tree for these panes, in this order. `ratio` is the main pane's
    /// share in the two main layouts.
    pub fn build(self, panes: &[PaneId], ratio: f32) -> Option<Node> {
        let leaves = |ids: &[PaneId]| -> Vec<(Node, f32)> {
            ids.iter().map(|p| (Node::Pane(*p), 1.0)).collect()
        };
        // One pane needs no container; a row or column of one is that pane.
        let line = |split: Split, ids: &[PaneId]| -> Node {
            match ids {
                [one] => Node::Pane(*one),
                many => Node::split(split, leaves(many)),
            }
        };
        match panes {
            [] => return None,
            [one] => return Some(Node::Pane(*one)),
            _ => {}
        }
        Some(match self {
            Preset::EvenHorizontal => line(Split::Horizontal, panes),
            Preset::EvenVertical => line(Split::Vertical, panes),
            Preset::MainVertical | Preset::MainHorizontal => {
                let (outer, inner) = if self == Preset::MainVertical {
                    (Split::Horizontal, Split::Vertical)
                } else {
                    (Split::Vertical, Split::Horizontal)
                };
                Node::split(
                    outer,
                    vec![
                        (Node::Pane(panes[0]), ratio),
                        (line(inner, &panes[1..]), 1.0 - ratio),
                    ],
                )
            }
            Preset::Tiled => {
                // tmux's count: rows grow first, then columns, until they hold
                // every pane. The last row shares its width among what is left.
                let n = panes.len();
                let (mut rows, mut cols) = (1, 1);
                while rows * cols < n {
                    rows += 1;
                    if rows * cols < n {
                        cols += 1;
                    }
                }
                let rows: Vec<(Node, f32)> = panes
                    .chunks(cols)
                    .map(|row| (line(Split::Horizontal, row), 1.0))
                    .collect();
                match rows.len() {
                    1 => rows.into_iter().next().unwrap().0,
                    _ => Node::split(Split::Vertical, rows),
                }
            }
        })
    }
}

/// One tab bar to draw: the row it takes and a pane standing for each tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabBar {
    pub rect: Rect,
    pub tabs: Vec<PaneId>,
    pub active: usize,
}

impl TabBar {
    /// Each tab's columns, `[x0, x1)`. They share the row; a tall (touch-sized)
    /// bar puts a column between them, as toolbar faces have.
    pub fn spans(&self) -> Vec<(u16, u16)> {
        let n = self.tabs.len().max(1) as u16;
        let gap = u16::from(self.rect.h > 1);
        let room = self.rect.w.saturating_sub(gap * (n - 1));
        (0..n)
            .map(|i| {
                let x0 = self.rect.x + i * room / n + gap * i;
                let x1 = self.rect.x + (i + 1) * room / n + gap * i;
                (x0, x1)
            })
            .collect()
    }

    /// The tab a click at column `x` lands on; a gap answers for the tab on
    /// its left.
    pub fn tab_at(&self, x: u16) -> Option<usize> {
        self.spans().iter().rposition(|(x0, _)| x >= *x0)
    }
}

/// A full layout: every pane (hidden ones sized as if shown, so switching tabs
/// does not resize the program) and the tab bars.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub visible: Vec<(PaneId, Rect)>,
    pub hidden: Vec<(PaneId, Rect)>,
    pub tab_bars: Vec<TabBar>,
}

/// The tree of one workspace, possibly empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Tree {
    pub root: Option<Node>,
    /// The master's share of the width the last time the master layout saw
    /// its shape intact: a master that closes is replaced at the same width.
    master_share: Option<f32>,
    /// How many panes the master layout last arranged: a master area forms
    /// when one pane becomes two, and takes `master_ratio` then.
    master_seen: usize,
}

/// A terminal cell is about twice as tall as it is wide; dwindle compares
/// visual length, not cell counts, or every split of a wide pane is vertical.
const CELL_ASPECT: u32 = 2;

impl Tree {
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn contains(&self, id: PaneId) -> bool {
        self.root
            .as_ref()
            .is_some_and(|r| find_path(r, id).is_some())
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
            Some(f) => insert_at(root, f, new, split, false),
            // No usable focus: append at the top level, the least surprising place.
            None => {
                let old = std::mem::replace(root, Node::Pane(new));
                *root = Node::split(split, vec![(old, 1.0), (Node::Pane(new), 1.0)]);
            }
        }
    }

    /// Insert `new` on the `dir` side of `target`: right of it, below it, and so
    /// on. What "open below" and dropping a dragged pane onto another use.
    pub fn insert_beside(&mut self, new: PaneId, target: PaneId, dir: Dir) -> bool {
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        if find_path(root, target).is_none() {
            return false;
        }
        let before = matches!(dir, Dir::Left | Dir::Up);
        insert_at(root, target, new, Split::of(dir), before);
        true
    }

    /// Remove a pane, collapsing split containers left with a single child.
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
        match self.parent_mut(id) {
            Some((Node::Container { split, .. }, _)) => {
                *split = split.flipped();
                true
            }
            _ => false,
        }
    }

    /// Give every child of every container an equal share (tuios's `=`), at every
    /// depth. Returns false when nothing changed.
    pub fn equalize(&mut self) -> bool {
        fn walk(n: &mut Node) -> bool {
            let Node::Container { children, .. } = n else {
                return false;
            };
            let mut changed = false;
            for (c, w) in children.iter_mut() {
                changed |= *w != 1.0;
                *w = 1.0;
                changed |= walk(c);
            }
            changed
        }
        self.root.as_mut().is_some_and(walk)
    }

    /// Put the tree in the master layout's shape: the first pane alone on the
    /// left, the others stacked top to bottom on the right, in tree order. A
    /// tree already in that shape is left as it is, so resizing the master or
    /// the stack sticks; one that is not (a pane closed, a split toggled, a
    /// group made) is rebuilt, keeping the master's share when it had one.
    /// Returns whether anything changed.
    pub fn arrange_master(&mut self, ratio: f32) -> bool {
        let share = match &self.root {
            Some(Node::Container {
                split: Split::Horizontal,
                tabbed: None,
                children,
            }) if children.len() == 2 => {
                let total: f32 = children.iter().map(|(_, w)| *w).sum();
                Some(children[0].1 / total.max(f32::EPSILON))
            }
            _ => None,
        };
        let n = self.panes().len();
        let forming = self.master_seen < 2 && n >= 2;
        self.master_seen = n;
        if self.is_master_shape() && !forming {
            self.master_share = share.or(self.master_share);
            return false;
        }
        let keep = if forming {
            ratio
        } else {
            share.or(self.master_share).unwrap_or(ratio)
        };
        self.master_share = Some(keep);
        let panes = self.panes();
        self.root = match panes.as_slice() {
            [] => None,
            [one] => Some(Node::Pane(*one)),
            [master, rest @ ..] => {
                let stack = match rest {
                    [one] => Node::Pane(*one),
                    many => Node::split(
                        Split::Vertical,
                        many.iter().map(|p| (Node::Pane(*p), 1.0)).collect(),
                    ),
                };
                Some(Node::split(
                    Split::Horizontal,
                    vec![(Node::Pane(*master), keep), (stack, 1.0 - keep)],
                ))
            }
        };
        true
    }

    fn is_master_shape(&self) -> bool {
        let leaf = |n: &Node| matches!(n, Node::Pane(_));
        match &self.root {
            None | Some(Node::Pane(_)) => true,
            Some(Node::Container {
                split: Split::Horizontal,
                tabbed: None,
                children,
            }) if children.len() == 2 && leaf(&children[0].0) => match &children[1].0 {
                Node::Pane(_) => true,
                Node::Container {
                    split: Split::Vertical,
                    tabbed: None,
                    children: stack,
                } => stack.len() >= 2 && stack.iter().all(|(n, _)| leaf(n)),
                _ => false,
            },
            _ => false,
        }
    }

    /// Rebuild the tiles, in tree order, into a preset's shape. Groups are
    /// flattened. Returns whether the tree changed.
    pub fn apply_preset(&mut self, preset: Preset, ratio: f32) -> bool {
        let next = preset.build(&self.panes(), ratio);
        if next == self.root {
            return false;
        }
        self.root = next;
        true
    }

    /// The pane `swap_master` trades places with `id`: the tree's first pane
    /// (the master), or, for the master itself, the next one.
    pub fn master_partner(&self, id: PaneId) -> Option<PaneId> {
        let panes = self.panes();
        match panes.first() {
            Some(m) if *m == id => panes.get(1).copied(),
            Some(m) if panes.contains(&id) => Some(*m),
            _ => None,
        }
    }

    /// Tab or untab the container holding `id` (i3's `layout tabbed`). A pane with
    /// no container around it gets a tabbed container of its own, so the next pane
    /// opened joins it as a tab.
    pub fn toggle_group(&mut self, id: PaneId) -> bool {
        if self.root == Some(Node::Pane(id)) {
            let only = self.root.take().unwrap();
            self.root = Some(Node::Container {
                split: Split::Horizontal,
                tabbed: Some(0),
                children: vec![(only, 1.0)],
            });
            return true;
        }
        match self.parent_mut(id) {
            Some((Node::Container { tabbed, .. }, idx)) => {
                *tabbed = match tabbed {
                    Some(_) => None,
                    None => Some(idx),
                };
                if let Some(root) = self.root.as_mut() {
                    normalize(root);
                }
                true
            }
            _ => false,
        }
    }

    /// Whether `id` sits in a tabbed container (a group).
    pub fn is_grouped(&self, id: PaneId) -> bool {
        let Some(root) = self.root.as_ref() else {
            return false;
        };
        let Some(path) = find_path(root, id) else {
            return false;
        };
        let mut n = root;
        for &i in &path {
            let Node::Container {
                tabbed, children, ..
            } = n
            else {
                break;
            };
            if tabbed.is_some() {
                return true;
            }
            n = &children[i].0;
        }
        false
    }

    /// Cycle the nearest tabbed container around `id`. Returns the pane to focus.
    pub fn cycle_group(&mut self, id: PaneId, forward: bool) -> Option<PaneId> {
        let root = self.root.as_mut()?;
        let path = find_path(root, id)?;
        for depth in (0..path.len()).rev() {
            if let Node::Container {
                tabbed: Some(active),
                children,
                ..
            } = node_at_mut(root, &path[..depth])
            {
                let n = children.len();
                *active = if forward {
                    (*active + 1) % n
                } else {
                    (*active + n - 1) % n
                };
                return Some(children[*active].0.first_pane());
            }
        }
        None
    }

    /// Make every tab on the way to `id` the active one, so `id` is visible.
    pub fn reveal(&mut self, id: PaneId) {
        let Some(root) = self.root.as_mut() else {
            return;
        };
        let Some(path) = find_path(root, id) else {
            return;
        };
        for depth in 0..path.len() {
            if let Node::Container {
                tabbed: Some(active),
                ..
            } = node_at_mut(root, &path[..depth])
            {
                *active = path[depth];
            }
        }
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

    /// Grow (`Right`, `Down`) or shrink (`Left`, `Up`) `id` by `cells`, the way
    /// Hyprland's `resizeactive` does.
    ///
    /// The edge that moves is the one shared with a neighbour: the right/bottom one
    /// when there is a sibling there, else the left/top one. The container adjusted
    /// is the nearest ancestor split along that axis with such a sibling. Returns
    /// false when nothing could move (no neighbour on that axis, or the neighbour
    /// or the pane is already at its minimum).
    pub fn resize(&mut self, id: PaneId, dir: Dir, cells: u16, area: Rect, inner_gap: u16) -> bool {
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        let Some(path) = find_path(root, id) else {
            return false;
        };
        let want = Split::of(dir);
        let grow = matches!(dir, Dir::Right | Dir::Down);
        // Walk up from the pane to find the container to adjust, tracking the rect
        // of each container so the change can be converted from cells to weight.
        let rects = path_rects(root, &path, area, inner_gap);
        for depth in (0..path.len()).rev() {
            let idx = path[depth];
            let container = node_at_mut(root, &path[..depth]);
            let Node::Container {
                split,
                tabbed: None,
                children,
            } = container
            else {
                continue;
            };
            if *split != want || children.len() < 2 {
                continue;
            }
            let nb = if idx + 1 < children.len() {
                idx + 1
            } else {
                idx - 1
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
            // Nothing drops below two cells. Exactly two cells' worth of weight can
            // floor to one once float error and rounding are applied, so the floor
            // is set half a cell above it.
            let min = 2.5 * total / span;
            let (from, to) = if grow { (nb, idx) } else { (idx, nb) };
            let give = delta.min(children[from].1 - min).max(0.0);
            if give <= 0.0 {
                return false;
            }
            children[to].1 += give;
            children[from].1 -= give;
            return true;
        }
        false
    }

    /// Move the edge on the far side of `id` — its right edge for a horizontal
    /// split, its bottom edge for a vertical one — by `delta` cells (negative is
    /// left/up). This is the edge a mouse drags: shared with the neighbour after
    /// `id`, found at whatever level of the tree that neighbour is. Returns false
    /// when there is no such edge or nothing could move.
    pub fn move_edge(
        &mut self,
        id: PaneId,
        split: Split,
        delta: i32,
        area: Rect,
        gap: u16,
    ) -> bool {
        let Some(root) = self.root.as_mut() else {
            return false;
        };
        let Some(path) = find_path(root, id) else {
            return false;
        };
        if delta == 0 {
            return false;
        }
        let rects = path_rects(root, &path, area, gap);
        for depth in (0..path.len()).rev() {
            let idx = path[depth];
            let Node::Container {
                split: s,
                tabbed: None,
                children,
            } = node_at_mut(root, &path[..depth])
            else {
                continue;
            };
            if *s != split || idx + 1 >= children.len() {
                continue;
            }
            let crect = rects[depth];
            let span = match split {
                Split::Horizontal => crect.w,
                Split::Vertical => crect.h,
            } as f32;
            if span <= 0.0 {
                return false;
            }
            let total: f32 = children.iter().map(|(_, w)| *w).sum();
            let min = 2.5 * total / span;
            let want = delta as f32 / span * total;
            // Positive grows `idx` at the expense of the child after it.
            let (from, to, amount) = if want > 0.0 {
                (idx + 1, idx, want)
            } else {
                (idx, idx + 1, -want)
            };
            let give = amount.min(children[from].1 - min).max(0.0);
            if give <= 0.0 {
                return false;
            }
            children[to].1 += give;
            children[from].1 -= give;
            return true;
        }
        false
    }

    /// The visible panes laid out in `area`, `inner_gap` cells between siblings.
    pub fn layout(&self, area: Rect, inner_gap: u16) -> Vec<(PaneId, Rect)> {
        self.layout_full(area, inner_gap).visible
    }

    pub fn layout_full(&self, area: Rect, inner_gap: u16) -> Layout {
        let mut out = Layout::default();
        if let Some(r) = &self.root {
            layout_node(r, area, inner_gap, true, &mut out);
        }
        out
    }

    /// The container directly holding `id`, and `id`'s index in it.
    fn parent_mut(&mut self, id: PaneId) -> Option<(&mut Node, usize)> {
        let root = self.root.as_mut()?;
        let path = find_path(root, id)?;
        let (&idx, parent) = path.split_last()?;
        Some((node_at_mut(root, parent), idx))
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

/// Insert `new` next to `focused` along `split`: after it, or `before` it.
fn insert_at(root: &mut Node, focused: PaneId, new: PaneId, split: Split, before: bool) {
    let path = find_path(root, focused).expect("caller checked");
    if let Some((&idx, parent_path)) = path.split_last()
        && let Node::Container {
            split: psplit,
            tabbed,
            children,
        } = node_at_mut(root, parent_path)
    {
        // Inside a group, a new pane is a new tab next to the current one.
        if let Some(active) = tabbed {
            let at = if before { idx } else { idx + 1 };
            children.insert(at, (Node::Pane(new), 1.0));
            *active = at;
            return;
        }
        // Joining the parent keeps i3's flat containers: three panes side by side
        // are one container of three, not a container nested in a container.
        if *psplit == split {
            // Split the focused pane's share instead of adding a full share, so the
            // rest of the row keeps its size.
            let w = children[idx].1 / 2.0;
            children[idx].1 = w;
            let at = if before { idx } else { idx + 1 };
            children.insert(at, (Node::Pane(new), w));
            return;
        }
    }
    let leaf = node_at_mut(root, &path);
    let old = std::mem::replace(leaf, Node::Pane(new));
    let pair = if before {
        vec![(Node::Pane(new), 1.0), (old, 1.0)]
    } else {
        vec![(old, 1.0), (Node::Pane(new), 1.0)]
    };
    *leaf = Node::split(split, pair);
}

fn remove_in(n: &mut Node, id: PaneId) -> bool {
    let Node::Container {
        children, tabbed, ..
    } = n
    else {
        return false;
    };
    if let Some(i) = children.iter().position(|(c, _)| *c == Node::Pane(id)) {
        children.remove(i);
        if let Some(active) = tabbed {
            // The tab to the left takes over, as in a browser; the first tab's
            // right-hand neighbour when the first one closes.
            if i < *active || (i == *active && *active > 0) {
                *active -= 1;
            }
            *active = (*active).min(children.len().saturating_sub(1));
        }
        return true;
    }
    children.iter_mut().any(|(c, _)| remove_in(c, id))
}

/// Remove empty containers and collapse split containers with one child. A tabbed
/// container with one tab stays, as in i3: it is still a group the next pane joins.
fn normalize(n: &mut Node) {
    if let Node::Container {
        children, tabbed, ..
    } = n
    {
        children.iter_mut().for_each(|(c, _)| normalize(c));
        children
            .retain(|(c, _)| !matches!(c, Node::Container { children, .. } if children.is_empty()));
        if let Some(active) = tabbed {
            *active = (*active).min(children.len().saturating_sub(1));
        }
        if children.len() == 1 && tabbed.is_none() {
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

/// Each child's rect. A tabbed container gives every child the area under its
/// tab bar.
fn child_rects(node: &Node, area: Rect, gap: u16) -> Vec<Rect> {
    let Node::Container {
        split,
        tabbed,
        children,
    } = node
    else {
        return Vec::new();
    };
    if tabbed.is_some() {
        let content = Rect::new(area.x, area.y + 1, area.w, area.h.saturating_sub(1));
        return vec![content; children.len()];
    }
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

fn layout_node(n: &Node, area: Rect, gap: u16, visible: bool, out: &mut Layout) {
    match n {
        Node::Pane(id) => {
            if visible {
                out.visible.push((*id, area));
            } else {
                out.hidden.push((*id, area));
            }
        }
        Node::Container {
            tabbed, children, ..
        } => {
            if let (Some(active), true) = (tabbed, visible) {
                out.tab_bars.push(TabBar {
                    rect: Rect::new(area.x, area.y, area.w, area.h.min(1)),
                    tabs: children.iter().map(|(c, _)| c.first_pane()).collect(),
                    active: *active,
                });
            }
            for (i, ((c, _), r)) in children.iter().zip(child_rects(n, area, gap)).enumerate() {
                let shown = visible && tabbed.is_none_or(|a| a == i);
                layout_node(c, r, gap, shown, out);
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
        let Node::Container { children, .. } = n else {
            break;
        };
        r = child_rects(n, r, gap)[i];
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
    fn resize_grows_right_and_shrinks_left_like_hyprland() {
        let mut t = dwindle(2);
        assert!(t.resize(1, Dir::Right, 10, AREA, 0));
        assert_eq!(rect_of(&t, 1).w, 110);
        assert!(t.resize(1, Dir::Left, 30, AREA, 0));
        assert_eq!(rect_of(&t, 1).w, 80);
        // The right-most pane has no right neighbour: its left edge moves instead.
        assert!(t.resize(2, Dir::Right, 20, AREA, 0));
        assert_eq!(rect_of(&t, 2).w, 140);
        assert_eq!(rect_of(&t, 2).x, 60);
        // Side by side, there is nothing to resize vertically.
        assert!(!t.resize(1, Dir::Up, 5, AREA, 0));
        assert!(!dwindle(1).resize(1, Dir::Right, 5, AREA, 0));
    }

    #[test]
    fn resize_never_crushes_either_side() {
        let mut t = dwindle(2);
        assert!(t.resize(1, Dir::Right, 500, AREA, 0));
        assert!(rect_of(&t, 2).w >= 2);
        assert!(t.resize(1, Dir::Left, 500, AREA, 0));
        assert!(rect_of(&t, 1).w >= 2);
    }

    #[test]
    fn move_edge_moves_the_shared_edge_at_any_depth() {
        // [1 | [2 / 3]]: dragging 1's right edge resizes against the container.
        let mut t = Tree::default();
        t.insert(1, None, None, Placement::Manual(Split::Horizontal));
        t.insert(2, Some(1), None, Placement::Manual(Split::Horizontal));
        t.insert(3, Some(2), None, Placement::Manual(Split::Vertical));
        assert!(t.move_edge(1, Split::Horizontal, 10, AREA, 0));
        assert_eq!(rect_of(&t, 1).w, 110);
        assert_eq!(rect_of(&t, 2).w, 90);
        assert!(t.move_edge(1, Split::Horizontal, -30, AREA, 0));
        assert_eq!(rect_of(&t, 1).w, 80);
        // 2's bottom edge is shared with 3.
        assert!(t.move_edge(2, Split::Vertical, 5, AREA, 0));
        assert_eq!(rect_of(&t, 2).h, 30);
        // 3 and 2 are on the right edge of the screen: nothing after them.
        assert!(!t.move_edge(3, Split::Horizontal, 5, AREA, 0));
        assert!(!t.move_edge(3, Split::Vertical, 5, AREA, 0));
        assert!(!t.move_edge(1, Split::Horizontal, 0, AREA, 0));
    }

    #[test]
    fn move_edge_keeps_both_sides_at_least_two_cells() {
        let mut t = dwindle(2);
        assert!(t.move_edge(1, Split::Horizontal, 500, AREA, 0));
        assert!(rect_of(&t, 2).w >= 2);
        assert!(t.move_edge(1, Split::Horizontal, -500, AREA, 0));
        assert!(rect_of(&t, 1).w >= 2);
    }

    #[test]
    fn insert_beside_puts_the_pane_on_that_side() {
        // 1 | 2, then 3 below 1 and 4 above 2.
        let mut t = dwindle(2);
        assert!(t.insert_beside(3, 1, Dir::Down));
        assert!(t.insert_beside(4, 2, Dir::Up));
        let (r1, r3) = (rect_of(&t, 1), rect_of(&t, 3));
        assert_eq!((r1.x, r3.x), (0, 0));
        assert!(r3.y > r1.y, "3 below 1");
        let (r2, r4) = (rect_of(&t, 2), rect_of(&t, 4));
        assert!(r4.y < r2.y && r4.x == r2.x, "4 above 2");
        // Left of a pane in a horizontal row joins that row before it.
        assert!(t.insert_beside(5, 1, Dir::Left));
        assert_eq!(rect_of(&t, 5).x, 0);
        assert!(rect_of(&t, 1).x > 0);
        assert!(!t.insert_beside(6, 99, Dir::Left));
    }

    #[test]
    fn equalize_evens_out_every_level() {
        // [1 | [2 / 3]], then resized unevenly on both axes.
        let mut t = Tree::default();
        t.insert(1, None, None, Placement::Manual(Split::Horizontal));
        t.insert(2, Some(1), None, Placement::Manual(Split::Horizontal));
        t.insert(3, Some(2), None, Placement::Manual(Split::Vertical));
        assert!(t.resize(1, Dir::Right, 30, AREA, 0));
        assert!(t.resize(2, Dir::Down, 10, AREA, 0));
        assert!(t.equalize());
        assert_eq!(rect_of(&t, 1).w, 100);
        assert_eq!(rect_of(&t, 2).h, 25);
        assert_eq!(rect_of(&t, 3).h, 25);
        // Already even: nothing to do, so no relayout.
        assert!(!t.equalize());
        assert!(!dwindle(1).equalize());
    }

    #[test]
    fn master_layout_keeps_one_master_and_a_stack() {
        let mut t = Tree::default();
        for id in 1..=4 {
            let f = (id > 1).then_some(id - 1);
            t.insert(id, f, None, Placement::Dwindle);
            t.arrange_master(0.75);
        }
        // 1 on the left at 75%, 2-4 stacked on the right.
        assert_eq!(rect_of(&t, 1), Rect::new(0, 0, 150, 50));
        for id in 2..=4 {
            assert_eq!(rect_of(&t, id).x, 150, "{id}");
        }
        assert!(rect_of(&t, 2).y < rect_of(&t, 3).y && rect_of(&t, 3).y < rect_of(&t, 4).y);
        // Resizing the master sticks: the shape holds, nothing is rebuilt.
        assert!(t.resize(1, Dir::Right, 10, AREA, 0));
        assert!(!t.arrange_master(0.75));
        assert_eq!(rect_of(&t, 1).w, 160);
        // The master closes: the first of the stack takes its place, same width.
        t.remove(1);
        assert!(t.arrange_master(0.75));
        // (Within a cell: the share goes through f32 weights and rounding.)
        let r2 = rect_of(&t, 2);
        assert_eq!((r2.x, r2.y, r2.h), (0, 0, 50));
        assert!(r2.w.abs_diff(160) <= 1, "{r2:?}");
        assert_eq!(rect_of(&t, 3).x, r2.w);
        // Down to one pane, and then none.
        t.remove(3);
        t.remove(4);
        t.arrange_master(0.75);
        assert_eq!(t.layout(AREA, 0), vec![(2, AREA)]);
        t.remove(2);
        assert!(!t.arrange_master(0.75));
    }

    #[test]
    fn swap_master_partners() {
        let mut t = dwindle(3);
        t.arrange_master(0.5);
        assert_eq!(t.master_partner(3), Some(1));
        assert_eq!(t.master_partner(1), Some(2));
        assert_eq!(dwindle(1).master_partner(1), None);
        assert_eq!(t.master_partner(9), None);
    }

    #[test]
    fn inset_saturates() {
        assert_eq!(Rect::new(0, 0, 3, 3).inset(5, 5).w, 0);
        assert_eq!(Rect::new(2, 2, 10, 6).inset(1, 1), Rect::new(3, 3, 8, 4));
        // Each side its own: more on top than at the bottom.
        assert_eq!(
            Rect::new(0, 0, 20, 10).inset_sides([3, 2, 1, 0]),
            Rect::new(0, 3, 18, 6)
        );
        assert_eq!(Rect::new(0, 0, 4, 4).inset_sides([9, 0, 9, 0]).h, 0);
    }

    #[test]
    fn grouping_shows_one_tab_under_a_bar() {
        let mut t = dwindle(2);
        assert!(t.toggle_group(2));
        let l = t.layout_full(AREA, 0);
        assert_eq!(l.visible, vec![(2, Rect::new(0, 1, 200, 49))]);
        assert_eq!(l.hidden, vec![(1, Rect::new(0, 1, 200, 49))]);
        assert_eq!(
            l.tab_bars,
            vec![TabBar {
                rect: Rect::new(0, 0, 200, 1),
                tabs: vec![1, 2],
                active: 1
            }]
        );
        // Untabbing restores the split exactly.
        assert!(t.toggle_group(2));
        assert_eq!(rect_of(&t, 1), Rect::new(0, 0, 100, 50));
        assert!(t.layout_full(AREA, 0).tab_bars.is_empty());
    }

    #[test]
    fn a_new_pane_in_a_group_is_a_new_active_tab() {
        let mut t = dwindle(1);
        assert!(t.toggle_group(1));
        t.insert(2, Some(1), Some(AREA), Placement::Dwindle);
        let l = t.layout_full(AREA, 0);
        assert_eq!(l.tab_bars[0].tabs, vec![1, 2]);
        assert_eq!(l.tab_bars[0].active, 1);
        assert_eq!(l.visible.len(), 1);
        assert_eq!(l.visible[0].0, 2);
    }

    #[test]
    fn cycling_and_revealing_tabs() {
        let mut t = dwindle(1);
        t.toggle_group(1);
        t.insert(2, Some(1), None, Placement::Dwindle);
        t.insert(3, Some(2), None, Placement::Dwindle);
        assert_eq!(t.cycle_group(3, true), Some(1));
        assert_eq!(t.cycle_group(1, false), Some(3));
        t.reveal(2);
        assert_eq!(t.layout(AREA, 0)[0].0, 2);
        // No group around a lone pane in a split: nothing to cycle.
        assert_eq!(dwindle(2).cycle_group(1, true), None);
    }

    #[test]
    fn closing_a_tab_activates_its_left_neighbour_and_keeps_the_group() {
        let mut t = dwindle(1);
        t.toggle_group(1);
        t.insert(2, Some(1), None, Placement::Dwindle);
        t.insert(3, Some(2), None, Placement::Dwindle);
        t.reveal(2);
        t.remove(2);
        assert_eq!(t.layout(AREA, 0)[0].0, 1);
        t.remove(3);
        // One tab left: still a group, still a bar.
        assert_eq!(t.layout_full(AREA, 0).tab_bars.len(), 1);
    }

    #[test]
    fn rect_helpers() {
        let a = Rect::new(0, 0, 100, 40);
        assert_eq!(a.centered(50, 50), Rect::new(25, 10, 50, 20));
        assert_eq!(
            Rect::new(90, 35, 20, 10).clamp_into(a),
            Rect::new(80, 30, 20, 10)
        );
        assert!(a.contains(99, 39) && !a.contains(100, 0));
    }

    #[test]
    fn snapping_and_sizing_floats() {
        let area = Rect::new(0, 1, 101, 41);
        let r = Rect::new(10, 10, 20, 10);
        // Opposite halves tile the area exactly, odd cells included.
        let (l, rt) = (r.snapped(area, Snap::Left), r.snapped(area, Snap::Right));
        assert_eq!(l, Rect::new(0, 1, 50, 41));
        assert_eq!(rt, Rect::new(50, 1, 51, 41));
        assert_eq!(
            r.snapped(area, Snap::BottomRight),
            Rect::new(50, 21, 51, 21)
        );
        assert_eq!(r.snapped(area, Snap::Top), Rect::new(0, 1, 101, 20));
        assert_eq!(r.snapped(area, Snap::Center), Rect::new(40, 16, 20, 10));
        // Resizing keeps the centre, and stays inside the area near an edge.
        assert_eq!(r.resized_in(area, 20, 50), Rect::new(10, 5, 20, 20));
        let edge = Rect::new(90, 30, 10, 10).resized_in(area, 50, 50);
        assert!(edge.right() <= area.right() && edge.bottom() <= area.bottom());
        assert_eq!((edge.w, edge.h), (50, 20));
    }

    /// The panes of each row, top to bottom, by where they are drawn.
    fn rows_of(t: &Tree) -> Vec<Vec<PaneId>> {
        let mut placed = t.layout(AREA, 0);
        placed.sort_by_key(|(_, r)| (r.y, r.x));
        let mut rows: Vec<(u16, Vec<PaneId>)> = Vec::new();
        for (id, r) in placed {
            match rows.iter_mut().find(|(y, _)| *y == r.y) {
                Some((_, row)) => row.push(id),
                None => rows.push((r.y, vec![id])),
            }
        }
        rows.into_iter().map(|(_, r)| r).collect()
    }

    #[test]
    fn presets_by_name_and_in_tmux_order() {
        for p in Preset::ALL {
            assert_eq!(Preset::from_name(p.name()), Some(p));
        }
        assert_eq!(Preset::from_name("main"), None);
        assert_eq!(Preset::after(None), Preset::EvenHorizontal);
        assert_eq!(
            Preset::after(Some(Preset::EvenHorizontal)),
            Preset::EvenVertical
        );
        assert_eq!(Preset::after(Some(Preset::Tiled)), Preset::EvenHorizontal);
    }

    #[test]
    fn even_presets_line_every_pane_up_equally() {
        let mut t = dwindle(4);
        assert!(t.apply_preset(Preset::EvenHorizontal, 0.5));
        assert_eq!(rows_of(&t), vec![vec![1, 2, 3, 4]]);
        assert!(t.layout(AREA, 0).iter().all(|(_, r)| r.w == 50));
        assert!(t.apply_preset(Preset::EvenVertical, 0.5));
        assert_eq!(rows_of(&t), vec![vec![1], vec![2], vec![3], vec![4]]);
        // Applied twice, nothing changes the second time.
        assert!(!t.apply_preset(Preset::EvenVertical, 0.5));
    }

    #[test]
    fn main_presets_give_the_first_pane_the_ratio() {
        let mut t = dwindle(3);
        t.apply_preset(Preset::MainVertical, 0.75);
        assert_eq!(rect_of(&t, 1), Rect::new(0, 0, 150, 50));
        assert_eq!(rect_of(&t, 2), Rect::new(150, 0, 50, 25));
        assert_eq!(rect_of(&t, 3), Rect::new(150, 25, 50, 25));
        t.apply_preset(Preset::MainHorizontal, 0.75);
        assert_eq!(rect_of(&t, 1).h, 38);
        assert_eq!(rows_of(&t), vec![vec![1], vec![2, 3]]);
    }

    #[test]
    fn tiled_counts_rows_first_as_tmux_does() {
        let grid = |n: PaneId| {
            let mut t = dwindle(n);
            t.apply_preset(Preset::Tiled, 0.5);
            rows_of(&t)
        };
        assert_eq!(grid(1), vec![vec![1]]);
        assert_eq!(grid(2), vec![vec![1], vec![2]]);
        assert_eq!(grid(3), vec![vec![1, 2], vec![3]]);
        assert_eq!(grid(4), vec![vec![1, 2], vec![3, 4]]);
        assert_eq!(grid(5), vec![vec![1, 2], vec![3, 4], vec![5]]);
        assert_eq!(grid(7), vec![vec![1, 2, 3], vec![4, 5, 6], vec![7]]);
        // A short last row stretches across the width.
        let mut t = dwindle(3);
        t.apply_preset(Preset::Tiled, 0.5);
        assert_eq!(rect_of(&t, 3).w, 200);
    }

    #[test]
    fn presets_flatten_groups() {
        let mut t = dwindle(3);
        t.toggle_group(3);
        assert!(t.is_grouped(3));
        t.apply_preset(Preset::EvenHorizontal, 0.5);
        assert!(!t.is_grouped(3));
        assert_eq!(rows_of(&t), vec![vec![1, 2, 3]]);
    }
}
