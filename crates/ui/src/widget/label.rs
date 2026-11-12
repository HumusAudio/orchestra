// SPDX-License-Identifier: GPL-3.0-or-later

//! The label (F2.09b, choice WC7): static text, in the theme's text tokens, optionally wrapped.
//!
//! # When the text is laid out again
//! Only when its text, its style or the width it may wrap at changes: the laid-out text is kept
//! with the width it was made for, and a measure with the same width (the common case: a frame
//! where something else changed) answers from it. Re-laying out reuses the layout's vectors
//! (ADR-0044 TX9).

use orchestra_text::TextLayout;

use crate::draw::geometry::Point;
use crate::draw::list::Recorder;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, Size};
use super::tree::{LayoutCx, RecordCx, Widget};

/// Static text (WC7).
#[derive(Debug)]
pub struct Label {
    /// The text, already localised by the application (CLAUDE.md §7).
    text: String,
    /// Its family, size and weight tokens.
    style: (FamilyToken, SizeToken, Weight),
    /// Its colour (`text` or `text-muted`, usually).
    color: ColorToken,
    /// Whether it wraps at the width its parent allows.
    wrap: bool,
    /// The laid-out text, and the wrapping width it was laid out for (`None`: one line), or
    /// nothing yet.
    layout: TextLayout,
    /// The wrapping width and the text generation (see [`LayoutCx::text_generation`]) `layout`
    /// was made for, if it is current.
    laid_out_for: Option<(Option<f32>, u64)>,
}

impl Label {
    /// A one-line label in the body size, regular weight, `text` colour.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: (FamilyToken::Ui, SizeToken::Body, Weight::Regular),
            color: ColorToken::Text,
            wrap: false,
            layout: TextLayout::new(),
            laid_out_for: None,
        }
    }

    /// The same label with another family, size and weight.
    #[must_use]
    pub fn styled(mut self, family: FamilyToken, size: SizeToken, weight: Weight) -> Self {
        self.style = (family, size, weight);
        self
    }

    /// The same label in another colour.
    #[must_use]
    pub const fn color(mut self, color: ColorToken) -> Self {
        self.color = color;
        self
    }

    /// The same label, wrapping at the width its parent allows.
    #[must_use]
    pub const fn wrapping(mut self) -> Self {
        self.wrap = true;
        self
    }

    /// The text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Replaces the text. The caller asks the interface to lay the label out again
    /// ([`Ui::request_layout`](crate::Ui::request_layout)).
    pub fn set_text(&mut self, text: &str) {
        if text != self.text {
            text.clone_into(&mut self.text);
            self.laid_out_for = None;
        }
    }
}

impl<M: 'static> Widget<M> for Label {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let width =
            (self.wrap && constraints.max.width.is_finite()).then_some(constraints.max.width);
        let key = (width, cx.text_generation());
        if self.laid_out_for != Some(key) {
            if cx
                .lay_out_text(&self.text, self.style, width, &mut self.layout)
                .is_err()
            {
                // Impossible with the bundled faces; an empty layout keeps the window usable.
                self.layout = TextLayout::new();
            }
            self.laid_out_for = Some(key);
        }
        Size::new(self.layout.width(), self.layout.height())
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        recorder.text(&self.layout, Point::new(cx.rect.x0, cx.rect.y0), self.color);
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::Label
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.text.clone());
    }
}
