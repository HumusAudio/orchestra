// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the commands and of the gestures (ADR-0026 §4, §6, §8; SCALETTA F1.11b, parts 2a
//! and 2b). The hand-written cases of part 2b are in `crate::mark_command_tests`; the property
//! test here generates the commands of both parts.
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
    BarIndex, BarKind, BeatGrouping, Duration, IrregularBar, Meter, MeterChange, MeterMap,
    Rational, Tempo,
};
use proptest::prelude::*;

use crate::bar_repeat::BarRepeatSpan;
use crate::command::{Command, EventChange, NewBody, NewEvent};
use crate::document::{Document, DocumentBuilder, DocumentError, Placement};
use crate::dynamic::{DynamicLevel, DynamicMark, DynamicPlacement, HairpinShape};
use crate::edit::{Initial, ObjectRef};
use crate::editor::{CommandError, Editor};
use crate::editor_tests::{assert_consistent, rich};
use crate::event::{BaseValue, EventBody, NoteValue, RestValue, Tie, Tuplet};
use crate::form::PassSet;
use crate::id::{BarId, EventId, NoteId, StaffId, TupletId, VoiceId};
use crate::jump::{JumpSpec, MarkerKind};
use crate::mark_command::{MarkSpec, NewVolta};
use crate::marking::{Fermata, Grace, GraceAttach, GraceKind};
use crate::marks::{Clef, KeyOverride, KeySignature, ScorePoint};
use crate::meter_command::MeterMode;
use crate::pitch::{Alteration, Interval, Key, Mode, Pitch, Step};
use crate::slur::SlurPlacement;
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

/// Past the last bar (W3), a tuplet note whose part before the barline no value can write (Z3), at
/// the end of a regular bar, and a remainder no value can write: all refused with their typed
/// errors.
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
            // A triplet note with 1/36 left in the bar: 1/36 ÷ 2/3 = 1/24, no written value (Z3).
            Command::EnterEvent(NewEvent {
                at: Placement::new(i.rh, i.bars[1], d(35, 36)).in_tuplet(triplet),
                body: NewBody::Chord {
                    value: value(BaseValue::Eighth),
                    pitches: vec![pitch(Step::C)],
                    grace: None,
                },
            }),
            DocumentError::NotWritable,
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

/// A triplet eighth entered 1/24 before the end of bar 1 crosses the barline: it becomes two
/// triplet sixteenths tied, both in the triplet (Z3, Z4; 1/24 ÷ 2/3 = 1/16 on each side).
#[test]
fn a_tuplet_note_across_a_barline_stays_in_its_tuplet() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let triplet = editor.document().events(i.rh, i.bars[1]).unwrap()[0]
        .event()
        .tuplet()
        .unwrap();
    execute_checked(
        &mut editor,
        &Command::EnterEvent(NewEvent {
            at: Placement::new(i.rh, i.bars[1], d(23, 24)).in_tuplet(triplet),
            body: NewBody::Chord {
                value: value(BaseValue::Eighth),
                pitches: vec![pitch(Step::C)],
                grace: None,
            },
        }),
    );
    let doc = editor.document();
    let first = doc
        .events(i.rh, i.bars[1])
        .unwrap()
        .last()
        .unwrap()
        .event()
        .clone();
    let second = doc.events(i.rh, i.bars[2]).unwrap()[0].event().clone();
    for part in [&first, &second] {
        assert_eq!(part.tuplet(), Some(triplet));
        assert_eq!(part.chord().unwrap().value(), value(BaseValue::Sixteenth));
    }
    let tie = first.chord().unwrap().notes().next().unwrap().tie();
    assert_eq!(
        tie,
        Tie::To(second.chord().unwrap().notes().next().unwrap().id())
    );
    assert_eq!(doc.metric_duration(second.id()).unwrap(), d(1, 24));
}

