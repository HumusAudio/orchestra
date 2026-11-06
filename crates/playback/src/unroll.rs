// SPDX-License-Identifier: GPL-3.0-or-later

//! The unrolling of the form: the bars in the order they are played (ADR-0025 §5; SCALETTA
//! F1.12, P1–P8).
//!
//! # What it produces
//! A **path**: one [`PlayedBar`] per bar played, in playing order, each knowing the written bar it
//! comes from, its **pass index** (how many times that written bar was played before, from 0),
//! the play number of its repeated section (for the "tacet 1st time" rules), and where it starts
//! on the unrolled timeline, in exact musical time and in seconds (P2). Around the path:
//! - for every written bar, the places on the path where it is played (P5);
//! - for every staff with bar repeats (%), which written bar each played bar takes its notes from
//!   on that staff, with the pass index of those notes (P3);
//! - a query telling whether the pass rules let a voice play at a point of a played bar (P4).
//!
//! It is **derived data** (ADR-0003): a cache built from the document and thrown away or rebuilt
//! at will, never a second editable copy of the music (P1).
//!
//! # The walk (ADR-0025 §5)
//! 1. Start at bar 0 with no jump taken, and walk the bars in order.
//! 2. Before playing a bar that opens a volta: skip the volta (to the bar after it) unless its
//!    passes contain the current play of its section; with the repeats skipped after a jump, only
//!    the last volta of a group is entered.
//! 3. Play the bar.
//! 4. At its end, in this order: the active jump's "until" marker (a Fine stops the path, a "To
//!    Coda" goes on at the coda); a jump not taken yet that fires now (after the section's last
//!    pass, or on the pass `OnPass` names), which becomes the active jump; an end repeat whose
//!    section has plays left (and whose repeats are not skipped after a jump), which goes back to
//!    the section's start with one more play.
//! 5. The path ends after the last bar.
//!
//! A section entered from outside starts at play 1; after a jump that skips the repeats every
//! section is played once, as play 1 (ADR-0025 §8).
//!
//! # Seconds (P2)
//! Inside a gradual tempo change the time of a barline is irrational (ADR-0021), so a played bar
//! stores the **shift** between its time on the unrolled timeline and the time of its written
//! start. The shift changes only where the path jumps (a repeat, a D.S.); the time of a point of a
//! played bar is the tempo map's time of the written point plus the shift. It is exact wherever the
//! tempo map is, and wherever a jump leaves and lands outside a gradual change.
//!
//! # Updating (P7)
//! The path depends on the form, the bars, the meter map (for the musical positions) and the tempo
//! map (for the seconds), never on the events: [`Unrolled::update`] rebuilds it whole when a change
//! set reports one of those, and does nothing otherwise. The rebuild is O(path).
//!
//! # Termination (P8)
//! Every jump is taken at most once and every section has at most `MAX_PLAYS` plays, so on a
//! valid document the path is finite; it is at most `bars × MAX_PLAYS × (jumps + 1)` long.

use std::error::Error;
use std::fmt;

use orchestra_core_model::{
    BarId, ChangeSet, Document, DocumentError, DynamicScope, JumpFires, JumpTarget,
    RepeatsAfterJump, StaffId, VoiceId,
};
use orchestra_core_time::{
    BarIndex, Duration, MeterMapError, Position, RationalError, Seconds, TempoMapError, TimeAt,
};

// ------------------------------------------------------------------------------------------------
// Errors
// ------------------------------------------------------------------------------------------------

/// Why the path could not be built. On a valid document only the arithmetic can fail (absurd
/// lengths or tempi); the other variants report a bug of the model rather than a panic.
///
/// The `Display` texts are English and meant for developers; the interface maps each variant to a
/// Fluent key (CLAUDE.md §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnrollError {
    /// The document answered with an error (an unknown bar: a bug of the model).
    Document(DocumentError),
    /// The meter map answered with an error.
    MeterMap(MeterMapError),
    /// The tempo map answered with an error.
    TempoMap(TempoMapError),
    /// The exact arithmetic overflowed.
    Arithmetic(RationalError),
    /// The path grew past its bound: the form is not valid (a bug of the model's `validate`).
    Endless,
}

