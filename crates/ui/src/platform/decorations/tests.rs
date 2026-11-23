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
