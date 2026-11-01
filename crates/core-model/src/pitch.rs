// SPDX-License-Identifier: GPL-3.0-or-later

//! Pitch with its spelling, intervals, key signatures and transposition (ADR-0023).
//!
//! # What lives here
//! - [`Step`]: the letter of a note (C D E F G A B), independent of the interface language.
//! - [`Alteration`]: a **rational number of semitones**, so that quarter tones (`1/2`), sixth tones
//!   (`1/3`) and the other equal divisions are written exactly (ADR-0023 §2, decision D07).
//! - [`Pitch`]: step + alteration + scientific octave (C4 = middle C). Two pitches are equal only
//!   if they are *spelled* the same (ADR-0004); [`Pitch::sounds_like`] compares what they sound.
//! - [`MidiNote`]: the nearest MIDI key plus a bend, derived with the 12-TET reference (§3).
//! - [`Interval`] and [`Quality`]: diatonic steps + semitones, the quality derived from them (§4).
//! - [`Key`], [`TraditionalKey`] and [`Mode`]: key signatures counted in fifths (§5), and the
//!   spelling of a MIDI key in a key ([`Pitch::spell_midi`], §6).
//!
//! # The line of fifths, used inside the computations
//! Every chromatic spelling has a place on the *line of fifths*: … B♭ = −2, F = −1, **C = 0**,
//! G = 1, D = 2, A = 3, E = 4, B = 5, F♯ = 6, C♯ = 7 … Moving one place to the right adds a perfect
//! fifth; moving seven places adds a sharp to the same letter. This is why keys (a window of seven
//! consecutive places), enharmonic spellings (places twelve apart) and the transposition of keys
//! (a shift along the line) are one addition each. The line is only a tool for computing: what is
//! stored is step + alteration + octave (ADR-0023 §1, representation R1).
//!
//! # Exactness and errors
//! Every value is small and `Copy`, every operation is O(1) and never allocates. Nothing here
//! rounds silently: an alteration beyond a triple sharp or flat, an octave that does not fit, a
//! MIDI key outside 0–127, a key beyond fourteen fifths or a rational overflow are all reported
//! as a [`PitchError`]. Integer arithmetic on bounded values (octaves fit in `i8`, steps in `i32`)
//! is done in `i64`, where it cannot overflow; arithmetic on unbounded values (the numerators of
//! rationals) uses the checked operations of `core-time`.

use std::error::Error;
use std::fmt;

use orchestra_core_time::{Rational, RationalError};

// ------------------------------------------------------------------------------------------------
// Errors
// ------------------------------------------------------------------------------------------------

/// Why a pitch operation failed (ADR-0023 §8).
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to a
/// Fluent key, like every other user-visible text (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PitchError {
    /// An alteration beyond a triple sharp or a triple flat (more than 3 semitones in magnitude).
    AlterationOutOfRange,
    /// An octave number that does not fit in an `i8` (only reachable by transposing by an absurd
    /// interval: the MIDI range covers octaves −1 to 9).
    OctaveOutOfRange,
    /// A sounding pitch whose nearest MIDI key is outside 0–127. Never clipped silently.
    MidiOutOfRange,
    /// A key signature beyond fourteen sharps or flats.
    KeyOutOfRange,
    /// A microtonal interval applied to a key signature: a traditional key has only chromatic
    /// alterations, so it cannot move by a fraction of a semitone.
    MicrotonalKey,
    /// A quality that does not exist for the interval number: a "perfect third", a "major fifth",
    /// an interval number of 0, or an augmentation or diminution of degree 0.
    InvalidQuality,
    /// The exact arithmetic overflowed (ADR-0015); only reachable with absurd inputs.
    Arithmetic(RationalError),
}

impl fmt::Display for PitchError {
    /// Writes a short English description of the error, for developers (see the type's docs).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlterationOutOfRange => {
                f.write_str("alteration beyond a triple sharp or a triple flat")
            }
            Self::OctaveOutOfRange => f.write_str("octave number out of the representable range"),
            Self::MidiOutOfRange => f.write_str("sounding pitch outside the MIDI keys 0-127"),
            Self::KeyOutOfRange => f.write_str("key signature beyond fourteen sharps or flats"),
            Self::MicrotonalKey => f.write_str("microtonal interval applied to a key signature"),
            Self::InvalidQuality => f.write_str("interval quality invalid for its number"),
            Self::Arithmetic(e) => write!(f, "pitch arithmetic: {e}"),
        }
    }
}

impl Error for PitchError {
    /// The underlying rational error, if any.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Arithmetic(e) => Some(e),
            _ => None,
        }
    }
}

