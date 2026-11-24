// SPDX-License-Identifier: GPL-3.0-or-later

//! Linux: does the XDG desktop portal offer a file chooser? (ADR-0050 DG4, choice 2 of F2.12a5a)
//!
//! # The question
//! The portal (`org.freedesktop.portal.Desktop` on the session bus, object
//! `/org/freedesktop/portal/desktop`) exports the interface `org.freedesktop.portal.FileChooser`
//! only when one of its back ends (`xdg-desktop-portal-kde`, `-gnome`, `-gtk`, `-wlr`…)
//! implements it. Every portal interface has a `version` property; reading it with the standard
//! `org.freedesktop.DBus.Properties.Get` succeeds exactly when the interface is there. The value
//! itself is not needed.
//!
//! - **No session bus**, **no portal** (the bus cannot start it), **no `FileChooser`** (a portal
//!   without a suitable back end: the call answers `InvalidArgs` or `UnknownInterface`), or
//!   **no answer within [`TIMEOUT`]**: `false`.
//! - Otherwise `true`.
//!
//! # Why `zbus` and not `rfd`'s own libdbus-1
//! `rfd` does not say why it answered `None`. `zbus` is already in the program for the system's
//! appearance (ADR-0038) and for AccessKit, so the probe costs no new crate; it opens a
//! connection of its own (one more socket for a moment), because the appearance's connection
//! lives on its own thread.
//!
//! # Cost
//! One connection and one call per chooser, on the chooser's worker thread (never the interface
//! thread): a few milliseconds when the portal runs, which it does in a session that already
//! answered the appearance's question at start-up. When the bus must first start the portal, the
//! call waits for it, up to [`TIMEOUT`].

use std::time::Duration;

use zbus::connection::Builder;

/// The portal's bus name.
const DESTINATION: &str = "org.freedesktop.portal.Desktop";
/// The portal's object path.
const PATH: &str = "/org/freedesktop/portal/desktop";
/// The standard interface whose `Get` reads a property.
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
/// The interface that must be there.
const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
/// The property every portal interface has.
const VERSION: &str = "version";

/// How long the probe waits for the portal's answer. Long enough for the bus to start the portal
/// and its back end on a slow machine (both are started on demand when nobody has called them
/// yet; not measured on the reference laptop, so the margin is generous), short enough that a
/// broken session gets its error dialog before the user wonders what happened.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Whether the portal's `FileChooser` answers (see the module documentation). Blocks the calling
/// thread (the chooser's worker) for at most about [`TIMEOUT`].
pub(crate) fn file_chooser_answers() -> bool {
    zbus::block_on(async {
        // The timeout applies to every method call on this connection, the bus's own `Hello`
        // during `build` included, so a bus that never answers cannot hang the worker.
        let Ok(builder) = Builder::session() else {
            return false;
        };
        let Ok(connection) = builder.method_timeout(TIMEOUT).build().await else {
            return false;
        };
        connection
            .call_method(
                Some(DESTINATION),
                PATH,
                Some(PROPERTIES),
                "Get",
                &(FILE_CHOOSER, VERSION),
            )
            .await
            .is_ok()
    })
}
