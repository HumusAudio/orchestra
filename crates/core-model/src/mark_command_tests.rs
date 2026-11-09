// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the commands on marks, form markings, tuplets, initial values and per-bar data
//! (ADR-0026 §9, §11 X1–X10; SCALETTA F1.11b, part 2b).
//!
//! # Why inside the crate
//! They start from the rich document of `crate::editor_tests` (a slur, two dynamics, a hairpin,
//! two tempo marks with a gradual change, a clef change, a key change and a key override, a
//! repeated section with a pass rule, a D.S. al Fine, a bar repeat), whose helpers are
//! crate-private.
//!
//! # What is checked
//! One hand-written case per decision ADR-0026 X1–X10, each through `execute_checked` (the
//! document is valid and consistent after the command, undo gives back the document from before and
//! redo the one after) or `refused` (the refusal changed nothing and recorded no step). The
//! property test of `crate::command_tests` generates these commands too.

use orchestra_core_time::{BarIndex, BeatGrouping, Duration, Rational, Tempo};

use crate::bar_repeat::BarRepeatSpan;
use crate::change::ChangeSet;
use crate::command::{Command, NewBody, NewEvent};
use crate::document::{Document, DocumentError, Placement};
use crate::dynamic::{DynamicLevel, DynamicMark, DynamicPlacement, HairpinShape};
use crate::edit::{Initial, ObjectRef};
use crate::editor::{CommandError, Editor};
use crate::editor_tests::{assert_consistent, rich};
use crate::event::{BaseValue, NoteValue, Tuplet};
use crate::form::PassSet;
use crate::id::{BarId, EventId, InstrumentId, StaffId, VoiceId};
use crate::jump::MarkerKind;
use crate::mark_command::{MarkSpec, NewVolta};
use crate::marks::{Clef, ScorePoint};
use crate::pitch::{Alteration, Key, Mode, Pitch, Step};
use crate::slur::SlurPlacement;
use crate::tempo_mark::{GradualMark, Metronome, TempoMark, TempoMarkChange};
use crate::validate::Violation;

/// Shorthand: a duration of `n/d` whole notes.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// Shorthand: a tempo of `bpm` quarters per minute.
fn tempo(bpm: i64) -> Tempo {
    Tempo::new(d(1, 4), Rational::from_integer(bpm).unwrap()).unwrap()
}

/// The IDs of the rich document the tests use.
struct Ids {
    /// The bars, in order.
    bars: Vec<BarId>,
    /// The piano.
    piano: InstrumentId,
    /// The piano's right hand.
    rh: VoiceId,
    /// The piano's lower staff.
    lh_staff: StaffId,
    /// The flute.
    flute: InstrumentId,
    /// The flute's voice.
    fl: VoiceId,
    /// The flute's staff.
    fl_staff: StaffId,
}

/// Reads the IDs of a rich document.
fn ids(doc: &Document) -> Ids {
    let mut instruments = doc.instruments();
    let piano = instruments.next().unwrap();
    let flute = instruments.next().unwrap();
    Ids {
        bars: doc.bars().ids().to_vec(),
        piano: piano.id(),
        rh: piano.voices()[0].id(),
        lh_staff: piano.staves()[1].id(),
        flute: flute.id(),
        fl: flute.voices()[0].id(),
        fl_staff: flute.staves()[0].id(),
    }
}

/// Executes a command, then checks the document is valid and consistent, that undo gives back
/// the document from before and redo the one after. Returns what the command changed.
fn execute_checked(editor: &mut Editor, command: &Command) -> ChangeSet {
    let before = editor.document().clone();
    let changes = editor.execute(command).unwrap();
    let after = editor.document().clone();
    after.validate().unwrap();
    assert_consistent(&after);
    editor.undo().unwrap().unwrap();
    assert!(editor.document().same_content(&before), "undo differs");
    assert_consistent(editor.document());
    editor.redo().unwrap().unwrap();
    assert!(editor.document().same_content(&after), "redo differs");
    changes
}

