// SPDX-License-Identifier: GPL-3.0-or-later

//! The full preset (ADR-0031 §6, SCALETTA F1.16b): it is valid, it unrolls to the length the
//! specification states, it plays, it survives a save and a load, and it holds every kind of
//! object of the "everything" document of the golden files except the three exceptions the owner
//! approved on 2026-11-07.
//!
//! Validity needs no test of its own: `full` ends with `DocumentBuilder::build`, which runs
//! `Document::validate` and would panic on any broken rule (ADR-0031 §5).

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{Articulation, Document, EventBody};
use orchestra_core_time::BarIndex;
use orchestra_playback::{Performance, Unrolled};
use orchestra_test_scores::{Size, full};

/// Every articulation of the model: each must appear somewhere in the normal score.
const ARTICULATIONS: [Articulation; 10] = [
    Articulation::Accent,
    Articulation::Marcato,
    Articulation::Staccato,
    Articulation::Staccatissimo,
    Articulation::Spiccato,
    Articulation::Tenuto,
    Articulation::Portato,
    Articulation::Stress,
    Articulation::Unstress,
    Articulation::SoftAccent,
];

/// What the normal score must contain, as fragments of the `Debug` form of its saved description
/// (`Document::to_parts`), with what each one stands for.
///
/// # Why the `Debug` form?
/// The description lists every object of the document with its variants spelled out, so one
/// search per kind of object is the shortest check that cannot miss a field; going through the
/// typed accessors would need a walk per kind. The fragments are specific (`attach: After`, not
/// `After`) so that they cannot match by accident. Articulations are the exception: their set is
/// a bit mask in `Debug`, so they are checked through the API below.
///
/// # Where the list comes from
/// Every builder call of `crates/io/tests/common/everything.rs` (the document with every kind of
/// saved object), minus the owner's three exceptions (the lettered numbering style, a numbering
/// restart, a last bar shorter than its meter), plus the free bar, which that document lacks.
const MUST_CONTAIN: [(&str, &str); 52] = [
    // The meter map.
    ("kind: AlignEnd", "a pickup bar"),
    ("kind: Free", "a free (cadenza) bar"),
    ("uncounted: [BarIndex", "an uncounted bar"),
    ("grouping: Some", "a meter change with its beat grouping"),
    ("bar_groupings: [(", "a beat grouping of one bar (hemiola)"),
    // Tempo marks.
    ("shown: true", "a shown metronome mark"),
    ("shown: false", "a hidden metronome mark"),
    (
        "Gradual(GradualMark { from: Some",
        "a gradual change with its starting tempo",
    ),
    (
        "Gradual(GradualMark { from: None",
        "a gradual change without starting tempo",
    ),
    ("change: None", "a tempo mark of text only"),
    // Keys and instruments.
    ("key_changes: [Mark", "a key change"),
    ("NoSignature", "a key override (no key signature)"),
    (
        "transposition: Interval { steps: -",
        "a transposing instrument",
    ),
    ("clef_changes: [Mark", "a clef change"),
    // Events.
    ("parent: Some", "a nested tuplet"),
    ("tuplet: Some", "a tuplet member"),
    ("WholeBar", "a whole-bar rest"),
    ("Rest(Rest { value: Value", "a rest of a note value"),
    ("upper: [Note", "a chord of several notes"),
    ("attach: Before", "a grace note before"),
    ("attach: After", "a grace note after"),
    ("kind: Acciaccatura", "an acciaccatura"),
    ("kind: Appoggiatura", "an appoggiatura"),
    ("Alteration(Rational(-1/2))", "a microtone"),
    // Note and chord markings.
    ("tie: To(", "a tie to a note"),
    ("tie: LetRing", "a let-ring tie"),
    ("Trill { auxiliary: None }", "a trill"),
    ("Trill { auxiliary: Some", "a trill with an auxiliary"),
    ("ornament: Some(Turn)", "a turn"),
    ("ornament: Some(InvertedTurn)", "an inverted turn"),
    ("ornament: Some(Mordent)", "a mordent"),
    ("ornament: Some(UpperMordent)", "an upper mordent"),
    ("ornament: Some(Tremolo", "a tremolo"),
    ("arpeggio: Some", "an arpeggio"),
    ("fermata: Some", "a fermata"),
    (
        "Rest(Rest { value: Value(NoteValue { base: Quarter, dots: 0 }), tuplet: None, fermata: Some",
        "a fermata on a rest",
    ),
    ("velocity: -", "an inline performance offset"),
    // Dynamics and hairpins.
    ("mark: Level(", "a dynamic level"),
    ("mark: Sfz", "an accent dynamic"),
    ("mark: Fp", "fp"),
    ("mark: Other(", "a dynamic of free text with a level"),
    ("scope: Voice(", "a dynamic or hairpin of one voice"),
    ("style: Text", "a hairpin written as text"),
    ("niente: true", "a hairpin to niente"),
    // Slurs and form.
    ("slurs: [Slur", "a slur"),
    ("repeat_starts: [", "a start repeat"),
    ("repeat_ends: [", "an end repeat"),
    ("voltas: [", "voltas"),
    ("kind: Segno", "markers (segno, To Coda, coda)"),
    ("jumps: [Jump", "a jump (D.S. al Coda)"),
    ("bar_repeats: [BarRepeat", "a bar repeat"),
    ("pass_rules: [PassRule", "a pass rule"),
];

