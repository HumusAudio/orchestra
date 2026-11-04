// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the commands and of the gestures (ADR-0026 §4, §6, §8; SCALETTA F1.11b, part 2a).
//!
//! # Why inside the crate
//! They start from the rich document of `crate::editor_tests` (every kind of object, a form with
//! a repeat, a D.S. al Fine, a bar repeat, a pass rule), whose helpers are crate-private, and they
//! look at the backward tie index directly.
//!
//! # What is checked
//! - One hand-written case per rule of the commands: the owner's decisions V2–V6 and W1–W6, and
//!   the rules of ADR-0026 §6.
//! - A property test: random sequences of random commands, with undos and redos. After **every**
//!   command the full [`Document::validate`] passes and the derived indices agree with a rebuild
//!   (ADR-0026 §8 (a), (d)); a refused command leaves the document as it was (e); undoing
//!   everything gives back the original (b); a redo after an undo gives back the state after the
//!   command (c).

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]
#![allow(
    clippy::too_many_lines,
    reason = "the random command is a long list of small, independent cases"
)]

use orchestra_core_time::{
    BarIndex, BarKind, Duration, IrregularBar, Meter, MeterChange, MeterMap, Rational, Tempo,
};
use proptest::prelude::*;

use crate::command::{Command, EventChange, NewBody, NewEvent};
use crate::document::{Document, DocumentBuilder, DocumentError, Placement};
use crate::edit::ObjectRef;
use crate::editor::{CommandError, Editor};
use crate::editor_tests::{assert_consistent, rich};
use crate::event::{BaseValue, EventBody, NoteValue, RestValue, Tie};
use crate::id::{BarId, EventId, NoteId, StaffId, VoiceId};
use crate::marking::{Fermata, Grace, GraceAttach, GraceKind};
use crate::marks::Clef;
use crate::pitch::{Alteration, Interval, Key, Mode, Pitch, Step};
use crate::tempo_mark::{Metronome, TempoMark, TempoMarkChange};
use crate::validate::Violation;

/// Shorthand: a duration of `n/d` whole notes.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// Shorthand: a pitch in octave 4.
const fn pitch(step: Step) -> Pitch {
    Pitch::new(step, Alteration::NATURAL, 4)
}

/// Shorthand: a plain written value.
const fn value(base: BaseValue) -> NoteValue {
    NoteValue::plain(base)
}

/// The IDs of the rich document the tests use.
struct Ids {
    /// The bars, in order.
    bars: Vec<BarId>,
    /// The piano's right hand.
    rh: VoiceId,
    /// The piano's left hand.
    lh: VoiceId,
    /// The flute.
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
        rh: piano.voices()[0].id(),
        lh: piano.voices()[1].id(),
        fl: flute.voices()[0].id(),
        fl_staff: flute.staves()[0].id(),
    }
}

/// A command entering a single note.
fn note(voice: VoiceId, bar: BarId, offset: Duration, v: NoteValue, p: &[Pitch]) -> Command {
    Command::EnterEvent(NewEvent {
        at: Placement::new(voice, bar, offset),
        body: NewBody::Chord {
            value: v,
            pitches: p.to_vec(),
            grace: None,
        },
    })
}

/// The events of a voice in a bar, as (offset, ID).
fn events_of(doc: &Document, voice: VoiceId, bar: BarId) -> Vec<(Duration, EventId)> {
    doc.events(voice, bar)
        .unwrap()
        .iter()
        .map(|p| (p.offset(), p.event().id()))
        .collect()
}

/// The noteheads of an event.
fn notes_of(doc: &Document, event: EventId) -> Vec<NoteId> {
    doc.event(event)
        .unwrap()
        .event()
        .chord()
        .unwrap()
        .notes()
        .map(crate::event::Note::id)
        .collect()
}

