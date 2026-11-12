// SPDX-License-Identifier: GPL-3.0-or-later

//! The check box and the switch (F2.09c, choices WD1–WD2): two-state controls with their label.
//!
//! Both behave the same and differ only in their look, so one type, [`Toggle`], draws either
//! (`Toggle::checkbox`, `Toggle::switch`).
//!
//! # Behaviour
//! The label is part of the control: a click anywhere on it toggles, as on every desktop. A
//! press shows nothing special and the release over the control toggles (dragging out cancels,
//! as for buttons); Space toggles when focused; AccessKit's click too. Each change emits the
//! message the application builds from the new state (`fn(bool) -> M`): the application, not
//! the widget, owns the setting, so the widget's state is what it shows (the application can set
//! it back with [`Toggle::set_on`]).
//!
//! # Looks
//! The box (or the switch's track) is `4/7` of the control height, beside the label with a small
//! gap (the density, WB10). A check box is `control` with a `control-border` line, `accent` with
//! an `on-accent` check mark when on; a switch is a pill, `control` with a `control-border` knob
//! when off, `accent` with an `on-accent` knob on the right when on. Disabled: `control` and
//! `text-disabled`. The focus ring surrounds the box or the track, not the label.

use orchestra_text::TextLayout;

use crate::draw::geometry::{Point, Rect, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::{LineCap, LineJoin, StrokeStyle};
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, Key, NamedKey, PointerButton};

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

/// The two looks of a two-state control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleKind {
    /// A box with a check mark.
    CheckBox,
    /// A sliding switch.
    Switch,
}

/// A check box or a switch with its label (WD1, WD2).
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent states (on, enabled, hovered, pressed), each a plain yes or no; an \
              enum of their combinations would be harder to read"
)]
pub struct Toggle<M> {
    /// Box or switch.
    kind: ToggleKind,
    /// The label, already localised.
    text: String,
    /// On or off, as shown.
    on: bool,
    /// The message built from the new state at each change.
    message: fn(bool) -> M,
    /// Whether it can be used.
    enabled: bool,
    /// The pointer is over it.
    hovered: bool,
    /// A press inside is held.
    pressed: bool,
    /// The laid-out label.
    layout: TextLayout,
    /// The text generation the label was laid out with.
    laid_out: Option<u64>,
}

impl<M> Toggle<M> {
    /// A check box.
    #[must_use]
    pub fn checkbox(text: impl Into<String>, on: bool, message: fn(bool) -> M) -> Self {
        Self::new(ToggleKind::CheckBox, text.into(), on, message)
    }

    /// A switch.
    #[must_use]
    pub fn switch(text: impl Into<String>, on: bool, message: fn(bool) -> M) -> Self {
        Self::new(ToggleKind::Switch, text.into(), on, message)
    }

    /// A control of `kind`.
    fn new(kind: ToggleKind, text: String, on: bool, message: fn(bool) -> M) -> Self {
        Self {
            kind,
            text,
            on,
            message,
            enabled: true,
            hovered: false,
            pressed: false,
            layout: TextLayout::new(),
            laid_out: None,
        }
    }

    /// On or off, as shown.
    #[must_use]
    pub const fn is_on(&self) -> bool {
        self.on
    }

    /// Shows `on` (the application's setting); the caller asks the interface to record it again.
    pub const fn set_on(&mut self, on: bool) {
        self.on = on;
    }

