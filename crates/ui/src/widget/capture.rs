// SPDX-License-Identifier: GPL-3.0-or-later

//! The capture of a new shortcut (ADR-0048 AM7, SCALETTA F2.12c1): a box that says "Press the
//! shortcut…" and records the next key pressed with the modifiers held.
//!
//! # What it does with each key
//! - **A modifier alone** (Ctrl, Shift, ⌘, Alt…: they arrive as unknown keys, or as
//!   [`NamedKey::Alt`]) is waited past: the user is building the combination.
//! - **Escape** cancels, whatever the modifiers: the message of [`ShortcutCapture::new`]'s
//!   `on_cancel`.
//! - **Tab** is left alone, so the focus moves on as everywhere (ADR-0045 WA4, the keyboard user
//!   must be able to leave); leaving the box cancels.
//! - **A fixed key** (AM8: the widgets' navigation, Enter, Space, the menus' keys, Alt+letter on
//!   Linux and Windows, macOS's system shortcuts) is refused: `on_refused` with the shortcut, so
//!   the application can say why; the box keeps waiting.
//! - **Anything else** is the shortcut: `on_capture`. The key is read as the actions' lookup
//!   reads it ([`candidates`]: the logical key first, the keypad's digits apart), so what is
//!   recorded is what will match later. Repeats and releases are swallowed.
//!
//! Since the focused widget sees a key before the actions do (ADR-0048 AM4), even the
//! Preferences' own shortcut (Ctrl+,) is captured here instead of closing the dialog.
//!
//! # Looks and accessibility
//! A text field's box (`surface-sunken`, `control-border`, the focus ring) with the prompt in
//! `text-muted`. For assistive technologies it is a text input named by the application (for
//! example "New shortcut for Open"), with the prompt as its placeholder: it is where the keyboard
//! goes, and nothing else fits better among AccessKit's roles.
//!
//! # Cost
//! The prompt is laid out once per text generation. Events allocate nothing; the messages are
//! plain function pointers (ADR-0045 WA7).

use orchestra_text::TextLayout;

use crate::action::shortcut::{Platform, Shortcut, candidates};
use crate::draw::geometry::{Point, RoundedRect};
use crate::draw::list::Recorder;
use crate::draw::paint::StrokeStyle;
use crate::theme::text::{FamilyToken, SizeToken, Weight};
use crate::theme::token::ColorToken;
use crate::window::input::{ButtonState, Key, NamedKey, PointerButton};

use super::access::{AccessInfo, AccessRole};
use super::layout::{Constraints, Size};
use super::tree::{EventCx, Handled, LayoutCx, RecordCx, Widget, WidgetEvent};

/// The prompt's style: as a text field's text.
const STYLE: (FamilyToken, SizeToken, Weight) = (FamilyToken::Ui, SizeToken::Body, Weight::Regular);

/// The box that captures a shortcut (see the module documentation).
#[derive(Debug)]
pub struct ShortcutCapture<M> {
    /// The prompt ("Press the shortcut…"), already localised.
    prompt: String,
    /// The accessible name, already localised; `None` for the prompt.
    name: Option<String>,
    /// Its layout.
    layout: TextLayout,
    /// The text generation it was laid out for.
    laid_out_for: Option<u64>,
    /// Whose fixed keys apply (AM8).
    platform: Platform,
    /// The message with the shortcut pressed.
    on_capture: fn(Shortcut) -> M,
    /// The message with a fixed key pressed.
    on_refused: fn(Shortcut) -> M,
    /// The message when it is cancelled (Escape, or the focus left).
    on_cancel: fn() -> M,
    /// A shortcut was captured or the capture was cancelled: nothing more is sent (the
    /// application removes the box, and its focus leaving must not cancel a capture done).
    done: bool,
}

impl<M> ShortcutCapture<M> {
    /// A box showing `prompt`, applying `platform`'s fixed keys, sending `on_capture` with the
    /// shortcut, `on_refused` with a fixed key and `on_cancel` when it is cancelled.
    #[must_use]
    pub fn new(
        prompt: impl Into<String>,
        platform: Platform,
        on_capture: fn(Shortcut) -> M,
        on_refused: fn(Shortcut) -> M,
        on_cancel: fn() -> M,
    ) -> Self {
        Self {
            prompt: prompt.into(),
            name: None,
            layout: TextLayout::new(),
            laid_out_for: None,
            platform,
            on_capture,
            on_refused,
            on_cancel,
            done: false,
        }
    }

