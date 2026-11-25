// SPDX-License-Identifier: GPL-3.0-or-later

//! Our own input events (ADR-0040 W5) and their translation from `winit`'s.
//!
//! The rest of the interface only ever sees these types, so widgets do not depend on `winit` and
//! a `winit` update touches this file only (ADR-0040 W1). Every translation here is a pure
//! function of `winit` values that tests can build, so it is tested without a display.

use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{
    Key as WinitKey, KeyCode, ModifiersState, NamedKey as WinitNamedKey,
    PhysicalKey as WinitPhysicalKey,
};

use crate::Point;

/// An input event, in logical pixels (ADR-0040 W5).
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// The pointer moved to a position inside the window.
    PointerMoved {
        /// Where it is now.
        position: Point,
    },
    /// A pointer button was pressed or released at the pointer's current position.
    PointerButton {
        /// Which button.
        button: PointerButton,
        /// Pressed or released.
        state: ButtonState,
        /// Where the pointer is.
        position: Point,
        /// Which click of a series this is: 1 for a single click, 2 for the second click of a
        /// double click, and so on (ADR-0046 V4). A release carries the count of its press.
        clicks: u8,
    },
    /// The pointer left the window.
    PointerLeft,
    /// The wheel or the touchpad scrolled.
    Wheel {
        /// By how much.
        delta: WheelDelta,
    },
    /// A key was pressed or released.
    Key {
        /// The key, as the keyboard layout names it.
        key: Key,
        /// Where the key is on the keyboard (a precision of ADR-0040 W5 written in F2.11d1):
        /// only for the shortcuts' two fallbacks by position (ADR-0048 AM5) and the keypad.
        physical: PhysicalKey,
        /// Pressed or released.
        state: ButtonState,
        /// `true` for an automatic repeat while the key is held.
        repeat: bool,
        /// The modifiers held at that moment.
        modifiers: Modifiers,
    },
    /// Text typed (ADR-0047 TF5): what a key press produced, as the keyboard layout and the
    /// system's dead keys and compose sequences made it (`"é"` after a dead acute and `e`), sent
    /// right after that key's [`InputEvent::Key`]. Never empty, never a control character, never
    /// produced while the primary modifier alone is held (a shortcut, not typing). Text fields
    /// insert it; everything else ignores it.
    Text(String),
    /// An input method's event (ADR-0047 TF5, TF6, F2.10c2): a composition shown at the caret,
    /// or the text it produced. Sent only while the window lets the input method work, that is
    /// while a widget that takes text has the focus ([`Widget::accepts_text`]); like
    /// [`InputEvent::Text`] it goes to the focused widget.
    ///
    /// [`Widget::accepts_text`]: crate::Widget::accepts_text
    Ime(ImeEvent),
    /// The modifiers held changed (also sent when they change without another key).
    ModifiersChanged {
        /// The modifiers held now.
        modifiers: Modifiers,
    },
    /// The window gained (`true`) or lost (`false`) the keyboard focus.
    Focus(bool),
    /// The window's state changed (ADR-0049 TB6, TB8, F2.11b2): sent once when it opens, then
    /// whenever one of its parts changes. Our title bar shows Restore instead of Maximise while
    /// maximised, and hides its title and buttons in full screen.
    WindowState(WindowState),
    /// The half of the theme the window draws with now (ADR-0037 T1, F2.12b1): sent once after
    /// the window opens, then whenever it changes (the system switched, the user chose another
    /// mode). The theme editor edits this half, so the preview always shows what changes.
    Appearance(crate::Appearance),
}

/// The state of the window that our title bar shows (see [`InputEvent::WindowState`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowState {
    /// The window fills the screen's work area (maximised).
    pub maximized: bool,
    /// The window is in full screen.
    pub fullscreen: bool,
}