    /// Enables or disables the control; the caller asks the interface to record it again.
    pub const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.pressed = false;
        }
    }

    /// Changes the state and tells the application.
    fn toggle(&mut self, cx: &mut EventCx<'_, M>) {
        self.on = !self.on;
        cx.emit((self.message)(self.on));
        cx.request_record();
    }

    /// The box's (or track's) height for a control of height `control`: 4/7 of it (16 px in the
    /// comfortable density, 12.6 in the compact one).
    fn indicator_height(control: f32) -> f32 {
        (control * 4.0 / 7.0).round()
    }

    /// The indicator's width: square for a box, twice as wide for a switch.
    fn indicator_width(&self, height: f32) -> f32 {
        match self.kind {
            ToggleKind::CheckBox => height,
            ToggleKind::Switch => 2.0 * height,
        }
    }

    /// The indicator's rectangle in a control laid out in `rect`.
    fn indicator(&self, rect: Rect, control: f32) -> Rect {
        let height = Self::indicator_height(control);
        let width = self.indicator_width(height);
        Rect::from_origin_size(
            rect.x0,
            rect.y0 + (rect.height() - height) / 2.0,
            width,
            height,
        )
    }

    /// Draws a check box's box and mark.
    fn record_box(&self, recorder: &mut Recorder<'_>, rect: Rect, radius: f32) {
        let shape = RoundedRect::new(rect, radius);
        let fill = match (self.enabled, self.on, self.hovered || self.pressed) {
            (true, true, _) => ColorToken::Accent,
            (true, false, true) => ColorToken::ControlHover,
            _ => ColorToken::Control,
        };
        recorder.fill_rounded_rect(shape, fill);
        if !(self.on && self.enabled) {
            recorder.stroke_rounded_rect(
                RoundedRect::new(rect.inflate(-0.5), (radius - 0.5).max(0.0)),
                StrokeStyle::new(1.0),
                ColorToken::ControlBorder,
            );
        }
        if self.on {
            // A check mark from left middle down to the bottom third, then up to the top right.
            let (w, h) = (rect.width(), rect.height());
            let mark = if self.enabled {
                ColorToken::OnAccent
            } else {
                ColorToken::TextDisabled
            };
            let style = StrokeStyle {
                cap: LineCap::Round,
                join: LineJoin::Round,
                ..StrokeStyle::new((h / 8.0).max(1.5))
            };
            recorder.stroke_path(style, mark, |path| {
                path.move_to(rect.x0 + 0.25 * w, rect.y0 + 0.52 * h);
                path.line_to(rect.x0 + 0.43 * w, rect.y0 + 0.70 * h);
                path.line_to(rect.x0 + 0.76 * w, rect.y0 + 0.32 * h);
            });
        }
    }

    /// Draws a switch's track and knob.
    fn record_switch(&self, recorder: &mut Recorder<'_>, rect: Rect) {
        let radius = rect.height() / 2.0;
        let track = RoundedRect::new(rect, radius);
        let (fill, knob) = match (self.enabled, self.on) {
            (true, true) => (ColorToken::Accent, ColorToken::OnAccent),
            (true, false) => (
                if self.hovered || self.pressed {
                    ColorToken::ControlHover
                } else {
                    ColorToken::Control
                },
                ColorToken::ControlBorder,
            ),
            (false, _) => (ColorToken::Control, ColorToken::TextDisabled),
        };
        recorder.fill_rounded_rect(track, fill);
        if !(self.on && self.enabled) {
            recorder.stroke_rounded_rect(
                RoundedRect::new(rect.inflate(-0.5), radius - 0.5),
                StrokeStyle::new(1.0),
                ColorToken::ControlBorder,
            );
        }
        // The knob: a disc 3 px inside the track, at the right when on.
        let diameter = rect.height() - 6.0;
        let x = if self.on {
            rect.x1 - 3.0 - diameter
        } else {
            rect.x0 + 3.0
        };
        let knob_rect = Rect::from_origin_size(x, rect.y0 + 3.0, diameter, diameter);
        recorder.fill_rounded_rect(RoundedRect::new(knob_rect, diameter / 2.0), knob);
    }
}

impl<M: std::fmt::Debug + 'static> Widget<M> for Toggle<M> {
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
        let indicator = self.indicator_width(Self::indicator_height(spacing.control_height));
        constraints.constrain(Size::new(
            indicator + spacing.gap_s + self.layout.width(),
            spacing.control_height.max(self.layout.height()),
        ))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let indicator = self.indicator(cx.rect, cx.spacing.control_height);
        let radius = match self.kind {
            ToggleKind::CheckBox => f32::from(cx.shape.corner_radius).min(4.0),
            ToggleKind::Switch => indicator.height() / 2.0,
        };
        match self.kind {
            ToggleKind::CheckBox => self.record_box(recorder, indicator, radius),
            ToggleKind::Switch => self.record_switch(recorder, indicator),
        }
        let text = if self.enabled {
            ColorToken::Text
        } else {
            ColorToken::TextDisabled
        };
        let x = indicator.x1 + cx.spacing.gap_s;
        let y = cx.rect.y0 + (cx.rect.height() - self.layout.height()) / 2.0;
        recorder.text(&self.layout, Point::new(x, y), text);
        if cx.focused && self.enabled {
            super::look::focus_ring(recorder, indicator, radius, cx.shape);
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
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Released,
                position,
            } => {
                let rect = cx.rect();
                let inside = position.x >= rect.x0
                    && position.x < rect.x1
                    && position.y >= rect.y0
                    && position.y < rect.y1;
                if self.pressed && inside {
                    self.toggle(cx);
                }
                self.pressed = false;
                Handled::Yes
            }
            WidgetEvent::PointerMoved { .. } => Handled::Yes,
            WidgetEvent::Key {
                key: Key::Named(NamedKey::Space),
                state: ButtonState::Pressed,
                repeat: false,
                ..
            }
            | WidgetEvent::Activate => {
                self.toggle(cx);
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        self.enabled
    }

    fn access_role(&self) -> AccessRole {
        match self.kind {
            ToggleKind::CheckBox => AccessRole::CheckBox,
            ToggleKind::Switch => AccessRole::Switch,
        }
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.text.clone());
        info.toggled = Some(self.on);
        info.disabled = !self.enabled;
        info.clickable = true;
    }
}