/// The error of a refused command, after checking it changed nothing and recorded no step.
fn refused(editor: &mut Editor, command: &Command) -> CommandError {
    let before = editor.document().clone();
    let steps = editor.undo_steps();
    let error = editor.execute(command).unwrap_err();
    assert!(
        editor.document().same_content(&before),
        "a refusal changed the document"
    );
    assert_eq!(editor.undo_steps(), steps);
    error
}

/// The one new object of a kind a change set reports as inserted.
fn inserted(changes: &ChangeSet, kind: fn(&ObjectRef) -> bool) -> ObjectRef {
    let found: Vec<ObjectRef> = changes.inserted().iter().copied().filter(kind).collect();
    assert_eq!(found.len(), 1, "{found:?}");
    found[0]
}

/// A dynamic of the whole piano at the start of a bar.
fn piano_dynamic(i: &Ids, bar: usize, level: DynamicLevel) -> MarkSpec {
    MarkSpec::Dynamic {
        instrument: i.piano,
        at: ScorePoint::start_of(i.bars[bar]),
        placement: DynamicPlacement::instrument(i.lh_staff),
        mark: DynamicMark::Level(level),
    }
}

/// A crescendo of the whole piano between the starts of two bars.
fn piano_hairpin(i: &Ids, from: usize, to: usize) -> MarkSpec {
    MarkSpec::Hairpin {
        instrument: i.piano,
        start: ScorePoint::start_of(i.bars[from]),
        end: ScorePoint::start_of(i.bars[to]),
        placement: DynamicPlacement::instrument(i.lh_staff),
        shape: HairpinShape::CRESCENDO,
    }
}

/// The events of the right hand in bar 0 that are not grace notes, in order.
fn main_events(doc: &Document, i: &Ids) -> Vec<EventId> {
    doc.events(i.rh, i.bars[0])
        .unwrap()
        .iter()
        .filter(|p| p.event().grace().is_none())
        .map(|p| p.event().id())
        .collect()
}

// ------------------------------------------------------------------------------------------------
// Dynamics, hairpins, slurs (ADR-0026 X3, ADR-0026 X5)
// ------------------------------------------------------------------------------------------------

/// A dynamic is added, moved keeping its ID, and removed; a second one of the same scope at the
/// same point is refused (ADR-0026 X3).
#[test]
fn dynamics_are_added_moved_and_removed() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let changes = execute_checked(
        &mut editor,
        &Command::AddMark(piano_dynamic(&i, 2, DynamicLevel::F)),
    );
    let id = inserted(&changes, |o| matches!(o, ObjectRef::Dynamic(_)));
    // H11 (ADR-0027 §14): the change set names the instrument whose loudness changed.
    assert_eq!(changes.dynamics_changed(), &[i.piano]);
    // The rich document already has an mf of the whole piano at bar 0.
    assert_eq!(
        refused(
            &mut editor,
            &Command::AddMark(piano_dynamic(&i, 0, DynamicLevel::F))
        ),
        CommandError::Document(DocumentError::DuplicateMark)
    );
    // Moved to bar 3: same ID, now at bar 3, reported as replaced.
    let changes = execute_checked(
        &mut editor,
        &Command::ChangeMark {
            mark: id,
            to: piano_dynamic(&i, 3, DynamicLevel::F),
        },
    );
    assert_eq!(changes.replaced(), &[id]);
    assert_eq!(changes.dynamics_changed(), &[i.piano]);
    let piano = editor.document().instrument(i.piano).unwrap();
    assert!(
        piano
            .dynamics()
            .iter()
            .any(|dy| ObjectRef::Dynamic(dy.id()) == id && dy.at().bar == i.bars[3])
    );
    let changes = execute_checked(&mut editor, &Command::RemoveMark(id));
    assert_eq!(changes.removed(), &[id]);
    // Removed: the dynamic is gone from the document, the change set still says whose it was; and
    // undoing it says so too.
    assert_eq!(changes.dynamics_changed(), &[i.piano]);
    let undone = editor.undo().unwrap().unwrap();
    assert_eq!(undone.dynamics_changed(), &[i.piano]);
}

