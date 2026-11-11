// SPDX-License-Identifier: GPL-3.0-or-later

//! The demonstration window of ADR-0040 W8: what `cargo run -p orchestra-app` shows until the
//! real interface arrives with the widgets (F2.08 onwards).
//!
//! It draws with the commands of ADR-0039 only (text comes in F2.07) and exists to check by eye:
//! - **presentation and damage:** a disc follows the pointer, re-recording one segment per move,
//!   so only its old and new place are redrawn;
//! - **input:** the disc turns to the accent colour while a button is held; four squares light up
//!   with the primary modifier, the secondary one, Shift and Alt;
//! - **crisp lines and scale:** a staff of five 1 px lines with noteheads, which must stay sharp at
//!   every scale factor;
//! - **following the system's theme:** switching the desktop between light and dark while the
//!   window is open redraws it in the other half of the pair (ADR-0038, F2.04b);
//! - **shapes and the theme:** a raised panel with its shadow and three controls in the theme's
//!   colours, at the window's size (resizing lays it out again).
//!
//! Nothing here is business logic (the crate's rule): it is a throw-away view, replaced as soon as
//! real views exist.

use orchestra_ui::{
    ButtonState, ColorToken, DisplayList, FillRule, InputEvent, Modifiers, Paint, Point, Rect,
    RoundedRect, SegmentId, StrokeStyle, WindowApp,
};

/// The panel with its shadow and controls.
const PANEL: SegmentId = SegmentId(1);
/// The staff with its noteheads.
const STAFF: SegmentId = SegmentId(2);
/// The four modifier squares.
const MODIFIERS: SegmentId = SegmentId(3);
/// The disc under the pointer, drawn last so it is on top.
const POINTER: SegmentId = SegmentId(4);

/// Margin around the panel, logical pixels.
const MARGIN: f32 = 24.0;
/// Radius of the pointer's disc, logical pixels.
const DISC: f32 = 14.0;

/// The demonstration view's state.
#[derive(Debug, Default)]
pub struct Demo {
    /// The window's size, logical pixels.
    bounds: Rect,
    /// Where the pointer is, if it is inside the window.
    pointer: Option<Point>,
    /// Whether a pointer button is held.
    pressed: bool,
    /// The modifiers held.
    modifiers: Modifiers,
}

impl Demo {
    /// The panel: a raised surface with a shadow, and three controls. Sized to the window, up to
    /// 520 × 260.
    fn record_panel(&self, list: &mut DisplayList) {
        let width = (self.bounds.width() - 2.0 * MARGIN).clamp(0.0, 520.0);
        let height = (self.bounds.height() - 2.0 * MARGIN).clamp(0.0, 260.0);
        let panel = RoundedRect::new(Rect::from_origin_size(MARGIN, MARGIN, width, height), 10.0);
        list.record(PANEL, |r| {
            // The shadow, offset downwards as ADR-0037's shadow tokens describe.
            let shadow = RoundedRect::new(
                Rect::from_origin_size(MARGIN, MARGIN + 4.0, width, height),
                10.0,
            );
            r.shadow(shadow, 8.0, ColorToken::Shadow);
            r.fill_rounded_rect(panel, ColorToken::SurfaceRaised);
            r.stroke_rounded_rect(panel, StrokeStyle::new(1.0), ColorToken::Border);
            // Three controls: a plain one, a focused one, the accent one.
            for (index, token) in
                (0_u8..).zip([ColorToken::Control, ColorToken::Control, ColorToken::Accent])
            {
                let x = MARGIN + 20.0 + f32::from(index) * 110.0;
                let control =
                    RoundedRect::new(Rect::from_origin_size(x, MARGIN + 20.0, 96.0, 32.0), 6.0);
                r.fill_rounded_rect(control, token);
                r.stroke_rounded_rect(control, StrokeStyle::new(1.0), ColorToken::ControlBorder);
                if index == 1 {
                    let ring = RoundedRect::new(control.rect.inflate(3.0), 9.0);
                    r.stroke_rounded_rect(ring, StrokeStyle::new(2.0), ColorToken::Focus);
                }
            }
        });
    }