impl From<RationalError> for PitchError {
    /// Wraps a rational overflow, so that `?` works on `core-time` operations.
    fn from(e: RationalError) -> Self {
        Self::Arithmetic(e)
    }
}

// ------------------------------------------------------------------------------------------------
// Small integer helpers
// ------------------------------------------------------------------------------------------------

/// The integer `n` as a [`Rational`].
///
/// `Rational::from_integer` only fails for `i64::MIN` (the excluded numerator of ADR-0015); the
/// callers here pass values bounded by a few thousand, so the error is unreachable in practice and
/// is still propagated rather than unwrapped.
fn int(n: i64) -> Result<Rational, PitchError> {
    Ok(Rational::from_integer(n)?)
}

/// The rational `r` as an `i64` if it is an integer, `None` otherwise.
fn as_integer(r: Rational) -> Option<i64> {
    // An integer in lowest terms has denominator 1, so `floor` returns the value itself.
    r.is_integer().then(|| r.floor())
}

// ------------------------------------------------------------------------------------------------
// Step
// ------------------------------------------------------------------------------------------------

/// The letter of a note: C D E F G A B (ADR-0023 §1).
///
/// The letter is independent of the interface language: Do/Re/Mi (Italian, French, Spanish),
/// C/D/E (English) and the German H for B natural are names the interface gives to these values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Step {
    /// C (Do).
    C,
    /// D (Re).
    D,
    /// E (Mi).
    E,
    /// F (Fa).
    F,
    /// G (Sol).
    G,
    /// A (La).
    A,
    /// B (Si; H in German).
    B,
}

impl Step {
    /// The seven steps in scale order, from C.
    pub const ALL: [Self; 7] = [
        Self::C,
        Self::D,
        Self::E,
        Self::F,
        Self::G,
        Self::A,
        Self::B,
    ];

    /// The step's place in the scale: C = 0 … B = 6. O(1).
    #[must_use]
    pub const fn index(self) -> i64 {
        match self {
            Self::C => 0,
            Self::D => 1,
            Self::E => 2,
            Self::F => 3,
            Self::G => 4,
            Self::A => 5,
            Self::B => 6,
        }
    }

    /// The step at place `i` of the scale, counted modulo 7 (so −1 is B and 7 is C). O(1).
    #[must_use]
    pub const fn from_index(i: i64) -> Self {
        match i.rem_euclid(7) {
            0 => Self::C,
            1 => Self::D,
            2 => Self::E,
            3 => Self::F,
            4 => Self::G,
            5 => Self::A,
            // `rem_euclid(7)` is in 0..7, so this arm is exactly 6.
            _ => Self::B,
        }
    }

    /// Semitones of the natural note above C in the same octave: 0 2 4 5 7 9 11. O(1).
    #[must_use]
    pub const fn natural_semitones(self) -> i64 {
        match self {
            Self::C => 0,
            Self::D => 2,
            Self::E => 4,
            Self::F => 5,
            Self::G => 7,
            Self::A => 9,
            Self::B => 11,
        }
    }

    /// The natural note's place on the line of fifths: F = −1, C = 0, G = 1, D = 2, A = 3, E = 4,
    /// B = 5. O(1).
    #[must_use]
    pub const fn natural_fifths(self) -> i64 {
        match self {
            Self::F => -1,
            Self::C => 0,
            Self::G => 1,
            Self::D => 2,
            Self::A => 3,
            Self::E => 4,
            Self::B => 5,
        }
    }

