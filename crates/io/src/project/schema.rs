// SPDX-License-Identifier: GPL-3.0-or-later

//! The schema of version 1 of the project file (ADR-0029; owner's choices I1–I6 of 2026-11-06).
//!
//! # What this is
//! Plain structs with serde derives that mirror the JSON text one to one. They are **not** the
//! model: the model's memory layout can change freely, and this schema changes only with a new
//! format version and its migration (ADR-0029 S4, ADR-0010). The conversion to and from the
//! model's `DocumentParts` lives in `crate::project::convert`.
//!
//! # Rules applied to every struct
//! - `deny_unknown_fields`: an unknown field is refused with its line and column (ADR-0029 S8),
//!   never dropped.
//! - Field order = the order written in the file (serde writes struct fields in declaration
//!   order); the writer is therefore deterministic (S3).
//! - Structural absences are omitted when absent (S6: `skip_serializing_if`) and filled in when
//!   missing (`default`); user settings and every required field are always written.
//! - Bars are named by **ID** everywhere, also in the meter map (I6), so inserting a bar changes
//!   one line of the file, not every later entry.
//! - IDs are JSON integers; values with an exact meaning are the text forms of
//!   `crate::project::codec`.

use serde::{Deserialize, Serialize};

use super::codec::{
    ArpeggioName, ArticulationName, BarKindName, BracketName, ClefText, Dur, FermataName,
    GraceAttachName, GraceKindName, GroupingText, HairpinKindName, HairpinStyleName, LevelName,
    MarkerKindName, MeterText, ModeName, NumberingName, OrnamentText, PitchText, Rat, RepeatsName,
    RestText, SlurPlacementName, SpanName, ToleranceText, TupletNumberName, ValueText,
};

/// Whether a vector is empty (serde helper for omitted lists).
fn is_empty<T>(v: &[T]) -> bool {
    v.is_empty()
}

/// Whether a flag is false (serde helper for omitted flags).
#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes a reference"
)]
const fn is_false(b: &bool) -> bool {
    !*b
}

/// The whole file (I1). `orchestra_format` is first, so that the header scanner reads the version
/// before anything else (ADR-0029 S5).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileV1 {
    /// The format version: 1.
    pub(crate) orchestra_format: u32,
    /// Which program wrote the file: informational only, never used to decide anything.
    pub(crate) written_by: String,
    /// The ID the counter hands out next; `null` once every value has been used (I1). Required.
    pub(crate) next_id: NextId,
    /// What belongs to the whole score.
    pub(crate) document: DocumentV1,
    /// The tracks, top to bottom.
    pub(crate) tracks: Vec<TrackV1>,
    /// The project's settings.
    pub(crate) settings: SettingsV1,
}

/// The counter's next value, `null` when exhausted. A newtype so that the field is **required**:
/// serde treats a missing plain `Option` field as `None`, and a missing counter must be an error.
#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct NextId(pub(crate) Option<u64>);

/// What belongs to the whole score.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DocumentV1 {
    /// The meter map.
    pub(crate) meter: MeterMapV1,
    /// The bars' IDs, in score order.
    pub(crate) bars: Vec<u64>,
    /// The tempo mark at the very start.
    pub(crate) tempo: InitialTempoV1,
    /// The tempo marks after the start, in score order.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) tempo_marks: Vec<TempoMarkV1>,
    /// The key at the start, at concert pitch.
    pub(crate) key: KeyV1,
    /// The key changes after the start, at concert pitch, in score order.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) key_changes: Vec<KeyChangeV1>,
    /// The start repeat barlines.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) repeat_starts: Vec<RepeatStartV1>,
    /// The end repeat barlines.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) repeat_ends: Vec<RepeatEndV1>,
    /// The voltas.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) voltas: Vec<VoltaV1>,
    /// The markers.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) markers: Vec<MarkerV1>,
    /// The jumps.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) jumps: Vec<JumpV1>,
    /// The bar-numbering style: a setting, always written (S6).
    pub(crate) numbering: NumberingName,
}