/// Executes a command, then checks the document is valid and consistent, that undo gives back
/// the document from before and redo the one after. Returns the editor after the command.
fn execute_checked(editor: &mut Editor, command: &Command) -> crate::change::ChangeSet {
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

/// The error of a refused command, after checking it changed nothing.
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

// ------------------------------------------------------------------------------------------------
// Entering events
// ------------------------------------------------------------------------------------------------

/// A note in free space is entered as it is; over another event it is refused (V2).
#[test]
fn entering_needs_free_space() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let changes = execute_checked(
        &mut editor,
        &note(
            i.fl,
            i.bars[4],
            d(1, 4),
            value(BaseValue::Half),
            &[pitch(Step::B)],
        ),
    );
    assert_eq!(changes.cells(), &[(i.fl, i.bars[4])]);
    assert_eq!(events_of(editor.document(), i.fl, i.bars[4]).len(), 2);
    // The quarter at 0 is still there: a half from 1/8 overlaps it.
    let error = refused(
        &mut editor,
        &note(
            i.fl,
            i.bars[4],
            d(1, 8),
            value(BaseValue::Eighth),
            &[pitch(Step::C)],
        ),
    );
    assert!(matches!(
        error,
        CommandError::Document(DocumentError::Invalid(Violation::Overlap(_)))
    ));
}

/// A note crossing a barline becomes a tie chain, noteheads tied pitch by pitch; the backward
/// index knows the ties (V3).
#[test]
fn a_chord_across_a_barline_becomes_a_tie_chain() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    // A dotted half at 3/4 of bar 4: a quarter in bar 4, a half in bar 5, tied.
    let chord = [pitch(Step::G), pitch(Step::C)];
    execute_checked(
        &mut editor,
        &note(
            i.fl,
            i.bars[4],
            d(3, 4),
            NoteValue::new(BaseValue::Half, 1).unwrap(),
            &chord,
        ),
    );
    let doc = editor.document();
    let first = events_of(doc, i.fl, i.bars[4])[1];
    let second = events_of(doc, i.fl, i.bars[5])[0];
    assert_eq!(first.0, d(3, 4));
    assert_eq!(second.0, Duration::ZERO);
    assert_eq!(doc.metric_duration(first.1).unwrap(), d(1, 4));
    assert_eq!(doc.metric_duration(second.1).unwrap(), d(1, 2));
    let (from, to) = (notes_of(doc, first.1), notes_of(doc, second.1));
    for (source, target) in from.iter().zip(&to) {
        let (_, n) = doc.note(*source).unwrap();
        assert_eq!(n.tie(), Tie::To(*target));
        assert_eq!(doc.ties_into(*target), &[*source]);
        // Tied to the same pitch.
        assert_eq!(n.pitch(), doc.note(*target).unwrap().1.pitch());
    }
}

/// A part longer than one value is written with the longest values first: five eighths left in a
/// bar are a half and an eighth, tied (V3).
#[test]
fn a_part_is_written_with_the_longest_values_first() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    // A whole note at 3/8 of the empty bar 5 of the flute: 5/8 in bar 5, 3/8 in bar 6… but bar 6
    // is under a bar repeat, so use the left hand, whose bars 3–4 are empty.
    execute_checked(
        &mut editor,
        &note(
            i.lh,
            i.bars[3],
            d(3, 8),
            value(BaseValue::Whole),
            &[pitch(Step::E)],
        ),
    );
    let doc = editor.document();
    let in_3: Vec<Duration> = events_of(doc, i.lh, i.bars[3])
        .iter()
        .map(|(_, e)| doc.metric_duration(*e).unwrap())
        .collect();
    let in_4: Vec<Duration> = events_of(doc, i.lh, i.bars[4])
        .iter()
        .map(|(_, e)| doc.metric_duration(*e).unwrap())
        .collect();
    assert_eq!(in_3, vec![d(1, 2), d(1, 8)]);
    assert_eq!(in_4, vec![d(3, 8)]);
}

/// A rest across a barline becomes several rests, not tied (there is nothing to tie).
#[test]
fn a_rest_across_a_barline_becomes_rests() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    execute_checked(
        &mut editor,
        &Command::EnterEvent(NewEvent {
            at: Placement::new(i.lh, i.bars[3], d(1, 2)),
            body: NewBody::Rest(RestValue::Value(value(BaseValue::Whole))),
        }),
    );
    let doc = editor.document();
    assert_eq!(events_of(doc, i.lh, i.bars[3]).len(), 1);
    assert_eq!(events_of(doc, i.lh, i.bars[4]).len(), 1);
}

