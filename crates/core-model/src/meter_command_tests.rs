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

// ------------------------------------------------------------------------------------------------
// Rewrite (F1.11c part 2b, R1–R11)
// ------------------------------------------------------------------------------------------------

/// Shorthand: the `Rewrite` meter change at a bar.
fn rewrite(bar: BarId, change: Option<MeterChange>) -> Command {
    Command::ChangeMeter {
        bar,
        change,
        mode: MeterMode::Rewrite,
    }
}

/// The rich document without the flute's bar repeat (bar 6 repeating bar 5), which would refuse
/// every rewrite that reaches it (R10).
fn rich_without_bar_repeat() -> Editor {
    let mut editor = Editor::new(rich());
    let repeat = editor
        .document()
        .instruments()
        .nth(1)
        .unwrap()
        .bar_repeats()[0]
        .id();
    editor
        .execute(&Command::RemoveMark(crate::edit::ObjectRef::BarRepeat(
            repeat,
        )))
        .unwrap();
    editor
}

/// The lengths of the bars from `from` on.
fn lengths(editor: &Editor, from: u32) -> Vec<Duration> {
    let map = editor.document().meter_map();
    let bars = u32::try_from(editor.document().bars().len()).unwrap();
    (from..bars)
        .map(|b| map.bar_length(BarIndex::new(b)).unwrap())
        .collect()
}

/// The reference case: a 3/4 at bar 3 of the rich document (8 bars of 4/4). Bars 3–7 last 20
/// quarters. The Fine at the end of old bar 4 (8 quarters in) falls inside a bar of 3/4, which is
/// split 2/4 + 1/4, the second part uncounted (R9); the D.S. at the end of the score ends the last
/// bar there (2/4) instead of leaving an empty tail. The flute's whole note of bar 3 becomes a
/// dotted half tied to a quarter (R5); the marks move to where they sound (R7).
#[test]
fn rewrite_moves_the_barlines() {
    use crate::jump::MarkerKind;
    let mut editor = rich_without_bar_repeat();
    let flute = editor.document().instruments().nth(1).unwrap().voices()[0].id();
    let bars = editor.document().bars().ids().to_vec();
    // A numbering restart at old bar 4 (4 quarters in) goes to the new bar holding that point.
    execute_checked(
        &mut editor,
        &Command::SetNumberingRestart {
            bar: bars[4],
            number: Some(50),
        },
    );
    execute_checked(&mut editor, &rewrite(bars[3], Some(meter(3, 4))));
    let doc = editor.document();
    let q = |n| d(n, 4);
    assert_eq!(
        lengths(&editor, 3),
        [q(3), q(3), q(2), q(1), q(3), q(3), q(3), q(2)]
    );
    let ids = doc.bars().ids();
    let map = doc.meter_map();
    assert_eq!(map.uncounted_bars(), [BarIndex::new(6)]);
    assert_eq!(map.numbering_restarts(), [(BarIndex::new(4), 50)]);
    // The Fine ends the 2/4 part (bar 5); the D.S. ends the last bar (bar 10).
    let form = doc.form();
    let fine = form
        .markers()
        .iter()
        .find(|m| m.kind() == MarkerKind::Fine)
        .unwrap();
    assert_eq!(fine.bar(), ids[5]);
    assert_eq!(form.jumps()[0].bar(), ids[10]);
    // The flute: a dotted half tied to a quarter, then its quarter of old bar 4 on beat 2 of bar 4.
    let first = &doc.events(flute, ids[3]).unwrap()[0];
    let chord = first.event().chord().unwrap();
    assert_eq!(
        chord.value(),
        crate::event::NoteValue::new(crate::event::BaseValue::Half, 1).unwrap()
    );
    let in_bar_4 = doc.events(flute, ids[4]).unwrap();
    assert_eq!(in_bar_4.len(), 2);
    assert_eq!(in_bar_4[1].offset(), q(1));
    assert!(matches!(
        chord.notes().next().unwrap().tie(),
        crate::event::Tie::To(n) if n == in_bar_4[0].event().chord().unwrap().notes().next().unwrap().id()
    ));
    // The tempo mark of old bar 4 (4 quarters in) is on beat 2 of bar 4; the accelerando from old
    // bar 5 (8 quarters in) starts bar 6 and ends where old bar 6 began, bar 8.
    let marks = doc.tempo_marks().as_slice();
    assert_eq!((marks[0].bar(), marks[0].offset()), (ids[4], q(1)));
    assert_eq!(
        (marks[1].bar(), marks[1].offset()),
        (ids[6], Duration::ZERO)
    );
    let Some(crate::tempo_mark::TempoMarkChange::Gradual(g)) = marks[1].value().change() else {
        panic!("the second tempo mark is gradual");
    };
    assert_eq!(g.end, crate::marks::ScorePoint::start_of(ids[8]));
    // Nothing before the region moved; the music sounds where it did.
    assert_eq!(
        map.bar_start(BarIndex::new(3)).unwrap(),
        Position::ZERO.checked_add(d(3, 1)).unwrap()
    );
}