/// A triplet half (1/3) entered 5/24 before the end of bar 1: the part before the barline is 5/16
/// in the triplet's written values, a quarter and a sixteenth, at offsets 19/24 and 23/24 (the
/// quarter lasts 1/4 × 2/3 = 1/6 in the bar), ending exactly at the barline; the rest, 1/8 in the
/// bar, is a dotted eighth of the triplet (Z3).
#[test]
fn a_tuplet_part_of_several_values_is_spaced_by_the_ratio() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let triplet = editor.document().events(i.rh, i.bars[1]).unwrap()[0]
        .event()
        .tuplet()
        .unwrap();
    execute_checked(
        &mut editor,
        &Command::EnterEvent(NewEvent {
            at: Placement::new(i.rh, i.bars[1], d(19, 24)).in_tuplet(triplet),
            body: NewBody::Rest(RestValue::Value(value(BaseValue::Half))),
        }),
    );
    let doc = editor.document();
    let parts: Vec<(Duration, Duration)> = doc
        .events(i.rh, i.bars[1])
        .unwrap()
        .iter()
        .skip(3)
        .map(|p| (p.offset(), doc.metric_duration(p.event().id()).unwrap()))
        .collect();
    assert_eq!(parts, [(d(19, 24), d(1, 6)), (d(23, 24), d(1, 24))]);
    let next = &doc.events(i.rh, i.bars[2]).unwrap()[0];
    assert_eq!(
        (
            next.offset(),
            doc.metric_duration(next.event().id()).unwrap()
        ),
        (Duration::ZERO, d(1, 8))
    );
}

