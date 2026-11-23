// SPDX-License-Identifier: GPL-3.0-or-later

//! The decision of who draws the frame (ADR-0049 TB2), on every platform, with made-up answers
//! of the probe: the real probe needs a compositor, and is checked by hand (TB12).

use super::{Decorations, ProbeAnswer, decide};

/// Every answer the probe can give.
const ANSWERS: [ProbeAnswer; 6] = [
    ProbeAnswer::NotWayland,
    ProbeAnswer::ServerSide,
    ProbeAnswer::ClientSide,
    ProbeAnswer::NoManager,
    ProbeAnswer::NoAnswer,
    ProbeAnswer::Failed,
];

#[test]
fn only_a_compositor_that_draws_the_frame_keeps_the_systems() {
    let decision = |answer| decide(None, || answer);
    // X11 (no Wayland compositor) and a compositor's server-side frame: the system's.
    assert_eq!(decision(ProbeAnswer::NotWayland), Decorations::System);
    assert_eq!(decision(ProbeAnswer::ServerSide), Decorations::System);
    // Everything else draws our bar: GNOME's client-side answer, no protocol, no answer, failure.
    assert_eq!(decision(ProbeAnswer::ClientSide), Decorations::Own);
    assert_eq!(decision(ProbeAnswer::NoManager), Decorations::Own);
    assert_eq!(decision(ProbeAnswer::NoAnswer), Decorations::Own);
    assert_eq!(decision(ProbeAnswer::Failed), Decorations::Own);
}

#[test]
fn the_override_decides_without_asking_the_compositor() {
    for answer in ANSWERS {
        for value in ["own", "OWN", " Own "] {
            // The probe must not even run: it would talk to the compositor for nothing.
            let decision = decide(Some(value), || panic!("probed despite {value:?}"));
            assert_eq!(decision, Decorations::Own, "{value:?} with {answer:?}");
        }
        for value in ["system", "System"] {
            let decision = decide(Some(value), || panic!("probed despite {value:?}"));
            assert_eq!(decision, Decorations::System, "{value:?} with {answer:?}");
        }
    }
}

#[test]
fn an_unknown_override_lets_the_probe_decide() {
    for value in ["", "yes", "server", "client"] {
        assert_eq!(
            decide(Some(value), || ProbeAnswer::ClientSide),
            Decorations::Own,
            "{value:?}"
        );
        assert_eq!(
            decide(Some(value), || ProbeAnswer::ServerSide),
            Decorations::System,
            "{value:?}"
        );
    }
}

// ----- GNOME's button-layout (F2.11b3) ---------------------------------------------------------

use super::layout::parse;
use crate::widget::titlebar::WindowButton::{Close, Maximize, Minimize};

#[test]
fn gnomes_default_is_close_on_the_right() {
    assert_eq!(parse("appmenu:close"), (vec![], vec![Close]));
}

#[test]
fn every_button_in_its_order_on_its_side() {
    assert_eq!(
        parse("appmenu:minimize,maximize,close"),
        (vec![], vec![Minimize, Maximize, Close])
    );
    assert_eq!(
        parse("close,minimize,maximize:"),
        (vec![Close, Minimize, Maximize], vec![])
    );
    assert_eq!(
        parse("close:maximize,minimize"),
        (vec![Close], vec![Maximize, Minimize])
    );
}

#[test]
fn unknown_names_spaces_repeats_and_no_colon() {
    // Items our bar does not have, and nonsense, are skipped; spaces are trimmed.
    assert_eq!(
        parse("icon, menu ,spacer:frobnicate, close"),
        (vec![], vec![Close])
    );
    // A button named twice appears once, where it is first named.
    assert_eq!(parse("close:close,minimize"), (vec![Close], vec![Minimize]));
    // No colon: everything on the leading side, as GTK reads it.
    assert_eq!(parse("close,maximize"), (vec![Close, Maximize], vec![]));
    // Empty, or only a colon: no buttons at all (the window still closes from the keyboard).
    assert_eq!(parse(""), (vec![], vec![]));
    assert_eq!(parse(":"), (vec![], vec![]));
}

#[test]
fn a_spec_from_the_setting_or_gnomes_default() {
    use crate::{TitleBarSpec, WindowButtonNames};
    let spec = |setting| {
        TitleBarSpec::from_setting(
            setting,
            "Orchestra".to_owned(),
            WindowButtonNames::default(),
        )
    };
    assert_eq!(spec(None).trailing, vec![Close]);
    assert_eq!(spec(None).leading, vec![]);
    let left = spec(Some("close,minimize:"));
    assert_eq!(left.leading, vec![Close, Minimize]);
    assert_eq!(left.trailing, vec![]);
}