/// Past the last bar (W3), across a barline inside a tuplet (V4), at the end of a regular bar,
/// and a remainder no value can write: all refused with their typed errors.
#[test]
fn entering_refusals() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let doc = editor.document().clone();
    let triplet = *doc
        .voice(i.rh)
        .unwrap()
        .1
        .content
        .tuplets
        .keys()
        .next()
        .unwrap();
    let cases = [
        (
            note(
                i.lh,
                i.bars[7],
                d(1, 2),
                value(BaseValue::Whole),
                &[pitch(Step::C)],
            ),
            DocumentError::PastLastBar,
        ),
        (
            Command::EnterEvent(NewEvent {
                at: Placement::new(i.rh, i.bars[1], d(23, 24)).in_tuplet(triplet),
                body: NewBody::Chord {
                    value: value(BaseValue::Eighth),
                    pitches: vec![pitch(Step::C)],
                    grace: None,
                },
            }),
            DocumentError::TupletAcrossBarline,
        ),
        (
            note(
                i.lh,
                i.bars[3],
                d(1, 1),
                value(BaseValue::Quarter),
                &[pitch(Step::C)],
            ),
            DocumentError::StartsPastBarline,
        ),
        (
            // A third of a whole note left in the bar cannot be written without a tuplet.
            note(
                i.lh,
                i.bars[3],
                d(2, 3),
                value(BaseValue::Half),
                &[pitch(Step::C)],
            ),
            DocumentError::NotWritable,
        ),
        (
            note(
                i.lh,
                i.bars[3],
                Duration::ZERO,
                value(BaseValue::Quarter),
                &[],
            ),
            DocumentError::EmptyChord,
        ),
    ];
    for (command, error) in cases {
        assert_eq!(
            refused(&mut editor, &command),
            CommandError::Document(error)
        );
    }
}

/// A note entered past the end of a free bar lengthens the bar instead of being split (V5).
#[test]
fn a_free_bar_grows() {
    let mut meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    meters
        .set_irregular_bar(
            BarIndex::new(1),
            IrregularBar {
                length: d(1, 4),
                kind: BarKind::Free,
            },
        )
        .unwrap();
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(60).unwrap()).unwrap();
    let mut b = DocumentBuilder::new(
        meters,
        TempoMark::immediate(None, Metronome::shown(tempo)).unwrap(),
        Key::traditional(0, Mode::Major).unwrap(),
    )
    .unwrap();
    let bars: Vec<BarId> = (0..3).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Violin", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let voice = b.document().track(track).unwrap().instrument().voices()[0].id();
    let mut editor = Editor::new(b.build().unwrap());
    let start_of_2 = editor.document().bar_start(bars[2]).unwrap();
    let changes = execute_checked(
        &mut editor,
        &note(
            voice,
            bars[1],
            d(1, 4),
            value(BaseValue::Half),
            &[pitch(Step::A)],
        ),
    );
    let doc = editor.document();
    assert_eq!(
        doc.meter_map().bar_length(BarIndex::new(1)).unwrap(),
        d(3, 4)
    );
    assert_eq!(events_of(doc, voice, bars[1]).len(), 1);
    assert_eq!(events_of(doc, voice, bars[2]).len(), 0);
    // The bars after it moved later: the meter map and the tempo map say so.
    assert_eq!(changes.meter_from(), Some(BarIndex::new(1)));
    assert!(doc.bar_start(bars[2]).unwrap() > start_of_2);
}

// ------------------------------------------------------------------------------------------------
// Deleting events
// ------------------------------------------------------------------------------------------------

/// Deleting a note deletes the grace notes ornamenting it; deleting the first chord deletes the
/// slur starting on it (V6). Both leave gaps.
#[test]
fn deleting_takes_graces_and_slurs_along() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let bar0 = events_of(editor.document(), i.rh, i.bars[0]);
    // At 1/4: two Before graces, then the D.
    let d_note = bar0.iter().find(|(o, e)| {
        *o == d(1, 4)
            && editor
                .document()
                .event(*e)
                .unwrap()
                .event()
                .grace()
                .is_none()
    });
    let changes = execute_checked(&mut editor, &Command::DeleteEvent(d_note.unwrap().1));
    assert_eq!(changes.removed().len(), 3, "the note and its two graces");
    let first = bar0[0].1;
    let slur = editor.document().slurs_at(first)[0];
    let changes = execute_checked(&mut editor, &Command::DeleteEvent(first));
    assert!(changes.removed().contains(&ObjectRef::Slur(slur)));
    assert!(editor.document().slur(slur).is_none());
}

