// SPDX-License-Identifier: GPL-3.0-or-later

//! Property test of the incremental update of the played notes (ADR-0027 §9, §11 and §14, H11 and
//! H12; SCALETTA F1.13c part 2): on random scores with every extra and dynamics, a random sequence
//! of commands of the editor (entering, deleting, moving and changing events, ties, articulations,
//! offsets, dynamics, hairpins, inserted bars, DAW moves of every pass or of one pass, DAW resizes
//! and moves split at barlines, ADR-0028)
//! with undos and redos; after each one the path is
//! updated, then the performance, and the result must be **identical** to a performance computed
//! from scratch on the new document: the same notes in the same order, the same extras, the same
//! timelines.

#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test helpers over their own small tables: a failure fails the test"
)]

mod common;

use common::{Extras, random};
use orchestra_core_model::{
    Alteration, Articulation, ArticulationSet, BarId, BaseValue, Command, Document, DynamicLevel,
    DynamicMark, DynamicPlacement, Editor, EventChange, EventId, Grace, GraceAttach, GraceKind,
    HairpinShape, MarkSpec, MoveMode, MovePlayed, NewBody, NewEvent, NoteId, NoteValue, ObjectRef,
    PerformanceOffsets, Pitch, Placement, ResizePlayed, ScorePoint, Step, Tie,
};
use orchestra_core_time::{BarIndex, Duration, Rational};
use orchestra_playback::{Performance, Unrolled};
use proptest::prelude::*;

/// `n/den` of a whole note.
fn d(n: i64, den: i64) -> Duration {
    Duration::from_rational(Rational::new(n, den).unwrap())
}

/// Every event of the instrument's voice, in bar order, with the first notehead of chords.
fn events(doc: &Document) -> Vec<(BarId, EventId, Option<NoteId>)> {
    let voice = doc.tracks()[0].instrument().voices()[0].id();
    let mut out = Vec::new();
    for &bar in doc.bars().ids() {
        for placed in doc.events(voice, bar).unwrap() {
            let event = placed.event();
            let note = event
                .chord()
                .and_then(|c| c.notes().next())
                .map(orchestra_core_model::Note::id);
            out.push((bar, event.id(), note));
        }
    }
    out
}

