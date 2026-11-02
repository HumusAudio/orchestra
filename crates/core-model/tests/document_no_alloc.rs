// SPDX-License-Identifier: GPL-3.0-or-later

//! Every lookup of the document skeleton is allocation-free (ADR-0007 rule 1, ADR-0022): layout,
//! playback compile and the commands call them once per event or per bar, and none of them needs
//! the heap. Building the document does allocate (vectors, the bar index) and is not tested here.
//!
//! This file is its own test binary with the counting allocator installed (ADR-0016 §5). The
//! inputs go through `black_box` so that the compiler cannot evaluate the lookups at compile time
//! and leave nothing to measure.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use std::hint::black_box;

use orchestra_core_model::{
    Clef, Document, DocumentBuilder, Interval, Key, Mode, Staff, Track, Voice,
};
use orchestra_core_time::{
    BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo, TempoMap,
};
use orchestra_test_support::assert_no_allocations;

/// `n` treble clefs: the initial clefs of an instrument with `n` staves (the skeleton tests do not
/// care which clef each staff starts with, only how many staves there are).
fn clefs(n: usize) -> Vec<Clef> {
    vec![Clef::TREBLE; n]
}

/// C major: the initial key of every test document.
fn c_major() -> Key {
    Key::traditional(0, Mode::Major).unwrap()
}

orchestra_test_support::install_counting_allocator!();

/// A small orchestra-like document: 32 bars, a flute, a piano with an extra voice, an organ.
fn document() -> Document {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[3], 4).unwrap())).unwrap();
    let quarter = Duration::from_rational(Rational::new(1, 4).unwrap());
    let tempo = Tempo::new(quarter, Rational::from_integer(72).unwrap()).unwrap();
    let mut b = DocumentBuilder::new(meters, TempoMap::new(tempo).unwrap(), c_major());
    let unison = Interval::new(0, Rational::ZERO);
    for _ in 0..32 {
        b.add_bar().unwrap();
    }
    b.add_instrument_track("Flute", unison, &clefs(1)).unwrap();
    let piano = b.add_instrument_track("Piano", unison, &clefs(2)).unwrap();
    let staff = b.document().track(piano).unwrap().instrument().staves()[0].id();
    b.add_voice(staff).unwrap();
    b.add_instrument_track("Organ", unison, &clefs(3)).unwrap();
    b.build().unwrap()
}

/// Every lookup, on hits and on misses (an index past the end, IDs of the wrong instrument).
#[test]
fn lookups_do_not_allocate() {
    let document = black_box(document());
    let bars: Vec<_> = document.bars().ids().to_vec();
    let tracks: Vec<_> = document.tracks().iter().map(Track::id).collect();
    let instruments: Vec<_> = document
        .tracks()
        .iter()
        .map(|t| t.instrument().id())
        .collect();
    let staves: Vec<_> = document
        .tracks()
        .iter()
        .flat_map(|t| t.instrument().staves().iter().map(Staff::id))
        .collect();
    let voices: Vec<_> = document
        .tracks()
        .iter()
        .flat_map(|t| t.instrument().voices().iter().map(Voice::id))
        .collect();

    assert_no_allocations(|| {
        for &bar in &bars {
            let index = black_box(document.bars().index_of(black_box(bar)));
            black_box(index.and_then(|i| document.bars().id_at(i)));
            black_box(document.bar_start(bar)).unwrap();
        }
        black_box(document.bars().id_at(black_box(BarIndex::new(1000))));
        for &track in &tracks {
            black_box(document.track(black_box(track)));
        }
        for &instrument in &instruments {
            black_box(document.instrument(black_box(instrument)));
        }
        for &staff in &staves {
            black_box(document.staff(black_box(staff)));
        }
        for &voice in &voices {
            black_box(document.voice(black_box(voice)));
            // A miss: the flute does not own the other voices.
            black_box(document.tracks()[0].instrument().voice(voice));
        }
    });
}
