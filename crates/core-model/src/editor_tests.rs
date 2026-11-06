// SPDX-License-Identifier: GPL-3.0-or-later

//! Tests of the primitive edits and of the editor (ADR-0026 §8; SCALETTA F1.11b, part 1).
//!
//! # Why inside the crate
//! The edits take whole objects (a dynamic, a tempo mark, an event with its noteheads), whose
//! constructors are crate-private: only the document and, from the second part of F1.11b, the
//! commands create them. So these tests live in the crate, where they can build any object.
//!
//! # The property test
//! A rich document (every kind of object, grace notes sharing an offset, a tuplet, a gradual tempo
//! change, a repeat, a D.S. al Fine, a bar repeat, a pass rule) receives random transactions of
//! random edits — many of them refused, which must change nothing — interleaved with random undos
//! and redos. The model is a list of **snapshots**, one per step of the history, and a cursor:
//! - after a transaction that changed something, the document is a new snapshot and the redo
//!   snapshots are gone;
//! - after a transaction that failed (the closure returned an error), or that applied nothing, the
//!   document has the same content as the current snapshot;
//! - an undo gives the content of the previous snapshot, a redo the content of the next one.
//!
//! After every step the derived indices must agree with a rebuild from scratch, and the tempo map
//! must be the one derived from the marks. Musical validity is not checked here: random edits
//! break it on purpose (a slur loses its end), and keeping it is the commands' job (part 2).

// Helper functions (not `#[test]` functions) are outside the scope of clippy.toml's
// `allow-unwrap-in-tests`, so the exemption is stated here, for this test module only: a helper
// that unwraps fails the calling test with a clear message, which is exactly what a test wants.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers: a failed unwrap or index fails the test"
)]
#![allow(
    clippy::too_many_lines,
    reason = "the rich document and the random edit are long lists of small, independent parts"
)]

use orchestra_core_time::{
    BarIndex, BarSpan, Duration, Meter, MeterChange, MeterMap, Rational, Tempo,
};
use proptest::prelude::*;

use crate::bar_repeat::BarRepeatSpan;
use crate::document::{Document, DocumentBuilder, DocumentError, Placement};
use crate::dynamic::{Dynamic, DynamicLevel, DynamicMark, DynamicPlacement, HairpinShape};
use crate::edit::{Edit, Initial, MeterEdit, Object, ObjectRef};
use crate::editor::{CommandError, Editor};
use crate::event::{BaseValue, Event, EventBody, NoteValue, PlacedEvent, Rest, RestValue, Tuplet};
use crate::form::{BarNumbering, PassSet};
use crate::id::{BarId, EventId, TupletId};
use crate::jump::{JumpFires, JumpSpec, JumpTarget, MarkerKind, RepeatsAfterJump};
use crate::marking::{Fermata, Grace, GraceAttach, GraceKind};
use crate::marks::{Clef, KeyOverride, KeySignature, Mark, ScorePoint};
use crate::pitch::{Alteration, Interval, Key, Mode, Pitch, Step};
use crate::tempo_mark::{GradualMark, Metronome, TempoMark, TempoMarkChange};

/// Shorthand: a duration of `n/d` whole notes.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// Shorthand: a quarter-note tempo of `bpm`.
fn tempo(bpm: i64) -> Tempo {
    Tempo::new(d(1, 4), Rational::from_integer(bpm).unwrap()).unwrap()
}

/// Shorthand: a pitch in octave 4.
const fn pitch(step: Step) -> Pitch {
    Pitch::new(step, Alteration::NATURAL, 4)
}

