// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the commands that change bar lengths in `KeepBars` mode (ADR-0026 §7, §11 Y1–Y6;
//! SCALETTA F1.11c, part 1).
//!
//! # Why inside the crate
//! They start from the rich document of `crate::editor_tests` (8 bars of 4/4, a flute whole note
//! in bar 3, tempo marks at bars 4 and 5, a bar repeat of the flute at bar 6 repeating bar 5),
//! whose helpers are crate-private.
//!
//! # What is checked
//! One hand-written case per decision Y2–Y5 and Y7, each through `execute_checked` (valid and consistent
//! after the command, undo gives back the document from before, redo the one after) or `refused`
//! (nothing changed, no step recorded). The property test of `crate::command_tests` generates
//! these commands too.

use orchestra_core_time::{
    BarIndex, BarKind, BeatGrouping, Duration, IrregularBar, Meter, MeterChange, MeterMapError,
    Position, Rational,
};

use crate::change::ChangeSet;
use crate::command::Command;
use crate::document::DocumentError;
use crate::editor::{CommandError, Editor};
use crate::editor_tests::{assert_consistent, rich};
use crate::id::BarId;
use crate::meter_command::MeterMode;

/// Shorthand: a duration of `n/d` whole notes.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// Shorthand: a meter change to `n/den`, counted by the default rule.
fn meter(n: u8, den: u16) -> MeterChange {
    MeterChange::new(Meter::new(&[n], den).unwrap())
}

/// Shorthand: the `KeepBars` meter change at a bar.
fn change(bar: BarId, change: Option<MeterChange>) -> Command {
    Command::ChangeMeter {
        bar,
        change,
        mode: MeterMode::KeepBars,
    }
}

