// SPDX-License-Identifier: GPL-3.0-or-later

//! Time signatures of the MIDI file: **the real length of each played bar** (ADR-0032 E5), so that
//! a sequencer's bar lines match ours, repeats, jumps, irregular and free bars included.
//!
//! # The rule
//! For each played bar, in path order:
//! - a **regular** bar (its length is its meter's nominal length) whose meter is writable (a
//!   power-of-two denominator, a numerator up to 255) gets the meter as written: 6/8 stays 6/8,
//!   it does not become 3/4; an additive meter (3+2+2/8) is written as its total (7/8);
//! - any other bar (an anacrusis, an irregular bar, a free bar of explicit length) gets a fraction
//!   of **its real length** with a power-of-two denominator: at least the meter's own
//!   denominator (when that is a power of two) or 4, more when the length needs it. A 5/8 bar in
//!   4/4 is 5/8, an anacrusis of a quarter in 3/4 is 1/4, a free bar of two whole notes in 4/4 is
//!   8/4;
//! - when neither can be written (a 4/3 meter, a free bar of 1/3), **no time signature**: a
//!   **marker** with the meter as text (`4/3`, `3+2/8`) and a warning (`MeterNotWritable`); the
//!   sequencer's bar lines drift from there on, the sound does not.
//!
//! An event is written only where it differs from the previous one (a time signature or a
//! marker), so a score in one meter has one time signature.

use orchestra_core_model::Document;
use orchestra_core_time::{Duration, Meter, Rational};
use orchestra_playback::Unrolled;

use super::MidiError;
use super::timing::{bar_length, ticks_of};

/// What the conductor track says at the start of a played bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BarSign {
    /// A time signature: numerator and the denominator's exponent of two.
    Signature(u8, u8),
    /// A marker with the meter's text, for a length the format cannot write.
    Marker(String),
}

/// The signs of the conductor track: (tick, sign, played bar) where the sign changes, and the
/// played bars whose length could not be written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Signs {
    /// The signs to write, by tick (with the lead-in), where they change.
    pub(crate) signs: Vec<(u64, BarSign)>,
    /// The played bars (index on the path) written as a marker.
    pub(crate) unwritable: Vec<u32>,
}

/// The time signature of a length, if the format can write it: `length = n / 2^e` with
/// `n ≤ 255`, the denominator at least `base` (a power of two). O(1).
fn signature_of(length: Duration, base: i64) -> Option<(u8, u8)> {
    let r = length.as_rational();
    if !r.is_positive() {
        return None;
    }
    // The reduced denominator must be a power of two; then widen it to the base if smaller.
    let den = r.denominator();
    if den <= 0 || den.count_ones() != 1 {
        return None;
    }
    let den = den.max(base);
    let num = r.checked_mul(Rational::from_integer(den).ok()?).ok()?;
    if !num.is_integer() {
        return None;
    }
    let numerator = u8::try_from(num.numerator()).ok()?;
    let exponent = u8::try_from(den.trailing_zeros()).ok()?;
    Some((numerator, exponent))
}

/// The meter as text, for the marker: `4/3`, or `3+2+2/8` for an additive meter. Numbers and
/// signs only: no language (CLAUDE.md §7). O(groups).
fn meter_text(meter: &Meter) -> String {
    let groups: Vec<String> = meter.groups().iter().map(u8::to_string).collect();
    format!("{}/{}", groups.join("+"), meter.denominator())
}

/// The signs of every played bar (see the module documentation). `lead` is the ticks of the
/// lead-in, whose own sign the caller writes.
///
/// # Errors
/// The meter map's, the path's and the arithmetic errors.
///
/// # Complexity
/// O(path × log m) for m meter changes.
pub(crate) fn signs(doc: &Document, path: &Unrolled, lead: i64) -> Result<Signs, MidiError> {
    let mut out = Signs::default();
    let meters = doc.meter_map();
    for (index, bar) in path.played().iter().enumerate() {
        let length = bar_length(path, index)?;
        let meter = meters
            .meter_at(bar.index)
            .map_err(orchestra_playback::UnrollError::from)?;
        let den = i64::from(meter.denominator());
        let nominal = meter.nominal_length()?;
        let sign = if length == nominal {
            // A regular bar: the meter as written, when writable.
            u8::try_from(meter.numerator())
                .ok()
                .filter(|_| den.count_ones() == 1)
                .and_then(|n| Some((n, u8::try_from(den.trailing_zeros()).ok()?)))
        } else {
            None
        }
        .or_else(|| {
            let base = if den.count_ones() == 1 { den } else { 4 };
            signature_of(length, base)
        })
        .map_or_else(
            || BarSign::Marker(meter_text(&meter)),
            |(n, e)| BarSign::Signature(n, e),
        );
        if matches!(sign, BarSign::Marker(_)) {
            out.unwritable
                .push(u32::try_from(index).unwrap_or(u32::MAX));
        }
        if out.signs.last().is_none_or(|(_, s)| *s != sign) {
            let tick = u64::try_from(ticks_of(bar.start)?.saturating_add(lead)).unwrap_or(0);
            out.signs.push((tick, sign));
        }
    }
    Ok(out)
}

/// The time signature of a lead-in of `quarters` quarter notes, if writable (`k/4`, k ≤ 255).
pub(crate) fn lead_in_sign(quarters: i64) -> Option<BarSign> {
    u8::try_from(quarters)
        .ok()
        .map(|n| BarSign::Signature(n, 2))
}

#[cfg(test)]
mod tests {
    //! The rule for a length, on hand-picked cases.

    use super::*;
    use orchestra_core_time::RationalError;

    /// `n/d` of a whole note.
    fn len(n: i64, d: i64) -> Result<Duration, RationalError> {
        Ok(Duration::from_rational(Rational::new(n, d)?))
    }

    /// Lengths with a power-of-two denominator are written; the others are not.
    #[test]
    fn lengths() -> Result<(), RationalError> {
        assert_eq!(signature_of(len(5, 8)?, 4), Some((5, 3)), "5/8");
        assert_eq!(signature_of(len(1, 4)?, 4), Some((1, 2)), "an anacrusis");
        assert_eq!(signature_of(len(2, 1)?, 4), Some((8, 2)), "a long free bar");
        assert_eq!(
            signature_of(len(3, 4)?, 8),
            Some((6, 3)),
            "widened to eighths"
        );
        assert_eq!(signature_of(len(1, 3)?, 4), None, "a third");
        assert_eq!(signature_of(len(256, 4)?, 4), None, "numerator too large");
        Ok(())
    }

    /// The marker's text.
    #[test]
    fn meter_texts() -> Result<(), orchestra_core_time::MeterError> {
        assert_eq!(meter_text(&Meter::new(&[4], 3)?), "4/3");
        assert_eq!(meter_text(&Meter::new(&[3, 2, 2], 8)?), "3+2+2/8");
        Ok(())
    }
}
