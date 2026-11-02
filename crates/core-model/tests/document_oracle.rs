// SPDX-License-Identifier: GPL-3.0-or-later

//! Property tests of the document builder against an independent oracle (ADR-0016 §4).
//!
//! # The oracle
//! A deliberately naive model written here, sharing no code with the library: a plain `u64`
//! counter, a vector of bar IDs, and one record per track listing its staff and voice IDs as
//! numbers. It applies the rules of ADR-0022 §1–3 and of the builder's documentation in the most
//! literal way (IDs in creation order, one voice per staff at creation, limits checked before any
//! ID is taken). Random sequences of builder calls are applied to both, and after every call the
//! result and the whole document must match the oracle.
//!
//! # Invariants checked on top
//! - Every ID of the document is distinct, across all kinds (one counter, never reused).
//! - The bar list's index is the exact inverse of its order.
//! - Every voice's home staff belongs to its own instrument, and every lookup by ID finds the
//!   object on the instrument the oracle says.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    BarId, Clef, Document, DocumentBuilder, DocumentError, Instrument, Interval, Key,
    MAX_STAVES_PER_INSTRUMENT, MAX_VOICES_PER_INSTRUMENT, Mode, StaffId, Track, TrackId, VoiceId,
};
use orchestra_core_time::{
    BarIndex, Duration, Meter, MeterChange, MeterMap, Rational, Tempo, TempoMap,
};
use proptest::prelude::*;

/// `n` treble clefs: the initial clefs of an instrument with `n` staves (the skeleton tests do not
/// care which clef each staff starts with, only how many staves there are).
fn clefs(n: usize) -> Vec<Clef> {
    vec![Clef::TREBLE; n]
}

/// C major: the initial key of every test document.
fn c_major() -> Key {
    Key::traditional(0, Mode::Major).unwrap()
}

/// One builder call, with its arguments chosen by proptest.
#[derive(Debug, Clone)]
enum Op {
    /// `add_bar()`.
    Bar,
    /// `add_instrument_track(_, _, &clefs(staves))`; 0 and 17 are included to hit the errors.
    Track(usize),
    /// `add_voice(staff)`, the staff picked as (track, staff) by index modulo the counts, or a
    /// staff ID that does not exist when there are no tracks.
    Voice(usize, usize),
}

/// The oracle's record of one instrument track: the raw IDs, in the builder's order.
#[derive(Debug, Clone)]
struct OracleTrack {
    /// The track's ID.
    track: u64,
    /// The instrument's ID.
    instrument: u64,
    /// The staves' IDs, top to bottom.
    staves: Vec<u64>,
    /// The voices as (ID, home staff ID), in creation order.
    voices: Vec<(u64, u64)>,
}

/// The naive model of the document skeleton.
#[derive(Debug, Default)]
struct Oracle {
    /// The last ID handed out (0: none yet).
    last_id: u64,
    /// The bars' IDs in order.
    bars: Vec<u64>,
    /// The tracks in order.
    tracks: Vec<OracleTrack>,
}

impl Oracle {
    /// The next ID of the single counter.
    fn take(&mut self) -> u64 {
        self.last_id += 1;
        self.last_id
    }
}

/// The empty builder every case starts from (the time maps do not matter here).
fn builder() -> DocumentBuilder {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let quarter = Duration::from_rational(Rational::new(1, 4).unwrap());
    let tempo = Tempo::new(quarter, Rational::from_integer(60).unwrap()).unwrap();
    DocumentBuilder::new(meters, TempoMap::new(tempo).unwrap(), c_major())
}

/// Applies `op` to both, and checks the result of the call against the oracle's expectation.
fn apply(builder: &mut DocumentBuilder, oracle: &mut Oracle, op: &Op) {
    match *op {
        Op::Bar => {
            let got = builder.add_bar().map(BarId::get);
            let expected = oracle.take();
            oracle.bars.push(expected);
            assert_eq!(got, Ok(expected));
        }
        Op::Track(staves) => {
            let got = builder
                .add_instrument_track("I", Interval::new(0, Rational::ZERO), &clefs(staves))
                .map(TrackId::get);
            if staves == 0 {
                assert_eq!(got, Err(DocumentError::NoStaves));
                return;
            }
            if staves > MAX_STAVES_PER_INSTRUMENT {
                assert_eq!(got, Err(DocumentError::TooManyStaves));
                return;
            }
            let track = oracle.take();
            let instrument = oracle.take();
            let staff_ids: Vec<u64> = (0..staves).map(|_| oracle.take()).collect();
            let voices = staff_ids.iter().map(|&s| (oracle.take(), s)).collect();
            oracle.tracks.push(OracleTrack {
                track,
                instrument,
                staves: staff_ids,
                voices,
            });
            assert_eq!(got, Ok(track));
        }
        Op::Voice(t, s) => {
            if oracle.tracks.is_empty() {
                // No staff exists: any staff ID is foreign to this document.
                let foreign = staff_id_not_in(builder.document());
                assert_eq!(
                    builder.add_voice(foreign),
                    Err(DocumentError::UnknownStaff(foreign))
                );
                return;
            }
            let t = t % oracle.tracks.len();
            let s = s % oracle.tracks[t].staves.len();
            let staff_raw = oracle.tracks[t].staves[s];
            let staff = builder.document().tracks()[t].instrument().staves()[s].id();
            assert_eq!(staff.get(), staff_raw);
            let got = builder.add_voice(staff).map(VoiceId::get);
            if oracle.tracks[t].voices.len() >= MAX_VOICES_PER_INSTRUMENT {
                assert_eq!(got, Err(DocumentError::TooManyVoices));
                return;
            }
            let id = oracle.take();
            oracle.tracks[t].voices.push((id, staff_raw));
            assert_eq!(got, Ok(id));
        }
    }
}

