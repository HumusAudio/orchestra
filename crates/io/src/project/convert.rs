// SPDX-License-Identifier: GPL-3.0-or-later

//! Between the model's description of a saved document (`DocumentParts`, ADR-0029 S4) and the
//! file schema of version 1 (`crate::project::schema`).
//!
//! # Two directions, two kinds of failure
//! - **Saving** (`to_file`) cannot fail for a valid document: every reference it converts (a bar
//!   index of the meter map to the bar's ID) exists by the model's rules (rule 13). It still
//!   returns a `Result` instead of unwrapping, as library code must (CLAUDE.md §4).
//! - **Loading** (`to_parts`) checks only what the text itself can get wrong and the model cannot
//!   see once the values are built: a zero ID, a bar ID that names no bar of the meter map, an
//!   event that is both a chord and a rest, two meter-map entries at one bar, a pass list out of
//!   order. Everything musical is then checked by `Document::from_parts`, which replays the
//!   description through the builder and runs `validate` (ADR-0029 L4).
//!
//! The conversion errors name the object by its ID (`event 144`), which, with one record per
//! line, finds the place in the file as surely as a line number.
//!
//! # Complexity
//! Both directions are O(size of the document), one pass, plus a hash map from bar IDs to their
//! index while loading.

use orchestra_core_model::{
    ArticulationSet, BarId, BarRepeat, BarRepeatId, CellParts, Chord, ClefId, DocumentParts,
    Dynamic, DynamicId, DynamicMark, DynamicPlacement, DynamicScope, Event, EventBody, EventId,
    Grace, GradualMark, Hairpin, HairpinId, HairpinShape, IdMap, InstrumentId, Interval, Jump,
    JumpFires, JumpId, JumpSpec, JumpTarget, Key, KeyChangeId, KeyOverride, KeySignature, Mark,
    Marker, MarkerId, Metronome, Note, NoteId, OtherDynamic, PassOffset, PassRule, PassRuleId,
    PassSet, PerformanceOffsets, PlacedEvent, RepeatEnd, RepeatId, RepeatStart, Rest, ScorePoint,
    Slur, SlurId, SlurPlacement, StaffId, StaffParts, TempoMark, TempoMarkChange, TempoMarkId, Tie,
    TrackId, TrackParts, Tuplet, TupletDisplay, TupletId, VoiceId, VoiceParts, Volta, VoltaId,
};
use orchestra_core_time::{BarIndex, Duration, IrregularBar, MeterChange, MeterMap, Tempo};

use super::codec::{
    ArpeggioName, ArticulationName, BarKindName, BracketName, ClefText, Dur, FermataName,
    GraceAttachName, GraceKindName, GroupingText, HairpinKindName, HairpinStyleName, LevelName,
    MarkerKindName, MeterText, ModeName, NumberingName, OrnamentText, PitchText, Rat, RepeatsName,
    RestText, SlurPlacementName, SpanName, ToleranceText, TupletNumberName, ValueText,
};
use super::schema::{
    BarGroupingV1, BarRepeatV1, CellV1, ClefChangeV1, DocumentV1, DynamicV1, EventV1, FileV1,
    GraceV1, GradualV1, HairpinV1, InitialTempoV1, IntervalV1, IrregularV1, JumpFiresV1,
    JumpTargetV1, JumpV1, KeyChangeV1, KeyOverrideMarkV1, KeyOverrideV1, KeyV1, LetRing, MarkerV1,
    MeterChangeV1, MeterMapV1, MeterV1, MetronomeV1, NextId, NoteV1, OffsetsV1, PassOffsetV1,
    PassRuleV1, RepeatEndV1, RepeatStartV1, RestartV1, SettingsV1, SlurV1, StaffV1, TempoChangeV1,
    TempoMarkV1, TempoV1, TieV1, TrackToleranceV1, TrackV1, TupletV1, VoiceV1, VoltaV1,
};
use super::{FORMAT_VERSION, LoadError, SaveError};

/// What wrote the file (`written_by`, informational only, ADR-0029 S5).
const WRITTEN_BY: &str = concat!("Orchestra ", env!("CARGO_PKG_VERSION"));

/// The names of the dynamic marks that are not plain levels, in the file (the levels use
/// [`LevelName`]'s names). Spelled out here, so that renaming a Rust variant never changes the
/// file (ADR-0029 S6).
const ACCENT_MARKS: [(DynamicMark, &str); 11] = [
    (DynamicMark::Sf, "sf"),
    (DynamicMark::Sfz, "sfz"),
    (DynamicMark::Sffz, "sffz"),
    (DynamicMark::Fz, "fz"),
    (DynamicMark::Rf, "rf"),
    (DynamicMark::Rfz, "rfz"),
    (DynamicMark::Fp, "fp"),
    (DynamicMark::Sfp, "sfp"),
    (DynamicMark::Sfpp, "sfpp"),
    (DynamicMark::Pf, "pf"),
    (DynamicMark::Niente, "niente"),
];

// ------------------------------------------------------------------------------------------------
// Saving: parts → file
// ------------------------------------------------------------------------------------------------