/// The meter map (ADR-0018, ADR-0019), with bars by ID (I6).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MeterMapV1 {
    /// The meter at the very start (the map always has one, even without bars).
    pub(crate) initial: MeterV1,
    /// The meter changes after the first bar, in score order.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) changes: Vec<MeterChangeV1>,
    /// The irregular and free bars.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) irregular: Vec<IrregularV1>,
    /// The beat groupings of single bars.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) groupings: Vec<BarGroupingV1>,
    /// The bars not counted by the numbering.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) uncounted: Vec<u64>,
    /// The numbering restarts.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) restarts: Vec<RestartV1>,
}

/// A written meter and its beat grouping, if not the default rule.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MeterV1 {
    /// The meter.
    pub(crate) meter: MeterText,
    /// The grouping, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) grouping: Option<GroupingText>,
}

/// A meter change at a bar.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MeterChangeV1 {
    /// The bar.
    pub(crate) bar: u64,
    /// The meter.
    pub(crate) meter: MeterText,
    /// The grouping, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) grouping: Option<GroupingText>,
}

/// An irregular or free bar.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IrregularV1 {
    /// The bar.
    pub(crate) bar: u64,
    /// Its length in whole notes.
    pub(crate) length: Dur,
    /// How it is aligned, or `free`.
    pub(crate) kind: BarKindName,
}

/// The beat grouping of one bar.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BarGroupingV1 {
    /// The bar.
    pub(crate) bar: u64,
    /// The grouping.
    pub(crate) grouping: GroupingText,
}

/// A numbering restart.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RestartV1 {
    /// The bar.
    pub(crate) bar: u64,
    /// The number it shows.
    pub(crate) number: u32,
}

/// A tempo (ADR-0020): a beat unit and its beats per minute.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TempoV1 {
    /// The beat unit in whole notes (`"1/4"`).
    pub(crate) unit: Dur,
    /// Beats per minute, exact.
    pub(crate) bpm: Rat,
}

/// A metronome mark: a tempo and whether it is printed.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MetronomeV1 {
    /// The beat unit.
    pub(crate) unit: Dur,
    /// Beats per minute.
    pub(crate) bpm: Rat,
    /// Whether "♩ = 120" is printed.
    pub(crate) shown: bool,
}

/// A gradual tempo change (ADR-0021).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GradualV1 {
    /// The starting tempo, if the mark sets one ("Meno mosso, poco a poco accel.").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) from: Option<MetronomeV1>,
    /// The tempo reached.
    pub(crate) to: TempoV1,
    /// The bar where it is reached.
    pub(crate) end_bar: u64,
    /// The offset in that bar.
    pub(crate) end_at: Dur,
}

/// What a tempo mark changes: an immediate tempo or a gradual change.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub(crate) enum TempoChangeV1 {
    /// An immediate tempo.
    Immediate(MetronomeV1),
    /// A gradual change.
    Gradual(GradualV1),
}

/// The tempo mark at the very start: always an immediate tempo.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InitialTempoV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// Its text ("Allegro"), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    /// The tempo it sets.
    pub(crate) change: TempoChangeV1,
}

/// A tempo mark after the start.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TempoMarkV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar.
    pub(crate) bar: u64,
    /// The offset in the bar.
    pub(crate) at: Dur,
    /// Its text, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    /// What it changes; absent for a text-only mark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) change: Option<TempoChangeV1>,
}

/// A key: fifths on the line of fifths and a mode (ADR-0023 §5).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyV1 {
    /// Sharps (positive) or flats (negative).
    pub(crate) fifths: i8,
    /// The mode.
    pub(crate) mode: ModeName,
}

/// A key change.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyChangeV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar.
    pub(crate) bar: u64,
    /// The offset in the bar.
    pub(crate) at: Dur,
    /// The new key.
    pub(crate) key: KeyV1,
}

