// SPDX-License-Identifier: GPL-3.0-or-later

//! The **plan of one movement** of the full preset: 100 bars in five sections, with their meters,
//! keys, tempo marks, harmony and form (ADR-0031 §6; the specification approved by the owner on
//! 2026-11-07, see the [preset's documentation](super)).
//!
//! # Bar numbers
//! Everything here speaks in **local** bar indices `0..100`, 0-based: the bar index inside one
//! movement. The heavy score repeats the movement ten times, so the document bar of local bar `l`
//! of movement `m` is `100 m + l`. The specification table counts bars from 1 (the pickup is bar
//! 1 there); the code counts from 0 (the pickup is local bar 0), so "b. 43" of the table is local
//! bar 42 here.
//!
//! | Local bars | Section | Meter | Key (movement 0) | Form |
//! |---|---|---|---|---|
//! | 0 | pickup (a quarter, uncounted) | 4/4 | E♭ major | — |
//! | 1–17 | A | 4/4 | E♭ major | repeat 1–16; volta 1 = 16, volta 2 = 17; the horn only plays the 2nd time |
//! | 18–41 | B | 6/8 (3+3), hemiola 2+2+2 in 40 | C minor | bar repeats in violin II and viola |
//! | 42–59 | C | 4/4 | G major | segno at 42, "To Coda" at the end of 59 |
//! | 60–79 | D | 3/4, free cadenza bar 69 | B♭ major | D.S. al Coda at the end of 79 |
//! | 80–99 | Coda | 4/4 | E♭ major | coda at 80; final fermata |
//!
//! # The path
//! 0, 1–15, 16 (volta 1), 1–15, 17 (volta 2), 18–79, then the D.S. back to 42–59 and the coda
//! 80–99: `1 + 16 + 16 + 62 + 18 + 20 = 133` bars played per movement, which `tests/full.rs`
//! checks against the real unrolling.

use orchestra_core_model::{
    BarId, DocumentBuilder, DynamicScope, GradualMark, InstrumentId, JumpFires, JumpSpec,
    JumpTarget, Key, MarkerKind, Metronome, Mode, PassSet, RepeatsAfterJump, ScorePoint, TempoMark,
    TempoMarkChange,
};
use orchestra_core_time::{
    BarIndex, BarKind, BeatGrouping, Duration, IrregularBar, Meter, MeterChange, MeterMap,
    Rational, Tempo,
};

/// Bars of one movement.
pub(super) const BARS: u32 = 100;

/// The five sections of a movement, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    /// The pickup bar and the repeated section with its two voltas (local bars 0–17).
    A,
    /// The 6/8 section in the relative minor (18–41).
    B,
    /// The 4/4 section starting with the segno (42–59).
    C,
    /// The 3/4 section with the free cadenza bar and the D.S. al Coda (60–79).
    D,
    /// The coda (80–99).
    Coda,
}

impl Section {
    /// Every section, in order: used to walk the section starts.
    pub(super) const ALL: [Self; 5] = [Self::A, Self::B, Self::C, Self::D, Self::Coda];

    /// The section holding local bar `local` (`0..100`). O(1).
    pub(super) const fn of(local: u32) -> Self {
        match local {
            0..=17 => Self::A,
            18..=41 => Self::B,
            42..=59 => Self::C,
            60..=79 => Self::D,
            _ => Self::Coda,
        }
    }

    /// The first local bar where the section's music starts: for A it is bar 1, after the pickup
    /// (the pickup belongs to A but is a lead-in, not the start of a phrase). O(1).
    pub(super) const fn start(self) -> u32 {
        match self {
            Self::A => 1,
            Self::B => 18,
            Self::C => 42,
            Self::D => 60,
            Self::Coda => 80,
        }
    }

    /// The last local bar of the section. O(1).
    pub(super) const fn last(self) -> u32 {
        match self {
            Self::A => 17,
            Self::B => 41,
            Self::C => 59,
            Self::D => 79,
            Self::Coda => 99,
        }
    }

