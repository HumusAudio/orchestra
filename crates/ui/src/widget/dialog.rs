// SPDX-License-Identifier: GPL-3.0-or-later

//! Our dialogs (ADR-0050 DG1–DG3, SCALETTA F2.12a3a): what a question or an error is made of,
//! and the two widgets that draw it.
//!
//! # The pieces
//! - [`DialogSpec`]: what the application asks: the kind (a question or an error), a title, a
//!   message, an optional detail, and the buttons, each with its [`ButtonRole`] and the message it
//!   sends. [`Ui::open_dialog`](crate::Ui::open_dialog) turns it into widgets.
//! - `DialogRoot`: the root of a dialog's layer. It spans the **whole window** but draws
//!   nothing, centres its one child, the panel, and **consumes** every press, release and move that
//!   reaches it: a click outside the panel does nothing (ADR-0045 WB9; unlike a menu, a dialog is
//!   never closed by a stray click).
//! - `DialogPanel`: the box itself, `surface-overlay` with the menus' corner and a **frame in the
//!   accent** as wide as the `dialog-frame` shape token (3 logical pixels in every pair), laid out as
//!   a column: the texts (title, message, detail) in a vertical scroll area, then the row of
//!   buttons, right-aligned, always visible. It handles **Enter** and **Escape** when no child did
//!   (C4): Enter sends the default button's message, Escape the cancel button's (or the only
//!   button's), and both close the dialog.
//!
//! # Which button is the default, which one cancels (C4, DG2)
//! The default is the first *affirmative* button; it is drawn with the accent and has the focus
//! when the dialog opens. A *destructive* button (*Don't save*, *Delete*) is never the default. The
//! cancel button is the first *cancel* button; with a single button, Escape presses it whatever
//! its role. Without an affirmative button the focus starts on the cancel button, or the first one.
//!
//! # The order of the buttons (C3)
//! [`ButtonOrder`](crate::ButtonOrder) decides it: affirmative first (Windows, KDE Plasma):
//! affirmative, destructive, others, cancel; affirmative last (macOS, GNOME): destructive, others,
//! cancel, affirmative. Within a role the buttons keep the order they were given.
//!
//! # A frame, no veil, no shadow, no icon
//! ADR-0050 DG2 first put a translucent veil over the whole window under a dialog. Measured in
//! F2.12a3a, blending it cost about 43 instructions per pixel in `tiny-skia` (about 47 M
//! instructions to open a dialog in a 1000 × 1000 window, about 90 M at 1080p, against a frame's
//! budget of 33 M). After screenshots of both, the owner chose a thick frame in the accent instead
//! and no veil: only the panel's rectangle is drawn, so opening a dialog costs the same in any
//! window (about 13 M instructions, the panel's texts and glyphs included). The window behind stays
//! as it is; the frame, the default button in the accent and the focus inside tell where the
//! keyboard is. The panel has no shadow, like the menus (owner's choice B in F2.11d2a). There is no
//! icon for an error yet (no icon set exists, C6): an error is told by its role for assistive
//! technologies and by its words on screen.
//!
//! # Size (C2)
//! At most [`MAX_WIDTH`] logical pixels wide, at least [`MIN_WIDTH`] (less when the window is
//! narrower), never closer than the large gap to the window's edges. When the window is too low for
//! the texts, they scroll and the buttons stay.

use crate::draw::geometry::{Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::token::ColorToken;
use crate::widget::access::{AccessInfo, AccessRole};
use crate::widget::layout::{Constraints, Size};
use crate::widget::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};
use crate::window::input::{ButtonState, Key, NamedKey};

/// The widest a dialog's panel gets, logical pixels (C2): the width of a comfortable line of
/// message text, as GNOME's and Windows' message dialogs keep theirs.
pub const MAX_WIDTH: f32 = 480.0;

/// The narrowest a dialog's panel gets when the window allows it, logical pixels: room for two
/// buttons side by side and a short title.
pub const MIN_WIDTH: f32 = 320.0;

/// What a dialog is for (DG2, DG3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DialogKind {
    /// A question the program asks ("Save the changes?").
    #[default]
    Question,
    /// An error to report: an alert for assistive technologies (`AlertDialog`).
    Error,
}

