// SPDX-License-Identifier: GPL-3.0-or-later

//! Dynamics and hairpins: how loud the music is written to be (ADR-0024 §8, SCALETTA F1.09c).
//!
//! # Position-anchored, kept per instrument
//! A dynamic sits at a (bar, offset) point, not on a note (ADR-0022 §6): a *p* under a rest, or
//! halfway through a held chord, is ordinary writing. A hairpin runs from one point to another.
//! Both live in lists **per instrument**, sorted by their (start) point in score order, because
//! they usually apply to the whole instrument (a piano's dynamic between its two staves is one
//! sign for both hands).
//!
//! # Scope and staff are two different things
//! [`DynamicScope`] says **what a dynamic applies to**: the whole instrument (the usual case) or
//! one voice (choral divisi, two players sharing a staff). [`DynamicPlacement::staff`] only says
//! **where it is drawn**. Keeping them apart means that moving a piano's dynamic from below the
//! upper staff to above the lower one changes nothing in playback.
//!
//! # From written dynamics to sound
//! Nothing here is a velocity. The playback rules of F1.13 turn the written dynamics and hairpins
//! into velocities and controller curves; the per-note velocity offsets of ADR-0024 §10 are deltas
//! on top of that result (ADR-0004), so changing a dynamic still changes every note under it, and
//! no note stores a copy of the dynamic it is played at.
//!
//! # The names are not translated
//! *pp*, *mf*, *sfz* are Italian by convention in every language (ADR-0024 §8): the interface
//! draws the symbol, it does not look up a word. Free text ([`DynamicMark::Other`], "subito p",
//! "più f") is the user's own content, shown as written, like an instrument name.

use crate::document::DocumentError;
use crate::id::{DynamicId, HairpinId, StaffId, VoiceId};
use crate::marks::ScorePoint;

// ------------------------------------------------------------------------------------------------
// Levels and marks
// ------------------------------------------------------------------------------------------------

/// A level of the usual scale, from *pppppp* to *ffffff* (ADR-0024 §8).
///
/// Ordered from soft to loud, so that `Ord` answers "is this louder?" (a crescendo should go up,
/// a playback rule can clamp to a range). Six letters is where published music stops (Tchaikovsky
/// writes *pppppp* in the Sixth Symphony); nothing is gained by going further.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DynamicLevel {
    /// *pppppp*.
    Pppppp,
    /// *ppppp*.
    Ppppp,
    /// *pppp*.
    Pppp,
    /// *ppp*.
    Ppp,
    /// *pp*.
    Pp,
    /// *p*.
    P,
    /// *mp*.
    Mp,
    /// *mf*.
    Mf,
    /// *f*.
    F,
    /// *ff*.
    Ff,
    /// *fff*.
    Fff,
    /// *ffff*.
    Ffff,
    /// *fffff*.
    Fffff,
    /// *ffffff*.
    Ffffff,
}

/// What a dynamic sign says (ADR-0024 §8).
///
/// The standard signs are variants, so that playback and the file format know them without
/// parsing text; anything else is [`Other`](Self::Other), free text with an optional level that
/// tells playback how to read it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DynamicMark {
    /// A level of the scale: *pp*, *mf*, *ff*…
    Level(DynamicLevel),
    /// *sf* (sforzando): one accented attack.
    Sf,
    /// *sfz* (sforzato).
    Sfz,
    /// *sffz*: a stronger sforzato.
    Sffz,
    /// *fz* (forzando).
    Fz,
    /// *rf* (rinforzando): a sudden, short increase.
    Rf,
    /// *rfz* (rinforzando).
    Rfz,
    /// *fp* (forte-piano): loud attack, then soft at once.
    Fp,
    /// *sfp*: sforzando, then soft.
    Sfp,
    /// *sfpp*: sforzando, then very soft.
    Sfpp,
    /// *pf* (piano-forte): soft attack, then loud.
    Pf,
    /// *n* (niente): nothing, silence.
    Niente,
    /// Anything else, as written ("subito p", "più f", "ppp possibile").
    Other(OtherDynamic),
}

/// A dynamic written as free text, with the level playback should use, if any (ADR-0024 §8).
///
/// The text is never empty: an empty text would draw nothing and could not be selected, so it is
/// refused when the value is made ([`OtherDynamic::new`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OtherDynamic {
    /// The text as the user wrote it. `Box<str>` rather than `String`: it never grows after it is
    /// made, so the spare capacity of a `String` would be wasted.
    text: Box<str>,
    /// The level playback treats it as ("più f" after an *f* might be *ff*); `None` leaves the
    /// level in force unchanged.
    level: Option<DynamicLevel>,
}

