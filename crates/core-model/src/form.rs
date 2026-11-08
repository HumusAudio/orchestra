// SPDX-License-Identifier: GPL-3.0-or-later

//! The form: repeats and voltas, with the bar-numbering style they imply (ADR-0025 §1–§3; SCALETTA
//! F1.10b). The document's markers and jumps are kept here too (their types are in
//! `crate::jump`); bar repeats and pass rules belong to their staff and instrument
//! (`crate::bar_repeat`).
//!
//! # On barlines, in the document
//! A repeat or a volta belongs to a **barline** (ADR-0025 F1): a `RepeatStart` to the start of a
//! bar, a `RepeatEnd` to its end, a volta to a run of whole bars. Nothing here has an offset, so the
//! unrolled path of F1.12 is always a sequence of whole bars. The form belongs to the **document**:
//! a repeat barline crosses the whole system, and every instrument plays the same passes.
//!
//! # Stored once, derived everything else
//! Only what is written is stored: where the repeat barlines and the voltas are, how many times a
//! section is played, which passes each volta is for. Which start an end pairs with, which voltas
//! belong together, how many times a section with voltas is played, and the displayed bar numbers
//! are **derived** from it, never stored next to it (ADR-0003).
//!
//! # Passes
//! A pass is one play of a repeated section, numbered from 1 as players read them ("1.", "2.").
//! [`PassSet`] keeps a set of them in one `u64`, bit `k − 1` for pass `k`, which is why the number
//! of plays is capped at [`MAX_PLAYS`] = 64 (ADR-0025 F10): far beyond real music, and every set
//! operation is one machine instruction.

use orchestra_core_time::BarIndex;

use crate::document::{Document, DocumentError};
use crate::id::{BarId, RepeatId, VoltaId};
use crate::jump::{Jump, Marker};
use crate::validate::Violation;

/// The most times a repeated section can be played (ADR-0025 F10): the size of a [`PassSet`]. A
/// technical limit against corrupt files, like the voice limit of ADR-0022; raising it changes the
/// width of the set, not the musical model.
pub const MAX_PLAYS: u8 = 64;

// ------------------------------------------------------------------------------------------------
// Pass sets
// ------------------------------------------------------------------------------------------------

/// A set of passes, numbered 1 to [`MAX_PLAYS`] (ADR-0025 §3): the passes a volta is played on
/// ("1.–3." = {1, 2, 3}).
///
/// One `u64`, bit `k − 1` for pass `k`: membership, union and disjointness are single bit
/// operations, the set is `Copy` and costs 8 bytes wherever it is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PassSet(u64);

impl PassSet {
    /// The empty set. A volta must have at least one pass; the builder refuses an empty set.
    pub const EMPTY: Self = Self(0);

    /// The bit of pass `pass`, or `None` outside 1–[`MAX_PLAYS`]. O(1).
    const fn bit(pass: u8) -> Option<u64> {
        if pass == 0 || pass > MAX_PLAYS {
            return None;
        }
        // `pass − 1` is in 0..64 here, so the shift never overflows.
        Some(1_u64 << (pass - 1))
    }

    /// The set with these passes. O(passes).
    ///
    /// # Errors
    /// [`DocumentError::PassOutOfRange`] for a pass outside 1–[`MAX_PLAYS`].
    pub fn of(passes: &[u8]) -> Result<Self, DocumentError> {
        let mut bits = 0_u64;
        for &pass in passes {
            bits |= Self::bit(pass).ok_or(DocumentError::PassOutOfRange(pass))?;
        }
        Ok(Self(bits))
    }

    /// The passes `first..=last` ("1.–3."). O(1).
    ///
    /// # Errors
    /// [`DocumentError::PassOutOfRange`] if either end is outside 1–[`MAX_PLAYS`]; an empty set if
    /// `last < first`.
    pub fn range(first: u8, last: u8) -> Result<Self, DocumentError> {
        let low = Self::bit(first).ok_or(DocumentError::PassOutOfRange(first))?;
        let high = Self::bit(last).ok_or(DocumentError::PassOutOfRange(last))?;
        if last < first {
            return Ok(Self::EMPTY);
        }
        // Every bit from `low` to `high` included: (high − low) + high, written so that it cannot
        // overflow even for pass 64 (`high` = 2⁶³).
        Ok(Self((high - low) | high))
    }