/// A hairpin overlapping another one of the same scope is refused, one touching it is accepted;
/// stretching it into the other is refused too (ADR-0026 X3).
#[test]
fn hairpins_never_overlap() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    // The rich document has a crescendo of the whole piano over bar 0.
    assert!(matches!(
        refused(&mut editor, &Command::AddMark(piano_hairpin(&i, 0, 2))),
        CommandError::Document(DocumentError::Invalid(Violation::HairpinOverlap(_)))
    ));
    let changes = execute_checked(&mut editor, &Command::AddMark(piano_hairpin(&i, 1, 2)));
    let id = inserted(&changes, |o| matches!(o, ObjectRef::Hairpin(_)));
    let error = refused(
        &mut editor,
        &Command::ChangeMark {
            mark: id,
            to: MarkSpec::Hairpin {
                instrument: i.piano,
                start: ScorePoint::new(i.bars[0], d(1, 2)),
                end: ScorePoint::start_of(i.bars[2]),
                placement: DynamicPlacement::instrument(i.lh_staff),
                shape: HairpinShape::DIMINUENDO,
            },
        },
    );
    assert!(matches!(
        error,
        CommandError::Document(DocumentError::Invalid(Violation::HairpinOverlap(_)))
    ));
}

/// A slur is added between two events of one instrument; one ending on another instrument, one
/// going backwards and a change into another kind of mark are refused.
#[test]
fn slurs_join_events_of_one_instrument_forwards() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let events = main_events(editor.document(), &i);
    let flute_note = editor.document().events(i.fl, i.bars[3]).unwrap()[0]
        .event()
        .id();
    let slur = |start, end| {
        Command::AddMark(MarkSpec::Slur {
            start,
            end,
            placement: SlurPlacement::Above,
        })
    };
    let changes = execute_checked(&mut editor, &slur(events[1], events[2]));
    let id = inserted(&changes, |o| matches!(o, ObjectRef::Slur(_)));
    assert!(matches!(
        refused(&mut editor, &slur(events[0], flute_note)),
        CommandError::Document(DocumentError::Invalid(Violation::SlurToAnotherInstrument(
            _
        )))
    ));
    assert!(matches!(
        refused(&mut editor, &slur(events[2], events[1])),
        CommandError::Document(DocumentError::Invalid(Violation::SlurNotForward(_)))
    ));
    assert_eq!(
        refused(
            &mut editor,
            &Command::ChangeMark {
                mark: id,
                to: piano_dynamic(&i, 2, DynamicLevel::P),
            }
        ),
        CommandError::Document(DocumentError::MarkKindMismatch(id))
    );
    // Events and tuplets are not marks: they have commands of their own.
    let event = ObjectRef::Event(events[0]);
    assert_eq!(
        refused(&mut editor, &Command::RemoveMark(event)),
        CommandError::Document(DocumentError::NotAMark(event))
    );
}

// ------------------------------------------------------------------------------------------------
// Tempo, clefs, keys (ADR-0026 X3, ADR-0026 X6, ADR-0026 X8)
// ------------------------------------------------------------------------------------------------

