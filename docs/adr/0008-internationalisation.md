# ADR-0008 — Internationalisation: six languages, Fluent, glossary, parity tests

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

The interface ships in **en-US** (source), **en-GB**, **it**, **fr**, **de**, **es**. Musical
terminology is not translated literally (a *quarter note* is a *crotchet* in British English, a
*semiminima* in Italian, a *noire* in French, a *Viertelnote* in German, a *negra* in Spanish), and
some languages need plural and gender agreement.

## Decision

1. **Project Fluent** is the message format: one catalog per language in
   `crates/i18n/locales/<lang>/*.ftl`. en-US and en-GB are **separate catalogs**. Fallback chain:
   en-GB → en-US; every other language falls back to en-US only for robustness (tests require all
   keys to be present anyway).
2. **Crates** (verified on crates.io, 2026-10-28):
   | Crate | Version | License | Role |
   |---|---|---|---|
   | `fluent-bundle` | 0.16.0 (2025-05-22) | Apache-2.0 OR MIT | Runtime: bundles, message formatting, plural rules |
   | `fluent-syntax` | 0.12.0 (2025-05-22) | Apache-2.0 OR MIT | Parser, used by the parity tests to list keys and placeables |
   | `unic-langid` | 0.9.6 (2025-05-09) | MIT OR Apache-2.0 | Language identifiers (already a dependency of `fluent-bundle`) |

   All three come from the official Project Fluent / unic-locale maintainers
   (<https://github.com/projectfluent/fluent-rs>). The release cadence is slow but the format is
   stable; we accept that.
3. **Not chosen:** `fluent-templates` 0.15.1 (convenient macros, but extra dependencies and less
   control over loading), `i18n-embed` 0.16.0 (targets `cargo-i18n` workflows we do not need),
   direct `fluent-langneg` 0.14.2 (would duplicate the 0.13 that `fluent-bundle` depends on; with
   six fixed languages, our own negotiation is trivial), ICU4X `icu_locale` 2.4 (excellent but far
   heavier than needed for plural rules, which `fluent-bundle` already handles via
   `intl_pluralrules`).
4. **Glossary first:** `crates/i18n/glossary.toml` holds one entry per musical/technical term with
   the correct term in each language, the sources consulted and a note on why it is not a literal
   translation. Every new or changed string is checked against it; missing terms are researched
   (music dictionaries, localised manuals of notation/DAW software, academic glossaries) before
   being added. Process: `docs/i18n-process.md`.
5. **No user-visible text in code**: only message keys. Plurals, gender and agreement through
   Fluent selectors, never string concatenation.
6. **Tests:** every key exists in all six languages; placeables (variables) are the same set in
   every language; an obvious-case scan finds hard-coded user-visible strings in UI code.

## Alternatives rejected

- **gettext (.po):** mature, but plural/gender handling is weaker than Fluent's and the Rust
  tooling is less idiomatic.
- **Hand-rolled key/value files:** would reinvent plural rules badly.

## Consequences

- Translations can be reviewed by musicians per language without reading code.
- The glossary is a living reference that must be maintained with every UI change.