    /// The concert key of the section in movement 0, as (fifths, mode): E♭ major, C minor (its
    /// relative), G major, B♭ major, E♭ major. Each movement of the heavy score shifts every
    /// fifths count by its own [`movement_shift`]. O(1).
    pub(super) const fn key(self) -> (i8, Mode) {
        match self {
            Self::A | Self::Coda => (-3, Mode::Major),
            Self::B => (-3, Mode::Minor),
            Self::C => (1, Mode::Major),
            Self::D => (-2, Mode::Major),
        }
    }
}

/// The shift, in fifths, of every key of movement `m` (0-based) relative to movement 0, so that
/// each of the heavy score's ten movements starts in its own key.
///
/// The keys of the plan span −3 to +1 fifths, and [`Key`] accepts up to ±14, but the written keys
/// of transposing instruments add up to two more fifths; the shifts stay within −4..=+5 so that
/// every concert key stays within ±7 (the traditional range) and every written one within ±9,
/// which [`super::music::alteration`] spells with at most double sharps or flats. Movement 0 has
/// no shift, so the normal score's keys are exactly the table's. O(1).
pub(super) const fn movement_shift(m: u32) -> i8 {
    // A fixed, varied sequence; any index beyond it (there is none today) wraps around.
    const SHIFTS: [i8; 10] = [0, 2, -1, 4, -3, 1, 5, -2, 3, -4];
    SHIFTS[(m % 10) as usize]
}

/// The kind of a local bar: what its length is and which rhythm cells fit it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BarShape {
    /// The pickup: a quarter at the end of a 4/4 bar.
    Pickup,
    /// A bar of 4/4.
    Four,
    /// A bar of 6/8 counted in two (3+3).
    SixEight,
    /// The hemiola bar of 6/8, counted 2+2+2 (three quarters).
    Hemiola,
    /// A bar of 3/4.
    ThreeFour,
    /// The free cadenza bar: no meter, an explicit length of 5/4.
    Free,
}

impl BarShape {
    /// The shape of local bar `local`. O(1).
    pub(super) const fn of(local: u32) -> Self {
        match local {
            0 => Self::Pickup,
            40 => Self::Hemiola,
            18..=41 => Self::SixEight,
            69 => Self::Free,
            60..=79 => Self::ThreeFour,
            _ => Self::Four,
        }
    }

    /// The bar's length in sixteenths (the unit of the rhythm cells of [`super::music`]). O(1).
    pub(super) const fn sixteenths(self) -> i64 {
        match self {
            Self::Pickup => 4,
            Self::Four => 16,
            Self::SixEight | Self::Hemiola | Self::ThreeFour => 12,
            Self::Free => 20,
        }
    }
}

/// The position of local bar `local` inside its 8-bar phrase, `0..8`: drives the harmony and the
/// cadences.
///
/// Phrases start at the section starts and run every 8 bars. Two exceptions: volta 2 (local 17)
/// replaces volta 1 (local 16), so it is a cadence like it (position 7); the pickup is position 0.
/// The free bar has a position too, but no harmony is read for it (the cadenza has its own). O(1).
pub(super) const fn phrase_position(local: u32) -> u32 {
    if local == 0 {
        return 0;
    }
    if local == 17 {
        return 7;
    }
    (local - Section::of(local).start()) % 8
}

/// Whether local bar `local` ends a phrase: the last bar of an 8-bar phrase or of a section, or
/// the bar before the cadenza. The melody plays its cadence there (a long note with a trill). O(1).
pub(super) const fn is_cadence(local: u32) -> bool {
    phrase_position(local) == 7 || local == Section::of(local).last() || local == 68
}

/// The harmony of a phrase position, as the scale degree of the chord's root above the tonic
/// (0 = I, 3 = IV, 4 = V, 5 = vi): I IV V I vi IV V I, the most ordinary of phrases, which is the
/// point (a plausible score, not an interesting one). In the minor section the same degrees give
/// i iv v i VI iv v i. O(1).
pub(super) const fn chord_root(position: u32) -> i64 {
    const PROGRESSION: [i64; 8] = [0, 3, 4, 0, 5, 3, 4, 0];
    PROGRESSION[(position % 8) as usize]
}