/// What an input method reports (ADR-0047 TF5, TF6), as `winit` gives it on every platform.
///
/// # The order of events
/// `Enabled` comes when the window turns the input method on (a field took the focus). Then,
/// while the user composes, `Preedit` with the text being composed, any number of times; a
/// `Preedit` with an empty text ends the composition (the user cancelled it, or it is about to
/// be committed). `Commit` brings the finished text, which is inserted like typing; on every
/// platform `winit` sends an empty `Preedit` before it. `Disabled` comes when the window turns
/// the input method off (the field lost the focus) or when the user switched it off; any
/// composition still shown is then dropped.
///
/// # Why two ways of typing, and no doubles
/// While the input method is on, a plain key still arrives as [`InputEvent::Key`] and
/// [`InputEvent::Text`] when the input method lets it through, and only a composed text arrives
/// as `Commit`: `winit` keeps the two apart on X11, Wayland, Windows and macOS (it reports no key
/// event for a key the input method used). So a text field inserts both, and nothing is typed
/// twice: rule (b) of ADR-0047 TF6, without a filter (precision written in F2.10c2, after the
/// owner's tests by hand with Fcitx5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImeEvent {
    /// The input method was turned on for the window.
    Enabled,
    /// The text being composed, shown at the caret and not yet part of the field's text.
    Preedit {
        /// The composition; empty when it ends.
        text: String,
        /// The input method's cursor inside `text`, in bytes: an empty range is a caret, a
        /// non-empty one the segment being converted (Mozc's current word), `None` hides the
        /// caret. Always inside `text` and on character boundaries (checked by
        /// [`ime_event`]).
        cursor: Option<std::ops::Range<usize>>,
    },
    /// The finished text, to insert at the caret as typing does (one undo step, grouped like
    /// typing: ADR-0047 TF6, TF7).
    Commit(String),
    /// The input method was turned off; a composition still shown is dropped.
    Disabled,
}

/// A pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    /// The primary button (left for right-handed setups).
    Left,
    /// The secondary button, usually for context menus.
    Right,
    /// The wheel's button.
    Middle,
    /// The "back" side button.
    Back,
    /// The "forward" side button.
    Forward,
    /// Any other button, by the number the system gives it.
    Other(u16),
}

/// Pressed or released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonState {
    /// Went down.
    Pressed,
    /// Went up.
    Released,
}

/// How far a scroll went.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WheelDelta {
    /// A mouse wheel: in lines (and columns), positive y = away from the user (scroll up).
    Lines {
        /// Columns.
        x: f32,
        /// Lines.
        y: f32,
    },
    /// A touchpad or a smooth wheel: in logical pixels, same directions.
    Pixels {
        /// Horizontal pixels.
        x: f32,
        /// Vertical pixels.
        y: f32,
    },
}

/// A key, as the current keyboard layout names it (the "logical" key, not the physical position).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// A key that types a character (or a short string, for some layouts): `"a"`, `"A"`, `"é"`,
    /// `"7"`, `" "` is [`NamedKey::Space`] instead.
    Character(String),
    /// A key with a name.
    Named(NamedKey),
    /// A key the system could not identify, or one we do not use (media keys, …).
    Unknown,
}

/// Where a key is on the keyboard, whatever the layout says it types (a precision of ADR-0040 W5
/// written in F2.11d1). Only the positions the shortcuts need are named (ADR-0048 AM5): the
/// letters and the digit row by their place on the US layout, and the keypad's digits; every
/// other key is [`PhysicalKey::Other`] (no keys "for later", SCALETTA §1.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicalKey {
    /// The key where the US layout has a letter: 0 for A, 25 for Z (the fallback for letters
    /// outside the Latin script, AM5 (i)).
    Letter(u8),
    /// A key of the digit row, 0 to 9, where the US layout has that digit (AM5 (ii)).
    Digit(u8),
    /// A digit of the numeric keypad, 0 to 9.
    Numpad(u8),
    /// Any other key, or one the system could not place.
    Other,
}