/// A tempo mark inside the accelerando (bars 5–6) is refused; one at bar 7 is accepted and the
/// change set says where the tempo map changes; the initial tempo can be set, but only to an
/// immediate tempo (ADR-0026 X8).
#[test]
fn tempo_marks_respect_gradual_changes() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let mark = |bar, offset, bpm| {
        Command::AddMark(MarkSpec::Tempo {
            at: ScorePoint::new(i.bars[bar], offset),
            mark: TempoMark::immediate(None, Metronome::shown(tempo(bpm))).unwrap(),
        })
    };
    assert!(matches!(
        refused(&mut editor, &mark(5, d(1, 4), 100)),
        CommandError::Document(DocumentError::Invalid(Violation::TempoChangeInsideGradual(
            _
        )))
    ));
    let changes = execute_checked(&mut editor, &mark(7, Duration::ZERO, 60));
    assert!(changes.tempo_from().is_some());
    // The initial tempo is set, never removed (ADR-0026 X8), and must be immediate.
    let gradual = TempoMark::new(
        None,
        Some(TempoMarkChange::Gradual(GradualMark {
            from: None,
            to: tempo(60),
            end: ScorePoint::start_of(i.bars[1]),
        })),
    )
    .unwrap();
    assert_eq!(
        refused(&mut editor, &Command::SetInitial(Initial::Tempo(gradual))),
        CommandError::Document(DocumentError::InitialTempoNotImmediate)
    );
    let fast = TempoMark::immediate(Some("Presto"), Metronome::shown(tempo(180))).unwrap();
    execute_checked(&mut editor, &Command::SetInitial(Initial::Tempo(fast)));
    assert_eq!(editor.document().initial_tempo().1.text(), Some("Presto"));
    assert_eq!(
        refused(&mut editor, &Command::RemoveMark(ObjectRef::InitialTempo)),
        CommandError::Document(DocumentError::NotAMark(ObjectRef::InitialTempo))
    );
}

/// Inside a larger transaction a refused tempo mark is reverted alone, by the local check, and
/// the transaction goes on: without the check the rule would only fail when the transaction
/// derives the tempo map at its end, taking the good commands down with it (ADR-0026 X10).
#[test]
fn a_refused_tempo_mark_leaves_the_transaction_open() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let inside = Command::AddMark(MarkSpec::Tempo {
        at: ScorePoint::new(i.bars[5], d(1, 4)),
        mark: TempoMark::immediate(None, Metronome::shown(tempo(100))).unwrap(),
    });
    let good = Command::AddMark(piano_dynamic(&i, 6, DynamicLevel::Ff));
    let changes = editor
        .transact(|tx| {
            let refusal = tx.execute(&inside).unwrap_err();
            assert!(matches!(
                refusal,
                CommandError::Document(DocumentError::Invalid(
                    Violation::TempoChangeInsideGradual(_)
                ))
            ));
            tx.execute(&good)
        })
        .unwrap();
    assert_eq!(changes.inserted().len(), 1);
    assert!(changes.tempo_from().is_none());
    editor.document().validate().unwrap();
}

/// A new key or clef changes no pitch (ADR-0026 X6): every event of the score is the same before
/// and after.
#[test]
fn keys_and_clefs_change_no_pitch() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let events = |doc: &Document| -> Vec<_> {
        doc.instruments()
            .flat_map(|ins| ins.voices().iter())
            .flat_map(|v| (0..8).flat_map(move |b| v.events_at(BarIndex::new(b)).to_vec()))
            .collect()
    };
    let before = events(editor.document());
    execute_checked(
        &mut editor,
        &Command::AddMark(MarkSpec::Key {
            at: ScorePoint::start_of(i.bars[2]),
            key: Key::traditional(-3, Mode::Minor).unwrap(),
        }),
    );
    execute_checked(
        &mut editor,
        &Command::SetInitial(Initial::Key(Key::traditional(2, Mode::Major).unwrap())),
    );
    execute_checked(
        &mut editor,
        &Command::AddMark(MarkSpec::Clef {
            staff: i.fl_staff,
            at: ScorePoint::new(i.bars[3], d(1, 2)),
            clef: Clef::BASS,
        }),
    );
    execute_checked(
        &mut editor,
        &Command::SetInitial(Initial::Clef {
            staff: i.fl_staff,
            clef: Clef::ALTO,
        }),
    );
    assert_eq!(events(editor.document()), before);
    // A clef change where the initial clef is (the very start) is refused.
    assert_eq!(
        refused(
            &mut editor,
            &Command::AddMark(MarkSpec::Clef {
                staff: i.fl_staff,
                at: ScorePoint::start_of(i.bars[0]),
                clef: Clef::BASS,
            })
        ),
        CommandError::Document(DocumentError::DuplicateMark)
    );
}

