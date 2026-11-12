// SPDX-License-Identifier: GPL-3.0-or-later

//! The slider (F2.09c, choice WD3): a value between a minimum and a maximum, by steps.
//!
//! # Behaviour
//! - **Pointer**: a press on the slider moves the value under the pointer and captures it
//!   (ADR-0045 WA3); dragging follows the pointer, also outside the slider, until the release.
//! - **Keyboard**, when focused: the arrows move one step (right and up increase), Page Up and
//!   Page Down ten steps, Home and End to the minimum and the maximum.
//! - **Assistive technologies**: increase, decrease (one step) and set the value (AccessKit's
//!   actions, which the slider receives as events).
//!
//! The value is always on the grid `minimum + k × step` and inside the range. Each change emits
//! the message the application builds from the new value (`fn(f64) -> M`); grouping a drag into
//! one undo step (ADR-0026's gestures) comes when a slider changes the document.
//!
//! # Looks
//! A track 4 px high across the slider (`control-border`, the part before the value `accent`)
//! and a round thumb (`surface-raised` with a `control-border` line, `accent` line while
//! dragged), as high as half the control. Disabled: `control` and `text-disabled`. The focus ring
//! surrounds the thumb.

use crate::draw::geometry::{Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, Key, NamedKey, PointerButton};

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

/// A horizontal slider (WD3).
#[derive(Debug)]
pub struct Slider<M> {
    /// The smallest value.
    min: f64,
    /// The largest value (≥ `min`).
    max: f64,
    /// The step (> 0).
    step: f64,
    /// The value shown.
    value: f64,
    /// The message built from the new value at each change.
    message: fn(f64) -> M,
    /// Whether it can be used.
    enabled: bool,
    /// A drag is in progress (the pointer is captured).
    dragging: bool,
    /// The name assistive technologies announce: the text of the label shown next to it (a
    /// slider has no text of its own).
    name: Option<String>,
}

/// The width a slider asks for when its parent does not size it, logical pixels.
const PREFERRED_WIDTH: f32 = 160.0;

impl<M> Slider<M> {
    /// A slider from `min` to `max` by `step`, showing `value`. A range given backwards is put
    /// right; a step that is not positive and finite becomes the whole range (two positions).
    #[must_use]
    pub fn new(min: f64, max: f64, step: f64, value: f64, message: fn(f64) -> M) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let span = max - min;
        let step = if step > 0.0 && step.is_finite() {
            step
        } else if span > 0.0 {
            span
        } else {
            1.0
        };
        let mut slider = Self {
            min,
            max,
            step,
            value: min,
            message,
            enabled: true,
            dragging: false,
            name: None,
        };
        slider.value = slider.snap(value);
        slider
    }

    /// The same slider, named for assistive technologies with the text of the label shown next to
    /// it (already localised).
    #[must_use]
    pub fn labelled(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The value shown.
    #[must_use]
    pub const fn value(&self) -> f64 {
        self.value
    }

    /// Shows `value` (snapped to the grid); the caller asks the interface to record it again.
    pub fn set_value(&mut self, value: f64) {
        self.value = self.snap(value);
    }

    /// Enables or disables the slider; the caller asks the interface to record it again.
    pub const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.dragging = false;
        }
    }

    /// `value` on the grid `min + k × step` and inside the range; NaN becomes the minimum.
    fn snap(&self, value: f64) -> f64 {
        if value.is_nan() {
            return self.min;
        }
        let steps = ((value - self.min) / self.step).round();
        (self.min + steps * self.step).clamp(self.min, self.max)
    }

    /// Moves to `value` (snapped); tells the application and asks for a redraw only if it
    /// changed.
    fn change(&mut self, cx: &mut EventCx<'_, M>, value: f64) {
        let value = self.snap(value);
        if value.to_bits() != self.value.to_bits() {
            self.value = value;
            cx.emit((self.message)(value));
            cx.request_record();
        }
    }

    /// The thumb's diameter for a control of height `control`.
    fn thumb(control: f32) -> f32 {
        (control / 2.0).round().max(8.0)
    }

    /// Where the value's position is along the slider in `rect`: the thumb's centre, between
    /// half a thumb from each end.
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a fraction in 0..=1 of a width in pixels: f32 is plenty"
    )]
    fn position(&self, rect: Rect, control: f32) -> f32 {
        let radius = Self::thumb(control) / 2.0;
        let span = self.max - self.min;
        let fraction = if span > 0.0 {
            ((self.value - self.min) / span) as f32
        } else {
            0.0
        };
        rect.x0 + radius + fraction * (rect.width() - 2.0 * radius).max(0.0)
    }

    /// The value under the pointer's `x` in a slider laid out in `rect` (before snapping).
    fn value_at(&self, rect: Rect, control: f32, x: f32) -> f64 {
        let radius = Self::thumb(control) / 2.0;
        let usable = (rect.width() - 2.0 * radius).max(1.0);
        let fraction = f64::from(((x - rect.x0 - radius) / usable).clamp(0.0, 1.0));
        self.min + fraction * (self.max - self.min)
    }
}