/// The named keys the interface uses. Others arrive as [`Key::Unknown`]; a new one is added here
/// when a feature needs it (no keys "for later", SCALETTA §1.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    /// Enter / Return.
    Enter,
    /// Escape.
    Escape,
    /// Tab.
    Tab,
    /// The space bar.
    Space,
    /// Backspace.
    Backspace,
    /// Delete (forward).
    Delete,
    /// Insert.
    Insert,
    /// Arrow up.
    ArrowUp,
    /// Arrow down.
    ArrowDown,
    /// Arrow left.
    ArrowLeft,
    /// Arrow right.
    ArrowRight,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// A function key, F1 to F24.
    Function(u8),
    /// The Menu key (the "application" key), which opens a context menu (ADR-0048 AM12); never
    /// an action's shortcut (AM8).
    ContextMenu,
    /// The Alt key itself (Option on macOS), pressed or released alone (F2.11d2b): on Linux and
    /// Windows, Alt pressed and released with nothing in between opens the menu bar (ADR-0048
    /// AM12). `AltGr` is not this key (it types characters) and stays [`Key::Unknown`]. Never an
    /// action's shortcut (AM8).
    Alt,
}

/// The modifier keys held (ADR-0040 W5).
///
/// Shortcuts are written with [`primary`](Self::primary), never with Ctrl or Cmd, so one shortcut
/// table works on every platform (CLAUDE.md §6): Ctrl+S on Linux and Windows is ⌘S on macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "four independent keys; a bit set would only hide them behind accessors"
)]
pub struct Modifiers {
    /// The platform's primary modifier: Ctrl on Linux and Windows, Cmd (⌘) on macOS.
    pub primary: bool,
    /// The other one: the Windows/Super key on Linux and Windows, Ctrl (⌃) on macOS.
    pub secondary: bool,
    /// Shift.
    pub shift: bool,
    /// Alt on Linux and Windows, Option (⌥) on macOS.
    pub alt: bool,
}

/// The modifier of fine adjustment (ADR-0046 V3, ADR-0050 DG9; the Preferences' choice,
/// F2.12a4b1): held, a value control moves one step instead of the coarse step and drags a tenth
/// as fast. The primary modifier by default, as V3 decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FineModifier {
    /// Ctrl on Linux and Windows, ⌘ on macOS.
    #[default]
    Primary,
    /// Shift. Shift+wheel over a value control then adjusts finely instead of scrolling the area
    /// around it (a precision of ADR-0046 V5, F2.12a4b1): the area scrolls with the wheel outside
    /// the control.
    Shift,
    /// Alt on Linux and Windows, ⌥ Option on macOS.
    Alt,
}

impl Modifiers {
    /// `true` while `fine` is held: the fine adjustment (ADR-0046 V3). Other keys held with it do
    /// not matter, as before the choice existed.
    #[must_use]
    pub const fn holds(self, fine: FineModifier) -> bool {
        match fine {
            FineModifier::Primary => self.primary,
            FineModifier::Shift => self.shift,
            FineModifier::Alt => self.alt,
        }
    }

    /// The modifiers from `winit`'s state, with the primary modifier of the platform this program
    /// was built for.
    #[must_use]
    pub(crate) fn from_winit(state: ModifiersState) -> Self {
        Self::from_keys(
            state.control_key(),
            state.super_key(),
            state.shift_key(),
            state.alt_key(),
            cfg!(target_os = "macos"),
        )
    }

    /// The modifiers from the four keys, with Cmd (the "super" key) as primary when `mac` is true,
    /// Ctrl otherwise. Separate from [`from_winit`](Self::from_winit) so the tests check both
    /// platforms' mapping on any platform.
    #[must_use]
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "the four modifier keys and the platform, each a yes/no; only the tests call it \
                  with literals, the real code goes through from_winit"
    )]
    pub(crate) const fn from_keys(
        control: bool,
        logo: bool,
        shift: bool,
        alt: bool,
        mac: bool,
    ) -> Self {
        let (primary, secondary) = if mac {
            (logo, control)
        } else {
            (control, logo)
        };
        Self {
            primary,
            secondary,
            shift,
            alt,
        }
    }
}

