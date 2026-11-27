// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-i18n — interface languages
//!
//! ## Purpose
//! Own every user-visible string of the program, as **Fluent** message catalogs in six languages:
//! en-US (source), en-GB, it, fr, de, es (ADR-0008). en-US and en-GB are separate catalogs because
//! musical terminology differs (quarter note/crotchet, measure/bar, staff/stave).
//!
//! ## How it works
//! - The catalogs live in `crates/i18n/locales/<tag>/*.ftl` and are **embedded in the binary** at
//!   compile time with `include_str!`. Why: no file system access at start-up (faster, nothing to
//!   install next to the executable, identical behaviour on every platform, no path handling).
//!   The cost is a rebuild of this crate when a catalog changes, which is negligible.
//! - A [`Localizer`] is built for one [`Locale`] and holds a *fallback chain* of Fluent bundles:
//!   the requested language first, then its fallbacks (en-GB → en-US; every other language →
//!   en-US). The tests guarantee that every key exists in every catalog, so the fallback is only a
//!   safety net for a broken build, never the normal path.
//! - Messages are looked up by **key** (e.g. `menu-edit-undo`), never by English text. Plurals,
//!   gender and agreement are expressed in the catalogs with Fluent selectors; the code passes
//!   numbers and other values as [`FluentArgs`] and never concatenates translated fragments.
//!
//! ## What this crate must NOT do
//! - Depend on any other workspace crate: everything else depends on it, not vice versa.
//! - Decide *which* language to use from the operating system: that is the UI's job (it passes
//!   a [`Locale`] in), so this crate stays deterministic and testable. What it does offer is the
//!   pure rule that brings the tags a system reports (`de-CH`, `en_AU.UTF-8`) down to one of
//!   ours ([`Locale::matching`], [`Locale::negotiate`], ADR-0053 LR3): reading those tags from
//!   the system is `orchestra-ui`'s `platform` module's job.
//!
//! ## Allowed dependencies
//! None inside the workspace; `fluent-bundle` and `unic-langid` (ADR-0008).
//!
//! ## Example
//! ```
//! use orchestra_i18n::{FluentArgs, Locale, Localizer};
//!
//! let it = Localizer::new(Locale::It).expect("embedded catalogs are valid");
//! assert_eq!(it.format("menu-edit-undo", None).unwrap(), "Annulla");
//!
//! let mut args = FluentArgs::new();
//! args.set("count", 3);
//! // Fluent wraps placeables in Unicode bidi isolation marks (U+2068 … U+2069), see `format`.
//! assert_eq!(
//!     it.format("status-measures-selected", Some(&args)).unwrap(),
//!     "\u{2068}3\u{2069} battute selezionate"
//! );
//! ```

#![forbid(unsafe_code)]

use std::fmt;

use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
use unic_langid::LanguageIdentifier;

// Re-exported so that callers can build arguments without depending on `fluent-bundle`
// themselves (one place to upgrade the Fluent version).
pub use fluent_bundle::{FluentArgs, FluentValue};

/// One of the six interface languages supported by Orchestra.
///
/// A closed enum rather than a free-form string: the set of languages is a product decision
/// (ADR-0008), every catalog must be complete, and a `match` on this type makes the compiler list
/// every place to update when a language is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Locale {
    /// American English — the **source** language: every key is written here first.
    EnUs,
    /// British English (bar, stave, crotchet…): a separate catalog, not a spelling variant.
    EnGb,
    /// Italian.
    It,
    /// French.
    Fr,
    /// German.
    De,
    /// Spanish.
    Es,
}

impl Locale {
    /// Every supported locale, source language first. Tests iterate over this list, so adding a
    /// variant without adding it here would leave the new catalog untested; the exhaustive
    /// `match` in [`Locale::tag`] keeps both in view.
    pub const ALL: [Self; 6] = [
        Self::EnUs,
        Self::EnGb,
        Self::It,
        Self::Fr,
        Self::De,
        Self::Es,
    ];

