// SPDX-License-Identifier: GPL-3.0-or-later

//! Damage in device pixels: the regions a frame redraws and presents (ADR-0039 D5, ADR-0040 W4).
//!
//! The renderer draws these regions into the frame and the window copies the same regions to the
//! screen, so both use this one computation: a pixel that was redrawn is always a pixel that is
//! presented, and the other way round.

use super::damage::{Damage, MAX_RECTS};
use super::geometry::Rect;

/// A rectangle of whole device pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeviceRect {
    /// Left column.
    pub(crate) x: u32,
    /// Top row.
    pub(crate) y: u32,
    /// Width in pixels, ≥ 1.
    pub(crate) width: u32,
    /// Height in pixels, ≥ 1.
    pub(crate) height: u32,
}

impl DeviceRect {
    /// Number of pixels.
    pub(crate) fn area(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }
}

/// The device regions of one frame's damage: either the whole frame or up to [`MAX_RECTS`]
/// rectangles, never empty ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DeviceRegions {
    /// The regions; only the first `count` are meaningful.
    rects: [DeviceRect; MAX_RECTS],
    /// How many are in use.
    count: usize,
}

impl DeviceRegions {
    /// The regions of `damage` on a frame of `width × height` device pixels at `scale`.
    ///
    /// Each logical rectangle is multiplied by the scale, rounded outwards, grown by one pixel of
    /// anti-aliasing fringe and clamped to the frame (ADR-0039 D5). If the regions cover more than
    /// half the frame, or the damage is "everything", the result is the whole frame: one full
    /// redraw is then cheaper than many clipped ones.
    ///
    /// # Complexity
    /// O(`MAX_RECTS`): a fixed bound, no allocation.
    pub(crate) fn new(damage: &Damage, scale: f32, width: u32, height: u32) -> Self {
        let frame = DeviceRect {
            x: 0,
            y: 0,
            width,
            height,
        };
        let whole = Self {
            rects: [frame; MAX_RECTS],
            count: 1,
        };
        if damage.is_empty() || width == 0 || height == 0 {
            return Self {
                rects: [frame; MAX_RECTS],
                count: 0,
            };
        }
        if damage.is_all() {
            return whole;
        }
        let mut regions = Self {
            rects: [frame; MAX_RECTS],
            count: 0,
        };
        let mut area = 0_u64;
        for rect in damage.rects() {
            if let Some(device) = device_region(rect, scale, width, height)
                && let Some(slot) = regions.rects.get_mut(regions.count)
            {
                area += device.area();
                *slot = device;
                regions.count += 1;
            }
        }
        if area.saturating_mul(2) > frame.area() {
            whole
        } else {
            regions
        }
    }

    /// The regions.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &DeviceRect> {
        self.rects.iter().take(self.count)
    }

    /// `true` when the only region is the whole `width × height` frame.
    pub(crate) fn is_whole(&self, width: u32, height: u32) -> bool {
        self.count == 1
            && self.rects.first().is_some_and(|rect| {
                rect.x == 0 && rect.y == 0 && rect.width == width && rect.height == height
            })
    }
}

/// The device region of a logical damage rectangle (ADR-0039 D5): times the scale, rounded
/// outwards, plus one pixel of anti-aliasing fringe, clamped to the frame. `None` when nothing is
/// left.
fn device_region(rect: &Rect, scale: f32, width: u32, height: u32) -> Option<DeviceRect> {
    let fringe = 1.0;
    let device = Rect::new(
        (rect.x0 * scale).floor() - fringe,
        (rect.y0 * scale).floor() - fringe,
        (rect.x1 * scale).ceil() + fringe,
        (rect.y1 * scale).ceil() + fringe,
    );
    let (x0, y0, x1, y1) = pixel_span(&device, width, height)?;
    Some(DeviceRect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

/// The whole pixels a device rectangle touches, clamped to `width × height`, as
/// `(first column, first row, end column, end row)` (ends exclusive). `None` when empty.
pub(crate) fn pixel_span(rect: &Rect, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    if rect.is_empty() {
        return None;
    }
    let x0 = clamp_to_pixels(rect.x0.floor(), width);
    let y0 = clamp_to_pixels(rect.y0.floor(), height);
    let x1 = clamp_to_pixels(rect.x1.ceil(), width);
    let y1 = clamp_to_pixels(rect.y1.ceil(), height);
    (x0 < x1 && y0 < y1).then_some((x0, y0, x1, y1))
}

/// A whole-pixel coordinate clamped to `0..=limit`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to 0..=limit (a u32) first, and is already a whole number"
)]
fn clamp_to_pixels(value: f32, limit: u32) -> u32 {
    value.clamp(0.0, to_f32(limit)) as u32
}

/// A pixel count as `f32`. Exact up to 2²⁴ (16 million pixels a side), far beyond any surface.
#[allow(
    clippy::cast_precision_loss,
    reason = "pixel coordinates stay far below 2^24, where f32 is exact"
)]
pub(crate) fn to_f32(value: u32) -> f32 {
    value as f32
}