impl OtherDynamic {
    /// A free-text dynamic. O(length of `text`); allocates its copy of the text.
    ///
    /// # Errors
    /// [`DocumentError::EmptyText`] if `text` is empty or only white space.
    pub fn new(text: &str, level: Option<DynamicLevel>) -> Result<Self, DocumentError> {
        if text.trim().is_empty() {
            return Err(DocumentError::EmptyText);
        }
        Ok(Self {
            text: text.into(),
            level,
        })
    }

    /// The text as written. O(1).
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The level playback treats it as, if any. O(1).
    #[must_use]
    pub const fn level(&self) -> Option<DynamicLevel> {
        self.level
    }
}

// ------------------------------------------------------------------------------------------------
// What a dynamic applies to, and where it is drawn
// ------------------------------------------------------------------------------------------------

/// What a dynamic or a hairpin applies to (ADR-0024 §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DynamicScope {
    /// Every voice of the instrument: the usual case.
    Instrument,
    /// One voice of the instrument only (divisi, two players on one staff).
    Voice(VoiceId),
}

/// The scope of a dynamic or hairpin together with the staff it is drawn on (ADR-0024 §8).
///
/// One value for the two, because every dynamic and every hairpin needs both, and the builder
/// checks them together: the staff and the voice of the scope must belong to the instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DynamicPlacement {
    /// What it applies to.
    pub scope: DynamicScope,
    /// The staff it is drawn on (below it, by default, in F2B). Changes nothing in playback.
    pub staff: StaffId,
}

impl DynamicPlacement {
    /// A placement for the whole instrument, drawn on `staff`. O(1).
    #[must_use]
    pub const fn instrument(staff: StaffId) -> Self {
        Self {
            scope: DynamicScope::Instrument,
            staff,
        }
    }

    /// A placement for one voice only, drawn on `staff`. O(1).
    #[must_use]
    pub const fn voice(voice: VoiceId, staff: StaffId) -> Self {
        Self {
            scope: DynamicScope::Voice(voice),
            staff,
        }
    }
}

// ------------------------------------------------------------------------------------------------
// Dynamics
// ------------------------------------------------------------------------------------------------

/// A dynamic sign at a point of the score (ADR-0024 §8).
///
/// At most one per scope at a point: two signs saying different things to the same players at
/// the same moment are a contradiction, not two facts. An instrument dynamic and a voice dynamic
/// at the same point are allowed; which one wins for that voice is a playback rule (F1.13).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Dynamic {
    /// The dynamic's identity.
    id: DynamicId,
    /// Where it is. The offset may reach past the end of an overfull bar (ADR-0022 §4).
    at: ScorePoint,
    /// What it applies to and where it is drawn.
    placement: DynamicPlacement,
    /// What it says.
    mark: DynamicMark,
}

impl Dynamic {
    /// A dynamic with its ID. O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// ADR-0029 L1 of 2026-11-06): the value only *describes* an object. It enters a document only
    /// through [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as
    /// the builder checks what it creates, so no invalid document can come from it.
    pub const fn new(
        id: DynamicId,
        at: ScorePoint,
        placement: DynamicPlacement,
        mark: DynamicMark,
    ) -> Self {
        Self {
            id,
            at,
            placement,
            mark,
        }
    }

    /// This dynamic at another point, the rest unchanged. O(1) plus the copy of its mark.
    ///
    /// For the commands that move or clip the object while keeping everything else (inserting,
    /// deleting and rewriting bars): one method instead of calling `new` with every
    /// other field copied by hand, so a field added later cannot be forgotten by one of them.
    #[must_use]
    pub(crate) fn with_point(&self, at: ScorePoint) -> Self {
        Self { at, ..self.clone() }
    }

    /// The dynamic's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> DynamicId {
        self.id
    }

    /// Where it is. O(1).
    #[must_use]
    pub const fn at(&self) -> ScorePoint {
        self.at
    }

    /// What it applies to and where it is drawn. O(1).
    #[must_use]
    pub const fn placement(&self) -> DynamicPlacement {
        self.placement
    }

    /// What it says. O(1).
    #[must_use]
    pub const fn mark(&self) -> &DynamicMark {
        &self.mark
    }
}

