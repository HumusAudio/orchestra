// SPDX-License-Identifier: GPL-3.0-or-later

//! The values of playback tuned by ear (ADR-0027 §14–§15), as a configuration the user can change
//! (ADR-0050 DG9, SCALETTA F2.12a4b2, owner's choices 1–3).
//!
//! # What is here
//! The eighteen values of the attack profiles: the **steps** of the articulation accents on the
//! 0–127 scale and their **times** to settle back (ADR-0027 H15), the times of the dynamic accents
//! *rf*, *sf*, *sffz* (H16), and the **holds** and **changes** of *fp*, *sfp*/*sfpp* and *pf*
//! (H16). Their defaults are the values in force after the second round of tuning by ear (§15).
//! The **levels** that the marks reach (*rf* one level up, *sf* two, *sffz* three, *fp* from *f*
//! to *p*…) are rules of ADR-0027 H3–H4, not tuning: they stay in the code (choice 1).
//!
//! # Why one array and a table of names
//! The settings file keeps the values that differ from the defaults by a **stable name** and an
//! integer (ADR-0050 S2, choice 3: steps on the 0–127 scale, times in whole milliseconds). A
//! [`TuningValue`] names each value, its range and its default once; [`Tuning`] is an array
//! indexed by it, so the file's reading and writing, the Preferences' controls and the engine's
//! reading all go through the same table, and adding a value is one line of it.
//!
//! # Units
//! Steps are levels of the 0–127 scale (a dynamic level is 13 of them, ADR-0027 §4); times are
//! whole **milliseconds**, turned into exact [`Seconds`] (`ms/1000`, reduced) by the engine, so
//! the defaults give exactly the rationals the constants gave before (0.25 s = 1/4).
//!
//! # Cost
//! Reading a value is an array index, O(1); the whole configuration is 18 integers (144 bytes),
//! copied into each voice's timeline so the loudness never searches for it.

use orchestra_core_time::{Rational, RationalError, Seconds};

/// One value of the [`Tuning`] (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TuningValue {
    /// The soft accent's step (ADR-0027 H15).
    SoftAccentStep,
    /// The soft accent's time to settle back, ms.
    SoftAccentSettle,
    /// The stress's step.
    StressStep,
    /// The stress's time to settle back, ms.
    StressSettle,
    /// The accent's step.
    AccentStep,
    /// The accent's time to settle back, ms.
    AccentSettle,
    /// The marcato's step.
    MarcatoStep,
    /// The marcato's time to settle back, ms.
    MarcatoSettle,
    /// The unstress's step, negative: on the whole note, with no profile.
    UnstressStep,
    /// *rf*, *rfz*: the time to settle back to the level in force, ms (H16).
    RfSettle,
    /// *sf*, *sfz*, *fz*: the time to settle back, ms.
    SfSettle,
    /// *sffz*: the time to settle back, ms.
    SffzSettle,
    /// *fp*: how long the *f* attack is held, ms.
    FpHold,
    /// *fp*: how long the sound takes to go down to *p*, ms.
    FpChange,
    /// *sfp*, *sfpp*: how long the *fff* attack is held, ms.
    SfpHold,
    /// *sfp*, *sfpp*: how long the sound takes to go down to *p* or *pp*, ms.
    SfpChange,
    /// *pf*: how long the *p* attack is held, ms.
    PfHold,
    /// *pf*: how long the sound takes to go up to *f*, ms.
    PfChange,
}

/// The kind of a [`TuningValue`]: what its integer means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TuningUnit {
    /// Levels of the 0–127 scale, added to the attack (negative for the unstress).
    Step,
    /// Whole milliseconds.
    Milliseconds,
}

/// The table of the values: (value, stable name in the settings file, unit, minimum, maximum,
/// default). The defaults are ADR-0027 §15 after round 2. The ranges (choice 3): steps 0–40 (the
/// unstress −40–0), times 0–2000 ms.
const TABLE: [(TuningValue, &str, TuningUnit, i64, i64, i64); 18] = {
    use TuningUnit::{Milliseconds as Ms, Step};
    use TuningValue as V;
    [
        (V::SoftAccentStep, "soft-accent.step", Step, 0, 40, 7),
        (
            V::SoftAccentSettle,
            "soft-accent.settle-ms",
            Ms,
            0,
            2000,
            200,
        ),
        (V::StressStep, "stress.step", Step, 0, 40, 7),
        (V::StressSettle, "stress.settle-ms", Ms, 0, 2000, 200),
        (V::AccentStep, "accent.step", Step, 0, 40, 22),
        (V::AccentSettle, "accent.settle-ms", Ms, 0, 2000, 250),
        (V::MarcatoStep, "marcato.step", Step, 0, 40, 30),
        (V::MarcatoSettle, "marcato.settle-ms", Ms, 0, 2000, 300),
        (V::UnstressStep, "unstress.step", Step, -40, 0, -7),
        (V::RfSettle, "rf.settle-ms", Ms, 0, 2000, 400),
        (V::SfSettle, "sf.settle-ms", Ms, 0, 2000, 600),
        (V::SffzSettle, "sffz.settle-ms", Ms, 0, 2000, 800),
        (V::FpHold, "fp.hold-ms", Ms, 0, 2000, 600),
        (V::FpChange, "fp.change-ms", Ms, 0, 2000, 100),
        (V::SfpHold, "sfp.hold-ms", Ms, 0, 2000, 200),
        (V::SfpChange, "sfp.change-ms", Ms, 0, 2000, 200),
        (V::PfHold, "pf.hold-ms", Ms, 0, 2000, 600),
        (V::PfChange, "pf.change-ms", Ms, 0, 2000, 100),
    ]
};

