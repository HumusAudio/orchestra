# ADR-0001 — Rust as the implementation language; policy on native dependencies

- **Status:** Accepted
- **Date:** 2026-10-28
- **Deciders:** project owner (decision given), recorded by Claude Code in Phase 0

## Context

Orchestra combines three demanding domains: a real-time audio engine (hard latency deadlines, no
allocation on the audio thread), a music-engraving engine (complex layout, incremental updates)
and a video timeline. It must run well on a five-year-old mid-range laptop (ADR-0007) and on
Linux, Windows and macOS (ADR-0006). It is developed largely with AI assistance across many
sessions, so the language must catch as many mistakes as possible at compile time.

## Decision

1. The whole program is written in **Rust** (stable toolchain; edition 2024 and `rust-version
   = 1.99` at the time of writing, bumped deliberately together with the CI toolchain).
2. **C/C++ libraries are allowed only when no reasonable Rust alternative exists.** Typical
   legitimate cases: the official plugin SDKs/ABIs (VST3, AU), operating-system audio APIs, LV2
   world libraries, perhaps video decoding.
3. Every native dependency is recorded in `docs/native-deps.md` **before** it is merged, with
   justification, license, per-platform build requirements, size/compile-time impact and a
   replacement plan.
4. `unsafe` is forbidden (`#![forbid(unsafe_code)]`) in every crate except the designated FFI
   crates: `audio-engine`, `plugin-host`, `io`. There, every block carries a `// SAFETY:` proof.

## Alternatives rejected

- **C++ (JUCE-style stack):** the de-facto standard for audio, but memory-safety bugs are the
  most common failure class in long AI-assisted development, and JUCE's licensing (GPL or
  commercial) would tie us to one framework's choices.
- **A garbage-collected language (C#, Java, Go):** GC pauses are incompatible with an audio thread
  that must never block; a hybrid (GC UI + native audio) doubles the toolchains.
- **Zig:** attractive for low-level work, but the language and ecosystem (GUI, audio, Fluent,
  MusicXML) are not yet mature enough for a project of this size.

## Consequences

- Strong compile-time guarantees (ownership, `Send`/`Sync`) make the lock-free audio rules
  enforceable rather than aspirational.
- Some ecosystems (plugin hosting, GUI) are younger than their C++ counterparts: the spikes of
  Phase 0, Step 13 measure that risk before we commit (ADR-0011, ADR-0012). *Done: both ADRs
  chose to own these layers on low-level crates.*
- Each native dependency complicates cross-platform builds; the registry keeps that cost visible.