/// The file schema of a description (see the module documentation). O(size).
///
/// # Errors
/// [`SaveError::Encode`] if the meter map names a bar index the document does not have, which a
/// valid document never does (rule 13).
pub(crate) fn to_file(parts: &DocumentParts) -> Result<FileV1, SaveError> {
    Ok(FileV1 {
        orchestra_format: FORMAT_VERSION,
        written_by: WRITTEN_BY.to_owned(),
        next_id: NextId(parts.next_id.map(std::num::NonZeroU64::get)),
        document: DocumentV1 {
            meter: meter_to_file(&parts.meter_map, &parts.bars)?,
            bars: parts.bars.iter().map(|b| b.get()).collect(),
            tempo: InitialTempoV1 {
                id: parts.initial_tempo.0.get(),
                text: parts.initial_tempo.1.text().map(str::to_owned),
                change: tempo_change_to_file(parts.initial_tempo.1.change())
                    .ok_or_else(|| SaveError::Encode("the initial tempo sets no tempo".into()))?,
            },
            tempo_marks: parts
                .tempo_marks
                .iter()
                .map(|m| TempoMarkV1 {
                    id: m.id().get(),
                    bar: m.bar().get(),
                    at: Dur(m.offset()),
                    text: m.value().text().map(str::to_owned),
                    change: tempo_change_to_file(m.value().change()),
                })
                .collect(),
            key: key_to_file(parts.initial_key),
            key_changes: parts
                .key_changes
                .iter()
                .map(|m| KeyChangeV1 {
                    id: m.id().get(),
                    bar: m.bar().get(),
                    at: Dur(m.offset()),
                    key: key_to_file(*m.value()),
                })
                .collect(),
            repeat_starts: parts
                .repeat_starts
                .iter()
                .map(|r| RepeatStartV1 {
                    id: r.id().get(),
                    bar: r.bar().get(),
                })
                .collect(),
            repeat_ends: parts
                .repeat_ends
                .iter()
                .map(|r| RepeatEndV1 {
                    id: r.id().get(),
                    bar: r.bar().get(),
                    times: r.times(),
                })
                .collect(),
            voltas: parts
                .voltas
                .iter()
                .map(|v| VoltaV1 {
                    id: v.id().get(),
                    first: v.first().get(),
                    last: v.last().get(),
                    passes: passes_to_file(v.passes()),
                    closed: v.closed(),
                    text: v.text().map(str::to_owned),
                })
                .collect(),
            markers: parts
                .markers
                .iter()
                .map(|m| MarkerV1 {
                    id: m.id().get(),
                    bar: m.bar().get(),
                    kind: MarkerKindName(m.kind()),
                    text: m.text().map(str::to_owned),
                })
                .collect(),
            jumps: parts.jumps.iter().map(jump_to_file).collect(),
            numbering: NumberingName(parts.numbering),
        },
        tracks: parts.tracks.iter().map(track_to_file).collect(),
        settings: SettingsV1 {
            tolerance: ToleranceText(parts.tolerance),
            track_tolerances: parts
                .track_tolerances
                .iter()
                .map(|&(track, tolerance)| TrackToleranceV1 {
                    track: track.get(),
                    tolerance: ToleranceText(tolerance),
                })
                .collect(),
        },
    })
}

/// The meter map with bars by ID (ADR-0029 I6). O(entries).
fn meter_to_file(map: &MeterMap, bars: &[BarId]) -> Result<MeterMapV1, SaveError> {
    let bar = |index: BarIndex| -> Result<u64, SaveError> {
        usize::try_from(index.get())
            .ok()
            .and_then(|i| bars.get(i))
            .map(|b| b.get())
            .ok_or_else(|| {
                SaveError::Encode(format!("the meter map names bar index {}", index.get()))
            })
    };
    let mut changes = map.meter_changes().iter();
    // The map always starts with an entry at index 0 (ADR-0018).
    let initial = changes
        .next()
        .filter(|(b, _)| b.get() == 0)
        .map(|(_, c)| meter_v1(*c))
        .ok_or_else(|| SaveError::Encode("the meter map has no initial meter".into()))?;
    Ok(MeterMapV1 {
        initial,
        changes: changes
            .map(|(b, c)| {
                Ok(MeterChangeV1 {
                    bar: bar(*b)?,
                    meter: MeterText(c.meter),
                    grouping: c.grouping.map(GroupingText),
                })
            })
            .collect::<Result<_, SaveError>>()?,
        irregular: map
            .irregular_bars()
            .iter()
            .map(|(b, i)| {
                Ok(IrregularV1 {
                    bar: bar(*b)?,
                    length: Dur(i.length),
                    kind: BarKindName(i.kind),
                })
            })
            .collect::<Result<_, SaveError>>()?,
        groupings: map
            .bar_groupings()
            .iter()
            .map(|(b, g)| {
                Ok(BarGroupingV1 {
                    bar: bar(*b)?,
                    grouping: GroupingText(*g),
                })
            })
            .collect::<Result<_, SaveError>>()?,
        uncounted: map
            .uncounted_bars()
            .iter()
            .map(|b| bar(*b))
            .collect::<Result<_, SaveError>>()?,
        restarts: map
            .numbering_restarts()
            .iter()
            .map(|(b, number)| {
                Ok(RestartV1 {
                    bar: bar(*b)?,
                    number: *number,
                })
            })
            .collect::<Result<_, SaveError>>()?,
    })
}

/// A meter and its grouping. O(1).
fn meter_v1(change: MeterChange) -> MeterV1 {
    MeterV1 {
        meter: MeterText(change.meter),
        grouping: change.grouping.map(GroupingText),
    }
}

/// A tempo. O(1).
fn tempo_v1(tempo: Tempo) -> TempoV1 {
    TempoV1 {
        unit: Dur(tempo.unit()),
        bpm: Rat(tempo.bpm()),
    }
}

/// A metronome mark. O(1).
fn metronome_v1(m: Metronome) -> MetronomeV1 {
    MetronomeV1 {
        unit: Dur(m.tempo.unit()),
        bpm: Rat(m.tempo.bpm()),
        shown: m.shown,
    }
}

