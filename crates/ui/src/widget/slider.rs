// SPDX-License-Identifier: GPL-3.0-or-later

//! The slider (F2.09c, choice WD3): a value between a minimum and a maximum, by steps.
//!
//! # Behaviour (the value-control conventions of ADR-0046, shared by every slider and knob)
//! - **Pointer**: a press on the slider moves the value under the pointer and captures it
//!   (ADR-0045 WA3); dragging follows the pointer, also outside the slider, until the release.
//!   With the primary modifier held (Ctrl, ⌘ on macOS) the drag is **fine**: the value moves a
//!   tenth as fast and the press does not jump to the pointer (V3). Pressing or releasing the
//!   modifier during a drag re-anchors it where the pointer is, so the value never jumps.
//! - **Double click**: back to the default value (V4), the initial value unless the application
//!   gives another with [`Slider::default_value`].
//! - **Wheel**, with the pointer over the slider (no focus needed, V2): each notch moves the
//!   *coarse step* (2% of the range, at least one step), one step with the primary modifier;
//!   away from the user increases. A touchpad's pixels count 40 to a notch, the remainder kept for
//!   the next event. With Shift held the slider ignores the wheel, which goes on to the scroll
//!   area around it (V5).
//! - **Keyboard**, when focused: the arrows move the coarse step (one step with the primary
//!   modifier; right and up increase), Page Up and Page Down ten steps, Home and End to the
//!   minimum and the maximum.
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
use crate::window::input::{ButtonState, Key, NamedKey, PointerButton, WheelDelta};

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
    /// The value a double click goes back to (V4), on the grid.
    default: f64,
    /// The drag in progress (the pointer is captured), if any.
    drag: Option<Drag>,
    /// The wheel's movement not yet turned into notches, in notches (a fraction, signed): a
    /// touchpad sends a few pixels at a time, and a notch is 40 of them (V2).
    wheel_rest: f32,
    /// The name assistive technologies announce: the text of the label shown next to it (a
    /// slider has no text of its own).
    name: Option<String>,
}

/// The width a slider asks for when its parent does not size it, logical pixels.
const PREFERRED_WIDTH: f32 = 160.0;

/// A touchpad's (or a smooth wheel's) pixels that make one wheel notch (ADR-0046 V2): about what
/// one line scrolls, so a touchpad swipe moves the value like a few clicks of a wheel.
const PIXELS_PER_NOTCH: f32 = 40.0;

/// The coarse step as a fraction of the range (ADR-0046 V2): 2%, so fifty notches go from one end
/// to the other whatever the range and the step.
const COARSE_FRACTION: f64 = 0.02;

/// How much slower a fine drag is (ADR-0046 V3).
const FINE_DRAG: f64 = 0.1;

