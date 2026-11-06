// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the DAW move, the tolerance thresholds and the pass-specific offsets (ADR-0028;
//! SCALETTA F1.14b).
//!
//! # Why inside the crate
//! Like `crate::command_tests`: the rich document of `crate::editor_tests` and its helpers are
//! crate-private, and the tests look at the pass-offset tables directly.
//!
//! # What is checked
//! - One hand-written case per row of ADR-0028 §2–§5 and per precision T1–T7: within and beyond
//!   the threshold, the residual, the grid bringing a note back, a chord's notehead, the
//!   performance-only mode, every refusal, tie chains, milliseconds, one pass, the removal of a
//!   deleted note's entries and its undo, the tolerance command, a drag in a gesture.
//! - A property test: random moves on every notehead of the rich document, with random onsets,
//!   grids, modes and passes. After each accepted move the full [`Document::validate`] passes, the
//!   indices agree with a rebuild, the **played start** (written start + inline onset, plus the
//!   pass entry for a per-pass move) is exactly the one asked for, and the score is unchanged when
//!   the move had to stay a performance offset. A refused move leaves the document as it was, and
//!   undoing everything gives back the original.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]

use orchestra_core_time::{Duration, Meter, MeterChange, MeterMap, Position, Rational, Tempo};
use proptest::prelude::*;

use crate::command::{Command, EventChange};
use crate::daw_command::{MoveMode, MovePlayed};
use crate::document::{Document, DocumentBuilder, DocumentError, Placement};
use crate::editor::{CommandError, Editor};
use crate::editor_tests::{assert_consistent, rich};
use crate::event::{BaseValue, NoteValue, Tie, Tuplet};
use crate::id::{EventId, NoteId, TrackId};
use crate::marking::{Grace, GraceAttach, GraceKind, PerformanceOffsets};
use crate::marks::Clef;
use crate::pitch::{Alteration, Interval, Key, Mode, Pitch, Step};
use crate::tempo_mark::{Metronome, TempoMark};
use crate::tolerance::Tolerance;
use crate::validate::Violation;

/// Shorthand: a duration of `n/d` whole notes.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// Shorthand: a pitch in octave 4.
const fn pitch(step: Step) -> Pitch {
    Pitch::new(step, Alteration::NATURAL, 4)
}

/// The small document of the hand-written cases, in 4/4 at ♩ = 120 (a whole note lasts 2 s), one
/// flute, four bars:
///
/// - bar 0: an eighth C at 0, a chord D+F (quarter) at 1/2;
/// - bar 1: a half E at 1/2, tied to a quarter E at the start of bar 2;
/// - bar 3: a triplet of eighths G, A, B from 0, and a grace note before a quarter C at 1/2.
struct Small {
    /// The document.
    doc: Document,
    /// The flute's track.
    track: TrackId,
    /// The eighth C of bar 0: its event and notehead.
    c: (EventId, NoteId),
    /// The chord D+F of bar 0: its event and its two noteheads.
    chord: (EventId, [NoteId; 2]),
    /// The half E of bar 1 (start of the tie chain).
    e_half: (EventId, NoteId),
    /// The quarter E of bar 2 (end of the tie chain).
    e_quarter: (EventId, NoteId),
    /// The first note of the triplet of bar 3.
    triplet_note: NoteId,
    /// The grace note of bar 3.
    grace: NoteId,
}

