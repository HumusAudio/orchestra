// SPDX-License-Identifier: GPL-3.0-or-later

//! The order of a dialog's buttons on each system (ADR-0050 DG2, choice C3 of F2.12a3a).
//!
//! # The conventions
//! | System | Order | Source |
//! |---|---|---|
//! | Windows | affirmative **first**: *OK, Cancel*; *Save, Don't Save, Cancel* | Microsoft's *Dialog boxes* guidelines (commit buttons) |
//! | KDE Plasma | affirmative **first**: *OK, Apply, Cancel* | Qt's `QDialogButtonBox::KdeLayout` |
//! | macOS | affirmative **last**, at the trailing edge: *Cancel, OK*; *Don't Save* at the far left | Apple's *Human Interface Guidelines*, Alerts |
//! | GNOME and the other Linux desktops | affirmative **last**: *Cancel, OK* | GNOME HIG, Dialogs; Qt's `GnomeLayout` |
//!
//! Every row is right-aligned in the dialog: only the order differs.
//!
//! # How the desktop is known on Linux
//! `XDG_CURRENT_DESKTOP` (from the XDG desktop entry specification) is a colon-separated list of
//! names set by the session (`KDE`, `GNOME`, `ubuntu:GNOME`, `XFCE`…). The order is
//! affirmative-first when one of the names is `KDE` (compared without case: some distributions
//! write `kde`), affirmative-last otherwise, including when the variable is missing. It is read
//! once ([`system_button_order`]); the decision itself, [`button_order`], takes the platform and the
//! variable's value as arguments, so the tests check every case on any system.

use crate::action::shortcut::Platform;

/// Where the affirmative button of a dialog goes in its row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ButtonOrder {
    /// The affirmative button first (Windows, KDE Plasma): affirmative, destructive, others,
    /// cancel.
    AffirmativeFirst,
    /// The affirmative button last, at the trailing edge (macOS, GNOME): destructive, others,
    /// cancel, affirmative.
    AffirmativeLast,
}

/// The order on `platform`, with `desktop` the value of `XDG_CURRENT_DESKTOP` (ignored outside
/// Linux). O(length of `desktop`).
#[must_use]
pub fn button_order(platform: Platform, desktop: Option<&str>) -> ButtonOrder {
    match platform {
        Platform::Windows => ButtonOrder::AffirmativeFirst,
        Platform::MacOs => ButtonOrder::AffirmativeLast,
        Platform::Linux => {
            let kde = desktop.is_some_and(|names| {
                names
                    .split(':')
                    .any(|name| name.trim().eq_ignore_ascii_case("KDE"))
            });
            if kde {
                ButtonOrder::AffirmativeFirst
            } else {
                ButtonOrder::AffirmativeLast
            }
        }
    }
}

/// The order of the system this program runs on, reading `XDG_CURRENT_DESKTOP` on Linux. The
/// application calls it once at start-up and hands the answer to the interface
/// ([`Ui::set_button_order`](crate::Ui::set_button_order)).
#[must_use]
pub fn system_button_order() -> ButtonOrder {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
    button_order(Platform::CURRENT, desktop.as_deref())
}

#[cfg(test)]
mod tests {
    //! The four conventions, and the forms `XDG_CURRENT_DESKTOP` takes.
    use super::*;

    /// Windows and macOS ignore the desktop; Linux follows it.
    #[test]
    fn each_system_has_its_order() {
        assert_eq!(
            button_order(Platform::Windows, Some("GNOME")),
            ButtonOrder::AffirmativeFirst
        );
        assert_eq!(
            button_order(Platform::MacOs, Some("KDE")),
            ButtonOrder::AffirmativeLast
        );
        for (desktop, order) in [
            (Some("KDE"), ButtonOrder::AffirmativeFirst),
            (Some("kde"), ButtonOrder::AffirmativeFirst),
            (Some("X-Generic:KDE"), ButtonOrder::AffirmativeFirst),
            (Some("GNOME"), ButtonOrder::AffirmativeLast),
            (Some("ubuntu:GNOME"), ButtonOrder::AffirmativeLast),
            (Some("XFCE"), ButtonOrder::AffirmativeLast),
            (Some("KDEish"), ButtonOrder::AffirmativeLast),
            (Some(""), ButtonOrder::AffirmativeLast),
            (None, ButtonOrder::AffirmativeLast),
        ] {
            assert_eq!(button_order(Platform::Linux, desktop), order, "{desktop:?}");
        }
    }
}
