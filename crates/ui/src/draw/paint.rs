// SPDX-License-Identifier: GPL-3.0-or-later

//! How a shape is painted: a theme token with an optional extra opacity (ADR-0039 D3), and the
//! stroke and fill styles.

use crate::ColorToken;

/// The paint of a shape: a colour **token** of the active theme, never a literal colour
/// (ADR-0014 §2, ADR-0039 D3).
///
/// The backend resolves the token with the theme it draws with, so the same list draws in every
/// theme and a theme change needs no re-recording. `opacity` multiplies the token's own alpha
/// (255 = as the theme says, 0 = invisible), for fades and translucent overlays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Paint {
    /// The theme's colour role.
    pub token: ColorToken,
    /// Extra opacity, multiplied with the token's alpha (straight, 0–255).
    pub opacity: u8,
}

impl Paint {
    /// The token as the theme defines it (full opacity).
    #[must_use]
    pub const fn token(token: ColorToken) -> Self {
        Self {
            token,
            opacity: u8::MAX,
        }
    }

    /// The token with an extra opacity.
    #[must_use]
    pub const fn with_opacity(token: ColorToken, opacity: u8) -> Self {
        Self { token, opacity }
    }
}

impl From<ColorToken> for Paint {
    /// A token at full opacity, so drawing calls can take `ColorToken::Accent` directly.
    fn from(token: ColorToken) -> Self {
        Self::token(token)
    }
}

/// Which parts of a self-intersecting or nested path are inside (the two standard rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FillRule {
    /// Non-zero winding: inside where the path winds around the point a non-zero number of times.
    /// The default, and what font outlines (Bravura) are designed for.
    #[default]
    NonZero,
    /// Even-odd: inside where a ray from the point crosses the path an odd number of times (holes
    /// regardless of direction).
    EvenOdd,
}

/// The shape at the open ends of a stroked line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineCap {
    /// The stroke ends exactly at the end point.
    #[default]
    Butt,
    /// A half circle beyond the end point.
    Round,
    /// A half square beyond the end point.
    Square,
}

/// The shape where two segments of a stroked path meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineJoin {
    /// A sharp corner, cut to a bevel beyond [`StrokeStyle::MITER_LIMIT`].
    #[default]
    Miter,
    /// A rounded corner.
    Round,
    /// A corner cut straight.
    Bevel,
}

/// How a path is stroked.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    /// Line width in logical pixels. A width of 0 or less, or not finite, draws nothing.
    pub width: f32,
    /// The ends of open sub-paths.
    pub cap: LineCap,
    /// The corners.
    pub join: LineJoin,
}

impl StrokeStyle {
    /// The miter limit (ratio of the miter length to the line width), the SVG default. Fixed, so
    /// the bounds of a stroke can be computed without storing it per command.
    pub const MITER_LIMIT: f32 = 4.0;

    /// A stroke of `width` with butt caps and miter joins.
    #[must_use]
    pub const fn new(width: f32) -> Self {
        Self {
            width,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
        }
    }

    /// How far the stroke can reach beyond its path, in logical pixels: what the bounds of a
    /// stroked shape must be grown by.
    ///
    /// Half the width, except for miter joins (up to half the miter length) and square caps (the
    /// corner of the half square, √2 · width / 2). Conservative: bounds may be a little larger
    /// than the ink, never smaller.
    #[must_use]
    pub fn reach(&self) -> f32 {
        let half = self.width.max(0.0) * 0.5;
        let join = match self.join {
            LineJoin::Miter => half * Self::MITER_LIMIT,
            LineJoin::Round | LineJoin::Bevel => half,
        };
        let cap = match self.cap {
            LineCap::Square => half * std::f32::consts::SQRT_2,
            LineCap::Butt | LineCap::Round => half,
        };
        join.max(cap)
    }
}