/// `winit`'s element state as ours.
pub(crate) const fn button_state(state: ElementState) -> ButtonState {
    match state {
        ElementState::Pressed => ButtonState::Pressed,
        ElementState::Released => ButtonState::Released,
    }
}

/// `winit`'s mouse button as ours.
pub(crate) const fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Back,
        MouseButton::Forward => PointerButton::Forward,
        MouseButton::Other(number) => PointerButton::Other(number),
    }
}

/// `winit`'s scroll delta as ours; pixel deltas are divided by the scale to become logical.
pub(crate) fn wheel_delta(delta: MouseScrollDelta, scale: f64) -> WheelDelta {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => WheelDelta::Lines { x, y },
        MouseScrollDelta::PixelDelta(position) => WheelDelta::Pixels {
            x: to_logical(position.x, scale),
            y: to_logical(position.y, scale),
        },
    }
}

/// A physical coordinate (`f64`, as `winit` gives it) in logical pixels (`f32`).
#[allow(
    clippy::cast_possible_truncation,
    reason = "screen coordinates fit f32 with room to spare; f64 is only winit's API type"
)]
pub(crate) fn to_logical(value: f64, scale: f64) -> f32 {
    if scale > 0.0 {
        (value / scale) as f32
    } else {
        value as f32
    }
}

/// `winit`'s logical key as ours.
pub(crate) fn key(key: &WinitKey) -> Key {
    match key {
        WinitKey::Character(text) => Key::Character(text.to_string()),
        WinitKey::Named(named) => named_key(*named).map_or(Key::Unknown, Key::Named),
        WinitKey::Unidentified(_) | WinitKey::Dead(_) => Key::Unknown,
    }
}

/// `winit`'s physical key as ours (see [`PhysicalKey`]): a plain table, tested without a
/// display.
#[must_use]
pub(crate) const fn physical_key(key: WinitPhysicalKey) -> PhysicalKey {
    let WinitPhysicalKey::Code(code) = key else {
        return PhysicalKey::Other;
    };
    match code {
        KeyCode::KeyA => PhysicalKey::Letter(0),
        KeyCode::KeyB => PhysicalKey::Letter(1),
        KeyCode::KeyC => PhysicalKey::Letter(2),
        KeyCode::KeyD => PhysicalKey::Letter(3),
        KeyCode::KeyE => PhysicalKey::Letter(4),
        KeyCode::KeyF => PhysicalKey::Letter(5),
        KeyCode::KeyG => PhysicalKey::Letter(6),
        KeyCode::KeyH => PhysicalKey::Letter(7),
        KeyCode::KeyI => PhysicalKey::Letter(8),
        KeyCode::KeyJ => PhysicalKey::Letter(9),
        KeyCode::KeyK => PhysicalKey::Letter(10),
        KeyCode::KeyL => PhysicalKey::Letter(11),
        KeyCode::KeyM => PhysicalKey::Letter(12),
        KeyCode::KeyN => PhysicalKey::Letter(13),
        KeyCode::KeyO => PhysicalKey::Letter(14),
        KeyCode::KeyP => PhysicalKey::Letter(15),
        KeyCode::KeyQ => PhysicalKey::Letter(16),
        KeyCode::KeyR => PhysicalKey::Letter(17),
        KeyCode::KeyS => PhysicalKey::Letter(18),
        KeyCode::KeyT => PhysicalKey::Letter(19),
        KeyCode::KeyU => PhysicalKey::Letter(20),
        KeyCode::KeyV => PhysicalKey::Letter(21),
        KeyCode::KeyW => PhysicalKey::Letter(22),
        KeyCode::KeyX => PhysicalKey::Letter(23),
        KeyCode::KeyY => PhysicalKey::Letter(24),
        KeyCode::KeyZ => PhysicalKey::Letter(25),
        KeyCode::Digit0 => PhysicalKey::Digit(0),
        KeyCode::Digit1 => PhysicalKey::Digit(1),
        KeyCode::Digit2 => PhysicalKey::Digit(2),
        KeyCode::Digit3 => PhysicalKey::Digit(3),
        KeyCode::Digit4 => PhysicalKey::Digit(4),
        KeyCode::Digit5 => PhysicalKey::Digit(5),
        KeyCode::Digit6 => PhysicalKey::Digit(6),
        KeyCode::Digit7 => PhysicalKey::Digit(7),
        KeyCode::Digit8 => PhysicalKey::Digit(8),
        KeyCode::Digit9 => PhysicalKey::Digit(9),
        KeyCode::Numpad0 => PhysicalKey::Numpad(0),
        KeyCode::Numpad1 => PhysicalKey::Numpad(1),
        KeyCode::Numpad2 => PhysicalKey::Numpad(2),
        KeyCode::Numpad3 => PhysicalKey::Numpad(3),
        KeyCode::Numpad4 => PhysicalKey::Numpad(4),
        KeyCode::Numpad5 => PhysicalKey::Numpad(5),
        KeyCode::Numpad6 => PhysicalKey::Numpad(6),
        KeyCode::Numpad7 => PhysicalKey::Numpad(7),
        KeyCode::Numpad8 => PhysicalKey::Numpad(8),
        KeyCode::Numpad9 => PhysicalKey::Numpad(9),
        _ => PhysicalKey::Other,
    }
}