/// Deleting the note a tie leads into clears the tie (V6); deleting every member of a tuplet
/// deletes the tuplet.
#[test]
fn deleting_clears_ties_and_empty_tuplets() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    execute_checked(
        &mut editor,
        &note(
            i.lh,
            i.bars[3],
            d(3, 4),
            value(BaseValue::Half),
            &[pitch(Step::G)],
        ),
    );
    let target = events_of(editor.document(), i.lh, i.bars[4])[0].1;
    let source = events_of(editor.document(), i.lh, i.bars[3])[0].1;
    execute_checked(&mut editor, &Command::DeleteEvent(target));
    let note = notes_of(editor.document(), source)[0];
    assert_eq!(editor.document().note(note).unwrap().1.tie(), Tie::None);
    // The triplet of bar 1: delete its three members in one transaction.
    let members: Vec<EventId> = events_of(editor.document(), i.rh, i.bars[1])
        .into_iter()
        .map(|(_, e)| e)
        .collect();
    let tuplets_before = editor.document().voice(i.rh).unwrap().1.tuplet_count();
    editor
        .transact(|tx| {
            for &m in &members {
                tx.execute(&Command::DeleteEvent(m))?;
            }
            Ok(())
        })
        .unwrap();
    editor.document().validate().unwrap();
    assert_eq!(
        editor.document().voice(i.rh).unwrap().1.tuplet_count(),
        tuplets_before - 1
    );
}

// ------------------------------------------------------------------------------------------------
// Changing events
// ------------------------------------------------------------------------------------------------

/// A new pitch on a tied note changes the whole tie chain (W2).
#[test]
fn a_pitch_change_follows_the_tie_chain() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    execute_checked(
        &mut editor,
        &note(
            i.lh,
            i.bars[3],
            d(1, 2),
            value(BaseValue::Whole),
            &[pitch(Step::G)],
        ),
    );
    let first = events_of(editor.document(), i.lh, i.bars[3])[0].1;
    let second = events_of(editor.document(), i.lh, i.bars[4])[0].1;
    // From the second note of the chain: the walk goes backwards through the tie index too.
    let n = notes_of(editor.document(), second)[0];
    let a_flat = Pitch::new(Step::A, Alteration::FLAT, 4);
    execute_checked(
        &mut editor,
        &Command::ChangeEvent {
            event: second,
            change: EventChange::Pitch {
                note: n,
                pitch: a_flat,
            },
        },
    );
    for event in [first, second] {
        let note = notes_of(editor.document(), event)[0];
        assert_eq!(editor.document().note(note).unwrap().1.pitch(), a_flat);
    }
}

/// Moving a note moves its grace notes with it; past the barline the bar becomes overfull, never
/// split (W1); onto another note it is refused.
#[test]
fn moving_keeps_graces_and_never_splits() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let doc = editor.document().clone();
    let d_note = events_of(&doc, i.rh, i.bars[0])
        .into_iter()
        .find(|(o, e)| *o == d(1, 4) && doc.event(*e).unwrap().event().grace().is_none())
        .unwrap()
        .1;
    // Onto the E at 1/2: refused.
    let error = refused(
        &mut editor,
        &Command::ChangeEvent {
            event: d_note,
            change: EventChange::Move {
                bar: i.bars[0],
                offset: d(1, 2),
            },
        },
    );
    assert!(matches!(
        error,
        CommandError::Document(DocumentError::Invalid(Violation::Overlap(_)))
    ));
    // To 7/8 of the empty bar 4 of the left hand: a quarter from 7/8 overfills the bar.
    execute_checked(
        &mut editor,
        &Command::ChangeEvent {
            event: d_note,
            change: EventChange::Move {
                bar: i.bars[4],
                offset: d(7, 8),
            },
        },
    );
    let doc = editor.document();
    assert!(
        doc.voice(i.rh).is_some(),
        "the event stays in its voice, which is the right hand"
    );
    let moved: Vec<(Duration, EventId)> = events_of(doc, i.rh, i.bars[4]);
    assert_eq!(moved.len(), 3, "the note and its two graces");
    assert!(moved.iter().all(|(o, _)| *o == d(7, 8)));
    assert_eq!(doc.event(d_note).unwrap().bar(), i.bars[4]);
}