/// The rich document of the module documentation, 8 bars of 4/4.
pub(crate) fn rich() -> Document {
    let meters = MeterMap::new(MeterChange::new(Meter::new(&[4], 4).unwrap())).unwrap();
    let initial = TempoMark::immediate(Some("Allegro"), Metronome::shown(tempo(120))).unwrap();
    let mut b =
        DocumentBuilder::new(meters, initial, Key::traditional(0, Mode::Major).unwrap()).unwrap();
    let bars: Vec<BarId> = (0..8).map(|_| b.add_bar().unwrap()).collect();
    let unison = Interval::new(0, Rational::ZERO);
    let piano = b
        .add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS])
        .unwrap();
    let flute = b
        .add_instrument_track("Flute", unison, &[Clef::TREBLE])
        .unwrap();
    let (piano_id, rh, lh, lh_staff) = {
        let i = b.document().track(piano).unwrap().instrument();
        (
            i.id(),
            i.voices()[0].id(),
            i.voices()[1].id(),
            i.staves()[1].id(),
        )
    };
    let (flute_id, fl, fl_staff) = {
        let i = b.document().track(flute).unwrap().instrument();
        (i.id(), i.voices()[0].id(), i.staves()[0].id())
    };
    let quarter = NoteValue::plain(BaseValue::Quarter);
    let eighth = NoteValue::plain(BaseValue::Eighth);
    // Right hand, bar 0: four quarters, a chord first, two grace notes before the second.
    let first = b
        .add_chord(
            Placement::new(rh, bars[0], Duration::ZERO),
            quarter,
            &[pitch(Step::C), pitch(Step::E)],
        )
        .unwrap();
    let grace = Grace {
        kind: GraceKind::Acciaccatura,
        attach: GraceAttach::Before,
    };
    for step in [Step::A, Step::B] {
        b.add_grace_chord(
            Placement::new(rh, bars[0], d(1, 4)),
            grace,
            eighth,
            &[pitch(step)],
        )
        .unwrap();
    }
    let mut last = first.event;
    for (i, step) in [Step::D, Step::E, Step::F].into_iter().enumerate() {
        let offset = d(i64::try_from(i).unwrap() + 1, 4);
        last = b
            .add_chord(Placement::new(rh, bars[0], offset), quarter, &[pitch(step)])
            .unwrap()
            .event;
    }
    b.add_slur(first.event, last, crate::slur::SlurPlacement::Auto)
        .unwrap();
    // Right hand, bar 1: a triplet of eighths, then a half.
    let triplet = b
        .add_tuplet(rh, Tuplet::new(3, 2, eighth).unwrap(), None)
        .unwrap();
    for (i, step) in [Step::G, Step::A, Step::B].into_iter().enumerate() {
        let offset = d(i64::try_from(i).unwrap(), 12);
        b.add_chord(
            Placement::new(rh, bars[1], offset).in_tuplet(triplet),
            eighth,
            &[pitch(step)],
        )
        .unwrap();
    }
    // Left hand: half notes in bar 0, a whole-bar rest in bar 2; a clef change in bar 2.
    for i in 0..2 {
        b.add_chord(
            Placement::new(lh, bars[0], d(i, 2)),
            NoteValue::plain(BaseValue::Half),
            &[Pitch::new(Step::C, Alteration::NATURAL, 3)],
        )
        .unwrap();
    }
    b.add_rest(
        Placement::new(lh, bars[2], Duration::ZERO),
        RestValue::WholeBar,
    )
    .unwrap();
    b.add_clef_change(lh_staff, bars[2], Duration::ZERO, Clef::TREBLE)
        .unwrap();
    // Flute: a whole note in bar 3, a quarter in bar 4; bar 6 repeats bar 5 (empty).
    b.add_chord(
        Placement::new(fl, bars[3], Duration::ZERO),
        NoteValue::plain(BaseValue::Whole),
        &[pitch(Step::G)],
    )
    .unwrap();
    b.add_chord(
        Placement::new(fl, bars[4], Duration::ZERO),
        quarter,
        &[pitch(Step::A)],
    )
    .unwrap();
    b.add_bar_repeat(fl_staff, bars[6], 1, BarRepeatSpan::One)
        .unwrap();
    // Dynamics and a hairpin.
    let level = |l| DynamicMark::Level(l);
    b.add_dynamic(
        piano_id,
        ScorePoint::start_of(bars[0]),
        DynamicPlacement::instrument(lh_staff),
        level(DynamicLevel::Mf),
    )
    .unwrap();
    b.add_dynamic(
        flute_id,
        ScorePoint::start_of(bars[3]),
        DynamicPlacement::instrument(fl_staff),
        level(DynamicLevel::P),
    )
    .unwrap();
    b.add_hairpin(
        piano_id,
        ScorePoint::start_of(bars[0]),
        ScorePoint::start_of(bars[1]),
        DynamicPlacement::instrument(lh_staff),
        HairpinShape::CRESCENDO,
    )
    .unwrap();
    // Tempo: a change at bar 4, a gradual change from bar 5 to bar 6.
    b.add_tempo_mark(
        ScorePoint::start_of(bars[4]),
        TempoMark::immediate(Some("Meno mosso"), Metronome::shown(tempo(90))).unwrap(),
    )
    .unwrap();
    b.add_tempo_mark(
        ScorePoint::start_of(bars[5]),
        TempoMark::new(
            Some("accel."),
            Some(TempoMarkChange::Gradual(GradualMark {
                from: None,
                to: tempo(132),
                end: ScorePoint::start_of(bars[6]),
            })),
        )
        .unwrap(),
    )
    .unwrap();
    // Keys: G major from bar 4; the flute without signature from bar 5.
    b.add_key_change(
        bars[4],
        Duration::ZERO,
        Key::traditional(1, Mode::Major).unwrap(),
    )
    .unwrap();
    b.add_key_override(
        flute_id,
        bars[5],
        Duration::ZERO,
        KeyOverride::Signature(KeySignature::NoSignature),
    )
    .unwrap();
    // Form: bars 1–2 repeated, the piano tacet the first time; segno at bar 3, Fine at the end of
    // bar 4, D.S. al Fine at the end of bar 7.
    b.add_repeat_start(bars[1]).unwrap();
    b.add_repeat_end(bars[2], 2).unwrap();
    b.add_pass_rule(
        piano_id,
        ScorePoint::start_of(bars[1]),
        ScorePoint::start_of(bars[2]),
        crate::dynamic::DynamicScope::Instrument,
        PassSet::of(&[2]).unwrap(),
        None,
    )
    .unwrap();
    let segno = b.add_marker(bars[3], MarkerKind::Segno, None).unwrap();
    let fine = b.add_marker(bars[4], MarkerKind::Fine, None).unwrap();
    b.add_jump(
        bars[7],
        JumpSpec {
            to: JumpTarget::Marker(segno),
            until: Some(fine),
            then: None,
            repeats: RepeatsAfterJump::Skip,
            fires: JumpFires::AfterRepeats,
        },
        None,
    )
    .unwrap();
    b.build().unwrap()
}