    /// Whether pass `pass` is in the set (`false` outside 1–[`MAX_PLAYS`]). O(1).
    #[must_use]
    pub const fn contains(self, pass: u8) -> bool {
        match Self::bit(pass) {
            Some(bit) => self.0 & bit != 0,
            None => false,
        }
    }

    /// Whether the set is empty. O(1).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many passes the set holds. O(1).
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// The lowest pass, if any. O(1).
    #[must_use]
    pub fn first(self) -> Option<u8> {
        // `trailing_zeros` is below 64 for a non-empty set, so the pass fits a `u8`.
        (!self.is_empty())
            .then(|| u8::try_from(self.0.trailing_zeros()).ok())
            .flatten()
            .map(|zeros| zeros.saturating_add(1))
    }

    /// The highest pass, if any. O(1).
    #[must_use]
    pub fn last(self) -> Option<u8> {
        (!self.is_empty())
            .then(|| u8::try_from(64 - self.0.leading_zeros()).ok())
            .flatten()
    }

    /// Whether the two sets have no pass in common. O(1).
    #[must_use]
    pub const fn is_disjoint(self, other: Self) -> bool {
        self.0 & other.0 == 0
    }

    /// The passes of either set. O(1).
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether the set is exactly the passes `1..=n` for some `n` ≥ 1: no gap, starting at 1.
    /// O(1): such a set is `2ⁿ − 1`, a run of ones from bit 0, so adding one clears every bit.
    #[must_use]
    pub const fn is_prefix(self) -> bool {
        self.0 != 0 && self.0 & self.0.wrapping_add(1) == 0
    }

    /// The raw bits, bit `k − 1` for pass `k` (what the file format stores, F1.15). O(1).
    #[must_use]
    pub const fn bits(self) -> u64 {
        self.0
    }
}

// ------------------------------------------------------------------------------------------------
// Repeats
// ------------------------------------------------------------------------------------------------

/// A start-repeat barline (‖:) at the start of a bar (ADR-0025 §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepeatStart {
    /// The barline's identity.
    id: RepeatId,
    /// The bar it starts.
    bar: BarId,
}

impl RepeatStart {
    /// A start repeat with its ID. O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// L1 of 2026-11-06): the value only *describes* an object. It enters a document only through
    /// [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as the builder
    /// checks what it creates, so no invalid document can come from it.
    pub const fn new(id: RepeatId, bar: BarId) -> Self {
        Self { id, bar }
    }

    /// This repeat start at another bar. O(1).
    ///
    /// For the commands that move or clip the object while keeping everything else (inserting,
    /// deleting and rewriting bars): they used to call `new` with every field copied by hand
    /// (review point B11).
    #[must_use]
    pub(crate) const fn with_bar(&self, bar: BarId) -> Self {
        Self { bar, ..*self }
    }

    /// The barline's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> RepeatId {
        self.id
    }

    /// The bar it starts. O(1).
    #[must_use]
    pub const fn bar(&self) -> BarId {
        self.bar
    }
}

/// An end-repeat barline (:‖) at the end of a bar, with the number of plays (ADR-0025 §2).
///
/// It goes back to the previous [`RepeatStart`], or to the start of the score if there is none
/// (F2). With voltas the number of plays is the highest volta number, and `times` must then be
/// either 2 (the default) or that number: a contradiction between the two is refused by
/// `validate` (owner's decision of 2026-11-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RepeatEnd {
    /// The barline's identity.
    id: RepeatId,
    /// The bar it ends.
    bar: BarId,
    /// The total number of plays of the section, 2 to [`MAX_PLAYS`].
    times: u8,
}

impl RepeatEnd {
    /// An end repeat with its ID. O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// L1 of 2026-11-06): the value only *describes* an object. It enters a document only through
    /// [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as the builder
    /// checks what it creates, so no invalid document can come from it.
    pub const fn new(id: RepeatId, bar: BarId, times: u8) -> Self {
        Self { id, bar, times }
    }

    /// This repeat end at another bar, played as many times. O(1).
    ///
    /// For the commands that move or clip the object while keeping everything else (inserting,
    /// deleting and rewriting bars): they used to call `new` with every field copied by hand
    /// (review point B11).
    #[must_use]
    pub(crate) const fn with_bar(&self, bar: BarId) -> Self {
        Self { bar, ..*self }
    }

