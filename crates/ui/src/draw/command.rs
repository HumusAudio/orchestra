// SPDX-License-Identifier: GPL-3.0-or-later

//! The recorded commands (ADR-0039 D4): internal to the crate, so a new kind (glyph runs in
//! F2.07) breaks no public caller.
//!
//! Callers never build these directly: they call the methods of
//! [`Recorder`](crate::Recorder), which also computes each command's bounds.

use std::ops::Range;

use super::geometry::{Point, Rect, RoundedRect, Transform};
use super::image::ImageId;
use super::paint::{FillRule, Paint, StrokeStyle};

/// One element of a path, stored in the segment's path arena (ADR-0039 D1).
///
/// The arena is one `Vec<PathElement>` per segment; a path command refers to a range of it. One
/// vector for all the paths of a segment means recording a path never allocates on its own, and
/// re-recording reuses the capacity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PathElement {
    /// Starts a new sub-path at the point.
    MoveTo(Point),
    /// A straight line to the point.
    LineTo(Point),
    /// A quadratic Bézier: control point, end point.
    QuadTo(Point, Point),
    /// A cubic Bézier: two control points, end point.
    CubicTo(Point, Point, Point),
    /// Closes the current sub-path with a straight line to its start.
    Close,
}

/// A drawing command of a segment.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Command {
    /// A filled axis-aligned rectangle (snapped to device pixels, ADR-0039 D7).
    FillRect {
        /// The rectangle.
        rect: Rect,
        /// Its paint.
        paint: Paint,
    },
    /// A filled rounded rectangle (anti-aliased, not snapped).
    FillRoundedRect {
        /// The shape.
        shape: RoundedRect,
        /// Its paint.
        paint: Paint,
    },
    /// The outline of a rectangle, centred on its edges (snapped, ADR-0039 D7).
    StrokeRect {
        /// The rectangle.
        rect: Rect,
        /// The line.
        stroke: StrokeStyle,
        /// Its paint.
        paint: Paint,
    },
    /// The outline of a rounded rectangle (anti-aliased).
    StrokeRoundedRect {
        /// The shape.
        shape: RoundedRect,
        /// The line.
        stroke: StrokeStyle,
        /// Its paint.
        paint: Paint,
    },
    /// A straight line; snapped when horizontal or vertical with butt or square caps.
    Line {
        /// Start.
        from: Point,
        /// End.
        to: Point,
        /// The line.
        stroke: StrokeStyle,
        /// Its paint.
        paint: Paint,
    },
    /// A filled path.
    FillPath {
        /// Its elements in the segment's path arena.
        elements: Range<usize>,
        /// Which parts are inside.
        rule: FillRule,
        /// Its paint.
        paint: Paint,
    },
    /// A stroked path.
    StrokePath {
        /// Its elements in the segment's path arena.
        elements: Range<usize>,
        /// The line.
        stroke: StrokeStyle,
        /// Its paint.
        paint: Paint,
    },
    /// A blurred rounded rectangle: a shadow (ADR-0039 D6).
    Shadow {
        /// The shape before blurring.
        shape: RoundedRect,
        /// The Gaussian's standard deviation, logical pixels; 0 is a sharp fill.
        blur: f32,
        /// Its paint (normally the `shadow` token).
        paint: Paint,
    },
    /// A picture of the list's image store, stretched into a rectangle.
    Image {
        /// Which picture.
        image: ImageId,
        /// Where it goes.
        dest: Rect,
    },
    /// Starts clipping to the shape (intersected with any clip already in effect).
    PushClip {
        /// The clip, in the coordinates of the transform in effect.
        shape: RoundedRect,
    },
    /// Ends the innermost clip.
    PopClip,
    /// Starts a transform, applied after the one in effect.
    PushTransform(Transform),
    /// Ends the innermost transform.
    PopTransform,
}

impl Command {
    /// `true` for the commands that change the drawing state instead of drawing: they must be
    /// replayed even when their (empty) bounds do not meet the damage, or the state of the
    /// commands after them would be wrong.
    pub(crate) const fn is_state(&self) -> bool {
        matches!(
            self,
            Self::PushClip { .. } | Self::PopClip | Self::PushTransform(_) | Self::PopTransform
        )
    }
}

/// A command and its bounds in list coordinates (after transforms and clips), used to skip it
/// when it does not meet the damage (ADR-0039 D5). State commands have empty bounds.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Entry {
    /// The command.
    pub(crate) command: Command,
    /// What it can touch, in logical pixels of the list.
    pub(crate) bounds: Rect,
}