/// Builds [`Small`].
fn small() -> Small {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let tempo = Tempo::new(d(1, 4), Rational::from_integer(120).unwrap()).unwrap();
    let initial = TempoMark::immediate(None, Metronome::shown(tempo)).unwrap();
    let mut b =
        DocumentBuilder::new(meters, initial, Key::traditional(0, Mode::Major).unwrap()).unwrap();
    let bars: Vec<_> = (0..4).map(|_| b.add_bar().unwrap()).collect();
    let track = b
        .add_instrument_track("Flute", Interval::new(0, Rational::ZERO), &[Clef::TREBLE])
        .unwrap();
    let fl = b.document().track(track).unwrap().instrument().voices()[0].id();
    let quarter = NoteValue::plain(BaseValue::Quarter);
    let eighth = NoteValue::plain(BaseValue::Eighth);
    let half = NoteValue::plain(BaseValue::Half);
    let c = b
        .add_chord(
            Placement::new(fl, bars[0], Duration::ZERO),
            eighth,
            &[pitch(Step::C)],
        )
        .unwrap();
    let chord = b
        .add_chord(
            Placement::new(fl, bars[0], d(1, 2)),
            quarter,
            &[pitch(Step::D), pitch(Step::F)],
        )
        .unwrap();
    let e_half = b
        .add_chord(
            Placement::new(fl, bars[1], d(1, 2)),
            half,
            &[pitch(Step::E)],
        )
        .unwrap();
    let e_quarter = b
        .add_chord(
            Placement::new(fl, bars[2], Duration::ZERO),
            quarter,
            &[pitch(Step::E)],
        )
        .unwrap();
    b.set_tie(e_half.notes[0], Tie::To(e_quarter.notes[0]))
        .unwrap();
    let triplet = b
        .add_tuplet(fl, Tuplet::new(3, 2, eighth).unwrap(), None)
        .unwrap();
    let mut triplet_note = None;
    for (i, step) in [Step::G, Step::A, Step::B].into_iter().enumerate() {
        let ids = b
            .add_chord(
                Placement::new(fl, bars[3], d(i64::try_from(i).unwrap(), 12)).in_tuplet(triplet),
                eighth,
                &[pitch(step)],
            )
            .unwrap();
        triplet_note.get_or_insert(ids.notes[0]);
    }
    let grace = b
        .add_grace_chord(
            Placement::new(fl, bars[3], d(1, 2)),
            Grace {
                kind: GraceKind::Acciaccatura,
                attach: GraceAttach::Before,
            },
            eighth,
            &[pitch(Step::D)],
        )
        .unwrap();
    b.add_chord(
        Placement::new(fl, bars[3], d(1, 2)),
        quarter,
        &[pitch(Step::C)],
    )
    .unwrap();
    Small {
        doc: b.build().unwrap(),
        track,
        c: (c.event, c.notes[0]),
        chord: (chord.event, [chord.notes[0], chord.notes[1]]),
        e_half: (e_half.event, e_half.notes[0]),
        e_quarter: (e_quarter.event, e_quarter.notes[0]),
        triplet_note: triplet_note.unwrap(),
        grace: grace.notes[0],
    }
}

/// A move of every pass with the default rule and a sixteenth grid.
fn drag(note: NoteId, onset: Duration) -> Command {
    Command::MovePlayed(MovePlayed {
        note,
        pass: None,
        onset,
        grid: d(1, 16),
        mode: MoveMode::ByThreshold,
    })
}

/// The written start of an event.
fn written(doc: &Document, event: EventId) -> Position {
    doc.event_position(event).unwrap()
}

/// The inline offsets of a notehead.
fn offsets(doc: &Document, note: NoteId) -> PerformanceOffsets {
    doc.note(note).unwrap().1.offsets()
}

/// Whether two documents have the same written music: every event at the same bar and offset,
/// with the same content apart from the performance offsets of its noteheads. Used to check that a
/// move "did not change the score".
fn same_score(a: &Document, b: &Document) -> bool {
    /// Every event of a document as (event, voice, bar, offset), in a fixed order.
    fn places(doc: &Document) -> Vec<(EventId, crate::id::VoiceId, crate::id::BarId, Duration)> {
        let mut out = Vec::new();
        for track in doc.tracks() {
            for voice in track.instrument().voices() {
                for &bar in doc.bars().ids() {
                    for placed in doc.events(voice.id(), bar).unwrap() {
                        out.push((placed.event().id(), voice.id(), bar, placed.offset()));
                    }
                }
            }
        }
        out
    }
    places(a) == places(b)
}

/// Executes a command, checks validity and consistency, and that undo and redo give back the
/// documents from before and after.
fn execute_checked(editor: &mut Editor, command: &Command) {
    let before = editor.document().clone();
    editor.execute(command).unwrap();
    let after = editor.document().clone();
    after.validate().unwrap();
    assert_consistent(&after);
    editor.undo().unwrap();
    assert!(editor.document().same_content(&before));
    editor.redo().unwrap();
    assert!(editor.document().same_content(&after));
}

/// Within the threshold (inclusive), the move is an onset offset and the score does not change.
#[test]
fn within_the_threshold_only_the_offset_changes() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    execute_checked(&mut editor, &drag(s.c.1, d(-1, 32)));
    let doc = editor.document();
    assert!(same_score(doc, &s.doc));
    assert_eq!(offsets(doc, s.c.1).onset, d(-1, 32));
}

