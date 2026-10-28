// SPDX-License-Identifier: GPL-3.0-or-later

//! # orchestra-notation-layout — incremental engraving
//!
//! ## Purpose
//! Turn the model into **positioned notation**: systems, staves, glyphs (SMuFL code points),
//! stems, beams, slurs, ties, dynamics, all with their coordinates in score space. The UI only has
//! to draw what this crate produces.
//!
//! ## Responsibilities (planned for Phase 2)
//! - Horizontal spacing, line and page breaking, collision avoidance.
//! - **Incremental** re-layout: editing one note re-lays out the affected measure(s) and, only if
//!   their width changes, the affected system — never the whole score (target: ≤ 16 ms per edit on
//!   the reference hardware, CLAUDE.md §5).
//! - Glyph metrics from the bundled SMuFL font, so layout is identical on every platform.
//!
//! ## What this crate must NOT do
//! - Draw pixels or depend on a GUI toolkit: its output is plain geometry, testable headless.
//! - Modify the model: layout decisions (e.g. a manual break) are stored in the model through
//!   commands issued by the UI, not by this crate.
//!
//! ## Allowed dependencies
//! `orchestra-core-time`, `orchestra-core-model`.

#![forbid(unsafe_code)]