    /// A staff of five crisp lines with a few noteheads and stems, inside the panel.
    fn record_staff(list: &mut DisplayList) {
        let left = MARGIN + 20.0;
        let top = MARGIN + 90.0;
        let space = 10.0;
        list.record(STAFF, |r| {
            r.push_clip_rect(Rect::from_origin_size(left, top - 40.0, 480.0, 120.0));
            for line in 0..5_u8 {
                let y = top + f32::from(line) * space;
                r.line(
                    Point::new(left, y),
                    Point::new(left + 440.0, y),
                    StrokeStyle::new(1.0),
                    ColorToken::Text,
                );
            }
            for note in 0..12_u8 {
                let x = left + 30.0 + f32::from(note) * 34.0;
                let y = top + 4.0 * space - f32::from(note % 9) * space * 0.5;
                r.fill_path(FillRule::NonZero, ColorToken::Text, |p| {
                    // An ellipse of 6 × 4.5 px: four cubics with the quarter-circle constant.
                    let (a, b) = (6.0, 4.5);
                    let (ka, kb) = (a * 0.552_284_8, b * 0.552_284_8);
                    p.move_to(x + a, y);
                    p.cubic_to(x + a, y + kb, x + ka, y + b, x, y + b);
                    p.cubic_to(x - ka, y + b, x - a, y + kb, x - a, y);
                    p.cubic_to(x - a, y - kb, x - ka, y - b, x, y - b);
                    p.cubic_to(x + ka, y - b, x + a, y - kb, x + a, y);
                    p.close();
                });
                r.line(
                    Point::new(x + 5.5, y),
                    Point::new(x + 5.5, y - 3.5 * space),
                    StrokeStyle::new(1.0),
                    ColorToken::Text,
                );
            }
            r.pop_clip();
        });
    }

    /// Four squares: primary, secondary, Shift, Alt; filled with the accent while held.
    fn record_modifiers(&self, list: &mut DisplayList) {
        let held = [
            self.modifiers.primary,
            self.modifiers.secondary,
            self.modifiers.shift,
            self.modifiers.alt,
        ];
        list.record(MODIFIERS, |r| {
            for (index, on) in (0_u8..).zip(held) {
                let square = RoundedRect::new(
                    Rect::from_origin_size(
                        MARGIN + 20.0 + f32::from(index) * 36.0,
                        MARGIN + 200.0,
                        24.0,
                        24.0,
                    ),
                    4.0,
                );
                if on {
                    r.fill_rounded_rect(square, ColorToken::Accent);
                }
                r.stroke_rounded_rect(square, StrokeStyle::new(1.0), ColorToken::ControlBorder);
            }
        });
    }

    /// The disc under the pointer (nothing when the pointer is outside).
    fn record_pointer(&self, list: &mut DisplayList) {
        let pointer = self.pointer;
        let paint = if self.pressed {
            Paint::token(ColorToken::Accent)
        } else {
            Paint::with_opacity(ColorToken::Text, 160)
        };
        list.record(POINTER, |r| {
            if let Some(at) = pointer {
                let disc = Rect::new(at.x - DISC, at.y - DISC, at.x + DISC, at.y + DISC);
                r.fill_rounded_rect(RoundedRect::new(disc, DISC), paint);
            }
        });
    }
}

impl WindowApp for Demo {
    /// Records everything for the window's new size.
    fn layout(&mut self, bounds: Rect, list: &mut DisplayList) {
        self.bounds = bounds;
        self.record_panel(list);
        Self::record_staff(list);
        self.record_modifiers(list);
        self.record_pointer(list);
    }

    /// Re-records only what the event changes: the disc, or the modifier squares.
    fn input(&mut self, event: &InputEvent, list: &mut DisplayList) {
        match event {
            InputEvent::PointerMoved { position } => {
                self.pointer = Some(*position);
                self.record_pointer(list);
            }
            InputEvent::PointerLeft => {
                self.pointer = None;
                self.record_pointer(list);
            }
            InputEvent::PointerButton { state, .. } => {
                self.pressed = *state == ButtonState::Pressed;
                self.record_pointer(list);
            }
            InputEvent::ModifiersChanged { modifiers } | InputEvent::Key { modifiers, .. } => {
                if *modifiers != self.modifiers {
                    self.modifiers = *modifiers;
                    self.record_modifiers(list);
                }
            }
            InputEvent::Wheel { .. } | InputEvent::Focus(_) => {}
        }
    }
}