/// Removing the last notehead deletes the event; a fermata, an added notehead and a new value
/// are plain replacements.
#[test]
fn small_changes() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let flute_a = events_of(editor.document(), i.fl, i.bars[4])[0].1;
    execute_checked(
        &mut editor,
        &Command::ChangeEvent {
            event: flute_a,
            change: EventChange::Fermata(Some(Fermata::Normal)),
        },
    );
    execute_checked(
        &mut editor,
        &Command::ChangeEvent {
            event: flute_a,
            change: EventChange::AddNote(pitch(Step::C)),
        },
    );
    assert_eq!(notes_of(editor.document(), flute_a).len(), 2);
    // The fermata survived the new notehead.
    assert_eq!(
        editor.document().event(flute_a).unwrap().event().fermata(),
        Some(Fermata::Normal)
    );
    execute_checked(
        &mut editor,
        &Command::ChangeEvent {
            event: flute_a,
            change: EventChange::Value(value(BaseValue::Half)),
        },
    );
    for n in notes_of(editor.document(), flute_a) {
        execute_checked(
            &mut editor,
            &Command::ChangeEvent {
                event: flute_a,
                change: EventChange::RemoveNote(n),
            },
        );
    }
    assert!(editor.document().event(flute_a).is_none());
}

/// A grace note entered where no main event is, or a note on a bar under a bar repeat, is refused
/// by the local check.
#[test]
fn the_local_check_refuses() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let grace = Command::EnterEvent(NewEvent {
        at: Placement::new(i.lh, i.bars[3], Duration::ZERO),
        body: NewBody::Chord {
            value: value(BaseValue::Eighth),
            pitches: vec![pitch(Step::D)],
            grace: Some(Grace {
                kind: GraceKind::Appoggiatura,
                attach: GraceAttach::Before,
            }),
        },
    });
    assert!(matches!(
        refused(&mut editor, &grace),
        CommandError::Document(DocumentError::Invalid(Violation::BeforeGraceWithoutEvent(
            _
        )))
    ));
    let under_repeat = note(
        i.fl,
        i.bars[6],
        Duration::ZERO,
        value(BaseValue::Quarter),
        &[pitch(Step::D)],
    );
    assert!(matches!(
        refused(&mut editor, &under_repeat),
        CommandError::Document(DocumentError::Invalid(Violation::BarRepeatNotEmpty(_)))
    ));
}

// ------------------------------------------------------------------------------------------------
// Gestures
// ------------------------------------------------------------------------------------------------

/// A drag of 200 updates is one step of undo whose history holds one old value (compaction);
/// cancelling gives back the document from before; nothing else runs meanwhile.
#[test]
fn a_gesture_is_one_compact_step() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let original = editor.document().clone();
    let flute_a = events_of(&original, i.fl, i.bars[4])[0].1;
    let drag = |k: i64| Command::ChangeEvent {
        event: flute_a,
        change: EventChange::Move {
            bar: i.bars[4],
            offset: d(k % 4, 8),
        },
    };
    assert_eq!(
        editor.update_gesture(&drag(1)),
        Err(CommandError::NoGesture)
    );
    editor.begin_gesture().unwrap();
    assert_eq!(editor.execute(&drag(1)), Err(CommandError::GestureOpen));
    assert_eq!(editor.undo(), Err(CommandError::GestureOpen));
    let revision = editor.document().revision();
    for k in 1..=200 {
        let live = editor.update_gesture(&drag(k)).unwrap();
        assert_eq!(live.cells(), &[(i.fl, i.bars[4])]);
        editor.document().validate().unwrap();
    }
    assert!(editor.document().revision() > revision);
    editor.end_gesture().unwrap();
    assert_eq!(editor.undo_steps(), 1);
    // One `Replace` with the old event, not 200.
    let one_step = editor.history_bytes();
    assert!(one_step < 1024, "the drag holds {one_step} bytes");
    editor.undo().unwrap().unwrap();
    assert!(editor.document().same_content(&original));
    editor.redo().unwrap().unwrap();
    let after = editor.document().clone();
    assert_eq!(after.event(flute_a).unwrap().offset(), d(0, 8));

    // Cancel: back to the state before the gesture, nothing recorded.
    editor.begin_gesture().unwrap();
    for k in 1..=10 {
        editor.update_gesture(&drag(k)).unwrap();
    }
    // A refused update leaves the gesture open with the updates before it.
    let onto_nothing = Command::ChangeEvent {
        event: flute_a,
        change: EventChange::Move {
            bar: i.bars[4],
            offset: d(-1, 8),
        },
    };
    assert!(editor.update_gesture(&onto_nothing).is_err());
    assert!(editor.in_gesture());
    editor.cancel_gesture().unwrap();
    assert!(!editor.in_gesture());
    assert!(editor.document().same_content(&after));
    assert_eq!(editor.undo_steps(), 1);
    assert_eq!(editor.cancel_gesture(), Err(CommandError::NoGesture));
}