/// The duration `n/den` of a whole note (a test helper: the generator only ever passes valid
/// constants, ADR-0031 §5).
pub(super) fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// A tempo of `bpm` beats of `unit` (a quarter is `d(1, 4)`, a dotted quarter `d(3, 8)`).
fn tempo(unit: Duration, bpm: i64) -> Tempo {
    Tempo::new(unit, Rational::from_integer(bpm).unwrap()).unwrap()
}

/// A quarter-note tempo.
fn quarter(bpm: i64) -> Tempo {
    tempo(d(1, 4), bpm)
}

/// The initial tempo mark of the document: "Andante", ♩ = 72, shown.
pub(super) fn initial_tempo() -> TempoMark {
    TempoMark::immediate(Some("Andante"), Metronome::shown(quarter(72))).unwrap()
}

/// The meter map of `movements` movements (see the module documentation).
///
/// Per movement: the pickup (an `AlignEnd` irregular bar of a quarter, uncounted, so the first
/// full bar is bar 1 as musicians number it), 6/8 grouped 3+3 at 18 with the hemiola 2+2+2 at 40,
/// 4/4 at 42, 3/4 at 60 with the free bar of 5/4 at 69, and 4/4 again at 80. Movements after the
/// first start with their own pickup inside the 4/4 of the previous coda, so they need no meter
/// change at their start. O(movements × log) for the map's sorted inserts.
pub(super) fn meter_map(movements: u32) -> MeterMap {
    let plain = |n: u8, den: u16| MeterChange::new(Meter::new(&[n], den).unwrap());
    let mut map = MeterMap::new(plain(4, 4)).unwrap();
    for m in 0..movements {
        let at = |local: u32| BarIndex::new(m * BARS + local);
        map.set_irregular_bar(
            at(0),
            IrregularBar {
                length: d(1, 4),
                kind: BarKind::AlignEnd,
            },
        )
        .unwrap();
        map.set_numbering(at(0), orchestra_core_time::BarCounting::Uncounted);
        map.set_meter_change(
            at(18),
            MeterChange::with_grouping(
                Meter::new(&[6], 8).unwrap(),
                BeatGrouping::new(&[3, 3], 8).unwrap(),
            ),
        )
        .unwrap();
        map.set_bar_grouping(at(40), BeatGrouping::new(&[2, 2, 2], 8).unwrap())
            .unwrap();
        map.set_meter_change(at(42), plain(4, 4)).unwrap();
        map.set_meter_change(at(60), plain(3, 4)).unwrap();
        map.set_irregular_bar(
            at(69),
            IrregularBar {
                length: d(5, 4),
                kind: BarKind::Free,
            },
        )
        .unwrap();
        map.set_meter_change(at(80), plain(4, 4)).unwrap();
    }
    map
}

/// The concert key of `section` in movement `m`.
pub(super) fn key(section: Section, m: u32) -> Key {
    let (fifths, mode) = section.key();
    Key::traditional(fifths + movement_shift(m), mode).unwrap()
}

/// Adds the score-wide marks of movement `m` whose bars are `bars` (the document's bars of that
/// movement, `bars.len() == 100`): key changes, tempo marks, the repeat with its voltas, the
/// markers and the D.S. al Coda.
///
/// # Why `RepeatsAfterJump::Play`?
/// After a jump that skips the repeats, the unrolling skips **every** later repeat to the end of
/// the document (ADR-0025 §8: "every section is played once"). That is right for one piece, but
/// the heavy score puts ten movements in one document, and movements 2–10 would then lose their
/// repeats. With `Play` the repeats after the jump are played as written: in a movement this
/// changes nothing (the D.S. goes back to section C, which has no repeat), and every movement
/// unrolls the same 133 bars.
pub(super) fn add_score_marks(b: &mut DocumentBuilder, bars: &[BarId], m: u32) {
    // Keys: movement 0 starts with the document's initial key; later movements change key at their
    // pickup. Then one key change per section start.
    if m > 0 {
        b.add_key_change(bars[0], Duration::ZERO, key(Section::A, m))
            .unwrap();
    }
    for section in [Section::B, Section::C, Section::D, Section::Coda] {
        b.add_key_change(
            bars[section.start() as usize],
            Duration::ZERO,
            key(section, m),
        )
        .unwrap();
    }
    add_tempo_marks(b, bars, m);
    add_form(b, bars);
}