/// The two settings of the DAW move (ADR-0028) the everything document has, checked apart because
/// they are not lists of marks.
const MUST_CONTAIN_SETTINGS: [(&str, &str); 2] = [
    ("pass_offsets: [PassOffset", "a pass-specific offset"),
    ("track_tolerances: [(TrackId", "a per-track tolerance"),
];

/// The normal score holds every kind of object of the everything document (see [`MUST_CONTAIN`]).
#[test]
fn the_normal_full_score_holds_every_kind_of_object() {
    let doc = full(Size::Normal);
    let text = format!("{:?}", doc.to_parts());
    let missing: Vec<&str> = MUST_CONTAIN
        .iter()
        .chain(&MUST_CONTAIN_SETTINGS)
        .filter(|(fragment, _)| !text.contains(fragment))
        .map(|(_, what)| *what)
        .collect();
    assert!(missing.is_empty(), "the full score lacks: {missing:?}");
    let mut seen = Vec::new();
    for_each_event(&doc, |body| {
        if let EventBody::Chord(chord) = body {
            seen.extend(chord.articulations().iter());
        }
    });
    for articulation in ARTICULATIONS {
        assert!(
            seen.contains(&articulation),
            "the full score has no {articulation:?}"
        );
    }
}

/// Calls `f` with the body of every event of the document, voice by voice, bar by bar.
fn for_each_event(doc: &Document, mut f: impl FnMut(&EventBody)) {
    let bars = u32::try_from(doc.bars().len()).unwrap();
    for voice in doc.tracks().iter().flat_map(|t| t.instrument().voices()) {
        for bar in 0..bars {
            for placed in voice.events_at(BarIndex::new(bar)) {
                f(placed.event().body());
            }
        }
    }
}

/// Checks one size: the bar and staff counts, the unrolled length (133 bars per movement, as the
/// plan of `src/full/plan.rs` computes), a performance that can be computed, and a save/load round
/// trip that gives back the same document, IDs included.
fn check(size: Size, bars: usize, staves: usize, played: usize) {
    let doc = full(size);
    assert_eq!(doc.bars().len(), bars);
    let staff_count: usize = doc
        .tracks()
        .iter()
        .map(|t| t.instrument().staves().len())
        .sum();
    assert_eq!(staff_count, staves);
    let path = Unrolled::new(&doc).unwrap();
    assert_eq!(path.played().len(), played);
    Performance::new(&doc, &path).unwrap();
    let bytes = orchestra_io::to_bytes(&doc).unwrap();
    assert_eq!(orchestra_io::from_bytes(&bytes).unwrap(), doc);
}

/// The normal size: 100 bars, 10 staves, 133 bars played.
#[test]
fn the_normal_full_score_unrolls_plays_and_round_trips() {
    check(Size::Normal, 100, 10, 133);
}

/// The heavy size: 1 000 bars, 30 staves, 1 330 bars played (every movement keeps its repeats
/// after the D.S. of the previous one: see `plan::add_score_marks`).
#[test]
fn the_heavy_full_score_unrolls_plays_and_round_trips() {
    check(Size::Heavy, 1000, 30, 1330);
}
