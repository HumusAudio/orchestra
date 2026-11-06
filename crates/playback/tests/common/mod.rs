// SPDX-License-Identifier: GPL-3.0-or-later

//! The random scores shared by the property tests of the played notes (`perform_props`) and of
//! their incremental update (`update_props`): one instrument, bars 1–2 repeated, the bars filled
//! with quarters, eighths and rests, and optionally the extras (ties, grace notes, articulations,
//! arpeggios, offsets, dynamic accents, hairpins) and dynamics with a level.

// Helper functions are outside the scope of clippy.toml's `allow-unwrap-in-tests`.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers over their own small tables: a failure fails the test"
)]
#![allow(
    dead_code,
    reason = "each test crate uses its own part of the generator"
)]

use orchestra_core_model::{
    Alteration, Arpeggio, Articulation, ArticulationSet, BaseValue, Clef, Document,
    DocumentBuilder, DynamicLevel, DynamicMark, DynamicPlacement, Grace, GraceAttach, GraceKind,
    HairpinShape, Interval, Key, Metronome, Mode, NoteValue, PerformanceOffsets, Pitch, Placement,
    RestValue, ScorePoint, Step, TempoMark, Tie,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};

/// What a random score holds besides plain notes.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Extras {
    /// Ties, grace notes, articulations, arpeggios, offsets, dynamic accents and hairpins at all.
    pub(crate) any: bool,
    /// Dynamics with a level, on a quarter of the bars' beats.
    pub(crate) dynamics: bool,
}

/// The levels the generator writes, with their value on the 0–127 scale for the oracle (ADR-0027
/// §4, written here again rather than taken from the library).
pub(crate) const LEVELS: [(DynamicLevel, u16); 6] = [
    (DynamicLevel::Ppppp, 10),
    (DynamicLevel::Pp, 36),
    (DynamicLevel::P, 49),
    (DynamicLevel::Mf, 75),
    (DynamicLevel::Ff, 101),
    (DynamicLevel::Ffffff, 127),
];