/// What an instrument's key override says (ADR-0024 §9).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub(crate) enum KeyOverrideV1 {
    /// Its own key signature.
    Key(KeyV1),
    /// No key signature (horns, timpani).
    NoSignature,
    /// Back to the score's key.
    FollowScore,
}

/// An instrument's key override.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyOverrideMarkV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar.
    pub(crate) bar: u64,
    /// The offset in the bar.
    pub(crate) at: Dur,
    /// What it says.
    pub(crate) key: KeyOverrideV1,
}

/// A start repeat barline.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepeatStartV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar it starts.
    pub(crate) bar: u64,
}

/// An end repeat barline.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepeatEndV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar it ends.
    pub(crate) bar: u64,
    /// The total number of plays.
    pub(crate) times: u8,
}

/// A volta.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VoltaV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// Its first bar.
    pub(crate) first: u64,
    /// Its last bar.
    pub(crate) last: u64,
    /// The passes it is played on, ascending.
    pub(crate) passes: Vec<u8>,
    /// Whether its bracket is closed at the end.
    pub(crate) closed: bool,
    /// Its own text, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
}

/// A form marker.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MarkerV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar.
    pub(crate) bar: u64,
    /// What it is.
    pub(crate) kind: MarkerKindName,
    /// Its own text, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
}

/// Where a jump goes.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub(crate) enum JumpTargetV1 {
    /// The start of the score (D.C.).
    Start,
    /// A marker (D.S.), by ID.
    Marker(u64),
}

/// When a jump is taken.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub(crate) enum JumpFiresV1 {
    /// After the repeats of its section.
    AfterRepeats,
    /// On this pass.
    OnPass(u8),
}

/// A jump.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct JumpV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar at whose end it is.
    pub(crate) bar: u64,
    /// Where it goes.
    pub(crate) to: JumpTargetV1,
    /// The marker where the jumped-to music ends, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) until: Option<u64>,
    /// The marker played next, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) then: Option<u64>,
    /// Whether repeats are played again after it.
    pub(crate) repeats: RepeatsName,
    /// When it is taken.
    pub(crate) fires: JumpFiresV1,
    /// Its own text, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
}

/// One track and its instrument.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrackV1 {
    /// The track's ID.
    pub(crate) id: u64,
    /// The instrument's ID.
    pub(crate) instrument: u64,
    /// The instrument's name.
    pub(crate) name: String,
    /// From written to sounding pitch.
    pub(crate) transposition: IntervalV1,
    /// The staves, top to bottom.
    pub(crate) staves: Vec<StaffV1>,
    /// The voices.
    pub(crate) voices: Vec<VoiceV1>,
    /// The key overrides.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) key_overrides: Vec<KeyOverrideMarkV1>,
    /// The slurs.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) slurs: Vec<SlurV1>,
    /// The dynamics.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) dynamics: Vec<DynamicV1>,
    /// The hairpins.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) hairpins: Vec<HairpinV1>,
    /// The bar repeats.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) bar_repeats: Vec<BarRepeatV1>,
    /// The pass rules.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) pass_rules: Vec<PassRuleV1>,
    /// The pass-specific offsets.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) pass_offsets: Vec<PassOffsetV1>,
}

/// An interval: diatonic steps and exact semitones (ADR-0023 §4).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IntervalV1 {
    /// Diatonic steps (negative: down).
    pub(crate) steps: i32,
    /// Semitones, exact.
    pub(crate) semitones: Rat,
}

/// One staff.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StaffV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// Its clef at the start.
    pub(crate) clef: ClefText,
    /// Its clef changes.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) clef_changes: Vec<ClefChangeV1>,
}

/// A clef change.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ClefChangeV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar.
    pub(crate) bar: u64,
    /// The offset in the bar.
    pub(crate) at: Dur,
    /// The new clef.
    pub(crate) clef: ClefText,
}

