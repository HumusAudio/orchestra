// SPDX-License-Identifier: GPL-3.0-or-later

//! Hand-written cases for the tempo marks and the tempo map derived from them (ADR-0024 §11,
//! SCALETTA F1.09c, second part; owner's choices E1–E3 and the clamping of 2026-11-03). Each case
//! compares the derived changes with a list computed by hand in the comments.
//!
//! Every builder here is in 4/4 (a bar lasts one whole note) at ♩ = 60, unless it says otherwise.

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test file only.
#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    BarId, DocumentBuilder, DocumentError, GradualMark, Key, Metronome, Mode, ScorePoint,
    TempoMark, TempoMarkChange, Violation,
};
use orchestra_core_time::{
    BarIndex, Duration, GradualChange, Meter, MeterChange, MeterMap, Position, Rational, Tempo,
    TempoChange,
};

/// The rational `n/d`.
fn q(n: i64, d: i64) -> Rational {
    Rational::new(n, d).unwrap()
}

/// The duration `n/d` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(q(n, den))
}

/// The position `n/d` whole notes.
fn pos(n: i64, den: i64) -> Position {
    Position::from_rational(q(n, den))
}

/// ♩ = `bpm`.
fn quarter(bpm: i64) -> Tempo {
    Tempo::new(d(1, 4), q(bpm, 1)).unwrap()
}

/// The point `n/den` after the start of `bar`.
fn at(bar: BarId, n: i64, den: i64) -> ScorePoint {
    ScorePoint::new(bar, d(n, den))
}

/// A mark that sets ♩ = `bpm`, printed, without text.
fn set(bpm: i64) -> TempoMark {
    TempoMark::immediate(None, Metronome::shown(quarter(bpm))).unwrap()
}

/// A gradual mark to ♩ = `bpm`, ending at `end`, with text.
fn towards(text: &str, bpm: i64, end: ScorePoint) -> TempoMark {
    TempoMark::new(
        Some(text),
        Some(TempoMarkChange::Gradual(GradualMark {
            from: None,
            to: quarter(bpm),
            end,
        })),
    )
    .unwrap()
}

/// A builder on `meters` at ♩ = 60 with `bars` bars.
fn builder_with(meters: MeterMap, bars: usize) -> (DocumentBuilder, Vec<BarId>) {
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(Some("Andante"), Metronome::shown(quarter(60))).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let ids = (0..bars).map(|_| b.add_bar().unwrap()).collect();
    (b, ids)
}

/// A builder in 4/4 at ♩ = 60 with `bars` bars.
fn builder(bars: usize) -> (DocumentBuilder, Vec<BarId>) {
    builder_with(
        MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap(),
        bars,
    )
}

/// An immediate change in the tempo map.
fn now(bpm: i64) -> TempoChange {
    TempoChange::Immediate(quarter(bpm))
}

/// The changes of the derived tempo map.
fn changes(b: &DocumentBuilder) -> Vec<(Position, TempoChange)> {
    b.document().tempo_map().changes().to_vec()
}

/// E1: the initial mark must set a tempo at once, and takes the document's first ID.
#[test]
fn the_initial_mark_sets_a_tempo() {
    let meters = || MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let key = Key::traditional(0, Mode::Major).unwrap();
    let text_only = TempoMark::new(Some("Allegro"), None).unwrap();
    assert_eq!(
        DocumentBuilder::new(meters(), text_only, key).err(),
        Some(DocumentError::InitialTempoNotImmediate)
    );
    let (b, bars) = builder(1);
    let gradual = towards("accel.", 90, ScorePoint::start_of(bars[0]));
    assert_eq!(
        DocumentBuilder::new(meters(), gradual, key).err(),
        Some(DocumentError::InitialTempoNotImmediate)
    );
    let (id, mark) = b.document().initial_tempo();
    assert_eq!(id.get(), 1);
    assert_eq!(mark.text(), Some("Andante"));
    assert_eq!(changes(&b), [(Position::ZERO, now(60))]);
}

/// A mark needs a text or a change, and a text is never empty.
#[test]
fn empty_marks_are_refused() {
    assert_eq!(
        TempoMark::new(None, None),
        Err(DocumentError::EmptyTempoMark)
    );
    assert_eq!(
        TempoMark::new(Some(" \t"), None),
        Err(DocumentError::EmptyText)
    );
    assert_eq!(
        TempoMark::immediate(Some(""), Metronome::hidden(quarter(60))),
        Err(DocumentError::EmptyText)
    );
}