/// What a dialog's button does (DG2), which decides its place in the row and whether Enter or
/// Escape presses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonRole {
    /// Goes ahead (*OK*, *Save*): the first one is the default.
    Affirmative,
    /// Goes ahead by throwing something away (*Don't save*, *Delete*): never the default.
    Destructive,
    /// Backs out (*Cancel*, *Close*): the first one answers Escape.
    Cancel,
    /// Anything else (*Help*, *Retry*).
    Other,
}

/// One button of a dialog.
#[derive(Debug, Clone)]
pub struct DialogButton<M> {
    /// Its text, already localised.
    pub text: String,
    /// What it does.
    pub role: ButtonRole,
    /// The message it sends when pressed (the dialog then closes by itself).
    pub message: M,
}

/// A question or an error to show (see the module documentation). Every text is already
/// localised by the application (CLAUDE.md §7).
#[derive(Debug, Clone)]
pub struct DialogSpec<M> {
    /// A question or an error.
    pub kind: DialogKind,
    /// The title, in the panel's heading and as the dialog's accessible name.
    pub title: String,
    /// The message, the dialog's accessible description.
    pub message: String,
    /// An optional detail under the message, in the muted colour (a file's path, an error's
    /// cause).
    pub detail: Option<String>,
    /// The buttons, in the order they were added; [`Ui::open_dialog`](crate::Ui::open_dialog)
    /// places them in the system's order.
    pub buttons: Vec<DialogButton<M>>,
}

impl<M> DialogSpec<M> {
    /// A dialog of `kind` with a title and a message and no button yet.
    #[must_use]
    pub fn new(kind: DialogKind, title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            title: title.into(),
            message: message.into(),
            detail: None,
            buttons: Vec::new(),
        }
    }

    /// The same dialog with a detail under the message.
    #[must_use]
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// The same dialog with one more button.
    #[must_use]
    pub fn button(mut self, text: impl Into<String>, role: ButtonRole, message: M) -> Self {
        self.buttons.push(DialogButton {
            text: text.into(),
            role,
            message,
        });
        self
    }

    /// The index (in [`buttons`](Self::buttons)) of the default button: the first affirmative
    /// one, if any.
    #[must_use]
    pub fn default_button(&self) -> Option<usize> {
        self.buttons
            .iter()
            .position(|button| button.role == ButtonRole::Affirmative)
    }

    /// The index of the button Escape presses: the first cancel one, or the only button.
    #[must_use]
    pub fn cancel_button(&self) -> Option<usize> {
        self.buttons
            .iter()
            .position(|button| button.role == ButtonRole::Cancel)
            .or_else(|| (self.buttons.len() == 1).then_some(0))
    }

    /// The indices of the buttons in the order they are drawn, left to right, for `order` (see
    /// the module documentation). O(buttons).
    #[must_use]
    pub fn display_order(&self, order: crate::ButtonOrder) -> Vec<usize> {
        let ranks: &[ButtonRole] = match order {
            crate::ButtonOrder::AffirmativeFirst => &[
                ButtonRole::Affirmative,
                ButtonRole::Destructive,
                ButtonRole::Other,
                ButtonRole::Cancel,
            ],
            crate::ButtonOrder::AffirmativeLast => &[
                ButtonRole::Destructive,
                ButtonRole::Other,
                ButtonRole::Cancel,
                ButtonRole::Affirmative,
            ],
        };
        ranks
            .iter()
            .flat_map(|&role| {
                self.buttons
                    .iter()
                    .enumerate()
                    .filter(move |(_, button)| button.role == role)
                    .map(|(index, _)| index)
            })
            .collect()
    }
}

/// The root of a dialog's layer: invisible, over the whole window, centring the panel (see the
/// module documentation).
#[derive(Debug, Default)]
pub(crate) struct DialogRoot;