impl fmt::Display for UnrollError {
    /// Writes a short English description of the error, for developers.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(e) => write!(f, "document: {e}"),
            Self::MeterMap(e) => write!(f, "meter map: {e}"),
            Self::TempoMap(e) => write!(f, "tempo map: {e}"),
            Self::Arithmetic(e) => write!(f, "arithmetic: {e}"),
            Self::Endless => f.write_str("the form does not end (an invalid document)"),
        }
    }
}

impl Error for UnrollError {
    /// The underlying error, if any.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Document(e) => Some(e),
            Self::MeterMap(e) => Some(e),
            Self::TempoMap(e) => Some(e),
            Self::Arithmetic(e) => Some(e),
            Self::Endless => None,
        }
    }
}

impl From<DocumentError> for UnrollError {
    /// Wraps a document error, so that `?` works.
    fn from(e: DocumentError) -> Self {
        Self::Document(e)
    }
}

impl From<MeterMapError> for UnrollError {
    /// Wraps a meter map error, so that `?` works.
    fn from(e: MeterMapError) -> Self {
        Self::MeterMap(e)
    }
}

impl From<TempoMapError> for UnrollError {
    /// Wraps a tempo map error, so that `?` works.
    fn from(e: TempoMapError) -> Self {
        Self::TempoMap(e)
    }
}

impl From<RationalError> for UnrollError {
    /// Wraps an arithmetic error, so that `?` works.
    fn from(e: RationalError) -> Self {
        Self::Arithmetic(e)
    }
}

// ------------------------------------------------------------------------------------------------
// The path
// ------------------------------------------------------------------------------------------------

/// One bar of the path (P2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayedBar {
    /// The written bar it plays.
    pub bar: BarId,
    /// Its index among the written bars.
    pub index: BarIndex,
    /// How many times this written bar was played before on the path, from 0 (ADR-0025 §5): the
    /// key of the pass-specific performance offsets of ADR-0024 §10 (with the note's ID, P6).
    pub pass: u32,
    /// The play number of the repeated section holding the bar (1 for its first play), or `None`
    /// outside any repeated section: the pass numbers the pass rules name (ADR-0025 §7).
    pub section_play: Option<u8>,
    /// Where its written bar starts in the score, in whole notes (the meter map's bar start): the
    /// written position of every point of the bar is this plus its offset. Kept here because the
    /// playback rules read it for every note (ADR-0027 §14 H2, H8) and the walk computes it anyway.
    pub written_start: Position,
    /// Where it starts on the unrolled timeline, in whole notes from the start of the performance.
    pub start: Position,
    /// The shift between its time on the unrolled timeline and the time of its written start (see
    /// the module documentation): what to add to the tempo map's time of a written point of the
    /// bar to get its time in the performance.
    pub shift: TimeAt,
}

/// The notes a staff plays in one played bar, when the staff has bar repeats (P3): the written bar
/// they come from and how many times those notes were played before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaffContent {
    /// The written bar whose notes the staff plays.
    pub bar: BarId,
    /// How many times those notes, on this staff, were played before, from 0.
    pub pass: u32,
}

/// The unrolled form of a document (see the module documentation).
#[derive(Debug, Clone, PartialEq)]
pub struct Unrolled {
    /// The played bars, in playing order.
    played: Vec<PlayedBar>,
    /// For each written bar (by index), the places on the path where it is played, ascending.
    plays_of: Vec<Vec<u32>>,
    /// For each staff with bar repeats, its content in every played bar (as long as `played`).
    staves: Vec<(StaffId, Vec<StaffContent>)>,
    /// The end of the performance, in whole notes.
    end: Position,
}

/// A repeated section, read from the form.
#[derive(Debug, Clone)]
struct Section {
    /// The index of its first bar.
    start: u32,
    /// The indices of the bars at whose end it repeats (more than one with voltas: "1. :‖ 2. :‖").
    ends: Vec<u32>,
    /// The index of its last bar: its last end repeat, or the last bar of its last volta.
    last: u32,
    /// How many times it is played.
    plays: u8,
}

