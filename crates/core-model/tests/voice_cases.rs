// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases for the content of the voices (ADR-0024 §1–§4 and §9, SCALETTA F1.09b):
//! note values and their derived durations, chords, tuplets (nested, incomplete, across a
//! barline), ties, whole-bar rests, overfull bars, clefs and key signatures, and the typed errors
//! of the builder and of `validate`. Each case of ADR-0024 §13 that belongs to this step has its
//! own test, named after it.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    Alteration, BarId, BaseValue, Clef, ClefSign, Document, DocumentBuilder, DocumentError,
    EventBody, Interval, Key, KeyOverride, KeySignature, MAX_TUPLET_DEPTH, Metronome, Mode,
    NoteValue, NoteValueError, Pitch, Placement, Quality, RestValue, Step, TempoMark, Tie, Tuplet,
    TupletBracket, TupletDisplay, TupletNumber, Violation, VoiceId,
};
use orchestra_core_time::{
    BarIndex, BarKind, Duration, IrregularBar, Meter, MeterChange, MeterMap, Position, Rational,
    Tempo,
};

/// The rational `n/d`.
fn q(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// The duration `n/d` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(q(n, den))
}

/// An undotted value.
fn v(base: BaseValue) -> NoteValue {
    NoteValue::plain(base)
}

/// A natural pitch.
fn nat(step: Step, octave: i8) -> Pitch {
    Pitch::new(step, Alteration::NATURAL, octave)
}

/// The unison: the transposition of an instrument at concert pitch.
fn unison() -> Interval {
    Interval::new(0, Rational::ZERO)
}

/// A key in major mode.
fn major(fifths: i8) -> Key {
    Key::traditional(fifths, Mode::Major).unwrap()
}

/// A builder in 4/4 at ♩ = 60, in C major, with `bars` bars.
fn builder_with(meters: MeterMap, bars: usize) -> (DocumentBuilder, Vec<BarId>) {
    let tempo = Tempo::new(d(1, 4), q(60, 1)).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        major(0),
    )
    .unwrap();
    let ids = (0..bars).map(|_| b.add_bar().unwrap()).collect();
    (b, ids)
}

/// 4/4.
fn four_four() -> MeterMap {
    MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap()
}

/// A builder in 4/4 with `bars` bars and one flute: (builder, bars, the flute's voice).
fn flute(bars: usize) -> (DocumentBuilder, Vec<BarId>, VoiceId) {
    let (mut b, ids) = builder_with(four_four(), bars);
    let track = b
        .add_instrument_track("Flute", unison(), &[Clef::TREBLE])
        .unwrap();
    let voice = b.document().track(track).unwrap().instrument().voices()[0].id();
    (b, ids, voice)
}

/// The metric duration of every event of `voice` in `bar`, in order.
fn durations(document: &Document, voice: VoiceId, bar: BarId) -> Vec<Duration> {
    document
        .events(voice, bar)
        .unwrap()
        .iter()
        .map(|e| document.metric_duration(e.event().id()).unwrap())
        .collect()
}

/// Every base value lasts 2^(−exponent) whole notes, and dots add half, a quarter, … of it.
#[test]
fn note_value_lengths() {
    assert_eq!(v(BaseValue::Maxima).length().unwrap(), d(8, 1));
    assert_eq!(v(BaseValue::Breve).length().unwrap(), d(2, 1));
    assert_eq!(v(BaseValue::Whole).length().unwrap(), d(1, 1));
    assert_eq!(v(BaseValue::Quarter).length().unwrap(), d(1, 4));
    assert_eq!(
        v(BaseValue::OneThousandTwentyFourth).length().unwrap(),
        d(1, 1024)
    );
    let dotted = |base, dots| NoteValue::new(base, dots).unwrap().length().unwrap();
    assert_eq!(dotted(BaseValue::Quarter, 1), d(3, 8));
    assert_eq!(dotted(BaseValue::Quarter, 2), d(7, 16));
    assert_eq!(dotted(BaseValue::Half, 3), d(15, 16));
    assert_eq!(dotted(BaseValue::Maxima, 4), d(31, 2));
    assert_eq!(dotted(BaseValue::OneThousandTwentyFourth, 4), d(31, 16_384));
    assert_eq!(
        NoteValue::new(BaseValue::Quarter, 5),
        Err(NoteValueError::TooManyDots)
    );
    // The table of base values is complete and ordered, longest first.
    for pair in BaseValue::ALL.windows(2) {
        assert_eq!(pair[0].exponent() + 1, pair[1].exponent());
    }
}