/// The text a key press typed (ADR-0047 TF5), from `winit`'s `KeyEvent::text`, or `None` when
/// it typed nothing a text field should insert.
///
/// # What is dropped, and why
/// - **Control characters** (C0 and C1, DEL): Ctrl+A reports U+0001, Backspace U+0008, Enter
///   U+000D, Tab U+0009, Escape U+001B on some platforms. Those keys act through their
///   [`InputEvent::Key`]; inserting them would put invisible characters in a name.
/// - **Text while the primary modifier is held without Alt**: on macOS ⌘A reports `"a"` as its
///   text, and the shortcut must not also type. Alt is let through because on Windows `AltGr` is
///   reported as Ctrl+Alt, and `AltGr` is how many layouts type `@`, `€`, `[` or `{`.
///
/// A press of several characters (an IME-less compose result, a key mapped to a string) keeps
/// them all, minus the control characters. Pure, so it is tested without a display.
#[must_use]
pub(crate) fn typed_text(text: Option<&str>, modifiers: Modifiers) -> Option<String> {
    if modifiers.primary && !modifiers.alt {
        return None;
    }
    let text = text?;
    // One small `String` per typed key, the one the event carries (keys arrive a few per second;
    // the field copies the text into its own buffer, which keeps its capacity).
    let kept: String = text.chars().filter(|c| !c.is_control()).collect();
    (!kept.is_empty()).then_some(kept)
}

/// `winit`'s input-method event as ours (ADR-0047 TF5).
///
/// The preedit's cursor arrives as a pair of byte offsets, begin and end, which `winit` passes
/// on from the platform unchecked; it becomes an ordered range, and a cursor that is not inside
/// the text or not on character boundaries (an input method's bug) becomes `None`: the
/// composition is still shown, without a caret, rather than slicing the text at an invalid
/// place. Pure, so it is tested without a display.
#[must_use]
pub(crate) fn ime_event(event: winit::event::Ime) -> ImeEvent {
    match event {
        winit::event::Ime::Enabled => ImeEvent::Enabled,
        winit::event::Ime::Preedit(text, cursor) => {
            let cursor = cursor
                .map(|(begin, end)| begin.min(end)..begin.max(end))
                .filter(|range| {
                    text.is_char_boundary(range.start) && text.is_char_boundary(range.end)
                });
            ImeEvent::Preedit { text, cursor }
        }
        winit::event::Ime::Commit(text) => ImeEvent::Commit(text),
        winit::event::Ime::Disabled => ImeEvent::Disabled,
    }
}