    /// The BCP 47 language tag, which is also the name of the catalog directory
    /// (`locales/<tag>/`).
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::EnGb => "en-GB",
            Self::It => "it",
            Self::Fr => "fr",
            Self::De => "de",
            Self::Es => "es",
        }
    }

    /// Parses a BCP 47 tag produced by [`Locale::tag`] (exact, case-sensitive match).
    ///
    /// Returns `None` for any other tag: this is how the settings file's language is read back,
    /// and a file must name exactly one of ours. Bringing an arbitrary system locale such as
    /// `it-CH` or `en_AU.UTF-8` down to one of ours is [`Locale::matching`]'s job.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        // Linear search over six entries: simpler and faster than any map for this size.
        Self::ALL.into_iter().find(|locale| locale.tag() == tag)
    }

    /// The interface language that serves a speaker of `tag`, a language tag as an operating
    /// system writes it, or `None` when none of ours does (ADR-0053 LR3).
    ///
    /// # Which forms are read
    /// Both BCP 47 (`de-CH`, `en-Latn-GB`, what Windows and macOS give) and POSIX locale names
    /// (`it_IT.UTF-8`, `de_DE@euro`, what Linux's environment variables hold): the codeset after
    /// `.` and the modifier after `@` are ignored, `_` counts as `-`, and letters are compared
    /// without regard to case (BCP 47 tags are case-insensitive, RFC 5646 §2.1.1).
    ///
    /// # The rule
    /// - The **language** subtag decides: `it`, `fr`, `de`, `es` give that language whatever the
    ///   region (`it-CH`, `de-AT`, `fr-CA`, `es-419`): the catalogues are written for every
    ///   speaker of the language, not for one country.
    /// - `en` alone needs its **region**, because en-US and en-GB differ in their musical words
    ///   (*quarter note* / *crotchet*, *measure* / *bar*): Britain, Australia, New Zealand,
    ///   Ireland, South Africa and India use the British words and get en-GB; every other region,
    ///   Canada included (*quarter note* and *measure* there), and `en` without a region get
    ///   en-US, the source language. A script subtag (`Latn`) before the region is skipped.
    /// - Anything else (`pt-BR`, `zh-Hant-TW`, the POSIX `C` and `POSIX` locales, an empty
    ///   string) is not ours: `None`, so [`Locale::negotiate`] tries the user's next language.
    ///
    /// # Complexity
    /// O(length of `tag`); no allocation (the subtags are slices of `tag`).
    #[must_use]
    pub fn matching(tag: &str) -> Option<Self> {
        // The POSIX codeset (`.UTF-8`) and modifier (`@euro`) say nothing about the language: cut
        // the tag at the first of them. BCP 47 tags contain neither character.
        let end = tag.find(['.', '@']).unwrap_or(tag.len());
        let tag = tag.get(..end).unwrap_or(tag).trim();
        // Subtags are separated by `-` in BCP 47 and by `_` in POSIX names.
        let mut subtags = tag.split(['-', '_']);
        let language = subtags.next()?;
        let is = |code: &str| language.eq_ignore_ascii_case(code);
        if is("it") {
            Some(Self::It)
        } else if is("fr") {
            Some(Self::Fr)
        } else if is("de") {
            Some(Self::De)
        } else if is("es") {
            Some(Self::Es)
        } else if is("en") {
            Some(english_for_region(region_of(subtags)))
        } else {
            None
        }
    }

    /// The first interface language that serves one of `preferred`, the user's languages in the
    /// system's order of preference (most preferred first), or `None` when none does
    /// (ADR-0053 LR3). Each tag is read with [`Locale::matching`]; the caller falls back to
    /// [`Locale::EnUs`], the source language, on `None`.
    ///
    /// Why the first match and not the best one: the user ordered the list, and a system that
    /// says "Swiss German, then English" wants German even though English is also ours.
    ///
    /// # Complexity
    /// O(total length of the tags); no allocation.
    pub fn negotiate<'a>(preferred: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        preferred.into_iter().find_map(Self::matching)
    }

    /// The next locale to try when a message is missing, or `None` for the source language.
    ///
    /// en-GB falls back to en-US (closest language); every other language also falls back to
    /// en-US because it is the source catalog and therefore always complete.
    #[must_use]
    pub const fn fallback(self) -> Option<Self> {
        match self {
            Self::EnUs => None,
            Self::EnGb | Self::It | Self::Fr | Self::De | Self::Es => Some(Self::EnUs),
        }
    }

    /// The Fluent source files of this locale, embedded at compile time.
    ///
    /// Returned as a slice so that catalogs can later be split into several files (one per
    /// area of the interface) without changing the callers.
    #[must_use]
    pub const fn catalog_sources(self) -> &'static [&'static str] {
        match self {
            Self::EnUs => &[include_str!("../locales/en-US/main.ftl")],
            Self::EnGb => &[include_str!("../locales/en-GB/main.ftl")],
            Self::It => &[include_str!("../locales/it/main.ftl")],
            Self::Fr => &[include_str!("../locales/fr/main.ftl")],
            Self::De => &[include_str!("../locales/de/main.ftl")],
            Self::Es => &[include_str!("../locales/es/main.ftl")],
        }
    }
}