impl<M: std::fmt::Debug + 'static> Widget<M> for Slider<M> {
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let spacing = cx.spacing();
        let width = if constraints.max.width.is_finite() {
            PREFERRED_WIDTH.min(constraints.max.width)
        } else {
            PREFERRED_WIDTH
        };
        constraints.constrain(Size::new(width, spacing.control_height))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let control = cx.spacing.control_height;
        let diameter = Self::thumb(control);
        let centre_y = rect.y0 + rect.height() / 2.0;
        // The thumb's centre on a quarter of a logical pixel: positions repeat, so the thumb's
        // coverage comes from the shape cache instead of being rasterised at every move
        // (F2.09c, measured: a drag move went from 0.55 M to 0.22 M instructions).
        let x = (self.position(rect, control) * 4.0).round() / 4.0;
        // The track, 4 px high, with rounded ends; the part before the value in the accent.
        let track = Rect::new(
            rect.x0 + diameter / 2.0,
            centre_y - 2.0,
            rect.x1 - diameter / 2.0,
            centre_y + 2.0,
        );
        recorder.fill_rounded_rect(RoundedRect::new(track, 2.0), ColorToken::ControlBorder);
        if self.enabled && x > track.x0 {
            // The part before the value: a rounded cap of fixed size (always the same shape, so
            // cached) and a plain rectangle up to the thumb, which covers its end. A rounded
            // rectangle of the varying width would be a new shape at every move.
            let cap = Rect::new(track.x0, track.y0, track.x0 + 4.0, track.y1);
            recorder.fill_rounded_rect(RoundedRect::new(cap, 2.0), ColorToken::Accent);
            if x > track.x0 + 2.0 {
                recorder.fill_rect(
                    Rect::new(track.x0 + 2.0, track.y0, x, track.y1),
                    ColorToken::Accent,
                );
            }
        }
        // The thumb.
        let thumb = Rect::from_origin_size(
            x - diameter / 2.0,
            centre_y - diameter / 2.0,
            diameter,
            diameter,
        );
        let shape = RoundedRect::new(thumb, diameter / 2.0);
        let (fill, line) = match (self.enabled, self.dragging) {
            (false, _) => (ColorToken::Control, ColorToken::TextDisabled),
            (true, true) => (ColorToken::SurfaceRaised, ColorToken::Accent),
            (true, false) => (ColorToken::SurfaceRaised, ColorToken::ControlBorder),
        };
        recorder.fill_rounded_rect(shape, fill);
        recorder.stroke_rounded_rect(
            RoundedRect::new(thumb.inflate(-0.5), diameter / 2.0 - 0.5),
            StrokeStyle::new(1.0),
            line,
        );
        if cx.focused && self.enabled {
            super::look::focus_ring(recorder, thumb, diameter / 2.0, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        if !self.enabled {
            return Handled::No;
        }
        // The control height is not in the event context; the slider's own height is it (its
        // measure answers the control height).
        let rect = cx.rect();
        let control = rect.height();
        match *event {
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                position,
            } => {
                self.dragging = true;
                cx.request_record();
                self.change(cx, self.value_at(rect, control, position.x));
                Handled::Yes
            }
            WidgetEvent::PointerMoved { position } => {
                if self.dragging {
                    self.change(cx, self.value_at(rect, control, position.x));
                }
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                ..
            } => {
                if self.dragging {
                    self.dragging = false;
                    cx.request_record();
                }
                Handled::Yes
            }
            WidgetEvent::Key {
                key: Key::Named(key),
                state: ButtonState::Pressed,
                ..
            } => {
                let target = match key {
                    NamedKey::ArrowRight | NamedKey::ArrowUp => self.value + self.step,
                    NamedKey::ArrowLeft | NamedKey::ArrowDown => self.value - self.step,
                    NamedKey::PageUp => self.value + 10.0 * self.step,
                    NamedKey::PageDown => self.value - 10.0 * self.step,
                    NamedKey::Home => self.min,
                    NamedKey::End => self.max,
                    _ => return Handled::No,
                };
                self.change(cx, target);
                Handled::Yes
            }
            WidgetEvent::Increment => {
                self.change(cx, self.value + self.step);
                Handled::Yes
            }
            WidgetEvent::Decrement => {
                self.change(cx, self.value - self.step);
                Handled::Yes
            }
            WidgetEvent::SetValue(value) => {
                self.change(cx, value);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::Slider
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label.clone_from(&self.name);
        info.value = Some(self.value);
        info.range = Some((self.min, self.max, self.step));
        info.disabled = !self.enabled;
    }
}
