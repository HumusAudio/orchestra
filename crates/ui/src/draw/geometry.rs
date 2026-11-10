// SPDX-License-Identifier: GPL-3.0-or-later

//! Plain geometry for the display list (ADR-0039 D2): points, rectangles, rounded rectangles and
//! the one kind of transform the interface needs (translate and uniform scale).
//!
//! # Units
//! Everything here is in **logical pixels**, `f32`, y down, origin at the top left of the
//! surface. The backend multiplies by the window's scale factor at draw time, so a list is valid
//! at any DPI. `f32` (not rationals): this is screen geometry, rounded to pixels at the end; the
//! exact musical time of ADR-0005 never reaches this layer.
//!
//! # Why our own types and not `kurbo`
//! `kurbo` is vello's geometry crate and would be the natural shared type with F2.06, but it is
//! one more dependency for a handful of plain structs; converting to it later is trivial
//! (ADR-0039, alternatives).

/// A point in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// Horizontal coordinate, growing to the right.
    pub x: f32,
    /// Vertical coordinate, growing downwards.
    pub y: f32,
}

impl Point {
    /// The point `(x, y)`.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle in logical pixels, stored by its edges.
///
/// # Invariant (not enforced)
/// A *valid* rectangle has `x0 <= x1` and `y0 <= y1` and finite edges. Constructors do not
/// reorder edges: a caller that builds an inverted or NaN rectangle gets an **empty** one
/// ([`Rect::is_empty`]), which draws nothing and damages nothing. That is the right answer for
/// layout code that computes a negative width at an extreme zoom: nothing to see, no panic.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub x0: f32,
    /// Top edge.
    pub y0: f32,
    /// Right edge.
    pub x1: f32,
    /// Bottom edge.
    pub y1: f32,
}

impl Rect {
    /// The empty rectangle at the origin: the neutral element of [`Rect::union`].
    pub const EMPTY: Self = Self {
        x0: 0.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    };

    /// The rectangle with edges `x0`, `y0` (top left) and `x1`, `y1` (bottom right).
    #[must_use]
    pub const fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// The rectangle at `(x, y)` with the given width and height.
    #[must_use]
    pub fn from_origin_size(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self::new(x, y, x + width, y + height)
    }

    /// Width (`x1 - x0`); negative or NaN for an invalid rectangle.
    #[must_use]
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }

    /// Height (`y1 - y0`); negative or NaN for an invalid rectangle.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }

    /// `true` when the rectangle covers no area: zero or negative width or height, or a NaN or
    /// infinite edge.
    ///
    /// Written so that a NaN edge counts as empty instead of slipping through: every comparison
    /// with NaN is false.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        // `<` is false when either side is NaN, so a NaN edge makes `ordered` false.
        let ordered = self.x0 < self.x1 && self.y0 < self.y1;
        let finite = self.x0.is_finite()
            && self.y0.is_finite()
            && self.x1.is_finite()
            && self.y1.is_finite();
        !(ordered && finite)
    }

    /// The smallest rectangle containing both. An empty operand is ignored, so folding a list of
    /// bounds starting from [`Rect::EMPTY`] gives their hull.
    #[must_use]
    pub fn union(&self, other: &Self) -> Self {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        Self::new(
            self.x0.min(other.x0),
            self.y0.min(other.y0),
            self.x1.max(other.x1),
            self.y1.max(other.y1),
        )
    }

    /// The common part of both, possibly empty.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        Self::new(
            self.x0.max(other.x0),
            self.y0.max(other.y0),
            self.x1.min(other.x1),
            self.y1.min(other.y1),
        )
    }

    /// `true` when the two rectangles share some area. Touching edges do not count: a command
    /// whose bounds only touch a damaged region does not need to be replayed for it.
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        !self.intersect(other).is_empty()
    }

    /// The rectangle grown by `amount` on every side (shrunk if negative).
    #[must_use]
    pub fn inflate(&self, amount: f32) -> Self {
        Self::new(
            self.x0 - amount,
            self.y0 - amount,
            self.x1 + amount,
            self.y1 + amount,
        )
    }

    /// Area in square logical pixels; 0 for an empty rectangle.
    #[must_use]
    pub fn area(&self) -> f32 {
        if self.is_empty() {
            0.0
        } else {
            self.width() * self.height()
        }
    }
}

