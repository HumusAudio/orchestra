// SPDX-License-Identifier: GPL-3.0-or-later

//! What every control draws the same way (F2.09c, choice WD4): the keyboard focus ring.
//!
//! One place, so that every control's focus looks the same and a change of the focus tokens
//! (`focus` colour, `focus-width`) reaches all of them.

use crate::draw::geometry::{Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::token::{ColorToken, Shape};

/// Draws the focus ring around `rect` (a control's shape with corner `radius`): a `focus` line of
/// `focus-width`, one pixel outside the shape, so it never covers the control itself.
pub(crate) fn focus_ring(recorder: &mut Recorder<'_>, rect: Rect, radius: f32, shape: Shape) {
    let width = f32::from(shape.focus_width.max(1));
    let gap = 1.0 + width / 2.0;
    recorder.stroke_rounded_rect(
        RoundedRect::new(rect.inflate(gap), radius + gap),
        StrokeStyle::new(width),
        ColorToken::Focus,
    );
}
