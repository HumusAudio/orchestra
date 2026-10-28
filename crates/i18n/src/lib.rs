// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-i18n — interface languages
//!
//! ## Purpose
//! Own every user-visible string of the program, as **Fluent** message catalogs in six languages:
//! en-US (source), en-GB, it, fr, de, es (ADR-0008). en-US and en-GB are separate catalogs because
//! musical terminology differs (quarter note/crotchet, measure/bar, staff/stave).
//!
//! ## Responsibilities (planned for Phase 0, Step 9)
//! - Load the catalogs and resolve messages with a fallback chain (en-GB → en-US).
//! - Tests that fail if a key is missing in any language or a placeholder is inconsistent.
//! - Plurals, gender and agreement are handled by Fluent selectors, never by string concatenation.
//!
//! ## What this crate must NOT do
//! - Depend on any other workspace crate: everything else depends on it, not vice versa.
//!
//! ## Allowed dependencies
//! None inside the workspace; the Fluent crates chosen in ADR-0008.

#![forbid(unsafe_code)]
