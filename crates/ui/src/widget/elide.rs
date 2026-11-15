// SPDX-License-Identifier: GPL-3.0-or-later

//! The text of a control, shortened with an ellipsis when it does not fit (step F2.09f; ADR-0045
//! WJ1–WJ7): what [`Label`](super::label::Label), [`Toggle`](super::toggle::Toggle) and
//! [`Button`](super::button::Button) share.
//!
//! # The two passes
//! - **Measure**: the text is laid out whole and its natural size is the answer, exactly as
//!   before this step: a control still *asks* for the room its whole text needs (WJ6).
//! - **Arrange**: the control learns the rectangle it really got. If the room for the text is
//!   narrower than the whole text, a second layout holds "start + ellipsis"
//!   ([`Shaper::elide`](orchestra_text::Shaper::elide)); otherwise nothing is done.
//!
//! [`ElidedText::shown`] is then the layout to draw. The text for assistive technologies is the
//! control's own string, always whole (WJ1): eliding is a matter of drawing only.
//!
//! # What it costs
//! Nothing when the text fits (one comparison). When it does not, the shortened text is shaped
//! (usually twice, see `Shaper::elide`) each time the width changes: at every move of a
//! splitter's divider, for the controls it narrows. A layout pass in which the width is the same
//! as before answers from what is kept here.

use orchestra_text::{Elision, TextLayout};

use crate::theme::text::{FamilyToken, SizeToken, Weight};

use super::layout::Size;
use super::tree::LayoutCx;

/// A control's text in both forms: whole, and shortened for the width the control got.
#[derive(Debug)]
pub(crate) struct ElidedText {
    /// The whole text laid out: what the control measures with, and what it draws when it fits.
    full: TextLayout,
    /// The text shortened for `fitted_for`, meaningful only when `elision` is not
    /// [`Elision::Whole`]. Kept between layouts so its vectors are reused.
    short: TextLayout,
    /// The wrapping width (`None`: one line) and the text generation (see
    /// [`LayoutCx::text_generation`]) `full` was made for, if it is current.
    laid_out_for: Option<(Option<f32>, u64)>,
    /// The width `elision` and `short` were decided for, if they are current for `full`.
    fitted_for: Option<f32>,
    /// What the last fit decided.
    elision: Elision,
}

impl ElidedText {
    /// Nothing laid out yet. Allocates nothing.
    pub(crate) const fn new() -> Self {
        Self {
            full: TextLayout::new(),
            short: TextLayout::new(),
            laid_out_for: None,
            fitted_for: None,
            elision: Elision::Whole,
        }
    }

    /// Forgets the layouts: the control's string changed. The next measure lays it out again.
    pub(crate) const fn invalidate(&mut self) {
        self.laid_out_for = None;
    }

    /// Lays `text` out whole if it is not current (new text, new theme text tokens, new wrapping
    /// width) and answers its natural size. `wrap_width` is `None` for a one-line text.
    ///
    /// A new whole layout makes the last fit stale: the next [`ElidedText::fit`] decides again,
    /// even for the same width.
    pub(crate) fn measure<M: 'static>(
        &mut self,
        cx: &mut LayoutCx<'_, M>,
        text: &str,
        style: (FamilyToken, SizeToken, Weight),
        wrap_width: Option<f32>,
    ) -> Size {
        let key = (wrap_width, cx.text_generation());
        if self.laid_out_for != Some(key) {
            if cx
                .lay_out_text(text, style, wrap_width, &mut self.full)
                .is_err()
            {
                // Impossible with the bundled faces; an empty layout keeps the window usable.
                self.full = TextLayout::new();
            }
            self.laid_out_for = Some(key);
            self.fitted_for = None;
            // Until the next fit the whole text is what there is to draw.
            self.elision = Elision::Whole;
        }
        Size::new(self.full.width(), self.full.height())
    }

    /// Decides what to draw in `width` logical pixels: the whole text if it fits, its start with
    /// an ellipsis otherwise (the ellipsis alone, or nothing, when the width is tiny: WJ5).
    ///
    /// `text` and `style` must be the ones of the last [`ElidedText::measure`], which must have
    /// been one-line (`wrap_width` `None`): a wrapping text is never elided (WJ7), its control
    /// simply does not call this.
    pub(crate) fn fit<M: 'static>(
        &mut self,
        cx: &mut LayoutCx<'_, M>,
        text: &str,
        style: (FamilyToken, SizeToken, Weight),
        width: f32,
    ) {
        // A rectangle narrower than the control's own parts gives a negative remainder: no room.
        let width = width.max(0.0);
        if self.fitted_for == Some(width) {
            return;
        }
        self.elision = cx
            .elide_text(text, style, width, &self.full, &mut self.short)
            // Impossible with the bundled faces and a non-negative width; drawing the whole text
            // (clipped by the pane, as before this step) keeps the window usable.
            .unwrap_or(Elision::Whole);
        self.fitted_for = Some(width);
    }

    /// The layout to draw: the whole text, or the shortened one after a fit that had to cut.
    pub(crate) const fn shown(&self) -> &TextLayout {
        match self.elision {
            Elision::Whole => &self.full,
            Elision::Cut { .. } | Elision::Hidden => &self.short,
        }
    }

    /// `true` when the last fit had to shorten (or hide) the text.
    pub(crate) const fn is_elided(&self) -> bool {
        !matches!(self.elision, Elision::Whole)
    }
}