/// A volta, read from the form, with the section it belongs to.
#[derive(Debug, Clone, Copy)]
struct VoltaInfo {
    /// The index of its last bar.
    last: u32,
    /// The passes it is played on.
    passes: orchestra_core_model::PassSet,
    /// Whether it is the last volta of its group.
    is_last: bool,
    /// The section it belongs to.
    section: usize,
}

/// A jump, read from the form, by bar indices.
#[derive(Debug, Clone, Copy)]
struct JumpInfo {
    /// Where it goes: a bar index (0 for D.C.).
    to: u32,
    /// The index of the bar at whose end the path stops (a Fine) or leaves for the coda (a "To
    /// Coda"), if any, and the bar where the coda starts for a "To Coda".
    until: Option<(u32, Option<u32>)>,
    /// Whether the repeats are skipped after it.
    skip_repeats: bool,
    /// When it is taken.
    fires: JumpFires,
}

/// The form of a document, read once into indices (O(f) for the f form markings).
struct FormIndex {
    /// The repeated sections, in score order.
    sections: Vec<Section>,
    /// The section holding each bar, if any.
    section_of: Vec<Option<usize>>,
    /// The volta starting at each bar, if any.
    volta_at: Vec<Option<VoltaInfo>>,
    /// The jump at the end of each bar, if any (at most one per bar, ADR-0025 §8), by its index
    /// in `jumps`.
    jump_at: Vec<Option<usize>>,
    /// The jumps.
    jumps: Vec<JumpInfo>,
}

impl FormIndex {
    /// Reads the form of `doc`. O(bars + f log f).
    ///
    /// # Errors
    /// [`UnrollError::Document`] for a marking on an unknown bar (a bug of the model).
    #[allow(
        clippy::too_many_lines,
        reason = "the three readings (sections, voltas, jumps) in order, each short"
    )]
    fn read(doc: &Document) -> Result<Self, UnrollError> {
        let bars = doc.bars();
        let total = bars.len();
        let index = |bar: BarId| -> Result<u32, UnrollError> {
            Ok(bars
                .index_of(bar)
                .ok_or(DocumentError::UnknownBar(bar))?
                .get())
        };
        let form = doc.form();

        // Sections: starts and ends merged in score order, as `validate` pairs them (rule 8): an
        // end closes the open start; an end without one is the first end of the score (from bar
        // 0) or a further end of the voltas of the section just closed.
        let mut marks: Vec<(u32, u8, Option<u8>)> = Vec::new();
        for r in form.repeat_starts() {
            marks.push((index(r.bar())?, 0, None));
        }
        for r in form.repeat_ends() {
            marks.push((index(r.bar())?, 1, Some(r.times())));
        }
        marks.sort_unstable();
        let mut sections: Vec<Section> = Vec::new();
        let mut open: Option<u32> = None;
        for (bar, kind, times) in marks {
            if kind == 0 {
                open = Some(bar);
                continue;
            }
            let times = times.unwrap_or(2);
            match open.take() {
                Some(start) => sections.push(Section {
                    start,
                    ends: vec![bar],
                    last: bar,
                    plays: times,
                }),
                None => match sections.last_mut() {
                    Some(last) => {
                        last.ends.push(bar);
                        last.last = last.last.max(bar);
                    }
                    None => sections.push(Section {
                        start: 0,
                        ends: vec![bar],
                        last: bar,
                        plays: times,
                    }),
                },
            }
        }

        // Voltas: grouped in score order (a group starts at the volta holding pass 1); a group
        // belongs to the section one of whose end repeats closes one of its voltas, and makes the
        // section's plays its highest pass and its last bar the group's last.
        let mut voltas: Vec<(u32, u32, orchestra_core_model::PassSet)> = Vec::new();
        for v in form.voltas() {
            voltas.push((index(v.first())?, index(v.last())?, v.passes()));
        }
        voltas.sort_unstable_by_key(|v| v.0);
        let mut volta_at: Vec<Option<VoltaInfo>> = vec![None; total];
        let mut group: Vec<(u32, u32, orchestra_core_model::PassSet)> = Vec::new();
        let mut groups: Vec<Vec<(u32, u32, orchestra_core_model::PassSet)>> = Vec::new();
        for v in voltas {
            if v.2.contains(1) && !group.is_empty() {
                groups.push(std::mem::take(&mut group));
            }
            group.push(v);
        }
        if !group.is_empty() {
            groups.push(group);
        }
        for g in groups {
            let Some(section) = sections
                .iter()
                .position(|s| g.iter().any(|v| s.ends.contains(&v.1)))
            else {
                continue;
            };
            let plays = g.iter().filter_map(|v| v.2.last()).max().unwrap_or(2);
            let last_bar = g.last().map_or(0, |v| v.1);
            if let Some(s) = sections.get_mut(section) {
                s.plays = plays;
                s.last = s.last.max(last_bar);
            }
            let count = g.len();
            for (k, v) in g.into_iter().enumerate() {
                if let Some(slot) = volta_at.get_mut(usize::try_from(v.0).unwrap_or(usize::MAX)) {
                    *slot = Some(VoltaInfo {
                        last: v.1,
                        passes: v.2,
                        is_last: k.saturating_add(1) == count,
                        section,
                    });
                }
            }
        }
        let mut section_of: Vec<Option<usize>> = vec![None; total];
        for (k, s) in sections.iter().enumerate() {
            for b in s.start..=s.last {
                if let Some(slot) = section_of.get_mut(usize::try_from(b).unwrap_or(usize::MAX)) {
                    *slot = Some(k);
                }
            }
        }

        // Jumps, with their markers' bars.
        let marker_bar = |id| -> Result<u32, UnrollError> {
            let marker = doc.marker(id).ok_or(DocumentError::UnknownMarker(id))?;
            index(marker.bar())
        };
        let mut jumps = Vec::new();
        let mut jump_at = vec![None; total];
        for j in form.jumps() {
            let spec = j.spec();
            let to = match spec.to {
                JumpTarget::Start => 0,
                JumpTarget::Marker(m) => marker_bar(m)?,
            };
            let until = match spec.until {
                Some(m) => Some((marker_bar(m)?, spec.then.map(marker_bar).transpose()?)),
                None => None,
            };
            let bar = index(j.bar())?;
            if let Some(slot) = jump_at.get_mut(usize::try_from(bar).unwrap_or(usize::MAX)) {
                *slot = Some(jumps.len());
            }
            jumps.push(JumpInfo {
                to,
                until,
                skip_repeats: spec.repeats == RepeatsAfterJump::Skip,
                fires: spec.fires,
            });
        }
        // A Fine or a "To Coda" is an end of the path only through a jump that names it, so the
        // markers need no table of their own: the jumps carry their bars.
        Ok(Self {
            sections,
            section_of,
            volta_at,
            jump_at,
            jumps,
        })
    }
}