// ------------------------------------------------------------------------------------------------
// The form (ADR-0026 X1–X4)
// ------------------------------------------------------------------------------------------------

/// A repeated section is added with its two barlines (ADR-0026 X1); one overlapping the section of
/// bars 1–2 is refused, and nothing of it stays (ADR-0026 X3). (Bars 3–4: bar 5 is the source of
/// the flute's bar repeat, and no form marking may lie between a bar repeat's source and its end.)
#[test]
fn repeats_are_added_in_pairs() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let changes = execute_checked(
        &mut editor,
        &Command::AddRepeat {
            start: i.bars[3],
            end: i.bars[4],
            times: 3,
        },
    );
    assert!(changes.form_changed());
    assert_eq!(changes.inserted().len(), 2);
    assert!(matches!(
        refused(
            &mut editor,
            &Command::AddRepeat {
                start: i.bars[0],
                end: i.bars[1],
                times: 2,
            }
        ),
        CommandError::Document(DocumentError::Invalid(Violation::RepeatsOverlap(_)))
    ));
}

/// A group of voltas is added at once with the end repeat it lacks (ADR-0026 X1); a single volta is
/// refused.
#[test]
fn voltas_are_added_by_group() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    // A new section, bars 4–5, without end repeat yet: first ending bar 5, second ending bar 6.
    // The bar repeat of the flute at bar 6 would put a form marking inside its range, so the
    // test removes it first.
    let repeat = editor.document().instrument(i.flute).unwrap().bar_repeats()[0].id();
    execute_checked(
        &mut editor,
        &Command::RemoveMark(ObjectRef::BarRepeat(repeat)),
    );
    execute_checked(
        &mut editor,
        &Command::AddMark(MarkSpec::RepeatStart { bar: i.bars[4] }),
    );
    let volta = |bar: usize, pass: u8, closed| NewVolta {
        first: i.bars[bar],
        last: i.bars[bar],
        passes: PassSet::of(&[pass]).unwrap(),
        closed,
        text: None,
    };
    assert!(matches!(
        refused(&mut editor, &Command::AddVoltas(vec![volta(5, 1, true)])),
        CommandError::Document(DocumentError::Invalid(_))
    ));
    let changes = execute_checked(
        &mut editor,
        &Command::AddVoltas(vec![volta(5, 1, true), volta(6, 2, false)]),
    );
    // Two voltas and the end repeat at the end of bar 5, with the group's two plays.
    assert_eq!(changes.inserted().len(), 3);
    let form = editor.document().form();
    assert_eq!(form.voltas().len(), 2);
    assert!(
        form.repeat_ends()
            .iter()
            .any(|r| r.bar() == i.bars[5] && r.times() == 2)
    );
    // Removing the first volta leaves a group of one: the cascade deletes the other (ADR-0026 X2),
    // the end repeat stays.
    let first = ObjectRef::Volta(form.voltas()[0].id());
    let changes = execute_checked(&mut editor, &Command::RemoveMark(first));
    assert_eq!(changes.removed().len(), 2);
    assert_eq!(editor.document().form().voltas().len(), 0);
}