/// ADR-0024 §13, enharmonic spelling: F♯4 and G♭4 in one chord stay two noteheads with their own
/// IDs and spellings, sorted by sound then by letter; the returned IDs follow the given order.
#[test]
fn enharmonic_chord_keeps_two_noteheads() {
    let (mut b, bars, voice) = flute(1);
    let f_sharp = Pitch::new(Step::F, Alteration::SHARP, 4);
    let g_flat = Pitch::new(Step::G, Alteration::FLAT, 4);
    let c4 = nat(Step::C, 4);
    let ids = b
        .add_chord(
            Placement::new(voice, bars[0], Duration::ZERO),
            v(BaseValue::Whole),
            &[g_flat, c4, f_sharp],
        )
        .unwrap();
    let document = b.build().unwrap();
    assert_eq!(ids.notes.len(), 3);
    let chord = document.event(ids.event).unwrap().event().chord().unwrap();
    let pitches: Vec<Pitch> = chord
        .notes()
        .map(orchestra_core_model::Note::pitch)
        .collect();
    assert_eq!(pitches, [c4, f_sharp, g_flat]);
    assert_eq!(chord.len(), 3);
    assert!(!chord.is_empty());
    // The IDs in the chord's order are the given ones permuted: C4 was given second.
    let in_chord: Vec<_> = chord.notes().map(orchestra_core_model::Note::id).collect();
    assert_eq!(in_chord, [ids.notes[1], ids.notes[2], ids.notes[0]]);
    // Each notehead is found by its ID, with its own spelling.
    assert_eq!(document.note(ids.notes[0]).unwrap().1.pitch(), g_flat);
    assert_eq!(document.note(ids.notes[2]).unwrap().1.pitch(), f_sharp);
    // IDs: the event first, then the noteheads in the given order.
    assert_eq!(ids.notes[0].get(), ids.event.get() + 1);
    assert_eq!(ids.notes[2].get(), ids.event.get() + 3);
}

/// ADR-0024 §13, tuplets: a triplet eighth lasts 1/12, a sixteenth of a 3:2 inside a 5:4 lasts
/// 1/30, and the tuplet's total and display are kept.
#[test]
fn tuplet_durations_nested() {
    let (mut b, bars, voice) = flute(1);
    let display = TupletDisplay {
        bracket: TupletBracket::Shown,
        number: TupletNumber::Ratio,
    };
    let triplet = b
        .add_tuplet(
            voice,
            Tuplet::new(3, 2, v(BaseValue::Eighth))
                .unwrap()
                .with_display(display),
            None,
        )
        .unwrap();
    let quintuplet = b
        .add_tuplet(
            voice,
            Tuplet::new(5, 4, v(BaseValue::Eighth)).unwrap(),
            None,
        )
        .unwrap();
    let inner = b
        .add_tuplet(
            voice,
            Tuplet::new(3, 2, v(BaseValue::Sixteenth)).unwrap(),
            Some(quintuplet),
        )
        .unwrap();
    let a4 = nat(Step::A, 4);
    // Three triplet eighths: 0, 1/12, 2/12.
    let mut ids = Vec::new();
    for i in 0..3 {
        let at = Placement::new(voice, bars[0], d(i, 12)).in_tuplet(triplet);
        ids.push(b.add_chord(at, v(BaseValue::Eighth), &[a4]).unwrap().event);
    }
    // One sixteenth of the inner triplet at 1/4.
    let at = Placement::new(voice, bars[0], d(1, 4)).in_tuplet(inner);
    let nested = b
        .add_chord(at, v(BaseValue::Sixteenth), &[a4])
        .unwrap()
        .event;
    let document = b.build().unwrap();

    for &id in &ids {
        assert_eq!(document.metric_duration(id).unwrap(), d(1, 12));
    }
    assert_eq!(document.metric_duration(nested).unwrap(), d(1, 30));
    let (_, voice) = document.voice(voice).unwrap();
    assert_eq!(voice.tuplet_count(), 3);
    let t = voice.tuplet(triplet).unwrap();
    assert_eq!((t.actual(), t.normal(), t.parent()), (3, 2, None));
    assert_eq!(t.total().unwrap(), d(1, 4));
    assert_eq!(t.display(), display);
    assert_eq!(voice.tuplet(inner).unwrap().parent(), Some(quintuplet));
    assert_eq!(
        Tuplet::new(0, 2, v(BaseValue::Eighth)),
        Err(NoteValueError::ZeroTupletTerm)
    );
}