/// What a tempo mark changes, if anything. O(1).
fn tempo_change_to_file(change: Option<TempoMarkChange>) -> Option<TempoChangeV1> {
    change.map(|c| match c {
        TempoMarkChange::Immediate(m) => TempoChangeV1::Immediate(metronome_v1(m)),
        TempoMarkChange::Gradual(g) => TempoChangeV1::Gradual(GradualV1 {
            from: g.from.map(metronome_v1),
            to: tempo_v1(g.to),
            end_bar: g.end.bar.get(),
            end_at: Dur(g.end.offset),
        }),
    })
}

/// A key. O(1).
fn key_to_file(key: Key) -> KeyV1 {
    match key {
        Key::Traditional(k) => KeyV1 {
            fifths: k.fifths(),
            mode: ModeName(k.mode()),
        },
    }
}

/// A set of passes as an ascending list. O(64).
fn passes_to_file(passes: PassSet) -> Vec<u8> {
    (1..=orchestra_core_model::MAX_PLAYS)
        .filter(|&p| passes.contains(p))
        .collect()
}

/// A jump. O(1).
fn jump_to_file(jump: &Jump) -> JumpV1 {
    let spec = jump.spec();
    JumpV1 {
        id: jump.id().get(),
        bar: jump.bar().get(),
        to: match spec.to {
            JumpTarget::Start => JumpTargetV1::Start,
            JumpTarget::Marker(m) => JumpTargetV1::Marker(m.get()),
        },
        until: spec.until.map(MarkerId::get),
        then: spec.then.map(MarkerId::get),
        repeats: RepeatsName(spec.repeats),
        fires: match spec.fires {
            JumpFires::AfterRepeats => JumpFiresV1::AfterRepeats,
            JumpFires::OnPass(n) => JumpFiresV1::OnPass(n),
        },
        text: jump.text().map(str::to_owned),
    }
}

/// Performance offsets with zero components omitted; `None` if all are zero. O(1).
fn offsets_to_file(offsets: PerformanceOffsets) -> Option<OffsetsV1> {
    if offsets.is_none() {
        return None;
    }
    Some(OffsetsV1 {
        onset: (!offsets.onset.is_zero()).then_some(Dur(offsets.onset)),
        length: (!offsets.length.is_zero()).then_some(Dur(offsets.length)),
        velocity: (offsets.velocity != 0).then_some(offsets.velocity),
    })
}

/// One track. O(size of the track).
#[allow(
    clippy::too_many_lines,
    reason = "one short mapping per field of the schema: splitting it would scatter the mapping"
)]
fn track_to_file(track: &TrackParts) -> TrackV1 {
    TrackV1 {
        id: track.id.get(),
        instrument: track.instrument.get(),
        name: track.name.clone(),
        transposition: IntervalV1 {
            steps: track.transposition.steps(),
            semitones: Rat(track.transposition.semitones()),
        },
        staves: track
            .staves
            .iter()
            .map(|s| StaffV1 {
                id: s.id.get(),
                clef: ClefText(s.initial_clef),
                clef_changes: s
                    .clef_changes
                    .iter()
                    .map(|m| ClefChangeV1 {
                        id: m.id().get(),
                        bar: m.bar().get(),
                        at: Dur(m.offset()),
                        clef: ClefText(*m.value()),
                    })
                    .collect(),
            })
            .collect(),
        voices: track.voices.iter().map(voice_to_file).collect(),
        key_overrides: track
            .key_overrides
            .iter()
            .map(|m| KeyOverrideMarkV1 {
                id: m.id().get(),
                bar: m.bar().get(),
                at: Dur(m.offset()),
                key: match m.value() {
                    KeyOverride::Signature(KeySignature::Key(k)) => {
                        KeyOverrideV1::Key(key_to_file(*k))
                    }
                    KeyOverride::Signature(KeySignature::NoSignature) => KeyOverrideV1::NoSignature,
                    KeyOverride::FollowScore => KeyOverrideV1::FollowScore,
                },
            })
            .collect(),
        slurs: track
            .slurs
            .iter()
            .map(|s| SlurV1 {
                id: s.id().get(),
                start: s.start().get(),
                end: s.end().get(),
                placement: (s.placement() != SlurPlacement::Auto)
                    .then_some(SlurPlacementName(s.placement())),
            })
            .collect(),
        dynamics: track.dynamics.iter().map(dynamic_to_file).collect(),
        hairpins: track
            .hairpins
            .iter()
            .map(|h| {
                let (staff, voice) = placement_to_file(h.placement());
                let shape = h.shape();
                HairpinV1 {
                    id: h.id().get(),
                    bar: h.start().bar.get(),
                    at: Dur(h.start().offset),
                    end_bar: h.end().bar.get(),
                    end_at: Dur(h.end().offset),
                    staff,
                    voice,
                    kind: HairpinKindName(shape.kind),
                    style: HairpinStyleName(shape.style),
                    niente: shape.niente,
                }
            })
            .collect(),
        bar_repeats: track
            .bar_repeats
            .iter()
            .map(|r| BarRepeatV1 {
                id: r.id().get(),
                staff: r.staff().get(),
                first: r.first().get(),
                count: r.count(),
                span: SpanName(r.span()),
            })
            .collect(),
        pass_rules: track
            .pass_rules
            .iter()
            .map(|r| PassRuleV1 {
                id: r.id().get(),
                bar: r.start().bar.get(),
                at: Dur(r.start().offset),
                end_bar: r.end().bar.get(),
                end_at: Dur(r.end().offset),
                voice: match r.scope() {
                    DynamicScope::Instrument => None,
                    DynamicScope::Voice(v) => Some(v.get()),
                },
                plays: passes_to_file(r.plays()),
                text: r.text().map(str::to_owned),
            })
            .collect(),
        pass_offsets: track
            .pass_offsets
            .iter()
            .map(|p| PassOffsetV1 {
                note: p.note.get(),
                pass: p.pass,
                offsets: offsets_to_file(p.offsets).unwrap_or_default(),
            })
            .collect(),
    }
}

