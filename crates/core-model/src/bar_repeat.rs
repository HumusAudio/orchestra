// SPDX-License-Identifier: GPL-3.0-or-later

//! Bar repeats (%) and pass rules ("tacet 1st time"): the two form objects that belong to one
//! staff or one instrument (ADR-0025 §6–§7; SCALETTA F1.10b, second part).
//!
//! # Bar repeats store no events
//! The bars under a bar-repeat sign hold **no events** on their staff (ADR-0025 F6): what is
//! played there is the source bars, derived by F1.12. Copying the events would be the duplication
//! ADR-0003 forbids: an edit of the source would not reach the copies. A bar repeat is per
//! **staff** (F7): a piano's left hand may repeat while the right hand writes new music.
//!
//! # Pass rules are ranges
//! "Tacet 1st time", "2nd time only": one instruction over a passage of one instrument or one
//! voice, with the passes of the repeated section containing it on which the passage is played
//! (F8). A range rather than a set on every event, because that is how it is written, selected and
//! edited.
//!
//! # What `validate` checks (rule 11, owner's decisions of 2026-11-03)
//! - Bar repeats: the source bars exist (at least `span` bars before the first repeated bar), the
//!   repeated bars hold no event drawn on the staff, two bar repeats of one staff do not overlap,
//!   and no form marking (repeat barline, volta, marker, jump) lies between the first source bar
//!   and the last repeated bar: otherwise "the previous bars" would depend on the path taken.
//! - Pass rules: the range lies inside one repeated section, its passes are within that section's
//!   plays, and two rules of one scope do not overlap (the end is excluded, as for hairpins).

use orchestra_core_time::{BarIndex, Duration};

use crate::document::{Document, DocumentError, Instrument};
use crate::dynamic::DynamicScope;
use crate::form::{PassSet, index, invalid};
use crate::id::{BarId, BarRepeatId, PassRuleId, StaffId};
use crate::marks::{ScorePoint, point_key};
use crate::validate::Violation;

// ------------------------------------------------------------------------------------------------
// Bar repeats
// ------------------------------------------------------------------------------------------------

/// How many bars a bar-repeat sign repeats (ADR-0025 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarRepeatSpan {
    /// The one-bar sign (%): each bar repeats the bar before it.
    One,
    /// The two-bar sign: two bars repeat the two before them.
    Two,
    /// The four-bar sign.
    Four,
}

impl BarRepeatSpan {
    /// The number of bars the sign repeats. O(1).
    #[must_use]
    pub const fn bars(self) -> u32 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Four => 4,
        }
    }
}

/// Consecutive bars of one staff that repeat the `span` bars before them (ADR-0025 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BarRepeat {
    /// The bar repeat's identity.
    id: BarRepeatId,
    /// The staff it is on.
    staff: StaffId,
    /// The first repeated bar.
    first: BarId,
    /// How many bars repeat, a positive multiple of the span (owner's decision of 2026-11-03).
    count: u32,
    /// The sign: how many bars each repetition repeats.
    span: BarRepeatSpan,
}

impl BarRepeat {
    /// A bar repeat with its ID. O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// L1 of 2026-11-06): the value only *describes* an object. It enters a document only through
    /// [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as the builder
    /// checks what it creates, so no invalid document can come from it.
    pub const fn new(
        id: BarRepeatId,
        staff: StaffId,
        first: BarId,
        count: u32,
        span: BarRepeatSpan,
    ) -> Self {
        Self {
            id,
            staff,
            first,
            count,
            span,
        }
    }

    /// The bar repeat's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> BarRepeatId {
        self.id
    }

    /// The staff it is on. O(1).
    #[must_use]
    pub const fn staff(&self) -> StaffId {
        self.staff
    }

    /// The first repeated bar. O(1).
    #[must_use]
    pub const fn first(&self) -> BarId {
        self.first
    }

    /// How many bars repeat. O(1).
    #[must_use]
    pub const fn count(&self) -> u32 {
        self.count
    }

    /// The sign. O(1).
    #[must_use]
    pub const fn span(&self) -> BarRepeatSpan {
        self.span
    }
}

// ------------------------------------------------------------------------------------------------
// Pass rules
// ------------------------------------------------------------------------------------------------

/// A passage of one instrument (or one voice) played only on some passes of the repeated section
/// containing it (ADR-0025 §7).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PassRule {
    /// The rule's identity.
    id: PassRuleId,
    /// Where the passage starts (included).
    start: ScorePoint,
    /// Where it ends (excluded).
    end: ScorePoint,
    /// What it applies to.
    scope: DynamicScope,
    /// The passes on which the passage is played ("tacet 1st time" = {2}); never empty.
    plays: PassSet,
    /// Printed instead of the derived words ("1st time tacet"), the user's own text; never empty.
    text: Option<Box<str>>,
}

impl PassRule {
    /// A pass rule with its ID. O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// L1 of 2026-11-06): the value only *describes* an object. It enters a document only through
    /// [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as the builder
    /// checks what it creates, so no invalid document can come from it.
    pub const fn new(
        id: PassRuleId,
        start: ScorePoint,
        end: ScorePoint,
        scope: DynamicScope,
        plays: PassSet,
        text: Option<Box<str>>,
    ) -> Self {
        Self {
            id,
            start,
            end,
            scope,
            plays,
            text,
        }
    }