/// ADR-0024 §3: an incomplete tuplet (one member of three) is a valid document.
#[test]
fn incomplete_tuplet_is_valid() {
    let (mut b, bars, voice) = flute(1);
    let triplet = b
        .add_tuplet(
            voice,
            Tuplet::new(3, 2, v(BaseValue::Eighth)).unwrap(),
            None,
        )
        .unwrap();
    let at = Placement::new(voice, bars[0], Duration::ZERO).in_tuplet(triplet);
    b.add_chord(at, v(BaseValue::Eighth), &[nat(Step::C, 5)])
        .unwrap();
    assert!(b.build().is_ok());
}

/// ADR-0024 §3, C4 (X2): a quarter-note triplet across a barline. Its middle note would straddle
/// the barline, so it is two tied triplet eighths, members of the same tuplet in two bars.
#[test]
fn tuplet_across_a_barline_with_a_tied_split() {
    let (mut b, bars, voice) = flute(2);
    let triplet = b
        .add_tuplet(
            voice,
            Tuplet::new(3, 2, v(BaseValue::Quarter)).unwrap(),
            None,
        )
        .unwrap();
    let g4 = nat(Step::G, 4);
    let in_triplet = |bar, offset| Placement::new(voice, bar, offset).in_tuplet(triplet);
    // Beat 4 of bar 0 (3/4): a triplet quarter, 1/6, ending at 11/12.
    let first = b
        .add_chord(in_triplet(bars[0], d(3, 4)), v(BaseValue::Quarter), &[g4])
        .unwrap();
    // The middle triplet quarter, split at the barline: a triplet eighth (1/12) in each bar.
    let before = b
        .add_chord(in_triplet(bars[0], d(11, 12)), v(BaseValue::Eighth), &[g4])
        .unwrap();
    let after = b
        .add_chord(
            in_triplet(bars[1], Duration::ZERO),
            v(BaseValue::Eighth),
            &[g4],
        )
        .unwrap();
    b.set_tie(before.notes[0], Tie::To(after.notes[0])).unwrap();
    // The last triplet quarter at 1/12 of bar 1.
    let last = b
        .add_chord(in_triplet(bars[1], d(1, 12)), v(BaseValue::Quarter), &[g4])
        .unwrap();
    let document = b.build().unwrap();
    let total: Rational = [first.event, before.event, after.event, last.event]
        .iter()
        .map(|&e| document.metric_duration(e).unwrap().as_rational())
        .fold(Rational::ZERO, |a, x| a.checked_add(x).unwrap());
    // The four members fill the tuplet's total: 2 quarters.
    assert_eq!(total, q(1, 2));
    assert_eq!(
        document.event_position(last.event).unwrap(),
        Position::from_rational(q(13, 12))
    );
}