    /// The barline's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> RepeatId {
        self.id
    }

    /// The bar it ends. O(1).
    #[must_use]
    pub const fn bar(&self) -> BarId {
        self.bar
    }

    /// The total number of plays written for the section ("3×" = 3; 2 by default). O(1).
    #[must_use]
    pub const fn times(&self) -> u8 {
        self.times
    }
}

// ------------------------------------------------------------------------------------------------
// Voltas
// ------------------------------------------------------------------------------------------------

/// A volta (first, second… ending): a run of whole bars played only on some passes (ADR-0025 §3).
///
/// The voltas of one repeated section are consecutive and in ascending order of their passes; all
/// but the last lie inside the section, and the last but one ends where the section's
/// [`RepeatEnd`] is. Their pass sets are disjoint and together are exactly `1..=n`, n being the
/// number of plays. `validate` checks all of it (rule 9).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Volta {
    /// The volta's identity.
    id: VoltaId,
    /// Its first bar.
    first: BarId,
    /// Its last bar (the same as `first` for a one-bar volta).
    last: BarId,
    /// The passes it is played on; never empty.
    passes: PassSet,
    /// Whether the bracket ends with a downward jog (MusicXML's `stop`; usual for a first ending)
    /// or stays open (`discontinue`; usual for a last ending).
    closed: bool,
    /// Printed instead of the numbers ("1.–7.", "Last time"), the user's own text; never empty.
    text: Option<Box<str>>,
}

impl Volta {
    /// A volta with its ID. O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// L1 of 2026-11-06): the value only *describes* an object. It enters a document only through
    /// [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as the builder
    /// checks what it creates, so no invalid document can come from it.
    pub const fn new(
        id: VoltaId,
        first: BarId,
        last: BarId,
        passes: PassSet,
        closed: bool,
        text: Option<Box<str>>,
    ) -> Self {
        Self {
            id,
            first,
            last,
            passes,
            closed,
            text,
        }
    }

    /// This volta over other first and last bars, the rest unchanged. O(text length).
    ///
    /// For the commands that move or clip the object while keeping everything else (inserting,
    /// deleting and rewriting bars): they used to call `new` with every field copied by hand
    /// (review point B11).
    #[must_use]
    pub(crate) fn with_bars(&self, first: BarId, last: BarId) -> Self {
        Self {
            first,
            last,
            ..self.clone()
        }
    }

    /// The volta's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> VoltaId {
        self.id
    }

    /// Its first bar. O(1).
    #[must_use]
    pub const fn first(&self) -> BarId {
        self.first
    }

    /// Its last bar. O(1).
    #[must_use]
    pub const fn last(&self) -> BarId {
        self.last
    }

    /// The passes it is played on. O(1).
    #[must_use]
    pub const fn passes(&self) -> PassSet {
        self.passes
    }

    /// Whether the bracket ends with a downward jog. O(1).
    #[must_use]
    pub const fn closed(&self) -> bool {
        self.closed
    }

    /// The text printed instead of the numbers, if any. O(1).
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

// ------------------------------------------------------------------------------------------------
// Bar numbering
// ------------------------------------------------------------------------------------------------

/// How the bars of later voltas are numbered: one style for the whole document (ADR-0025 §3, F9,
/// owner's decision of 2026-11-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BarNumbering {
    /// Every written bar takes the next number, voltas included: the number is the written bar,
    /// the DAW's ruler and "from bar 9" at a rehearsal. The default.
    #[default]
    Continue,
    /// The bars of every volta take the numbers of the first volta's bars, with a letter per volta
    /// ("8a", "8b").
    SameAsFirstLettered,
    /// The same numbers without letters ("8", "8").
    SameAsFirst,
}

/// The number shown above a bar: a number, and for the lettered style the volta it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BarLabel {
    /// The number.
    pub number: u32,
    /// With [`BarNumbering::SameAsFirstLettered`], the place of the bar's volta in its group (1
    /// for the first volta, shown "a"); otherwise `None`.
    pub volta: Option<u8>,
}

impl BarLabel {
    /// The letter of the volta ("a" for the first), if any; `None` past the 26th volta, which no
    /// real score reaches (the interface then shows the number of the volta). O(1).
    #[must_use]
    pub fn letter(self) -> Option<char> {
        let place = self.volta?;
        let offset = place.checked_sub(1).filter(|&o| o < 26)?;
        char::from_u32(u32::from(b'a') + u32::from(offset))
    }
}