/// A gesture derives the tempo map after each update that moves where bars start (the views need
/// it live), and its end reports where the map differs from before the gesture. Inserting a bar
/// moves every later bar, and with them the tempo marks.
#[test]
fn a_gesture_derives_the_tempo_map_live() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    editor.begin_gesture().unwrap();
    let changes = editor
        .update_gesture(&Command::InsertBars {
            at: BarIndex::new(2),
            count: 1,
        })
        .unwrap();
    assert!(changes.tempo_from().is_some());
    assert_consistent(editor.document());
    let changes = editor.end_gesture().unwrap();
    assert!(changes.tempo_from().is_some());
    assert_eq!(editor.document().bars().len(), i.bars.len() + 1);
}

// ------------------------------------------------------------------------------------------------
// Bars
// ------------------------------------------------------------------------------------------------

/// Inserting bars: at the start, in the middle, and refused between a bar repeat's source and its
/// end (W6).
#[test]
fn inserting_bars() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    execute_checked(
        &mut editor,
        &Command::InsertBars {
            at: BarIndex::FIRST,
            count: 2,
        },
    );
    execute_checked(
        &mut editor,
        &Command::InsertBars {
            at: BarIndex::new(5),
            count: 1,
        },
    );
    assert_eq!(editor.document().bars().len(), i.bars.len() + 3);
    // The flute's bar repeat is now on bar 9 with its source bar 8: inserting before 9 is refused,
    // before 8 or 10 is fine.
    let repeat = editor
        .document()
        .instruments()
        .nth(1)
        .unwrap()
        .bar_repeats()[0]
        .id();
    assert_eq!(
        refused(
            &mut editor,
            &Command::InsertBars {
                at: BarIndex::new(9),
                count: 1
            }
        ),
        CommandError::Document(DocumentError::InsideBarRepeat(repeat))
    );
    for at in [10, 8] {
        execute_checked(
            &mut editor,
            &Command::InsertBars {
                at: BarIndex::new(at),
                count: 1,
            },
        );
    }
}

/// Deleting bar 2: its whole-bar rest goes, its clef change moves to bar 3, its end repeat goes,
/// and in cascade the pass rule that needed the repeated section (W5).
#[test]
fn deleting_a_bar_carries_the_clef_and_cascades() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let changes = execute_checked(
        &mut editor,
        &Command::DeleteBars {
            at: BarIndex::new(2),
            count: 1,
        },
    );
    let doc = editor.document();
    let piano = doc.instruments().next().unwrap();
    let clef = &piano.staves()[1].clef_changes().as_slice()[0];
    assert_eq!((clef.bar(), clef.offset()), (i.bars[3], Duration::ZERO));
    assert_eq!(doc.form().repeat_ends().len(), 0);
    assert_eq!(piano.pass_rules().len(), 0);
    assert!(
        changes
            .removed()
            .iter()
            .any(|r| matches!(r, ObjectRef::PassRule(_)))
    );
}

/// Deleting bar 5: the gradual tempo change on it (ending at the start of bar 6) arrives at bar 6
/// as an immediate mark of its target tempo, without text (W4); the flute's key override moves to
/// bar 6; the bar repeat that repeated bar 5 is deleted (W5).
#[test]
fn deleting_a_bar_carries_the_tempo_and_key() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    execute_checked(
        &mut editor,
        &Command::DeleteBars {
            at: BarIndex::new(5),
            count: 1,
        },
    );
    let doc = editor.document();
    let marks = doc.tempo_marks().as_slice();
    let carried = marks.iter().find(|m| m.bar() == i.bars[6]).unwrap();
    assert_eq!(carried.offset(), Duration::ZERO);
    assert_eq!(carried.value().text(), None);
    let Some(TempoMarkChange::Immediate(m)) = carried.value().change() else {
        panic!("not immediate");
    };
    assert_eq!(
        m.tempo,
        Tempo::new(d(1, 4), Rational::from_integer(132).unwrap()).unwrap()
    );
    let flute = doc.instruments().nth(1).unwrap();
    assert_eq!(flute.key_overrides().as_slice()[0].bar(), i.bars[6]);
    assert_eq!(flute.bar_repeats().len(), 0);
}