/// ADR-0024 §13, overfull bar: five quarters in a 4/4 bar are a valid document; every event
/// keeps its offset and its duration.
#[test]
fn overfull_bar_is_valid() {
    let (mut b, bars, voice) = flute(2);
    let e5 = nat(Step::E, 5);
    for i in 0..5 {
        b.add_chord(
            Placement::new(voice, bars[0], d(i, 4)),
            v(BaseValue::Quarter),
            &[e5],
        )
        .unwrap();
    }
    // The next bar starts at 1 as usual, although bar 0's content reaches 5/4.
    b.add_chord(
        Placement::new(voice, bars[1], Duration::ZERO),
        v(BaseValue::Whole),
        &[e5],
    )
    .unwrap();
    let document = b.build().unwrap();
    assert_eq!(durations(&document, voice, bars[0]), vec![d(1, 4); 5]);
    let offsets: Vec<_> = document
        .events(voice, bars[0])
        .unwrap()
        .iter()
        .map(orchestra_core_model::PlacedEvent::offset)
        .collect();
    assert_eq!(offsets, [d(0, 1), d(1, 4), d(1, 2), d(3, 4), d(1, 1)]);
}

/// ADR-0024 §13, whole-bar rest: it lasts its bar's length, in a pickup bar, a normal bar and
/// after a meter change.
#[test]
fn whole_bar_rest_follows_the_meter() {
    let mut meters = four_four();
    meters
        .set_irregular_bar(
            BarIndex::FIRST,
            IrregularBar {
                length: d(1, 4),
                kind: BarKind::AlignEnd,
            },
        )
        .unwrap();
    meters
        .set_meter_change(
            BarIndex::new(2),
            MeterChange::new(Meter::new(&[3], 8).unwrap()),
        )
        .unwrap();
    let (mut b, bars) = builder_with(meters, 3);
    let track = b
        .add_instrument_track("Flute", unison(), &[Clef::TREBLE])
        .unwrap();
    let voice = b.document().track(track).unwrap().instrument().voices()[0].id();
    let rests: Vec<_> = bars
        .iter()
        .map(|&bar| {
            b.add_rest(
                Placement::new(voice, bar, Duration::ZERO),
                RestValue::WholeBar,
            )
            .unwrap()
        })
        .collect();
    let document = b.build().unwrap();
    let lengths: Vec<_> = rests
        .iter()
        .map(|&r| document.metric_duration(r).unwrap())
        .collect();
    assert_eq!(lengths, [d(1, 4), d(1, 1), d(3, 8)]);
}

/// ADR-0024 §2: a whole-bar rest shares its bar with nothing, and is never in a tuplet.
#[test]
fn whole_bar_rest_rules() {
    let (mut b, bars, voice) = flute(1);
    let rest = b
        .add_rest(
            Placement::new(voice, bars[0], Duration::ZERO),
            RestValue::WholeBar,
        )
        .unwrap();
    let triplet = b
        .add_tuplet(
            voice,
            Tuplet::new(3, 2, v(BaseValue::Eighth)).unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(
        b.add_rest(
            Placement::new(voice, bars[0], Duration::ZERO).in_tuplet(triplet),
            RestValue::WholeBar
        ),
        Err(DocumentError::WholeBarRestInTuplet)
    );
    b.add_rest(
        Placement::new(voice, bars[0], d(1, 2)),
        RestValue::Value(v(BaseValue::Half)),
    )
    .unwrap();
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::WholeBarRestNotAlone(
            rest
        )))
    );
}

/// ADR-0024 §1, voice invariant: a gap is fine, an overlap is reported with the later event.
#[test]
fn overlap_within_a_voice_is_invalid() {
    let (mut b, bars, voice) = flute(1);
    let c5 = nat(Step::C, 5);
    let at = |offset| Placement::new(voice, bars[0], offset);
    b.add_chord(at(Duration::ZERO), v(BaseValue::Quarter), &[c5])
        .unwrap();
    // A gap from 1/4 to 1/2: fine.
    b.add_chord(at(d(1, 2)), v(BaseValue::Quarter), &[c5])
        .unwrap();
    let mut gapped = b.clone();
    assert!(gapped.add_bar().is_ok());
    assert!(gapped.build().is_ok());
    // A half note at 5/8 starts before the quarter at 1/2 has ended (3/4).
    let late = b
        .add_chord(at(d(5, 8)), v(BaseValue::Half), &[c5])
        .unwrap()
        .event;
    assert_eq!(
        b.build(),
        Err(DocumentError::Invalid(Violation::Overlap(late)))
    );
}