// ------------------------------------------------------------------------------------------------
// The form of a document
// ------------------------------------------------------------------------------------------------

/// The form markings of a document (ADR-0025): sorted vectors, few per score (tens), each in
/// score order of its bar (voltas by their first bar). Sorting by bar index costs one O(1)
/// `BarId → index` lookup per comparison; inserting is O(f).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Form {
    /// The start repeats, sorted by bar.
    pub(crate) repeat_starts: Vec<RepeatStart>,
    /// The end repeats, sorted by bar.
    pub(crate) repeat_ends: Vec<RepeatEnd>,
    /// The voltas, sorted by first bar.
    pub(crate) voltas: Vec<Volta>,
    /// The markers, sorted by bar (at one bar the start marker before the end marker).
    pub(crate) markers: Vec<Marker>,
    /// The jumps, sorted by bar.
    pub(crate) jumps: Vec<Jump>,
    /// The bar-numbering style.
    pub(crate) numbering: BarNumbering,
}

impl Form {
    /// The start repeats, in score order. O(1).
    #[must_use]
    pub fn repeat_starts(&self) -> &[RepeatStart] {
        &self.repeat_starts
    }

    /// The end repeats, in score order. O(1).
    #[must_use]
    pub fn repeat_ends(&self) -> &[RepeatEnd] {
        &self.repeat_ends
    }

    /// The voltas, in score order. O(1).
    #[must_use]
    pub fn voltas(&self) -> &[Volta] {
        &self.voltas
    }

    /// The markers, in score order. O(1).
    #[must_use]
    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }

    /// The jumps, in score order. O(1).
    #[must_use]
    pub fn jumps(&self) -> &[Jump] {
        &self.jumps
    }

    /// The bar-numbering style. O(1).
    #[must_use]
    pub const fn numbering(&self) -> BarNumbering {
        self.numbering
    }
}

// ------------------------------------------------------------------------------------------------
// Validation (rules 8 and 9) and bar numbers
// ------------------------------------------------------------------------------------------------

/// What rule 9 remembers of the volta group being read: where it starts in the volta list, its
/// passes so far and the index of its last bar.
#[derive(Debug, Clone, Copy)]
struct Group {
    /// Index of the group's first volta in `Form::voltas`.
    start: usize,
    /// One past the index of its last volta.
    end: usize,
    /// The passes of its voltas so far.
    passes: PassSet,
    /// The bar index of its last bar so far.
    last_bar: u32,
}