#[allow(
    clippy::too_many_lines,
    clippy::many_single_char_names,
    reason = "one generator, its short names are the usual ones (n bars, b builder, r random)"
)]
/// A random score of `n` bars: bars 1–2 repeated, each bar filled with quarters, eighths and rests
/// chosen by `seed`, with the extras if `extras.any`.
pub(crate) fn random(n: usize, seed: &[u8], extras: Extras) -> Document {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(
        Duration::from_rational(Rational::new(1, 4).unwrap()),
        Rational::from_integer(120).unwrap(),
    )
    .unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars: Vec<_> = (0..n).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Piano", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let instrument = b.document().track(track).unwrap().instrument();
    let (voice, staff, piano) = (
        instrument.voices()[0].id(),
        instrument.staves()[0].id(),
        instrument.id(),
    );
    b.add_repeat_start(bars[1]).unwrap();
    b.add_repeat_end(bars[2], 2).unwrap();
    let steps = [
        Step::C,
        Step::D,
        Step::E,
        Step::F,
        Step::G,
        Step::A,
        Step::B,
    ];
    let mut k = 0_usize;
    let mut next = || {
        k += 1;
        seed[k % seed.len()]
    };
    let mut previous: Option<(orchestra_core_model::NoteId, Step)> = None;
    for &bar in &bars {
        let mut offset = 0_i64; // in eighths
        while offset < 8 {
            let r = next();
            let length = if r % 3 == 0 && offset + 2 <= 8 { 2 } else { 1 };
            let at = Placement::new(
                voice,
                bar,
                Duration::from_rational(Rational::new(offset, 8).unwrap()),
            );
            let value = if length == 2 {
                NoteValue::plain(BaseValue::Quarter)
            } else {
                NoteValue::plain(BaseValue::Eighth)
            };
            if r % 7 == 0 {
                b.add_rest(at, RestValue::Value(value)).unwrap();
                previous = None;
            } else {
                if extras.any && r % 5 == 0 {
                    let kind = if r % 2 == 0 {
                        GraceKind::Acciaccatura
                    } else {
                        GraceKind::Appoggiatura
                    };
                    b.add_grace_chord(
                        at,
                        Grace {
                            kind,
                            attach: GraceAttach::Before,
                        },
                        NoteValue::plain(BaseValue::Sixteenth),
                        &[Pitch::new(Step::B, Alteration::NATURAL, 4)],
                    )
                    .unwrap();
                }
                // A tie to the previous note needs the same pitch.
                let step = match previous {
                    Some((_, s)) if extras.any && r % 4 == 1 => s,
                    _ => steps[usize::from(next()) % steps.len()],
                };
                let pitches = if extras.any && r % 9 == 0 {
                    vec![
                        Pitch::new(step, Alteration::NATURAL, 4),
                        Pitch::new(step, Alteration::NATURAL, 5),
                    ]
                } else {
                    vec![Pitch::new(step, Alteration::NATURAL, 4)]
                };
                let ids = b.add_chord(at, value, &pitches).unwrap();
                if extras.any {
                    if let Some((prev, s)) = previous
                        && s == step
                        && r % 4 == 1
                    {
                        b.set_tie(prev, Tie::To(ids.notes[0])).unwrap();
                    }
                    let set = match r % 6 {
                        0 => ArticulationSet::EMPTY.with(Articulation::Staccato),
                        1 => ArticulationSet::EMPTY.with(Articulation::Accent),
                        2 => ArticulationSet::EMPTY
                            .with(Articulation::Tenuto)
                            .with(Articulation::Staccato),
                        _ => ArticulationSet::EMPTY,
                    };
                    b.set_articulations(ids.event, set).unwrap();
                    if pitches.len() > 1 {
                        b.set_arpeggio(ids.event, Some(Arpeggio::Up)).unwrap();
                    }
                    if r % 8 == 3 {
                        b.set_offsets(
                            ids.notes[0],
                            PerformanceOffsets {
                                onset: Duration::from_rational(Rational::new(-1, 64).unwrap()),
                                length: Duration::from_rational(Rational::new(-1, 2).unwrap()),
                                velocity: i32::from(next()) * 1000 - 100_000,
                            },
                        )
                        .unwrap();
                    }
                }
                previous = Some((ids.notes[0], step));
            }
            offset += length;
        }
    }
    // Dynamics: per bar at most one level (on a beat) and, with the extras, one dynamic accent on
    // another beat and one hairpin (within the bar or up to the next barline), so that no two
    // dynamics share a point and no two hairpins overlap.
    if extras.dynamics {
        let quarter = |k: u8| Duration::from_rational(Rational::new(i64::from(k), 4).unwrap());
        for (i, &bar) in bars.iter().enumerate() {
            let r = next();
            let beat = r % 4;
            if r % 3 == 0 {
                let level = LEVELS[usize::from(next()) % LEVELS.len()].0;
                b.add_dynamic(
                    piano,
                    ScorePoint::new(bar, quarter(beat)),
                    DynamicPlacement::instrument(staff),
                    DynamicMark::Level(level),
                )
                .unwrap();
            }
            if extras.any {
                let marks = [
                    DynamicMark::Sf,
                    DynamicMark::Rfz,
                    DynamicMark::Sffz,
                    DynamicMark::Fp,
                    DynamicMark::Sfpp,
                    DynamicMark::Pf,
                    DynamicMark::Niente,
                ];
                if r % 5 < 3 {
                    b.add_dynamic(
                        piano,
                        ScorePoint::new(bar, quarter((beat + 2) % 4)),
                        DynamicPlacement::instrument(staff),
                        marks[usize::from(next()) % marks.len()].clone(),
                    )
                    .unwrap();
                }
                let h = next();
                if h % 2 == 0 {
                    let end = if h % 4 == 0 && i + 1 < bars.len() {
                        ScorePoint::new(bars[i + 1], Duration::ZERO)
                    } else {
                        ScorePoint::new(bar, quarter(1 + h % 3))
                    };
                    let mut shape = if h % 3 == 0 {
                        HairpinShape::CRESCENDO
                    } else {
                        HairpinShape::DIMINUENDO
                    };
                    shape.niente = h % 5 == 0;
                    b.add_hairpin(
                        piano,
                        ScorePoint::new(bar, Duration::ZERO),
                        end,
                        DynamicPlacement::instrument(staff),
                        shape,
                    )
                    .unwrap();
                }
            }
        }
    }
    b.build().unwrap()
}
