// SPDX-License-Identifier: GPL-3.0-or-later

//! A colour swatch (SCALETTA F2.12b1, ADR-0037 T7): a small square filled with one colour token,
//! as the theme editor shows next to each token's value.
//!
//! # Why a token and not a colour
//! The display list records themed commands (ADR-0039): the renderer resolves each token with the
//! theme it draws with. A swatch of a token therefore always shows that token's value **in the
//! half on screen**, which is the half the theme editor edits (choice 2 of F2.12b1): a change
//! applied to the theme repaints the swatch with every other use of the token, with no colour of
//! its own to keep in step.
//!
//! # Looks and accessibility
//! A rounded square as high as a control's text line (two thirds of a control), with the
//! `control-border` line around it so that a colour equal to the background stays visible. It is
//! decorative: the value is written beside it, so it is a plain container for assistive
//! technologies and takes no focus.

use crate::draw::geometry::RoundedRect;
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::token::ColorToken;

use super::layout::{Constraints, Size};
use super::tree::{LayoutCx, RecordCx, Widget};

/// A square showing one colour token (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Swatch {
    /// The token shown.
    token: ColorToken,
}

impl Swatch {
    /// A swatch of `token`.
    #[must_use]
    pub const fn new(token: ColorToken) -> Self {
        Self { token }
    }

    /// The token shown.
    #[must_use]
    pub const fn token(&self) -> ColorToken {
        self.token
    }
}

impl<M: 'static> Widget<M> for Swatch {
    /// A square of two thirds of a control's height.
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let side = (cx.spacing().control_height * 2.0 / 3.0).round();
        constraints.constrain(Size::new(side, side))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let radius = f32::from(cx.shape.corner_radius).min(cx.rect.height() / 4.0);
        recorder.fill_rounded_rect(RoundedRect::new(cx.rect, radius), self.token);
        let line = f32::from(cx.shape.line_width.max(1));
        recorder.stroke_rounded_rect(
            RoundedRect::new(cx.rect.inflate(-line / 2.0), (radius - line / 2.0).max(0.0)),
            StrokeStyle::new(line),
            ColorToken::ControlBorder,
        );
    }
}