impl fmt::Display for Locale {
    /// Displays the BCP 47 tag (used in error messages and logs, never shown as UI text).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// The region subtag among `subtags`, the ones after the language subtag, or `None` without one.
///
/// BCP 47 orders them language, optional script (four letters, `Latn`), optional region (two
/// letters, `GB`, or three digits, `419`), then variants and extensions. So: skip one script,
/// take the next subtag if it has a region's shape, and stop otherwise (a variant is never a
/// region).
fn region_of<'a>(mut subtags: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    let mut next = subtags.next()?;
    if next.len() == 4 && next.bytes().all(|byte| byte.is_ascii_alphabetic()) {
        next = subtags.next()?;
    }
    let letters = next.len() == 2 && next.bytes().all(|byte| byte.is_ascii_alphabetic());
    let digits = next.len() == 3 && next.bytes().all(|byte| byte.is_ascii_digit());
    (letters || digits).then_some(next)
}

/// The English catalogue for a speaker from `region` (see [`Locale::matching`]): en-GB for the
/// regions that use the British musical words, en-US for every other one and for none.
fn english_for_region(region: Option<&str>) -> Locale {
    // Great Britain, Australia, New Zealand, Ireland, South Africa, India (ADR-0053 LR3, approved
    // by the owner on 2026-11-27). Canada stays with en-US on purpose: *quarter note* and
    // *measure* are the usual words there.
    const BRITISH: [&str; 6] = ["GB", "AU", "NZ", "IE", "ZA", "IN"];
    match region {
        Some(region) if BRITISH.iter().any(|code| region.eq_ignore_ascii_case(code)) => {
            Locale::EnGb
        }
        _ => Locale::EnUs,
    }
}

