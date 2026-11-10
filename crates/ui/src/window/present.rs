// SPDX-License-Identifier: GPL-3.0-or-later

//! Presenting the frame on screen (ADR-0040 W4): from our premultiplied RGBA frame to
//! `softbuffer`'s `0RGB` buffer, copying only the damage when the buffer allows it.
//!
//! Pure functions on plain slices, so they are tested without a window.

use crate::Image;
use crate::draw::device::{DeviceRect, DeviceRegions};

/// What to copy into the window's buffer this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyPlan {
    /// The whole frame: the buffer's contents are unknown or older than the previous frame.
    Whole,
    /// Only the damaged regions: the buffer holds exactly the previous frame.
    Damage,
}

impl CopyPlan {
    /// Chooses by the buffer's age (ADR-0040 W4): `softbuffer` reports 1 when the buffer holds the
    /// frame presented just before, 0 when its contents are unknown (always after a resize), and
    /// more when the system rotates several buffers. Only age 1 allows copying the damage alone;
    /// so does damage that is already the whole frame, trivially.
    pub(crate) fn choose(age: u8, regions: &DeviceRegions, width: u32, height: u32) -> Self {
        if age == 1 && !regions.is_whole(width, height) {
            Self::Damage
        } else {
            Self::Whole
        }
    }
}

/// One premultiplied RGBA pixel as `softbuffer`'s `0x00RRGGBB`.
///
/// The frame is always opaque (every region is first cleared with the theme's opaque `surface`,
/// ADR-0040 W4), so premultiplied and straight colours are the same and alpha can be dropped.
#[must_use]
pub(crate) fn to_0rgb(pixel: [u8; 4]) -> u32 {
    let [r, g, b, _] = pixel;
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// Copies one region of `frame` into `buffer` (`frame.width() × frame.height()` values, rows
/// without padding), converting each pixel with [`to_0rgb`]. Pixels outside either are skipped.
///
/// # Complexity
/// O(region's pixels).
pub(crate) fn copy_region(frame: &Image, region: &DeviceRect, buffer: &mut [u32]) {
    let width = frame.width() as usize;
    let data = frame.data();
    let x0 = region.x as usize;
    let columns = region.width as usize;
    for row in region.y as usize..(region.y + region.height) as usize {
        let start = row * width + x0;
        let source = data.get(start * 4..(start + columns) * 4);
        let target = buffer.get_mut(start..start + columns);
        if let (Some(source), Some(target)) = (source, target) {
            // Whole pixels only: `source` is a multiple of 4 bytes by construction.
            let (pixels, _) = source.as_chunks::<4>();
            for (out, pixel) in target.iter_mut().zip(pixels) {
                *out = to_0rgb(*pixel);
            }
        }
    }
}

/// Copies the whole frame into `buffer`.
pub(crate) fn copy_whole(frame: &Image, buffer: &mut [u32]) {
    let whole = DeviceRect {
        x: 0,
        y: 0,
        width: frame.width(),
        height: frame.height(),
    };
    copy_region(frame, &whole, buffer);
}

/// A device region as `softbuffer`'s rectangle, for `present_with_damage`. `None` for an empty
/// region (never produced by [`DeviceRegions`], checked anyway since `softbuffer` needs non-zero
/// sides).
pub(crate) fn softbuffer_rect(region: &DeviceRect) -> Option<softbuffer::Rect> {
    Some(softbuffer::Rect {
        x: region.x,
        y: region.y,
        width: std::num::NonZeroU32::new(region.width)?,
        height: std::num::NonZeroU32::new(region.height)?,
    })
}