    /// The rule's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> PassRuleId {
        self.id
    }

    /// Where the passage starts (included). O(1).
    #[must_use]
    pub const fn start(&self) -> ScorePoint {
        self.start
    }

    /// Where it ends (excluded). O(1).
    #[must_use]
    pub const fn end(&self) -> ScorePoint {
        self.end
    }

    /// What it applies to. O(1).
    #[must_use]
    pub const fn scope(&self) -> DynamicScope {
        self.scope
    }

    /// The passes on which the passage is played. O(1).
    #[must_use]
    pub const fn plays(&self) -> PassSet {
        self.plays
    }

    /// The text printed instead of the derived words, if any. O(1).
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

// ------------------------------------------------------------------------------------------------
// Validation (rule 11)
// ------------------------------------------------------------------------------------------------

impl Document {
    /// Checks rule 11 of [`Document::validate`] for one instrument (see the module
    /// documentation). Runs after rules 8 and 9, which the section lookup relies on.
    ///
    /// # Complexity
    /// O(b·(f + e)) for b bar repeats, the f form markings they are compared with and the e events
    /// of their bars on the instrument's voices, plus O(p·(r + s + v)) for p pass rules (the
    /// section lookup) and O(p·k) for the overlap check over k scopes. Allocates only a small
    /// list of scopes, and only if the instrument has pass rules.
    ///
    /// # Errors
    /// [`DocumentError::Invalid`] with the first violation found.
    pub(crate) fn check_bar_repeats_and_pass_rules(
        &self,
        instrument: &Instrument,
    ) -> Result<(), DocumentError> {
        let bars = self.bars();
        // Bar repeats, sorted by first bar: the end of the last one seen on each staff (at most
        // 16 staves, so a fixed array of (staff, end) slots and no allocation).
        let mut ends: [Option<(StaffId, u32)>; crate::document::MAX_STAVES_PER_INSTRUMENT] =
            [None; crate::document::MAX_STAVES_PER_INSTRUMENT];
        for repeat in instrument.bar_repeats() {
            let first = index(bars, repeat.first);
            let span = repeat.span.bars();
            let last = first.saturating_add(repeat.count).saturating_sub(1);
            let Some(source) = first.checked_sub(span) else {
                return Err(invalid(Violation::BarRepeatNoSource(repeat.id)));
            };
            // Overlap on the same staff.
            let slot = ends
                .iter()
                .position(|e| e.is_none_or(|(staff, _)| staff == repeat.staff));
            if let Some(i) = slot
                && let Some(entry) = ends.get_mut(i)
            {
                if entry.is_some_and(|(_, end)| first <= end) {
                    return Err(invalid(Violation::BarRepeatOverlap(repeat.id)));
                }
                *entry = Some((repeat.staff, last));
            }
            // No event drawn on the staff in the repeated bars.
            for voice in instrument.voices() {
                for k in first..=last {
                    if voice
                        .events_at(BarIndex::new(k))
                        .iter()
                        .any(|placed| placed.event().staff() == repeat.staff)
                    {
                        return Err(invalid(Violation::BarRepeatNotEmpty(repeat.id)));
                    }
                }
            }
            if self.form_marking_between(source, last) {
                return Err(invalid(Violation::BarRepeatAcrossForm(repeat.id)));
            }
        }

        // Pass rules, sorted by start: the end of the last one seen in each scope. A vector that
        // allocates only if the instrument has pass rules (an empty `Vec` does not allocate).
        let mut scope_ends: Vec<(DynamicScope, (u32, Duration))> = Vec::new();
        for rule in instrument.pass_rules() {
            let start = point_key(bars, rule.start);
            let end = point_key(bars, rule.end);
            let Some(section) = self.section_at(start.0) else {
                return Err(invalid(Violation::PassRuleOutsideRepeat(rule.id)));
            };
            // The end may be anywhere up to the barline closing the section: (last + 1, 0).
            if end > (section.last.saturating_add(1), Duration::ZERO) {
                return Err(invalid(Violation::PassRuleOutsideRepeat(rule.id)));
            }
            if rule.plays.last().is_none_or(|p| p > section.plays) {
                return Err(invalid(Violation::PassRulePasses(rule.id)));
            }
            match scope_ends
                .iter_mut()
                .find(|(scope, _)| *scope == rule.scope)
            {
                Some((_, previous)) if start < *previous => {
                    return Err(invalid(Violation::PassRuleOverlap(rule.id)));
                }
                Some((_, previous)) => *previous = end,
                None => scope_ends.push((rule.scope, end)),
            }
        }
        Ok(())
    }

    /// Whether a form marking lies strictly inside the run of bars `from..=to` (indices): a repeat
    /// barline, a volta edge, a marker or a jump on a barline between two bars of the run. The
    /// start of `from` and the end of `to` are its outer edges and do not count. O(f) for the f
    /// form markings.
    fn form_marking_between(&self, from: u32, to: u32) -> bool {
        let bars = self.bars();
        let form = self.form();
        // A marking at the start of bar k is inside when from < k ≤ to; at the end of bar k when
        // from ≤ k < to.
        let at_start = |bar: BarId| {
            let k = index(bars, bar);
            from < k && k <= to
        };
        let at_end = |bar: BarId| {
            let k = index(bars, bar);
            from <= k && k < to
        };
        form.repeat_starts().iter().any(|r| at_start(r.bar()))
            || form.repeat_ends().iter().any(|r| at_end(r.bar()))
            || form
                .voltas()
                .iter()
                .any(|v| at_start(v.first()) || at_end(v.last()))
            || form.markers().iter().any(|m| {
                if m.kind().at_start() {
                    at_start(m.bar())
                } else {
                    at_end(m.bar())
                }
            })
            || form.jumps().iter().any(|j| at_end(j.bar()))
    }
}