/// The staff and, for a voice scope, the voice of a dynamic's placement. O(1).
fn placement_to_file(placement: DynamicPlacement) -> (u64, Option<u64>) {
    let voice = match placement.scope {
        DynamicScope::Instrument => None,
        DynamicScope::Voice(v) => Some(v.get()),
    };
    (placement.staff.get(), voice)
}

/// A dynamic. O(1).
fn dynamic_to_file(dynamic: &Dynamic) -> DynamicV1 {
    let (staff, voice) = placement_to_file(dynamic.placement());
    let (mark, text, level) = match dynamic.mark() {
        DynamicMark::Level(l) => (Some(LevelName::encode(*l).to_owned()), None, None),
        DynamicMark::Other(other) => (
            None,
            Some(other.text().to_owned()),
            other.level().map(LevelName),
        ),
        named => (
            ACCENT_MARKS
                .iter()
                .find(|(m, _)| m == named)
                .map(|(_, t)| (*t).to_owned()),
            None,
            None,
        ),
    };
    DynamicV1 {
        id: dynamic.id().get(),
        bar: dynamic.at().bar.get(),
        at: Dur(dynamic.at().offset),
        staff,
        voice,
        mark,
        text,
        level,
    }
}

/// One voice. O(size of the voice).
fn voice_to_file(voice: &VoiceParts) -> VoiceV1 {
    VoiceV1 {
        id: voice.id.get(),
        home_staff: voice.home_staff.get(),
        tuplets: voice
            .tuplets
            .iter()
            .map(|(id, t)| TupletV1 {
                id: id.get(),
                actual: t.actual(),
                normal: t.normal(),
                unit: ValueText(t.unit()),
                parent: t.parent().map(TupletId::get),
                bracket: BracketName(t.display().bracket),
                number: TupletNumberName(t.display().number),
            })
            .collect(),
        cells: voice
            .cells
            .iter()
            .map(|cell| CellV1 {
                bar: cell.bar.get(),
                events: cell.events.iter().map(event_to_file).collect(),
            })
            .collect(),
    }
}

/// One event (ADR-0029 I3). O(noteheads).
fn event_to_file(placed: &PlacedEvent) -> EventV1 {
    let event = placed.event();
    let mut v = EventV1 {
        id: event.id().get(),
        at: Dur(placed.offset()),
        staff: event.staff().get(),
        value: None,
        rest: None,
        tuplet: None,
        grace: None,
        articulations: Vec::new(),
        ornament: None,
        fermata: None,
        arpeggio: None,
        notes: None,
    };
    match event.body() {
        EventBody::Chord(chord) => {
            v.value = Some(ValueText(chord.value()));
            v.tuplet = chord.tuplet().map(TupletId::get);
            v.grace = chord.grace().map(|g| GraceV1 {
                kind: GraceKindName(g.kind),
                attach: GraceAttachName(g.attach),
            });
            v.articulations = chord.articulations().iter().map(ArticulationName).collect();
            v.ornament = chord.ornament().map(OrnamentText);
            v.fermata = chord.fermata().map(FermataName);
            v.arpeggio = chord.arpeggio().map(ArpeggioName);
            v.notes = Some(
                chord
                    .notes()
                    .map(|n| NoteV1 {
                        id: n.id().get(),
                        pitch: PitchText(n.pitch()),
                        tie: match n.tie() {
                            Tie::None => None,
                            Tie::To(t) => Some(TieV1::To(t.get())),
                            Tie::LetRing => Some(TieV1::LetRing(LetRing::LetRing)),
                        },
                        offsets: offsets_to_file(n.offsets()),
                    })
                    .collect(),
            );
        }
        EventBody::Rest(rest) => {
            v.rest = Some(RestText(rest.value()));
            v.tuplet = rest.tuplet().map(TupletId::get);
            v.fermata = rest.fermata().map(FermataName);
        }
    }
    v
}

// ------------------------------------------------------------------------------------------------
// Loading: file → parts
// ------------------------------------------------------------------------------------------------

/// A conversion error naming what is wrong (see the module documentation).
fn content(message: impl Into<String>) -> LoadError {
    LoadError::Schema {
        line: None,
        column: None,
        message: message.into(),
    }
}

/// A typed ID from a raw saved value; 0 is never an ID. O(1).
fn id<T>(raw: u64, from_raw: fn(u64) -> Option<T>) -> Result<T, LoadError> {
    from_raw(raw).ok_or(LoadError::Document(
        orchestra_core_model::DocumentError::ZeroId,
    ))
}

/// A mark read from the file: its ID, its bar, its offset in the bar and its value. The one
/// conversion behind the tempo marks, the key changes, the clef changes and the key overrides.
///
/// The value comes as a closure, called after both IDs are checked, so a record with a bad ID
/// and a bad value reports the ID, as the field order of the record would. O(1) plus the value.
///
/// # Errors
/// [`LoadError::Document`] with `ZeroId` for a zero ID; the value's own error.
fn mark_from_file<I: Copy, T>(
    raw_id: u64,
    from_raw: fn(u64) -> Option<I>,
    raw_bar: u64,
    at: Duration,
    value: impl FnOnce() -> Result<T, LoadError>,
) -> Result<Mark<I, T>, LoadError> {
    let mark_id = id(raw_id, from_raw)?;
    let bar = id(raw_bar, BarId::from_raw)?;
    Ok(Mark::new(mark_id, bar, at, value()?))
}