/// The rewrite stops at the next meter change (R1): with a 4/4 marked again at bar 5, a 3/4 at
/// bar 3 rewrites bars 3–4 only (8 quarters: two bars of 3/4 and an irregular bar of 2/4 with what
/// is left, R2), and old bar 5 starts where it did.
#[test]
fn rewrite_stops_at_the_next_meter_change() {
    let mut editor = rich_without_bar_repeat();
    let bars = editor.document().bars().ids().to_vec();
    execute_checked(&mut editor, &change(bars[5], Some(meter(4, 4))));
    let before = editor
        .document()
        .meter_map()
        .bar_start(BarIndex::new(5))
        .unwrap();
    execute_checked(&mut editor, &rewrite(bars[3], Some(meter(3, 4))));
    let doc = editor.document();
    let five = doc.bars().index_of(bars[5]).unwrap();
    assert_eq!(five, BarIndex::new(6));
    assert_eq!(doc.meter_map().bar_start(five).unwrap(), before);
    assert_eq!(
        lengths(&editor, 3)[..4],
        [d(3, 4), d(3, 4), d(2, 4), d(1, 1)]
    );
}

/// A whole-bar rest becomes rests of written values (R6), split at the new barline; the end
/// repeat at the end of old bar 2 falls inside a bar and splits it (R9).
#[test]
fn rewrite_turns_whole_bar_rests_into_values() {
    use crate::event::{BaseValue, EventBody, NoteValue, RestValue};
    let mut editor = rich_without_bar_repeat();
    let lh = editor.document().instruments().next().unwrap().voices()[1].id();
    let bars = editor.document().bars().ids().to_vec();
    execute_checked(&mut editor, &rewrite(bars[2], Some(meter(3, 4))));
    let doc = editor.document();
    let ids = doc.bars().ids();
    let rest_value = |bar: usize| match doc.events(lh, ids[bar]).unwrap()[0].event().body() {
        EventBody::Rest(r) => r.value(),
        EventBody::Chord(_) => panic!("a rest"),
    };
    assert_eq!(
        rest_value(2),
        RestValue::Value(NoteValue::new(BaseValue::Half, 1).unwrap())
    );
    assert_eq!(
        rest_value(3),
        RestValue::Value(NoteValue::plain(BaseValue::Quarter))
    );
    // The section 1–2 now ends with the 1/4 part of new bar 3.
    // (A whole-bar rest becomes a written rest even where no barline cuts it: see the 2/2 case.)
    assert_eq!(doc.form().repeat_ends()[0].bar(), ids[3]);
    assert_eq!(lengths(&editor, 3)[0], d(1, 4));
}

/// A slur ending on a note that the rewrite splits ends on its last part (R5).
#[test]
fn rewrite_moves_slur_ends_to_the_last_part() {
    let mut editor = rich_without_bar_repeat();
    let piano = editor.document().instruments().next().unwrap();
    let (rh, lh) = (piano.voices()[0].id(), piano.voices()[1].id());
    let bars = editor.document().bars().ids().to_vec();
    let start = editor.document().events(rh, bars[0]).unwrap()[0]
        .event()
        .id();
    // The left hand's second half note, from beat 3 of bar 0: it will cross the 3/4 barline.
    let end = editor.document().events(lh, bars[0]).unwrap()[1]
        .event()
        .id();
    let changes = execute_checked(
        &mut editor,
        &Command::AddMark(crate::mark_command::MarkSpec::Slur {
            start,
            end,
            placement: crate::slur::SlurPlacement::Auto,
        }),
    );
    let crate::edit::ObjectRef::Slur(slur) = changes.inserted()[0] else {
        panic!("a slur");
    };
    execute_checked(&mut editor, &rewrite(bars[0], Some(meter(3, 4))));
    let doc = editor.document();
    let last = doc.events(lh, doc.bars().ids()[1]).unwrap()[0].event().id();
    assert_eq!(doc.slur(slur).unwrap().1.end(), last);
    assert_ne!(last, end);
}

/// Adding a quarter-note pickup to music already written shifts every barline by a quarter (R4);
/// a later meter change at bar 0 keeps the pickup (R3).
#[test]
fn rewrite_with_a_pickup() {
    let mut editor = rich_without_bar_repeat();
    let bars = editor.document().bars().ids().to_vec();
    let pickup = IrregularBar {
        length: d(1, 4),
        kind: BarKind::AlignEnd,
    };
    execute_checked(
        &mut editor,
        &Command::SetIrregularBar {
            bar: bars[0],
            irregular: Some(pickup),
            mode: MeterMode::Rewrite,
        },
    );
    assert_eq!(lengths(&editor, 0)[0], d(1, 4));
    let rh = editor.document().instruments().next().unwrap().voices()[0].id();
    // The right hand's first chord fills the pickup; the next quarter starts bar 1.
    let doc = editor.document();
    assert_eq!(doc.events(rh, doc.bars().ids()[0]).unwrap().len(), 1);
    let first = editor.document().bars().ids()[0];
    execute_checked(&mut editor, &rewrite(first, Some(meter(3, 4))));
    assert_eq!(lengths(&editor, 0)[..2], [d(1, 4), d(3, 4)]);
}