impl<M: 'static> Widget<M> for DialogRoot {
    /// The whole window: the interface measures the root with the window's tight size.
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let margin = cx.spacing().gap_l;
        let room = Size::new(
            (constraints.max.width - 2.0 * margin).max(0.0),
            (constraints.max.height - 2.0 * margin).max(0.0),
        );
        if cx.child_count() > 0 {
            cx.measure_child(0, Constraints::loose(room));
        }
        constraints.max
    }

    /// The panel in the middle, on whole logical pixels (so its text keeps its cached glyph
    /// phases from one opening to the next, as the tooltip does).
    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        if cx.child_count() == 0 {
            return;
        }
        let size = cx.child_size(0);
        let x = (rect.x0 + (rect.width() - size.width) / 2.0).round();
        let y = (rect.y0 + (rect.height() - size.height) / 2.0).round();
        cx.place_child(0, Rect::new(x, y, x + size.width, y + size.height));
    }

    /// Nothing: the root is invisible (no veil, see the module documentation). Its segment stays
    /// empty, so it damages no pixel of the window behind.
    fn record(&self, _cx: &RecordCx, _recorder: &mut Recorder<'_>) {}

    /// Every pointer event that reaches the root is consumed: outside the panel nothing happens.
    fn event(&mut self, _cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        match event {
            WidgetEvent::PointerButton { .. }
            | WidgetEvent::PointerMoved { .. }
            | WidgetEvent::Wheel { .. } => Handled::Yes,
            _ => Handled::No,
        }
    }
}

/// The box of a dialog (see the module documentation). Its children: the scroll area of the
/// texts, then the buttons in their display order.
#[derive(Debug)]
pub(crate) struct DialogPanel<M> {
    /// A question or an error.
    kind: DialogKind,
    /// The title, the accessible name.
    title: String,
    /// The message, the accessible description.
    message: String,
    /// What Enter sends, when no child handled it: the default button's message.
    enter: Option<M>,
    /// What Escape sends: the cancel button's message.
    escape: Option<M>,
}

impl<M> DialogPanel<M> {
    /// The panel of `kind`, with its texts and the messages of Enter and Escape.
    pub(crate) const fn new(
        kind: DialogKind,
        title: String,
        message: String,
        enter: Option<M>,
        escape: Option<M>,
    ) -> Self {
        Self {
            kind,
            title,
            message,
            enter,
            escape,
        }
    }
}

impl<M: Clone + std::fmt::Debug + 'static> Widget<M> for DialogPanel<M> {
    /// The texts at the panel's width, the buttons at their natural size in a row (see the module
    /// documentation).
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let spacing = cx.spacing();
        let pad = spacing.gap_l;
        let max = constraints.max;
        // The buttons first: the row's width can widen the panel, and its height is kept.
        let mut row_width: f32 = 0.0;
        let mut row_height: f32 = 0.0;
        for child in 1..cx.child_count() {
            let size = cx.measure_child(child, Constraints::loose(max));
            // A small gap before every button but the first.
            if child > 1 {
                row_width += spacing.gap_s;
            }
            row_width += size.width;
            row_height = row_height.max(size.height);
        }
        // The widest the panel may be: MAX_WIDTH, or the buttons' row if it is wider, never more
        // than the window gives.
        let cap = MAX_WIDTH.max(row_width + 2.0 * pad).min(max.width);
        if cx.child_count() == 0 {
            return constraints.constrain(Size::new(cap.min(MIN_WIDTH), 2.0 * pad));
        }
        // The texts' natural width within the cap (a long message wraps at it, a short one is
        // narrower), then the panel's width: at least MIN_WIDTH when the window allows it.
        let natural = cx.measure_child(
            0,
            Constraints::loose(Size::new((cap - 2.0 * pad).max(0.0), max.height)),
        );
        let width = (natural.width.max(row_width) + 2.0 * pad)
            .max(MIN_WIDTH)
            .min(cap);
        // The texts again at the panel's width, in the height left once the buttons are placed:
        // the scroll area takes the rest when they do not fit.
        let texts_room = Size::new(
            (width - 2.0 * pad).max(0.0),
            (max.height - row_height - 3.0 * pad).max(0.0),
        );
        let texts = cx.measure_child(0, Constraints::loose(texts_room));
        let height = pad + texts.height + pad + row_height + pad;
        constraints.constrain(Size::new(width, height))
    }

    fn arrange(&mut self, cx: &mut LayoutCx<'_, M>, rect: Rect) {
        let spacing = cx.spacing();
        let pad = spacing.gap_l;
        if cx.child_count() == 0 {
            return;
        }
        let texts = cx.child_size(0);
        cx.place_child(
            0,
            Rect::new(
                rect.x0 + pad,
                rect.y0 + pad,
                rect.x1 - pad,
                rect.y0 + pad + texts.height,
            ),
        );
        // The buttons right-aligned on the bottom line, from the last one leftwards.
        let mut x = rect.x1 - pad;
        for child in (1..cx.child_count()).rev() {
            let size = cx.child_size(child);
            let y1 = rect.y1 - pad;
            cx.place_child(child, Rect::new(x - size.width, y1 - size.height, x, y1));
            x -= size.width + spacing.gap_s;
        }
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let radius = f32::from(cx.shape.corner_radius);
        recorder.fill_rounded_rect(RoundedRect::new(rect, radius), ColorToken::SurfaceOverlay);
        // The frame in the accent, inside the box's edge (as the menus draw their border), as
        // wide as the `dialog-frame` token; at least one pixel, so a custom theme that sets it
        // to 0 still shows the panel's edge.
        let width = f32::from(cx.shape.dialog_frame.max(1));
        recorder.stroke_rounded_rect(
            RoundedRect::new(rect.inflate(-width / 2.0), (radius - width / 2.0).max(0.0)),
            StrokeStyle::new(width),
            ColorToken::Accent,
        );
    }

    /// Enter and Escape that no child handled (C4): the default's or the cancel button's message,
    /// then the dialog closes. Every other key is left to bubble to the root, which ignores it:
    /// nothing reaches the window behind.
    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        let WidgetEvent::Key {
            key: Key::Named(named),
            state: ButtonState::Pressed,
            repeat: false,
            ..
        } = event
        else {
            return Handled::No;
        };
        let message = match named {
            NamedKey::Enter => self.enter.clone(),
            NamedKey::Escape => self.escape.clone(),
            _ => return Handled::No,
        };
        if let Some(message) = message {
            cx.emit(message);
            cx.close_dialog();
            return Handled::Yes;
        }
        Handled::No
    }

    fn access_role(&self) -> AccessRole {
        match self.kind {
            DialogKind::Question => AccessRole::Dialog,
            DialogKind::Error => AccessRole::AlertDialog,
        }
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.title.clone());
        info.description = Some(self.message.clone());
        info.modal = true;
    }
}