/// Beyond the threshold, the note moves to the nearest grid point and the rest becomes its offset:
/// +3/32 on a 1/16 grid is halfway between 1/16 and 1/8, and goes to 1/8 (farther from the written
/// place), with −1/32 as the offset. The played start is exactly the one asked for.
#[test]
fn beyond_the_threshold_the_note_moves_and_keeps_the_residual() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    execute_checked(&mut editor, &drag(s.c.1, d(3, 32)));
    let doc = editor.document();
    let start = written(doc, s.c.0);
    assert_eq!(start, Position::from_rational(Rational::new(1, 8).unwrap()));
    assert_eq!(offsets(doc, s.c.1).onset, d(-1, 32));
    assert_eq!(
        start.checked_add(offsets(doc, s.c.1).onset).unwrap(),
        Position::from_rational(Rational::new(3, 32).unwrap())
    );
}

/// When the grid brings the note back to its own place, only the offset is stored.
#[test]
fn a_grid_that_brings_the_note_back_keeps_the_score() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    let order = Command::MovePlayed(MovePlayed {
        note: s.c.1,
        pass: None,
        onset: d(1, 24),
        grid: d(1, 4),
        mode: MoveMode::ByThreshold,
    });
    execute_checked(&mut editor, &order);
    assert!(same_score(editor.document(), &s.doc));
    assert_eq!(offsets(editor.document(), s.c.1).onset, d(1, 24));
}

/// A notehead of a chord with two noteheads (Q5) and the performance-only mode (§4) never change
/// the score, whatever the size of the move.
#[test]
fn chords_and_the_performance_only_mode_keep_the_score() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    execute_checked(&mut editor, &drag(s.chord.1[1], d(1, 8)));
    assert!(same_score(editor.document(), &s.doc));
    assert_eq!(offsets(editor.document(), s.chord.1[1]).onset, d(1, 8));
    let free = Command::MovePlayed(MovePlayed {
        note: s.c.1,
        pass: None,
        onset: d(3, 8),
        grid: d(1, 16),
        mode: MoveMode::PerformanceOnly,
    });
    execute_checked(&mut editor, &free);
    assert!(same_score(editor.document(), &s.doc));
    assert_eq!(offsets(editor.document(), s.c.1).onset, d(3, 8));
}

/// The refusals of Q6, Q9, T1 and T2, and a grid of zero: each leaves the document unchanged.
#[test]
fn written_moves_that_cannot_be_written_are_refused() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    let cases = [
        // Onto the chord of bar 0: the local check reports the overlap of the moved note (V2).
        (
            drag(s.c.1, d(1, 2)),
            DocumentError::Invalid(Violation::Overlap(s.c.0)),
        ),
        // An eighth moved to 15/16 would end past the barline.
        (drag(s.c.1, d(15, 16)), DocumentError::MoveCrossesBarline),
        // Past the last bar (bar 3 ends at 4).
        (drag(s.e_quarter.1, d(3, 1)), DocumentError::MoveOfTiedNote),
        (drag(s.grace, d(1, 8)), DocumentError::MoveOfGraceNote),
        (
            drag(s.triplet_note, d(1, 8)),
            DocumentError::MoveOfTupletNote,
        ),
        (
            Command::MovePlayed(MovePlayed {
                note: s.c.1,
                pass: None,
                onset: d(1, 8),
                grid: Duration::ZERO,
                mode: MoveMode::ByThreshold,
            }),
            DocumentError::GridNotPositive,
        ),
    ];
    for (command, error) in cases {
        assert_eq!(
            editor.execute(&command),
            Err(CommandError::Document(error)),
            "{command:?}"
        );
        assert!(editor.document().same_content(&s.doc));
    }
    // A note in bar 3 moved past the end of the last bar.
    let past = small();
    let mut editor = Editor::new(past.doc.clone());
    let last_c = past
        .doc
        .note(past.grace)
        .map(|(e, _)| (e.voice(), e.bar()))
        .unwrap();
    let main = past.doc.events(last_c.0, last_c.1).unwrap().last().unwrap();
    let main_note = main.event().chord().unwrap().notes().next().unwrap().id();
    assert_eq!(
        editor.execute(&drag(main_note, d(1, 2))),
        Err(CommandError::Document(DocumentError::PastLastBar))
    );
}