/// Deleting the first bars: the events go, the segno on bar 3 goes and the D.S. jump that needed
/// it goes in cascade; deleting every bar is refused.
#[test]
fn deleting_the_first_bars() {
    let mut editor = Editor::new(rich());
    let total = editor.document().bars().len();
    execute_checked(
        &mut editor,
        &Command::DeleteBars {
            at: BarIndex::FIRST,
            count: 4,
        },
    );
    let doc = editor.document();
    assert_eq!(doc.bars().len(), total - 4);
    assert_eq!(doc.form().jumps().len(), 0);
    // The tempo and key of old bar 4 are now the initial ones.
    assert_eq!(doc.initial_key(), Key::traditional(1, Mode::Major).unwrap());
    assert!(
        doc.tempo_marks()
            .as_slice()
            .iter()
            .all(|m| m.bar() != doc.bars().ids()[0] || !m.offset().is_zero())
    );
    let remaining = u32::try_from(doc.bars().len()).unwrap();
    assert_eq!(
        refused(
            &mut editor,
            &Command::DeleteBars {
                at: BarIndex::FIRST,
                count: remaining
            }
        ),
        CommandError::Document(DocumentError::NoBarsLeft)
    );
}

// ------------------------------------------------------------------------------------------------
// The property test
// ------------------------------------------------------------------------------------------------

/// One random command, built from the document as it is: `op` chooses the kind, `a` and `b` the
/// objects and values.
fn random_command(doc: &Document, op: u8, a: usize, b: usize) -> Option<Command> {
    let bars = doc.bars().ids();
    let voices: Vec<VoiceId> = doc
        .instruments()
        .flat_map(|i| i.voices().iter().map(crate::document::Voice::id))
        .collect();
    let events: Vec<EventId> = voices
        .iter()
        .flat_map(|v| {
            bars.iter()
                .flat_map(move |bar| doc.events(*v, *bar).unwrap().iter())
                .map(|p| p.event().id())
        })
        .collect();
    let bar = *bars.get(a % bars.len())?;
    let voice = *voices.get(b % voices.len())?;
    let offset = d(i64::try_from(b % 8).ok()?, 8);
    let values = [
        BaseValue::Whole,
        BaseValue::Half,
        BaseValue::Quarter,
        BaseValue::Eighth,
    ];
    let v = NoteValue::new(values[a % 4], u8::try_from(b % 2).ok()?).ok()?;
    let steps = [Step::C, Step::D, Step::E, Step::F, Step::G];
    let event = events.get(a % events.len().max(1)).copied();
    Some(match op % 9 {
        0 | 1 => note(voice, bar, offset, v, &[pitch(steps[b % 5])]),
        2 => Command::EnterEvent(NewEvent {
            at: Placement::new(voice, bar, offset),
            body: NewBody::Rest(RestValue::Value(v)),
        }),
        3 => Command::DeleteEvent(event?),
        4 => Command::ChangeEvent {
            event: event?,
            change: EventChange::Move { bar, offset },
        },
        5 => {
            let id = event?;
            let found = doc.event(id)?;
            let n = found.event().chord()?.notes().next()?.id();
            Command::ChangeEvent {
                event: id,
                change: match b % 4 {
                    0 => EventChange::Pitch {
                        note: n,
                        pitch: pitch(steps[a % 5]),
                    },
                    1 => EventChange::Value(v),
                    2 => EventChange::AddNote(pitch(steps[a % 5])),
                    _ => EventChange::RemoveNote(n),
                },
            }
        }
        6 => {
            // A tie from a note to the first note of another event (often refused).
            let id = event?;
            let n = doc.event(id)?.event().chord()?.notes().next()?.id();
            let other = events.get(b % events.len())?;
            let target = doc.event(*other)?.event().chord()?.notes().next()?.id();
            Command::ChangeEvent {
                event: id,
                change: EventChange::Tie {
                    note: n,
                    tie: Tie::To(target),
                },
            }
        }
        7 => Command::InsertBars {
            at: BarIndex::new(u32::try_from(a % (bars.len() + 1)).ok()?),
            count: u32::try_from(1 + b % 2).ok()?,
        },
        _ => Command::DeleteBars {
            at: BarIndex::new(u32::try_from(a % bars.len()).ok()?),
            count: u32::try_from(1 + b % 2).ok()?,
        },
    })
}

