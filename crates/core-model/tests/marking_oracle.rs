// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of grace chords, score order and slurs against an independent oracle
//! (ADR-0016 §4, ADR-0024 §5 and §7, SCALETTA F1.09c).
//!
//! # The oracle
//! Written here, sharing no code with the library. A bar of 4/4 is counted in **sixteenths as
//! plain integers**; main events are generated as a gap and a length in sixteenths (so they never
//! overlap), grace chords as a position in sixteenths and a side. The oracle then decides:
//! - **the chunk order**: sort by (position, side rank, order of entry), where the side rank is
//!   written out here (After = 0, Before = 1, main = 2) from ADR-0024 §5, not taken from the
//!   library;
//! - **validity**: a grace strictly inside a main event is an overlap; a `Before` grace needs a
//!   main event starting at its position; an `After` grace needs one ending there; a slur needs
//!   its end strictly after its start in the oracle's order;
//! - **score order** of any two events, which [`Document::comes_before`] must reproduce.
//!
//! # What is checked
//! - The chunk holds the events in the oracle's order, and every grace lasts zero.
//! - `build()` succeeds exactly when the oracle finds the document valid.
//! - `comes_before` agrees with the oracle on every pair of events.
//! - `slurs_at` lists, for every event, exactly the slurs that start or end on it, in the order
//!   they were added.
//!
//! [`Document::comes_before`]: orchestra_core_model::Document::comes_before

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    Alteration, BaseValue, Clef, DocumentBuilder, EventId, Grace, GraceAttach, GraceKind, Interval,
    Key, Metronome, Mode, NoteValue, Pitch, Placement, SlurId, SlurPlacement, Step, TempoMark,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use proptest::prelude::*;

/// What the oracle knows of one event.
#[derive(Debug, Clone, Copy)]
struct OracleEvent {
    /// Its position in sixteenths from the start of the bar.
    at: i64,
    /// For a main event, its length in sixteenths; `None` for a grace.
    length: Option<i64>,
    /// The side rank of ADR-0024 §5: After grace 0, Before grace 1, main event 2.
    rank: u8,
    /// The order in which it was entered.
    entry: usize,
}

impl OracleEvent {
    /// The oracle's score-order key: position, side rank, order of entry (one voice and bar
    /// only, so the entry order is always a valid tie-break).
    fn key(&self) -> (i64, u8, usize) {
        (self.at, self.rank, self.entry)
    }
}

/// One generated event: either a main quarter-ish note (gap and length in sixteenths, relative to
/// the end of the previous main event) or a grace (absolute position and side).
#[derive(Debug, Clone, Copy)]
enum GenEvent {
    /// A main event `gap` sixteenths after the previous one ends, lasting `length` sixteenths.
    Main {
        /// Sixteenths of silence before it.
        gap: i64,
        /// Its length in sixteenths: 1, 2 or 4 (a sixteenth, an eighth, a quarter).
        length: i64,
    },
    /// A grace at `at` sixteenths, before or after its main event.
    Grace {
        /// Its position in sixteenths, 0..=16.
        at: i64,
        /// `true` for `After`.
        after: bool,
    },
}

/// The note value for a length of 1, 2 or 4 sixteenths.
fn value_of(length: i64) -> NoteValue {
    NoteValue::plain(match length {
        1 => BaseValue::Sixteenth,
        2 => BaseValue::Eighth,
        _ => BaseValue::Quarter,
    })
}

/// A position or length of `n` sixteenths.
fn sixteenths(n: i64) -> Duration {
    Duration::from_rational(Rational::new(n, 16).unwrap())
}

/// The events of one bar: mostly main events, some graces.
fn events() -> impl Strategy<Value = Vec<GenEvent>> {
    prop::collection::vec(
        prop_oneof![
            3 => (0_i64..3, prop::sample::select(vec![1_i64, 2, 4]))
                .prop_map(|(gap, length)| GenEvent::Main { gap, length }),
            2 => (0_i64..=16, any::<bool>())
                .prop_map(|(at, after)| GenEvent::Grace { at, after }),
        ],
        1..12,
    )
}

/// Slurs as pairs of indices into the events (reduced modulo their number).
fn slurs() -> impl Strategy<Value = Vec<(usize, usize)>> {
    prop::collection::vec((0_usize..64, 0_usize..64), 0..6)
}

