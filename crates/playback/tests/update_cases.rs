// SPDX-License-Identifier: GPL-3.0-or-later

//! The incremental update of the played notes on the cases that stretch its window (ADR-0027 §9 and
//! §14, H11 and H12; SCALETTA F1.13c part 2): each edits a small score with the real editor, updates
//! the path and the performance, and compares the result with a performance computed from scratch.
//! The random property test (`update_props`) rarely builds these on its own.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    Alteration, BarId, BaseValue, Clef, Command, Document, DocumentBuilder, DynamicLevel,
    DynamicMark, DynamicPlacement, Editor, EventChange, Grace, GraceAttach, GraceKind,
    InstrumentId, Interval, Key, MarkSpec, Metronome, Mode, NewBody, NewEvent, NoteId, NoteValue,
    Pitch, Placement, ScorePoint, StaffId, Step, TempoMark, Tie, VoiceId,
};
use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Rational, Tempo};
use orchestra_playback::{Performance, Unrolled};

/// `n/den` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// C4.
fn c4() -> Pitch {
    Pitch::new(Step::C, Alteration::NATURAL, 4)
}

/// A score of `n` bars of `num/4`, one instrument with one voice.
struct Score {
    /// The builder.
    b: DocumentBuilder,
    /// The bars.
    bars: Vec<BarId>,
    /// The voice.
    voice: VoiceId,
    /// The staff.
    staff: StaffId,
    /// The instrument.
    instrument: InstrumentId,
}

/// `n` bars of `num/4`.
fn score(n: usize, num: u8) -> Score {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[num], 4).unwrap())).unwrap();
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(120).unwrap()).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars = (0..n).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Flute", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let i = b.document().track(track).unwrap().instrument();
    let (voice, staff, instrument) = (i.voices()[0].id(), i.staves()[0].id(), i.id());
    Score {
        b,
        bars,
        voice,
        staff,
        instrument,
    }
}

impl Score {
    /// A quarter C4 at (bar, offset); returns its notehead.
    fn quarter(&mut self, bar: usize, offset: Duration) -> NoteId {
        self.b
            .add_chord(
                Placement::new(self.voice, self.bars[bar], offset),
                NoteValue::plain(BaseValue::Quarter),
                &[c4()],
            )
            .unwrap()
            .notes[0]
    }

    /// `count` acciaccaturas before (bar, offset).
    fn acciaccaturas(&mut self, bar: usize, offset: Duration, count: usize) {
        for _ in 0..count {
            self.b
                .add_grace_chord(
                    Placement::new(self.voice, self.bars[bar], offset),
                    Grace {
                        kind: GraceKind::Acciaccatura,
                        attach: GraceAttach::Before,
                    },
                    NoteValue::plain(BaseValue::Sixteenth),
                    &[Pitch::new(Step::D, Alteration::NATURAL, 4)],
                )
                .unwrap();
        }
    }
}

/// Runs `command` on an editor of `doc`, updates the path and the performance, and checks the
/// result against a performance computed from scratch; then the same for the undo and the redo.
fn check(doc: Document, command: &Command) {
    let mut editor = Editor::new(doc);
    let mut path = Unrolled::new(editor.document()).unwrap();
    let mut performance = Performance::new(editor.document(), &path).unwrap();
    let before = performance.clone();
    let steps = [
        editor.execute(command).unwrap(),
        editor.undo().unwrap().unwrap(),
        editor.redo().unwrap().unwrap(),
    ];
    for (k, changes) in steps.iter().enumerate() {
        let doc = editor.document();
        path.update(doc, changes).unwrap();
        assert!(performance.update(doc, &path, changes).unwrap());
        assert_eq!(
            performance,
            Performance::new(doc, &path).unwrap(),
            "step {k}"
        );
    }
    // The edit changed something, so the comparison above was not trivially between equals.
    assert_ne!(performance, before);
}

/// A tie removed at the start of a chain of three bars: the rest of the chain becomes a shorter
/// note, so the ten acciaccaturas of the bar after it, bounded by half of the note they take their
/// time from, get shorter too. The window must reach the bar of those graces, three bars after the
/// change (H12: a note starting in the window reaches at most to the bar before its end).
#[test]
fn a_tie_removed_shortens_the_graces_after_the_chain() {
    let mut s = score(5, 1);
    let first = s.quarter(0, Duration::ZERO);
    let second = s.quarter(1, Duration::ZERO);
    let third = s.quarter(2, Duration::ZERO);
    s.b.set_tie(first, Tie::To(second)).unwrap();
    s.b.set_tie(second, Tie::To(third)).unwrap();
    s.acciaccaturas(3, Duration::ZERO, 10);
    s.quarter(3, Duration::ZERO);
    let doc = s.b.build().unwrap();
    let event = doc.note(first).unwrap().0.event().id();
    check(
        doc,
        &Command::ChangeEvent {
            event,
            change: EventChange::Tie {
                note: first,
                tie: Tie::None,
            },
        },
    );
}