// ------------------------------------------------------------------------------------------------
// Hairpins
// ------------------------------------------------------------------------------------------------

/// Which way a hairpin goes (ADR-0024 §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HairpinKind {
    /// Louder: an opening wedge, or "cresc.".
    Crescendo,
    /// Softer: a closing wedge, or "dim." / "decresc.".
    Diminuendo,
}

/// How a hairpin is drawn (ADR-0024 §8). The words of the text style come from the interface's
/// catalogues in F2B, not from the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HairpinStyle {
    /// A wedge: two lines that open or close.
    #[default]
    Wedge,
    /// A word ("cresc.", "dim.") followed by a dashed line.
    Text,
}

/// What a hairpin looks like: its direction, its style and whether it goes to or from nothing.
///
/// One value for the three, because they are chosen together (one palette item in F2B) and so
/// that the builder takes one argument instead of three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HairpinShape {
    /// Louder or softer.
    pub kind: HairpinKind,
    /// Wedge or text.
    pub style: HairpinStyle,
    /// *Al niente* / *dal niente*: a small circle at the closed end of the wedge, meaning that the
    /// sound fades to (or grows from) nothing.
    pub niente: bool,
}

impl HairpinShape {
    /// A crescendo wedge, not from nothing.
    pub const CRESCENDO: Self = Self {
        kind: HairpinKind::Crescendo,
        style: HairpinStyle::Wedge,
        niente: false,
    };

    /// A diminuendo wedge, not to nothing.
    pub const DIMINUENDO: Self = Self {
        kind: HairpinKind::Diminuendo,
        style: HairpinStyle::Wedge,
        niente: false,
    };
}

/// A hairpin from one point of the score to a later one (ADR-0024 §8).
///
/// The end is **exclusive**: a hairpin up to (bar 5, 0) stops at the barline, so "crescendo up to
/// here, diminuendo from here" is two hairpins that touch without overlapping. The model checks
/// ([`crate::Document::validate`]) that the end comes after the start and that two hairpins of the
/// same scope never overlap (a crescendo and a diminuendo at once, for the same players, cannot be
/// performed). A dynamic inside a hairpin is fine ("cresc. … mf … f").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hairpin {
    /// The hairpin's identity.
    id: HairpinId,
    /// Where it starts (included).
    start: ScorePoint,
    /// Where it ends (excluded).
    end: ScorePoint,
    /// What it applies to and where it is drawn.
    placement: DynamicPlacement,
    /// Its direction and look.
    shape: HairpinShape,
}

impl Hairpin {
    /// A hairpin with its ID. O(1).
    ///
    /// Public so that a saved document can be described for loading (ADR-0029 S4, owner's choice
    /// ADR-0029 L1 of 2026-11-06): the value only *describes* an object. It enters a document only
    /// through [`Document::from_parts`](crate::Document::from_parts), which checks it exactly as
    /// the builder checks what it creates, so no invalid document can come from it.
    pub const fn new(
        id: HairpinId,
        start: ScorePoint,
        end: ScorePoint,
        placement: DynamicPlacement,
        shape: HairpinShape,
    ) -> Self {
        Self {
            id,
            start,
            end,
            placement,
            shape,
        }
    }

    /// This hairpin with other start and end points, the rest unchanged. O(1).
    ///
    /// For the commands that move or clip the object while keeping everything else (inserting,
    /// deleting and rewriting bars): one method instead of calling `new` with every
    /// other field copied by hand, so a field added later cannot be forgotten by one of them.
    #[must_use]
    pub(crate) const fn with_points(&self, start: ScorePoint, end: ScorePoint) -> Self {
        Self {
            start,
            end,
            ..*self
        }
    }

    /// The hairpin's identity. O(1).
    #[must_use]
    pub const fn id(&self) -> HairpinId {
        self.id
    }

    /// Where it starts (included). O(1).
    #[must_use]
    pub const fn start(&self) -> ScorePoint {
        self.start
    }

    /// Where it ends (excluded). O(1).
    #[must_use]
    pub const fn end(&self) -> ScorePoint {
        self.end
    }

    /// What it applies to and where it is drawn. O(1).
    #[must_use]
    pub const fn placement(&self) -> DynamicPlacement {
        self.placement
    }

    /// Its direction and look. O(1).
    #[must_use]
    pub const fn shape(&self) -> HairpinShape {
        self.shape
    }
}