/// A rewrite that reaches a bar repeat is refused (R10).
#[test]
fn rewrite_refuses_bar_repeats() {
    let mut editor = Editor::new(rich());
    let bars = editor.document().bars().ids().to_vec();
    let repeat = editor
        .document()
        .instruments()
        .nth(1)
        .unwrap()
        .bar_repeats()[0]
        .id();
    assert_eq!(
        refused(&mut editor, &rewrite(bars[3], Some(meter(3, 4)))),
        CommandError::Document(DocumentError::InsideBarRepeat(repeat))
    );
}

/// A whole-bar rest becomes a written rest even when its new bar has the same length (R6): under a
/// 2/2 from bar 2, the left hand's whole-bar rest of bar 2 is a whole rest.
#[test]
fn rewrite_writes_whole_bar_rests_even_when_aligned() {
    use crate::event::{BaseValue, EventBody, NoteValue, RestValue};
    let mut editor = rich_without_bar_repeat();
    let lh = editor.document().instruments().next().unwrap().voices()[1].id();
    let bars = editor.document().bars().ids().to_vec();
    execute_checked(&mut editor, &rewrite(bars[2], Some(meter(2, 2))));
    let doc = editor.document();
    let EventBody::Rest(rest) = doc.events(lh, bars[2]).unwrap()[0].event().body() else {
        panic!("a rest");
    };
    assert_eq!(
        rest.value(),
        RestValue::Value(NoteValue::plain(BaseValue::Whole))
    );
}

/// A split note's own tie goes on from its last part (R5), and an `After` grace note at the end of
/// its main event stays in that bar when the end becomes a barline (R5). Under 3/4 from bar 0, the
/// left hand's half note on beat 3, tied to a C3 at the start of bar 1, becomes a quarter tied to
/// a quarter, the second tied to that C3; the right hand's `After` grace at the end of beat 3
/// (offset 3/4) stays at the end of bar 0.
#[test]
fn rewrite_keeps_ties_and_after_graces() {
    use crate::command::{EventChange, NewBody, NewEvent};
    use crate::document::Placement;
    use crate::event::{BaseValue, NoteValue, Tie};
    use crate::marking::{Grace, GraceAttach, GraceKind};
    use crate::pitch::{Alteration, Pitch, Step};
    let mut editor = rich_without_bar_repeat();
    let piano = editor.document().instruments().next().unwrap();
    let (rh, lh) = (piano.voices()[0].id(), piano.voices()[1].id());
    let bars = editor.document().bars().ids().to_vec();
    let c3 = Pitch::new(Step::C, Alteration::NATURAL, 3);
    let changes = execute_checked(
        &mut editor,
        &Command::EnterEvent(NewEvent {
            at: Placement::new(lh, bars[1], Duration::ZERO),
            body: NewBody::Chord {
                value: NoteValue::plain(BaseValue::Quarter),
                pitches: vec![c3],
                grace: None,
            },
        }),
    );
    let target_event = changes
        .inserted()
        .iter()
        .find_map(|o| match o {
            crate::edit::ObjectRef::Event(e) => Some(*e),
            _ => None,
        })
        .unwrap();
    let doc = editor.document();
    let target = doc
        .event(target_event)
        .unwrap()
        .event()
        .chord()
        .unwrap()
        .notes()
        .next()
        .unwrap()
        .id();
    let half = doc.events(lh, bars[0]).unwrap()[1].event().clone();
    let half_note = half.chord().unwrap().notes().next().unwrap().id();
    execute_checked(
        &mut editor,
        &Command::ChangeEvent {
            event: half.id(),
            change: EventChange::Tie {
                note: half_note,
                tie: Tie::To(target),
            },
        },
    );
    // An `After` grace at the end of the right hand's third quarter (offset 3/4).
    execute_checked(
        &mut editor,
        &Command::EnterEvent(NewEvent {
            at: Placement::new(rh, bars[0], d(3, 4)),
            body: NewBody::Chord {
                value: NoteValue::plain(BaseValue::Eighth),
                pitches: vec![Pitch::new(Step::G, Alteration::NATURAL, 5)],
                grace: Some(Grace {
                    kind: GraceKind::Acciaccatura,
                    attach: GraceAttach::After,
                }),
            },
        }),
    );
    execute_checked(&mut editor, &rewrite(bars[0], Some(meter(3, 4))));
    let doc = editor.document();
    let ids = doc.bars().ids();
    // The half's last part, at the start of bar 1, is tied to the C3 (now on beat 2 of bar 1).
    let last = doc.events(lh, ids[1]).unwrap()[0].event().clone();
    assert_ne!(last.id(), half.id());
    assert_eq!(
        last.chord().unwrap().notes().next().unwrap().tie(),
        Tie::To(target)
    );
    // The grace note is the last event of bar 0, at its end.
    let end_of_bar_0 = doc.events(rh, ids[0]).unwrap().last().unwrap().clone();
    assert!(end_of_bar_0.event().grace().is_some());
    assert_eq!(end_of_bar_0.offset(), d(3, 4));
}