/// The errors a random command may meet: refusals of the commands, never a bug of the model.
fn expected_refusal(error: CommandError) -> bool {
    !matches!(
        error,
        CommandError::Rollback { .. }
            | CommandError::Document(DocumentError::Inconsistent | DocumentError::BarsNotEmpty(_))
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Random commands on the rich document, with undos and redos: valid after every command,
    /// unchanged after a refusal, back to the original after undoing everything.
    #[test]
    fn commands_keep_the_document_valid(
        steps in prop::collection::vec((0_u8..9, 0_usize..64, 0_usize..64, 0_u8..4), 1..24),
    ) {
        let mut editor = Editor::new(rich());
        let original = editor.document().clone();
        for (op, a, b, action) in steps {
            let Some(command) = random_command(editor.document(), op, a, b) else { continue };
            let before = editor.document().clone();
            match editor.execute(&command) {
                Ok(changes) => {
                    let after = editor.document().clone();
                    prop_assert!(
                        after.validate().is_ok(),
                        "{:?} left {:?}", command, after.validate()
                    );
                    assert_consistent(&after);
                    if changes.is_empty() {
                        prop_assert!(after.same_content(&before));
                    }
                    // Sometimes undo and redo at once: (c) do, undo, redo equals do.
                    if action == 0 && !changes.is_empty() {
                        editor.undo().unwrap().unwrap();
                        prop_assert!(editor.document().same_content(&before));
                        editor.redo().unwrap().unwrap();
                        prop_assert!(editor.document().same_content(&after));
                    }
                }
                Err(error) => {
                    prop_assert!(expected_refusal(error), "{:?}: {:?}", command, error);
                    prop_assert!(editor.document().same_content(&before));
                }
            }
            if action == 1 {
                editor.undo().unwrap();
                editor.document().validate().unwrap();
                assert_consistent(editor.document());
            }
        }
        while editor.undo().unwrap().is_some() {}
        prop_assert!(editor.document().same_content(&original));
    }
}

/// The ties of a chain entered by a command survive a do–undo–redo with the backward index
/// rebuilt identically (a regression guard for the index's sort order).
#[test]
fn the_tie_index_survives_undo() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    execute_checked(
        &mut editor,
        &note(
            i.lh,
            i.bars[3],
            d(1, 4),
            NoteValue::new(BaseValue::Whole, 2).unwrap(),
            &[pitch(Step::C), pitch(Step::E), pitch(Step::G)],
        ),
    );
    let doc = editor.document();
    assert!(
        doc.voice(i.lh)
            .unwrap()
            .1
            .events_at(BarIndex::new(4))
            .iter()
            .all(|p| matches!(p.event().body(), EventBody::Chord(_)))
    );
    let _ = i.fl_staff;
}

/// Deleting the first bars in the middle of a gradual tempo change: the change cannot start the
/// score (ADR-0021), so the score starts at the tempo it **arrives at**, keeping the initial
/// mark's text, and the change is deleted (W4, owner's choice of 2026-11-04).
#[test]
fn deleting_the_start_of_a_gradual_change_starts_at_its_target() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    // Stretch the accelerando of bar 5 (to ♩ = 132) so that it ends at the start of bar 7.
    let mark = editor.document().tempo_marks().as_slice()[1].clone();
    let Some(TempoMarkChange::Gradual(mut gradual)) = mark.value().change() else {
        panic!("the rich document's second tempo mark is gradual");
    };
    gradual.end = crate::marks::ScorePoint::start_of(i.bars[7]);
    editor
        .transact(|tx| {
            tx.apply(crate::edit::Edit::Replace(crate::edit::Object::TempoMark(
                crate::marks::Mark::new(
                    mark.id(),
                    mark.bar(),
                    mark.offset(),
                    TempoMark::new(mark.value().text(), Some(TempoMarkChange::Gradual(gradual)))
                        .unwrap(),
                ),
            )))
        })
        .unwrap();
    editor.document().validate().unwrap();
    // Bars 0–5 go: the accelerando started in bar 5 and goes on to bar 7.
    execute_checked(
        &mut editor,
        &Command::DeleteBars {
            at: BarIndex::FIRST,
            count: 6,
        },
    );
    let doc = editor.document();
    let (_, initial) = doc.initial_tempo();
    assert_eq!(initial.text(), Some("Allegro"));
    let Some(TempoMarkChange::Immediate(m)) = initial.change() else {
        panic!("the initial tempo is always immediate");
    };
    assert_eq!(
        m.tempo,
        Tempo::new(d(1, 4), Rational::from_integer(132).unwrap()).unwrap()
    );
    assert_eq!(doc.tempo_marks().len(), 0, "the accelerando is gone");
}
