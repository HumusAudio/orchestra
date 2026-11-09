// SPDX-License-Identifier: GPL-3.0-or-later

//! Property test of the played notes (SCALETTA F1.13b, F1.13c): random scores (a repeated section,
//! notes and rests of random values filling the bars, random ties to the next note, articulations,
//! grace notes, arpeggios, performance offsets, dynamics, dynamic accents and hairpins), and the
//! invariants every performance keeps:
//! - the notes of an instrument are sorted by start, then by pitch;
//! - every note lasts at least 1/1024 of a whole note, and its velocity is in 1..=65 535;
//! - its attack profile is well formed (ADR-0027 §14, H14): no profile at all when the attack and
//!   the settling level are equal, otherwise a hold of 0, 0.2 s or 0.6 s and a settling time of
//!   at most 0.8 s (the times of §15, round 2);
//! - every note names a notehead the document has, at the pitch it sounds;
//! - without ties, grace notes, articulations, arpeggios and offsets, every written note is
//!   played once per play of its bar, exactly where it is written on the path, and (ADR-0027 H1) at
//!   the level of the last dynamic **played** at or before it along the path, by an oracle that
//!   walks the path backwards from the note (*mf* if there is none).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers over their own small tables: a failure fails the test"
)]

mod common;

use common::{Extras, LEVELS, random};
use orchestra_core_model::{Document, DynamicMark};
use orchestra_core_time::{Duration, Rational, Seconds};
use orchestra_playback::{Performance, Unrolled};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Every performance keeps the invariants of the module documentation.
    #[test]
    fn every_performance_keeps_its_invariants(
        n in 3_usize..8,
        seed in prop::collection::vec(any::<u8>(), 8..64),
        any_extra in any::<bool>(),
        dynamics in any::<bool>(),
    ) {
        let doc = random(n, &seed, Extras { any: any_extra, dynamics });
        let path = Unrolled::new(&doc).unwrap();
        let performance = Performance::new(&doc, &path).unwrap();
        let min = Duration::from_rational(Rational::new(1, 1024).unwrap());
        for (_, notes) in performance.instruments() {
            for pair in notes.windows(2) {
                prop_assert!((pair[0].start, pair[0].pitch) <= (pair[1].start, pair[1].pitch));
            }
            for note in notes {
                prop_assert!(note.end >= note.start.checked_add(min).unwrap());
                prop_assert!(note.velocity >= 1 && note.settle_to >= 1);
                if note.velocity == note.settle_to {
                    prop_assert!(note.hold == Seconds::ZERO && note.settle_in == Seconds::ZERO);
                } else {
                    prop_assert!([Seconds::ZERO, secs(1, 5), secs(3, 5)].contains(&note.hold));
                    prop_assert!(note.settle_in <= secs(4, 5));
                }
                let (_, written) = doc.note(note.note).unwrap();
                prop_assert_eq!(written.pitch().sounding().unwrap(), note.pitch);
            }
            if !any_extra {
                // One played note per written note per play of its bar, where it is written.
                let mut expected = 0_usize;
                for (k, played) in path.played().iter().enumerate() {
                    let voice = doc.tracks()[0].instrument().voices()[0].id();
                    for placed in doc.events(voice, played.bar).unwrap() {
                        if placed.event().chord().is_some() {
                            expected += 1;
                            let start = played.start.checked_add(placed.offset()).unwrap();
                            let found = notes
                                .iter()
                                .find(|n| n.start == start && n.played == u32::try_from(k).unwrap());
                            prop_assert!(found.is_some(), "no note at {:?}", start);
                            let level = oracle_level(&doc, &path, k, placed.offset());
                            prop_assert_eq!(found.unwrap().velocity, up(level));
                        }
                    }
                }
                prop_assert_eq!(notes.len(), expected);
            }
        }
    }
}

/// `n/den` seconds.
fn secs(n: i64, den: i64) -> Seconds {
    Seconds::from_rational(Rational::new(n, den).unwrap())
}

/// The 16-bit velocity of an integer level of the 0–127 scale (ADR-0027 §4: bit replication).
fn up(v: u16) -> u16 {
    (v << 9) | (v << 2) | (v >> 5)
}

/// The oracle of ADR-0027 H1 for the plain scores (levels only): the level of the last dynamic
/// played at or before the point `offset` of the played bar `k`, found by walking the path
/// **backwards** from the note, bar by bar, and taking in each played bar the dynamic with the
/// largest offset that is not after the note; *mf* (75) if there is none.
fn oracle_level(doc: &Document, path: &Unrolled, k: usize, offset: Duration) -> u16 {
    let dynamics = doc.tracks()[0].instrument().dynamics();
    for j in (0..=k).rev() {
        let bar = path.played()[j].bar;
        let best = dynamics
            .iter()
            .filter(|d| d.at().bar == bar && (j < k || d.at().offset <= offset))
            .max_by_key(|d| d.at().offset);
        if let Some(d) = best {
            let DynamicMark::Level(level) = d.mark() else {
                continue;
            };
            return LEVELS.iter().find(|(l, _)| l == level).unwrap().1;
        }
    }
    75
}