impl TuningValue {
    /// Every value, in the table's order (the order the Preferences show them in).
    pub const ALL: [Self; 18] = {
        let mut all = [Self::SoftAccentStep; 18];
        let mut i = 0;
        while i < 18 {
            all[i] = TABLE[i].0;
            i += 1;
        }
        all
    };

    /// Its index in the table: the enum's order is the table's (checked by a test).
    const fn index(self) -> usize {
        self as usize
    }

    /// The stable name in the settings file (`"accent.step"`, `"fp.hold-ms"`): never renamed.
    #[must_use]
    pub const fn name(self) -> &'static str {
        TABLE[self.index()].1
    }

    /// Its unit.
    #[must_use]
    pub const fn unit(self) -> TuningUnit {
        TABLE[self.index()].2
    }

    /// Its range, inclusive.
    #[must_use]
    pub const fn range(self) -> (i64, i64) {
        (TABLE[self.index()].3, TABLE[self.index()].4)
    }

    /// Its default: the value tuned by ear (ADR-0027 §15).
    #[must_use]
    pub const fn default_value(self) -> i64 {
        TABLE[self.index()].5
    }

    /// The value named `name` in the settings file, or `None`. O(18).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.name() == name)
    }
}

/// The playback's tuning (see the module documentation). `Default` is the tuning by ear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tuning {
    /// The values, indexed by [`TuningValue`], always inside their ranges.
    values: [i64; 18],
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            values: TuningValue::ALL.map(TuningValue::default_value),
        }
    }
}

impl Tuning {
    /// The value of `value`.
    #[must_use]
    pub const fn get(&self, value: TuningValue) -> i64 {
        self.values[value.index()]
    }

    /// Sets `value` to `to`, brought inside its range; answers `true` when `to` was outside it
    /// (the settings' reader reports it).
    pub const fn set(&mut self, value: TuningValue, to: i64) -> bool {
        let (min, max) = value.range();
        let clamped = if to < min {
            min
        } else if to > max {
            max
        } else {
            to
        };
        self.values[value.index()] = clamped;
        clamped != to
    }

    /// The values that differ from the defaults, by name, in the table's order: what the
    /// settings file keeps (S2).
    pub fn differences(&self) -> impl Iterator<Item = (&'static str, i64)> + '_ {
        TuningValue::ALL
            .into_iter()
            .filter(|value| self.get(*value) != value.default_value())
            .map(|value| (value.name(), self.get(value)))
    }

    /// The time of `value` (a value in milliseconds) as exact seconds, for the engine.
    ///
    /// # Errors
    /// Never on values inside their ranges (the arithmetic of `ms/1000`).
    pub(crate) fn seconds(&self, value: TuningValue) -> Result<Seconds, RationalError> {
        Ok(Seconds::from_rational(Rational::new(
            self.get(value),
            1000,
        )?))
    }
}

#[cfg(test)]
mod tests {
    //! The table's consistency, the ranges, the names.
    use super::*;

    /// The enum's order is the table's, every name is unique, every default inside its range.
    #[test]
    fn the_table_is_consistent() {
        for (i, value) in TuningValue::ALL.iter().enumerate() {
            assert_eq!(value.index(), i);
            let (min, max) = value.range();
            assert!((min..=max).contains(&value.default_value()), "{value:?}");
            assert_eq!(TuningValue::from_name(value.name()), Some(*value));
        }
        let mut names: Vec<_> = TuningValue::ALL.iter().map(|v| v.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 18);
        assert_eq!(TuningValue::from_name("nonsense"), None);
    }

    /// Out of range is clamped and reported; the defaults have no differences.
    #[test]
    fn values_are_clamped_and_differences_listed() {
        let mut tuning = Tuning::default();
        assert_eq!(tuning.differences().count(), 0);
        assert!(!tuning.set(TuningValue::AccentStep, 25));
        assert!(tuning.set(TuningValue::FpHold, 5000));
        assert_eq!(tuning.get(TuningValue::FpHold), 2000);
        assert!(tuning.set(TuningValue::UnstressStep, 3));
        assert_eq!(tuning.get(TuningValue::UnstressStep), 0);
        assert_eq!(
            tuning.differences().collect::<Vec<_>>(),
            vec![
                ("accent.step", 25),
                ("unstress.step", 0),
                ("fp.hold-ms", 2000)
            ]
        );
        assert_eq!(
            Tuning::default()
                .seconds(TuningValue::AccentSettle)
                .unwrap(),
            Seconds::from_rational(Rational::new(1, 4).unwrap())
        );
    }
}