/// An immediate mark becomes a change at its bar's start plus its offset; a text-only mark (E2)
/// and a hidden metronome are both kept as written, and only the latter changes the map.
#[test]
fn marks_become_changes_of_the_map() {
    let (mut b, bars) = builder(4);
    let dolce = TempoMark::new(Some("dolce"), None).unwrap();
    b.add_tempo_mark(at(bars[1], 1, 4), dolce).unwrap();
    // Bar 2 starts at 2 whole notes; its third beat at 2 + 1/2.
    b.add_tempo_mark(at(bars[2], 1, 2), set(120)).unwrap();
    let automation = TempoMark::immediate(None, Metronome::hidden(quarter(100))).unwrap();
    b.add_tempo_mark(ScorePoint::start_of(bars[3]), automation)
        .unwrap();
    assert_eq!(
        changes(&b),
        [
            (Position::ZERO, now(60)),
            (pos(5, 2), now(120)),
            (pos(3, 1), now(100)),
        ]
    );
    let document = b.build().unwrap();
    let marks = document.tempo_marks().as_slice();
    assert_eq!(marks.len(), 3);
    assert_eq!(marks[0].value().text(), Some("dolce"));
    assert_eq!(marks[0].value().change(), None);
    assert_eq!(
        marks[2].value().change(),
        Some(TempoMarkChange::Immediate(Metronome::hidden(quarter(100))))
    );
}

/// E3: a gradual change ends at an explicit point; "a tempo" exactly at its end is allowed, and
/// so is a text-only mark inside it ("poco a poco").
#[test]
fn a_gradual_change_and_a_tempo() {
    let (mut b, bars) = builder(3);
    b.add_tempo_mark(
        ScorePoint::start_of(bars[1]),
        towards("rit.", 40, ScorePoint::start_of(bars[2])),
    )
    .unwrap();
    b.add_tempo_mark(
        at(bars[1], 1, 2),
        TempoMark::new(Some("poco a poco"), None).unwrap(),
    )
    .unwrap();
    b.add_tempo_mark(ScorePoint::start_of(bars[2]), set(60))
        .unwrap();
    assert_eq!(
        changes(&b),
        [
            (Position::ZERO, now(60)),
            (
                pos(1, 1),
                TempoChange::Gradual(GradualChange {
                    from: None,
                    to: quarter(40),
                    end: pos(2, 1),
                })
            ),
            (pos(2, 1), now(60)),
        ]
    );
    assert!(b.build().is_ok());
}

/// A gradual change with a starting tempo ("Meno mosso ♩ = 50, poco a poco accel.").
#[test]
fn a_gradual_change_with_a_starting_tempo() {
    let (mut b, bars) = builder(2);
    let meno_mosso = TempoMark::new(
        Some("Meno mosso, poco a poco accel."),
        Some(TempoMarkChange::Gradual(GradualMark {
            from: Some(Metronome::shown(quarter(50))),
            to: quarter(72),
            end: ScorePoint::start_of(bars[1]),
        })),
    )
    .unwrap();
    b.add_tempo_mark(at(bars[0], 1, 2), meno_mosso).unwrap();
    assert_eq!(
        changes(&b)[1],
        (
            pos(1, 2),
            TempoChange::Gradual(GradualChange {
                from: Some(quarter(50)),
                to: quarter(72),
                end: pos(1, 1),
            })
        )
    );
}

/// A tempo change strictly inside a gradual one is refused, all or nothing: the builder is
/// unchanged and no ID is consumed.
#[test]
fn a_change_inside_a_gradual_one_is_refused() {
    let (mut b, bars) = builder(3);
    let rit = b
        .add_tempo_mark(
            ScorePoint::start_of(bars[1]),
            towards("rit.", 40, ScorePoint::start_of(bars[2])),
        )
        .unwrap();
    let before = b.document().clone();
    let inside = b.add_tempo_mark(at(bars[1], 1, 2), set(90));
    assert!(
        matches!(
            inside,
            Err(DocumentError::Invalid(Violation::TempoChangeInsideGradual(
                _
            )))
        ),
        "{inside:?}"
    );
    assert_eq!(b.document(), &before);
    // A gradual change that would contain an existing change is refused too.
    let (mut c, bars_c) = builder(3);
    c.add_tempo_mark(at(bars_c[1], 1, 2), set(90)).unwrap();
    let containing = c.add_tempo_mark(
        ScorePoint::start_of(bars_c[1]),
        towards("rit.", 40, ScorePoint::start_of(bars_c[2])),
    );
    assert!(
        matches!(
            containing,
            Err(DocumentError::Invalid(Violation::TempoChangeInsideGradual(
                _
            )))
        ),
        "{containing:?}"
    );
    // No ID was consumed by the refusal in `b`: the next ID follows the gradual mark's.
    assert_eq!(b.add_bar().unwrap().get(), rit.get() + 1);
}