/// Every object of a document that an `Edit::Remove` can name, in a stable order.
fn removable(doc: &Document) -> Vec<ObjectRef> {
    let mut all = Vec::new();
    for instrument in doc.instruments() {
        for voice in instrument.voices() {
            for bar in doc.bars().ids() {
                for placed in doc.events(voice.id(), *bar).unwrap() {
                    all.push(ObjectRef::Event(placed.event().id()));
                }
            }
            let mut tuplets: Vec<TupletId> = voice.content.tuplets.keys().copied().collect();
            tuplets.sort_unstable();
            all.extend(tuplets.into_iter().map(ObjectRef::Tuplet));
        }
        all.extend(instrument.slurs().iter().map(|s| ObjectRef::Slur(s.id())));
        all.extend(
            instrument
                .dynamics()
                .iter()
                .map(|x| ObjectRef::Dynamic(x.id())),
        );
        all.extend(
            instrument
                .hairpins()
                .iter()
                .map(|x| ObjectRef::Hairpin(x.id())),
        );
        all.extend(
            instrument
                .bar_repeats()
                .iter()
                .map(|x| ObjectRef::BarRepeat(x.id())),
        );
        all.extend(
            instrument
                .pass_rules()
                .iter()
                .map(|x| ObjectRef::PassRule(x.id())),
        );
        all.extend(
            instrument
                .key_overrides()
                .as_slice()
                .iter()
                .map(|m| ObjectRef::KeyOverride(m.id())),
        );
        for staff in instrument.staves() {
            all.extend(
                staff
                    .clef_changes()
                    .as_slice()
                    .iter()
                    .map(|m| ObjectRef::ClefChange(m.id())),
            );
        }
    }
    all.extend(
        doc.tempo_marks()
            .as_slice()
            .iter()
            .map(|m| ObjectRef::TempoMark(m.id())),
    );
    all.extend(
        doc.key_changes()
            .as_slice()
            .iter()
            .map(|m| ObjectRef::KeyChange(m.id())),
    );
    let form = doc.form();
    all.extend(
        form.repeat_starts()
            .iter()
            .map(|r| ObjectRef::RepeatStart(r.id())),
    );
    all.extend(
        form.repeat_ends()
            .iter()
            .map(|r| ObjectRef::RepeatEnd(r.id())),
    );
    all.extend(form.voltas().iter().map(|v| ObjectRef::Volta(v.id())));
    all.extend(form.markers().iter().map(|m| ObjectRef::Marker(m.id())));
    all.extend(form.jumps().iter().map(|j| ObjectRef::Jump(j.id())));
    all
}