/// ADR-0024 §13, tied notes: across a barline, across two voices of one instrument, F♯ to G♭,
/// and a laissez-vibrer tie are valid.
#[test]
fn legal_ties() {
    let (mut b, bars) = builder_with(four_four(), 2);
    let piano = b
        .add_instrument_track("Piano", unison(), &[Clef::TREBLE, Clef::BASS])
        .unwrap();
    let (rh, lh) = {
        let voices = b.document().track(piano).unwrap().instrument().voices();
        (voices[0].id(), voices[1].id())
    };
    let f_sharp = Pitch::new(Step::F, Alteration::SHARP, 4);
    let g_flat = Pitch::new(Step::G, Alteration::FLAT, 4);
    let whole = v(BaseValue::Whole);
    let a = b
        .add_chord(
            Placement::new(rh, bars[0], Duration::ZERO),
            whole,
            &[f_sharp],
        )
        .unwrap();
    // The tie goes to the other voice, in the next bar, spelled G♭.
    let z = b
        .add_chord(
            Placement::new(lh, bars[1], Duration::ZERO),
            whole,
            &[g_flat],
        )
        .unwrap();
    b.set_tie(a.notes[0], Tie::To(z.notes[0])).unwrap();
    b.set_tie(z.notes[0], Tie::LetRing).unwrap();
    let document = b.build().unwrap();
    assert_eq!(
        document.note(a.notes[0]).unwrap().1.tie(),
        Tie::To(z.notes[0])
    );
    assert_eq!(document.note(z.notes[0]).unwrap().1.tie(), Tie::LetRing);
}

/// ADR-0024 §4: ties to another pitch, another instrument, backwards or to itself are refused.
#[test]
fn illegal_ties() {
    let (mut b, bars) = builder_with(four_four(), 2);
    let flute = b
        .add_instrument_track("Flute", unison(), &[Clef::TREBLE])
        .unwrap();
    let oboe = b
        .add_instrument_track("Oboe", unison(), &[Clef::TREBLE])
        .unwrap();
    let voice_of =
        |b: &DocumentBuilder, t| b.document().track(t).unwrap().instrument().voices()[0].id();
    let (fl, ob) = (voice_of(&b, flute), voice_of(&b, oboe));
    let whole = v(BaseValue::Whole);
    let c5 = nat(Step::C, 5);
    let first = b
        .add_chord(Placement::new(fl, bars[0], Duration::ZERO), whole, &[c5])
        .unwrap();
    let second = b
        .add_chord(
            Placement::new(fl, bars[1], Duration::ZERO),
            whole,
            &[nat(Step::D, 5)],
        )
        .unwrap();
    let oboe_note = b
        .add_chord(Placement::new(ob, bars[1], Duration::ZERO), whole, &[c5])
        .unwrap();

    assert_eq!(
        b.set_tie(first.notes[0], Tie::To(first.notes[0])),
        Err(DocumentError::TieToItself)
    );
    let cases = [
        (
            first.notes[0],
            second.notes[0],
            Violation::TieToAnotherPitch(first.notes[0]),
        ),
        (
            first.notes[0],
            oboe_note.notes[0],
            Violation::TieToAnotherInstrument(first.notes[0]),
        ),
        (
            second.notes[0],
            first.notes[0],
            Violation::TieToAnotherPitch(second.notes[0]),
        ),
    ];
    for (from, to, violation) in cases {
        let mut attempt = b.clone();
        attempt.set_tie(from, Tie::To(to)).unwrap();
        assert_eq!(attempt.build(), Err(DocumentError::Invalid(violation)));
    }
    // Backwards with the same pitch: the oboe's C5 of bar 1 tied back to an oboe C5 of bar 0.
    let mut b2 = b.clone();
    let early = b2
        .add_chord(Placement::new(ob, bars[0], Duration::ZERO), whole, &[c5])
        .unwrap();
    b2.set_tie(oboe_note.notes[0], Tie::To(early.notes[0]))
        .unwrap();
    assert_eq!(
        b2.build(),
        Err(DocumentError::Invalid(Violation::TieNotForward(
            oboe_note.notes[0]
        )))
    );
}

