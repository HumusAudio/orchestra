// SPDX-License-Identifier: GPL-3.0-or-later

//! The memory one step of the undo history holds, measured, against the estimate the editor
//! bounds the history with (ADR-0026 §3 and §8; SCALETTA F1.11b, part 2b).
//!
//! # Why
//! The history is bounded by memory (128 MB by default, U3), but by an **estimate**: each edit
//! reports its own size plus the extra noteheads of a chord ([`orchestra_core_model::Edit`]'s
//! `heap_bytes`). If the estimate were far below the real heap, the limit would let the history
//! grow well past 128 MB on the 8 GB reference laptop. This test measures the real heap of one
//! step for the typical actions and checks that the estimate is not too low. Its first run found
//! the spare capacity of the steps' vectors (up to half of a large step), which the history now
//! drops when it records a step.
//!
//! # How the heap of a step is measured
//! An editor with a history limit of 0 keeps exactly the most recent step (the last one is
//! always kept). After the action, the editor is turned back into its document
//! ([`Editor::into_document`]), which drops the history: the bytes that frees, as counted by the
//! counting allocator of `orchestra-test-support`, are what the history held. That includes the
//! history's own small buffers (the deque of steps), measured once on a step of a single edit
//! and reported as the overhead.
//!
//! The counts are what the program requests from the allocator, not the allocator's rounding:
//! the same on every platform.

#![allow(
    clippy::unwrap_used,
    reason = "test helpers: a failed unwrap fails the test"
)]

use orchestra_core_model::{
    BaseValue, Command, Document, DynamicLevel, DynamicMark, DynamicPlacement, Editor, MarkSpec,
    NewBody, NewEvent, NoteValue, Pitch, Placement, ScorePoint, Step,
};
use orchestra_core_time::{BarIndex, Duration, Rational};

// The plain preset of the reference scores (ADR-0031): `plain(size, editing)`.
use orchestra_test_scores::{Size, plain};

orchestra_test_support::install_counting_allocator!();

/// The estimate must be at least this percentage of the measured heap of a step (beyond the
/// history's own buffers). Measured on 2026-11-05, after the steps were shrunk to their length
/// (`Step::undoing`): one note 208/208 bytes, 200 notes pasted 41 600/41 600, 100 bars of the heavy
/// score deleted 4 567 688/4 567 768: the estimate is exact but for the texts it does not count
/// (tens of bytes). Below 90% the history could outgrow its limit by more than a tenth.
const MIN_ESTIMATE_PERCENT: usize = 90;

/// The editing voice and staff of a score built with the editing instrument (the last one).
fn editing(doc: &Document) -> (orchestra_core_model::VoiceId, orchestra_core_model::StaffId) {
    let instrument = doc.tracks().last().unwrap().instrument();
    (instrument.voices()[0].id(), instrument.staves()[0].id())
}

/// A quarter note C5 entered into the editing voice at a bar and beat.
fn note(doc: &Document, bar: u32, beat: i64) -> Command {
    let (voice, _) = editing(doc);
    Command::EnterEvent(NewEvent {
        at: Placement::new(
            voice,
            doc.bars().id_at(BarIndex::new(bar)).unwrap(),
            Duration::from_rational(Rational::new(beat, 4).unwrap()),
        ),
        body: NewBody::Chord {
            value: NoteValue::plain(BaseValue::Quarter),
            pitches: vec![Pitch::new(
                Step::C,
                orchestra_core_model::Alteration::NATURAL,
                5,
            )],
            grace: None,
        },
    })
}

/// Runs `action` on an editor whose history keeps only the last step, then returns the history's
/// estimate and the heap that dropping the history frees.
fn measure(doc: Document, action: impl FnOnce(&mut Editor)) -> (usize, i128) {
    let mut editor = Editor::with_history_limit(doc, 0);
    action(&mut editor);
    assert_eq!(editor.undo_steps(), 1);
    let estimate = editor.history_bytes();
    let (document, stats) =
        orchestra_test_support::count_allocations(move || editor.into_document()).unwrap();
    drop(document);
    (estimate, -stats.net_bytes())
}