/// The events of a document, in a stable order.
fn events(doc: &Document) -> Vec<EventId> {
    removable(doc)
        .into_iter()
        .filter_map(|r| match r {
            ObjectRef::Event(id) => Some(id),
            _ => None,
        })
        .collect()
}

/// The checks every state must pass: indices rebuilt from scratch agree, and the tempo map is the
/// one derived from the marks.
pub(crate) fn assert_consistent(doc: &Document) {
    assert!(doc.indices_are_consistent(), "a derived index drifted");
    assert_eq!(doc.tempo_map(), &doc.derive_tempo_map().unwrap());
    assert_round_trip(doc);
}

/// The round trip of a saved document (ADR-0029 S4, owner's choice L8 of 2026-11-06): the
/// description of a valid document builds back a document with the same content, the same IDs
/// and the same counter, whose description is identical; the description of an **invalid** one
/// (the raw edits of the snapshot test may make one: they keep only the structure) is refused, so
/// a file can never bring in what `validate` forbids. Called by [`assert_consistent`], so that
/// every property test of the commands and the edits checks it after every step.
pub(crate) fn assert_round_trip(doc: &Document) {
    let parts = doc.to_parts();
    if let Err(invalid) = doc.validate() {
        assert!(
            Document::from_parts(&parts).is_err(),
            "an invalid document ({invalid:?}) loaded"
        );
        return;
    }
    let back = match Document::from_parts(&parts) {
        Ok(back) => back,
        Err(e) => panic!("a valid document did not load back: {e:?}"),
    };
    assert!(back.same_content(doc), "the loaded document differs");
    assert!(back.indices_are_consistent(), "the loaded indices differ");
    assert_eq!(back.to_parts(), parts, "the loaded description differs");
    assert_eq!(back.revision(), 0, "a loaded document starts at revision 0");
}