/// Bars inserted between the first and the second volta of a group join the first one, and its
/// end repeat moves to their end (ADR-0026 X11): the group stays valid.
#[test]
fn bars_inserted_between_voltas_join_the_first() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    // The section of bars 1–2 gets a first ending on bar 2 (its end repeat is already there) and a
    // second ending on bar 3.
    let volta = |bar: usize, pass: u8| NewVolta {
        first: i.bars[bar],
        last: i.bars[bar],
        passes: PassSet::of(&[pass]).unwrap(),
        closed: pass == 1,
        text: None,
    };
    execute_checked(
        &mut editor,
        &Command::AddVoltas(vec![volta(2, 1), volta(3, 2)]),
    );
    let changes = execute_checked(
        &mut editor,
        &Command::InsertBars {
            at: BarIndex::new(3),
            count: 2,
        },
    );
    let doc = editor.document();
    let bars = doc.bars().ids();
    let form = doc.form();
    assert_eq!(form.voltas()[0].first(), bars[2]);
    assert_eq!(form.voltas()[0].last(), bars[4]);
    assert_eq!(form.voltas()[1].first(), bars[5]);
    assert!(form.repeat_ends().iter().any(|r| r.bar() == bars[4]));
    assert!(form.repeat_ends().iter().all(|r| r.bar() != bars[2]));
    assert!(changes.form_changed());
}

/// Removing the segno deletes the D.S. jump that needed it, in the same step (ADR-0026 X2).
#[test]
fn removing_a_marker_deletes_its_jump_in_cascade() {
    let mut editor = Editor::new(rich());
    let form = editor.document().form();
    let segno = form
        .markers()
        .iter()
        .find(|m| m.kind() == MarkerKind::Segno)
        .unwrap()
        .id();
    let jump = form.jumps()[0].id();
    let changes = execute_checked(&mut editor, &Command::RemoveMark(ObjectRef::Marker(segno)));
    assert_eq!(
        changes.removed(),
        &[ObjectRef::Marker(segno), ObjectRef::Jump(jump)]
    );
    // Undo brings both back at once.
    editor.undo().unwrap();
    assert_eq!(editor.document().form().jumps().len(), 1);
}

/// A bar repeat over bars that hold music is refused (ADR-0026 X4); over empty bars it is accepted.
#[test]
fn a_bar_repeat_needs_empty_bars() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let repeat = |bar: usize| {
        Command::AddMark(MarkSpec::BarRepeat {
            staff: i.fl_staff,
            first: i.bars[bar],
            count: 1,
            span: BarRepeatSpan::One,
        })
    };
    // The flute plays a quarter in bar 4.
    assert!(matches!(
        refused(&mut editor, &repeat(4)),
        CommandError::Document(DocumentError::Invalid(Violation::BarRepeatNotEmpty(_)))
    ));
    // Bar 7 is empty (the D.S. jump at its end closes the range, it is not inside it).
    execute_checked(&mut editor, &repeat(7));
    // Bar 3 holds the flute's whole note: refused as bar 4 was.
    assert!(matches!(
        refused(&mut editor, &repeat(3)),
        CommandError::Document(DocumentError::Invalid(Violation::BarRepeatNotEmpty(_)))
    ));
}

/// A marker is moved with a new text and keeps its ID; a move into a bar repeat's range and an
/// empty text are refused.
#[test]
fn form_markings_are_changed_in_place() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let fine = editor
        .document()
        .form()
        .markers()
        .iter()
        .find(|m| m.kind() == MarkerKind::Fine)
        .unwrap()
        .id();
    let changes = execute_checked(
        &mut editor,
        &Command::ChangeMark {
            mark: ObjectRef::Marker(fine),
            to: MarkSpec::Marker {
                bar: i.bars[3],
                kind: MarkerKind::Fine,
                text: Some("Fine (2nd time)".into()),
            },
        },
    );
    assert_eq!(changes.replaced(), &[ObjectRef::Marker(fine)]);
    // At the end of bar 5 it would lie between the flute's bar repeat and its source (ADR-0026 X3).
    assert!(matches!(
        refused(
            &mut editor,
            &Command::ChangeMark {
                mark: ObjectRef::Marker(fine),
                to: MarkSpec::Marker {
                    bar: i.bars[5],
                    kind: MarkerKind::Fine,
                    text: None,
                },
            }
        ),
        CommandError::Document(DocumentError::Invalid(Violation::BarRepeatAcrossForm(_)))
    ));
    assert_eq!(
        refused(
            &mut editor,
            &Command::ChangeMark {
                mark: ObjectRef::Marker(fine),
                to: MarkSpec::Marker {
                    bar: i.bars[3],
                    kind: MarkerKind::Fine,
                    text: Some("  ".into()),
                },
            }
        ),
        CommandError::Document(DocumentError::EmptyText)
    );
}

