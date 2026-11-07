// SPDX-License-Identifier: GPL-3.0-or-later

//! The **markings of each instrument** in the full preset: dynamics, hairpins, clef changes, the
//! key signature of the instruments written without one, bar repeats and the pass rule.
//!
//! Everything here is placed at bar starts or simple offsets of the plan ([`super::plan`]), so it
//! never depends on the notes the voices happened to get: the markings stay where a copyist would
//! put them, whatever the random rhythms are.

use orchestra_core_model::{
    BarId, BarRepeatSpan, Clef, DocumentBuilder, DynamicLevel, DynamicMark, DynamicPlacement,
    HairpinKind, HairpinShape, HairpinStyle, KeyOverride, KeySignature, OtherDynamic, ScorePoint,
};
use orchestra_core_time::Duration;

use super::ensemble::{Built, Family, InstrumentSpec, Role};
use super::plan::{self, Section};

/// The first bars of the 8-bar phrases that get a hairpin pair (a crescendo over bars 2–3 of the
/// phrase, a diminuendo over bars 4–5): every full phrase of every section except D's second one,
/// which the cadenza interrupts.
const PHRASES: [u32; 10] = [1, 9, 18, 26, 34, 42, 50, 60, 80, 88];

/// Adds the markings of one instrument in movement `m`, whose bars are `bars`.
pub(super) fn add_instrument_marks(
    b: &mut DocumentBuilder,
    spec: &InstrumentSpec,
    built: &Built,
    bars: &[BarId],
    m: u32,
) {
    if spec.no_signature && m == 0 {
        // From the very first bar, for the whole score: the key override holds until another.
        b.add_key_override(
            built.instrument,
            bars[0],
            Duration::ZERO,
            KeyOverride::Signature(KeySignature::NoSignature),
        )
        .unwrap();
    }
    if spec.second_time_only {
        plan::add_second_time_only(b, bars, built.instrument);
    }
    if spec.bar_repeats {
        add_bar_repeats(b, built, bars);
    }
    add_clef_changes(b, spec, built, bars);
    add_dynamics(b, spec, built, bars);
    add_hairpins(b, spec, built, bars);
}

/// The bar repeats of section B on the instrument's first staff: the one-bar sign in bar 21
/// (repeating 20) and the two-bar sign over 30–31 (repeating 28–29). The voices leave those bars
/// empty (`music::Part::plays`), and section B has no form marking, as `validate` requires.
fn add_bar_repeats(b: &mut DocumentBuilder, built: &Built, bars: &[BarId]) {
    let staff = built.staves[0];
    b.add_bar_repeat(staff, bars[21], 1, BarRepeatSpan::One)
        .unwrap();
    b.add_bar_repeat(staff, bars[30], 2, BarRepeatSpan::Two)
        .unwrap();
}

/// The clef changes: the tenor clef for section D on the instruments with a tenor passage (back to
/// the bass clef at the coda); the piano's left hand in the treble clef for bars 42–49.
fn add_clef_changes(b: &mut DocumentBuilder, spec: &InstrumentSpec, built: &Built, bars: &[BarId]) {
    if spec.tenor_passage {
        b.add_clef_change(built.staves[0], bars[60], Duration::ZERO, Clef::TENOR)
            .unwrap();
        b.add_clef_change(built.staves[0], bars[80], Duration::ZERO, Clef::BASS)
            .unwrap();
    }
    if built.voices.iter().any(|(_, v)| v.role == Role::PianoLeft) {
        b.add_clef_change(built.staves[1], bars[42], Duration::ZERO, Clef::TREBLE)
            .unwrap();
        b.add_clef_change(built.staves[1], bars[50], Duration::ZERO, Clef::BASS)
            .unwrap();
    }
}

