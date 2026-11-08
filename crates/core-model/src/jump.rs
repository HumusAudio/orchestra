// SPDX-License-Identifier: GPL-3.0-or-later

//! Markers and jumps: segno, coda, Fine, "To Coda", D.C. and D.S. in all their variants (ADR-0025
//! §4; SCALETTA F1.10b, second part).
//!
//! # A jump is "go to / play until / continue at", with IDs
//! Every notation program models a jump with these three parts (`MuseScore`'s "Jump to", "Play
//! until", "Continue at"). Here they point at markers by **ID**, never by a label: a label goes
//! stale when its marker is renamed or deleted, an ID cannot point at the wrong segno in a D.S.S.
//! The printed words ("D.S. al Coda") are derived from the parts; a `text` replaces them for the
//! jumps written as free text (owner's scope: "D.C. senza ritornelli" is a jump to the start with
//! repeats played and its own words).
//!
//! # Where they sit
//! On barlines, like the rest of the form (ADR-0025 F1): a segno and a coda mark the **start** of
//! their bar, a Fine, a "To Coda" and every jump the **end** of theirs. At most one start marker,
//! one end marker and one jump per bar (owner's decision of 2026-11-03).
//!
//! # What `validate` checks (rule 10, owner's decisions of 2026-11-03)
//! The targets have the right kind (a segno for `to`, a Fine or a "To Coda" for `until`, a coda
//! for `then`, `then` present exactly with a "To Coda"); the `until` marker lies between the
//! target and the jump in score order, or the jump could never stop there; `OnPass(n)` only for a
//! jump inside a repeated section, with n at most its plays. The coda may be anywhere: each jump
//! is taken once, so the path is finite whatever the order.

use crate::document::{Document, DocumentError};
use crate::form::invalid;
use crate::id::{BarId, JumpId, MarkerId};
use crate::validate::Violation;

// ------------------------------------------------------------------------------------------------
// Markers
// ------------------------------------------------------------------------------------------------

/// What a marker is (ADR-0025 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerKind {
    /// 𝄋, at the start of its bar: where a D.S. goes back to.
    Segno,
    /// 𝄌, at the start of its bar: where the music continues after a "To Coda".
    Coda,
    /// "Fine", at the end of its bar: where the music ends after a jump that names it.
    Fine,
    /// "To Coda" (𝄌), at the end of its bar: where a jump that names it leaves for the coda.
    ToCoda,
}

impl MarkerKind {
    /// Whether the marker belongs to the start of its bar (segno, coda) rather than its end. O(1).
    #[must_use]
    pub const fn at_start(self) -> bool {
        matches!(self, Self::Segno | Self::Coda)
    }
}

/// A marker on a barline (ADR-0025 §4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Marker {
    /// The marker's identity.
    id: MarkerId,
    /// Its bar (the start or the end of it, by its kind).
    bar: BarId,
    /// What it is.
    kind: MarkerKind,
    /// Printed instead of the usual sign or word, the user's own text; never empty.
    text: Option<Box<str>>,
}

impl Marker {
    /// A marker with its ID. O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// L1 of 2026-11-06): the value only *describes* an object. It enters a document only through
    /// [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as the builder
    /// checks what it creates, so no invalid document can come from it.
    pub const fn new(id: MarkerId, bar: BarId, kind: MarkerKind, text: Option<Box<str>>) -> Self {
        Self {
            id,
            bar,
            kind,
            text,
        }
    }

    /// This marker at another bar, the rest unchanged. O(text length).
    ///
    /// For the commands that move or clip the object while keeping everything else (inserting,
    /// deleting and rewriting bars): they used to call `new` with every field copied by hand
    /// (review point B11).
    #[must_use]
    pub(crate) fn with_bar(&self, bar: BarId) -> Self {
        Self {
            bar,
            ..self.clone()
        }
    }

    /// The marker's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> MarkerId {
        self.id
    }

    /// Its bar. O(1).
    #[must_use]
    pub const fn bar(&self) -> BarId {
        self.bar
    }

    /// What it is. O(1).
    #[must_use]
    pub const fn kind(&self) -> MarkerKind {
        self.kind
    }

    /// The text printed instead of the usual sign or word, if any. O(1).
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