/// Everything that can go wrong while loading catalogs or formatting a message.
///
/// The `Display` texts are **developer-facing** diagnostics (logs, test failures), deliberately
/// in English and not translated: they describe bugs in catalogs or calling code, not situations
/// the user can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum I18nError {
    /// A locale's tag was rejected by `unic-langid` (would indicate a bug in [`Locale::tag`]).
    InvalidLanguageTag {
        /// The rejected tag.
        tag: &'static str,
    },
    /// A catalog file contains Fluent syntax errors.
    Parse {
        /// The catalog's locale.
        locale: Locale,
        /// Human-readable description of every parser error.
        details: String,
    },
    /// A catalog could not be added to its bundle (e.g. the same key defined twice).
    AddResource {
        /// The catalog's locale.
        locale: Locale,
        /// Human-readable description of every error.
        details: String,
    },
    /// No catalog in the fallback chain defines this key.
    MissingMessage {
        /// The requested message key.
        id: String,
    },
    /// The message exists but has no value (only attributes), so it cannot be formatted as text.
    NoValue {
        /// The requested message key.
        id: String,
    },
    /// The message exists but has no such attribute ([`Localizer::format_attribute`], F2.11d2b).
    MissingAttribute {
        /// The requested message key.
        id: String,
        /// The requested attribute.
        attribute: String,
    },
    /// Formatting failed, typically because an argument the message uses was not supplied.
    Format {
        /// Locale of the catalog that provided the message.
        locale: Locale,
        /// The requested message key.
        id: String,
        /// Human-readable description of every formatting error.
        details: String,
    },
}

impl fmt::Display for I18nError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidLanguageTag { tag } => write!(f, "invalid language tag `{tag}`"),
            Self::Parse { locale, details } => {
                write!(f, "syntax errors in the {locale} catalog: {details}")
            }
            Self::AddResource { locale, details } => {
                write!(f, "cannot load the {locale} catalog: {details}")
            }
            Self::MissingMessage { id } => {
                write!(f, "message `{id}` is not defined in any catalog")
            }
            Self::NoValue { id } => write!(f, "message `{id}` has no value"),
            Self::MissingAttribute { id, attribute } => {
                write!(f, "message `{id}` has no attribute `{attribute}`")
            }
            Self::Format {
                locale,
                id,
                details,
            } => write!(f, "cannot format `{id}` ({locale}): {details}"),
        }
    }
}

impl std::error::Error for I18nError {}

/// A Fluent bundle together with the locale it was built for.
///
/// `concurrent::FluentBundle` (instead of the default one) makes the bundle `Send + Sync`, so a
/// [`Localizer`] can be shared between the UI thread and worker threads. The price is a mutex
/// inside its internal memoizer of plural/number formatters, which is irrelevant here: messages
/// are formatted on UI events, never on the audio thread.
struct LocaleBundle {
    /// The locale whose catalogs this bundle contains.
    locale: Locale,
    /// The Fluent bundle that owns the parsed catalogs of `locale`.
    bundle: FluentBundle<FluentResource>,
}

/// Formats interface messages in one language, with fallback.
///
/// Building a `Localizer` parses the embedded catalogs of the locale and of its fallbacks
/// (a few microseconds for today's catalogs, linear in their size). Build it once when the
/// language is chosen and keep it; formatting does not re-parse anything.
pub struct Localizer {
    /// The requested locale, i.e. the first link of the chain.
    locale: Locale,
    /// The fallback chain, in lookup order: the requested locale first, en-US last. At most two
    /// entries today, so a `Vec` scan is the right structure.
    chain: Vec<LocaleBundle>,
}

impl fmt::Debug for Localizer {
    /// `FluentBundle` does not implement `Debug`; show the chain of locales instead.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let chain: Vec<Locale> = self.chain.iter().map(|b| b.locale).collect();
        f.debug_struct("Localizer")
            .field("locale", &self.locale)
            .field("chain", &chain)
            .finish()
    }
}

impl Localizer {
    /// Builds a localizer for `locale`, loading its catalogs and those of its fallbacks.
    ///
    /// # Errors
    /// Returns [`I18nError::Parse`] or [`I18nError::AddResource`] if an embedded catalog is
    /// invalid, and [`I18nError::InvalidLanguageTag`] if a tag is rejected. All three indicate a
    /// bug caught by this crate's tests, so in a released build they do not happen.
    pub fn new(locale: Locale) -> Result<Self, I18nError> {
        let mut chain = Vec::with_capacity(2);
        // Walk the fallback links: locale → fallback → … → None. The chain is finite because
        // `fallback` always moves towards en-US, whose fallback is `None`.
        let mut next = Some(locale);
        while let Some(current) = next {
            chain.push(LocaleBundle {
                locale: current,
                bundle: build_bundle(current)?,
            });
            next = current.fallback();
        }
        Ok(Self { locale, chain })
    }