/// The bar IDs of the file and where each is, for the meter map's entries (ADR-0029 I6).
struct Bars {
    /// Bar ID (raw) → index.
    index: IdMap<u64, u32>,
}

impl Bars {
    /// The index of a bar named by the meter map, which must be one of the document's bars.
    /// O(1) average.
    fn of(&self, raw: u64) -> Result<BarIndex, LoadError> {
        self.index
            .get(&raw)
            .map(|&i| BarIndex::new(i))
            .ok_or_else(|| {
                content(format!(
                    "the meter map names bar {raw}, which is not a bar of the document"
                ))
            })
    }
}

/// The description of a file's document (see the module documentation). O(size).
///
/// # Errors
/// [`LoadError::Schema`] (without a line) for what the text gets wrong; [`LoadError::Document`]
/// for a zero ID or a value the model refuses.
#[allow(
    clippy::too_many_lines,
    reason = "one short mapping per field of the schema: splitting it would scatter the mapping"
)]
pub(crate) fn to_parts(file: FileV1) -> Result<DocumentParts, LoadError> {
    let doc = file.document;
    let bars: Vec<BarId> = doc
        .bars
        .iter()
        .map(|&b| id(b, BarId::from_raw))
        .collect::<Result<_, _>>()?;
    let index = Bars {
        index: doc
            .bars
            .iter()
            .enumerate()
            .filter_map(|(i, &b)| u32::try_from(i).ok().map(|i| (b, i)))
            .collect(),
    };
    let initial_change = tempo_change_from_file(doc.tempo.change)?;
    Ok(DocumentParts {
        next_id: match file.next_id.0 {
            None => None,
            Some(raw) => Some(std::num::NonZeroU64::new(raw).ok_or(LoadError::Document(
                orchestra_core_model::DocumentError::ZeroId,
            ))?),
        },
        meter_map: meter_from_file(doc.meter, &index)?,
        bars,
        initial_tempo: (
            id(doc.tempo.id, TempoMarkId::from_raw)?,
            tempo_mark(doc.tempo.text.as_deref(), Some(initial_change))?,
        ),
        tempo_marks: doc
            .tempo_marks
            .into_iter()
            .map(|m| {
                let change = m.change.map(tempo_change_from_file).transpose()?;
                mark_from_file(m.id, TempoMarkId::from_raw, m.bar, m.at.0, || {
                    tempo_mark(m.text.as_deref(), change)
                })
            })
            .collect::<Result<_, LoadError>>()?,
        initial_key: key_from_file(&doc.key)?,
        key_changes: doc
            .key_changes
            .iter()
            .map(|m| {
                mark_from_file(m.id, KeyChangeId::from_raw, m.bar, m.at.0, || {
                    key_from_file(&m.key)
                })
            })
            .collect::<Result<_, LoadError>>()?,
        repeat_starts: doc
            .repeat_starts
            .iter()
            .map(|r| {
                Ok(RepeatStart::new(
                    id(r.id, RepeatId::from_raw)?,
                    id(r.bar, BarId::from_raw)?,
                ))
            })
            .collect::<Result<_, LoadError>>()?,
        repeat_ends: doc
            .repeat_ends
            .iter()
            .map(|r| {
                Ok(RepeatEnd::new(
                    id(r.id, RepeatId::from_raw)?,
                    id(r.bar, BarId::from_raw)?,
                    r.times,
                ))
            })
            .collect::<Result<_, LoadError>>()?,
        voltas: doc
            .voltas
            .into_iter()
            .map(|v| {
                Ok(Volta::new(
                    id(v.id, VoltaId::from_raw)?,
                    id(v.first, BarId::from_raw)?,
                    id(v.last, BarId::from_raw)?,
                    passes_from_file(&v.passes, v.id)?,
                    v.closed,
                    v.text.map(String::into_boxed_str),
                ))
            })
            .collect::<Result<_, LoadError>>()?,
        markers: doc
            .markers
            .into_iter()
            .map(|m| {
                Ok(Marker::new(
                    id(m.id, MarkerId::from_raw)?,
                    id(m.bar, BarId::from_raw)?,
                    m.kind.0,
                    m.text.map(String::into_boxed_str),
                ))
            })
            .collect::<Result<_, LoadError>>()?,
        jumps: doc
            .jumps
            .into_iter()
            .map(jump_from_file)
            .collect::<Result<_, LoadError>>()?,
        numbering: doc.numbering.0,
        tracks: file
            .tracks
            .into_iter()
            .map(track_from_file)
            .collect::<Result<_, LoadError>>()?,
        tolerance: file.settings.tolerance.0,
        track_tolerances: file
            .settings
            .track_tolerances
            .iter()
            .map(|t| Ok((id(t.track, TrackId::from_raw)?, t.tolerance.0)))
            .collect::<Result<_, LoadError>>()?,
    })
}

/// A meter change. O(1).
const fn meter_change(meter: MeterText, grouping: Option<GroupingText>) -> MeterChange {
    MeterChange {
        meter: meter.0,
        grouping: match grouping {
            Some(g) => Some(g.0),
            None => None,
        },
    }
}