/// The dynamics: a level for the whole instrument at every section start (p, mf, f, mp, p), except
/// "dolce" (a text with the level mp) for the soloist at A and fp for the held parts at D; sfz on
/// the strings two bars into C; pp for everybody at the final rit.; and the piano's inner voice
/// gets its own pp (a dynamic of one voice, ADR-0024 §8) at A and C.
fn add_dynamics(b: &mut DocumentBuilder, spec: &InstrumentSpec, built: &Built, bars: &[BarId]) {
    let whole = DynamicPlacement::instrument(built.staves[0]);
    let held = built.voices.iter().any(|(_, v)| v.role == Role::Sustain);
    for section in Section::ALL {
        let level = match section {
            Section::A | Section::Coda => DynamicLevel::P,
            Section::B => DynamicLevel::Mf,
            Section::C => DynamicLevel::F,
            Section::D => DynamicLevel::Mp,
        };
        let mark = match section {
            Section::A if spec.soloist => {
                DynamicMark::Other(OtherDynamic::new("dolce", Some(DynamicLevel::Mp)).unwrap())
            }
            Section::D if held => DynamicMark::Fp,
            _ => DynamicMark::Level(level),
        };
        let at = ScorePoint::start_of(bars[section.start() as usize]);
        b.add_dynamic(built.instrument, at, whole, mark).unwrap();
    }
    if spec.family == Family::Strings {
        b.add_dynamic(
            built.instrument,
            ScorePoint::start_of(bars[44]),
            whole,
            DynamicMark::Sfz,
        )
        .unwrap();
    }
    b.add_dynamic(
        built.instrument,
        ScorePoint::start_of(bars[96]),
        whole,
        DynamicMark::Level(DynamicLevel::Pp),
    )
    .unwrap();
    for (voice, v) in &built.voices {
        if v.role == Role::PianoInner {
            let placement = DynamicPlacement::voice(*voice, built.staves[v.staff]);
            for local in [1, 42] {
                b.add_dynamic(
                    built.instrument,
                    ScorePoint::start_of(bars[local]),
                    placement,
                    DynamicMark::Level(DynamicLevel::Pp),
                )
                .unwrap();
            }
        }
    }
}

/// The hairpins: in every phrase of [`PHRASES`] a crescendo from its bar 2 to its bar 4 and a
/// diminuendo from bar 4 to bar 6 (touching ends are allowed: the end is exclusive, ADR-0024 §8);
/// the crescendo of C's first phrase is written as text ("cresc."); the soloist ends with a
/// diminuendo to niente over the last three bars; the piano's inner voice has one hairpin of its
/// own in the first phrase (a voice hairpin may overlap the instrument's).
fn add_hairpins(b: &mut DocumentBuilder, spec: &InstrumentSpec, built: &Built, bars: &[BarId]) {
    let whole = DynamicPlacement::instrument(built.staves[0]);
    let point = |local: u32| ScorePoint::start_of(bars[local as usize]);
    for start in PHRASES {
        let crescendo = if start == 42 {
            HairpinShape {
                kind: HairpinKind::Crescendo,
                style: HairpinStyle::Text,
                niente: false,
            }
        } else {
            HairpinShape::CRESCENDO
        };
        b.add_hairpin(
            built.instrument,
            point(start + 2),
            point(start + 4),
            whole,
            crescendo,
        )
        .unwrap();
        b.add_hairpin(
            built.instrument,
            point(start + 4),
            point(start + 6),
            whole,
            HairpinShape::DIMINUENDO,
        )
        .unwrap();
    }
    if spec.soloist {
        b.add_hairpin(
            built.instrument,
            point(96),
            point(99),
            whole,
            HairpinShape {
                kind: HairpinKind::Diminuendo,
                style: HairpinStyle::Wedge,
                niente: true,
            },
        )
        .unwrap();
    }
    for (voice, v) in &built.voices {
        if v.role == Role::PianoInner {
            b.add_hairpin(
                built.instrument,
                point(1),
                point(3),
                DynamicPlacement::voice(*voice, built.staves[v.staff]),
                HairpinShape::CRESCENDO,
            )
            .unwrap();
        }
    }
}