/// The time `a + b`, exact if both are. O(1).
fn add_times(a: TimeAt, b: TimeAt) -> Result<TimeAt, RationalError> {
    Ok(match (a, b) {
        (TimeAt::Exact(x), TimeAt::Exact(y)) => TimeAt::Exact(x.checked_add(y)?),
        _ => TimeAt::Approximate(a.to_f64() + b.to_f64()),
    })
}

/// The time `a − b`, exact if both are. O(1).
fn sub_times(a: TimeAt, b: TimeAt) -> Result<TimeAt, RationalError> {
    Ok(match (a, b) {
        (TimeAt::Exact(x), TimeAt::Exact(y)) => TimeAt::Exact(x.checked_sub(y)?),
        _ => TimeAt::Approximate(a.to_f64() - b.to_f64()),
    })
}

impl Unrolled {
    /// Unrolls the form of `doc` (see the module documentation).
    ///
    /// # Complexity
    /// O(bars + f log f) to read the form, then O(1) amortised per played bar for the walk, plus
    /// O(log t) per played bar for the times (the tempo map's lookup), plus O(path) per staff with
    /// bar repeats. Allocates the path and its indices.
    ///
    /// # Errors
    /// [`UnrollError`]: arithmetic only on a valid document.
    #[allow(
        clippy::too_many_lines,
        reason = "the steps of the walk in the order of ADR-0025 §5: split, they would hide it"
    )]
    pub fn new(doc: &Document) -> Result<Self, UnrollError> {
        let form = FormIndex::read(doc)?;
        let total = u32::try_from(doc.bars().len()).map_err(|_| UnrollError::Endless)?;
        let meter = doc.meter_map();
        let tempo = doc.tempo_map();
        // The bound of P8: no valid form plays more than this.
        let bound = u64::from(total)
            .saturating_mul(u64::from(orchestra_core_model::MAX_PLAYS))
            .saturating_mul(
                u64::try_from(form.jumps.len())
                    .unwrap_or(u64::MAX)
                    .saturating_add(1),
            );

        let mut played: Vec<PlayedBar> = Vec::new();
        let mut counts: Vec<u32> = vec![0; usize::try_from(total).unwrap_or(0)];
        let mut taken = vec![false; form.jumps.len()];
        let mut active: Option<usize> = None;
        let mut skip_repeats = false;
        // The current section and its play; `None` outside sections.
        let mut current: Option<(usize, u8)> = None;
        let mut start = Position::ZERO;
        // The written end of the previous played bar (position and time), to find the shift where
        // the path jumps.
        let mut previous_end: Option<(Position, TimeAt)> = None;
        let mut shift = TimeAt::Exact(Seconds::ZERO);
        let mut bar = 0_u32;

        while bar < total {
            // The section of this bar: entered from outside, it starts at play 1.
            let section = form
                .section_of
                .get(usize::try_from(bar).unwrap_or(usize::MAX))
                .copied()
                .flatten();
            current = match (section, current) {
                (Some(s), Some((c, play))) if s == c => Some((c, play)),
                (Some(s), _) => Some((s, 1)),
                (None, _) => None,
            };
            // Rule 3: voltas that this play does not enter are skipped.
            if let Some(volta) = form
                .volta_at
                .get(usize::try_from(bar).unwrap_or(usize::MAX))
                .copied()
                .flatten()
            {
                let play = current
                    .filter(|(c, _)| *c == volta.section)
                    .map_or(1, |(_, p)| p);
                let enter = if skip_repeats {
                    volta.is_last
                } else {
                    volta.passes.contains(play)
                };
                if !enter {
                    bar = volta.last.saturating_add(1);
                    continue;
                }
            }

            // Play the bar.
            if u64::try_from(played.len()).unwrap_or(u64::MAX) >= bound {
                return Err(UnrollError::Endless);
            }
            let index = BarIndex::new(bar);
            let id = doc.bars().id_at(index).ok_or(UnrollError::Endless)?;
            let written_start = meter.bar_start(index)?;
            let length = meter.bar_length(index)?;
            let written_end = written_start.checked_add(length)?;
            let start_time = tempo.time_at(written_start)?;
            if let Some((end, end_time)) = previous_end
                && end != written_start
            {
                // Where the path jumps, the shift takes the difference between the time the
                // previous bar ended and the written time this bar starts at. Only there: between
                // consecutive bars the difference is zero, and computing it inside a gradual change
                // would turn an exact shift into an approximate one for nothing.
                shift = add_times(shift, sub_times(end_time, start_time)?)?;
            }
            let count = counts
                .get_mut(usize::try_from(bar).unwrap_or(usize::MAX))
                .ok_or(UnrollError::Endless)?;
            played.push(PlayedBar {
                bar: id,
                index,
                pass: *count,
                section_play: current.map(|(_, p)| p),
                written_start,
                start,
                shift,
            });
            *count = count.saturating_add(1);
            start = start.checked_add(length)?;
            previous_end = Some((written_end, tempo.time_at(written_end)?));

            // Rule 4, at the end of the bar. First the active jump's "until" marker.
            if let Some(j) = active
                && let Some(info) = form.jumps.get(j)
                && let Some((until, then)) = info.until
                && until == bar
            {
                match then {
                    None => break,
                    Some(coda) => {
                        active = None;
                        bar = coda;
                        continue;
                    }
                }
            }
            // Then a jump not taken yet that fires now.
            if let Some(j) = form
                .jump_at
                .get(usize::try_from(bar).unwrap_or(usize::MAX))
                .copied()
                .flatten()
                && !taken.get(j).copied().unwrap_or(true)
                && let Some(info) = form.jumps.get(j)
            {
                let fires = match (info.fires, current) {
                    (JumpFires::OnPass(n), Some((_, play))) => play == n,
                    // On the section's last play (once, after skipped repeats).
                    (JumpFires::AfterRepeats, Some((s, play))) => {
                        skip_repeats || form.sections.get(s).is_none_or(|sec| play >= sec.plays)
                    }
                    // Outside any repeated section, the first time it is reached.
                    (_, None) => true,
                };
                if fires {
                    if let Some(t) = taken.get_mut(j) {
                        *t = true;
                    }
                    active = Some(j);
                    skip_repeats = info.skip_repeats;
                    // The target is entered from outside: its section starts at play 1.
                    current = None;
                    bar = info.to;
                    continue;
                }
            }
            // Then an end repeat with plays left.
            if let Some((s, play)) = current
                && !skip_repeats
                && let Some(sec) = form.sections.get(s)
                && sec.ends.contains(&bar)
                && play < sec.plays
            {
                current = Some((s, play.saturating_add(1)));
                bar = sec.start;
                continue;
            }
            bar = bar.saturating_add(1);
        }

        // P5: where each written bar is played.
        let mut plays_of: Vec<Vec<u32>> = vec![Vec::new(); usize::try_from(total).unwrap_or(0)];
        for (k, p) in played.iter().enumerate() {
            if let Some(list) =
                plays_of.get_mut(usize::try_from(p.index.get()).unwrap_or(usize::MAX))
            {
                list.push(u32::try_from(k).map_err(|_| UnrollError::Endless)?);
            }
        }
        let staves = staff_contents(doc, &played)?;
        Ok(Self {
            played,
            plays_of,
            staves,
            end: start,
        })
    }

    /// Rebuilds the path if a change set reports a change it depends on: the form (bars inserted
    /// or removed included), the meter map or the tempo map (P7). Returns whether it rebuilt.
    /// O(1) when nothing it depends on changed, O(path) otherwise.
    ///
    /// # Errors
    /// As [`new`](Self::new); on error `self` is unchanged.
    pub fn update(&mut self, doc: &Document, changes: &ChangeSet) -> Result<bool, UnrollError> {
        if !changes.form_changed()
            && changes.meter_from().is_none()
            && changes.tempo_from().is_none()
        {
            return Ok(false);
        }
        *self = Self::new(doc)?;
        Ok(true)
    }

    /// The played bars, in playing order. O(1).
    #[must_use]
    pub fn played(&self) -> &[PlayedBar] {
        &self.played
    }

    /// The places on the path where the written bar at `index` is played, ascending (P5); empty
    /// for a bar never played (a volta skipped, the bars after a Fine) or past the end. O(1).
    #[must_use]
    pub fn plays_of(&self, index: BarIndex) -> &[u32] {
        usize::try_from(index.get())
            .ok()
            .and_then(|i| self.plays_of.get(i))
            .map_or(&[], Vec::as_slice)
    }

    /// The end of the performance on the unrolled timeline, in whole notes. O(1).
    #[must_use]
    pub const fn end(&self) -> Position {
        self.end
    }

    /// The written bar whose notes `staff` plays in the played bar at `played`, with the pass
    /// index of those notes (P3): the bar itself and its pass, unless the staff has a bar repeat
    /// there, in which case the source bar and the number of times its notes on that staff were
    /// played before. `None` past the end of the path. O(staves with bar repeats).
    #[must_use]
    pub fn content(&self, played: usize, staff: StaffId) -> Option<StaffContent> {
        if let Some((_, list)) = self.staves.iter().find(|(s, _)| *s == staff) {
            return list.get(played).copied();
        }
        self.played.get(played).map(|p| StaffContent {
            bar: p.bar,
            pass: p.pass,
        })
    }

    /// The time on the unrolled timeline of the written point `offset` of the played bar at
    /// `played` (see the module documentation): the tempo map's time of the written point plus
    /// the bar's shift. O(log t).
    ///
    /// # Errors
    /// [`UnrollError::Endless`] past the end of the path; the maps' and the arithmetic errors.
    pub fn time_at(
        &self,
        doc: &Document,
        played: usize,
        offset: Duration,
    ) -> Result<TimeAt, UnrollError> {
        let p = self.played.get(played).ok_or(UnrollError::Endless)?;
        let written = doc.meter_map().bar_start(p.index)?.checked_add(offset)?;
        Ok(add_times(doc.tempo_map().time_at(written)?, p.shift)?)
    }

    /// Whether the pass rules of the instrument holding `voice` let it play at the written point
    /// `offset` of the played bar at `played` (P4): every rule of the instrument whose scope is
    /// the whole instrument or this voice, and whose range holds the point, must name the play of
    /// the bar's repeated section (ADR-0025 §7). Filtering the notes is F1.13's job; this is the
    /// rule. O(r log m) for the r pass rules of the instrument.
    ///
    /// # Errors
    /// [`UnrollError::Endless`] past the end of the path; [`UnrollError::Document`] for an unknown
    /// voice; the meter map's errors.
    pub fn plays(
        &self,
        doc: &Document,
        voice: VoiceId,
        played: usize,
        offset: Duration,
    ) -> Result<bool, UnrollError> {
        let p = self.played.get(played).ok_or(UnrollError::Endless)?;
        let (instrument, _) = doc.voice(voice).ok_or(DocumentError::UnknownVoice(voice))?;
        let meter = doc.meter_map();
        let at = meter.bar_start(p.index)?.checked_add(offset)?;
        let position_of =
            |point: orchestra_core_model::ScorePoint| -> Result<Position, UnrollError> {
                let index = doc
                    .bars()
                    .index_of(point.bar)
                    .ok_or(DocumentError::UnknownBar(point.bar))?;
                Ok(meter.bar_start(index)?.checked_add(point.offset)?)
            };
        for rule in instrument.pass_rules() {
            let applies = match rule.scope() {
                DynamicScope::Instrument => true,
                DynamicScope::Voice(v) => v == voice,
            };
            if !applies {
                continue;
            }
            if position_of(rule.start())? <= at && at < position_of(rule.end())? {
                let play = p.section_play.unwrap_or(1);
                if !rule.plays().contains(play) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

/// P3: for every staff with bar repeats, its content in every played bar. The repeated bar
/// `first + j` of a bar repeat of span `s` plays the source bar `first − s + (j mod s)`; the pass
/// of the notes is the number of times that source bar's notes on that staff were played before
/// (by the bar itself or by a repeat of it).
///
/// # Complexity
/// O(path + bar repeats) per staff with bar repeats.
fn staff_contents(
    doc: &Document,
    played: &[PlayedBar],
) -> Result<Vec<(StaffId, Vec<StaffContent>)>, UnrollError> {
    let bars = doc.bars();
    let mut out: Vec<(StaffId, Vec<StaffContent>)> = Vec::new();
    for track in doc.tracks() {
        for repeat in track.instrument().bar_repeats() {
            let staff = repeat.staff();
            if out.iter().any(|(s, _)| *s == staff) {
                continue;
            }
            // Every bar repeat of this staff: written bar index → source bar index.
            let mut source: Vec<Option<u32>> = vec![None; bars.len()];
            for r in track
                .instrument()
                .bar_repeats()
                .iter()
                .filter(|r| r.staff() == staff)
            {
                let first = bars
                    .index_of(r.first())
                    .ok_or(DocumentError::UnknownBar(r.first()))?
                    .get();
                let span = r.span().bars();
                for j in 0..r.count() {
                    let repeated = first.saturating_add(j);
                    let from = first.saturating_sub(span).saturating_add(j % span);
                    if let Some(slot) =
                        source.get_mut(usize::try_from(repeated).unwrap_or(usize::MAX))
                    {
                        *slot = Some(from);
                    }
                }
            }
            let mut counts: Vec<u32> = vec![0; bars.len()];
            let mut list = Vec::with_capacity(played.len());
            for p in played {
                let written = p.index.get();
                let from = source
                    .get(usize::try_from(written).unwrap_or(usize::MAX))
                    .copied()
                    .flatten()
                    .unwrap_or(written);
                let count = counts
                    .get_mut(usize::try_from(from).unwrap_or(usize::MAX))
                    .ok_or(UnrollError::Endless)?;
                let bar = bars
                    .id_at(BarIndex::new(from))
                    .ok_or(UnrollError::Endless)?;
                list.push(StaffContent { bar, pass: *count });
                *count = count.saturating_add(1);
            }
            out.push((staff, list));
        }
    }
    Ok(out)
}