    /// The step whose natural note sits at line-of-fifths place `t` modulo 7 (the inverse of
    /// [`natural_fifths`](Self::natural_fifths) up to sharps and flats). O(1).
    const fn from_fifths(t: i64) -> Self {
        // Shift by one so that F (−1) maps to 0: the naturals then run F C G D A E B = 0…6.
        match (t + 1).rem_euclid(7) {
            0 => Self::F,
            1 => Self::C,
            2 => Self::G,
            3 => Self::D,
            4 => Self::A,
            5 => Self::E,
            _ => Self::B,
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Alteration
// ------------------------------------------------------------------------------------------------

/// An alteration: a rational number of semitones, from −3 to +3 inclusive (ADR-0023 §2).
///
/// 1 is a sharp, −1 a flat, 2 a double sharp, `1/2` a quarter-tone sharp, `−3/2` a
/// three-quarter-tone flat, `1/3` a sixth-tone sharp. Which glyph draws it is an engraving choice
/// (F2B). Comma accidentals of just intonation (Helmholtz-Ellis, Sagittal) are not numbers of
/// semitones and are not represented here (deferred with tuning, ADR-0023 §2).
///
/// # Cost
/// 16 bytes (one [`Rational`]), `Copy`.
///
/// # Examples
/// ```
/// use orchestra_core_model::pitch::Alteration;
/// use orchestra_core_time::Rational;
///
/// let quarter_sharp = Alteration::new(Rational::new(1, 2)?)?;
/// assert!(!quarter_sharp.is_chromatic());
/// assert!(Alteration::new(Rational::from_integer(4)?).is_err());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Alteration(Rational);

impl Alteration {
    /// The largest alteration in magnitude, in semitones: a triple sharp or a triple flat.
    pub const MAX_SEMITONES: i64 = 3;

    /// No alteration.
    pub const NATURAL: Self = Self(Rational::ZERO);

    /// A sharp: +1 semitone.
    pub const SHARP: Self = Self(Rational::ONE);

    /// A flat: −1 semitone. Built with a `const` match because `Neg` is not `const`;
    /// `Rational::new(-1, 1)` cannot fail, so the fallback arm is never taken.
    pub const FLAT: Self = Self(match Rational::new(-1, 1) {
        Ok(r) => r,
        Err(_) => Rational::ZERO,
    });

    /// An alteration of `semitones`.
    ///
    /// # Errors
    /// [`PitchError::AlterationOutOfRange`] if `|semitones| > 3`.
    pub fn new(semitones: Rational) -> Result<Self, PitchError> {
        let max = int(Self::MAX_SEMITONES)?;
        if semitones > max || semitones < -max {
            return Err(PitchError::AlterationOutOfRange);
        }
        Ok(Self(semitones))
    }

    /// A chromatic alteration of a whole number of semitones (2 = double sharp, −1 = flat).
    ///
    /// # Errors
    /// [`PitchError::AlterationOutOfRange`] if `|semitones| > 3`.
    pub fn chromatic(semitones: i64) -> Result<Self, PitchError> {
        // `unsigned_abs` cannot overflow, unlike `abs` on `i64::MIN`.
        if semitones.unsigned_abs() > Self::MAX_SEMITONES.unsigned_abs() {
            return Err(PitchError::AlterationOutOfRange);
        }
        Ok(Self(int(semitones)?))
    }

    /// The alteration in semitones. O(1).
    #[must_use]
    pub const fn semitones(self) -> Rational {
        self.0
    }

    /// Whether the alteration is a whole number of semitones (natural, sharps, flats). O(1).
    #[must_use]
    pub const fn is_chromatic(self) -> bool {
        self.0.is_integer()
    }
}

// ------------------------------------------------------------------------------------------------
// Pitch
// ------------------------------------------------------------------------------------------------

/// A pitch as spelled, at concert pitch (ADR-0004, ADR-0023 §1).
///
/// Octaves are scientific: C4 is middle C (MIDI 60), and the octave number changes at C, so B♯3
/// sounds like C4 and C♭4 like B3. Equality is **spelling** equality: F♯4 ≠ G♭4. Use
/// [`sounds_like`](Self::sounds_like) for enharmonic equivalence.
///
/// # Cost
/// 24 bytes, `Copy`; every operation O(1) and allocation-free.
///
/// # Examples
/// ```
/// use orchestra_core_model::pitch::{Alteration, Pitch, Step};
///
/// let b_sharp_3 = Pitch::new(Step::B, Alteration::SHARP, 3);
/// let c_4 = Pitch::new(Step::C, Alteration::NATURAL, 4);
/// assert_ne!(b_sharp_3, c_4); // spelled differently…
/// assert!(b_sharp_3.sounds_like(c_4)?); // …same sound
/// assert_eq!(c_4.midi()?.key, 60);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Pitch {
    /// The letter.
    step: Step,
    /// The alteration (within ±3 semitones by construction of [`Alteration`]).
    alteration: Alteration,
    /// The scientific octave number (4 = the octave starting at middle C).
    octave: i8,
}

/// A MIDI key and the bend that brings it to the exact sounding pitch (ADR-0023 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MidiNote {
    /// The nearest MIDI key, 0–127 (60 = C4).
    pub key: u8,
    /// Semitones to add to `key` to reach the exact pitch, in `(−1/2, +1/2]`: a pitch exactly half
    /// way between two keys takes the lower key and a bend of `+1/2`. Zero for every chromatic
    /// pitch.
    pub bend: Rational,
}

impl Pitch {
    /// A pitch from its spelling. Infallible: every step, alteration and octave combination is a
    /// valid spelling (whether it is *playable* is answered by [`midi`](Self::midi)). O(1).
    #[must_use]
    pub const fn new(step: Step, alteration: Alteration, octave: i8) -> Self {
        Self {
            step,
            alteration,
            octave,
        }
    }