/// A builder with one bar of 4/4 and a flute: (builder, the bar's ID, the voice).
fn flute() -> (
    DocumentBuilder,
    orchestra_core_model::BarId,
    orchestra_core_model::VoiceId,
) {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(sixteenths(4), Rational::from_integer(60).unwrap()).unwrap();
    let key = Key::traditional(0, Mode::Major).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        key,
    )
    .unwrap();
    let bar = b.add_bar().unwrap();
    let track = b
        .add_instrument_track("Flute", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let voice = b.document().track(track).unwrap().instrument().voices()[0].id();
    (b, bar, voice)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Chunk order, validity, score order and the slur index against the oracle.
    #[test]
    fn graces_order_and_slurs(gen_events in events(), gen_slurs in slurs()) {
        let (mut b, bar, voice) = flute();
        let pitch = Pitch::new(Step::C, Alteration::NATURAL, 5);

        // Enter the events, keeping the oracle's view of each next to its ID.
        let mut oracle: Vec<(EventId, OracleEvent)> = Vec::new();
        let mut main_end = 0_i64;
        for (entry, e) in gen_events.iter().enumerate() {
            let (id, view) = match *e {
                GenEvent::Main { gap, length } => {
                    let at = main_end + gap;
                    main_end = at + length;
                    let ids = b
                        .add_chord(Placement::new(voice, bar, sixteenths(at)), value_of(length), &[pitch])
                        .unwrap();
                    (ids.event, OracleEvent { at, length: Some(length), rank: 2, entry })
                }
                GenEvent::Grace { at, after } => {
                    let grace = Grace {
                        kind: GraceKind::Acciaccatura,
                        attach: if after { GraceAttach::After } else { GraceAttach::Before },
                    };
                    let ids = b
                        .add_grace_chord(
                            Placement::new(voice, bar, sixteenths(at)),
                            grace,
                            NoteValue::plain(BaseValue::Eighth),
                            &[pitch],
                        )
                        .unwrap();
                    (ids.event, OracleEvent { at, length: None, rank: u8::from(!after), entry })
                }
            };
            oracle.push((id, view));
        }

        // Slurs between two distinct events (a slur to itself is a builder error, tested by
        // hand); the oracle remembers which ends each one has.
        let mut added: Vec<(SlurId, usize, usize)> = Vec::new();
        for &(i, j) in &gen_slurs {
            let (i, j) = (i % oracle.len(), j % oracle.len());
            if i == j {
                continue;
            }
            let id = b.add_slur(oracle[i].0, oracle[j].0, SlurPlacement::Auto).unwrap();
            added.push((id, i, j));
        }

        // The chunk order is the oracle's sort.
        let mut sorted = oracle.clone();
        sorted.sort_by_key(|(_, view)| view.key());
        let expected: Vec<EventId> = sorted.iter().map(|(id, _)| *id).collect();
        let chunk: Vec<EventId> = b
            .document()
            .events(voice, bar)
            .unwrap()
            .iter()
            .map(|e| e.event().id())
            .collect();
        prop_assert_eq!(chunk, expected);

        // The oracle's validity.
        let mains: Vec<(i64, i64)> = oracle
            .iter()
            .filter_map(|(_, view)| view.length.map(|l| (view.at, view.at + l)))
            .collect();
        let graces_valid = oracle.iter().all(|(_, view)| match (view.length, view.rank) {
            (Some(_), _) => true,
            (None, rank) => {
                let inside = mains.iter().any(|&(s, e)| s < view.at && view.at < e);
                let has_main = if rank == 0 {
                    mains.iter().any(|&(_, e)| e == view.at)
                } else {
                    mains.iter().any(|&(s, _)| s == view.at)
                };
                !inside && has_main
            }
        });
        let slurs_valid = added
            .iter()
            .all(|&(_, i, j)| oracle[i].1.key() < oracle[j].1.key());

        // The slur index, before building (the builder maintains it as slurs are added).
        for (k, (event, _)) in oracle.iter().enumerate() {
            let expected: Vec<SlurId> = added
                .iter()
                .filter(|&&(_, i, j)| i == k || j == k)
                .map(|&(id, _, _)| id)
                .collect();
            prop_assert_eq!(b.document().slurs_at(*event), expected.as_slice());
        }

        // Score order on every pair.
        for (a, va) in &oracle {
            for (z, vz) in &oracle {
                prop_assert_eq!(
                    b.document().comes_before(*a, *z).unwrap(),
                    va.key() < vz.key(),
                    "comes_before({:?}, {:?})", a, z
                );
            }
        }

        // Graces last zero, main events their length.
        for (id, view) in &oracle {
            let expected = sixteenths(view.length.unwrap_or(0));
            prop_assert_eq!(b.document().metric_duration(*id).unwrap(), expected);
        }

        prop_assert_eq!(b.build().is_ok(), graces_valid && slurs_valid);
    }
}