/// A rectangle with all four corners rounded by the same radius, in logical pixels.
///
/// The radius is clamped at draw time to half the shorter side, so a pill shape can be asked for
/// with a large radius; a negative radius counts as 0.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RoundedRect {
    /// The outer rectangle.
    pub rect: Rect,
    /// The corner radius.
    pub radius: f32,
}

impl RoundedRect {
    /// `rect` with corners of `radius`.
    #[must_use]
    pub const fn new(rect: Rect, radius: f32) -> Self {
        Self { rect, radius }
    }

    /// The radius actually drawn: between 0 and half the shorter side.
    #[must_use]
    pub fn clamped_radius(&self) -> f32 {
        let limit = (self.rect.width().min(self.rect.height()) * 0.5).max(0.0);
        self.radius.clamp(0.0, limit)
    }
}

/// The transform of the display list: a uniform scale followed by a translation
/// (`p' = p * scale + offset`), ADR-0039 D2.
///
/// # Why only these two
/// Scrolling is a translation and zooming a score is a uniform scale; the interface never
/// rotates or skews. Keeping the transform this small means bounds stay axis-aligned rectangles
/// after transformation (exact, not a conservative hull), and the backend's pixel snapping (D7)
/// stays valid under every transform the list can hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// Multiplies both coordinates. Must be positive and finite to be meaningful; the recorder
    /// ignores a push with any other value.
    pub scale: f32,
    /// Added after the scale.
    pub offset: Point,
}

impl Transform {
    /// The identity: scale 1, no offset.
    pub const IDENTITY: Self = Self {
        scale: 1.0,
        offset: Point::new(0.0, 0.0),
    };

    /// A pure translation.
    #[must_use]
    pub const fn translate(dx: f32, dy: f32) -> Self {
        Self {
            scale: 1.0,
            offset: Point::new(dx, dy),
        }
    }

    /// A pure uniform scale around the origin.
    #[must_use]
    pub const fn scale(scale: f32) -> Self {
        Self {
            scale,
            offset: Point::new(0.0, 0.0),
        }
    }

    /// `true` when the scale is positive and finite and the offset finite.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.scale > 0.0
            && self.scale.is_finite()
            && self.offset.x.is_finite()
            && self.offset.y.is_finite()
    }

    /// `self` applied after `inner`: a point goes through `inner` first, then `self`. This is how
    /// a pushed transform combines with the one already in effect.
    #[must_use]
    pub fn then(&self, inner: &Self) -> Self {
        // p' = (p * si + oi) * so + oo = p * (si * so) + (oi * so + oo)
        Self {
            scale: inner.scale * self.scale,
            offset: Point::new(
                inner.offset.x * self.scale + self.offset.x,
                inner.offset.y * self.scale + self.offset.y,
            ),
        }
    }

    /// The point transformed.
    #[must_use]
    pub fn apply(&self, point: Point) -> Point {
        Point::new(
            point.x * self.scale + self.offset.x,
            point.y * self.scale + self.offset.y,
        )
    }

    /// The rectangle transformed. Exact, because the scale is uniform and positive (no rotation).
    #[must_use]
    pub fn apply_rect(&self, rect: &Rect) -> Rect {
        let p0 = self.apply(Point::new(rect.x0, rect.y0));
        let p1 = self.apply(Point::new(rect.x1, rect.y1));
        Rect::new(p0.x, p0.y, p1.x, p1.y)
    }
}

impl Default for Transform {
    /// The identity.
    fn default() -> Self {
        Self::IDENTITY
    }
}