    /// The letter. O(1).
    #[must_use]
    pub const fn step(self) -> Step {
        self.step
    }

    /// The alteration. O(1).
    #[must_use]
    pub const fn alteration(self) -> Alteration {
        self.alteration
    }

    /// The scientific octave number. O(1).
    #[must_use]
    pub const fn octave(self) -> i8 {
        self.octave
    }

    /// The pitch's place in the sequence of all steps: `7 × octave + step index` (C4 = 28,
    /// D4 = 29, B3 = 27). The difference of two indices is the number of diatonic steps between
    /// the pitches, whatever their alterations. O(1); cannot overflow (`|octave| ≤ 128`).
    #[must_use]
    pub const fn diatonic_index(self) -> i64 {
        7 * (self.octave as i64) + self.step.index()
    }

    /// The semitones of the natural note of this step and octave on the MIDI scale (C4 = 60).
    /// O(1); bounded by about ±1550, cannot overflow.
    const fn natural_midi(step: Step, octave: i64) -> i64 {
        12 * (octave + 1) + step.natural_semitones()
    }

    /// The exact sounding pitch in semitones on the MIDI scale (C4 = 60, C♯4 = 61, a quarter-tone
    /// sharp C4 = 60½), with the 12-TET reference of ADR-0023 §3. O(1).
    ///
    /// # Errors
    /// [`PitchError::Arithmetic`] only for absurd alterations (not reachable through
    /// [`Alteration`], whose values are bounded).
    pub fn sounding(self) -> Result<Rational, PitchError> {
        let natural = Self::natural_midi(self.step, i64::from(self.octave));
        Ok(int(natural)?.checked_add(self.alteration.semitones())?)
    }

    /// Whether the two pitches sound the same (enharmonic equivalence), e.g. B♯3 and C4. O(1).
    ///
    /// # Errors
    /// See [`sounding`](Self::sounding).
    pub fn sounds_like(self, other: Self) -> Result<bool, PitchError> {
        Ok(self.sounding()? == other.sounding()?)
    }

    /// The nearest MIDI key and the bend to the exact pitch (ADR-0023 §3). O(1).
    ///
    /// The key is `⌈sounding − 1/2⌉`: the nearest integer, and the lower one on a tie (a quarter
    /// tone above C4 is C4 bent up by a half, never C♯4 bent down), so the result is deterministic
    /// and the bend lies in `(−1/2, +1/2]`.
    ///
    /// # Errors
    /// [`PitchError::MidiOutOfRange`] if the key is outside 0–127.
    pub fn midi(self) -> Result<MidiNote, PitchError> {
        // Fast path for chromatic alterations (almost every note): the key is a sum of small
        // integers and the bend is zero, with no rational arithmetic (≈ 10× cheaper, measured in
        // `benches/pitch.rs`). `|alteration| ≤ 3` and `|octave| ≤ 128`, so the sum cannot overflow.
        if let Some(alteration) = as_integer(self.alteration.semitones()) {
            let key = Self::natural_midi(self.step, i64::from(self.octave)) + alteration;
            let key = u8::try_from(key)
                .ok()
                .filter(|k| *k <= 127)
                .ok_or(PitchError::MidiOutOfRange)?;
            return Ok(MidiNote {
                key,
                bend: Rational::ZERO,
            });
        }
        // Microtonal alterations: nearest key, lower on a tie, through exact arithmetic.
        let exact = self.sounding()?;
        let half = Rational::new(1, 2)?;
        let key = exact.checked_sub(half)?.ceil();
        let key_u8 = u8::try_from(key)
            .ok()
            .filter(|k| *k <= 127)
            .ok_or(PitchError::MidiOutOfRange)?;
        let bend = exact.checked_sub(int(key)?)?;
        Ok(MidiNote { key: key_u8, bend })
    }

