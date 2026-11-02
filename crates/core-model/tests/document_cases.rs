// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases for the document skeleton (ADR-0022 §1–3, SCALETTA F1.08): IDs, instruments
//! with their staves and voices, the bar list, the limits and the typed errors.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    BarId, DocumentBuilder, DocumentError, Interval, MAX_STAVES_PER_INSTRUMENT,
    MAX_VOICES_PER_INSTRUMENT, Quality,
};
use orchestra_core_time::{
    BarIndex, BarKind, Duration, IrregularBar, Meter, MeterChange, MeterMap, Position, Rational,
    Tempo, TempoMap,
};

/// The rational `n/d`.
fn q(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// A builder in 4/4 at ♩ = 120, with a pickup of one quarter in bar 0 when `pickup` is set.
fn builder(pickup: bool) -> DocumentBuilder {
    let mut meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    if pickup {
        meters
            .set_irregular_bar(
                BarIndex::FIRST,
                IrregularBar {
                    length: Duration::from_rational(q(1, 4)),
                    kind: BarKind::AlignEnd,
                },
            )
            .unwrap();
    }
    let tempo = Tempo::new(Duration::from_rational(q(1, 4)), q(120, 1)).unwrap();
    DocumentBuilder::new(meters, TempoMap::new(tempo).unwrap())
}

/// The unison: the transposition of an instrument at concert pitch.
fn unison() -> Interval {
    Interval::new(0, Rational::ZERO)
}

/// A new document has no bars and no tracks, and keeps the time maps it was given.
#[test]
fn empty_document() {
    let b = builder(false);
    let meters = b.document().meter_map().clone();
    let document = b.build();
    assert!(document.bars().is_empty());
    assert_eq!(document.bars().len(), 0);
    assert_eq!(document.tracks(), &[]);
    assert_eq!(document.meter_map(), &meters);
    assert_eq!(document.bars().id_at(BarIndex::FIRST), None);
}

/// IDs come from one counter starting at 1, in creation order, across every kind.
#[test]
fn ids_are_sequential_across_kinds() {
    let mut b = builder(false);
    let bar0 = b.add_bar().unwrap();
    let piano = b.add_instrument_track("Piano", unison(), 2).unwrap();
    let bar1 = b.add_bar().unwrap();
    let document = b.build();
    assert_eq!(bar0.get(), 1);
    // Track 2, instrument 3, staves 4 and 5, voices 6 and 7.
    let track = document.track(piano).unwrap();
    let instrument = track.instrument();
    assert_eq!(track.id().get(), 2);
    assert_eq!(instrument.id().get(), 3);
    let staves: Vec<u64> = instrument.staves().iter().map(|s| s.id().get()).collect();
    let voices: Vec<u64> = instrument.voices().iter().map(|v| v.id().get()).collect();
    assert_eq!(staves, [4, 5]);
    assert_eq!(voices, [6, 7]);
    assert_eq!(bar1.get(), 8);
}

/// A piano gets one voice per staff, each at home on its own staff, top to bottom.
#[test]
fn one_voice_per_staff_at_creation() {
    let mut b = builder(false);
    let piano = b.add_instrument_track("Piano", unison(), 2).unwrap();
    let document = b.build();
    let instrument = document.track(piano).unwrap().instrument();
    assert_eq!(instrument.name(), "Piano");
    assert_eq!(instrument.staves().len(), 2);
    assert_eq!(instrument.voices().len(), 2);
    for (voice, staff) in instrument.voices().iter().zip(instrument.staves()) {
        assert_eq!(voice.home_staff(), staff.id());
    }
}

/// More voices can be added on any staff of the instrument; the document finds every object by
/// ID, together with its instrument.
#[test]
fn extra_voices_and_lookups() {
    let mut b = builder(false);
    let flute = b.add_instrument_track("Flute", unison(), 1).unwrap();
    let piano = b.add_instrument_track("Piano", unison(), 2).unwrap();
    let upper = b.document().track(piano).unwrap().instrument().staves()[0].id();
    let third = b.add_voice(upper).unwrap();
    let fourth = b.add_voice(upper).unwrap();
    let document = b.build();

    let instrument = document.track(piano).unwrap().instrument();
    assert_eq!(instrument.voices().len(), 4);
    let (owner, voice) = document.voice(third).unwrap();
    assert_eq!(owner.id(), instrument.id());
    assert_eq!(voice.home_staff(), upper);
    assert_eq!(document.voice(fourth).unwrap().1.home_staff(), upper);
    assert_eq!(document.staff(upper).unwrap().0.id(), instrument.id());
    assert_eq!(
        document.instrument(instrument.id()).unwrap().name(),
        "Piano"
    );
    // The flute's objects are found on the flute, not on the piano.
    let flute_instrument = document.track(flute).unwrap().instrument();
    let flute_voice = flute_instrument.voices()[0].id();
    assert_eq!(
        document.voice(flute_voice).unwrap().0.id(),
        flute_instrument.id()
    );
    // An instrument only answers for its own staves and voices.
    assert!(flute_instrument.voice(third).is_none());
    assert!(flute_instrument.staff(upper).is_none());
}

/// A transposing instrument keeps its written-to-sounding interval (ADR-0023 §7).
#[test]
fn transposition_is_kept() {
    let mut b = builder(false);
    let major_second_down = Interval::from_quality(Quality::Major, 2, true).unwrap();
    let clarinet = b
        .add_instrument_track("Clarinet in B♭", major_second_down, 1)
        .unwrap();
    let document = b.build();
    let instrument = document.track(clarinet).unwrap().instrument();
    assert_eq!(instrument.transposition(), major_second_down);
    assert_eq!(instrument.name(), "Clarinet in B♭");
}

/// The bar list maps IDs to indices and back; bar starts come from the meter map (here with a
/// one-quarter pickup, so bar 1 starts at 1/4 and bar 2 at 5/4).
#[test]
fn bar_list_and_bar_starts() {
    let mut b = builder(true);
    let bars: Vec<BarId> = (0..3).map(|_| b.add_bar().unwrap()).collect();
    let document = b.build();
    assert_eq!(document.bars().ids(), bars.as_slice());
    for (i, &id) in bars.iter().enumerate() {
        let index = BarIndex::new(u32::try_from(i).unwrap());
        assert_eq!(document.bars().index_of(id), Some(index));
        assert_eq!(document.bars().id_at(index), Some(id));
    }
    assert_eq!(document.bars().id_at(BarIndex::new(3)), None);
    assert_eq!(document.bar_start(bars[0]).unwrap(), Position::ZERO);
    assert_eq!(
        document.bar_start(bars[1]).unwrap(),
        Position::from_rational(q(1, 4))
    );
    assert_eq!(
        document.bar_start(bars[2]).unwrap(),
        Position::from_rational(q(5, 4))
    );
}

/// An ID of another document (or of another kind of object) is not a bar of this one.
#[test]
fn unknown_bar() {
    let mut other = builder(false);
    other.add_bar().unwrap();
    let foreign = other.add_bar().unwrap();
    let mut b = builder(false);
    b.add_bar().unwrap();
    let document = b.build();
    assert_eq!(document.bars().index_of(foreign), None);
    assert_eq!(
        document.bar_start(foreign),
        Err(DocumentError::UnknownBar(foreign))
    );
}

/// Staff limits: none is refused, 16 is accepted, 17 is refused; refusals add nothing and
/// consume no ID.
#[test]
fn staff_limits() {
    let mut b = builder(false);
    assert_eq!(
        b.add_instrument_track("Nothing", unison(), 0),
        Err(DocumentError::NoStaves)
    );
    assert_eq!(
        b.add_instrument_track("Too many", unison(), MAX_STAVES_PER_INSTRUMENT + 1),
        Err(DocumentError::TooManyStaves)
    );
    assert_eq!(b.document().tracks(), &[]);
    let wide = b
        .add_instrument_track("Wide", unison(), MAX_STAVES_PER_INSTRUMENT)
        .unwrap();
    // No ID was consumed by the refusals: the track took the very first one.
    assert_eq!(wide.get(), 1);
    let document = b.build();
    let instrument = document.track(wide).unwrap().instrument();
    assert_eq!(instrument.staves().len(), MAX_STAVES_PER_INSTRUMENT);
    assert_eq!(instrument.voices().len(), MAX_STAVES_PER_INSTRUMENT);
}

/// Voice limit: 256 voices per instrument, the 257th is refused without consuming an ID; other
/// instruments are not affected.
#[test]
fn voice_limit() {
    let mut b = builder(false);
    let piano = b.add_instrument_track("Piano", unison(), 2).unwrap();
    let flute = b.add_instrument_track("Flute", unison(), 1).unwrap();
    let staff = b.document().track(piano).unwrap().instrument().staves()[1].id();
    let flute_staff = b.document().track(flute).unwrap().instrument().staves()[0].id();
    // The piano starts with two voices.
    for _ in 2..MAX_VOICES_PER_INSTRUMENT {
        b.add_voice(staff).unwrap();
    }
    let before = b.document().clone();
    assert_eq!(b.add_voice(staff), Err(DocumentError::TooManyVoices));
    assert_eq!(b.document(), &before);
    // The next ID handed out is the one right after the last voice.
    let last = before
        .track(piano)
        .unwrap()
        .instrument()
        .voices()
        .last()
        .unwrap()
        .id();
    let flute_voice = b.add_voice(flute_staff).unwrap();
    assert_eq!(flute_voice.get(), last.get() + 1);
    let document = b.build();
    assert_eq!(
        document.track(piano).unwrap().instrument().voices().len(),
        MAX_VOICES_PER_INSTRUMENT
    );
}

/// A voice cannot be added to a staff that does not exist in the document.
#[test]
fn unknown_staff() {
    let mut other = builder(false);
    let foreign_track = other.add_instrument_track("Oboe", unison(), 1).unwrap();
    // Push the other document's counter forward, so its staff ID is unused in `b`.
    for _ in 0..10 {
        other.add_bar().unwrap();
    }
    let foreign_staff = other
        .document()
        .track(foreign_track)
        .unwrap()
        .instrument()
        .staves()[0]
        .id();
    let mut b = builder(false);
    b.add_bar().unwrap();
    assert_eq!(
        b.add_voice(foreign_staff),
        Err(DocumentError::UnknownStaff(foreign_staff))
    );
}

/// Two builders given the same calls produce identical documents, IDs included.
#[test]
fn building_is_deterministic() {
    let make = || {
        let mut b = builder(true);
        for _ in 0..4 {
            b.add_bar().unwrap();
        }
        let piano = b.add_instrument_track("Piano", unison(), 2).unwrap();
        let staff = b.document().track(piano).unwrap().instrument().staves()[0].id();
        b.add_voice(staff).unwrap();
        b.build()
    };
    assert_eq!(make(), make());
}

/// The errors print an English description for developers and expose their source.
#[test]
fn error_display_and_source() {
    use std::error::Error;
    assert_eq!(
        DocumentError::TooManyVoices.to_string(),
        "an instrument can have at most 256 voices"
    );
    assert_eq!(
        DocumentError::TooManyStaves.to_string(),
        "an instrument can have at most 16 staves"
    );
    assert!(DocumentError::NoStaves.source().is_none());
}