impl Document {
    /// Checks rules 8 (repeats) and 9 (voltas) of [`Document::validate`] (ADR-0025 §8).
    ///
    /// # How
    /// Rule 9 first, because rule 8 needs to know which end repeats belong to the voltas of one
    /// section. The voltas are read once in order, split into groups (a group starts at the volta
    /// holding pass 1, and each later volta must follow the previous one directly with higher
    /// passes), and each group is checked when it ends. Then the start and end repeats are merged
    /// in score order (a start at the start of a bar comes before an end at the end of the same
    /// bar) and paired: a start must not find another start open, and an end without an open start
    /// is fine only as the first end of the score (it goes back to the start, F2) or as a further
    /// end of the voltas of the section just closed.
    ///
    /// # Complexity
    /// O(v log r + (r + s)·v) for v voltas, r end and s start repeats: tens of each in a real
    /// score. Never allocates.
    ///
    /// # Errors
    /// [`DocumentError::Invalid`] with the first violation found.
    pub(crate) fn check_form(&self) -> Result<(), DocumentError> {
        let bars = self.bars();
        let form = self.form();
        let voltas = form.voltas();

        // Rule 9: voltas.
        let mut group: Option<Group> = None;
        let mut previous_last: Option<u32> = None;
        for (i, volta) in voltas.iter().enumerate() {
            let first = bars.key(volta.first);
            let last = bars.key(volta.last);
            if previous_last.is_some_and(|p| first <= p) {
                return Err(invalid(Violation::VoltaOverlap(volta.id)));
            }
            previous_last = Some(last);
            if volta.passes.contains(1) {
                if let Some(done) = group.take() {
                    self.check_group(done)?;
                }
                group = Some(Group {
                    start: i,
                    end: i.saturating_add(1),
                    passes: volta.passes,
                    last_bar: last,
                });
                continue;
            }
            let Some(g) = group.as_mut() else {
                // A volta that is not the first of a group must follow one.
                return Err(invalid(Violation::VoltaOrder(volta.id)));
            };
            let follows = g.last_bar.checked_add(1) == Some(first);
            let higher = volta.passes.first() > g.passes.last();
            if !follows || !higher {
                return Err(invalid(Violation::VoltaOrder(volta.id)));
            }
            g.passes = g.passes.union(volta.passes);
            g.last_bar = last;
            g.end = i.saturating_add(1);
        }
        if let Some(done) = group {
            self.check_group(done)?;
        }

        // Rule 8: repeats, merged in score order.
        let starts = form.repeat_starts();
        let ends = form.repeat_ends();
        let (mut s, mut e) = (0_usize, 0_usize);
        let mut open: Option<RepeatId> = None;
        let mut seen_end = false;
        loop {
            let next_start = starts.get(s).map(|r| (bars.key(r.bar), 0_u8, r.id));
            let next_end = ends.get(e).map(|r| (bars.key(r.bar), 1_u8, r.id));
            let take_start = match (next_start, next_end) {
                (None, None) => break,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (Some(a), Some(b)) => (a.0, a.1) < (b.0, b.1),
            };
            if take_start {
                let id = starts.get(s).map(RepeatStart::id);
                s = s.saturating_add(1);
                if open.is_some() {
                    return Err(invalid(Violation::RepeatsOverlap(
                        id.ok_or(DocumentError::DuplicateMark)?,
                    )));
                }
                open = id;
            } else {
                let Some(end) = ends.get(e) else { break };
                e = e.saturating_add(1);
                let paired = open.take().is_some();
                if !paired && seen_end && !self.is_volta_continuation(end) {
                    return Err(invalid(Violation::RepeatsOverlap(end.id)));
                }
                seen_end = true;
            }
        }
        Ok(())
    }

    /// Checks one finished volta group (rule 9): at least two voltas, passes exactly `1..=n`,
    /// every volta but the last ending with an end repeat whose `times` is 2 or n, and no start
    /// repeat inside the group. O(k log r + s) for k voltas.
    fn check_group(&self, group: Group) -> Result<(), DocumentError> {
        let bars = self.bars();
        let form = self.form();
        let voltas = form.voltas().get(group.start..group.end).unwrap_or(&[]);
        let (Some(first), Some((last, inner))) = (voltas.first(), voltas.split_last()) else {
            return Ok(());
        };
        if inner.is_empty() {
            return Err(invalid(Violation::VoltaWithoutRepeat(first.id)));
        }
        if !group.passes.is_prefix() {
            return Err(invalid(Violation::VoltaPassesIncomplete(first.id)));
        }
        let plays = group.passes.last().unwrap_or(2);
        for volta in inner {
            let at = bars.key(volta.last);
            let end = form
                .repeat_ends()
                .binary_search_by_key(&at, |r| bars.key(r.bar))
                .ok()
                .and_then(|i| form.repeat_ends().get(i))
                .ok_or(invalid(Violation::VoltaWithoutRepeat(volta.id)))?;
            if end.times != 2 && end.times != plays {
                return Err(invalid(Violation::RepeatTimesMismatch(end.id)));
            }
        }
        // No start repeat strictly after the group's first bar and up to its last one.
        let (from, to) = (bars.key(first.first), bars.key(last.last));
        if let Some(start) = form.repeat_starts().iter().find(|r| {
            let at = bars.key(r.bar);
            at > from && at <= to
        }) {
            return Err(invalid(Violation::RepeatsOverlap(start.id)));
        }
        Ok(())
    }

    /// Whether `end` is a further end repeat of a volta group: it closes a volta that is neither
    /// the first nor the last of its group (in "1. :‖ 2. :‖ 3.", the end of the second volta).
    /// O(v): a linear search, done only for an end that has no open start.
    fn is_volta_continuation(&self, end: &RepeatEnd) -> bool {
        let voltas = self.form().voltas();
        let Some(i) = voltas.iter().position(|v| v.last == end.bar) else {
            return false;
        };
        let not_first = voltas.get(i).is_some_and(|v| !v.passes.contains(1));
        let not_last = voltas
            .get(i.saturating_add(1))
            .is_some_and(|next| !next.passes.contains(1));
        not_first && not_last
    }