/// Inside a triplet nested in a quintuplet (ratio 2/3 × 4/5 = 8/15), a triplet eighth (1/15) 1/30
/// before the barline becomes two sixteenths of the inner triplet, tied (Z3: 1/30 ÷ 8/15 = 1/16).
#[test]
fn a_nested_tuplet_note_is_split_in_its_own_values() {
    let mut editor = Editor::new(rich());
    let i = ids(editor.document());
    let sixteenth = value(BaseValue::Sixteenth);
    let eighth = value(BaseValue::Eighth);
    let changes = execute_checked(
        &mut editor,
        &Command::AddTuplet {
            voice: i.fl,
            tuplet: Tuplet::new(5, 4, sixteenth).unwrap(),
            parent: None,
        },
    );
    let ObjectRef::Tuplet(outer) = changes.inserted()[0] else {
        panic!("the inserted object is a tuplet");
    };
    let changes = execute_checked(
        &mut editor,
        &Command::AddTuplet {
            voice: i.fl,
            tuplet: Tuplet::new(3, 2, eighth).unwrap(),
            parent: Some(outer),
        },
    );
    let ObjectRef::Tuplet(inner) = changes.inserted()[0] else {
        panic!("the inserted object is a tuplet");
    };
    execute_checked(
        &mut editor,
        &Command::EnterEvent(NewEvent {
            at: Placement::new(i.fl, i.bars[4], d(29, 30)).in_tuplet(inner),
            body: NewBody::Rest(RestValue::Value(eighth)),
        }),
    );
    let doc = editor.document();
    let first = doc
        .events(i.fl, i.bars[4])
        .unwrap()
        .last()
        .unwrap()
        .event()
        .clone();
    let second = doc.events(i.fl, i.bars[5]).unwrap()[0].event().clone();
    for part in [&first, &second] {
        assert_eq!(part.tuplet(), Some(inner));
        assert_eq!(doc.metric_duration(part.id()).unwrap(), d(1, 30));
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
    if op % OPS >= 21 {
        return random_meter_command(doc, op % OPS, a, b);
    }
    if op % OPS >= 9 {
        return random_mark_command(doc, op % OPS, a, b);
    }
    Some(match op % OPS {
        0 => note(voice, bar, offset, v, &[pitch(steps[b % 5])]),
        1 => {
            // A note inside one of the voice's tuplets (if any), often across a barline (Z4).
            let mut tuplets: Vec<TupletId> = doc
                .voice(voice)?
                .1
                .content
                .tuplets
                .keys()
                .copied()
                .collect();
            tuplets.sort_unstable();
            let offset = d(i64::try_from(a % 12).ok()?, 12);
            let mut at = Placement::new(voice, bar, offset);
            if let Some(&t) = tuplets.get(b % tuplets.len().max(1)) {
                at = at.in_tuplet(t);
            }
            Command::EnterEvent(NewEvent {
                at,
                body: NewBody::Chord {
                    value: v,
                    pitches: vec![pitch(steps[b % 5])],
                    grace: None,
                },
            })
        }
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

/// Shorthand: a tempo of `bpm` quarters per minute.
fn tempo(bpm: i64) -> Tempo {
    Tempo::new(d(1, 4), Rational::from_integer(bpm).unwrap()).unwrap()
}

/// How many kinds of random command there are: 9 on events and bars (part 2a), 12 on marks,
/// form markings, tuplets and per-bar data (part 2b), 2 on bar lengths (F1.11c).
const OPS: u8 = 23;

/// Every mark and form marking of the document, in a stable order: what a random
/// [`Command::ChangeMark`] or [`Command::RemoveMark`] picks from.
fn all_marks(doc: &Document) -> Vec<ObjectRef> {
    let mut marks = Vec::new();
    for instrument in doc.instruments() {
        marks.extend(instrument.slurs().iter().map(|x| ObjectRef::Slur(x.id())));
        marks.extend(
            instrument
                .dynamics()
                .iter()
                .map(|x| ObjectRef::Dynamic(x.id())),
        );
        marks.extend(
            instrument
                .hairpins()
                .iter()
                .map(|x| ObjectRef::Hairpin(x.id())),
        );
        marks.extend(
            instrument
                .pass_rules()
                .iter()
                .map(|x| ObjectRef::PassRule(x.id())),
        );
        marks.extend(
            instrument
                .bar_repeats()
                .iter()
                .map(|x| ObjectRef::BarRepeat(x.id())),
        );
        marks.extend(
            instrument
                .key_overrides()
                .as_slice()
                .iter()
                .map(|x| ObjectRef::KeyOverride(x.id())),
        );
        for staff in instrument.staves() {
            marks.extend(
                staff
                    .clef_changes()
                    .as_slice()
                    .iter()
                    .map(|x| ObjectRef::ClefChange(x.id())),
            );
        }
    }
    marks.extend(
        doc.tempo_marks()
            .as_slice()
            .iter()
            .map(|x| ObjectRef::TempoMark(x.id())),
    );
    marks.extend(
        doc.key_changes()
            .as_slice()
            .iter()
            .map(|x| ObjectRef::KeyChange(x.id())),
    );
    let form = doc.form();
    marks.extend(
        form.repeat_starts()
            .iter()
            .map(|x| ObjectRef::RepeatStart(x.id())),
    );
    marks.extend(
        form.repeat_ends()
            .iter()
            .map(|x| ObjectRef::RepeatEnd(x.id())),
    );
    marks.extend(form.voltas().iter().map(|x| ObjectRef::Volta(x.id())));
    marks.extend(form.markers().iter().map(|x| ObjectRef::Marker(x.id())));
    marks.extend(form.jumps().iter().map(|x| ObjectRef::Jump(x.id())));
    marks
}

/// One random change of bar lengths of F1.11c (`op` 21 or 22), in either mode: a meter change
/// (often refused inside the bar repeat, or by a pickup) or one bar made irregular, a pickup, free
/// or regular again.
fn random_meter_command(doc: &Document, op: u8, a: usize, b: usize) -> Option<Command> {
    let bars = doc.bars().ids();
    let bar = *bars.get(a % bars.len())?;
    let length = Duration::from_rational(Rational::new(i64::try_from(1 + b % 6).ok()?, 4).ok()?);
    // Half the time the music is rewritten (part 2b), half the bars keep their contents (part 1).
    let mode = if (a / 3).is_multiple_of(2) {
        MeterMode::Rewrite
    } else {
        MeterMode::KeepBars
    };
    Some(if op == 21 {
        let meters = [(2, 4), (3, 4), (4, 4), (6, 8), (5, 8)];
        let change = (b % 6 != 5).then(|| {
            let (n, den) = meters[b % 5];
            MeterChange::new(Meter::new(&[n], den).unwrap())
        });
        Command::ChangeMeter { bar, change, mode }
    } else {
        let kinds = [BarKind::AlignStart, BarKind::AlignEnd, BarKind::Free];
        Command::SetIrregularBar {
            bar,
            irregular: (a % 4 != 3).then(|| IrregularBar {
                length,
                kind: kinds[a % 3],
            }),
            mode,
        }
    })
}

/// One random command of part 2b (`op` from 9 to 20), built from the document as it is.
#[allow(
    clippy::too_many_lines,
    reason = "one short arm per kind of command, as in `random_command`"
)]
fn random_mark_command(doc: &Document, op: u8, a: usize, b: usize) -> Option<Command> {
    let bars = doc.bars().ids();
    let bar = *bars.get(a % bars.len())?;
    let later = *bars.get((a + 1 + b % 2) % bars.len())?;
    let offset = d(i64::try_from(b % 4).ok()?, 4);
    let point = ScorePoint::new(bar, offset);
    let instruments: Vec<_> = doc.instruments().collect();
    let instrument = instruments.get(b % instruments.len())?;
    let staff = instrument.staves().get(a % instrument.staves().len())?.id();
    let voice = instrument.voices().get(a % instrument.voices().len())?.id();
    let placement = if b.is_multiple_of(3) {
        DynamicPlacement::voice(voice, staff)
    } else {
        DynamicPlacement::instrument(staff)
    };
    let levels = [DynamicLevel::P, DynamicLevel::Mf, DynamicLevel::F];
    let events: Vec<EventId> = instrument
        .voices()
        .iter()
        .flat_map(|v| {
            (0..bars.len())
                .flat_map(move |i| v.events_at(BarIndex::new(u32::try_from(i).unwrap_or(0))))
        })
        .map(|p| p.event().id())
        .collect();
    let marks = all_marks(doc);
    let tuplets: Vec<TupletId> = {
        let mut t: Vec<TupletId> = instrument
            .voices()
            .iter()
            .flat_map(|v| v.content.tuplets.keys().copied())
            .collect();
        t.sort_unstable();
        t
    };
    let eighth = NoteValue::plain(BaseValue::Eighth);
    Some(match op {
        9 => Command::AddMark(MarkSpec::Dynamic {
            instrument: instrument.id(),
            at: point,
            placement,
            mark: DynamicMark::Level(levels[a % 3]),
        }),
        10 => Command::AddMark(MarkSpec::Hairpin {
            instrument: instrument.id(),
            start: point,
            end: ScorePoint::new(later, d(i64::try_from(a % 4).ok()?, 4)),
            placement,
            shape: if a.is_multiple_of(2) {
                HairpinShape::CRESCENDO
            } else {
                HairpinShape::DIMINUENDO
            },
        }),
        11 => {
            // Moves an existing mark: a dynamic or a tempo mark to the point, a hairpin's end or
            // a slur's end elsewhere.
            let name = *marks.get(a % marks.len().max(1))?;
            let to = match name {
                ObjectRef::Dynamic(id) => {
                    let (owner, dy) = doc.instruments().find_map(|i| {
                        i.dynamics()
                            .iter()
                            .find(|x| x.id() == id)
                            .map(|x| (i.id(), x))
                    })?;
                    MarkSpec::Dynamic {
                        instrument: owner,
                        at: point,
                        placement: dy.placement(),
                        mark: dy.mark().clone(),
                    }
                }
                ObjectRef::Hairpin(id) => {
                    let (owner, h) = doc.instruments().find_map(|i| {
                        i.hairpins()
                            .iter()
                            .find(|x| x.id() == id)
                            .map(|x| (i.id(), *x))
                    })?;
                    MarkSpec::Hairpin {
                        instrument: owner,
                        start: h.start(),
                        end: ScorePoint::new(later, offset),
                        placement: h.placement(),
                        shape: h.shape(),
                    }
                }
                ObjectRef::TempoMark(id) => {
                    let m = doc.tempo_marks().as_slice().iter().find(|m| m.id() == id)?;
                    MarkSpec::Tempo {
                        at: point,
                        mark: m.value().clone(),
                    }
                }
                ObjectRef::Slur(id) => {
                    let (_, slur) = doc.slur(id)?;
                    MarkSpec::Slur {
                        start: slur.start(),
                        end: *events.get(b % events.len().max(1))?,
                        placement: slur.placement(),
                    }
                }
                ObjectRef::Marker(id) => {
                    let m = doc.marker(id)?;
                    MarkSpec::Marker {
                        bar,
                        kind: m.kind(),
                        text: None,
                    }
                }
                // Another kind with a dynamic's content: refused with `MarkKindMismatch`.
                _ => MarkSpec::Dynamic {
                    instrument: instrument.id(),
                    at: point,
                    placement,
                    mark: DynamicMark::Level(DynamicLevel::P),
                },
            };
            Command::ChangeMark { mark: name, to }
        }
        12 => Command::RemoveMark(*marks.get(b % marks.len().max(1))?),
        13 => Command::AddMark(MarkSpec::Slur {
            start: *events.get(a % events.len().max(1))?,
            end: *events.get(b % events.len().max(1))?,
            placement: SlurPlacement::Auto,
        }),
        14 => Command::AddMark(MarkSpec::Tempo {
            at: point,
            mark: if b.is_multiple_of(2) {
                TempoMark::immediate(
                    None,
                    Metronome::shown(tempo(60 + 10 * i64::try_from(a % 8).ok()?)),
                )
                .ok()?
            } else {
                TempoMark::new(
                    Some("rit."),
                    Some(TempoMarkChange::Gradual(crate::tempo_mark::GradualMark {
                        from: None,
                        to: tempo(70),
                        end: ScorePoint::start_of(later),
                    })),
                )
                .ok()?
            },
        }),
        15 => Command::AddRepeat {
            start: bar,
            end: later,
            times: u8::try_from(2 + b % 2).ok()?,
        },
        16 => {
            let second = *bars.get((a + 1) % bars.len())?;
            Command::AddVoltas(vec![
                NewVolta {
                    first: bar,
                    last: bar,
                    passes: PassSet::of(&[1]).ok()?,
                    closed: true,
                    text: None,
                },
                NewVolta {
                    first: second,
                    last: second,
                    passes: PassSet::of(&[2]).ok()?,
                    closed: false,
                    text: None,
                },
            ])
        }
        17 => match b % 3 {
            0 => Command::AddMark(MarkSpec::Marker {
                bar,
                kind: [MarkerKind::Segno, MarkerKind::Coda, MarkerKind::Fine][a % 3],
                text: None,
            }),
            1 => Command::AddMark(MarkSpec::Jump {
                bar,
                spec: JumpSpec::da_capo(),
                text: None,
            }),
            _ => Command::AddMark(MarkSpec::BarRepeat {
                staff,
                first: bar,
                count: 1,
                span: BarRepeatSpan::One,
            }),
        },
        18 => match b % 3 {
            0 => Command::AddTuplet {
                voice,
                tuplet: Tuplet::new(3, 2, eighth).ok()?,
                parent: tuplets
                    .get(a % tuplets.len().max(1))
                    .copied()
                    .filter(|_| a.is_multiple_of(2)),
            },
            1 => Command::ChangeTuplet {
                tuplet: *tuplets.get(a % tuplets.len().max(1))?,
                to: Tuplet::new(u8::try_from(2 + a % 4).ok()?, 2, eighth).ok()?,
            },
            _ => Command::RemoveTuplet(*tuplets.get(a % tuplets.len().max(1))?),
        },
        19 => match b % 3 {
            0 => Command::AddMark(MarkSpec::Clef {
                staff,
                at: point,
                clef: Clef::BASS,
            }),
            1 => Command::AddMark(MarkSpec::Key {
                at: point,
                key: Key::traditional(i8::try_from(a % 5).ok()?, Mode::Major).ok()?,
            }),
            _ => Command::AddMark(MarkSpec::KeyOverride {
                instrument: instrument.id(),
                at: point,
                key: KeyOverride::Signature(KeySignature::NoSignature),
            }),
        },
        _ => match b % 4 {
            0 => Command::SetBarGrouping {
                bar,
                grouping: (a.is_multiple_of(2))
                    .then(|| BeatGrouping::new(&[3, 3, 2], 8).ok())
                    .flatten(),
            },
            1 => Command::SetUncounted {
                bar,
                uncounted: a.is_multiple_of(2),
            },
            2 => Command::SetNumberingRestart {
                bar,
                number: (a.is_multiple_of(2)).then_some(u32::try_from(a).ok()?),
            },
            _ => Command::SetInitial(Initial::Key(
                Key::traditional(i8::try_from(a % 3).ok()?, Mode::Minor).ok()?,
            )),
        },
    })
}

/// The errors a random command may meet: refusals of the commands, never a bug of the model.
fn expected_refusal(error: CommandError) -> bool {
    !matches!(
        error,
        CommandError::Rollback { .. }
            | CommandError::Document(
                DocumentError::Inconsistent { .. } | DocumentError::BarsNotEmpty(_)
            )
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Random commands on the rich document, with undos and redos: valid after every command,
    /// unchanged after a refusal, back to the original after undoing everything.
    #[test]
    fn commands_keep_the_document_valid(
        steps in prop::collection::vec((0_u8..OPS, 0_usize..64, 0_usize..64, 0_u8..4), 1..24),
        without_bar_repeat in any::<bool>(),
    ) {
        let mut editor = Editor::new(rich());
        // Half the cases start without the flute's bar repeat, which refuses every rewrite that
        // reaches it (R10), so that the rewrites are often carried out.
        if without_bar_repeat {
            let repeat = editor.document().instruments().nth(1).unwrap().bar_repeats()[0].id();
            let document = {
                let mut first = Editor::new(editor.document().clone());
                first.execute(&Command::RemoveMark(ObjectRef::BarRepeat(repeat))).unwrap();
                first.into_document()
            };
            editor = Editor::new(document);
        }
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
    gradual.end = ScorePoint::start_of(i.bars[7]);
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
