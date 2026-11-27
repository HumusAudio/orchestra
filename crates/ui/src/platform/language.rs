// SPDX-License-Identifier: GPL-3.0-or-later

//! The system's languages (ADR-0053 LR2, SCALETTA F2.13): which languages the user asked their
//! system for, most preferred first, so *System language* in the Preferences means something.
//!
//! # What this module gives the rest of the program
//!
//! - [`LanguageSource`] is the one trait: the user's languages as tags, in their order. Its
//!   provided [`LanguageSource::locale`] brings them down to one of the six interface languages
//!   with [`Locale::negotiate`] (the rule lives in `orchestra-i18n`, where it is pure and tested
//!   on its own), falling back to en-US, the source language.
//! - [`SystemLanguages`] asks the system, through `sys-locale`.
//! - [`FixedLanguages`] answers a list given in advance: what an application holds until it is
//!   given the system's, so no test and no screenshot ever depends on the machine's languages
//!   (CLAUDE.md §3; the screenshots are always en-US, ADR-0009 §6).
//!
//! # What `sys-locale` reads on each system
//!
//! - **Linux** (and the other Unix systems): the environment, as gettext does: `LANGUAGE` first,
//!   a list separated by `:` (`it:en_GB:en`, which GNOME and KDE set from their language
//!   settings), then `LC_ALL`, `LC_MESSAGES` and `LANG`, one name each (`it_IT.UTF-8`). Duplicates
//!   are dropped; the codeset and modifier are cut (`.UTF-8`, `@euro`). No call into a library.
//! - **Windows**: `GetUserPreferredUILanguages`, the display languages of the user's account in
//!   their order (BCP 47).
//! - **macOS**: `CFLocaleCopyPreferredLanguages`, the order of *System Settings → General →
//!   Language & Region* (BCP 47).
//!
//! The Windows and macOS calls are `sys-locale`'s own declarations of the system's functions (no
//! extra crate, no native library to install: nothing for `docs/native-deps.md`), so the
//! `unsafe` they need stays inside it and this crate adds none.
//!
//! # When it is asked (ADR-0053 LR4)
//!
//! Once at start-up, and again whenever the user chooses *System language* in the Preferences.
//! The program does not follow a change of the system's language while it runs: on Linux the
//! environment of a running process never changes, and on Windows and macOS such a change asks
//! the user to sign out or to restart the programs anyway.
//!
//! # Cost
//!
//! A few environment reads or one system call, and one small `String` per language: nothing that
//! matters at the two moments it runs, on the interface thread (never on the audio thread).

use std::fmt;

use orchestra_i18n::Locale;

/// Where the user's languages come from (ADR-0053 LR2).
///
/// ```
/// use orchestra_i18n::Locale;
/// use orchestra_ui::{FixedLanguages, LanguageSource};
///
/// let swiss = FixedLanguages::new(["de-CH", "en-GB"]);
/// assert_eq!(swiss.locale(), Locale::De);
/// // None of the user's languages is ours: the source language.
/// assert_eq!(FixedLanguages::new(["ja-JP"]).locale(), Locale::EnUs);
/// ```
pub trait LanguageSource: fmt::Debug {
    /// The user's languages as tags (BCP 47, or POSIX names on Linux), most preferred first;
    /// empty when the system says nothing.
    fn preferred_languages(&self) -> Vec<String>;

    /// The interface language that serves the user best: the first of
    /// [`LanguageSource::preferred_languages`] that one of ours serves ([`Locale::negotiate`]),
    /// or en-US, the source language, when none does.
    fn locale(&self) -> Locale {
        let languages = self.preferred_languages();
        Locale::negotiate(languages.iter().map(String::as_str)).unwrap_or(Locale::EnUs)
    }
}

/// The system's languages, through `sys-locale` (see the module documentation for what it reads
/// on each system).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemLanguages;

impl LanguageSource for SystemLanguages {
    fn preferred_languages(&self) -> Vec<String> {
        // `get_locales` answers an iterator over owned tags, already without duplicates.
        sys_locale::get_locales().collect()
    }
}

/// A list of languages given in advance (tests, screenshots, and every interface until the
/// program hands it the system's).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FixedLanguages {
    /// The tags, most preferred first.
    languages: Vec<String>,
}

impl FixedLanguages {
    /// The languages `languages`, most preferred first.
    pub fn new<'a>(languages: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            languages: languages.into_iter().map(str::to_owned).collect(),
        }
    }
}

impl LanguageSource for FixedLanguages {
    fn preferred_languages(&self) -> Vec<String> {
        self.languages.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fixed list is answered as given, and brought down to ours in its order.
    #[test]
    fn fixed_languages_answer_their_list() {
        let source = FixedLanguages::new(["pt-BR", "fr_CA.UTF-8", "en-US"]);
        assert_eq!(
            source.preferred_languages(),
            ["pt-BR", "fr_CA.UTF-8", "en-US"]
        );
        assert_eq!(source.locale(), Locale::Fr);
        assert_eq!(FixedLanguages::default().locale(), Locale::EnUs);
    }

    /// The system's answer always gives one of our languages, whatever the machine running the
    /// test is set to (the value itself depends on it, so only its existence is checked).
    #[test]
    fn the_system_always_gives_one_of_our_languages() {
        assert!(Locale::ALL.contains(&SystemLanguages.locale()));
    }
}
