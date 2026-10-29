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
//!   a [`Locale`] in), so this crate stays deterministic and testable.
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
    /// Returns `None` for any other tag. Negotiating an arbitrary system locale such as `it-CH`
    /// or `en_AU.UTF-8` down to one of ours is a UI concern and will be implemented there.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        // Linear search over six entries: simpler and faster than any map for this size.
        Self::ALL.into_iter().find(|locale| locale.tag() == tag)
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
}