/// A drag in progress: the value follows the pointer relatively to an anchor, set at the press
/// and again whenever the fine modifier changes, so switching speed never makes the value jump.
#[derive(Debug, Clone, Copy)]
struct Drag {
    /// The pointer's x at the anchor, logical pixels.
    x: f32,
    /// The value at the anchor, before snapping (a fine drag moves less than a step per pixel,
    /// and the snapped value alone would lose those fractions).
    value: f64,
    /// Whether this stretch is fine (the primary modifier was held at the anchor).
    fine: bool,
    /// The value before snapping at the last move, the next anchor's value.
    raw: f64,
    /// The pointer's x at the last move, the next anchor's x: anchoring at the move that carries
    /// the new modifier would lose that move's distance.
    last_x: f32,
}

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
            default: min,
            drag: None,
            wheel_rest: 0.0,
            name: None,
        };
        slider.value = slider.snap(value);
        slider.default = slider.value;
        slider
    }

    /// The same slider, going back to `value` (snapped) on a double click instead of its initial
    /// value (ADR-0046 V4).
    #[must_use]
    pub fn default_value(mut self, value: f64) -> Self {
        self.default = self.snap(value);
        self
    }

    /// The value a double click goes back to.
    #[must_use]
    pub const fn default(&self) -> f64 {
        self.default
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
            self.drag = None;
        }
    }

    /// The coarse step (V2): 2% of the range rounded to whole steps, at least one step.
    fn coarse_step(&self) -> f64 {
        let steps = ((self.max - self.min) * COARSE_FRACTION / self.step)
            .round()
            .max(1.0);
        steps * self.step
    }

    /// The step of a wheel notch or an arrow key: the coarse step, or one step when the primary
    /// modifier asks for the fine adjustment (V3).
    fn notch(&self, fine: bool) -> f64 {
        if fine { self.step } else { self.coarse_step() }
    }

    /// How much the value moves per logical pixel of a drag in `rect`: the range over the track's
    /// usable width (between half a thumb from each end), a tenth of it when fine.
    fn per_pixel(&self, rect: Rect, control: f32, fine: bool) -> f64 {
        let usable = (rect.width() - Self::thumb(control)).max(1.0);
        let speed = if fine { FINE_DRAG } else { 1.0 };
        (self.max - self.min) / f64::from(usable) * speed
    }

    /// Turns a wheel movement into whole notches, keeping the remainder for the next event: lines
    /// are notches, pixels count [`PIXELS_PER_NOTCH`] to a notch. Away from the user is positive.
    fn notches(&mut self, delta: WheelDelta) -> f32 {
        let moved = match delta {
            WheelDelta::Lines { y, .. } => y,
            WheelDelta::Pixels { y, .. } => y / PIXELS_PER_NOTCH,
        };
        // A movement the other way drops what was left: a reversed swipe acts at once.
        if moved.signum() != self.wheel_rest.signum() {
            self.wheel_rest = 0.0;
        }
        self.wheel_rest += moved;
        let whole = self.wheel_rest.trunc();
        self.wheel_rest -= whole;
        whole
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

    /// A left press at `x` in a slider laid out in `rect`: the second of a double click restores
    /// the default (V4); otherwise a drag starts, jumping to the pointer unless fine (V3).
    fn press(&mut self, cx: &mut EventCx<'_, M>, rect: Rect, control: f32, x: f32, clicks: u8) {
        if clicks == 2 {
            // A double click: back to the default (V4). The first click of the pair already
            // moved the value and its release ended that drag; this press starts no drag (it
            // returns before creating one), so a slight move before the release does not undo
            // the reset.
            self.change(cx, self.default);
            return;
        }
        let fine = cx.modifiers().primary;
        if !fine {
            // A plain press jumps to the pointer; a fine one keeps the value (V3).
            self.change(cx, self.value_at(rect, control, x));
        }
        self.drag = Some(Drag {
            x,
            value: self.value,
            fine,
            raw: self.value,
            last_x: x,
        });
        cx.request_record();
    }

    /// The pointer moved to `x` during a drag (nothing otherwise): the value follows relatively
    /// to the anchor, at the plain or the fine speed (V3).
    fn drag_to(&mut self, cx: &mut EventCx<'_, M>, rect: Rect, control: f32, x: f32) {
        let Some(mut drag) = self.drag else {
            return;
        };
        let fine = cx.modifiers().primary;
        if fine != drag.fine {
            // The modifier changed: anchor again where the pointer was at the last move, at the
            // value reached, so the new speed applies from there on (no jump, and this move
            // counts at the new speed). Clamped, so a drag far past an end comes back at once.
            drag = Drag {
                x: drag.last_x,
                value: drag.raw.clamp(self.min, self.max),
                fine,
                raw: drag.raw,
                last_x: drag.last_x,
            };
        }
        let moved = f64::from(x - drag.x);
        drag.raw = drag.value + moved * self.per_pixel(rect, control, fine);
        drag.last_x = x;
        self.drag = Some(drag);
        self.change(cx, drag.raw);
    }

    /// The wheel over the slider (V2): whole notches move the coarse step (one step when fine);
    /// with Shift it is left to the scroll area around (V5).
    fn wheel(&mut self, cx: &mut EventCx<'_, M>, delta: WheelDelta) -> Handled {
        let modifiers = cx.modifiers();
        if modifiers.shift {
            // Shift+wheel scrolls the area around the slider (V5).
            return Handled::No;
        }
        let notches = self.notches(delta);
        if notches != 0.0 {
            let target = self.value + f64::from(notches) * self.notch(modifiers.primary);
            self.change(cx, target);
        }
        // Handled even at the ends of the range: the area around must not scroll under a slider
        // the user is turning.
        Handled::Yes
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
        let (fill, line) = match (self.enabled, self.drag.is_some()) {
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
                clicks,
            } => {
                self.press(cx, rect, control, position.x, clicks);
                Handled::Yes
            }
            WidgetEvent::PointerMoved { position } => {
                self.drag_to(cx, rect, control, position.x);
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                ..
            } => {
                if self.drag.take().is_some() {
                    cx.request_record();
                }
                Handled::Yes
            }
            WidgetEvent::Wheel { delta, .. } => self.wheel(cx, delta),
            WidgetEvent::Key {
                key: Key::Named(key),
                state: ButtonState::Pressed,
                modifiers,
                ..
            } => {
                let notch = self.notch(modifiers.primary);
                let target = match key {
                    NamedKey::ArrowRight | NamedKey::ArrowUp => self.value + notch,
                    NamedKey::ArrowLeft | NamedKey::ArrowDown => self.value - notch,
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
