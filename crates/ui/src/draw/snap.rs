// SPDX-License-Identifier: GPL-3.0-or-later

//! Pixel snapping (ADR-0039 D7), shared by the backends: straight edges land on whole device
//! pixels, so a 1 px staff line or a widget border covers exactly one row of pixels in every
//! backend, never two half-transparent ones.
//!
//! These are pure functions of device coordinates, kept apart from the `tiny-skia` backend since
//! F2.06b (when a second backend was tried, ADR-0042, then dropped, ADR-0043): any future backend,
//! a GPU one included (ADR-0042 R2), must snap with exactly these rules.

use super::geometry::Rect;

/// The control-point distance of a cubic Bézier approximating a quarter circle of radius 1:
/// 4/3 · (√2 − 1) ≈ 0.552 285 (the standard value; error below 0.03 % of the radius).
pub(crate) const KAPPA: f32 = 0.552_284_8;

/// Snaps a device rectangle to whole pixels: each edge to the nearest pixel boundary; a non-empty
/// rectangle thinner than half a pixel keeps one pixel (the one containing its centre), so a
/// hairline never vanishes. `None` for an empty or invalid rectangle.
///
/// # Complexity
/// O(1).
pub(crate) fn snap_rect(rect: &Rect) -> Option<Rect> {
    if rect.is_empty() {
        return None;
    }
    let (mut x0, mut x1) = (rect.x0.round(), rect.x1.round());
    let (mut y0, mut y1) = (rect.y0.round(), rect.y1.round());
    if x1 <= x0 {
        // Keep one pixel, the one containing the rectangle's centre.
        x0 = f32::midpoint(rect.x0, rect.x1).floor();
        x1 = x0 + 1.0;
    }
    if y1 <= y0 {
        y0 = f32::midpoint(rect.y0, rect.y1).floor();
        y1 = y0 + 1.0;
    }
    let snapped = Rect::new(x0, y0, x1, y1);
    // Non-finite input (NaN, infinities) survives `round`; reject it here.
    (snapped.x0.is_finite()
        && snapped.y0.is_finite()
        && snapped.x1.is_finite()
        && snapped.y1.is_finite())
    .then_some(snapped)
}

/// The first pixel row (or column) of a snapped line of `band` whole pixels centred on `centre`
/// (device pixels): an odd band is centred on the pixel containing `centre`; an even band on the
/// pixel boundary nearest to it.
///
/// # Complexity
/// O(1).
pub(crate) fn band_start(centre: f32, band: f32) -> f32 {
    // `band` is a whole number ≥ 1; its parity decides where the middle falls.
    let odd = (band % 2.0 - 1.0).abs() < 0.5;
    if odd {
        centre.floor() - (band - 1.0) * 0.5
    } else {
        centre.round() - band * 0.5
    }
}

/// The snapped device rectangle of a horizontal or vertical line from `a` to `b` (device
/// points), `width` device pixels wide, extended by `extend` at both ends (0 for butt caps, half
/// the width for square caps). `None` when the line is neither horizontal nor vertical (it is
/// then stroked as an anti-aliased path) or when the result is empty.
///
/// # Why here
/// Both backends must turn the same line into the same band of pixels (ADR-0039 D7).
pub(crate) fn snapped_line(a: (f32, f32), b: (f32, f32), width: f32, extend: f32) -> Option<Rect> {
    let horizontal = (a.1 - b.1).abs() < f32::EPSILON;
    let vertical = (a.0 - b.0).abs() < f32::EPSILON;
    let band = width.round().max(1.0);
    let rect = if horizontal {
        let (start, end) = (a.0.min(b.0) - extend, a.0.max(b.0) + extend);
        let top = band_start(a.1, band);
        Rect::new(start.round(), top, end.round(), top + band)
    } else if vertical {
        let (start, end) = (a.1.min(b.1) - extend, a.1.max(b.1) + extend);
        let left = band_start(a.0, band);
        Rect::new(left, start.round(), left + band, end.round())
    } else {
        return None;
    };
    (!rect.is_empty()).then_some(rect)
}