    /// This pitch moved by `interval`, with the spelling the interval implies (ADR-0023 §4).
    ///
    /// The steps are added to the diatonic index (which gives the new letter and octave); then the
    /// alteration is whatever makes the sounding pitch move by exactly the interval's semitones.
    /// C4 up a major third is E4; C4 up a diminished fourth is F♭4; F♯4 up an augmented sixth is
    /// D𝄪5. O(1).
    ///
    /// # Errors
    /// [`PitchError::AlterationOutOfRange`] if the new alteration would exceed a triple sharp or
    /// flat; [`PitchError::OctaveOutOfRange`] if the octave does not fit; arithmetic overflow.
    pub fn transposed(self, interval: Interval) -> Result<Self, PitchError> {
        // Bounded: |diatonic index| ≤ 7 × 128 + 6 and |steps| ≤ 2³¹, so the sum fits easily.
        let index = self.diatonic_index() + i64::from(interval.steps);
        let step = Step::from_index(index);
        let octave = i8::try_from(index.div_euclid(7)).map_err(|_| PitchError::OctaveOutOfRange)?;
        let target = self.sounding()?.checked_add(interval.semitones)?;
        let natural = int(Self::natural_midi(step, i64::from(octave)))?;
        let alteration = Alteration::new(target.checked_sub(natural)?)?;
        Ok(Self::new(step, alteration, octave))
    }

    /// The interval from this pitch to `other` (positive if `other` is higher in steps). O(1).
    ///
    /// # Errors
    /// Arithmetic overflow only (unreachable with valid alterations).
    pub fn interval_to(self, other: Self) -> Result<Interval, PitchError> {
        // Both indices are bounded by about ±900, so the difference fits an `i32`.
        let steps = other.diatonic_index() - self.diatonic_index();
        let steps = i32::try_from(steps).map_err(|_| PitchError::OctaveOutOfRange)?;
        let semitones = other.sounding()?.checked_sub(self.sounding()?)?;
        Ok(Interval { steps, semitones })
    }

    /// The spelling of MIDI `key` in `key_signature` (ADR-0023 §6).
    ///
    /// Among the spellings of the key's pitch class (places twelve apart on the line of fifths),
    /// the one inside a window of twelve consecutive fifths is chosen:
    /// - major and the church modes: `[k − 4, k + 7]` (in C major: C♯ E♭ F♯ A♭ B♭);
    /// - "no mode": the C major window `[−4, 7]`, whatever the signature;
    /// - minor: `[k − 3, k + 8]`, so that the raised leading tone is a sharp (A minor: G♯, E♭).
    ///
    /// The octave then follows from the key number, so the result always sounds exactly `key`
    /// (B♯3 for key 60 in C♯ major). Infallible: with at most fourteen fifths the window ends at
    /// place 22, a triple sharp at most. O(1).
    #[must_use]
    pub fn spell_midi(key: u8, key_signature: Key) -> Self {
        let Key::Traditional(k) = key_signature;
        // "No mode" spells as C major (k = 0), whatever the signature: without a tonic there is no
        // key context to follow, and ADR-0023 §6 fixes the neutral window.
        let start = match k.mode {
            Mode::Minor => i64::from(k.fifths) - 3,
            Mode::None => -4,
            _ => i64::from(k.fifths) - 4,
        };
        let key = i64::from(key);
        let pitch_class = key.rem_euclid(12);
        // The spellings of a pitch class `pc` are the places `t` with `7t ≡ pc (mod 12)`; since
        // 7 × 7 = 49 ≡ 1, the base solution is `t ≡ 7 pc`. Move it into the window.
        let base = (7 * pitch_class).rem_euclid(12);
        let t = start + (base - start).rem_euclid(12);
        let (step, alteration) = spelling_of_fifths(t);
        // `key − natural − alteration` is a multiple of 12 by construction (the spelling sounds
        // like the pitch class); the octave is that multiple minus one (MIDI 0 is C−1).
        let octave = (key - step.natural_semitones() - alteration).div_euclid(12) - 1;
        // |alteration| ≤ 3 (see above) and octave ∈ −2..=10: both conversions always succeed;
        // the fallbacks only keep the code free of panics.
        let alteration = Alteration::chromatic(alteration).unwrap_or(Alteration::NATURAL);
        let octave = i8::try_from(octave).unwrap_or(0);
        Self::new(step, alteration, octave)
    }
}

/// The step and the chromatic alteration (in semitones) of line-of-fifths place `t`: places seven
/// apart are the same letter with one more sharp, so the alteration is `⌊(t + 1) / 7⌋` (F at −1
/// starts the naturals). O(1).
const fn spelling_of_fifths(t: i64) -> (Step, i64) {
    (Step::from_fifths(t), (t + 1).div_euclid(7))
}

// ------------------------------------------------------------------------------------------------
// Interval
// ------------------------------------------------------------------------------------------------

/// The quality of an interval (ADR-0023 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quality {
    /// Perfect: unisons, fourths, fifths, octaves and their compounds.
    Perfect,
    /// Major: seconds, thirds, sixths, sevenths and their compounds.
    Major,
    /// Minor: one semitone smaller than major.
    Minor,
    /// Augmented `n` times (1 = augmented, 2 = doubly augmented…); `n ≥ 1`.
    Augmented(u32),
    /// Diminished `n` times (1 = diminished, 2 = doubly diminished…); `n ≥ 1`.
    Diminished(u32),
}