    /// The locale this localizer was built for.
    #[must_use]
    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// Formats the message `id` with optional arguments.
    ///
    /// The first bundle of the chain that defines `id` is used. Fluent surrounds every placeable
    /// (e.g. `{ $count }`) with the Unicode isolation marks U+2068/U+2069. We keep this default
    /// on purpose: it guarantees correct rendering if a right-to-left language or RTL user data
    /// (a track name in Arabic or Hebrew) is ever mixed into a message, and our bundled fonts and
    /// text shaper must handle these invisible characters anyway.
    ///
    /// # Errors
    /// - [`I18nError::MissingMessage`] if no catalog in the chain defines `id`;
    /// - [`I18nError::NoValue`] if the message has only attributes;
    /// - [`I18nError::Format`] if formatting reported errors (for example a missing argument).
    ///   Fluent would still produce a best-effort string in that case, but we treat it as an
    ///   error so that such bugs surface in tests instead of showing `{$count}` to users.
    ///
    /// # Complexity
    /// O(c) bundle lookups (c = chain length ≤ 2), each a hash-map lookup, plus linear time in
    /// the size of the formatted pattern.
    pub fn format(&self, id: &str, args: Option<&FluentArgs<'_>>) -> Result<String, I18nError> {
        for link in &self.chain {
            let Some(message) = link.bundle.get_message(id) else {
                // Not in this catalog: try the next locale of the chain.
                continue;
            };
            let pattern = message
                .value()
                .ok_or_else(|| I18nError::NoValue { id: id.into() })?;
            let mut errors = Vec::new();
            let text = link.bundle.format_pattern(pattern, args, &mut errors);
            if !errors.is_empty() {
                return Err(I18nError::Format {
                    locale: link.locale,
                    id: id.into(),
                    details: join_debug(&errors),
                });
            }
            return Ok(text.into_owned());
        }
        Err(I18nError::MissingMessage { id: id.into() })
    }

    /// Formats the attribute `attribute` of the message `id` (`menu-file.mnemonic`, F2.11d2b),
    /// with optional arguments.
    ///
    /// # Why attributes
    /// A menu's mnemonic is a property of its name chosen by the translator, not a marker inside
    /// the text (ADR-0048 AM10): the name stays clean for assistive technologies and searches,
    /// and the letter lives next to it, in the same message, as Fluent's attributes are meant
    /// for.
    ///
    /// The first bundle of the chain that defines the **message** answers: an attribute missing
    /// there is an error even if a fallback catalogue has it, so a translation can never mix its
    /// own name with another language's letter.
    ///
    /// # Errors
    /// - [`I18nError::MissingMessage`] if no catalog in the chain defines `id`;
    /// - [`I18nError::MissingAttribute`] if the message has no such attribute;
    /// - [`I18nError::Format`] as [`Localizer::format`].
    ///
    /// # Complexity
    /// As [`Localizer::format`], plus a linear scan of the message's attributes (one or two).
    pub fn format_attribute(
        &self,
        id: &str,
        attribute: &str,
        args: Option<&FluentArgs<'_>>,
    ) -> Result<String, I18nError> {
        for link in &self.chain {
            let Some(message) = link.bundle.get_message(id) else {
                continue;
            };
            let pattern = message
                .get_attribute(attribute)
                .map(|attribute| attribute.value())
                .ok_or_else(|| I18nError::MissingAttribute {
                    id: id.into(),
                    attribute: attribute.into(),
                })?;
            let mut errors = Vec::new();
            let text = link.bundle.format_pattern(pattern, args, &mut errors);
            if !errors.is_empty() {
                return Err(I18nError::Format {
                    locale: link.locale,
                    id: format!("{id}.{attribute}"),
                    details: join_debug(&errors),
                });
            }
            return Ok(text.into_owned());
        }
        Err(I18nError::MissingMessage { id: id.into() })
    }
}

