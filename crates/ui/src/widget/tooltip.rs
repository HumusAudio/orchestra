// SPDX-License-Identifier: GPL-3.0-or-later

//! The tooltip (F2.11c, ADR-0048 AM14): a small box of text above everything, shown by the
//! interface itself after the pointer rests on a control that has something to say.
//!
//! # Who decides what it says
//! A control answers [`Widget::tooltip`]: today the label, the button and the toggles answer
//! their whole text while it is drawn shortened with an ellipsis (F2.09f), and nothing otherwise;
//! the controls tied to an action will add its name and shortcut (F2.11d). The tooltip has no
//! text of its own, so it needs no catalogue entry (CLAUDE.md §7 has nothing to translate here).
//!
//! # Looks (choice 3 of F2.11c)
//! The `text` colour on `surface-overlay` in `size-small` (12 px by default): that pair is already
//! one of the contrast pairs every theme is checked against (ADR-0037 T4). A `border` line of the
//! theme's `line-width` and the theme's `corner-radius`. At most [`MAX_WIDTH`] wide, the text
//! wrapping inside it.
//!
//! # No shadow (owner's choice B in F2.11c)
//! The overlay shadow was planned, and measured: the renderer caches a shadow's coverage keyed by
//! its exact position (ADR-0043 K2), and a tooltip opens at the pointer, so nearly every tooltip
//! computed its shadow anew, 15.4 M instructions in the reference panel against 2.0 M without it
//! (0.23 M against 1.66 M when the same tooltip shows again at the same place). The menus dropped
//! it too, for the same reason (owner's choice B in F2.11d2a, see `widget::menu`).
//!
//! # When it shows and hides
//! The interface's job, not the widget's: see the tooltip section of [`Ui`](crate::Ui)'s module.
//! This widget only lays its text out and draws it.
//!
//! # Accessibility
//! Not a node for assistive technologies (ADR-0048 AM14): they already have the whole text of the
//! control (ADR-0045, F2.09f). The interface keeps popups out of the accessibility tree.

use orchestra_text::TextLayout;

use crate::draw::geometry::{Point, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;

use super::layout::{Constraints, Size};
use super::tree::{LayoutCx, RecordCx, Widget};

/// The tooltip's text style: the interface family, small size, regular weight.
const STYLE: (FamilyToken, SizeToken, Weight) =
    (FamilyToken::Ui, SizeToken::Small, Weight::Regular);

/// The widest a tooltip may be, border and padding included, logical pixels (choice 3 of F2.11c):
/// about fifty characters of 12 px text a line, the width the platforms' own tooltips wrap at.
pub(crate) const MAX_WIDTH: f32 = 320.0;

/// The tooltip's box (see the module documentation).
#[derive(Debug)]
pub(crate) struct Tooltip {
    /// The text, whole, as the control gave it (already localised by the application).
    text: String,
    /// The laid-out text, wrapped at the room [`MAX_WIDTH`] leaves inside the padding.
    layout: TextLayout,
    /// The text generation and wrap width the layout was made for (see
    /// [`LayoutCx::text_generation`]); `None` when the text changed since.
    laid_out_for: Option<(u64, u32)>,
    /// `false` while hidden: the interface keeps the widget once created and hides it between
    /// tooltips, so showing one allocates nothing (it then records nothing, and measures zero).
    visible: bool,
}

impl Tooltip {
    /// A tooltip saying `text`.
    pub(crate) fn new(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            layout: TextLayout::new(),
            laid_out_for: None,
            visible: true,
        }
    }

    /// Shows or hides it; the caller records it again.
    pub(crate) const fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// What it says.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// Replaces what it says; the caller lays it out again. Keeps the string's capacity.
    pub(crate) fn set_text(&mut self, text: &str) {
        if text != self.text {
            text.clone_into(&mut self.text);
            self.laid_out_for = None;
        }
    }
}

/// The padding inside the tooltip's border: `(horizontal, vertical)`, logical pixels, from the
/// theme's density (WB10): the small gap across, the extra-small gap above and below, so a
/// one-line tooltip is about as tall as a compact control.
fn padding(spacing: crate::theme::density::Spacing) -> (f32, f32) {
    (spacing.gap_s, spacing.gap_xs)
}

impl<M: 'static> Widget<M> for Tooltip {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        if !self.visible {
            return Size::ZERO;
        }
        let (pad_x, pad_y) = padding(cx.spacing());
        // The room for the text: the maximum width (or the window's, if narrower) minus the
        // padding, never negative.
        let outer = MAX_WIDTH.min(constraints.max.width);
        let room = (outer - 2.0 * pad_x).max(0.0);
        // Laid out again only when the text, the theme's text tokens or the room changed. The
        // room is compared by its bits: it is the same computation each time, so equal widths
        // give equal bits, and an f32 key would not be `Eq`.
        let key = (cx.text_generation(), room.to_bits());
        if self.laid_out_for != Some(key) {
            if cx
                .lay_out_text(&self.text, STYLE, Some(room), &mut self.layout)
                .is_err()
            {
                // Impossible with the bundled faces; an empty box keeps the window usable.
                self.layout = TextLayout::new();
            }
            self.laid_out_for = Some(key);
        }
        Size::new(
            self.layout.width() + 2.0 * pad_x,
            self.layout.height() + 2.0 * pad_y,
        )
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        // Hidden: an empty segment, so the pixels it covered are repainted.
        if !self.visible {
            return;
        }
        let rect = cx.rect;
        let radius = f32::from(cx.shape.corner_radius);
        // No shadow (owner's choice B in F2.11c): the box, its border and the text only. See the
        // module documentation for why.
        recorder.fill_rounded_rect(RoundedRect::new(rect, radius), ColorToken::SurfaceOverlay);
        // The border inside the box's edge, as the button draws its own.
        let width = f32::from(cx.shape.line_width.max(1));
        recorder.stroke_rounded_rect(
            RoundedRect::new(rect.inflate(-width / 2.0), (radius - width / 2.0).max(0.0)),
            StrokeStyle::new(width),
            ColorToken::Border,
        );
        let (pad_x, pad_y) = padding(cx.spacing);
        recorder.text(
            &self.layout,
            Point::new(rect.x0 + pad_x, rect.y0 + pad_y),
            ColorToken::Text,
        );
    }
}