// ------------------------------------------------------------------------------------------------
// Tuplets (ADR-0026 X7)
// ------------------------------------------------------------------------------------------------

/// A tuplet is added empty and filled with `EnterEvent`; a new ratio that makes its notes overlap
/// is refused, one that keeps their durations is accepted and names their cell; removing it
/// takes its notes along (ADR-0026 X7).
#[test]
fn tuplets_are_added_changed_and_removed() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let eighth = NoteValue::plain(BaseValue::Eighth);
    let changes = execute_checked(
        &mut editor,
        &Command::AddTuplet {
            voice: i.fl,
            tuplet: Tuplet::new(3, 2, eighth).unwrap(),
            parent: None,
        },
    );
    let ObjectRef::Tuplet(new) = inserted(&changes, |o| matches!(o, ObjectRef::Tuplet(_))) else {
        panic!("the inserted object is a tuplet");
    };
    // Its first note, in the empty bar 5 of the flute.
    execute_checked(
        &mut editor,
        &Command::EnterEvent(NewEvent {
            at: Placement::new(i.fl, i.bars[5], Duration::ZERO).in_tuplet(new),
            body: NewBody::Chord {
                value: eighth,
                pitches: vec![Pitch::new(Step::C, Alteration::NATURAL, 5)],
                grace: None,
            },
        }),
    );
    // The triplet of the right hand in bar 1 holds three eighths at 0, 1/12 and 2/12.
    let triplet = editor.document().events(i.rh, i.bars[1]).unwrap()[0]
        .event()
        .tuplet()
        .unwrap();
    // 3:3 would make each eighth last 1/8: they would overlap.
    assert!(matches!(
        refused(
            &mut editor,
            &Command::ChangeTuplet {
                tuplet: triplet,
                to: Tuplet::new(3, 3, eighth).unwrap(),
            }
        ),
        CommandError::Document(DocumentError::Invalid(Violation::Overlap(_)))
    ));
    // 6:4 is the same ratio: accepted, the cell of its notes is named.
    let changes = execute_checked(
        &mut editor,
        &Command::ChangeTuplet {
            tuplet: triplet,
            to: Tuplet::new(6, 4, eighth).unwrap(),
        },
    );
    assert_eq!(changes.cells(), &[(i.rh, i.bars[1])]);
    assert_eq!(changes.replaced(), &[ObjectRef::Tuplet(triplet)]);
    // Removing it deletes its three notes, the whole bar of the right hand.
    let changes = execute_checked(&mut editor, &Command::RemoveTuplet(triplet));
    assert_eq!(
        changes
            .removed()
            .iter()
            .filter(|o| matches!(o, ObjectRef::Event(_)))
            .count(),
        3
    );
    assert_eq!(editor.document().events(i.rh, i.bars[1]).unwrap(), []);
    assert!(
        editor
            .document()
            .voice(i.rh)
            .unwrap()
            .1
            .tuplet(triplet)
            .is_none()
    );
}