/// A random command from three random bytes, or `None` for an undo (`Some(None)`: a redo is
/// signalled separately by the caller). The commands may be refused by the editor (an overlap, a
/// tie between different pitches…): refused ones are skipped, which is also what an interface
/// would do.
#[allow(
    clippy::too_many_lines,
    reason = "one random choice per kind of command"
)]
fn command(doc: &Document, kind: u8, x: u8, y: u8) -> Option<Command> {
    let instrument = doc.tracks()[0].instrument();
    let (voice, staff, piano) = (
        instrument.voices()[0].id(),
        instrument.staves()[0].id(),
        instrument.id(),
    );
    let bars = doc.bars().ids();
    let bar = bars[usize::from(x) % bars.len()];
    let list = events(doc);
    let pick = |k: u8| list.get(usize::from(k) % list.len().max(1)).copied();
    let eighth = d(i64::from(y % 8), 8);
    let beat = d(i64::from(y % 4), 4);
    Some(match kind % 15 {
        0 => Command::DeleteEvent(pick(x)?.1),
        1 => Command::ChangeEvent {
            event: pick(x)?.1,
            change: EventChange::Articulations(match y % 4 {
                0 => ArticulationSet::EMPTY,
                1 => ArticulationSet::EMPTY.with(Articulation::Accent),
                2 => ArticulationSet::EMPTY.with(Articulation::Staccato),
                _ => ArticulationSet::EMPTY.with(Articulation::Marcato),
            }),
        },
        2 => {
            // A tie from one note to the next event's first note (refused if the pitches
            // differ or it does not start later), or a tie removed.
            let i = usize::from(x) % list.len().max(1);
            let (_, _, from) = *list.get(i)?;
            let to = list.get(i + 1).and_then(|e| e.2);
            Command::ChangeEvent {
                event: list.get(i)?.1,
                change: EventChange::Tie {
                    note: from?,
                    tie: match to {
                        Some(to) if !y.is_multiple_of(3) => Tie::To(to),
                        _ => Tie::None,
                    },
                },
            }
        }
        3 => Command::ChangeEvent {
            event: pick(x)?.1,
            change: if y.is_multiple_of(2) {
                EventChange::Value(NoteValue::plain(match y % 6 {
                    0 => BaseValue::Eighth,
                    2 => BaseValue::Quarter,
                    _ => BaseValue::Half,
                }))
            } else {
                EventChange::Move {
                    bar,
                    offset: eighth,
                }
            },
        },
        4 => Command::EnterEvent(NewEvent {
            at: Placement::new(voice, bar, eighth),
            body: NewBody::Chord {
                value: NoteValue::plain(BaseValue::Eighth),
                pitches: vec![Pitch::new(Step::E, Alteration::NATURAL, 4)],
                grace: match y % 3 {
                    0 => Some(Grace {
                        kind: GraceKind::Acciaccatura,
                        attach: GraceAttach::Before,
                    }),
                    1 => Some(Grace {
                        kind: GraceKind::Appoggiatura,
                        attach: GraceAttach::Before,
                    }),
                    _ => None,
                },
            },
        }),
        5 => {
            let (_, event, note) = pick(x)?;
            Command::ChangeEvent {
                event,
                change: EventChange::Offsets {
                    note: note?,
                    offsets: PerformanceOffsets {
                        onset: d(i64::from(y % 5) - 2, 64),
                        length: d(i64::from(y % 7) - 3, 16),
                        velocity: i32::from(y) * 300 - 30_000,
                    },
                },
            }
        }
        6 => Command::AddMark(MarkSpec::Dynamic {
            instrument: piano,
            at: ScorePoint::new(bar, beat),
            placement: DynamicPlacement::instrument(staff),
            mark: match y % 5 {
                0 => DynamicMark::Level(DynamicLevel::Pp),
                1 => DynamicMark::Level(DynamicLevel::Ff),
                2 => DynamicMark::Sfz,
                3 => DynamicMark::Fp,
                _ => DynamicMark::Level(DynamicLevel::Mp),
            },
        }),
        7 => {
            let i = usize::from(x) % bars.len();
            let end = if y.is_multiple_of(2) && i + 1 < bars.len() {
                ScorePoint::new(bars[i + 1], Duration::ZERO)
            } else {
                ScorePoint::new(bar, d(3, 4))
            };
            Command::AddMark(MarkSpec::Hairpin {
                instrument: piano,
                start: ScorePoint::new(bar, beat),
                end,
                placement: DynamicPlacement::instrument(staff),
                shape: if y.is_multiple_of(3) {
                    HairpinShape::DIMINUENDO
                } else {
                    HairpinShape::CRESCENDO
                },
            })
        }
        8 => {
            // Removes a dynamic or a hairpin, if there is one.
            let dynamics = instrument.dynamics();
            let hairpins = instrument.hairpins();
            if y.is_multiple_of(2) && !dynamics.is_empty() {
                Command::RemoveMark(ObjectRef::Dynamic(
                    dynamics[usize::from(x) % dynamics.len()].id(),
                ))
            } else if !hairpins.is_empty() {
                Command::RemoveMark(ObjectRef::Hairpin(
                    hairpins[usize::from(x) % hairpins.len()].id(),
                ))
            } else {
                return None;
            }
        }
        9 => Command::ChangeEvent {
            event: pick(x)?.1,
            change: EventChange::AddNote(Pitch::new(Step::G, Alteration::NATURAL, 5)),
        },
        // A DAW move of one pass (ADR-0028 §5): a pass-offset entry, recomputed through the cell
        // of its note (T7).
        10 => Command::MovePlayed(MovePlayed {
            note: pick(x)?.2?,
            pass: Some(u32::from(y % 3)),
            onset: d(i64::from(y % 16) - 8, 64),
            grid: d(1, 16),
            mode: MoveMode::ByThreshold,
        }),
        // A DAW move of every pass: an offset or a written move, by the threshold.
        11 => Command::MovePlayed(MovePlayed {
            note: pick(x)?.2?,
            pass: None,
            onset: d(i64::from(y % 32) - 16, 32),
            grid: d(1, 16),
            mode: MoveMode::ByThreshold,
        }),
        // A DAW resize (ADR-0028 §11): a length offset, or the tie chain rewritten.
        12 => Command::ResizePlayed(ResizePlayed {
            note: pick(x)?.2?,
            pass: None,
            length: d(i64::from(y % 32) - 16, 32),
            scale: Rational::ONE,
            grid: d(1, 16),
            mode: MoveMode::ByThreshold,
        }),
        // "Move anyway" (U5): split at the barlines.
        13 => Command::MovePlayed(MovePlayed {
            note: pick(x)?.2?,
            pass: None,
            onset: d(i64::from(y % 32) - 16, 16),
            grid: d(1, 16),
            mode: MoveMode::SplitAtBarlines,
        }),
        _ => Command::InsertBars {
            at: BarIndex::new(u32::from(x) % 3),
            count: 1,
        },
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(384))]

    /// After every command, undo or redo, the updated performance is the performance computed
    /// from scratch.
    #[test]
    fn an_update_equals_a_full_computation(
        n in 3_usize..7,
        seed in prop::collection::vec(any::<u8>(), 8..64),
        ops in prop::collection::vec((any::<u8>(), any::<u8>(), any::<u8>(), any::<u8>()), 1..16),
    ) {
        let doc = random(n, &seed, Extras { any: true, dynamics: true });
        let mut editor = Editor::new(doc);
        let mut path = Unrolled::new(editor.document()).unwrap();
        let mut performance = Performance::new(editor.document(), &path).unwrap();
        for (what, kind, x, y) in ops {
            let changes = match what % 8 {
                0 => editor.undo().unwrap(),
                1 => editor.redo().unwrap(),
                _ => match command(editor.document(), kind, x, y) {
                    Some(c) => editor.execute(&c).ok(),
                    None => None,
                },
            };
            let Some(changes) = changes else { continue };
            let doc = editor.document();
            path.update(doc, &changes).unwrap();
            performance.update(doc, &path, &changes).unwrap();
            let full = Performance::new(doc, &path).unwrap();
            prop_assert_eq!(&performance, &full, "after {:?}", changes);
        }
    }
}