// ------------------------------------------------------------------------------------------------
// Jumps
// ------------------------------------------------------------------------------------------------

/// Where a jump goes (ADR-0025 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JumpTarget {
    /// The start of the score: D.C.
    Start,
    /// A segno: D.S.
    Marker(MarkerId),
}

/// Whether the repeats are played again after the jump (ADR-0025 F4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RepeatsAfterJump {
    /// Every repeated section is played once, and a section with voltas plays only its last
    /// volta: the classical convention, and the default.
    #[default]
    Skip,
    /// The repeats are played again in full.
    Play,
}

/// When a jump inside a repeated section is taken (ADR-0025 F5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum JumpFires {
    /// On the section's last pass (a D.S. at the end of a repeated chorus); for a jump outside any
    /// repeated section, simply the first time it is reached. The default.
    #[default]
    AfterRepeats,
    /// On pass n of the section containing the jump (1-based).
    OnPass(u8),
}

/// What a jump does: go to, play until, continue at, with its two choices (ADR-0025 §4). Plain
/// data that the builder checks: one value so that a jump is added with one argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct JumpSpec {
    /// Where it goes.
    pub to: JumpTarget,
    /// Where it stops (a Fine) or leaves for the coda (a "To Coda"); `None` plays on to the end.
    pub until: Option<MarkerId>,
    /// The coda to continue at, exactly when `until` is a "To Coda".
    pub then: Option<MarkerId>,
    /// Whether the repeats are played again after the jump.
    pub repeats: RepeatsAfterJump,
    /// When the jump is taken, if it is inside a repeated section.
    pub fires: JumpFires,
}

impl JumpSpec {
    /// D.C.: back to the start, then to the end. O(1).
    #[must_use]
    pub const fn da_capo() -> Self {
        Self {
            to: JumpTarget::Start,
            until: None,
            then: None,
            repeats: RepeatsAfterJump::Skip,
            fires: JumpFires::AfterRepeats,
        }
    }

    /// D.S.: back to `segno`, then to the end. O(1).
    #[must_use]
    pub const fn dal_segno(segno: MarkerId) -> Self {
        Self {
            to: JumpTarget::Marker(segno),
            ..Self::da_capo()
        }
    }

    /// The same jump, stopping at `fine` ("al Fine"). O(1).
    #[must_use]
    pub const fn al_fine(self, fine: MarkerId) -> Self {
        Self {
            until: Some(fine),
            then: None,
            ..self
        }
    }

    /// The same jump, leaving at `to_coda` for `coda` ("al Coda"). O(1).
    #[must_use]
    pub const fn al_coda(self, to_coda: MarkerId, coda: MarkerId) -> Self {
        Self {
            until: Some(to_coda),
            then: Some(coda),
            ..self
        }
    }
}

/// A jump at the end of a bar (ADR-0025 §4). Each jump is taken once, the first time it is
/// reached on the path (on the pass `fires` says), so the unrolled path is finite by construction.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Jump {
    /// The jump's identity.
    id: JumpId,
    /// The bar at whose end it is.
    bar: BarId,
    /// What it does.
    spec: JumpSpec,
    /// Printed instead of the derived words, the user's own text; never empty.
    text: Option<Box<str>>,
}

impl Jump {
    /// A jump with its ID. O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// L1 of 2026-11-06): the value only *describes* an object. It enters a document only through
    /// [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as the builder
    /// checks what it creates, so no invalid document can come from it.
    pub const fn new(id: JumpId, bar: BarId, spec: JumpSpec, text: Option<Box<str>>) -> Self {
        Self {
            id,
            bar,
            spec,
            text,
        }
    }

    /// This jump at another bar, the rest unchanged. O(text length).
    ///
    /// For the commands that move or clip the object while keeping everything else (inserting,
    /// deleting and rewriting bars): they used to call `new` with every field copied by hand
    /// (review point B11).
    #[must_use]
    pub(crate) fn with_bar(&self, bar: BarId) -> Self {
        Self {
            bar,
            ..self.clone()
        }
    }

