// SPDX-License-Identifier: GPL-3.0-or-later

//! Following the system's light/dark preference while the window is open (ADR-0038 S3, ADR-0040
//! W3, step F2.04b).
//!
//! # One path for every platform
//! Whatever the platform, a change of the system's preference reaches the window the same way:
//!
//! ```text
//! Linux:          portal thread ──┐
//!                                 ├─▶ Reporter (repeats dropped) ─▶ callback
//! Windows/macOS:  ThemeChanged ───┘        ─▶ EventLoopProxy::send_event(ShellEvent)
//!                 (fed by the loop)        ─▶ user_event ─▶ ThemeState::set_system ─▶ redraw all
//! ```
//!
//! The portal's thread cannot touch the interface (its state lives on the event loop's thread and
//! has no locks, W3), so it must go through the proxy. On Windows and macOS `ThemeChanged` already
//! arrives on the loop's thread and could be applied at once; it takes the same detour on purpose,
//! so the rules of the reporter (repeats dropped, nothing after the watch is dropped) and the code
//! that applies a change are one and the same everywhere. The cost is one more wake-up of the
//! loop per theme change, which happens a few times a day at most.
//!
//! # What this file holds
//! - [`ShellEvent`]: our own user event for `winit`'s proxy (W3);
//! - [`ThemeState`]: the pair, the mode and the half in use, and the rule that decides whether a
//!   new system answer changes what is drawn (pure logic, tested without a display);
//! - [`appearance_from_winit`]: `winit`'s theme as ours (`winit` lives only in this module, W1).

use winit::window::Theme as WinitTheme;

use crate::{Appearance, Theme, ThemeMode, ThemePair};

/// Our own event, sent to the event loop from any thread through `winit`'s `EventLoopProxy`
/// (ADR-0040 W3). Later variants will carry the audio engine's meters and transport.
///
/// Exhaustive (ADR-0034 D2), and crate-private: only the window module sends and receives it.
/// Not `Clone`: AccessKit's event is not.
#[derive(Debug)]
pub(crate) enum ShellEvent {
    /// The system's preference changed to this answer (`None`: the system stopped answering).
    SystemAppearance(Option<Appearance>),
    /// AccessKit's adapter needs something (ADR-0045 WA5, WC6): the whole tree, an action carried
    /// out, or nothing more (assistive technology gone).
    Access(accesskit_winit::WindowEvent),
    /// The user chose this action in the system's menu bar (macOS, F2.11e1, ADR-0048 AM9): it
    /// runs as the drawn menus and the shortcuts run theirs.
    MenuAction(crate::ActionId),
}

/// AccessKit's adapter sends its requests through our proxy (`with_event_loop_proxy` requires
/// this conversion). There is one window, so the window ID is not kept.
impl From<accesskit_winit::Event> for ShellEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Access(event.window_event)
    }
}

/// The theme the window draws with, and how it follows the system (ADR-0037 T1, ADR-0038).
///
/// The user's choice is a pair and a mode (ADR-0037 T1); the half shown depends on the mode and,
/// for *System*, on the system's answer. This struct keeps the three together so the window
/// always draws with the half they select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ThemeState {
    /// The pair chosen by the user.
    pair: ThemePair,
    /// The user's mode: *System*, *Light* or *Dark*.
    mode: ThemeMode,
    /// The system's last answer (`None` before it gave one, or when it has no preference), kept
    /// so that a change of mode back to *System* resolves at once (F2.12a4a).
    system: Option<Appearance>,
    /// The half in use, `mode.resolve(system)`.
    appearance: Appearance,
}

impl ThemeState {
    /// The state before the system has said anything: `None`, which for *System* is dark
    /// (ADR-0037 T9). The window replaces it with the first answer before its first frame.
    pub(crate) const fn new(pair: ThemePair, mode: ThemeMode) -> Self {
        Self {
            pair,
            mode,
            system: None,
            appearance: mode.resolve(None),
        }
    }

    /// Takes the user's new choice of pair and mode (the Preferences, ADR-0050 DG9, F2.12a4a).
    /// Returns `true` when what is drawn changed (another pair, or another half), so the whole
    /// window must be redrawn (ADR-0037 T6).
    ///
    /// The half is resolved with the system's last answer: back to *System*, the window shows the
    /// system's half at once, without waiting for the system to speak again.
    ///
    /// # Complexity
    /// O(1) plus the comparison of two pairs (a few hundred bytes of tokens).
    pub(crate) fn set_choice(&mut self, pair: ThemePair, mode: ThemeMode) -> bool {
        let appearance = mode.resolve(self.system);
        let changed = appearance != self.appearance || pair != self.pair;
        self.pair = pair;
        self.mode = mode;
        self.appearance = appearance;
        changed
    }

    /// The user's mode.
    pub(crate) const fn mode(&self) -> ThemeMode {
        self.mode
    }

    /// Takes a new answer of the system. Returns `true` when the half to draw with changed, so
    /// the whole window must be redrawn (ADR-0037 T6: switching theme is a full redraw).
    ///
    /// `false` when nothing visible changes: the same half again, or a mode of *Light* or *Dark*,
    /// which ignores the system. This is the second line of defence against repeats; the
    /// reporter already drops a value equal to the last one it delivered.
    ///
    /// # Complexity
    /// O(1): one `match` and one comparison of two-valued enums.
    pub(crate) fn set_system(&mut self, system: Option<Appearance>) -> bool {
        self.system = system;
        let next = self.mode.resolve(system);
        let changed = next != self.appearance;
        self.appearance = next;
        changed
    }

    /// The theme instance to draw with now. O(1), a reference into the pair.
    pub(crate) const fn theme(&self) -> &Theme {
        self.pair.instance(self.appearance)
    }

    /// The half in use (for tests and diagnostics).
    #[cfg(test)]
    pub(crate) const fn appearance(&self) -> Appearance {
        self.appearance
    }
}

/// `winit`'s theme as our [`Appearance`]: the two have the same two values.
///
/// `winit`'s enum is not marked non-exhaustive in 0.30, so the match is complete and a new
/// variant in a future `winit` would be a compile error here, not a silent default.
pub(crate) const fn appearance_from_winit(theme: WinitTheme) -> Appearance {
    match theme {
        WinitTheme::Light => Appearance::Light,
        WinitTheme::Dark => Appearance::Dark,
    }
}