/// One voice and its content.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VoiceV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// Its home staff.
    pub(crate) home_staff: u64,
    /// Its tuplets, by ID.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) tuplets: Vec<TupletV1>,
    /// Its non-empty cells, in score order.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) cells: Vec<CellV1>,
}

/// A tuplet (ADR-0024 §3).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TupletV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// Notes played (the 3 of 3:2).
    pub(crate) actual: u8,
    /// In the time of (the 2 of 3:2).
    pub(crate) normal: u8,
    /// The unit the ratio counts.
    pub(crate) unit: ValueText,
    /// The enclosing tuplet, if nested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) parent: Option<u64>,
    /// Its bracket: a display choice, always written.
    pub(crate) bracket: BracketName,
    /// Its number: a display choice, always written.
    pub(crate) number: TupletNumberName,
}

/// The events of one voice in one bar.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CellV1 {
    /// The bar.
    pub(crate) bar: u64,
    /// The events, in chunk order.
    pub(crate) events: Vec<EventV1>,
}

/// A grace note's kind and attachment (ADR-0024 §5).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GraceV1 {
    /// Acciaccatura or appoggiatura.
    pub(crate) kind: GraceKindName,
    /// Before or after its main event.
    pub(crate) attach: GraceAttachName,
}

/// One event: a chord (with `value` and `notes`) or a rest (with `rest`), exactly one of the two
/// (I3; checked by the conversion, which names the event's ID).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// Its offset from the barline.
    pub(crate) at: Dur,
    /// The staff it is drawn on.
    pub(crate) staff: u64,
    /// A chord's written value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) value: Option<ValueText>,
    /// A rest's value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) rest: Option<RestText>,
    /// The innermost tuplet containing it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tuplet: Option<u64>,
    /// A chord's grace note kind, if it is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) grace: Option<GraceV1>,
    /// A chord's articulations, in the model's fixed order.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) articulations: Vec<ArticulationName>,
    /// A chord's ornament.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) ornament: Option<OrnamentText>,
    /// The fermata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) fermata: Option<FermataName>,
    /// A chord's arpeggio sign.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) arpeggio: Option<ArpeggioName>,
    /// A chord's noteheads, lowest first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) notes: Option<Vec<NoteV1>>,
}

/// Where a notehead's tie goes: a notehead's ID, or `"let_ring"`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum TieV1 {
    /// Tied to this notehead.
    To(u64),
    /// Let ring.
    LetRing(LetRing),
}

/// The word `"let_ring"`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LetRing {
    /// The only value.
    LetRing,
}

/// A notehead's performance offsets (ADR-0024 §10); zero components omitted.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OffsetsV1 {
    /// Onset offset in whole notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) onset: Option<Dur>,
    /// Length offset in whole notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) length: Option<Dur>,
    /// Velocity offset (16-bit scale, ADR-0027).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) velocity: Option<i32>,
}

/// One notehead.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NoteV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// Its spelled pitch.
    pub(crate) pitch: PitchText,
    /// Its tie, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) tie: Option<TieV1>,
    /// Its performance offsets, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) offsets: Option<OffsetsV1>,
}

/// A slur.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SlurV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The event it starts on.
    pub(crate) start: u64,
    /// The event it ends on.
    pub(crate) end: u64,
    /// Where it is drawn, if not left to the program.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) placement: Option<SlurPlacementName>,
}

/// A dynamic: a named mark, or a free text with an optional level (exactly one of the two).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DynamicV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar.
    pub(crate) bar: u64,
    /// The offset in the bar.
    pub(crate) at: Dur,
    /// The staff it is drawn on.
    pub(crate) staff: u64,
    /// The voice it applies to; absent: the whole instrument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) voice: Option<u64>,
    /// A named mark: a level (`"mf"`) or an accent mark (`"sfz"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) mark: Option<String>,
    /// A free text ("dolce"), instead of a named mark.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
    /// The level a free text plays at, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) level: Option<LevelName>,
}