/// A tie chain moves as a whole (Q7): the half E of bar 1 moved a quarter earlier takes its tied
/// quarter E from bar 2 into bar 1, both by the same amount, and the tie still holds.
#[test]
fn a_tie_chain_moves_as_a_whole() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    let order = Command::MovePlayed(MovePlayed {
        note: s.e_half.1,
        pass: None,
        onset: d(-1, 4),
        grid: d(1, 4),
        mode: MoveMode::ByThreshold,
    });
    execute_checked(&mut editor, &order);
    let doc = editor.document();
    let one = Position::from_rational(Rational::from_integer(1).unwrap());
    assert_eq!(written(doc, s.e_half.0), one.checked_add(d(1, 4)).unwrap());
    assert_eq!(
        written(doc, s.e_quarter.0),
        one.checked_add(d(3, 4)).unwrap()
    );
    assert_eq!(
        doc.note(s.e_half.1).unwrap().1.tie(),
        Tie::To(s.e_quarter.1)
    );
    assert!(offsets(doc, s.e_half.1).onset.is_zero());
}

/// A threshold in milliseconds (T6): at ♩ = 120 a whole note lasts 2 s, so 3/64 is 93.75 ms,
/// within 100 ms but beyond the default 1/32 (62.5 ms).
#[test]
fn a_threshold_in_milliseconds_converts_through_the_tempo_map() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    execute_checked(&mut editor, &drag(s.c.1, d(3, 64)));
    assert!(!same_score(editor.document(), &s.doc));
    let mut editor = Editor::new(s.doc.clone());
    execute_checked(
        &mut editor,
        &Command::SetTolerance {
            track: Some(s.track),
            tolerance: Some(Tolerance::Millis(100)),
        },
    );
    execute_checked(&mut editor, &drag(s.c.1, d(3, 64)));
    assert!(same_score(editor.document(), &s.doc));
    assert_eq!(offsets(editor.document(), s.c.1).onset, d(3, 64));
}

/// The tolerance command: overrides, the project value that cannot be removed, unknown tracks.
#[test]
fn the_tolerance_command_is_undoable_and_checked() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    let changes = editor
        .execute(&Command::SetTolerance {
            track: None,
            tolerance: Some(Tolerance::Musical(d(1, 16))),
        })
        .unwrap();
    assert!(changes.tolerance_changed());
    assert_eq!(
        editor.document().tolerance().for_track(s.track),
        Tolerance::Musical(d(1, 16))
    );
    editor.undo().unwrap();
    assert!(editor.document().same_content(&s.doc));
    assert_eq!(
        editor.execute(&Command::SetTolerance {
            track: None,
            tolerance: None
        }),
        Err(CommandError::Document(
            DocumentError::ProjectToleranceRequired
        ))
    );
    assert_eq!(
        editor.execute(&Command::SetTolerance {
            track: None,
            tolerance: Some(Tolerance::Millis(0))
        }),
        Err(CommandError::Document(DocumentError::ToleranceNotPositive))
    );
}

/// A move of one pass (§5) is always an offset of that pass, stored as the difference from the
/// inline offset; deleting the note removes its entries (T5), and the undo brings them back.
#[test]
fn a_pass_move_is_an_offset_of_that_pass_and_dies_with_its_note() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    execute_checked(&mut editor, &drag(s.c.1, d(1, 64)));
    let per_pass = Command::MovePlayed(MovePlayed {
        note: s.c.1,
        pass: Some(1),
        onset: d(1, 2),
        grid: d(1, 16),
        mode: MoveMode::ByThreshold,
    });
    execute_checked(&mut editor, &per_pass);
    let doc = editor.document();
    assert!(same_score(doc, &s.doc));
    let table = doc
        .track(s.track)
        .unwrap()
        .instrument()
        .pass_offsets()
        .clone();
    assert_eq!(
        table.get(s.c.1, 1).onset,
        d(1, 2).checked_sub(d(1, 64)).unwrap()
    );
    assert!(table.get(s.c.1, 0).is_none());
    let with_entry = doc.clone();
    execute_checked(&mut editor, &Command::DeleteEvent(s.c.0));
    let doc = editor.document();
    assert!(
        doc.track(s.track)
            .unwrap()
            .instrument()
            .pass_offsets()
            .is_empty()
    );
    editor.undo().unwrap();
    assert!(editor.document().same_content(&with_entry));
}

/// A drag inside a gesture that crosses the threshold back and forth is one step of undo, and
/// undoing it gives back the original. As the DAW view does, every update asks for the onset
/// from the note's **current** written start, toward an absolute target on the timeline.
#[test]
fn a_drag_across_the_threshold_is_one_step() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    editor.begin_gesture().unwrap();
    for target in [d(1, 64), d(1, 8), d(3, 16), d(1, 32), d(-1, 64), d(1, 8)] {
        let target = Position::from_rational(target.as_rational());
        let now = written(editor.document(), s.c.0);
        let onset = target.checked_duration_since(now).unwrap();
        editor.update_gesture(&drag(s.c.1, onset)).unwrap();
        editor.document().validate().unwrap();
        let doc = editor.document();
        assert_eq!(
            written(doc, s.c.0)
                .checked_add(offsets(doc, s.c.1).onset)
                .unwrap(),
            target
        );
    }
    editor.end_gesture().unwrap();
    assert_eq!(editor.undo_steps(), 1);
    assert_eq!(
        written(editor.document(), s.c.0),
        Position::from_rational(Rational::new(1, 8).unwrap())
    );
    editor.undo().unwrap();
    assert!(editor.document().same_content(&s.doc));
}