/// An interval: diatonic steps and semitones, both signed (negative = descending) (ADR-0023 §4).
///
/// `steps` is 0 for a unison, 2 for a third, 7 for an octave, 9 for a tenth; `semitones` is the
/// exact size, possibly microtonal. Transposing instruments are described by the interval from
/// written to sounding pitch (ADR-0023 §7), e.g. the B♭ clarinet by a descending major second.
///
/// # Examples
/// ```
/// use orchestra_core_model::pitch::{Interval, Quality};
///
/// let major_third = Interval::from_quality(Quality::Major, 3, false)?;
/// assert_eq!(major_third.quality()?, Some(Quality::Major));
/// let clarinet_in_b_flat = Interval::from_quality(Quality::Major, 2, true)?;
/// assert!(clarinet_in_b_flat.is_descending());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Interval {
    /// Diatonic steps, signed.
    steps: i32,
    /// Exact size in semitones, signed.
    semitones: Rational,
}

/// Semitones of the major or perfect interval of `simple` steps (0–6) above the lower note:
/// unison 0, second 2, third 4, fourth 5, fifth 7, sixth 9, seventh 11 (the natural notes above
/// C, which form a major scale). O(1).
const fn reference_semitones(simple: i64) -> i64 {
    Step::from_index(simple).natural_semitones()
}

/// Whether an interval of `simple` steps (0–6) belongs to the perfect class (unison, fourth,
/// fifth) rather than the major/minor class. O(1).
const fn is_perfect_class(simple: i64) -> bool {
    matches!(simple, 0 | 3 | 4)
}

impl Interval {
    /// An interval from its parts. Every combination is accepted (a second of 12 semitones is a
    /// very augmented second); [`quality`](Self::quality) names it. O(1).
    #[must_use]
    pub const fn new(steps: i32, semitones: Rational) -> Self {
        Self { steps, semitones }
    }

    /// The interval with this quality and number (1 = unison, 3 = third, 8 = octave, 10 =
    /// tenth), ascending or descending. A unison has no direction: `descending` is ignored for it,
    /// and a diminished unison (C to C♭) has −1 semitone. O(1).
    ///
    /// # Errors
    /// [`PitchError::InvalidQuality`] for number 0, a perfect second, a major fifth, an
    /// augmentation or diminution of degree 0, or a number too large for an `i32`.
    pub fn from_quality(
        quality: Quality,
        number: u32,
        descending: bool,
    ) -> Result<Self, PitchError> {
        let steps = i64::from(number.checked_sub(1).ok_or(PitchError::InvalidQuality)?);
        let simple = steps.rem_euclid(7);
        let reference = reference_semitones(simple) + 12 * steps.div_euclid(7);
        // The deviation `d` from the major/perfect interval, by the rules of ADR-0023 §4.
        let d: i64 = match (quality, is_perfect_class(simple)) {
            (Quality::Perfect, true) | (Quality::Major, false) => 0,
            (Quality::Minor, false) => -1,
            (Quality::Augmented(n), _) if n >= 1 => i64::from(n),
            (Quality::Diminished(n), true) if n >= 1 => -i64::from(n),
            (Quality::Diminished(n), false) if n >= 1 => -i64::from(n) - 1,
            _ => return Err(PitchError::InvalidQuality),
        };
        let size = reference + d;
        let sign = if descending && steps != 0 { -1 } else { 1 };
        let steps = i32::try_from(sign * steps).map_err(|_| PitchError::InvalidQuality)?;
        Ok(Self {
            steps,
            semitones: int(sign * size)?,
        })
    }

    /// Diatonic steps, signed. O(1).
    #[must_use]
    pub const fn steps(self) -> i32 {
        self.steps
    }

    /// Exact size in semitones, signed. O(1).
    #[must_use]
    pub const fn semitones(self) -> Rational {
        self.semitones
    }

    /// The interval number: 1 for a unison, 3 for a third, 8 for an octave, whatever the
    /// direction. O(1).
    #[must_use]
    pub const fn number(self) -> u32 {
        self.steps.unsigned_abs() + 1
    }

    /// Whether the interval goes down (negative steps). A unison is never descending. O(1).
    #[must_use]
    pub const fn is_descending(self) -> bool {
        self.steps < 0
    }