    /// The same box named `name` for assistive technologies.
    #[must_use]
    pub fn labelled(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sends `message` once: after it the box is done.
    fn finish(&mut self, cx: &mut EventCx<'_, M>, message: M) {
        if !self.done {
            self.done = true;
            cx.emit(message);
        }
    }

    /// What a pressed key does (see the module documentation).
    fn pressed(&mut self, cx: &mut EventCx<'_, M>, key: &Key, physical: crate::PhysicalKey) {
        let modifiers = cx.modifiers;
        match key {
            Key::Named(NamedKey::Escape) => self.finish(cx, (self.on_cancel)()),
            // A modifier alone: the user is still building the combination.
            Key::Named(NamedKey::Alt) | Key::Unknown => {}
            _ => {
                let [logical, fallback] = candidates(key, physical, modifiers);
                let Some(shortcut) = logical.or(fallback) else {
                    return;
                };
                if self.done {
                    return;
                }
                if shortcut.is_fixed(self.platform) {
                    // Refused, and still waiting: not `finish`.
                    cx.emit((self.on_refused)(shortcut));
                } else {
                    self.finish(cx, (self.on_capture)(shortcut));
                }
            }
        }
    }
}

impl<M: std::fmt::Debug + 'static> Widget<M> for ShortcutCapture<M> {
    /// As wide as the prompt and its padding, as high as a control.
    fn measure(&mut self, cx: &mut LayoutCx<'_, M>, constraints: Constraints) -> Size {
        let generation = cx.text_generation();
        if self.laid_out_for != Some(generation) {
            if cx
                .lay_out_text(&self.prompt, STYLE, None, &mut self.layout)
                .is_err()
            {
                self.layout = TextLayout::new();
            }
            self.laid_out_for = Some(generation);
        }
        let spacing = cx.spacing();
        constraints.constrain(Size::new(
            self.layout.width() + 2.0 * spacing.control_padding,
            spacing.control_height,
        ))
    }

    fn record(&self, cx: &RecordCx, recorder: &mut Recorder<'_>) {
        let rect = cx.rect;
        let radius = f32::from(cx.shape.corner_radius);
        recorder.fill_rounded_rect(RoundedRect::new(rect, radius), ColorToken::SurfaceSunken);
        let line = f32::from(cx.shape.line_width.max(1));
        recorder.stroke_rounded_rect(
            RoundedRect::new(rect.inflate(-line / 2.0), (radius - line / 2.0).max(0.0)),
            StrokeStyle::new(line),
            ColorToken::ControlBorder,
        );
        let y = rect.y0 + (rect.height() - self.layout.height()) / 2.0;
        recorder.text(
            &self.layout,
            Point::new(rect.x0 + cx.spacing.control_padding, y),
            ColorToken::TextMuted,
        );
        if cx.focused {
            super::look::focus_ring(recorder, rect, radius, cx.shape);
        }
    }

    fn event(&mut self, cx: &mut EventCx<'_, M>, event: &WidgetEvent<'_>) -> Handled {
        match *event {
            // Tab moves on, as everywhere; the focus leaving then cancels.
            WidgetEvent::Key {
                key: Key::Named(NamedKey::Tab),
                ..
            } => Handled::No,
            WidgetEvent::Key {
                key,
                physical,
                state: ButtonState::Pressed,
                repeat: false,
                ..
            } => {
                self.pressed(cx, key, physical);
                Handled::Yes
            }
            // Repeats, releases and typed text go nowhere else.
            WidgetEvent::Key { .. } | WidgetEvent::Text(_) => Handled::Yes,
            WidgetEvent::FocusLost => {
                self.finish(cx, (self.on_cancel)());
                Handled::Yes
            }
            WidgetEvent::PointerButton {
                button: PointerButton::Left,
                state: ButtonState::Pressed,
                ..
            } => {
                cx.request_focus();
                Handled::Yes
            }
            _ => Handled::No,
        }
    }

    fn focusable(&self) -> bool {
        true
    }

    fn access_role(&self) -> AccessRole {
        AccessRole::TextInput
    }

    fn access_info(&self, info: &mut AccessInfo) {
        info.label = Some(self.name.clone().unwrap_or_else(|| self.prompt.clone()));
        info.placeholder = Some(self.prompt.clone());
    }
}