/// The meter map, refusing two entries of one kind at one bar (a re-save would keep only one).
/// O(entries × log entries).
fn meter_from_file(meter: MeterMapV1, bars: &Bars) -> Result<MeterMap, LoadError> {
    let model = |e: orchestra_core_time::MeterMapError| {
        LoadError::Document(orchestra_core_model::DocumentError::MeterMap(e))
    };
    let twice = |what: &str, bar: u64| content(format!("two {what} at bar {bar}"));
    let mut map =
        MeterMap::new(meter_change(meter.initial.meter, meter.initial.grouping)).map_err(model)?;
    for c in meter.changes {
        let index = bars.of(c.bar)?;
        if index.get() == 0 {
            return Err(content(
                "a meter change at the first bar: write it as the initial meter",
            ));
        }
        if map
            .set_meter_change(index, meter_change(c.meter, c.grouping))
            .map_err(model)?
            .is_some()
        {
            return Err(twice("meter changes", c.bar));
        }
    }
    for i in meter.irregular {
        let irregular = IrregularBar {
            length: i.length.0,
            kind: i.kind.0,
        };
        if map
            .set_irregular_bar(bars.of(i.bar)?, irregular)
            .map_err(model)?
            .is_some()
        {
            return Err(twice("irregular bars", i.bar));
        }
    }
    for g in meter.groupings {
        if map
            .set_bar_grouping(bars.of(g.bar)?, g.grouping.0)
            .map_err(model)?
            .is_some()
        {
            return Err(twice("beat groupings", g.bar));
        }
    }
    for raw in meter.uncounted {
        // Already uncounted before this entry: the file lists the bar twice.
        if map
            .set_numbering(bars.of(raw)?, orchestra_core_time::BarCounting::Uncounted)
            .is_uncounted()
        {
            return Err(twice("uncounted marks", raw));
        }
    }
    for r in meter.restarts {
        if map
            .set_numbering_restart(bars.of(r.bar)?, Some(r.number))
            .is_some()
        {
            return Err(twice("numbering restarts", r.bar));
        }
    }
    Ok(map)
}

/// A tempo. O(1).
fn tempo_from_file(t: &TempoV1) -> Result<Tempo, LoadError> {
    Tempo::new(t.unit.0, t.bpm.0).map_err(|e| content(format!("invalid tempo: {e}")))
}

/// A metronome mark. O(1).
fn metronome_from_file(m: &MetronomeV1) -> Result<Metronome, LoadError> {
    Ok(Metronome {
        tempo: tempo_from_file(&TempoV1 {
            unit: m.unit,
            bpm: m.bpm,
        })?,
        shown: m.shown,
    })
}

/// What a tempo mark changes. O(1).
fn tempo_change_from_file(change: TempoChangeV1) -> Result<TempoMarkChange, LoadError> {
    Ok(match change {
        TempoChangeV1::Immediate(m) => TempoMarkChange::Immediate(metronome_from_file(&m)?),
        TempoChangeV1::Gradual(g) => TempoMarkChange::Gradual(GradualMark {
            from: g.from.as_ref().map(metronome_from_file).transpose()?,
            to: tempo_from_file(&g.to)?,
            end: ScorePoint::new(id(g.end_bar, BarId::from_raw)?, g.end_at.0),
        }),
    })
}

/// A tempo mark from its text and its change. O(text).
fn tempo_mark(text: Option<&str>, change: Option<TempoMarkChange>) -> Result<TempoMark, LoadError> {
    TempoMark::new(text, change).map_err(LoadError::Document)
}

/// A key. O(1).
fn key_from_file(k: &KeyV1) -> Result<Key, LoadError> {
    Key::traditional(k.fifths, k.mode.0)
        .map_err(|e| LoadError::Document(orchestra_core_model::DocumentError::Pitch(e)))
}

/// A set of passes, which must be written ascending and without repeats (a re-save would
/// normalise any other list). `owner` names the object in the error. O(passes).
fn passes_from_file(passes: &[u8], owner: u64) -> Result<PassSet, LoadError> {
    if passes.windows(2).any(|w| matches!(w, [a, b] if a >= b)) {
        return Err(content(format!(
            "the passes of object {owner} are not ascending"
        )));
    }
    PassSet::of(passes).map_err(LoadError::Document)
}

/// A jump. O(1).
fn jump_from_file(j: JumpV1) -> Result<Jump, LoadError> {
    let spec = JumpSpec {
        to: match j.to {
            JumpTargetV1::Start => JumpTarget::Start,
            JumpTargetV1::Marker(m) => JumpTarget::Marker(id(m, MarkerId::from_raw)?),
        },
        until: j.until.map(|m| id(m, MarkerId::from_raw)).transpose()?,
        then: j.then.map(|m| id(m, MarkerId::from_raw)).transpose()?,
        repeats: j.repeats.0,
        fires: match j.fires {
            JumpFiresV1::AfterRepeats => JumpFires::AfterRepeats,
            JumpFiresV1::OnPass(n) => JumpFires::OnPass(n),
        },
    };
    Ok(Jump::new(
        id(j.id, JumpId::from_raw)?,
        id(j.bar, BarId::from_raw)?,
        spec,
        j.text.map(String::into_boxed_str),
    ))
}