    /// The same interval in the opposite direction (a major third up becomes a major third down);
    /// used to go from sounding to written pitch (ADR-0023 §7). O(1).
    #[must_use]
    pub fn reversed(self) -> Self {
        Self {
            // `saturating_neg`: only `i32::MIN`, unreachable from real pitches, would overflow.
            steps: self.steps.saturating_neg(),
            semitones: -self.semitones,
        }
    }

    /// The quality, or `None` for a microtonal interval, which has no standard quality
    /// (ADR-0023 §4); its deviation is [`deviation`](Self::deviation). O(1).
    ///
    /// # Errors
    /// Arithmetic overflow only (absurd sizes).
    pub fn quality(self) -> Result<Option<Quality>, PitchError> {
        let d = self.deviation()?;
        let Some(d) = as_integer(d) else {
            return Ok(None);
        };
        let simple = i64::from(self.steps.unsigned_abs()).rem_euclid(7);
        // `d` is at most a few thousand for any interval between valid pitches; the conversion
        // to `u32` cannot fail there, and saturates for absurd hand-built intervals.
        let n = |x: i64| u32::try_from(x).unwrap_or(u32::MAX);
        let quality = if is_perfect_class(simple) {
            match d {
                0 => Quality::Perfect,
                d if d > 0 => Quality::Augmented(n(d)),
                d => Quality::Diminished(n(-d)),
            }
        } else {
            match d {
                0 => Quality::Major,
                -1 => Quality::Minor,
                d if d > 0 => Quality::Augmented(n(d)),
                d => Quality::Diminished(n(-d - 1)),
            }
        };
        Ok(Some(quality))
    }

    /// The semitones in the interval's direction minus those of the major or perfect interval of
    /// the same number: 0 for major and perfect, −1 for minor, +1 for augmented… A unison takes
    /// the sign of its semitones (C to C♭ deviates by −1). O(1).
    ///
    /// # Errors
    /// Arithmetic overflow only.
    pub fn deviation(self) -> Result<Rational, PitchError> {
        let steps = i64::from(self.steps.unsigned_abs());
        let reference = reference_semitones(steps.rem_euclid(7)) + 12 * steps.div_euclid(7);
        let along = if self.steps < 0 {
            -self.semitones
        } else {
            self.semitones
        };
        Ok(along.checked_sub(int(reference)?)?)
    }

    /// How far the interval moves along the line of fifths: `7 × semitones − 12 × steps` (a
    /// perfect fifth +1, a major second +2, a minor second −5, an octave 0). This is the shift of a
    /// key signature transposed by the interval (ADR-0023 §7). O(1).
    ///
    /// # Errors
    /// [`PitchError::MicrotonalKey`] for a microtonal interval; arithmetic overflow.
    pub fn fifths(self) -> Result<i64, PitchError> {
        let semitones = as_integer(self.semitones).ok_or(PitchError::MicrotonalKey)?;
        let seven = semitones
            .checked_mul(7)
            .ok_or(PitchError::Arithmetic(RationalError::Overflow))?;
        seven
            .checked_sub(12 * i64::from(self.steps))
            .ok_or(PitchError::Arithmetic(RationalError::Overflow))
    }
}

// ------------------------------------------------------------------------------------------------
// Keys
// ------------------------------------------------------------------------------------------------

/// The mode of a traditional key (ADR-0023 §5). It does not change the signature; it names the
/// tonic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Major (Ionian).
    Major,
    /// Dorian.
    Dorian,
    /// Phrygian.
    Phrygian,
    /// Lydian.
    Lydian,
    /// Mixolydian.
    Mixolydian,
    /// Minor (Aeolian).
    Minor,
    /// Locrian.
    Locrian,
    /// No mode: atonal music, or an open (empty) signature without a tonic.
    None,
}

impl Mode {
    /// The tonic's place on the line of fifths relative to the key's fifths: major 0, Dorian +2,
    /// Phrygian +4, Lydian −1, Mixolydian +1, minor +3, Locrian +5; `None` has no tonic. O(1).
    #[must_use]
    pub const fn tonic_offset(self) -> Option<i64> {
        match self {
            Self::Major => Some(0),
            Self::Dorian => Some(2),
            Self::Phrygian => Some(4),
            Self::Lydian => Some(-1),
            Self::Mixolydian => Some(1),
            Self::Minor => Some(3),
            Self::Locrian => Some(5),
            Self::None => None,
        }
    }
}