/// A nested tuplet that holds no event is removed with its parent; one nested too deep is refused.
#[test]
fn removing_a_tuplet_removes_the_nested_ones() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let eighth = NoteValue::plain(BaseValue::Eighth);
    let mut parent = None;
    let mut chain = Vec::new();
    for _ in 0..crate::MAX_TUPLET_DEPTH {
        let changes = execute_checked(
            &mut editor,
            &Command::AddTuplet {
                voice: i.fl,
                tuplet: Tuplet::new(3, 2, eighth).unwrap(),
                parent,
            },
        );
        let ObjectRef::Tuplet(id) = changes.inserted()[0] else {
            panic!("the inserted object is a tuplet");
        };
        chain.push(id);
        parent = Some(id);
    }
    assert_eq!(
        refused(
            &mut editor,
            &Command::AddTuplet {
                voice: i.fl,
                tuplet: Tuplet::new(3, 2, eighth).unwrap(),
                parent,
            }
        ),
        CommandError::Document(DocumentError::TupletTooDeep)
    );
    let changes = execute_checked(&mut editor, &Command::RemoveTuplet(chain[0]));
    assert_eq!(changes.removed().len(), chain.len());
}

// ------------------------------------------------------------------------------------------------
// Per-bar data (ADR-0026 X9) and gestures (ADR-0026 X5)
// ------------------------------------------------------------------------------------------------

/// The beat grouping, an uncounted bar and a numbering restart are set and undone; the displayed
/// numbers follow.
#[test]
fn per_bar_data_is_set() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let changes = execute_checked(
        &mut editor,
        &Command::SetBarGrouping {
            bar: i.bars[3],
            grouping: Some(BeatGrouping::new(&[3, 3, 2], 8).unwrap()),
        },
    );
    assert_eq!(changes.meter_from(), Some(BarIndex::new(3)));
    execute_checked(
        &mut editor,
        &Command::SetUncounted {
            bar: i.bars[2],
            uncounted: true,
        },
    );
    execute_checked(
        &mut editor,
        &Command::SetNumberingRestart {
            bar: i.bars[5],
            number: Some(100),
        },
    );
    let doc = editor.document();
    assert_eq!(doc.bar_label(i.bars[2]).unwrap(), None);
    assert_eq!(doc.bar_label(i.bars[6]).unwrap().unwrap().number, 101);
}

/// Dragging a hairpin's end is a gesture of `ChangeMark` updates: one step of undo, and the
/// history keeps one old value however long the drag (X5 with the compaction of ADR-0026 §4).
#[test]
fn dragging_a_hairpin_is_one_step() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let before = editor.document().clone();
    let hairpin = editor.document().instrument(i.piano).unwrap().hairpins()[0].id();
    // The memory of one step holding a single hairpin change, measured on a twin editor.
    let mut twin = Editor::new(rich());
    twin.execute(&Command::ChangeMark {
        mark: ObjectRef::Hairpin(hairpin),
        to: MarkSpec::Hairpin {
            instrument: i.piano,
            start: ScorePoint::start_of(i.bars[0]),
            end: ScorePoint::new(i.bars[1], d(1, 4)),
            placement: DynamicPlacement::instrument(i.lh_staff),
            shape: HairpinShape::CRESCENDO,
        },
    })
    .unwrap();
    let one = twin.history_bytes();
    editor.begin_gesture().unwrap();
    for step in 1..=24 {
        let end = ScorePoint::new(i.bars[1], d(step % 4, 4));
        let update = Command::ChangeMark {
            mark: ObjectRef::Hairpin(hairpin),
            to: MarkSpec::Hairpin {
                instrument: i.piano,
                start: ScorePoint::start_of(i.bars[0]),
                end,
                placement: DynamicPlacement::instrument(i.lh_staff),
                shape: HairpinShape::CRESCENDO,
            },
        };
        editor.update_gesture(&update).unwrap();
    }
    editor.end_gesture().unwrap();
    assert_eq!(editor.undo_steps(), 1);
    // The compaction keeps one old value: the 24 updates weigh what one does.
    assert_eq!(editor.history_bytes(), one);
    editor.undo().unwrap();
    assert!(editor.document().same_content(&before));
}