/// A gradual change must end strictly after it starts, in score order.
#[test]
fn a_gradual_change_goes_forwards() {
    let (mut b, bars) = builder(2);
    for end in [at(bars[1], 1, 4), ScorePoint::start_of(bars[1])] {
        let result = b.add_tempo_mark(at(bars[1], 1, 4), towards("accel.", 90, end));
        assert!(
            matches!(
                result,
                Err(DocumentError::Invalid(Violation::GradualTempoNotForward(_)))
            ),
            "{result:?}"
        );
    }
    assert!(b.document().tempo_marks().is_empty());
}

/// The points are checked like those of every other mark; the very start belongs to the initial
/// mark, and two marks cannot share a point.
#[test]
fn points_are_checked() {
    let (mut b, bars) = builder(2);
    let (_, foreign) = builder(9);
    let before = b.document().clone();
    assert_eq!(
        b.add_tempo_mark(ScorePoint::start_of(bars[0]), set(90)),
        Err(DocumentError::DuplicateMark)
    );
    assert_eq!(
        b.add_tempo_mark(at(bars[0], -1, 4), set(90)),
        Err(DocumentError::NegativeOffset)
    );
    assert_eq!(
        b.add_tempo_mark(ScorePoint::start_of(foreign[8]), set(90)),
        Err(DocumentError::UnknownBar(foreign[8]))
    );
    assert_eq!(
        b.add_tempo_mark(
            ScorePoint::start_of(bars[1]),
            towards("rit.", 40, ScorePoint::start_of(foreign[8])),
        ),
        Err(DocumentError::UnknownBar(foreign[8]))
    );
    assert_eq!(b.document(), &before);
    b.add_tempo_mark(ScorePoint::start_of(bars[1]), set(90))
        .unwrap();
    assert_eq!(
        b.add_tempo_mark(ScorePoint::start_of(bars[1]), set(100)),
        Err(DocumentError::DuplicateMark)
    );
}

/// Positions follow the meter map: in 3/4 then 2/4 from bar 1, bar 2 starts at 3/4 + 2/4 = 5/4,
/// and a mark a quarter into it is at 3/2.
#[test]
fn positions_follow_the_meters() {
    let mut meters = MeterMap::new(MeterChange::new(Meter::new(&[3], 4).unwrap())).unwrap();
    meters
        .set_meter_change(
            BarIndex::new(1),
            MeterChange::new(Meter::new(&[2], 4).unwrap()),
        )
        .unwrap();
    let (mut b, bars) = builder_with(meters, 3);
    b.add_tempo_mark(at(bars[2], 1, 4), set(80)).unwrap();
    assert_eq!(changes(&b)[1], (pos(3, 2), now(80)));
}

/// An offset past the end of an overfull bar counts as the barline (owner's decision of
/// 2026-11-03); when two marks land on the same position, the later one in score order wins.
#[test]
fn overfull_bars_clamp_to_the_barline() {
    let (mut b, bars) = builder(2);
    // At 5/4 of a 4/4 bar: played at the barline, position 1.
    b.add_tempo_mark(at(bars[0], 5, 4), set(90)).unwrap();
    assert_eq!(
        changes(&b),
        [(Position::ZERO, now(60)), (pos(1, 1), now(90))]
    );
    // At 1 (exactly the barline) comes before 5/4 in score order: the 5/4 mark still wins.
    b.add_tempo_mark(at(bars[0], 1, 1), set(70)).unwrap();
    assert_eq!(
        changes(&b),
        [(Position::ZERO, now(60)), (pos(1, 1), now(90))]
    );
    // At the start of the next bar: later in score order than both, so it wins.
    b.add_tempo_mark(ScorePoint::start_of(bars[1]), set(120))
        .unwrap();
    assert_eq!(
        changes(&b),
        [(Position::ZERO, now(60)), (pos(1, 1), now(120))]
    );
    // All three stay written in the score.
    assert_eq!(b.document().tempo_marks().len(), 3);
    assert!(b.build().is_ok());
}

/// A gradual change whose end is clamped onto its start shrinks to nothing and acts as an
/// immediate change to its target.
#[test]
fn a_gradual_change_shrunk_by_the_clamping_is_immediate() {
    let (mut b, bars) = builder(1);
    // From the barline (offset 1 of a 4/4 bar) to 3/2: both clamp to position 1.
    b.add_tempo_mark(at(bars[0], 1, 1), towards("accel.", 90, at(bars[0], 3, 2)))
        .unwrap();
    assert_eq!(
        changes(&b),
        [(Position::ZERO, now(60)), (pos(1, 1), now(90))]
    );
}