/// One random edit, chosen by `op` and parametrised by `a` and `b`, built against the document as
/// it is inside the transaction. `pool` holds objects removed earlier, to be inserted again.
fn random_edit(
    tx: &mut crate::editor::Transaction<'_>,
    pool: &[Object],
    op: u8,
    a: usize,
    b: usize,
) -> Option<Edit> {
    // A fresh ID for the edits that create a notehead, taken before the document is borrowed. An
    // unused one is simply spent, as in a reverted transaction.
    let fresh = tx.new_id().ok()?;
    let doc = tx.document();
    let bars = doc.bars().ids().to_vec();
    match op % 10 {
        // Remove any object.
        0 => {
            let all = removable(doc);
            (!all.is_empty()).then(|| Edit::Remove(all[a % all.len()]))
        }
        // Insert again an object removed earlier (often refused: its bar or ID may be gone).
        1 => (!pool.is_empty()).then(|| Edit::Insert(pool[a % pool.len()].clone())),
        // Replace an event: a new fermata (in place), a later offset, or another bar.
        2 => {
            let all = events(doc);
            let id = *all.get(a % all.len().max(1))?;
            let found = doc.event(id)?;
            let mut event: Event = found.event().clone();
            let (bar, offset) = match b % 4 {
                // New noteheads in place: keep the lowest one, add a new one above it.
                3 => {
                    let chord = found.event().chord()?;
                    let lowest = *chord.notes().next()?;
                    let added = crate::event::Note::new(
                        crate::id::NoteId::from_counter(fresh),
                        Pitch::new(Step::G, Alteration::NATURAL, 6),
                    );
                    let changed = crate::event::Chord::from_sorted(
                        chord.value(),
                        chord.tuplet(),
                        chord.grace(),
                        lowest,
                        Box::new([added]),
                    );
                    event = Event::new(id, event.staff(), EventBody::Chord(changed));
                    (found.bar(), found.offset())
                }
                0 => {
                    let fermata = event.fermata().map_or(Some(Fermata::Normal), |_| None);
                    event.set_fermata(fermata);
                    (found.bar(), found.offset())
                }
                1 => (found.bar(), found.offset().checked_add(d(1, 8)).ok()?),
                _ => (bars[b % bars.len()], found.offset()),
            };
            Some(Edit::Replace(Object::Event {
                voice: found.voice(),
                bar,
                placed: PlacedEvent::new(offset, event),
                place: None,
            }))
        }
        // Move a dynamic or a tempo mark to another point.
        3 => {
            let point = ScorePoint::new(bars[a % bars.len()], d(i64::try_from(b % 4).ok()?, 4));
            if b.is_multiple_of(2) {
                let instrument = doc.instruments().nth(a % 2)?;
                let dynamic = instrument.dynamics().first()?;
                Some(Edit::Replace(Object::Dynamic {
                    instrument: instrument.id(),
                    dynamic: Dynamic::new(
                        dynamic.id(),
                        point,
                        dynamic.placement(),
                        dynamic.mark().clone(),
                    ),
                }))
            } else {
                let mark = doc.tempo_marks().as_slice().first()?;
                Some(Edit::Replace(Object::TempoMark(Mark::new(
                    mark.id(),
                    point.bar,
                    point.offset,
                    mark.value().clone(),
                ))))
            }
        }
        // Insert one or two empty bars.
        4 => {
            let count = 1 + b % 2;
            let ids = (0..count)
                .map(|_| tx.new_id().map(BarId::from_counter))
                .collect::<Result<Vec<_>, _>>()
                .ok()?;
            let at = u32::try_from(a % (bars.len() + 1)).ok()?;
            let mut meter = BarSpan::empty(u32::try_from(count).ok()?);
            if at == 0 {
                // The map needs a meter at bar 0 (see `MeterMap::insert_bars`).
                meter
                    .set_meter_change(BarIndex::FIRST, MeterChange::new(Meter::new(&[4], 4).ok()?));
            }
            Some(Edit::InsertBars {
                at: BarIndex::new(at),
                ids,
                meter,
            })
        }
        // Remove one or two bars (refused unless they carry nothing).
        5 => Some(Edit::RemoveBars {
            at: BarIndex::new(u32::try_from(a % bars.len()).ok()?),
            count: u32::try_from(1 + b % 2).ok()?,
        }),
        // Edit the meter map.
        6 => {
            let bar = bars[a % bars.len()];
            Some(Edit::Meter(match b % 4 {
                0 => MeterEdit::MeterChange {
                    bar,
                    change: Some(MeterChange::new(Meter::new(&[3], 4).ok()?)),
                },
                1 => MeterEdit::MeterChange { bar, change: None },
                2 => MeterEdit::Uncounted {
                    bar,
                    uncounted: a.is_multiple_of(2),
                },
                _ => MeterEdit::NumberingRestart {
                    bar,
                    number: Some(u32::try_from(b).ok()? + 1),
                },
            }))
        }
        // Set an initial value.
        7 => Some(Edit::SetInitial(match b % 4 {
            0 => Initial::Key(Key::traditional(i8::try_from(a % 5).ok()? - 2, Mode::Major).ok()?),
            1 => Initial::Clef {
                staff: doc.instruments().next()?.staves()[0].id(),
                clef: [Clef::TREBLE, Clef::BASS][a % 2],
            },
            2 => Initial::Numbering(
                [
                    BarNumbering::Continue,
                    BarNumbering::SameAsFirst,
                    BarNumbering::SameAsFirstLettered,
                ][a % 3],
            ),
            _ => Initial::Tempo(
                TempoMark::immediate(
                    None,
                    Metronome::shown(tempo(60 + i64::try_from(a % 60).ok()?)),
                )
                .ok()?,
            ),
        })),
        // Insert a new rest in a random voice and bar.
        8 => {
            let instrument = doc.instruments().nth(a % 2)?;
            let voice = &instrument.voices()[b % instrument.voices().len()];
            let (voice_id, staff) = (voice.id(), voice.home_staff());
            let bar = bars[a % bars.len()];
            let offset = d(i64::try_from(b % 4).ok()?, 4);
            let id = EventId::from_counter(tx.new_id().ok()?);
            let rest = Rest::new(RestValue::Value(NoteValue::plain(BaseValue::Quarter)), None);
            Some(Edit::Insert(Object::Event {
                voice: voice_id,
                bar,
                placed: PlacedEvent::new(offset, Event::new(id, staff, EventBody::Rest(rest))),
                place: None,
            }))
        }
        // Replace or remove the tuplet.
        _ => {
            let instrument = doc.instruments().next()?;
            let voice = &instrument.voices()[0];
            let (&id, tuplet) = voice.content.tuplets.iter().next()?;
            if b.is_multiple_of(2) {
                let mut changed = Tuplet::new(5, 4, tuplet.unit()).ok()?;
                changed.set_parent(tuplet.parent());
                Some(Edit::Replace(Object::Tuplet {
                    voice: voice.id(),
                    id,
                    tuplet: changed,
                }))
            } else {
                Some(Edit::Remove(ObjectRef::Tuplet(id)))
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Random transactions, undos and redos against the snapshot model of the module docs.
    #[test]
    fn undo_and_redo_follow_the_snapshots(
        steps in prop::collection::vec(
            (
                prop::collection::vec((0_u8..10, 0_usize..64, 0_usize..64), 1..5),
                any::<bool>(),
                prop::collection::vec(0_u8..3, 0..3),
            ),
            1..8,
        ),
    ) {
        let mut editor = Editor::new(rich());
        let mut snapshots = vec![editor.document().clone()];
        let mut cursor = 0_usize;
        let mut pool: Vec<Object> = Vec::new();
        for (edits, fail, actions) in steps {
            let mut removed = Vec::new();
            let result = editor.transact(|tx| {
                for &(op, a, b) in &edits {
                    let Some(edit) = random_edit(tx, &pool, op, a, b) else { continue };
                    // A refused edit changes nothing; the transaction goes on.
                    if let (Ok(()), Edit::Remove(name)) = (tx.apply(edit.clone()), &edit) {
                        removed.push(*name);
                    }
                }
                if fail { Err(CommandError::Document(DocumentError::Inconsistent)) } else { Ok(()) }
            });
            let before = snapshots[cursor].clone();
            match result {
                Ok(changes) if !changes.is_empty() => {
                    prop_assert!(!fail);
                    snapshots.truncate(cursor + 1);
                    snapshots.push(editor.document().clone());
                    cursor += 1;
                    prop_assert_eq!(editor.redo_steps(), 0);
                    // Remember what was removed, to try inserting it again later.
                    for name in removed {
                        if let Some(object) = removed_object(&before, name) {
                            pool.push(object);
                        }
                    }
                }
                Ok(_) => prop_assert!(editor.document().same_content(&before)),
                Err(e) => {
                    // Besides a closure that asks to fail, the tempo map derived at the end may
                    // refuse the marks (a mark moved inside a gradual change): then the whole
                    // transaction is reverted too.
                    let tempo_refused = matches!(
                        e,
                        CommandError::Document(DocumentError::Invalid(_) | DocumentError::TempoMap(_))
                    );
                    prop_assert!(fail || tempo_refused, "unexpected error {:?}", e);
                    prop_assert!(editor.document().same_content(&before));
                }
            }
            assert_consistent(editor.document());
            for action in actions {
                match action {
                    1 => {
                        let undone = editor.undo().unwrap();
                        prop_assert_eq!(undone.is_some(), cursor > 0);
                        if undone.is_some() {
                            cursor -= 1;
                        }
                    }
                    2 => {
                        let redone = editor.redo().unwrap();
                        prop_assert_eq!(redone.is_some(), cursor + 1 < snapshots.len());
                        if redone.is_some() {
                            cursor += 1;
                        }
                    }
                    _ => {}
                }
                // What happens after a transaction: 0 nothing, 1 undo, 2 redo (see above).
                prop_assert!(editor.document().same_content(&snapshots[cursor]));
                assert_consistent(editor.document());
            }
        }
        // Undoing everything gives back the original document.
        while editor.undo().unwrap().is_some() {}
        prop_assert!(editor.document().same_content(&snapshots[0]));
    }
}

/// The object `name` as it was in `doc`, rebuilt the way a removal would return it: obtained by
/// removing it from a copy (which also checks that removal works on any state).
fn removed_object(doc: &Document, name: ObjectRef) -> Option<Object> {
    let mut copy = doc.clone();
    match copy.apply_edit(Edit::Remove(name), &mut crate::change::ChangeSet::default()) {
        Ok(Edit::Insert(object)) => Some(object),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------------------
// Hand-written cases
// ------------------------------------------------------------------------------------------------

/// The rich document is valid, and so is every state of its indices.
#[test]
fn the_rich_document_is_valid() {
    let doc = rich();
    doc.validate().unwrap();
    assert_consistent(&doc);
}

/// Removing the first of two grace notes at one offset and undoing it puts it back **before** the
/// other one, not after: the place in the chunk is part of the inverse.
#[test]
fn undo_restores_the_order_of_grace_notes() {
    let mut editor = Editor::new(rich());
    let original = editor.document().clone();
    let (voice, bar) = {
        let i = original.instruments().next().unwrap();
        (i.voices()[0].id(), original.bars().ids()[0])
    };
    let graces: Vec<EventId> = original
        .events(voice, bar)
        .unwrap()
        .iter()
        .filter(|e| e.event().grace().is_some())
        .map(|e| e.event().id())
        .collect();
    assert_eq!(graces.len(), 2);
    let changes = editor
        .transact(|tx| tx.apply(Edit::Remove(ObjectRef::Event(graces[0]))))
        .unwrap();
    assert_eq!(changes.cells(), &[(voice, bar)]);
    assert_eq!(changes.removed(), &[ObjectRef::Event(graces[0])]);
    editor.undo().unwrap().unwrap();
    assert!(editor.document().same_content(&original));
    assert_eq!(editor.document().revision(), 2);
}

/// A transaction whose closure fails leaves the document unchanged and records nothing.
#[test]
fn a_failed_transaction_changes_nothing() {
    let mut editor = Editor::new(rich());
    let original = editor.document().clone();
    let first = events(&original)[0];
    let result = editor.transact(|tx| {
        tx.apply(Edit::Remove(ObjectRef::Event(first)))?;
        // Removing it twice is refused: the closure returns the error.
        tx.apply(Edit::Remove(ObjectRef::Event(first)))
    });
    assert_eq!(
        result,
        Err(CommandError::Document(DocumentError::UnknownObject(
            ObjectRef::Event(first)
        )))
    );
    assert!(editor.document().same_content(&original));
    assert_eq!(editor.undo_steps(), 0);
    assert_eq!(editor.document().revision(), 0);
}

/// A transaction that applies nothing records no step and changes nothing.
#[test]
fn an_empty_transaction_records_nothing() {
    let mut editor = Editor::new(rich());
    let changes = editor.transact(|_| Ok(())).unwrap();
    assert!(changes.is_empty());
    assert_eq!(editor.undo_steps(), 0);
    assert_eq!(editor.undo().unwrap(), None);
}

/// Moving a tempo mark reports where the tempo map changed; inserting bars reports where the
/// meter map changed and that the form changed (bars move the path).
#[test]
fn change_sets_name_the_maps() {
    let mut editor = Editor::new(rich());
    let doc = editor.document().clone();
    let mark = doc.tempo_marks().as_slice()[0].clone();
    let bar2 = doc.bars().ids()[2];
    let changes = editor
        .transact(|tx| {
            tx.apply(Edit::Replace(Object::TempoMark(Mark::new(
                mark.id(),
                bar2,
                Duration::ZERO,
                mark.value().clone(),
            ))))
        })
        .unwrap();
    assert_eq!(changes.replaced(), &[ObjectRef::TempoMark(mark.id())]);
    assert_eq!(
        changes.tempo_from(),
        Some(doc.bar_start(bar2).unwrap()),
        "the change moved earlier, to bar 2"
    );
    assert!(changes.meter_from().is_none());

    let changes = editor
        .transact(|tx| {
            let id = BarId::from_counter(tx.new_id()?);
            tx.apply(Edit::InsertBars {
                at: BarIndex::new(3),
                ids: vec![id],
                meter: BarSpan::empty(1),
            })
        })
        .unwrap();
    assert_eq!(changes.meter_from(), Some(BarIndex::new(3)));
    assert!(changes.form_changed());
    assert!(matches!(changes.inserted(), [ObjectRef::Bar(_)]));
    // Every voice gained a chunk; the tempo marks after bar 3 moved one bar later in time.
    assert_consistent(editor.document());
    assert!(changes.tempo_from().is_some());
}

/// Bars that hold an event, or on which something is anchored, are not removed.
#[test]
fn busy_bars_are_not_removed() {
    let mut editor = Editor::new(rich());
    let doc = editor.document().clone();
    let bars = doc.bars().ids().to_vec();
    // Bar 0 holds notes; bar 6 carries the end of the gradual tempo change and a bar repeat.
    for (at, bar) in [(0, bars[0]), (6, bars[6])] {
        let result = editor.transact(|tx| {
            tx.apply(Edit::RemoveBars {
                at: BarIndex::new(at),
                count: 1,
            })
        });
        assert_eq!(
            result,
            Err(CommandError::Document(DocumentError::BarsNotEmpty(bar)))
        );
    }
    assert!(editor.document().same_content(&doc));
}

/// An object whose ID was never handed out is refused, so that it can never collide with a later
/// object.
#[test]
fn unissued_ids_are_refused() {
    let mut editor = Editor::new(rich());
    let doc = editor.document().clone();
    let instrument = doc.instruments().next().unwrap();
    let voice = &instrument.voices()[0];
    let future = EventId::from_counter(doc.ids.peek().unwrap());
    let rest = Rest::new(RestValue::Value(NoteValue::plain(BaseValue::Quarter)), None);
    let result = editor.transact(|tx| {
        tx.apply(Edit::Insert(Object::Event {
            voice: voice.id(),
            bar: doc.bars().ids()[5],
            placed: PlacedEvent::new(
                Duration::ZERO,
                Event::new(future, voice.home_staff(), EventBody::Rest(rest)),
            ),
            place: None,
        }))
    });
    assert_eq!(
        result,
        Err(CommandError::Document(DocumentError::DuplicateObject(
            ObjectRef::Event(future)
        )))
    );
}

/// The history drops its oldest steps when it passes its memory limit, but always keeps the most
/// recent one; a new transaction after an undo discards the redo step.
#[test]
fn the_history_is_bounded_and_linear() {
    // A limit smaller than one step: only the last step can stay.
    let mut editor = Editor::with_history_limit(rich(), 1);
    let ids = events(editor.document());
    for id in &ids[..3] {
        editor
            .transact(|tx| tx.apply(Edit::Remove(ObjectRef::Event(*id))))
            .unwrap();
    }
    assert_eq!(editor.undo_steps(), 1);
    assert!(editor.history_bytes() > editor.history_limit());
    editor.undo().unwrap().unwrap();
    assert_eq!((editor.undo_steps(), editor.redo_steps()), (0, 1));
    editor
        .transact(|tx| tx.apply(Edit::Remove(ObjectRef::Event(ids[3]))))
        .unwrap();
    assert_eq!((editor.undo_steps(), editor.redo_steps()), (1, 0));
}

/// Removing a tuplet that events still use is refused; the meter map refuses to lose its meter at
/// bar 0; a value the document has once cannot be removed.
#[test]
fn structural_refusals() {
    let mut editor = Editor::new(rich());
    let doc = editor.document().clone();
    let voice = &doc.instruments().next().unwrap().voices()[0];
    let (&tuplet, _) = voice.content.tuplets.iter().next().unwrap();
    let first = doc.bars().ids()[0];
    let cases = [
        (
            Edit::Remove(ObjectRef::Tuplet(tuplet)),
            DocumentError::TupletInUse(tuplet),
        ),
        (
            Edit::Meter(MeterEdit::MeterChange {
                bar: first,
                change: None,
            }),
            DocumentError::MeterMap(orchestra_core_time::MeterMapError::NoMeterAtStart),
        ),
        (
            Edit::Remove(ObjectRef::InitialKey),
            DocumentError::NotRemovable(ObjectRef::InitialKey),
        ),
    ];
    for (edit, error) in cases {
        assert_eq!(
            editor.transact(|tx| tx.apply(edit.clone())),
            Err(CommandError::Document(error))
        );
    }
    assert!(editor.document().same_content(&doc));
}