/// Performance offsets, refusing an all-absent object (a re-save would drop it). `owner` names
/// the notehead. O(1).
fn offsets_from_file(o: Option<OffsetsV1>, owner: u64) -> Result<PerformanceOffsets, LoadError> {
    let Some(o) = o else {
        return Ok(PerformanceOffsets::NONE);
    };
    let offsets = PerformanceOffsets {
        onset: o.onset.map_or(Duration::ZERO, |d| d.0),
        length: o.length.map_or(Duration::ZERO, |d| d.0),
        velocity: o.velocity.unwrap_or(0),
    };
    let explicit_zero = o.onset.is_some_and(|d| d.0.is_zero())
        || o.length.is_some_and(|d| d.0.is_zero())
        || o.velocity == Some(0);
    if explicit_zero || offsets.is_none() {
        return Err(content(format!(
            "the offsets of notehead {owner} write a zero component: omit it"
        )));
    }
    Ok(offsets)
}

/// A dynamic's placement. O(1).
fn placement_from_file(staff: u64, voice: Option<u64>) -> Result<DynamicPlacement, LoadError> {
    let staff = id(staff, StaffId::from_raw)?;
    Ok(match voice {
        None => DynamicPlacement::instrument(staff),
        Some(v) => DynamicPlacement::voice(id(v, VoiceId::from_raw)?, staff),
    })
}

/// The scope of a pass rule. O(1).
fn scope_from_file(voice: Option<u64>) -> Result<DynamicScope, LoadError> {
    Ok(match voice {
        None => DynamicScope::Instrument,
        Some(v) => DynamicScope::Voice(id(v, VoiceId::from_raw)?),
    })
}

/// One track. O(size of the track).
#[allow(
    clippy::too_many_lines,
    reason = "one short mapping per field of the schema: splitting it would scatter the mapping"
)]
fn track_from_file(t: TrackV1) -> Result<TrackParts, LoadError> {
    Ok(TrackParts {
        id: id(t.id, TrackId::from_raw)?,
        instrument: id(t.instrument, InstrumentId::from_raw)?,
        name: t.name,
        transposition: Interval::new(t.transposition.steps, t.transposition.semitones.0),
        staves: t
            .staves
            .into_iter()
            .map(|s| {
                Ok(StaffParts {
                    id: id(s.id, StaffId::from_raw)?,
                    initial_clef: s.clef.0,
                    clef_changes: s
                        .clef_changes
                        .iter()
                        .map(|c| {
                            mark_from_file(c.id, ClefId::from_raw, c.bar, c.at.0, || Ok(c.clef.0))
                        })
                        .collect::<Result<_, LoadError>>()?,
                })
            })
            .collect::<Result<_, LoadError>>()?,
        voices: t
            .voices
            .into_iter()
            .map(voice_from_file)
            .collect::<Result<_, LoadError>>()?,
        key_overrides: t
            .key_overrides
            .iter()
            .map(|m| {
                mark_from_file(m.id, KeyChangeId::from_raw, m.bar, m.at.0, || {
                    Ok(match &m.key {
                        KeyOverrideV1::Key(k) => {
                            KeyOverride::Signature(KeySignature::Key(key_from_file(k)?))
                        }
                        KeyOverrideV1::NoSignature => {
                            KeyOverride::Signature(KeySignature::NoSignature)
                        }
                        KeyOverrideV1::FollowScore => KeyOverride::FollowScore,
                    })
                })
            })
            .collect::<Result<_, LoadError>>()?,
        slurs: t
            .slurs
            .iter()
            .map(|s| {
                if s.placement.is_some_and(|p| p.0 == SlurPlacement::Auto) {
                    return Err(content(format!(
                        "slur {} writes the default placement: omit it",
                        s.id
                    )));
                }
                Ok(Slur::new(
                    id(s.id, SlurId::from_raw)?,
                    id(s.start, EventId::from_raw)?,
                    id(s.end, EventId::from_raw)?,
                    s.placement.map_or(SlurPlacement::Auto, |p| p.0),
                ))
            })
            .collect::<Result<_, LoadError>>()?,
        dynamics: t
            .dynamics
            .into_iter()
            .map(dynamic_from_file)
            .collect::<Result<_, LoadError>>()?,
        hairpins: t
            .hairpins
            .iter()
            .map(|h| {
                Ok(Hairpin::new(
                    id(h.id, HairpinId::from_raw)?,
                    ScorePoint::new(id(h.bar, BarId::from_raw)?, h.at.0),
                    ScorePoint::new(id(h.end_bar, BarId::from_raw)?, h.end_at.0),
                    placement_from_file(h.staff, h.voice)?,
                    HairpinShape {
                        kind: h.kind.0,
                        style: h.style.0,
                        niente: h.niente,
                    },
                ))
            })
            .collect::<Result<_, LoadError>>()?,
        bar_repeats: t
            .bar_repeats
            .iter()
            .map(|r| {
                Ok(BarRepeat::new(
                    id(r.id, BarRepeatId::from_raw)?,
                    id(r.staff, StaffId::from_raw)?,
                    id(r.first, BarId::from_raw)?,
                    r.count,
                    r.span.0,
                ))
            })
            .collect::<Result<_, LoadError>>()?,
        pass_rules: t
            .pass_rules
            .into_iter()
            .map(|r| {
                Ok(PassRule::new(
                    id(r.id, PassRuleId::from_raw)?,
                    ScorePoint::new(id(r.bar, BarId::from_raw)?, r.at.0),
                    ScorePoint::new(id(r.end_bar, BarId::from_raw)?, r.end_at.0),
                    scope_from_file(r.voice)?,
                    passes_from_file(&r.plays, r.id)?,
                    r.text.map(String::into_boxed_str),
                ))
            })
            .collect::<Result<_, LoadError>>()?,
        pass_offsets: t
            .pass_offsets
            .into_iter()
            .map(|p| {
                Ok(PassOffset {
                    note: id(p.note, NoteId::from_raw)?,
                    pass: p.pass,
                    offsets: offsets_from_file(Some(p.offsets), p.note)?,
                })
            })
            .collect::<Result<_, LoadError>>()?,
    })
}

