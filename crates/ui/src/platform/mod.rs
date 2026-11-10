// SPDX-License-Identifier: GPL-3.0-or-later

//! Platform-specific code of the interface (ADR-0006 §2).
//!
//! Everything that differs between Linux, Windows and macOS lives under this module, behind
//! traits, so the rest of the crate never asks which system it runs on. Each submodule is one
//! concern, and inside it the per-platform files are chosen with `#[cfg(target_os = ...)]`:
//!
//! - `appearance`: the system's light/dark preference and its changes (ADR-0014 §5, ADR-0038).
//!
//! Like every module of the library crates, it is private (ADR-0034 D1): what callers need is
//! re-exported at the crate root.

pub(crate) mod appearance;