/// `winit`'s named key as ours, if we use it.
fn named_key(key: WinitNamedKey) -> Option<NamedKey> {
    Some(match key {
        WinitNamedKey::Enter => NamedKey::Enter,
        WinitNamedKey::Escape => NamedKey::Escape,
        WinitNamedKey::Tab => NamedKey::Tab,
        WinitNamedKey::Space => NamedKey::Space,
        WinitNamedKey::Backspace => NamedKey::Backspace,
        WinitNamedKey::Delete => NamedKey::Delete,
        WinitNamedKey::Insert => NamedKey::Insert,
        WinitNamedKey::ArrowUp => NamedKey::ArrowUp,
        WinitNamedKey::ArrowDown => NamedKey::ArrowDown,
        WinitNamedKey::ArrowLeft => NamedKey::ArrowLeft,
        WinitNamedKey::ArrowRight => NamedKey::ArrowRight,
        WinitNamedKey::Home => NamedKey::Home,
        WinitNamedKey::End => NamedKey::End,
        WinitNamedKey::PageUp => NamedKey::PageUp,
        WinitNamedKey::PageDown => NamedKey::PageDown,
        WinitNamedKey::F1 => NamedKey::Function(1),
        WinitNamedKey::F2 => NamedKey::Function(2),
        WinitNamedKey::F3 => NamedKey::Function(3),
        WinitNamedKey::F4 => NamedKey::Function(4),
        WinitNamedKey::F5 => NamedKey::Function(5),
        WinitNamedKey::F6 => NamedKey::Function(6),
        WinitNamedKey::F7 => NamedKey::Function(7),
        WinitNamedKey::F8 => NamedKey::Function(8),
        WinitNamedKey::F9 => NamedKey::Function(9),
        WinitNamedKey::F10 => NamedKey::Function(10),
        WinitNamedKey::F11 => NamedKey::Function(11),
        WinitNamedKey::F12 => NamedKey::Function(12),
        WinitNamedKey::F13 => NamedKey::Function(13),
        WinitNamedKey::F14 => NamedKey::Function(14),
        WinitNamedKey::F15 => NamedKey::Function(15),
        WinitNamedKey::F16 => NamedKey::Function(16),
        WinitNamedKey::F17 => NamedKey::Function(17),
        WinitNamedKey::F18 => NamedKey::Function(18),
        WinitNamedKey::F19 => NamedKey::Function(19),
        WinitNamedKey::F20 => NamedKey::Function(20),
        WinitNamedKey::F21 => NamedKey::Function(21),
        WinitNamedKey::F22 => NamedKey::Function(22),
        WinitNamedKey::F23 => NamedKey::Function(23),
        WinitNamedKey::F24 => NamedKey::Function(24),
        WinitNamedKey::ContextMenu => NamedKey::ContextMenu,
        WinitNamedKey::Alt => NamedKey::Alt,
        _ => return None,
    })
}

/// The longest pause between two presses of a double click (ADR-0046 V4): 400 ms, the usual
/// default of desktop systems; the system's own setting can replace it later.
pub(crate) const DOUBLE_CLICK_TIME: std::time::Duration = std::time::Duration::from_millis(400);

/// The farthest the pointer may move between the two presses of a double click, logical pixels.
pub(crate) const DOUBLE_CLICK_DISTANCE: f32 = 4.0;

/// Counts the clicks of a series (ADR-0046 V4): `winit` reports presses one by one, with no
/// count and no time, so the window keeps the last press and decides whether the next one
/// continues the series (same button, soon enough, close enough).
///
/// Pure logic, with the time passed in, so it is tested without a window or a clock.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct ClickCounter {
    /// The last press: when, where, which button, and its count.
    last: Option<(std::time::Instant, Point, PointerButton, u8)>,
}