/// A traditional key signature: sharps (positive) or flats (negative) in the order of fifths,
/// from −14 to +14, and a mode (ADR-0023 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraditionalKey {
    /// Number of sharps (positive) or flats (negative); |fifths| ≤ 14 by construction.
    fifths: i8,
    /// The mode.
    mode: Mode,
}

/// A key signature (ADR-0023 §5).
///
/// An enum with one variant from the start: non-traditional signatures (free lists of steps and
/// alterations, as in Bartók, klezmer and Balkan modes, maqam and makam) are deferred by the
/// owner's choice and will be a second variant (the MusicXML import after F2B, or the first
/// concrete need).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A signature in the order of fifths.
    Traditional(TraditionalKey),
}

impl TraditionalKey {
    /// The largest number of sharps or flats: fourteen (theoretical keys from transposition).
    pub const MAX_FIFTHS: i8 = 14;

    /// A key of `fifths` sharps (positive) or flats (negative) in `mode`. O(1).
    ///
    /// # Errors
    /// [`PitchError::KeyOutOfRange`] if `|fifths| > 14`.
    pub fn new(fifths: i8, mode: Mode) -> Result<Self, PitchError> {
        if fifths.unsigned_abs() > Self::MAX_FIFTHS.unsigned_abs() {
            return Err(PitchError::KeyOutOfRange);
        }
        Ok(Self { fifths, mode })
    }

    /// Number of sharps (positive) or flats (negative). O(1).
    #[must_use]
    pub const fn fifths(self) -> i8 {
        self.fifths
    }

    /// The mode. O(1).
    #[must_use]
    pub const fn mode(self) -> Mode {
        self.mode
    }

    /// The alteration this key gives to `step`, in whole semitones (ADR-0023 §5).
    ///
    /// The seven spellings of a key with `k` fifths are the places `[k − 1, k + 5]` of the line of
    /// fifths; the step at natural place `t` takes the place `t + 7j` inside that window, so its
    /// alteration is `j = ⌈(k − 1 − t) / 7⌉` sharps. With `|k| ≤ 14`, `|j| ≤ 2`. O(1).
    #[must_use]
    pub fn alteration_of(self, step: Step) -> Alteration {
        let start = i64::from(self.fifths) - 1;
        let j = (start - step.natural_fifths() + 6).div_euclid(7);
        // |j| ≤ 2 (see above): the conversion always succeeds.
        Alteration::chromatic(j).unwrap_or(Alteration::NATURAL)
    }

    /// The tonic's step and alteration, or `None` for [`Mode::None`]. O(1).
    #[must_use]
    pub fn tonic(self) -> Option<(Step, Alteration)> {
        let t = i64::from(self.fifths) + self.mode.tonic_offset()?;
        let (step, alteration) = spelling_of_fifths(t);
        // The tonic lies in [k − 1, k + 5], so |alteration| ≤ 3.
        Some((step, Alteration::chromatic(alteration).ok()?))
    }

    /// This key moved by `interval` along the line of fifths (ADR-0023 §7): the written key of a
    /// B♭ clarinet is the concert key transposed by a major second up, two more sharps. O(1).
    ///
    /// # Errors
    /// [`PitchError::MicrotonalKey`] for a microtonal interval; [`PitchError::KeyOutOfRange`]
    /// beyond fourteen sharps or flats.
    pub fn transposed(self, interval: Interval) -> Result<Self, PitchError> {
        let fifths = i64::from(self.fifths)
            .checked_add(interval.fifths()?)
            .ok_or(PitchError::KeyOutOfRange)?;
        let fifths = i8::try_from(fifths).map_err(|_| PitchError::KeyOutOfRange)?;
        Self::new(fifths, self.mode)
    }
}

impl Key {
    /// A traditional key; see [`TraditionalKey::new`].
    ///
    /// # Errors
    /// [`PitchError::KeyOutOfRange`] if `|fifths| > 14`.
    pub fn traditional(fifths: i8, mode: Mode) -> Result<Self, PitchError> {
        Ok(Self::Traditional(TraditionalKey::new(fifths, mode)?))
    }

    /// The alteration this key gives to `step`; see [`TraditionalKey::alteration_of`].
    #[must_use]
    pub fn alteration_of(self, step: Step) -> Alteration {
        match self {
            Self::Traditional(k) => k.alteration_of(step),
        }
    }

    /// This key transposed by `interval`; see [`TraditionalKey::transposed`].
    ///
    /// # Errors
    /// As [`TraditionalKey::transposed`].
    pub fn transposed(self, interval: Interval) -> Result<Self, PitchError> {
        match self {
            Self::Traditional(k) => Ok(Self::Traditional(k.transposed(interval)?)),
        }
    }
}