/// The heap of one step of the typical actions, against the estimate.
#[test]
fn the_history_estimate_follows_the_heap() {
    let normal = || plain(Size::Normal, true);
    let mut rows: Vec<(&str, usize, i128)> = Vec::new();

    // The overhead: a step of one small edit (a bar left uncounted).
    let (estimate, freed) = measure(normal(), |e| {
        let bar = e.document().bars().ids()[0];
        e.execute(&Command::SetUncounted {
            bar,
            uncounted: true,
        })
        .unwrap();
    });
    rows.push(("one small edit (overhead)", estimate, freed));
    let overhead = freed;

    let (estimate, freed) = measure(normal(), |e| {
        let command = note(e.document(), 10, 0);
        e.execute(&command).unwrap();
    });
    rows.push(("one note entered", estimate, freed));

    let (estimate, freed) = measure(normal(), |e| {
        let (_, staff) = editing(e.document());
        let instrument = e.document().tracks().last().unwrap().instrument().id();
        let bar = e.document().bars().ids()[3];
        e.execute(&Command::AddMark(MarkSpec::Dynamic {
            instrument,
            at: ScorePoint::start_of(bar),
            placement: DynamicPlacement::instrument(staff),
            mark: DynamicMark::Level(DynamicLevel::F),
        }))
        .unwrap();
    });
    rows.push(("one dynamic added", estimate, freed));

    let (estimate, freed) = measure(normal(), |e| {
        let commands: Vec<Command> = (0..200)
            .map(|i| note(e.document(), 10 + i / 4, i64::from(i % 4)))
            .collect();
        e.transact(|tx| commands.iter().try_for_each(|c| tx.execute(c)))
            .unwrap();
    });
    rows.push(("200 notes pasted", estimate, freed));

    let (estimate, freed) = measure(normal(), |e| {
        let command = note(e.document(), 10, 0);
        e.execute(&command).unwrap();
        let event = e
            .document()
            .events(editing(e.document()).0, e.document().bars().ids()[10])
            .unwrap()[0]
            .event()
            .id();
        let bar = e.document().bars().ids()[10];
        // The entering step is dropped by the gesture's (limit 0): only the gesture is measured.
        e.begin_gesture().unwrap();
        for i in 0..1000 {
            e.update_gesture(&Command::ChangeEvent {
                event,
                change: orchestra_core_model::EventChange::Move {
                    bar,
                    offset: Duration::from_rational(Rational::new(i % 4, 4).unwrap()),
                },
            })
            .unwrap();
        }
        e.end_gesture().unwrap();
    });
    rows.push(("1 000-update gesture", estimate, freed));

    let (estimate, freed) = measure(plain(Size::Heavy, true), |e| {
        let bar = e.document().bars().ids()[500];
        e.execute(&Command::ChangeMeter {
            bar,
            change: Some(orchestra_core_time::MeterChange::new(
                orchestra_core_time::Meter::new(&[3], 4).unwrap(),
            )),
            mode: orchestra_core_model::MeterMode::Rewrite,
        })
        .unwrap();
    });
    rows.push((
        "a 3/4 rewriting 60 bars of the heavy score",
        estimate,
        freed,
    ));

    let (estimate, freed) = measure(plain(Size::Heavy, true), |e| {
        e.execute(&Command::DeleteBars {
            at: BarIndex::new(100),
            count: 100,
        })
        .unwrap();
    });
    rows.push(("100 bars of the heavy score deleted", estimate, freed));

    // Printed so that `cargo test -- --nocapture` shows the numbers the bound is based on.
    println!("{:<40} {:>12} {:>12}", "step", "estimate", "measured");
    for (name, estimate, freed) in &rows {
        println!("{name:<40} {estimate:>12} {freed:>12}");
    }
    for (name, estimate, freed) in rows {
        // The overhead of the history's own buffers is not part of a step's estimate.
        let step = usize::try_from((freed - overhead).max(0)).unwrap();
        assert!(
            estimate.saturating_mul(100) >= step.saturating_mul(MIN_ESTIMATE_PERCENT),
            "{name}: estimate {estimate} bytes, measured {step} bytes beyond the overhead"
        );
    }
}
