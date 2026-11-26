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

use crate::window::input::{ButtonState, Key, NamedKey, PointerButton};

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

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

/// A swatch that is also a button (F2.12b3b): the theme editor's swatch, which opens the colour
/// picker of its row. It draws as [`Swatch`] and acts as a [`Button`](crate::Button): a click,
/// Enter or Space, or an assistive technology's click sends its message; it takes the focus and
/// draws the focus ring; assistive technologies hear a button named with the label the
/// application gives ("Choose “Accent color”", already localised).
#[derive(Debug)]
pub struct SwatchButton<M> {
    /// The token shown.
    token: ColorToken,
    /// The message of a click.
    message: M,
    /// The name for assistive technologies.
    label: String,
    /// Pressed by the pointer and not yet released.
    pressed: bool,
}

impl<M> SwatchButton<M> {
    /// A swatch of `token` sending `message` when clicked, named `label`.
    #[must_use]
    pub fn new(token: ColorToken, message: M, label: impl Into<String>) -> Self {
        Self {
            token,
            message,
            label: label.into(),
            pressed: false,
        }
    }

    /// The token shown.
    #[must_use]
    pub const fn token(&self) -> ColorToken {
        self.token
    }
}

impl<M: Clone + std::fmt::Debug + 'static> Widget<M> for SwatchButton<M> {
    /// As [`Swatch`]: a square of two thirds of a control's height.
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
        if cx.focused {
            super::look::focus_ring(recorder, cx.rect, radius, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        match *event {
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                ..
            } => {
                self.pressed = true;
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                position,
                ..
            } => {
                // A click is a press and a release on the swatch, as a button's (dragging out
                // cancels).
                let rect = cx.rect();
                let inside = (rect.x0..=rect.x1).contains(&position.x)
                    && (rect.y0..=rect.y1).contains(&position.y);
                if std::mem::take(&mut self.pressed) && inside {
                    cx.emit(self.message.clone());
                }
                Handled::Yes
            }
            WidgetEvent::PointerMoved { .. } => Handled::Yes,
            WidgetEvent::Key {
                key: Key::Named(NamedKey::Enter | NamedKey::Space),
                state: ButtonState::Pressed,
                repeat: false,
                ..
            }
            | WidgetEvent::Activate => {
                cx.emit(self.message.clone());
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::Button
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.label.clone());
        info.clickable = true;
    }
}
