// SPDX-License-Identifier: GPL-3.0-or-later

//! Which window buttons our title bar shows, and on which side (ADR-0049 TB4, F2.11b3): GNOME's
//! setting `org.gnome.desktop.wm.preferences button-layout`, read once at start-up (owner's
//! choice of F2.11b3: a change is seen at the next start).
//!
//! # The setting's form
//! Two comma-separated lists separated by a colon: the buttons before the title, then after it,
//! each list in order. GNOME's default is `appmenu:close` (only Close, on the right); a user who
//! turns on the other buttons gets `appmenu:minimize,maximize,close`, and the left-handed layout
//! of other desktops is `close,minimize,maximize:`. The names are `close`, `minimize`,
//! `maximize`, and items our bar does not have (`appmenu`, `menu`, `icon`, `spacer`), which are
//! ignored, as are names nobody knows. Without a colon every button is on the leading side (GTK
//! reads it so). A button named twice appears once, where it is first named. An empty side is
//! allowed: `:` hides every button, as in GTK (the window still closes with the keyboard).
//!
//! # Reading it
//! Through the desktop portal's `org.freedesktop.portal.Settings.ReadOne` (or the older `Read`),
//! the way GTK 4 and `sctk-adwaita` read it, on the session bus with `zbus` (ADR-0038). On another
//! thread, waited for at most [`READ_TIMEOUT`]: a portal that does not answer must not delay the
//! window. No portal, no such namespace (KDE's portal does not expose GNOME's), no answer in time,
//! or a value that is not a string: `None`, and the caller uses GNOME's default.

use crate::widget::titlebar::WindowButton;

/// How long the start-up waits for the portal's answer at most: the same bound as the first read
/// of the light/dark preference (ADR-0038 S5).
#[cfg_attr(
    not(target_os = "linux"),
    allow(dead_code, reason = "the portal exists on Linux only")
)]
pub(crate) const READ_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

/// The buttons of `text` (see the module documentation): the leading ones, then the trailing.
///
/// # Complexity
/// O(length of `text`); allocates the two short lists.
pub(crate) fn parse(text: &str) -> (Vec<WindowButton>, Vec<WindowButton>) {
    let (leading, trailing) = text.split_once(':').unwrap_or((text, ""));
    let mut seen = Vec::with_capacity(3);
    let mut side = |list: &str| -> Vec<WindowButton> {
        let mut buttons = Vec::new();
        for name in list.split(',').map(str::trim) {
            let button = match name {
                "close" => WindowButton::Close,
                "minimize" => WindowButton::Minimize,
                "maximize" => WindowButton::Maximize,
                // `appmenu`, `menu`, `icon`, `spacer`, empty, unknown: not on our bar.
                _ => continue,
            };
            if !seen.contains(&button) {
                seen.push(button);
                buttons.push(button);
            }
        }
        buttons
    };
    let leading = side(leading);
    let trailing = side(trailing);
    (leading, trailing)
}

/// The setting's text from the portal, or `None` (see the module documentation).
#[cfg(target_os = "linux")]
pub(crate) fn read() -> Option<String> {
    let (sender, receiver) = std::sync::mpsc::channel();
    // A thread of its own, so a portal that never answers costs the start-up at most the timeout;
    // the thread ends by itself when (if) the answer comes, and sends into a closed channel then.
    let spawned = std::thread::Builder::new()
        .name("orchestra-button-layout".to_owned())
        .spawn(move || {
            let _closed = sender.send(zbus::block_on(portal::read()));
        });
    if spawned.is_err() {
        return None;
    }
    receiver.recv_timeout(READ_TIMEOUT).ok().flatten()
}

/// No portal off Linux: our bar is drawn on Linux only anyway (ADR-0049 TB1).
#[cfg(not(target_os = "linux"))]
pub(crate) const fn read() -> Option<String> {
    None
}

/// The portal's side, Linux only.
#[cfg(target_os = "linux")]
mod portal {
    use zbus::Connection;
    use zbus::zvariant::{OwnedValue, Value};

    /// The portal's bus name, object path and interface (the same as the appearance's).
    const DESTINATION: &str = "org.freedesktop.portal.Desktop";
    /// See [`DESTINATION`].
    const PATH: &str = "/org/freedesktop/portal/desktop";
    /// See [`DESTINATION`].
    const INTERFACE: &str = "org.freedesktop.portal.Settings";
    /// GNOME's window manager settings.
    const NAMESPACE: &str = "org.gnome.desktop.wm.preferences";
    /// The buttons' layout.
    const KEY: &str = "button-layout";
    /// How many variant layers are unwrapped at most (the old `Read` adds one).
    const MAX_DEPTH: u8 = 2;

    /// Reads the setting with `ReadOne`, then with the deprecated `Read`; `None` on any failure.
    pub(super) async fn read() -> Option<String> {
        let connection = Connection::session().await.ok()?;
        for method in ["ReadOne", "Read"] {
            let Ok(reply) = connection
                .call_method(
                    Some(DESTINATION),
                    PATH,
                    Some(INTERFACE),
                    method,
                    &(NAMESPACE, KEY),
                )
                .await
            else {
                // An unknown method (an old portal) or an unknown key: try the next, or give up.
                continue;
            };
            let value: OwnedValue = reply.body().deserialize().ok()?;
            return text(&value, 0);
        }
        None
    }

    /// The string inside `value`, through at most [`MAX_DEPTH`] variant layers.
    fn text(value: &Value<'_>, depth: u8) -> Option<String> {
        match value {
            Value::Str(text) => Some(text.as_str().to_owned()),
            Value::Value(inner) if depth < MAX_DEPTH => text(inner, depth.saturating_add(1)),
            _ => None,
        }
    }
}