/// A `StaffId` that exists in no instrument of `document`: built in a scratch builder whose
/// counter has been pushed past every ID of `document`.
fn staff_id_not_in(document: &Document) -> StaffId {
    let mut scratch = builder();
    // Bars take one ID each; push the scratch counter past the document's bars.
    for _ in 0..=document.bars().len() {
        scratch.add_bar().unwrap();
    }
    let track = scratch
        .add_instrument_track("X", Interval::new(0, Rational::ZERO), &clefs(1))
        .unwrap();
    scratch
        .document()
        .track(track)
        .unwrap()
        .instrument()
        .staves()[0]
        .id()
}

/// Compares the whole document with the oracle, and checks the invariants of the module docs.
fn check(document: &Document, oracle: &Oracle) {
    // The bar list, in both directions.
    let bars: Vec<u64> = document.bars().ids().iter().map(|b| b.get()).collect();
    assert_eq!(bars, oracle.bars);
    for (i, &bar) in document.bars().ids().iter().enumerate() {
        let index = BarIndex::new(u32::try_from(i).unwrap());
        assert_eq!(document.bars().index_of(bar), Some(index));
        assert_eq!(document.bars().id_at(index), Some(bar));
    }
    // The tracks, their instruments, staves and voices.
    assert_eq!(document.tracks().len(), oracle.tracks.len());
    let mut all_ids = bars.clone();
    for (track, expected) in document.tracks().iter().zip(&oracle.tracks) {
        let instrument = track.instrument();
        assert_eq!(track.id().get(), expected.track);
        assert_eq!(instrument.id().get(), expected.instrument);
        let staves: Vec<u64> = instrument.staves().iter().map(|s| s.id().get()).collect();
        assert_eq!(staves, expected.staves);
        let voices: Vec<(u64, u64)> = instrument
            .voices()
            .iter()
            .map(|v| (v.id().get(), v.home_staff().get()))
            .collect();
        assert_eq!(voices, expected.voices);
        // Lookups by ID land on this instrument.
        assert_eq!(document.track(track.id()).map(Track::id), Some(track.id()));
        assert_eq!(
            document.instrument(instrument.id()).map(Instrument::id),
            Some(instrument.id())
        );
        for staff in instrument.staves() {
            assert_eq!(
                document.staff(staff.id()).map(|(i, _)| i.id()),
                Some(instrument.id())
            );
        }
        for voice in instrument.voices() {
            assert_eq!(
                document.voice(voice.id()).map(|(i, _)| i.id()),
                Some(instrument.id())
            );
            assert!(instrument.staff(voice.home_staff()).is_some());
        }
        all_ids.extend([expected.track, expected.instrument]);
        all_ids.extend(&expected.staves);
        all_ids.extend(expected.voices.iter().map(|&(v, _)| v));
    }
    // One counter, never reused: every ID is distinct.
    let count = all_ids.len();
    all_ids.sort_unstable();
    all_ids.dedup();
    assert_eq!(all_ids.len(), count);
}

/// The strategy for one builder call: bars and voices more often than tracks, staff counts
/// including the invalid 0 and 17.
fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => Just(Op::Bar),
        1 => (0..=MAX_STAVES_PER_INSTRUMENT + 1).prop_map(Op::Track),
        2 => (any::<usize>(), any::<usize>()).prop_map(|(t, s)| Op::Voice(t, s)),
    ]
}

proptest! {
    /// Any sequence of builder calls agrees with the oracle after every call.
    #[test]
    fn builder_matches_oracle(ops in prop::collection::vec(op(), 0..120)) {
        let mut b = builder();
        let mut oracle = Oracle::default();
        for op in &ops {
            apply(&mut b, &mut oracle, op);
            check(b.document(), &oracle);
        }
        let document = b.build().unwrap();
        check(&document, &oracle);
    }
}