/// The tempo marks of one movement: "Andante" (later movements), "Allegretto" ♩. = 60 at 18 (a
/// dotted-quarter beat, as 6/8 is counted), "poco più mosso" with a hidden metronome at 42, "ad
/// lib." (text only) on the cadenza, "a tempo" at 70, a "rit." with its starting tempo from 76 to
/// 79, "Tempo I" at the coda and a final "rit." (no starting tempo) from 96 to 99.
fn add_tempo_marks(b: &mut DocumentBuilder, bars: &[BarId], m: u32) {
    let at = |local: usize| ScorePoint::start_of(bars[local]);
    if m > 0 {
        b.add_tempo_mark(at(0), initial_tempo()).unwrap();
    }
    let marks = [
        (
            18,
            TempoMark::immediate(Some("Allegretto"), Metronome::shown(tempo(d(3, 8), 60))),
        ),
        (
            42,
            TempoMark::immediate(Some("poco più mosso"), Metronome::hidden(quarter(84))),
        ),
        (69, TempoMark::new(Some("ad lib."), None)),
        (
            70,
            TempoMark::immediate(Some("a tempo"), Metronome::hidden(quarter(84))),
        ),
        (
            76,
            TempoMark::new(
                Some("rit."),
                Some(TempoMarkChange::Gradual(GradualMark {
                    from: Some(Metronome::shown(quarter(84))),
                    to: quarter(60),
                    end: at(79),
                })),
            ),
        ),
        (
            80,
            TempoMark::immediate(Some("Tempo I"), Metronome::shown(quarter(72))),
        ),
        (
            96,
            TempoMark::new(
                Some("rit."),
                Some(TempoMarkChange::Gradual(GradualMark {
                    from: None,
                    to: quarter(50),
                    end: at(99),
                })),
            ),
        ),
    ];
    for (local, mark) in marks {
        b.add_tempo_mark(at(local), mark.unwrap()).unwrap();
    }
}

/// The form of one movement: the repeat of A with its two voltas, the segno, the "To Coda", the
/// coda and the D.S. al Coda (see [`add_score_marks`] for the choice of `Play`).
fn add_form(b: &mut DocumentBuilder, bars: &[BarId]) {
    b.add_repeat_start(bars[1]).unwrap();
    b.add_repeat_end(bars[16], 2).unwrap();
    b.add_volta(bars[16], bars[16], PassSet::of(&[1]).unwrap(), true, None)
        .unwrap();
    b.add_volta(bars[17], bars[17], PassSet::of(&[2]).unwrap(), false, None)
        .unwrap();
    let segno = b.add_marker(bars[42], MarkerKind::Segno, None).unwrap();
    let to_coda = b.add_marker(bars[59], MarkerKind::ToCoda, None).unwrap();
    let coda = b.add_marker(bars[80], MarkerKind::Coda, None).unwrap();
    b.add_jump(
        bars[79],
        JumpSpec {
            to: JumpTarget::Marker(segno),
            until: Some(to_coda),
            then: Some(coda),
            repeats: RepeatsAfterJump::Play,
            fires: JumpFires::AfterRepeats,
        },
        Some("D.S. al Coda"),
    )
    .unwrap();
}

/// The pass rule of an instrument that plays the repeat of A only the second time ("2nd time
/// only", ADR-0025 §7): the range lies inside the repeated section (local 1 to the start of volta
/// 1, the end being exclusive), as `validate` requires.
pub(super) fn add_second_time_only(b: &mut DocumentBuilder, bars: &[BarId], who: InstrumentId) {
    b.add_pass_rule(
        who,
        ScorePoint::start_of(bars[1]),
        ScorePoint::start_of(bars[16]),
        DynamicScope::Instrument,
        PassSet::of(&[2]).unwrap(),
        Some("2nd time only"),
    )
    .unwrap();
}
