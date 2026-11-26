// SPDX-License-Identifier: GPL-3.0-or-later

//! Texts with names in them, formatted once and filled when shown (F2.12a5b, generalised in
//! F2.12c1).
//!
//! # Why
//! The demonstration's texts are formatted once, when the language is chosen, and the localizer
//! is not kept (it is not `Clone`, and the texts are plain data the tests compare). Some texts
//! hold values known only when they are shown: a project's name, a file's, a command's, a
//! shortcut's. Each such message is formatted once with a **placeholder** in place of each
//! variable: a character of Unicode's Private Use Area, U+E000 for the first variable, U+E001 for
//! the second, and so on, which no catalogue writes and names do not contain in practice. When
//! the text is shown each placeholder is replaced by its value.
//!
//! This is not concatenation (CLAUDE.md §7): Fluent still decides where each value goes in each
//! language, and in which order. It works because no message of ours **selects** on these
//! variables (a selector on a placeholder would always take its default branch): the tests check
//! that every language holds each placeholder exactly once.
//!
//! # Fluent's isolation marks
//! Fluent wraps every placeable in U+2068 and U+2069 (first strong isolate, pop directional
//! isolate). They are dropped, as for the other texts of the program: they would reach the
//! system's window title, the choosers and the screen readers.
//!
//! # Cost
//! Formatting: once per message and language. Filling: O(text length + values' length), one
//! allocation for the result.

use orchestra_i18n::{FluentArgs, FluentValue, I18nError, Localizer};

/// The isolation marks Fluent puts around every placeable (see the module documentation).
pub(crate) const ISOLATION_MARKS: [char; 2] = ['\u{2068}', '\u{2069}'];

/// The first placeholder (see the module documentation); variable `i` is `FIRST_SLOT + i`.
const FIRST_SLOT: u32 = 0xE000;

/// How many variables a template may have: the placeholders stay inside the Private Use Area's
/// first block, far from anything a catalogue writes.
const MAX_VARIABLES: usize = 8;

/// The placeholder of variable number `index` (`None` beyond [`MAX_VARIABLES`]).
fn slot(index: usize) -> Option<char> {
    if index >= MAX_VARIABLES {
        return None;
    }
    u32::try_from(index)
        .ok()
        .and_then(|index| char::from_u32(FIRST_SLOT + index))
}

/// A text with placeholders (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template(String);

impl Template {
    /// The message `key` of `localizer`, with each of `variables` left as its placeholder, in
    /// their order. Variables beyond the eighth are formatted as nothing (no message has more
    /// than three).
    ///
    /// # Errors
    /// The localizer's, if the message is missing.
    pub fn new(localizer: &Localizer, key: &str, variables: &[&str]) -> Result<Self, I18nError> {
        Self::with_values(localizer, key, variables, &[])
    }

    /// Like [`Template::new`], with `values` given to Fluent as they are: variables the message
    /// **selects** on (a plural count, a kind of problem), which placeholders cannot stand for
    /// (see the module documentation). The caller formats one template per combination of these
    /// values it needs, and fills the placeholders when the text is shown (F2.12b1: the
    /// readability warnings, which select on four variables and name two colours).
    ///
    /// # Errors
    /// The localizer's, if the message is missing.
    pub fn with_values(
        localizer: &Localizer,
        key: &str,
        variables: &[&str],
        values: &[(&str, FluentValue<'static>)],
    ) -> Result<Self, I18nError> {
        let mut args = FluentArgs::new();
        // The fixed values first; a placeholder variable of the same name (never expected) would
        // replace its value, since `set` keeps the last one.
        for (name, value) in values {
            args.set(*name, value.clone());
        }
        for (index, variable) in variables.iter().enumerate() {
            args.set(*variable, slot(index).map(String::from).unwrap_or_default());
        }
        Ok(Self(
            localizer
                .format(key, Some(&args))?
                .replace(ISOLATION_MARKS, ""),
        ))
    }

    /// The text with `values` in place of the placeholders, in the variables' order. A value
    /// missing is left empty. O(text length + values' length).
    #[must_use]
    pub fn fill(&self, values: &[&str]) -> String {
        let mut out =
            String::with_capacity(self.0.len() + values.iter().map(|v| v.len()).sum::<usize>());
        for c in self.0.chars() {
            let index = u32::from(c)
                .checked_sub(FIRST_SLOT)
                .and_then(|i| usize::try_from(i).ok());
            match index {
                Some(index) if index < MAX_VARIABLES => {
                    out.push_str(values.get(index).copied().unwrap_or_default());
                }
                _ => out.push(c),
            }
        }
        out
    }

    /// How many times the placeholder of variable `index` appears: exactly once in every
    /// language for most messages (the tests check it); a message may repeat a value on purpose
    /// (the readability warnings name the colour behind twice in some of their forms), and
    /// [`Template::fill`] then writes it at every place.
    #[must_use]
    pub fn slots(&self, index: usize) -> usize {
        slot(index).map_or(0, |placeholder| self.0.matches(placeholder).count())
    }
}

#[cfg(test)]
mod tests {
    //! Filling, with values in another order than the variables'.
    use super::*;

    /// Values go where the placeholders are, whatever their order in the text; a missing value
    /// is empty; other characters stay.
    #[test]
    fn values_fill_their_placeholders() {
        let template = Template("«\u{E001}» → «\u{E000}»!".to_owned());
        assert_eq!(template.fill(&["a", "b"]), "«b» → «a»!");
        assert_eq!(template.fill(&["a"]), "«» → «a»!");
        assert_eq!(template.slots(0), 1);
        assert_eq!(template.slots(1), 1);
        assert_eq!(template.slots(2), 0);
        assert_eq!(template.slots(99), 0);
    }
}