/// A dynamic: exactly one of a named mark and a free text. O(1).
fn dynamic_from_file(d: DynamicV1) -> Result<Dynamic, LoadError> {
    let mark = match (d.mark.as_deref(), d.text) {
        (Some(name), None) if d.level.is_none() => ACCENT_MARKS
            .iter()
            .find(|(_, t)| *t == name)
            .map(|(m, _)| m.clone())
            .or_else(|| LevelName::decode(name).map(DynamicMark::Level))
            .ok_or_else(|| content(format!("dynamic {}: unknown mark {name:?}", d.id)))?,
        (None, Some(text)) => DynamicMark::Other(
            OtherDynamic::new(&text, d.level.map(|l| l.0)).map_err(LoadError::Document)?,
        ),
        _ => {
            return Err(content(format!(
                "dynamic {}: write either a mark or a text (with an optional level)",
                d.id
            )));
        }
    };
    Ok(Dynamic::new(
        id(d.id, DynamicId::from_raw)?,
        ScorePoint::new(id(d.bar, BarId::from_raw)?, d.at.0),
        placement_from_file(d.staff, d.voice)?,
        mark,
    ))
}

/// One voice. O(size of the voice).
fn voice_from_file(v: VoiceV1) -> Result<VoiceParts, LoadError> {
    Ok(VoiceParts {
        id: id(v.id, VoiceId::from_raw)?,
        home_staff: id(v.home_staff, StaffId::from_raw)?,
        tuplets: v
            .tuplets
            .iter()
            .map(|t| {
                let tuplet = Tuplet::new(t.actual, t.normal, t.unit.0)
                    .map_err(|e| {
                        LoadError::Document(orchestra_core_model::DocumentError::NoteValue(e))
                    })?
                    .with_parent(t.parent.map(|p| id(p, TupletId::from_raw)).transpose()?)
                    .with_display(TupletDisplay {
                        bracket: t.bracket.0,
                        number: t.number.0,
                    });
                Ok((id(t.id, TupletId::from_raw)?, tuplet))
            })
            .collect::<Result<_, LoadError>>()?,
        cells: v
            .cells
            .into_iter()
            .map(|c| {
                if c.events.is_empty() {
                    return Err(content(format!("bar {} has an empty cell: omit it", c.bar)));
                }
                Ok(CellParts {
                    bar: id(c.bar, BarId::from_raw)?,
                    events: c
                        .events
                        .into_iter()
                        .map(event_from_file)
                        .collect::<Result<_, LoadError>>()?,
                })
            })
            .collect::<Result<_, LoadError>>()?,
    })
}

/// One event: a chord or a rest, exactly one (ADR-0029 I3). O(noteheads).
fn event_from_file(e: EventV1) -> Result<PlacedEvent, LoadError> {
    let event_id = id(e.id, EventId::from_raw)?;
    let staff = id(e.staff, StaffId::from_raw)?;
    let tuplet = e.tuplet.map(|t| id(t, TupletId::from_raw)).transpose()?;
    let body = match (e.value, e.notes, e.rest) {
        (Some(value), Some(notes), None) => {
            let notes = notes
                .into_iter()
                .map(|n| {
                    let tie = match n.tie {
                        None => Tie::None,
                        Some(TieV1::To(t)) => Tie::To(id(t, NoteId::from_raw)?),
                        Some(TieV1::LetRing(LetRing::LetRing)) => Tie::LetRing,
                    };
                    Ok(Note::new(id(n.id, NoteId::from_raw)?, n.pitch.0)
                        .with_tie(tie)
                        .with_offsets(offsets_from_file(n.offsets, n.id)?))
                })
                .collect::<Result<Vec<Note>, LoadError>>()?;
            let articulations = e
                .articulations
                .iter()
                .fold(ArticulationSet::EMPTY, |set, a| set.with(a.0));
            // The articulations must be written in the model's order, once each.
            let canonical: Vec<_> = articulations.iter().collect();
            if canonical.len() != e.articulations.len()
                || canonical
                    .iter()
                    .zip(&e.articulations)
                    .any(|(a, b)| *a != b.0)
            {
                return Err(content(format!(
                    "event {}: the articulations are repeated or out of order",
                    e.id
                )));
            }
            let chord = Chord::new(
                value.0,
                tuplet,
                e.grace.map(|g| Grace {
                    kind: g.kind.0,
                    attach: g.attach.0,
                }),
                notes,
            )
            .map_err(LoadError::Document)?
            .with_articulations(articulations)
            .with_ornament(e.ornament.map(|o| o.0))
            .with_fermata(e.fermata.map(|f| f.0))
            .with_arpeggio(e.arpeggio.map(|a| a.0));
            EventBody::Chord(chord)
        }
        (None, None, Some(rest)) => {
            if e.grace.is_some()
                || !e.articulations.is_empty()
                || e.ornament.is_some()
                || e.arpeggio.is_some()
            {
                return Err(content(format!(
                    "event {}: a rest has no grace, articulation, ornament or arpeggio",
                    e.id
                )));
            }
            EventBody::Rest(Rest::new(rest.0, tuplet).with_fermata(e.fermata.map(|f| f.0)))
        }
        _ => {
            return Err(content(format!(
                "event {}: write either \"value\" and \"notes\" (a chord) or \"rest\"",
                e.id
            )));
        }
    };
    Ok(PlacedEvent::new(e.at.0, Event::new(event_id, staff, body)))
}