/// A hairpin.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HairpinV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar it starts in.
    pub(crate) bar: u64,
    /// The offset in that bar.
    pub(crate) at: Dur,
    /// The bar it ends in.
    pub(crate) end_bar: u64,
    /// The offset in that bar.
    pub(crate) end_at: Dur,
    /// The staff it is drawn on.
    pub(crate) staff: u64,
    /// The voice it applies to; absent: the whole instrument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) voice: Option<u64>,
    /// Crescendo or diminuendo.
    pub(crate) kind: HairpinKindName,
    /// Wedge or text: a display choice, always written.
    pub(crate) style: HairpinStyleName,
    /// Whether it starts or ends at niente.
    #[serde(default, skip_serializing_if = "is_false")]
    pub(crate) niente: bool,
}

/// A bar repeat.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BarRepeatV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The staff.
    pub(crate) staff: u64,
    /// The first repeated bar.
    pub(crate) first: u64,
    /// How many bars repeat.
    pub(crate) count: u32,
    /// How many bars each sign repeats.
    pub(crate) span: SpanName,
}

/// A pass rule.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PassRuleV1 {
    /// Its ID.
    pub(crate) id: u64,
    /// The bar it starts in.
    pub(crate) bar: u64,
    /// The offset in that bar.
    pub(crate) at: Dur,
    /// The bar it ends in.
    pub(crate) end_bar: u64,
    /// The offset in that bar.
    pub(crate) end_at: Dur,
    /// The voice it applies to; absent: the whole instrument.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) voice: Option<u64>,
    /// The passes on which the passage is played, ascending.
    pub(crate) plays: Vec<u8>,
    /// Its own text, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) text: Option<String>,
}

/// A pass-specific offset (ADR-0028 §5).
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PassOffsetV1 {
    /// The notehead.
    pub(crate) note: u64,
    /// The pass index, from 0.
    pub(crate) pass: u32,
    /// The offsets on that pass.
    pub(crate) offsets: OffsetsV1,
}

/// The project's settings.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SettingsV1 {
    /// The tolerance threshold of the DAW move (ADR-0028): a setting, always written.
    pub(crate) tolerance: ToleranceText,
    /// The per-track overrides.
    #[serde(default, skip_serializing_if = "is_empty")]
    pub(crate) track_tolerances: Vec<TrackToleranceV1>,
}

/// A per-track tolerance override.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrackToleranceV1 {
    /// The track.
    pub(crate) track: u64,
    /// Its threshold.
    pub(crate) tolerance: ToleranceText,
}

#[cfg(test)]
mod tests {
    //! The text forms of the schema's few tagged enumerations, which the golden files do not all
    //! use (a jump taken on a given pass, a jump to the start).
    use super::*;

    /// `"fires"` and `"to"` of a jump, both ways.
    #[test]
    fn jump_enumerations() {
        let cases = [
            (JumpFiresV1::AfterRepeats, "\"after_repeats\""),
            (JumpFiresV1::OnPass(2), "{\"on_pass\":2}"),
        ];
        for (value, text) in cases {
            assert_eq!(serde_json::to_string(&value).ok().as_deref(), Some(text));
            let back: Result<JumpFiresV1, _> = serde_json::from_str(text);
            assert!(back.is_ok(), "{text}");
        }
        assert_eq!(
            serde_json::to_string(&JumpTargetV1::Start).ok().as_deref(),
            Some("\"start\"")
        );
        assert_eq!(
            serde_json::to_string(&JumpTargetV1::Marker(7))
                .ok()
                .as_deref(),
            Some("{\"marker\":7}")
        );
        assert!(serde_json::from_str::<JumpFiresV1>("{\"on_pass\":2,\"x\":1}").is_err());
    }
}
