// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases of the pitch module (ADR-0023), each one checkable by a musician without a
//! computer: enharmonic spellings across the octave boundary, MIDI keys and quarter tones, the
//! qualities of intervals from doubly diminished to doubly augmented, transposing instruments,
//! key signatures and the spelling of MIDI keys.
//!
//! The property tests in `pitch_oracle.rs` cover the same rules over many random inputs against an
//! independent oracle; these cases pin the musically meaningful examples down by name.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only: a helper that
// unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::pitch::{
    Alteration, Interval, Key, Mode, Pitch, PitchError, Quality, Step, TraditionalKey,
};
use orchestra_core_time::Rational;

/// Shorthand: the rational `n/d`.
fn r(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// Shorthand: a pitch with a chromatic alteration of `alt` semitones.
fn p(step: Step, alt: i64, octave: i8) -> Pitch {
    Pitch::new(step, Alteration::chromatic(alt).unwrap(), octave)
}

/// Shorthand: a pitch with a microtonal alteration of `n/d` semitones.
fn pm(step: Step, n: i64, d: i64, octave: i8) -> Pitch {
    Pitch::new(step, Alteration::new(r(n, d)).unwrap(), octave)
}

/// Shorthand: an interval from its quality and number.
fn iv(quality: Quality, number: u32, descending: bool) -> Interval {
    Interval::from_quality(quality, number, descending).unwrap()
}

/// Shorthand: a traditional key.
fn key(fifths: i8, mode: Mode) -> Key {
    Key::traditional(fifths, mode).unwrap()
}

// ------------------------------------------------------------------------------------------------
// Spelling, sounding pitch, MIDI
// ------------------------------------------------------------------------------------------------

/// B♯3 and C4 are different notes with the same sound; C♭4 sounds like B3. The octave number
/// changes at C, not at the sound.
#[test]
fn enharmonics_across_the_octave_boundary() {
    let b_sharp_3 = p(Step::B, 1, 3);
    let c_4 = p(Step::C, 0, 4);
    assert_ne!(b_sharp_3, c_4);
    assert!(b_sharp_3.sounds_like(c_4).unwrap());
    assert_eq!(b_sharp_3.midi().unwrap().key, 60);
    let c_flat_4 = p(Step::C, -1, 4);
    assert!(c_flat_4.sounds_like(p(Step::B, 0, 3)).unwrap());
    assert_eq!(c_flat_4.midi().unwrap().key, 59);
    // F♯4 and G♭4: equal sound, different spelling.
    assert_ne!(p(Step::F, 1, 4), p(Step::G, -1, 4));
    assert!(p(Step::F, 1, 4).sounds_like(p(Step::G, -1, 4)).unwrap());
}

/// The ends of the MIDI range and the first keys outside it.
#[test]
fn midi_range() {
    assert_eq!(p(Step::A, 0, 4).midi().unwrap().key, 69);
    assert_eq!(p(Step::C, 0, -1).midi().unwrap().key, 0);
    assert_eq!(p(Step::G, 0, 9).midi().unwrap().key, 127);
    assert_eq!(p(Step::G, 1, 9).midi(), Err(PitchError::MidiOutOfRange));
    assert_eq!(p(Step::C, -1, -1).midi(), Err(PitchError::MidiOutOfRange));
    // B♯−2 sounds like C−1: the spelling is outside the octave but the key is fine.
    assert_eq!(p(Step::B, 1, -2).midi().unwrap().key, 0);
}

/// Quarter tones: the nearest key, the lower one on a tie, and the bend.
#[test]
fn microtonal_midi() {
    // C4 a quarter tone sharp: exactly half way between C4 and C♯4 → C4 bent up by a half.
    let m = pm(Step::C, 1, 2, 4).midi().unwrap();
    assert_eq!((m.key, m.bend), (60, r(1, 2)));
    // C4 a quarter tone flat: half way between B3 and C4 → B3 bent up by a half.
    let m = pm(Step::C, -1, 2, 4).midi().unwrap();
    assert_eq!((m.key, m.bend), (59, r(1, 2)));
    // C4 three eighths of a tone sharp (3/4 semitone): nearer to C♯4 → C♯4 bent down a quarter.
    let m = pm(Step::C, 3, 4, 4).midi().unwrap();
    assert_eq!((m.key, m.bend), (61, r(-1, 4)));
    // A sixth-tone sharp A4 (1/3 semitone): A4 bent up by a third.
    let m = pm(Step::A, 1, 3, 4).midi().unwrap();
    assert_eq!((m.key, m.bend), (69, r(1, 3)));
    // Chromatic pitches never bend.
    assert_eq!(p(Step::E, -1, 4).midi().unwrap().bend, Rational::ZERO);
}

/// Alterations are limited to a triple sharp or flat.
#[test]
fn alteration_range() {
    assert!(Alteration::chromatic(3).is_ok());
    assert!(Alteration::chromatic(-3).is_ok());
    assert_eq!(
        Alteration::chromatic(4),
        Err(PitchError::AlterationOutOfRange)
    );
    assert_eq!(
        Alteration::new(r(7, 2)),
        Err(PitchError::AlterationOutOfRange)
    );
    assert_eq!(
        Alteration::new(r(-13, 4)),
        Err(PitchError::AlterationOutOfRange)
    );
    assert_eq!(Alteration::new(r(-3, 2)).unwrap().semitones(), r(-3, 2));
    assert!(Alteration::SHARP.is_chromatic());
    assert_eq!(Alteration::FLAT.semitones(), r(-1, 1));
}

// ------------------------------------------------------------------------------------------------
// Intervals
// ------------------------------------------------------------------------------------------------

/// The quality of the interval from `a` to `b`.
fn quality(a: Pitch, b: Pitch) -> Option<Quality> {
    a.interval_to(b).unwrap().quality().unwrap()
}

/// Simple and compound intervals above C4, with every quality class.
#[test]
fn qualities() {
    let c4 = p(Step::C, 0, 4);
    assert_eq!(quality(c4, p(Step::E, 0, 4)), Some(Quality::Major));
    assert_eq!(quality(c4, p(Step::E, -1, 4)), Some(Quality::Minor));
    assert_eq!(quality(c4, p(Step::E, -2, 4)), Some(Quality::Diminished(1)));
    assert_eq!(quality(c4, p(Step::E, 1, 4)), Some(Quality::Augmented(1)));
    assert_eq!(quality(c4, p(Step::F, 1, 4)), Some(Quality::Augmented(1)));
    assert_eq!(quality(c4, p(Step::G, -1, 4)), Some(Quality::Diminished(1)));
    assert_eq!(quality(c4, p(Step::F, -1, 4)), Some(Quality::Diminished(1)));
    assert_eq!(quality(c4, p(Step::G, 0, 4)), Some(Quality::Perfect));
    assert_eq!(quality(c4, p(Step::C, 0, 5)), Some(Quality::Perfect));
    assert_eq!(quality(c4, p(Step::G, 2, 4)), Some(Quality::Augmented(2)));
    // C♯4 → G♭4: a fifth of five semitones, doubly diminished.
    assert_eq!(
        quality(p(Step::C, 1, 4), p(Step::G, -1, 4)),
        Some(Quality::Diminished(2))
    );
    // C♯4 → B♭4: a diminished seventh (9 semitones).
    assert_eq!(
        quality(p(Step::C, 1, 4), p(Step::B, -1, 4)),
        Some(Quality::Diminished(1))
    );
    // C4 → E5: a major tenth.
    let tenth = c4.interval_to(p(Step::E, 0, 5)).unwrap();
    assert_eq!(
        (tenth.number(), tenth.quality().unwrap()),
        (10, Some(Quality::Major))
    );
}

/// Unisons take their sign from the semitones; descending intervals keep their quality.
#[test]
fn unisons_and_descending_intervals() {
    let c4 = p(Step::C, 0, 4);
    assert_eq!(quality(c4, p(Step::C, -1, 4)), Some(Quality::Diminished(1)));
    assert_eq!(quality(c4, p(Step::C, 1, 4)), Some(Quality::Augmented(1)));
    assert_eq!(quality(c4, c4), Some(Quality::Perfect));
    let down = p(Step::E, 0, 4).interval_to(c4).unwrap();
    assert!(down.is_descending());
    assert_eq!(
        (down.number(), down.quality().unwrap()),
        (3, Some(Quality::Major))
    );
    // C4 down to C♯3: a diminished octave.
    assert_eq!(quality(c4, p(Step::C, 1, 3)), Some(Quality::Diminished(1)));
}

/// A microtonal interval has no standard quality, only a deviation.
#[test]
fn microtonal_interval() {
    let neutral_third = p(Step::C, 0, 4).interval_to(pm(Step::E, -1, 2, 4)).unwrap();
    assert_eq!(neutral_third.quality().unwrap(), None);
    assert_eq!(neutral_third.deviation().unwrap(), r(-1, 2));
    assert_eq!(neutral_third.fifths(), Err(PitchError::MicrotonalKey));
}

/// Building intervals from their names, and the names that do not exist.
#[test]
fn intervals_from_quality() {
    assert_eq!(iv(Quality::Major, 3, false).semitones(), r(4, 1));
    assert_eq!(iv(Quality::Perfect, 5, true).semitones(), r(-7, 1));
    assert_eq!(iv(Quality::Diminished(1), 7, false).semitones(), r(9, 1));
    assert_eq!(iv(Quality::Perfect, 8, false).steps(), 7);
    assert_eq!(iv(Quality::Minor, 10, false).semitones(), r(15, 1));
    // A unison has no direction: `descending` is ignored, a diminished unison is −1 semitone.
    assert_eq!(iv(Quality::Diminished(1), 1, true).semitones(), r(-1, 1));
    assert_eq!(iv(Quality::Diminished(1), 1, true).steps(), 0);
    for (q, n) in [
        (Quality::Perfect, 3),
        (Quality::Major, 5),
        (Quality::Minor, 1),
        (Quality::Augmented(0), 3),
        (Quality::Diminished(0), 4),
        (Quality::Major, 0),
    ] {
        assert_eq!(
            Interval::from_quality(q, n, false),
            Err(PitchError::InvalidQuality),
            "{q:?} {n}"
        );
    }
}

// ------------------------------------------------------------------------------------------------
// Transposition and transposing instruments
// ------------------------------------------------------------------------------------------------

/// Transposition keeps the spelling the interval implies.
#[test]
fn transposition_spelling() {
    let c4 = p(Step::C, 0, 4);
    assert_eq!(
        c4.transposed(iv(Quality::Major, 3, false)).unwrap(),
        p(Step::E, 0, 4)
    );
    assert_eq!(
        c4.transposed(iv(Quality::Diminished(1), 4, false)).unwrap(),
        p(Step::F, -1, 4)
    );
    assert_eq!(
        c4.transposed(iv(Quality::Augmented(1), 4, false)).unwrap(),
        p(Step::F, 1, 4)
    );
    // F♯4 up an augmented sixth: D double sharp 5.
    let d_double_sharp = p(Step::F, 1, 4)
        .transposed(iv(Quality::Augmented(1), 6, false))
        .unwrap();
    assert_eq!(d_double_sharp, p(Step::D, 2, 5));
    // Down a minor second from C4: B3, across the octave boundary.
    assert_eq!(
        c4.transposed(iv(Quality::Minor, 2, true)).unwrap(),
        p(Step::B, 0, 3)
    );
    // B𝄪4 up a doubly augmented unison would need four sharps.
    assert_eq!(
        p(Step::B, 2, 4).transposed(iv(Quality::Augmented(2), 1, false)),
        Err(PitchError::AlterationOutOfRange)
    );
}

/// The written pitch of a transposing instrument: the concert pitch moved by the reversed
/// written-to-sounding interval (ADR-0023 §7).
fn written(concert: Pitch, transposition: Interval) -> Pitch {
    concert.transposed(transposition.reversed()).unwrap()
}

/// B♭ clarinet, horn in F, piccolo, double bass.
#[test]
fn transposing_instruments() {
    let b_flat_clarinet = iv(Quality::Major, 2, true);
    assert_eq!(written(p(Step::C, 0, 4), b_flat_clarinet), p(Step::D, 0, 4));
    assert_eq!(
        written(p(Step::B, -1, 3), b_flat_clarinet),
        p(Step::C, 0, 4)
    );
    let horn_in_f = iv(Quality::Perfect, 5, true);
    assert_eq!(written(p(Step::C, 0, 4), horn_in_f), p(Step::G, 0, 4));
    assert_eq!(written(p(Step::E, -1, 4), horn_in_f), p(Step::B, -1, 4));
    let piccolo = iv(Quality::Perfect, 8, false);
    assert_eq!(written(p(Step::C, 0, 6), piccolo), p(Step::C, 0, 5));
    let double_bass = iv(Quality::Perfect, 8, true);
    assert_eq!(written(p(Step::E, 0, 1), double_bass), p(Step::E, 0, 2));
    // Back again: the sounding pitch of what is written.
    let w = written(p(Step::F, 1, 4), b_flat_clarinet);
    assert_eq!(w, p(Step::G, 1, 4));
    assert_eq!(w.transposed(b_flat_clarinet).unwrap(), p(Step::F, 1, 4));
}

/// The written key of a transposing instrument.
#[test]
fn transposed_keys() {
    let to_written = iv(Quality::Major, 2, true).reversed();
    // E♭ major (3 flats) for a B♭ clarinet: F major (1 flat).
    assert_eq!(
        key(-3, Mode::Major).transposed(to_written).unwrap(),
        key(-1, Mode::Major)
    );
    // C♯ major (7 sharps): D♯ major, 9 sharps, a theoretical key the model must hold.
    assert_eq!(
        key(7, Mode::Major).transposed(to_written).unwrap(),
        key(9, Mode::Major)
    );
    // Horn in F: E major (4 sharps) is written in B major (5 sharps).
    let horn = iv(Quality::Perfect, 5, true).reversed();
    assert_eq!(
        key(4, Mode::Major).transposed(horn).unwrap(),
        key(5, Mode::Major)
    );
    // An octave does not move the key; beyond 14 fifths is an error.
    assert_eq!(
        key(2, Mode::Minor)
            .transposed(iv(Quality::Perfect, 8, false))
            .unwrap(),
        key(2, Mode::Minor)
    );
    assert_eq!(
        key(13, Mode::Major).transposed(to_written),
        Err(PitchError::KeyOutOfRange)
    );
    assert_eq!(
        Key::traditional(15, Mode::Major),
        Err(PitchError::KeyOutOfRange)
    );
}

// ------------------------------------------------------------------------------------------------
// Keys and spelling
// ------------------------------------------------------------------------------------------------

/// The alteration a key gives to a step.
#[test]
fn key_alterations() {
    let alt = |k: Key, s: Step| k.alteration_of(s).semitones();
    assert_eq!(alt(key(1, Mode::Major), Step::F), r(1, 1));
    assert_eq!(alt(key(1, Mode::Major), Step::C), r(0, 1));
    assert_eq!(alt(key(-1, Mode::Major), Step::B), r(-1, 1));
    assert_eq!(alt(key(-1, Mode::Major), Step::E), r(0, 1));
    for s in Step::ALL {
        assert_eq!(alt(key(7, Mode::Major), s), r(1, 1), "C♯ major, {s:?}");
        assert_eq!(alt(key(-7, Mode::Major), s), r(-1, 1), "C♭ major, {s:?}");
        assert_eq!(
            alt(key(14, Mode::Major), s),
            r(2, 1),
            "fourteen sharps, {s:?}"
        );
        assert_eq!(
            alt(key(-14, Mode::Major), s),
            r(-2, 1),
            "fourteen flats, {s:?}"
        );
        assert_eq!(alt(key(0, Mode::None), s), r(0, 1), "open signature, {s:?}");
    }
    // Nine sharps (D♯ major): F𝄪 and C𝄪, the rest sharp.
    assert_eq!(alt(key(9, Mode::Major), Step::F), r(2, 1));
    assert_eq!(alt(key(9, Mode::Major), Step::C), r(2, 1));
    assert_eq!(alt(key(9, Mode::Major), Step::G), r(1, 1));
}

/// The tonic named by the mode.
#[test]
fn tonics() {
    let tonic = |f: i8, m: Mode| TraditionalKey::new(f, m).unwrap().tonic();
    let nat = Alteration::NATURAL;
    assert_eq!(tonic(-3, Mode::Minor), Some((Step::C, nat)));
    assert_eq!(tonic(-3, Mode::Major), Some((Step::E, Alteration::FLAT)));
    assert_eq!(tonic(2, Mode::Dorian), Some((Step::E, nat)));
    assert_eq!(tonic(0, Mode::Phrygian), Some((Step::E, nat)));
    assert_eq!(tonic(0, Mode::Lydian), Some((Step::F, nat)));
    assert_eq!(tonic(0, Mode::Mixolydian), Some((Step::G, nat)));
    assert_eq!(tonic(0, Mode::Locrian), Some((Step::B, nat)));
    assert_eq!(tonic(6, Mode::Minor), Some((Step::D, Alteration::SHARP)));
    assert_eq!(tonic(0, Mode::None), None);
}

/// The spelling of MIDI keys in a few keys (ADR-0023 §6).
#[test]
fn spelling_midi_keys() {
    let spell = |k: u8, key: Key| Pitch::spell_midi(k, key);
    let c_major = key(0, Mode::Major);
    assert_eq!(spell(60, c_major), p(Step::C, 0, 4));
    assert_eq!(spell(61, c_major), p(Step::C, 1, 4));
    assert_eq!(spell(63, c_major), p(Step::E, -1, 4));
    assert_eq!(spell(66, c_major), p(Step::F, 1, 4));
    assert_eq!(spell(68, c_major), p(Step::A, -1, 4));
    assert_eq!(spell(70, c_major), p(Step::B, -1, 4));
    // A minor: the raised leading tone is G♯, E♭ stays.
    let a_minor = key(0, Mode::Minor);
    assert_eq!(spell(68, a_minor), p(Step::G, 1, 4));
    assert_eq!(spell(63, a_minor), p(Step::E, -1, 4));
    // E♭ major: D♭, A and E natural.
    let e_flat = key(-3, Mode::Major);
    assert_eq!(spell(61, e_flat), p(Step::D, -1, 4));
    assert_eq!(spell(69, e_flat), p(Step::A, 0, 4));
    assert_eq!(spell(64, e_flat), p(Step::E, 0, 4));
    // C♯ major: key 60 is B♯3, across the octave boundary.
    assert_eq!(spell(60, key(7, Mode::Major)), p(Step::B, 1, 3));
    // "No mode" spells as C major whatever the signature.
    assert_eq!(spell(70, key(4, Mode::None)), p(Step::B, -1, 4));
    // The extremes of the MIDI range.
    assert_eq!(spell(0, c_major), p(Step::C, 0, -1));
    assert_eq!(spell(127, c_major), p(Step::G, 0, 9));
}