    /// The jump's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> JumpId {
        self.id
    }

    /// The bar at whose end it is. O(1).
    #[must_use]
    pub const fn bar(&self) -> BarId {
        self.bar
    }

    /// What it does. O(1).
    #[must_use]
    pub const fn spec(&self) -> JumpSpec {
        self.spec
    }

    /// The text printed instead of the derived words, if any. O(1).
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }
}

// ------------------------------------------------------------------------------------------------
// Validation (rule 10)
// ------------------------------------------------------------------------------------------------

impl Document {
    /// Refuses a jump whose target, "until" or "then" marker does not exist. O(m) per marker.
    ///
    /// Used by the builder and by the mark commands; an edit does not check it, because a later
    /// edit of the same transaction could add or remove the markers (see `crate::edit`). It was
    /// written out in both places (review point B7).
    ///
    /// # Errors
    /// [`DocumentError::UnknownMarker`] with the first marker missing.
    pub(crate) fn check_jump_markers(&self, spec: &JumpSpec) -> Result<(), DocumentError> {
        let target = match spec.to {
            JumpTarget::Start => None,
            JumpTarget::Marker(id) => Some(id),
        };
        for id in [target, spec.until, spec.then].into_iter().flatten() {
            if self.marker(id).is_none() {
                return Err(DocumentError::UnknownMarker(id));
            }
        }
        Ok(())
    }

    /// The marker with this ID. O(m): markers are few (a handful per score).
    pub fn marker(&self, id: MarkerId) -> Option<&Marker> {
        self.form().markers().iter().find(|m| m.id == id)
    }

    /// Checks rule 10 of [`Document::validate`] (see the module documentation). Runs after rules
    /// 8 and 9, which the section lookup of `OnPass` relies on.
    ///
    /// # Complexity
    /// O(j·(m + r + s + v)) for j jumps, m markers and the repeats and voltas the section lookup
    /// walks: a handful of each. Never allocates.
    ///
    /// # Errors
    /// [`DocumentError::Invalid`] with the first violation found, in the order of the jumps.
    pub(crate) fn check_jumps(&self) -> Result<(), DocumentError> {
        let bars = self.bars();
        // The form's sections, read once and only if a jump fires on a given pass (the only check
        // that needs them): most documents allocate nothing here.
        let sections = if self
            .form()
            .jumps()
            .iter()
            .any(|j| matches!(j.spec.fires, JumpFires::OnPass(_)))
        {
            self.form_sections()?
        } else {
            crate::form::FormSections::default()
        };
        // A point of the score in score order: (bar index, 0 for the start, 1 for the end).
        let kind_of = |id: MarkerId| self.marker(id).map(|m| (m.kind, bars.key(m.bar)));
        for jump in self.form().jumps() {
            let bad = |v: fn(JumpId) -> Violation| Err(invalid(v(jump.id)));
            let spec = jump.spec;
            let target = match spec.to {
                JumpTarget::Start => (0, 0),
                JumpTarget::Marker(id) => match kind_of(id) {
                    Some((MarkerKind::Segno, at)) => (at, 0),
                    _ => return bad(Violation::JumpTargetKind),
                },
            };
            let here = (bars.key(jump.bar), 1_u8);
            match (spec.until.map(kind_of), spec.then.map(kind_of)) {
                (None, None) => {}
                (Some(Some((MarkerKind::Fine, at))), None)
                | (Some(Some((MarkerKind::ToCoda, at))), Some(Some((MarkerKind::Coda, _)))) => {
                    let until = (at, 1);
                    if until < target || until > here {
                        return bad(Violation::JumpUntilOutOfRange);
                    }
                }
                _ => return bad(Violation::JumpTargetKind),
            }
            if let JumpFires::OnPass(n) = spec.fires {
                let plays = sections.repeated_at(here.0).map_or(0, |s| s.plays);
                if n == 0 || n > plays {
                    return bad(Violation::JumpPassOutOfRange);
                }
            }
        }
        Ok(())
    }
}