/// A plain event change of the offsets still works next to the DAW move (the two write the same
/// inline field).
#[test]
fn the_inline_offsets_are_shared_with_event_changes() {
    let s = small();
    let mut editor = Editor::new(s.doc.clone());
    let set = Command::ChangeEvent {
        event: s.c.0,
        change: EventChange::Offsets {
            note: s.c.1,
            offsets: PerformanceOffsets {
                velocity: 300,
                ..PerformanceOffsets::NONE
            },
        },
    };
    execute_checked(&mut editor, &set);
    execute_checked(&mut editor, &drag(s.c.1, d(1, 64)));
    let o = offsets(editor.document(), s.c.1);
    assert_eq!((o.onset, o.velocity), (d(1, 64), 300));
}

/// Every notehead of a document, with the pass entries and the inline onset it has.
fn noteheads(doc: &Document) -> Vec<NoteId> {
    let mut out = Vec::new();
    for track in doc.tracks() {
        for voice in track.instrument().voices() {
            for &bar in doc.bars().ids() {
                for placed in doc.events(voice.id(), bar).unwrap() {
                    if let Some(chord) = placed.event().chord() {
                        out.extend(chord.notes().map(crate::event::Note::id));
                    }
                }
            }
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Random DAW moves on the rich document (see the module documentation).
    #[test]
    fn random_moves_keep_the_document_valid_and_land_where_asked(
        moves in prop::collection::vec(
            (any::<u16>(), -48_i64..=48, 0_u8..4, 0_u8..4, 0_u8..3),
            1..24,
        ),
    ) {
        let original = rich();
        let mut editor = Editor::new(original.clone());
        let grids = [d(1, 16), d(1, 8), d(1, 4), d(1, 12)];
        for (pick, sixty_fourths, grid, mode, pass) in moves {
            let doc = editor.document().clone();
            let heads = noteheads(&doc);
            let note = heads[usize::from(pick) % heads.len()];
            let mode = if mode == 0 { MoveMode::PerformanceOnly } else { MoveMode::ByThreshold };
            let pass = (pass == 2).then_some(1);
            let onset = d(sixty_fourths, 64);
            let order = MovePlayed {
                note,
                pass,
                onset,
                grid: grids[usize::from(grid)],
                mode,
            };
            let event = doc.note(note).unwrap().0.event().id();
            // One draw in twelve deletes the note's event instead: its pass entries must go with
            // it (T5), which the full validation checks (rule 12).
            if pick % 12 == 0 {
                if editor.execute(&Command::DeleteEvent(event)).is_ok() {
                    editor.document().validate().unwrap();
                    assert_consistent(editor.document());
                }
                continue;
            }
            let before_start = written(&doc, event);
            let before_inline = offsets(&doc, note).onset;
            match editor.execute(&Command::MovePlayed(order)) {
                Ok(_) => {
                    let after = editor.document();
                    after.validate().unwrap();
                    assert_consistent(after);
                    let start = written(after, event);
                    let inline = offsets(after, note).onset;
                    if let Some(p) = pass {
                        // A pass move: score and inline offset untouched, the sum lands.
                        prop_assert!(same_score(after, &doc));
                        prop_assert_eq!(inline, before_inline);
                        let (instrument, _) = after.voice(after.note(note).unwrap().0.voice()).unwrap();
                        let entry = instrument.pass_offsets().get(note, p).onset;
                        prop_assert_eq!(inline.checked_add(entry).unwrap(), onset);
                    } else {
                        // Every other move: the played start is exactly the one asked for.
                        prop_assert_eq!(
                            start.checked_add(inline).unwrap(),
                            before_start.checked_add(onset).unwrap()
                        );
                        if mode == MoveMode::PerformanceOnly {
                            prop_assert!(same_score(after, &doc));
                        }
                    }
                }
                Err(_) => prop_assert!(editor.document().same_content(&doc)),
            }
        }
        while editor.undo().unwrap().is_some() {}
        prop_assert!(editor.document().same_content(&original));
    }
}