    /// The number shown above a bar, or `None` if it is uncounted (ADR-0018 §5, ADR-0025 §3).
    ///
    /// # How
    /// The number of `core-time` (counting only the counted bars, with its restarts) is the
    /// `Continue` style. The two "same as first volta" styles change it on top: in every volta
    /// group, the bars of a later volta take the numbers of the first volta's counted bars (and
    /// continue past them if the later volta is longer: 8a | 8b 9b), and every bar after the group
    /// is moved back by what the group adds beyond its longest volta. A numbering restart wins
    /// over the style: groups before the last restart at or before the bar are absorbed by it,
    /// and so is the group containing that restart (owner's decision of 2026-11-03).
    ///
    /// # Complexity
    /// O(log n) for the plain number, plus O(v log u) with the "same as first" styles for the v
    /// voltas up to the bar and u uncounted bars (the counted bars of a volta are counted with two
    /// binary searches): tens of voltas in a real score. Never allocates.
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`]; [`DocumentError::MeterMap`] past `u32::MAX`.
    pub fn bar_label(&self, bar: BarId) -> Result<Option<BarLabel>, DocumentError> {
        let at = self.bar_index(bar)?;
        let Some(plain) = self.meter_map().display_number(at)? else {
            return Ok(None);
        };
        let style = self.form().numbering();
        if style == BarNumbering::Continue {
            return Ok(Some(BarLabel {
                number: plain,
                volta: None,
            }));
        }
        // The last restart at or before the bar: groups starting before it do not count.
        let restart = self
            .meter_map()
            .numbering_restarts()
            .iter()
            .rev()
            .find(|(k, _)| *k <= at)
            .map_or(0, |(k, _)| k.get());
        let bars = self.bars();
        let voltas = self.form().voltas();
        let mut shift: u32 = 0;
        let mut i = 0;
        while let Some(head) = voltas.get(i) {
            // The group starting at `i`: `head` holds pass 1 (the document is valid).
            let len = voltas
                .get(i.saturating_add(1)..)
                .map_or(0, |rest| {
                    rest.iter().take_while(|v| !v.passes.contains(1)).count()
                })
                .saturating_add(1);
            let group = voltas.get(i..i.saturating_add(len)).unwrap_or(&[]);
            i = i.saturating_add(len);
            let from = bars.key(head.first);
            let to = group.last().map_or(from, |v| bars.key(v.last));
            if from < restart || from > at.get() {
                // Absorbed by a restart, or after the bar: changes nothing here.
                if from > at.get() {
                    break;
                }
                continue;
            }
            let counted = |v: &Volta| self.counted_between(bars.key(v.first), bars.key(v.last));
            if at.get() > to {
                // After the group: moved back by what it adds beyond its longest volta.
                let mut total: u32 = 0;
                let mut longest: u32 = 0;
                for v in group {
                    let c = counted(v)?;
                    total = total.saturating_add(c);
                    longest = longest.max(c);
                }
                shift = shift.saturating_add(total.saturating_sub(longest));
                continue;
            }
            // Inside the group: find the bar's volta and its place among the counted bars.
            let Some((place, volta)) = group
                .iter()
                .enumerate()
                .find(|(_, v)| bars.key(v.first) <= at.get() && at.get() <= bars.key(v.last))
            else {
                break;
            };
            // The number the voltas start from: that of the first counted bar from the first
            // volta on (normally the first volta's own first bar; if all its bars are uncounted,
            // the first counted bar of a later volta). There is one: `bar` itself is counted.
            let first_number = self
                .first_counted_number(from, to)?
                .unwrap_or(plain)
                .saturating_sub(shift);
            let before = self.counted_between_exclusive(bars.key(volta.first), at.get())?;
            let letter = (style == BarNumbering::SameAsFirstLettered)
                .then(|| u8::try_from(place.saturating_add(1)).ok())
                .flatten();
            return Ok(Some(BarLabel {
                number: first_number.saturating_add(before),
                volta: letter,
            }));
        }
        Ok(Some(BarLabel {
            number: plain.saturating_sub(shift),
            volta: None,
        }))
    }

    /// How many counted bars lie in `from..=to` (bar indices): the bars of the range minus the
    /// uncounted ones, found with two binary searches in the meter map's sorted list of uncounted
    /// bars. O(log u) for u uncounted bars, whatever the length of the range.
    fn counted_between(&self, from: u32, to: u32) -> Result<u32, DocumentError> {
        if to < from {
            return Ok(0);
        }
        let uncounted = self.meter_map().uncounted_bars();
        let low = uncounted.partition_point(|k| k.get() < from);
        let high = uncounted.partition_point(|k| k.get() <= to);
        let skipped =
            u32::try_from(high.saturating_sub(low)).map_err(|_| DocumentError::TooManyBars)?;
        Ok((to - from).saturating_add(1).saturating_sub(skipped))
    }

    /// How many counted bars lie in `from..to` (bar indices, `to` excluded). O(log u).
    fn counted_between_exclusive(&self, from: u32, to: u32) -> Result<u32, DocumentError> {
        match to.checked_sub(1) {
            Some(last) => self.counted_between(from, last),
            None => Ok(0),
        }
    }

    /// The plain number of the first counted bar in `from..=to`, if any.
    ///
    /// The first counted bar is found by walking the uncounted bars from `from` on while they are
    /// consecutive: O(log u + r) for a run of r uncounted bars (r is 0 or 1 in practice), then
    /// one O(log n) plain number.
    fn first_counted_number(&self, from: u32, to: u32) -> Result<Option<u32>, DocumentError> {
        let uncounted = self.meter_map().uncounted_bars();
        let mut candidate = from;
        for k in uncounted
            .get(uncounted.partition_point(|k| k.get() < from)..)
            .unwrap_or(&[])
        {
            if k.get() != candidate {
                break;
            }
            candidate = candidate.saturating_add(1);
        }
        if candidate > to {
            return Ok(None);
        }
        Ok(self.meter_map().display_number(BarIndex::new(candidate))?)
    }
}

/// A repeated section of the form, as bar indices (ADR-0025 §2–§4).
///
/// # Two ends
/// `last_end` is the bar of its last end repeat (the further ends of its voltas included): the
/// part that is *repeated*, which the rules on pass rules and jumps (rules 10 and 11) work with,
/// since the last volta is played once, after the repeats. `last` also covers that last volta:
/// the part the player walks through as one section, playing a volta or skipping it by pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepeatSection {
    /// The index of its first bar (0 when the first end repeat has no start repeat).
    pub start: u32,
    /// The indices of the bars at whose end it repeats, ascending (more than one with voltas:
    /// "1. :‖ 2. :‖").
    pub ends: Vec<u32>,
    /// The index of its last end repeat: the last of `ends`.
    pub last_end: u32,
    /// The index of its last bar: `last_end`, or the last bar of its last volta.
    pub last: u32,
    /// How many times it is played: the highest pass of its voltas, or its end repeat's `times`.
    pub plays: u8,
}

/// A volta, as bar indices, with the section it belongs to (ADR-0025 §3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionVolta {
    /// The index of its first bar.
    pub first: u32,
    /// The index of its last bar.
    pub last: u32,
    /// The passes it is played on.
    pub passes: PassSet,
    /// Whether it is the last volta of its group (the one played after the repeats).
    pub is_last: bool,
    /// The index of its section in [`FormSections::sections`].
    pub section: usize,
}

/// The repeated sections of a document and the voltas attached to them: the structure of the
/// form, read once. Both the validation (rules 10 and 11) and the player (`orchestra-playback`)
/// use it, so they always agree on what a section is (review point A2: each used to compute it
/// with its own algorithm).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FormSections {
    /// The sections in score order; they do not overlap in a valid document.
    pub sections: Vec<RepeatSection>,
    /// The voltas that belong to a section, in score order. A volta of a group no section claims
    /// (an invalid document) is left out.
    pub voltas: Vec<SectionVolta>,
}

impl FormSections {
    /// The section whose repeated part (`start ..= last_end`) contains the bar with index `bar`.
    /// O(log s): the sections are sorted and disjoint.
    #[must_use]
    pub fn repeated_at(&self, bar: u32) -> Option<&RepeatSection> {
        let after = self.sections.partition_point(|s| s.start <= bar);
        let candidate = self.sections.get(after.checked_sub(1)?)?;
        (bar <= candidate.last_end).then_some(candidate)
    }
}

impl Document {
    /// The structure of the form: its repeated sections and their voltas (see [`FormSections`]).
    ///
    /// # How
    /// 1. **Sections.** Start and end repeats are merged in score order, as rule 8 pairs them: a
    ///    start opens a section, an end closes the open one; an end with none open is the first end
    ///    of the score (a section from bar 0) or a further end of the voltas of the section just
    ///    closed, which it extends.
    /// 2. **Voltas.** They are grouped in score order, a group starting at the volta that holds the
    ///    first pass. A group belongs to the section one of whose ends closes one of its voltas; it
    ///    makes the section's plays its highest pass, and the section's last bar its own last bar.
    ///
    /// The results agree with rules 8 and 9 on every valid document; on an invalid one they are
    /// whatever the merge gives, and only `validate` decides.
    ///
    /// # Errors
    /// [`DocumentError::UnknownBar`] for a form marking on a bar the document does not have (an
    /// inconsistent document).
    ///
    /// # Complexity
    /// O(f log f + v·s) for f repeat barlines, v voltas and s sections: tens. Allocates the two
    /// lists (and a few scratch lists).
    pub fn form_sections(&self) -> Result<FormSections, DocumentError> {
        let bars = self.bars();
        let index = |bar: BarId| -> Result<u32, DocumentError> {
            Ok(bars
                .index_of(bar)
                .ok_or(DocumentError::UnknownBar(bar))?
                .get())
        };
        let form = self.form();

        // 1. Sections: (bar, 0 for a start or 1 for an end, times of an end), in score order; at
        // the same bar the start comes first (it opens the section the end closes).
        let mut marks: Vec<(u32, u8, u8)> = Vec::new();
        for r in form.repeat_starts() {
            marks.push((index(r.bar)?, 0, 0));
        }
        for r in form.repeat_ends() {
            marks.push((index(r.bar)?, 1, r.times));
        }
        marks.sort_unstable();
        let mut sections: Vec<RepeatSection> = Vec::new();
        let mut open: Option<u32> = None;
        for (bar, kind, times) in marks {
            if kind == 0 {
                open = Some(bar);
                continue;
            }
            let fresh = |start: u32| RepeatSection {
                start,
                ends: vec![bar],
                last_end: bar,
                last: bar,
                plays: times,
            };
            match open.take() {
                Some(start) => sections.push(fresh(start)),
                None => match sections.last_mut() {
                    // A further end of the voltas of the section just closed.
                    Some(last) => {
                        last.ends.push(bar);
                        last.last_end = last.last_end.max(bar);
                        last.last = last.last.max(bar);
                    }
                    // The first end of the score, with no start: the section starts at bar 0.
                    None => sections.push(fresh(0)),
                },
            }
        }

        // 2. Voltas, grouped (a group starts at the volta holding pass 1).
        let mut voltas: Vec<(u32, u32, PassSet)> = Vec::with_capacity(form.voltas().len());
        for v in form.voltas() {
            voltas.push((index(v.first)?, index(v.last)?, v.passes));
        }
        voltas.sort_unstable_by_key(|v| v.0);
        let mut out: Vec<SectionVolta> = Vec::with_capacity(voltas.len());
        let mut rest = voltas.as_slice();
        while !rest.is_empty() {
            // The group: the first volta left and the following ones, up to the next volta that
            // holds pass 1 (which starts the next group).
            let size = rest
                .iter()
                .skip(1)
                .position(|v| v.2.contains(1))
                .map_or(rest.len(), |p| p.saturating_add(1));
            let (group, next) = rest.split_at(size.min(rest.len()));
            rest = next;
            let Some(section) = sections
                .iter()
                .position(|s| group.iter().any(|v| s.ends.contains(&v.1)))
            else {
                continue;
            };
            if let Some(s) = sections.get_mut(section) {
                s.plays = group.iter().filter_map(|v| v.2.last()).max().unwrap_or(2);
                s.last = s.last.max(group.last().map_or(0, |v| v.1));
            }
            let count = group.len();
            for (k, &(first, last, passes)) in group.iter().enumerate() {
                out.push(SectionVolta {
                    first,
                    last,
                    passes,
                    is_last: k.saturating_add(1) == count,
                    section,
                });
            }
        }
        Ok(FormSections {
            sections,
            voltas: out,
        })
    }
}

/// A violation as a document error. O(1).
pub(crate) const fn invalid(violation: Violation) -> DocumentError {
    DocumentError::Invalid(violation)
}