#[cfg(test)]
mod tests {
    //! The choice of the default and cancel buttons, and the display order.
    use super::*;
    use crate::ButtonOrder;

    /// A dialog with the three usual answers, added in an arbitrary order.
    fn save_question() -> DialogSpec<u8> {
        // i18n-ignore: test data, a dialog's texts come from the catalogues
        DialogSpec::new(DialogKind::Question, "Save?", "Save the changes?") // i18n-ignore: test data
            .button("Cancel", ButtonRole::Cancel, 0)
            .button("Don't save", ButtonRole::Destructive, 1) // i18n-ignore: test data
            .button("Save", ButtonRole::Affirmative, 2)
    }

    /// The default is the affirmative button, Escape the cancel one; a lone button answers
    /// Escape; a destructive button is never the default.
    #[test]
    fn default_and_cancel() {
        let spec = save_question();
        assert_eq!(spec.default_button(), Some(2));
        assert_eq!(spec.cancel_button(), Some(0));
        let lone = DialogSpec::new(DialogKind::Error, "E", "m").button("OK", ButtonRole::Other, 0);
        assert_eq!(lone.default_button(), None);
        assert_eq!(lone.cancel_button(), Some(0));
        let destructive = DialogSpec::new(DialogKind::Question, "D", "m")
            .button("Delete", ButtonRole::Destructive, 0)
            .button("Keep", ButtonRole::Other, 1);
        assert_eq!(destructive.default_button(), None);
        assert_eq!(destructive.cancel_button(), None);
    }

    /// Windows and KDE: Save, Don't save, Cancel; macOS and GNOME: Don't save, Cancel, Save.
    #[test]
    fn display_order_follows_the_system() {
        let spec = save_question();
        assert_eq!(
            spec.display_order(ButtonOrder::AffirmativeFirst),
            vec![2, 1, 0]
        );
        assert_eq!(
            spec.display_order(ButtonOrder::AffirmativeLast),
            vec![1, 0, 2]
        );
    }
}
