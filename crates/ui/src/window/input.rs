// SPDX-License-Identifier: GPL-3.0-or-later

//! Our own input events (ADR-0040 W5) and their translation from `winit`'s.
//!
//! The rest of the interface only ever sees these types, so widgets do not depend on `winit` and
//! a `winit` update touches this file only (ADR-0040 W1). Every translation here is a pure
//! function of `winit` values that tests can build, so it is tested without a display.

use winit::event::{ElementState, MouseButton, MouseScrollDelta};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey as WinitNamedKey};

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
        /// Pressed or released.
        state: ButtonState,
        /// `true` for an automatic repeat while the key is held.
        repeat: bool,
        /// The modifiers held at that moment.
        modifiers: Modifiers,
    },
    /// The modifiers held changed (also sent when they change without another key).
    ModifiersChanged {
        /// The modifiers held now.
        modifiers: Modifiers,
    },
    /// The window gained (`true`) or lost (`false`) the keyboard focus.
    Focus(bool),
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

impl Modifiers {
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
        _ => return None,
    })
}