/// ADR-0024 §13, clef change mid-bar: the initial clef holds until the change, which applies from
/// its own offset on; a duplicate, or a change at the very start, is refused.
#[test]
fn clef_changes() {
    let (mut b, bars) = builder_with(four_four(), 3);
    let cello = b
        .add_instrument_track("Cello", unison(), &[Clef::BASS])
        .unwrap();
    let staff = b.document().track(cello).unwrap().instrument().staves()[0].id();
    let tenor = b
        .add_clef_change(staff, bars[1], d(1, 2), Clef::TENOR)
        .unwrap();
    b.add_clef_change(staff, bars[2], Duration::ZERO, Clef::BASS)
        .unwrap();
    assert_eq!(
        b.add_clef_change(staff, bars[1], d(1, 2), Clef::ALTO),
        Err(DocumentError::DuplicateMark)
    );
    assert_eq!(
        b.add_clef_change(staff, bars[0], Duration::ZERO, Clef::ALTO),
        Err(DocumentError::DuplicateMark)
    );
    assert_eq!(
        b.add_clef_change(staff, bars[0], d(-1, 4), Clef::ALTO),
        Err(DocumentError::NegativeOffset)
    );
    let document = b.build().unwrap();
    let at = |bar: BarId, offset| document.clef_at(staff, bar, offset).unwrap();
    assert_eq!(at(bars[0], Duration::ZERO), Clef::BASS);
    assert_eq!(at(bars[1], d(1, 4)), Clef::BASS);
    assert_eq!(at(bars[1], d(1, 2)), Clef::TENOR);
    assert_eq!(at(bars[1], d(7, 8)), Clef::TENOR);
    assert_eq!(at(bars[2], Duration::ZERO), Clef::BASS);
    let (_, staff_ref) = document.staff(staff).unwrap();
    assert_eq!(staff_ref.initial_clef(), Clef::BASS);
    assert_eq!(staff_ref.clef_changes().len(), 2);
    assert_eq!(staff_ref.clef_changes().as_slice()[0].id(), tenor);
    assert_eq!(Clef::new(ClefSign::G, 2, -1).unwrap(), Clef::TREBLE_8VB);
    assert_eq!(
        Clef::new(ClefSign::C, 6, 0),
        Err(DocumentError::ClefOutOfRange)
    );
    assert_eq!(
        Clef::new(ClefSign::F, 4, 3),
        Err(DocumentError::ClefOutOfRange)
    );
}