/// A tie added at the end of a chain joins the next note to it: the thirteen acciaccaturas of the
/// bar after it were bounded by half of that short note (1/8) and no longer are (half of the chain
/// is 1/2). The window grows on the new notes, to the bar of those graces (H12).
#[test]
fn a_tie_added_lengthens_the_chain() {
    let mut s = score(6, 1);
    let mut notes = Vec::new();
    for bar in 0..5 {
        if bar == 4 {
            s.acciaccaturas(4, Duration::ZERO, 13);
        }
        notes.push(s.quarter(bar, Duration::ZERO));
    }
    s.b.set_tie(notes[0], Tie::To(notes[1])).unwrap();
    s.b.set_tie(notes[1], Tie::To(notes[2])).unwrap();
    let doc = s.b.build().unwrap();
    let event = doc.note(notes[2]).unwrap().0.event().id();
    check(
        doc,
        &Command::ChangeEvent {
            event,
            change: EventChange::Tie {
                note: notes[2],
                tie: Tie::To(notes[3]),
            },
        },
    );
}

/// The window keeps the graces of its first bar, bounded by half of the note before them, which
/// is outside the window: that note must be computed too, as context (the margin of H12).
#[test]
fn the_graces_of_the_first_kept_bar_see_the_note_before() {
    let mut s = score(4, 1);
    s.quarter(0, Duration::ZERO);
    s.acciaccaturas(1, Duration::ZERO, 5);
    s.quarter(1, Duration::ZERO);
    let changed = s.quarter(2, Duration::ZERO);
    s.quarter(3, Duration::ZERO);
    let doc = s.b.build().unwrap();
    let event = doc.note(changed).unwrap().0.event().id();
    check(
        doc,
        &Command::ChangeEvent {
            event,
            change: EventChange::Articulations(
                orchestra_core_model::ArticulationSet::EMPTY
                    .with(orchestra_core_model::Articulation::Staccato),
            ),
        },
    );
}

/// Acciaccaturas entered at the start of a bar take their time from the chain that ends there,
/// which started two bars before: the window starts with that chain (H12).
#[test]
fn an_acciaccatura_entered_shortens_a_chain_from_before() {
    let mut s = score(4, 2);
    let first = s.quarter(0, d(1, 4));
    let second = s.quarter(1, Duration::ZERO);
    let third = s.quarter(1, d(1, 4));
    s.b.set_tie(first, Tie::To(second)).unwrap();
    s.b.set_tie(second, Tie::To(third)).unwrap();
    s.quarter(2, Duration::ZERO);
    let voice = s.voice;
    let bar = s.bars[2];
    check(
        s.b.build().unwrap(),
        &Command::EnterEvent(NewEvent {
            at: Placement::new(voice, bar, Duration::ZERO),
            body: NewBody::Chord {
                value: NoteValue::plain(BaseValue::Sixteenth),
                pitches: vec![c4()],
                grace: Some(Grace {
                    kind: GraceKind::Acciaccatura,
                    attach: GraceAttach::Before,
                }),
            },
        }),
    );
}

/// A dynamic added recomputes only the velocities of its instrument (H11), with the notes of a
/// repeat played after it at its level.
#[test]
fn a_dynamic_added_changes_the_velocities() {
    let mut s = score(3, 4);
    for bar in 0..3 {
        for k in 0..4 {
            s.quarter(bar, d(k, 4));
        }
    }
    s.b.add_repeat_end(s.bars[2], 2).unwrap();
    let (instrument, staff, bar) = (s.instrument, s.staff, s.bars[1]);
    check(
        s.b.build().unwrap(),
        &Command::AddMark(MarkSpec::Dynamic {
            instrument,
            at: ScorePoint::new(bar, d(1, 2)),
            placement: DynamicPlacement::instrument(staff),
            mark: DynamicMark::Level(DynamicLevel::Pp),
        }),
    );
    // And `sfz` at a note: an accent, a profile.
    let mut s = score(1, 4);
    s.quarter(0, Duration::ZERO);
    let (instrument, staff, bar) = (s.instrument, s.staff, s.bars[0]);
    check(
        s.b.build().unwrap(),
        &Command::AddMark(MarkSpec::Dynamic {
            instrument,
            at: ScorePoint::new(bar, Duration::ZERO),
            placement: DynamicPlacement::instrument(staff),
            mark: DynamicMark::Sfz,
        }),
    );
}

/// A tempo change recomputes nothing (ADR-0027 §1, H14: musical time, profiles in seconds).
#[test]
fn a_tempo_change_recomputes_nothing() {
    let mut s = score(2, 4);
    s.quarter(0, Duration::ZERO);
    let mut editor = Editor::new(s.b.build().unwrap());
    let mut path = Unrolled::new(editor.document()).unwrap();
    let mut performance = Performance::new(editor.document(), &path).unwrap();
    let faster = Tempo::new(d(1, 4), Rational::from_integer(90).unwrap()).unwrap();
    let changes = editor
        .execute(&Command::SetInitial(orchestra_core_model::Initial::Tempo(
            TempoMark::immediate(None, Metronome::shown(faster)).unwrap(),
        )))
        .unwrap();
    let doc = editor.document();
    path.update(doc, &changes).unwrap();
    assert!(!performance.update(doc, &path, &changes).unwrap());
    assert_eq!(performance, Performance::new(doc, &path).unwrap());
}
