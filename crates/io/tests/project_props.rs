// SPDX-License-Identifier: GPL-3.0-or-later

//! Robustness and round-trip properties of the project file (ADR-0029 S8, S12; owner's choice
//! ADR-0029 I10 of 2026-11-06).
//!
//! - **Truncations**: every prefix of the "everything" file is refused with a typed error; none
//!   panics.
//! - **Corruption**: random bytes changed anywhere never panic; when a corrupted file still loads
//!   (a changed digit of a valid offset, say), the document is valid and saves and loads again.
//! - **Random edits**: random commands on the "everything" document (enter, delete, change the
//!   offsets and the ties of notes) keep the file's round trip exact after every command: same
//!   bytes after a second save, same content, same IDs and counter.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test helpers: a failed unwrap, index or panic fails the test"
)]

#[path = "common/everything.rs"]
mod everything;

use orchestra_core_model::{
    Alteration, BaseValue, Command, Document, Editor, EventChange, EventId, NewBody, NewEvent,
    Note, NoteId, NoteValue, PerformanceOffsets, Pitch, Placement, Step, Tie, Voice, VoiceId,
};
use orchestra_core_time::{BarIndex, Duration, Rational};
use orchestra_io::{from_bytes, to_bytes};
use proptest::prelude::*;

/// Saves and loads, checking that nothing changes.
fn assert_round_trip(doc: &Document) {
    let bytes = to_bytes(doc).unwrap();
    let back = from_bytes(&bytes).unwrap();
    assert!(back.same_content(doc));
    assert_eq!(back.to_parts(), doc.to_parts());
    assert_eq!(to_bytes(&back).unwrap(), bytes);
}

/// Every prefix of the file is refused, and none panics.
#[test]
fn every_truncation_is_refused() {
    let bytes = to_bytes(&everything::everything()).unwrap();
    // The full file minus its final newline still loads (whitespace); anything shorter cuts JSON.
    for len in 0..bytes.len().saturating_sub(1) {
        assert!(
            from_bytes(&bytes[..len]).is_err(),
            "a prefix of {len} bytes loaded"
        );
    }
}

/// Every voice of a document.
fn voices(doc: &Document) -> Vec<VoiceId> {
    doc.tracks()
        .iter()
        .flat_map(|t| t.instrument().voices().iter().map(Voice::id))
        .collect()
}

/// Every event of a document, and every notehead.
fn events_and_notes(doc: &Document) -> (Vec<EventId>, Vec<NoteId>) {
    let mut events = Vec::new();
    let mut notes = Vec::new();
    for voice in voices(doc) {
        for &bar in doc.bars().ids() {
            for placed in doc.events(voice, bar).unwrap() {
                events.push(placed.event().id());
                if let Some(chord) = placed.event().chord() {
                    notes.extend(chord.notes().map(Note::id));
                }
            }
        }
    }
    (events, notes)
}

/// One random command (`None` if the document has nothing to apply it to).
fn command(doc: &Document, op: u8, a: usize, b: usize) -> Option<Command> {
    let (events, notes) = events_and_notes(doc);
    let pick = |n: usize| (n > 0).then(|| a % n);
    Some(match op % 4 {
        0 => {
            let voices = voices(doc);
            let bars = doc.bars().len();
            let bar = doc
                .bars()
                .id_at(BarIndex::new(u32::try_from(b % bars).ok()?))?;
            let offset =
                Duration::from_rational(Rational::new(i64::try_from(b % 4).ok()?, 8).ok()?);
            Command::EnterEvent(NewEvent {
                at: Placement::new(voices[pick(voices.len())?], bar, offset),
                body: NewBody::Chord {
                    value: NoteValue::plain(BaseValue::Eighth),
                    pitches: vec![Pitch::new(Step::ALL[b % 7], Alteration::NATURAL, 4)],
                    grace: None,
                },
            })
        }
        1 => Command::DeleteEvent(events[pick(events.len())?]),
        2 => {
            let note = notes[pick(notes.len())?];
            let (event, _) = doc.note(note)?;
            Command::ChangeEvent {
                event: event.event().id(),
                change: EventChange::Offsets {
                    note,
                    offsets: PerformanceOffsets {
                        onset: Duration::from_rational(
                            Rational::new(i64::try_from(b % 5).ok()? - 2, 64).ok()?,
                        ),
                        length: Duration::ZERO,
                        velocity: i32::try_from(b % 7).ok()? * 100 - 300,
                    },
                },
            }
        }
        _ => {
            let note = notes[pick(notes.len())?];
            let (event, _) = doc.note(note)?;
            Command::ChangeEvent {
                event: event.event().id(),
                change: EventChange::Tie {
                    note,
                    tie: if b.is_multiple_of(2) {
                        Tie::LetRing
                    } else {
                        Tie::None
                    },
                },
            }
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Random bytes changed anywhere: never a panic; what still loads is valid and round-trips.
    #[test]
    fn corruption_never_panics(
        changes in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..8)
    ) {
        let mut bytes = to_bytes(&everything::everything()).unwrap();
        for (index, byte) in changes {
            let i = index.index(bytes.len());
            bytes[i] = byte;
        }
        if let Ok(doc) = from_bytes(&bytes) {
            prop_assert!(doc.validate().is_ok());
            assert_round_trip(&doc);
        }
    }

    /// Random commands keep the round trip exact after every one.
    #[test]
    fn edited_documents_round_trip(
        steps in prop::collection::vec((0_u8..4, 0_usize..256, 0_usize..256), 1..12)
    ) {
        let mut editor = Editor::new(everything::everything());
        for (op, a, b) in steps {
            let Some(command) = command(editor.document(), op, a, b) else { continue };
            if editor.execute(&command).is_ok() {
                assert_round_trip(editor.document());
            }
        }
    }
}