/// ADR-0024 §13, transposing instruments and K1: a B♭ clarinet's written key is derived (two more
/// sharps); a horn without signature shows none, concert or written; an override can switch an
/// instrument to another key and back to the score's.
#[test]
fn key_signatures_and_overrides() {
    let (mut b, bars) = builder_with(four_four(), 4);
    let down = |quality, number| Interval::from_quality(quality, number, true).unwrap();
    let clarinet = b
        .add_instrument_track("Clarinet in B♭", down(Quality::Major, 2), &[Clef::TREBLE])
        .unwrap();
    let horn = b
        .add_instrument_track("Horn in F", down(Quality::Perfect, 5), &[Clef::TREBLE])
        .unwrap();
    let violin = b
        .add_instrument_track("Violin", unison(), &[Clef::TREBLE])
        .unwrap();
    let instrument = |b: &DocumentBuilder, t| b.document().track(t).unwrap().instrument().id();
    let (cl, hn, vn) = (
        instrument(&b, clarinet),
        instrument(&b, horn),
        instrument(&b, violin),
    );
    // The score moves to E♭ major (three flats) in bar 2.
    b.add_key_change(bars[2], Duration::ZERO, major(-3))
        .unwrap();
    assert_eq!(
        b.add_key_change(bars[0], Duration::ZERO, major(1)),
        Err(DocumentError::DuplicateMark)
    );
    // The horn is written without signature from the start.
    b.add_key_override(
        hn,
        bars[0],
        Duration::ZERO,
        KeyOverride::Signature(KeySignature::NoSignature),
    )
    .unwrap();
    // The violin plays in A major (three sharps) in bar 1, then follows the score again.
    b.add_key_override(
        vn,
        bars[1],
        Duration::ZERO,
        KeyOverride::Signature(KeySignature::Key(major(3))),
    )
    .unwrap();
    b.add_key_override(vn, bars[3], Duration::ZERO, KeyOverride::FollowScore)
        .unwrap();
    let document = b.build().unwrap();
    let zero = Duration::ZERO;

    assert_eq!(document.key_at(bars[1], zero).unwrap(), major(0));
    assert_eq!(document.key_at(bars[2], zero).unwrap(), major(-3));
    // The clarinet: C major sounds as written D major; E♭ major as written F major.
    assert_eq!(
        document.written_key_at(cl, bars[0], zero).unwrap(),
        KeySignature::Key(major(2))
    );
    assert_eq!(
        document.written_key_at(cl, bars[2], zero).unwrap(),
        KeySignature::Key(major(-1))
    );
    assert_eq!(
        document.instrument_key_at(cl, bars[2], zero).unwrap(),
        KeySignature::Key(major(-3))
    );
    // The horn: no signature, concert and written, whatever the score does.
    for bar in &bars {
        assert_eq!(
            document.written_key_at(hn, *bar, zero).unwrap(),
            KeySignature::NoSignature
        );
    }
    // The violin: C, A major (override), still A major after the score's change, E♭ again.
    let violin_keys: Vec<_> = bars
        .iter()
        .map(|&bar| document.instrument_key_at(vn, bar, zero).unwrap())
        .collect();
    assert_eq!(
        violin_keys,
        [
            KeySignature::Key(major(0)),
            KeySignature::Key(major(3)),
            KeySignature::Key(major(3)),
            KeySignature::Key(major(-3)),
        ]
    );
    assert_eq!(document.initial_key(), major(0));
    assert_eq!(document.key_changes().len(), 1);
    assert_eq!(document.instrument(vn).unwrap().key_overrides().len(), 2);
}

/// Cross-staff notes: an event of the right hand drawn on the lower staff keeps its voice; a
/// staff of another instrument is refused.
#[test]
fn cross_staff_events_and_foreign_staves() {
    let (mut b, bars) = builder_with(four_four(), 1);
    let piano = b
        .add_instrument_track("Piano", unison(), &[Clef::TREBLE, Clef::BASS])
        .unwrap();
    let flute = b
        .add_instrument_track("Flute", unison(), &[Clef::TREBLE])
        .unwrap();
    let (rh, lower) = {
        let i = b.document().track(piano).unwrap().instrument();
        (i.voices()[0].id(), i.staves()[1].id())
    };
    let flute_staff = b.document().track(flute).unwrap().instrument().staves()[0].id();
    let at = Placement::new(rh, bars[0], Duration::ZERO);
    let crossed = b
        .add_chord(at.on_staff(lower), v(BaseValue::Whole), &[nat(Step::C, 3)])
        .unwrap();
    assert_eq!(
        b.add_rest(at.on_staff(flute_staff), RestValue::WholeBar),
        Err(DocumentError::StaffOfAnotherInstrument(flute_staff))
    );
    let document = b.build().unwrap();
    let found = document.event(crossed.event).unwrap();
    assert_eq!(found.event().staff(), lower);
    assert_eq!(found.voice(), rh);
    assert_eq!(found.bar(), bars[0]);
}