/// Shorthand: the `KeepBars` change of one bar's length.
fn irregular(bar: BarId, irregular: Option<IrregularBar>) -> Command {
    Command::SetIrregularBar {
        bar,
        irregular,
        mode: MeterMode::KeepBars,
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

/// Shorthand: a refusal by the meter map.
fn map_error(error: MeterMapError) -> CommandError {
    CommandError::Document(DocumentError::MeterMap(error))
}

/// The start of a bar, as a position.
fn start(editor: &Editor, bar: u32) -> Position {
    editor
        .document()
        .meter_map()
        .bar_start(BarIndex::new(bar))
        .unwrap()
}

/// A 3/4 at bar 3 in `KeepBars`: the flute's whole note stays in bar 3 (now overfull), the bars
/// after it start earlier, the tempo map changes from there; no event changes (Y2).
#[test]
fn keep_bars_moves_the_music_after_the_change() {
    let mut editor = Editor::new(rich());
    let bars = editor.document().bars().ids().to_vec();
    let flute = editor.document().instruments().nth(1).unwrap().voices()[0].id();
    let events = editor.document().events(flute, bars[3]).unwrap().to_vec();
    let changes = execute_checked(&mut editor, &change(bars[3], Some(meter(3, 4))));
    assert_eq!(editor.document().events(flute, bars[3]).unwrap(), events);
    // Bars 0–2 last 3 wholes, bar 3 three quarters.
    assert_eq!(
        start(&editor, 4),
        Position::ZERO.checked_add(d(15, 4)).unwrap()
    );
    assert_eq!(changes.meter_from(), Some(BarIndex::new(3)));
    assert!(changes.tempo_from().is_some());
    assert_eq!(changes.cells(), []);
}

/// A meter change is removed with `None`, and the bars are as before; at bar 0 it is refused.
#[test]
fn a_meter_change_is_removed() {
    let mut editor = Editor::new(rich());
    let bars = editor.document().bars().ids().to_vec();
    let original = start(&editor, 7);
    execute_checked(&mut editor, &change(bars[3], Some(meter(3, 4))));
    execute_checked(&mut editor, &change(bars[3], None));
    assert_eq!(start(&editor, 7), original);
    assert_eq!(
        refused(&mut editor, &change(bars[0], None)),
        map_error(MeterMapError::NoMeterAtStart)
    );
}

/// The per-bar groupings of the section that no longer add up are removed (Y3); one outside the
/// section, and one that still adds up, stay.
#[test]
fn groupings_that_no_longer_fit_are_removed() {
    let mut editor = Editor::new(rich());
    let bars = editor.document().bars().ids().to_vec();
    let three_three_two = BeatGrouping::new(&[3, 3, 2], 8).unwrap();
    for bar in [1, 4] {
        execute_checked(
            &mut editor,
            &Command::SetBarGrouping {
                bar: bars[bar],
                grouping: Some(three_three_two),
            },
        );
    }
    let groupings = |editor: &Editor| -> Vec<u32> {
        editor
            .document()
            .meter_map()
            .bar_groupings()
            .iter()
            .map(|(b, _)| b.get())
            .collect()
    };
    // 2/2 lasts as long as 4/4: both groupings still add up.
    execute_checked(&mut editor, &change(bars[3], Some(meter(2, 2))));
    assert_eq!(groupings(&editor), [1, 4]);
    // 3/4 does not: the one of bar 4 goes, the one of bar 1 (before the change) stays.
    execute_checked(&mut editor, &change(bars[3], Some(meter(3, 4))));
    assert_eq!(groupings(&editor), [1]);
}

/// A pickup longer than the new meter, and a section grouping that does not add up to it, are
/// refused with the meter map's error (Y4).
#[test]
fn contradictions_are_refused() {
    let mut editor = Editor::new(rich());
    let bars = editor.document().bars().ids().to_vec();
    execute_checked(
        &mut editor,
        &irregular(
            bars[0],
            Some(IrregularBar {
                length: d(3, 4),
                kind: BarKind::AlignEnd,
            }),
        ),
    );
    assert_eq!(
        refused(&mut editor, &change(bars[0], Some(meter(2, 4)))),
        map_error(MeterMapError::PickupLongerThanMeter {
            bar: BarIndex::FIRST
        })
    );
    let wrong = MeterChange::with_grouping(
        Meter::new(&[3], 4).unwrap(),
        BeatGrouping::new(&[3, 3, 2], 8).unwrap(),
    );
    assert_eq!(
        refused(&mut editor, &change(bars[3], Some(wrong))),
        map_error(MeterMapError::GroupingLengthMismatch {
            bar: BarIndex::new(3)
        })
    );
}

/// Inside the flute's bar repeat (bar 6 repeats bar 5) a meter change after its first source bar,
/// and any change of one bar's length, are refused (Y5); a meter change at the first source bar
/// changes every bar of the range alike and is accepted.
#[test]
fn bar_repeats_keep_their_lengths() {
    let mut editor = Editor::new(rich());
    let bars = editor.document().bars().ids().to_vec();
    let repeat = editor
        .document()
        .instruments()
        .nth(1)
        .unwrap()
        .bar_repeats()[0]
        .id();
    let inside = CommandError::Document(DocumentError::InsideBarRepeat(repeat));
    assert_eq!(
        refused(&mut editor, &change(bars[6], Some(meter(3, 4)))),
        inside
    );
    let short = Some(IrregularBar {
        length: d(1, 2),
        kind: BarKind::AlignStart,
    });
    assert_eq!(refused(&mut editor, &irregular(bars[5], short)), inside);
    assert_eq!(refused(&mut editor, &irregular(bars[6], short)), inside);
    execute_checked(&mut editor, &irregular(bars[4], short));
    execute_checked(&mut editor, &change(bars[5], Some(meter(3, 4))));
}

/// A free bar loses its per-bar grouping (a free bar has no beats, Y3); a bar shortened under its
/// music becomes overfull, a valid state (Y2); `None` makes it regular again.
#[test]
fn irregular_and_free_bars() {
    let mut editor = Editor::new(rich());
    let bars = editor.document().bars().ids().to_vec();
    execute_checked(
        &mut editor,
        &Command::SetBarGrouping {
            bar: bars[1],
            grouping: Some(BeatGrouping::new(&[3, 3, 2], 8).unwrap()),
        },
    );
    let free = Some(IrregularBar {
        length: d(3, 2),
        kind: BarKind::Free,
    });
    execute_checked(&mut editor, &irregular(bars[1], free));
    let map = editor.document().meter_map();
    assert!(map.is_free(BarIndex::new(1)).unwrap());
    assert_eq!(map.bar_groupings(), []);
    // The flute's whole note in a bar of a quarter: overfull, valid.
    let quarter = Some(IrregularBar {
        length: d(1, 4),
        kind: BarKind::AlignStart,
    });
    execute_checked(&mut editor, &irregular(bars[3], quarter));
    assert_eq!(
        editor
            .document()
            .meter_map()
            .bar_length(BarIndex::new(3))
            .unwrap(),
        d(1, 4)
    );
    execute_checked(&mut editor, &irregular(bars[3], None));
    assert_eq!(
        editor
            .document()
            .meter_map()
            .bar_length(BarIndex::new(3))
            .unwrap(),
        d(1, 1)
    );
}

/// A dotted half entered on the last beat of bar 4 of the flute becomes a quarter tied to a half
/// in bar 5. Shortening bar 4 to 3/4 would leave the quarter past the barline, its tie going
/// backwards in score order: refused (Y7). Without the tie (bar 7, a single quarter on beat 4) the
/// same change is accepted and the bar is overfull.
#[test]
fn a_tie_past_a_shortened_barline_is_refused() {
    use crate::command::{NewBody, NewEvent};
    use crate::document::Placement;
    use crate::event::{BaseValue, NoteValue};
    use crate::pitch::{Alteration, Pitch, Step};
    let mut editor = Editor::new(rich());
    let bars = editor.document().bars().ids().to_vec();
    let flute = editor.document().instruments().nth(1).unwrap().voices()[0].id();
    let enter = |bar: BarId, value: NoteValue| {
        Command::EnterEvent(NewEvent {
            at: Placement::new(flute, bar, d(3, 4)),
            body: NewBody::Chord {
                value,
                pitches: vec![Pitch::new(Step::E, Alteration::NATURAL, 5)],
                grace: None,
            },
        })
    };
    execute_checked(
        &mut editor,
        &enter(bars[4], NoteValue::new(BaseValue::Half, 1).unwrap()),
    );
    let error = refused(&mut editor, &change(bars[4], Some(meter(3, 4))));
    assert!(
        matches!(
            error,
            CommandError::Document(DocumentError::Invalid(
                crate::validate::Violation::TieNotForward(_)
            ))
        ),
        "{error:?}"
    );
    execute_checked(
        &mut editor,
        &enter(bars[7], NoteValue::plain(BaseValue::Quarter)),
    );
    execute_checked(&mut editor, &change(bars[7], Some(meter(3, 4))));
}

/// Bars inserted right where a hairpin, a pass rule or a gradual tempo change ends ("up to" that
/// barline, an exclusive end) go after it: the end stays before the new bars, at the start of the
/// first one (Y8). In the rich document the crescendo ends at bar 1, the piano's pass rule at bar
/// 2, the accelerando at bar 6.
#[test]
fn ends_at_the_insertion_point_stay_before_the_new_bars() {
    use crate::marks::ScorePoint;
    use crate::tempo_mark::TempoMarkChange;
    let mut editor = Editor::new(rich());
    // The flute's bar repeat at bar 6 would refuse an insertion there (W6): it goes first.
    let repeat = editor
        .document()
        .instruments()
        .nth(1)
        .unwrap()
        .bar_repeats()[0]
        .id();
    execute_checked(
        &mut editor,
        &Command::RemoveMark(crate::edit::ObjectRef::BarRepeat(repeat)),
    );
    for at in [6, 2, 1] {
        execute_checked(
            &mut editor,
            &Command::InsertBars {
                at: BarIndex::new(at),
                count: 2,
            },
        );
    }
    let doc = editor.document();
    let bars = doc.bars().ids();
    let piano = doc.instruments().next().unwrap();
    // Bar 1 got two bars before it (indices 1 and 2): the crescendo ends at the first of them.
    assert_eq!(piano.hairpins()[0].end(), ScorePoint::start_of(bars[1]));
    // The pass rule began at old bar 1 (now 3) and ends where old bar 2 began: the first of the
    // two bars inserted before it (now 4).
    assert_eq!(piano.pass_rules()[0].start(), ScorePoint::start_of(bars[3]));
    assert_eq!(piano.pass_rules()[0].end(), ScorePoint::start_of(bars[4]));
    let gradual = doc
        .tempo_marks()
        .as_slice()
        .iter()
        .find_map(|m| match m.value().change() {
            Some(TempoMarkChange::Gradual(g)) => Some(g),
            _ => None,
        })
        .unwrap();
    // Old bar 6 is now 12, with the two bars inserted before it at 10 and 11.
    assert_eq!(gradual.end, ScorePoint::start_of(bars[10]));
}