impl ClickCounter {
    /// Counts a press of `button` at `position` at time `now`, and returns its place in the series.
    pub(crate) fn press(
        &mut self,
        now: std::time::Instant,
        position: Point,
        button: PointerButton,
    ) -> u8 {
        let count = match self.last {
            Some((time, at, last_button, count))
                if last_button == button
                    && now.saturating_duration_since(time) <= DOUBLE_CLICK_TIME
                    && (position.x - at.x).abs() <= DOUBLE_CLICK_DISTANCE
                    && (position.y - at.y).abs() <= DOUBLE_CLICK_DISTANCE =>
            {
                count.saturating_add(1)
            }
            _ => 1,
        };
        self.last = Some((now, position, button, count));
        count
    }

    /// The count of the last press (what its release carries), 1 before any press.
    pub(crate) fn current(&self) -> u8 {
        self.last.map_or(1, |(_, _, _, count)| count)
    }
}

#[cfg(test)]
mod click_tests {
    use std::time::{Duration, Instant};

    use super::{ClickCounter, DOUBLE_CLICK_TIME, PointerButton};
    use crate::Point;

    #[test]
    fn presses_close_in_time_and_space_make_a_series() {
        let start = Instant::now();
        let mut counter = ClickCounter::default();
        let at = Point::new(10.0, 10.0);
        assert_eq!(counter.press(start, at, PointerButton::Left), 1);
        assert_eq!(
            counter.press(start + Duration::from_millis(200), at, PointerButton::Left),
            2
        );
        assert_eq!(counter.current(), 2);
        assert_eq!(
            counter.press(start + Duration::from_millis(350), at, PointerButton::Left),
            3
        );
        // Too late: a new series.
        let late =
            start + Duration::from_millis(350) + DOUBLE_CLICK_TIME + Duration::from_millis(1);
        assert_eq!(counter.press(late, at, PointerButton::Left), 1);
        // Too far, or another button: a new series.
        assert_eq!(
            counter.press(late, Point::new(20.0, 10.0), PointerButton::Left),
            1
        );
        assert_eq!(
            counter.press(late, Point::new(20.0, 10.0), PointerButton::Right),
            1
        );
        // Vertically too far: a new series; within 4 px both ways: the series goes on.
        assert_eq!(
            counter.press(late, Point::new(20.0, 15.0), PointerButton::Right),
            1
        );
        assert_eq!(
            counter.press(late, Point::new(24.0, 11.0), PointerButton::Right),
            2
        );
    }
}

#[cfg(test)]
mod ime_tests {
    use winit::event::Ime;

    use super::{ImeEvent, ime_event};

    #[test]
    fn enabling_committing_and_disabling_pass_through() {
        assert_eq!(ime_event(Ime::Enabled), ImeEvent::Enabled);
        assert_eq!(
            ime_event(Ime::Commit("日本".to_owned())),
            ImeEvent::Commit("日本".to_owned())
        );
        assert_eq!(ime_event(Ime::Disabled), ImeEvent::Disabled);
    }

    #[test]
    fn a_preedit_cursor_becomes_an_ordered_range_inside_the_text() {
        // "にほん" is three characters of three bytes each.
        let event = ime_event(Ime::Preedit("にほん".to_owned(), Some((9, 3))));
        assert_eq!(
            event,
            ImeEvent::Preedit {
                text: "にほん".to_owned(),
                cursor: Some(3..9),
            }
        );
        let caret = ime_event(Ime::Preedit("nihao".to_owned(), Some((5, 5))));
        assert_eq!(
            caret,
            ImeEvent::Preedit {
                text: "nihao".to_owned(),
                cursor: Some(5..5),
            }
        );
    }

    #[test]
    fn a_cursor_outside_the_text_or_inside_a_character_is_hidden() {
        for cursor in [(0, 7), (1, 1), (4, 6)] {
            assert_eq!(
                ime_event(Ime::Preedit("にほ".to_owned(), Some(cursor))),
                ImeEvent::Preedit {
                    text: "にほ".to_owned(),
                    cursor: None,
                },
                "{cursor:?}"
            );
        }
        assert_eq!(
            ime_event(Ime::Preedit(String::new(), None)),
            ImeEvent::Preedit {
                text: String::new(),
                cursor: None,
            }
        );
    }
}
