// SPDX-License-Identifier: GPL-3.0-or-later

//! Who draws the window's frame (ADR-0049 TB1, TB2; F2.11b2): the system, or our own title bar.
//!
//! # The rule (TB1)
//! Our title bar is drawn **only where the system draws none**: on Wayland when the compositor
//! does not give server-side decorations (GNOME's compositor never does; KDE's, sway and most
//! others do). On X11 the window manager always draws the frame, and on Windows and macOS the
//! system does, so there the answer is [`Decorations::System`] without asking anything.
//!
//! # Asking the compositor (TB2)
//! `winit` asks the compositor for server-side decorations itself, but does not tell the program
//! which answer it got (`Window::is_decorated` is `true` either way). So, **before** the window
//! exists, [`decorations`] opens its own short connection to the compositor and asks the same
//! question on a toplevel that never gets a buffer, so nothing ever appears on screen
//! (`wayland`, the way Rerun's viewer does it). The answer:
//! - **server-side** → [`Decorations::System`]: the window keeps the compositor's frame;
//! - **client-side**, **no decoration manager** (the protocol is optional), **no answer**, or
//!   **any failure** of the probe → [`Decorations::Own`]. A window without a frame cannot be
//!   moved or closed with the mouse; a frame too many is the lesser fault;
//! - no Wayland compositor to connect to → [`Decorations::System`]: `winit` then uses X11, where
//!   the window manager draws the frame.
//!
//! # The override
//! `ORCHESTRA_DECORATIONS=system` or `=own` (case-insensitive) skips the probe and forces the
//! answer: to see our title bar on KDE, to test, or as a way out if a compositor answers wrongly.
//! Any other value is ignored (the probe decides). On Windows and macOS the variable is ignored
//! too: their frame is always the system's (TB13).
//!
//! # The buttons
//! Which buttons our title bar shows, and where, is GNOME's own setting, read by `layout` through
//! the desktop portal (ADR-0049 TB4, F2.11b3).
//!
//! # Cost
//! Two round trips to the compositor on Wayland, once per program run, measured in F2.11b2 (see
//! `DOCUMENTATION.md`); nothing elsewhere.

pub(crate) mod layout;
#[cfg(target_os = "linux")]
mod wayland;

#[cfg(test)]
mod tests;

/// Who draws the window's frame (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Decorations {
    /// The system: the window manager, the compositor, Windows or macOS.
    System,
    /// We do: the window is created without decorations and the application draws its own title
    /// bar ([`Ui::add_title_bar`](crate::Ui::add_title_bar)).
    Own,
}

/// What the compositor answered to the probe (see the module documentation).
#[cfg_attr(
    not(target_os = "linux"),
    allow(
        dead_code,
        reason = "made by the Wayland probe, which exists on Linux only; tested"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeAnswer {
    /// There is no Wayland compositor to connect to: `winit` will use X11.
    NotWayland,
    /// The compositor draws the frame.
    ServerSide,
    /// The compositor wants the program to draw it.
    ClientSide,
    /// The compositor does not offer the (optional) decoration protocol at all.
    NoManager,
    /// The compositor offered the protocol but sent no mode.
    NoAnswer,
    /// The probe failed (a protocol error, a lost connection): we do not know.
    Failed,
}

/// The environment variable that forces the answer (see the module documentation).
pub(crate) const OVERRIDE_VARIABLE: &str = "ORCHESTRA_DECORATIONS";

/// Who draws the window's frame on this system, now (see the module documentation). Call it once,
/// before the window is created: on Wayland it talks to the compositor.
#[must_use]
pub fn decorations() -> Decorations {
    let value = std::env::var(OVERRIDE_VARIABLE).ok();
    if cfg!(target_os = "linux") {
        decide(value.as_deref(), probe)
    } else {
        Decorations::System
    }
}

/// GNOME's `button-layout` setting, read once from the desktop portal (ADR-0049 TB4, F2.11b3), for
/// [`TitleBarSpec::from_setting`](crate::TitleBarSpec::from_setting); `None` when it cannot be
/// read (another desktop, no portal, no answer within 250 ms, not Linux). Call it once, at
/// start-up, and only when [`decorations`] answered [`Decorations::Own`]: it may wait that long.
#[must_use]
pub fn button_layout() -> Option<String> {
    layout::read()
}

/// The decision of the module documentation, from the override's value (if the variable is set)
/// and the probe, which is called only when the override does not decide. Separate from
/// [`decorations`] so the tests give it made-up answers.
pub(crate) fn decide(value: Option<&str>, probe: impl FnOnce() -> ProbeAnswer) -> Decorations {
    match value.map(str::trim) {
        Some(value) if value.eq_ignore_ascii_case("system") => return Decorations::System,
        Some(value) if value.eq_ignore_ascii_case("own") => return Decorations::Own,
        _ => {}
    }
    match probe() {
        ProbeAnswer::NotWayland | ProbeAnswer::ServerSide => Decorations::System,
        ProbeAnswer::ClientSide
        | ProbeAnswer::NoManager
        | ProbeAnswer::NoAnswer
        | ProbeAnswer::Failed => Decorations::Own,
    }
}

/// The probe on this platform: the Wayland one on Linux.
#[cfg(target_os = "linux")]
fn probe() -> ProbeAnswer {
    wayland::probe()
}

/// No probe off Linux: [`decorations`] answers [`Decorations::System`] before asking.
#[cfg(not(target_os = "linux"))]
#[allow(dead_code, reason = "never called off Linux, see `decorations`")]
const fn probe() -> ProbeAnswer {
    ProbeAnswer::NotWayland
}