/// Parses every catalog of `locale` and adds it to a new bundle.
///
/// # Errors
/// See [`Localizer::new`].
fn build_bundle(locale: Locale) -> Result<FluentBundle<FluentResource>, I18nError> {
    let tag = locale.tag();
    let langid: LanguageIdentifier = tag
        .parse()
        .map_err(|_| I18nError::InvalidLanguageTag { tag })?;
    // The language list drives plural rules and number formatting for this bundle.
    let mut bundle = FluentBundle::new_concurrent(vec![langid]);
    for source in locale.catalog_sources() {
        // `try_new` takes an owned `String`; the copy of a few kilobytes happens once per
        // localizer, which is fine.
        let resource = FluentResource::try_new((*source).to_owned()).map_err(|(_, errors)| {
            I18nError::Parse {
                locale,
                details: join_debug(&errors),
            }
        })?;
        bundle
            .add_resource(resource)
            .map_err(|errors| I18nError::AddResource {
                locale,
                details: join_debug(&errors),
            })?;
    }
    Ok(bundle)
}

/// Joins the `Debug` representations of a list of errors with `"; "`.
///
/// Fluent's error types implement `Debug` (and not always a helpful `Display`), and these texts
/// are developer diagnostics only, so `Debug` is the most informative choice.
fn join_debug<E: fmt::Debug>(errors: &[E]) -> String {
    errors
        .iter()
        .map(|e| format!("{e:?}"))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    //! Unit tests of the public API. Catalog completeness is tested in `tests/parity.rs`.

    use super::*;

    /// Attributes are read from the message's own catalogue (F2.11d2b): a mnemonic exists in
    /// every language, a missing attribute and a missing message are typed errors.
    #[test]
    fn attributes_are_formatted_and_missing_ones_reported() {
        for locale in Locale::ALL {
            let localizer = Localizer::new(locale).unwrap();
            let letter = localizer
                .format_attribute("menu-file", "mnemonic", None)
                .unwrap();
            assert_eq!(letter.chars().count(), 1, "{locale}: {letter:?}");
        }
        let en = Localizer::new(Locale::EnUs).unwrap();
        assert!(matches!(
            en.format_attribute("menu-file", "nothing", None),
            Err(I18nError::MissingAttribute { .. })
        ));
        assert!(matches!(
            en.format_attribute("no-such-message", "mnemonic", None),
            Err(I18nError::MissingMessage { .. })
        ));
    }

    /// Every locale has a valid catalog, a localizer can be built and the tag round-trips.
    #[test]
    fn every_locale_loads_and_round_trips() {
        for locale in Locale::ALL {
            let localizer = Localizer::new(locale).unwrap();
            assert_eq!(localizer.locale(), locale);
            assert_eq!(Locale::from_tag(locale.tag()), Some(locale));
        }
    }

    /// en-GB uses its own terminology (glossary: measure → bar), not the en-US text.
    #[test]
    fn en_gb_is_a_separate_catalog() {
        let mut args = FluentArgs::new();
        args.set("count", 2);
        let gb = Localizer::new(Locale::EnGb).unwrap();
        let us = Localizer::new(Locale::EnUs).unwrap();
        assert!(
            gb.format("status-measures-selected", Some(&args))
                .unwrap()
                .ends_with("bars selected")
        );
        assert!(
            us.format("status-measures-selected", Some(&args))
                .unwrap()
                .ends_with("measures selected")
        );
    }

    /// Plural selection follows each language's CLDR rules (French "one" includes 0).
    #[test]
    fn plurals_follow_language_rules() {
        let fr = Localizer::new(Locale::Fr).unwrap();
        let mut zero = FluentArgs::new();
        zero.set("count", 0);
        assert_eq!(
            fr.format("status-measures-selected", Some(&zero)).unwrap(),
            "\u{2068}0\u{2069} mesure sélectionnée"
        );
        let en = Localizer::new(Locale::EnUs).unwrap();
        assert_eq!(
            en.format("status-measures-selected", Some(&zero)).unwrap(),
            "\u{2068}0\u{2069} measures selected"
        );
    }

    /// Unknown keys and missing arguments are errors, not silently wrong text.
    #[test]
    fn errors_are_reported() {
        let it = Localizer::new(Locale::It).unwrap();
        assert_eq!(
            it.format("no-such-key", None),
            Err(I18nError::MissingMessage {
                id: "no-such-key".into()
            })
        );
        assert!(matches!(
            it.format("status-measures-selected", None),
            Err(I18nError::Format { .. })
        ));
    }

    /// Unknown tags are rejected rather than guessed.
    #[test]
    fn unknown_tags_are_rejected() {
        assert_eq!(Locale::from_tag("pt-BR"), None);
        assert_eq!(Locale::from_tag("en-us"), None);
    }

    /// The system's tags are brought down to ours by language, and by region for English
    /// (ADR-0053 LR3), in both BCP 47 and POSIX forms.
    #[test]
    fn system_tags_match_by_language_and_english_region() {
        let cases = [
            // Our own tags, and their POSIX spellings with codeset and modifier.
            ("en-US", Some(Locale::EnUs)),
            ("en-GB", Some(Locale::EnGb)),
            ("it", Some(Locale::It)),
            ("it_IT.UTF-8", Some(Locale::It)),
            ("de_DE.UTF-8@euro", Some(Locale::De)),
            ("fr_FR@dict", Some(Locale::Fr)),
            // Other regions of our languages.
            ("it-CH", Some(Locale::It)),
            ("de-AT", Some(Locale::De)),
            ("de-CH", Some(Locale::De)),
            ("fr-CA", Some(Locale::Fr)),
            ("es-MX", Some(Locale::Es)),
            ("es-419", Some(Locale::Es)),
            // English: the British words for these regions, the American ones otherwise.
            ("en-AU", Some(Locale::EnGb)),
            ("en_NZ.UTF-8", Some(Locale::EnGb)),
            ("en-IE", Some(Locale::EnGb)),
            ("en-ZA", Some(Locale::EnGb)),
            ("en-IN", Some(Locale::EnGb)),
            ("en-CA", Some(Locale::EnUs)),
            ("en", Some(Locale::EnUs)),
            ("en-001", Some(Locale::EnUs)),
            ("en-Latn-GB", Some(Locale::EnGb)),
            // Case does not matter in a language tag.
            ("EN-gb", Some(Locale::EnGb)),
            ("IT", Some(Locale::It)),
            // A variant is not a region.
            ("en-oxendict", Some(Locale::EnUs)),
            // Not ours.
            ("pt-BR", None),
            ("zh-Hant-TW", None),
            ("C", None),
            ("C.UTF-8", None),
            ("POSIX", None),
            ("", None),
            ("ita", None),
        ];
        for (tag, expected) in cases {
            assert_eq!(Locale::matching(tag), expected, "{tag:?}");
        }
    }

    /// The first of the user's languages that is ours wins, in the user's order.
    #[test]
    fn negotiation_takes_the_first_language_that_is_ours() {
        assert_eq!(
            Locale::negotiate(["pt-BR", "de-CH", "en-US"]),
            Some(Locale::De)
        );
        assert_eq!(Locale::negotiate(["en-AU", "it-IT"]), Some(Locale::EnGb));
        assert_eq!(Locale::negotiate(["C", "es_ES.UTF-8"]), Some(Locale::Es));
        assert_eq!(Locale::negotiate(["ja-JP", "ko-KR"]), None);
        assert_eq!(Locale::negotiate([]), None);
    }
}
