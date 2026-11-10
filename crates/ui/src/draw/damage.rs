// SPDX-License-Identifier: GPL-3.0-or-later

//! The damage of a frame: which parts of the surface must be redrawn (ADR-0039 D5).

use super::geometry::Rect;

/// The most rectangles a [`Damage`] keeps before merging (ADR-0039 D5).
///
/// # Why eight
/// Typical edits damage one or two places (a note and its old position, a button and a tooltip).
/// Eight leaves room for a few unrelated changes in one frame while keeping every operation on
/// the list a fixed, tiny amount of work (merging scans at most 9 × 9 pairs).
pub(crate) const MAX_RECTS: usize = 8;

/// The parts of the surface to redraw, in logical pixels: up to [`MAX_RECTS`] rectangles, or the
/// whole surface.
///
/// Fixed capacity (an array, not a vector): adding damage never allocates, so a frame can be
/// prepared on any thread without touching the heap. The backend rounds each rectangle outwards
/// to device pixels and adds the anti-aliasing fringe when it draws (ADR-0039 D5); it also
/// decides that more than half the frame is "everything", because only it knows the frame's size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Damage {
    /// The rectangles; only the first `len` are meaningful, each non-empty.
    rects: [Rect; MAX_RECTS],
    /// How many rectangles are in use.
    len: usize,
    /// `true` when the whole surface is damaged; then `rects` is ignored.
    all: bool,
}

impl Damage {
    /// No damage: nothing to redraw.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            rects: [Rect::EMPTY; MAX_RECTS],
            len: 0,
            all: false,
        }
    }

    /// The whole surface: the first frame, a resize, a theme or scale change.
    #[must_use]
    pub const fn all() -> Self {
        Self {
            rects: [Rect::EMPTY; MAX_RECTS],
            len: 0,
            all: true,
        }
    }

    /// `true` when nothing needs redrawing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        !self.all && self.len == 0
    }

    /// `true` when the whole surface needs redrawing.
    #[must_use]
    pub const fn is_all(&self) -> bool {
        self.all
    }

    /// The damaged rectangles (empty when [`is_all`](Self::is_all) or [`is_empty`](Self::is_empty)).
    #[must_use]
    pub fn rects(&self) -> &[Rect] {
        if self.all {
            &[]
        } else {
            self.rects.get(..self.len).unwrap_or(&[])
        }
    }

    /// Marks the whole surface as damaged.
    pub fn add_all(&mut self) {
        self.all = true;
        self.len = 0;
    }

    /// Adds everything `other` damages.
    pub fn add_damage(&mut self, other: &Self) {
        if other.all {
            self.add_all();
        } else {
            for rect in other.rects() {
                self.add(*rect);
            }
        }
    }

    /// Adds a damaged rectangle, merging as ADR-0039 D5 says.
    ///
    /// 1. An empty rectangle, or any rectangle when everything is already damaged, changes
    ///    nothing.
    /// 2. A rectangle that overlaps or touches one already there is merged with it; the union may
    ///    now touch others, so merging repeats until it touches none (at most [`MAX_RECTS`]
    ///    rounds).
    /// 3. If all [`MAX_RECTS`] places are taken, the two rectangles (among the stored ones and
    ///    the new one) whose union adds the least area are merged, which frees a place.
    ///
    /// # Complexity
    /// O(`MAX_RECTS`²) in the worst case, a fixed bound: no allocation, no dependence on the size of
    /// the list or the frame.
    pub fn add(&mut self, rect: Rect) {
        if self.all || rect.is_empty() {
            return;
        }
        let mut rect = rect;
        // Step 2: absorb every stored rectangle the new one touches. Removing swaps the last one
        // into the hole, so the scan restarts at the same index.
        let mut index = 0;
        while index < self.len {
            let stored = self.rects.get(index).copied().unwrap_or(Rect::EMPTY);
            if touches(&stored, &rect) {
                rect = stored.union(&rect);
                self.swap_remove(index);
                // The union is larger: it may touch rectangles already scanned.
                index = 0;
            } else {
                index += 1;
            }
        }
        if self.len == MAX_RECTS {
            // Step 3: make room by merging the cheapest pair, the new rectangle included. Either
            // way one place is freed (`len` drops to MAX_RECTS - 1), so the insertion below fits.
            rect = self.merge_cheapest(rect);
        }
        if let Some(slot) = self.rects.get_mut(self.len) {
            *slot = rect;
            self.len += 1;
        }
    }

    /// Removes the rectangle at `index`, moving the last one into its place (O(1)).
    fn swap_remove(&mut self, index: usize) {
        if index >= self.len {
            return;
        }
        let last = self.len - 1;
        self.rects.swap(index, last);
        self.len = last;
    }

    /// With all places taken, finds the pair whose union adds the least area, among the stored
    /// rectangles and `new`, and merges it. Returns the rectangle still to be inserted: the union
    /// when `new` was part of the pair, otherwise `new` itself (and a stored pair was merged,
    /// freeing a place).
    fn merge_cheapest(&mut self, new: Rect) -> Rect {
        // Candidates: indices 0..len are stored rectangles, index `len` is `new`.
        let candidate = |damage: &Self, i: usize| -> Rect {
            if i == damage.len {
                new
            } else {
                damage.rects.get(i).copied().unwrap_or(Rect::EMPTY)
            }
        };
        let count = self.len + 1;
        let mut best = (0, 1, f32::INFINITY);
        for i in 0..count {
            for j in (i + 1)..count {
                let a = candidate(self, i);
                let b = candidate(self, j);
                let added = a.union(&b).area() - a.area() - b.area();
                if added < best.2 {
                    best = (i, j, added);
                }
            }
        }
        let (i, j, _) = best;
        let merged = candidate(self, i).union(&candidate(self, j));
        if j == self.len {
            // `new` is in the pair: drop the stored partner, insert the union.
            self.swap_remove(i);
            merged
        } else {
            // Two stored rectangles: the union replaces `i`, `j` is removed, `new` waits.
            if let Some(slot) = self.rects.get_mut(i) {
                *slot = merged;
            }
            self.swap_remove(j);
            new
        }
    }
}

impl Default for Damage {
    /// No damage.
    fn default() -> Self {
        Self::none()
    }
}

/// `true` when the rectangles overlap or share an edge: merging them then costs no extra area
/// beyond what they enclose together, and drawing one region is cheaper than two.
fn touches(a: &Rect, b: &Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}