/// The placement errors of the builder: unknown voice, bar and tuplet (a tuplet of another voice
/// included), negative offset, empty chord, tuplets nested too deep. None of them changes the
/// builder.
#[test]
fn placement_errors_leave_the_builder_unchanged() {
    let (mut b, bars) = builder_with(four_four(), 1);
    let piano = b
        .add_instrument_track("Piano", unison(), &[Clef::TREBLE, Clef::BASS])
        .unwrap();
    let (rh, lh) = {
        let voices = b.document().track(piano).unwrap().instrument().voices();
        (voices[0].id(), voices[1].id())
    };
    let lh_tuplet = b
        .add_tuplet(lh, Tuplet::new(3, 2, v(BaseValue::Eighth)).unwrap(), None)
        .unwrap();
    let mut other = DocumentBuilder::new(
        four_four(),
        TempoMark::immediate(
            None,
            Metronome::shown(Tempo::new(d(1, 4), q(60, 1)).unwrap()),
        )
        .unwrap(),
        major(0),
    )
    .unwrap();
    let foreign_bar = {
        other.add_bar().unwrap();
        other.add_bar().unwrap()
    };
    let snapshot = b.clone().build().unwrap();
    let c4 = [nat(Step::C, 4)];
    let quarter = v(BaseValue::Quarter);
    let at = Placement::new(rh, bars[0], Duration::ZERO);
    let errors = [
        b.add_chord(at, quarter, &[]).map(|_| ()),
        b.add_chord(
            Placement {
                offset: d(-1, 8),
                ..at
            },
            quarter,
            &c4,
        )
        .map(|_| ()),
        b.add_chord(
            Placement {
                bar: foreign_bar,
                ..at
            },
            quarter,
            &c4,
        )
        .map(|_| ()),
        b.add_chord(at.in_tuplet(lh_tuplet), quarter, &c4)
            .map(|_| ()),
    ];
    assert_eq!(
        errors,
        [
            Err(DocumentError::EmptyChord),
            Err(DocumentError::NegativeOffset),
            Err(DocumentError::UnknownBar(foreign_bar)),
            Err(DocumentError::UnknownTuplet(lh_tuplet)),
        ]
    );
    // Nesting: four levels are fine, a fifth is refused.
    let mut parent = None;
    for _ in 0..MAX_TUPLET_DEPTH {
        parent = Some(
            b.add_tuplet(rh, Tuplet::new(3, 2, quarter).unwrap(), parent)
                .unwrap(),
        );
    }
    assert_eq!(
        b.add_tuplet(rh, Tuplet::new(3, 2, quarter).unwrap(), parent),
        Err(DocumentError::TupletTooDeep)
    );
    assert_eq!(
        b.add_tuplet(rh, Tuplet::new(3, 2, quarter).unwrap(), Some(lh_tuplet)),
        Err(DocumentError::UnknownTuplet(lh_tuplet))
    );
    // Only the four tuplets were added; the events are as before.
    let document = b.build().unwrap();
    assert_eq!(
        document.events(rh, bars[0]).unwrap(),
        snapshot.events(rh, bars[0]).unwrap()
    );
    assert_eq!(document.voice(rh).unwrap().1.tuplet_count(), 4);
}

/// Lookups of things that do not exist return `None` or the typed error, never panic.
#[test]
fn lookups_of_missing_things() {
    let (mut b, bars, voice) = flute(1);
    let rest = b
        .add_rest(
            Placement::new(voice, bars[0], Duration::ZERO),
            RestValue::Value(v(BaseValue::Quarter)),
        )
        .unwrap();
    let mut other = b.clone();
    let foreign = other
        .add_chord(
            Placement::new(voice, bars[0], d(1, 4)),
            v(BaseValue::Quarter),
            &[nat(Step::C, 4)],
        )
        .unwrap();
    let document = b.build().unwrap();
    assert!(document.event(foreign.event).is_none());
    assert!(document.note(foreign.notes[0]).is_none());
    assert_eq!(
        document.metric_duration(foreign.event),
        Err(DocumentError::UnknownEvent(foreign.event))
    );
    assert_eq!(
        document.event_position(foreign.event),
        Err(DocumentError::UnknownEvent(foreign.event))
    );
    // A rest has no noteheads and no chord.
    let found = document.event(rest).unwrap();
    assert!(found.event().chord().is_none());
    assert!(matches!(found.event().body(), EventBody::Rest(_)));
    assert_eq!(found.event().tuplet(), None);
    // The voice's chunks past the end are empty.
    let (_, voice) = document.voice(voice).unwrap();
    assert_eq!(voice.events_at(BarIndex::new(99)), []);
}
