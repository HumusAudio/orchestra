// SPDX-License-Identifier: GPL-3.0-or-later

//! The push button (F2.09b, choice WC8).
//!
//! # How it is activated
//! - **Pointer**: a press inside captures the pointer (ADR-0045 WA3) and shows the button pressed;
//!   the release activates it only if it happens over the button. Dragging out and releasing
//!   cancels, as on every desktop platform.
//! - **Keyboard**: Enter or Space while it has the focus.
//! - **Assistive technologies**: AccessKit's click ([`WidgetEvent::Activate`]).
//!
//! Each activation emits the message the application gave it (ADR-0045 WA7). A disabled button
//! handles nothing, takes no focus and draws its text in `text-disabled`.
//!
//! # Looks
//! A rounded rectangle (`corner-radius`) of `control` (hovered `control-hover`, pressed
//! `control-pressed`) with a `control-border` line, or, for a **primary** button, `accent`
//! (`accent-hover`, `accent-pressed`) with `on-accent` text. The focus ring is a `focus` line of
//! `focus-width`, outside the button. Height `control-height`, side padding `control-padding`
//! (the density, WB10).

use orchestra_text::TextLayout;

use crate::draw::geometry::{Point, Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, Key, NamedKey, PointerButton};

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

/// The two looks of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonKind {
    /// A plain button.
    #[default]
    Normal,
    /// The main action of its group (a dialog's OK): drawn in the accent colour.
    Primary,
}

/// A push button that emits a message when activated (WC8).
#[derive(Debug)]
pub struct Button<M> {
    /// The text, already localised.
    text: String,
    /// The message emitted at each activation.
    message: M,
    /// Plain or primary.
    kind: ButtonKind,
    /// Whether it can be used.
    enabled: bool,
    /// The pointer is over it.
    hovered: bool,
    /// A press inside is held (the pointer is captured).
    pressed: bool,
    /// The laid-out text.
    layout: TextLayout,
    /// The text generation `layout` was made for (see [`LayoutCx::text_generation`]), or `None`
    /// when it must be made again.
    laid_out: Option<u64>,
}

impl<M: Clone> Button<M> {
    /// A plain, enabled button.
    #[must_use]
    pub fn new(text: impl Into<String>, message: M) -> Self {
        Self {
            text: text.into(),
            message,
            kind: ButtonKind::Normal,
            enabled: true,
            hovered: false,
            pressed: false,
            layout: TextLayout::new(),
            laid_out: None,
        }
    }

    /// The same button with another look.
    #[must_use]
    pub const fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }

    /// Enables or disables the button. The caller asks the interface to record it again.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.pressed = false;
        }
    }

    /// Whether it can be used.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Replaces the text. The caller asks the interface to lay the button out again.
    pub fn set_text(&mut self, text: &str) {
        if text != self.text {
            text.clone_into(&mut self.text);
            self.laid_out = None;
        }
    }

    /// Emits the message.
    fn activate(&self, cx: &mut EventCx<'_, M>) {
        cx.emit(self.message.clone());
    }

    /// The fill, the text colour and whether to draw a border, for the current state.
    const fn colors(&self) -> (ColorToken, ColorToken, bool) {
        if !self.enabled {
            return (ColorToken::Control, ColorToken::TextDisabled, true);
        }
        match self.kind {
            ButtonKind::Normal => {
                let fill = if self.pressed {
                    ColorToken::ControlPressed
                } else if self.hovered {
                    ColorToken::ControlHover
                } else {
                    ColorToken::Control
                };
                (fill, ColorToken::Text, true)
            }
            ButtonKind::Primary => {
                let fill = if self.pressed {
                    ColorToken::AccentPressed
                } else if self.hovered {
                    ColorToken::AccentHover
                } else {
                    ColorToken::Accent
                };
                (fill, ColorToken::OnAccent, false)
            }
        }
    }
}

/// `true` if `point` is inside `rect`.
fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.x0 && point.x < rect.x1 && point.y >= rect.y0 && point.y < rect.y1
}

impl<M: Clone + std::fmt::Debug + 'static> Widget<M> for Button<M> {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let generation = cx.text_generation();
        if self.laid_out != Some(generation) {
            let style = (FamilyToken::Ui, SizeToken::Body, Weight::Regular);
            if cx
                .lay_out_text(&self.text, style, None, &mut self.layout)
                .is_err()
            {
                self.layout = TextLayout::new();
            }
            self.laid_out = Some(generation);
        }
        let spacing = cx.spacing();
        constraints.constrain(Size::new(
            self.layout.width() + 2.0 * spacing.control_padding,
            spacing.control_height.max(self.layout.height()),
        ))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let radius = f32::from(cx.shape.corner_radius);
        let shape = RoundedRect::new(rect, radius);
        let (fill, text, border) = self.colors();
        recorder.fill_rounded_rect(shape, fill);
        if border {
            let width = f32::from(cx.shape.line_width.max(1));
            recorder.stroke_rounded_rect(
                RoundedRect::new(rect.inflate(-width / 2.0), (radius - width / 2.0).max(0.0)),
                StrokeStyle::new(width),
                ColorToken::ControlBorder,
            );
        }
        // The text centred in the button.
        let x = rect.x0 + (rect.width() - self.layout.width()) / 2.0;
        let y = rect.y0 + (rect.height() - self.layout.height()) / 2.0;
        recorder.text(&self.layout, Point::new(x, y), text);
        if cx.focused && self.enabled {
            super::look::focus_ring(recorder, rect, radius, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        if !self.enabled {
            return Handled::No;
        }
        match *event {
            WidgetEvent::PointerEntered | WidgetEvent::PointerLeft => {
                self.hovered = matches!(event, WidgetEvent::PointerEntered);
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                ..
            } => {
                self.pressed = true;
                cx.request_record();
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                position,
                ..
            } => {
                if self.pressed {
                    self.pressed = false;
                    cx.request_record();
                    if contains(cx.rect(), position) {
                        self.activate(cx);
                    }
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
                self.activate(cx);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::Button
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.text.clone());
        info.disabled = !self.enabled;
        info.clickable = true;
    }
}
