# ADR-0034 — Public API of the library crates: one path per item, exhaustive errors

- **Status:** Accepted (owner, 2026-11-09; second codebase review, point P3.3, the owner asked
  for an ADR before any change and accepted it as written).
- **Date:** 2026-11-09
- **Amends:** nothing. It adds a rule next to CLAUDE.md §9 ("public APIs change deliberately").
  ADR-0026's `#[non_exhaustive]` `ChangeSet` stays as it is (see D3).

## Context

The second codebase review found two habits in the public APIs of the library crates
(`core-time`, `core-model`, `playback`, `io`):

1. **Every module is public and re-exported too.** `core-model` has 24 `pub mod` lines and 24
   `pub use` lines, `core-time` 7 and 7, `playback` 2 and 3, `io` 2 and 1. So almost every type
   has two paths (`orchestra_core_model::Pitch` and `orchestra_core_model::pitch::Pitch`), and the
   module layout (how files are split, which is an implementation choice) is part of the public
   API: moving a type to another file, or splitting `document.rs` (point P3.2), breaks callers
   that used the long path. Today the long paths are used 8 times outside their crate
   (`orchestra_core_model::pitch::Alteration` ×7 in tests and benchmarks, plus the new
   `orchestra_core_time::invariant::` helpers of P1.3). Modules also make public helpers that only
   make sense inside the crate (for example `core-model::edit`, one `pub fn`).

2. **`#[non_exhaustive]` is used without a rule.** 9 types have it (`ChangeSet`, by ADR-0026;
   `Command`, `EventChange`, `MarkSpec`, `MeterMode`; `PerformError`, `UnrollError`, `MidiError`,
   `MidiWarning`), the ~60 other public enums do not, and nothing says which way is right.

### What `#[non_exhaustive]` actually buys here (checked 2026-11-09)

The review report suggested putting `#[non_exhaustive]` on the error enums. Looking closer, that
advice comes from published libraries and fits this workspace badly:

- **Its benefit is semver for outside users.** A crate outside the workspace cannot write an
  exhaustive `match`, so adding a variant is not a breaking change for it. Orchestra publishes
  nothing (`publish = false` in the workspace manifest), and CLAUDE.md §9 already requires every
  caller to be updated in the same commit when an API changes. Nobody outside the workspace reads
  these types.
- **Its cost falls on our own crates.** Across crates, a `#[non_exhaustive]` enum forces a `_` arm
  in every `match`. The interface (`ui`) will map every error variant to a Fluent key (ADR-0008,
  CLAUDE.md §7). With an exhaustive `match`, a new variant fails to compile until it gets its key;
  with a `_` arm it silently falls back to a generic message, in six languages. The exhaustive
  `match` is the guarantee we want.
- **Inside its own crate** `#[non_exhaustive]` changes nothing (matches stay exhaustive there).

## Decision

### D1 — One path per public item

In the library crates, modules are **private** (`mod`, or `pub(crate)` where a sibling module
needs them). The public API is what the crate root re-exports with `pub use`, listed in `lib.rs`.
A module stays public only when it is a **namespace** worth its name, because its items would be
ambiguous or confusing at the root (for example `core-time::invariant::convert_or`). Each such
exception has a comment in `lib.rs` saying why.

Rustdoc then shows one path per item, the file layout becomes free to change (P3.2's split of
`document.rs`, `command.rs` and `edit.rs` breaks no caller), and an item that is `pub` only for a
sibling module becomes `pub(crate)`: the existing `unreachable_pub` lint then reports every `pub`
that the root does not export.

### D2 — Error and warning enums stay exhaustive

Error and warning enums (`DocumentError`, `CommandError`, `LoadError`, `SaveError`,
`MigrationError`, `MidiError`, `MidiWarning`, `PerformError`, `UnrollError`, …) are **not**
`#[non_exhaustive]`, so that every consumer, the interface's Fluent mapping above all, has to
handle every variant, and a new variant breaks the build until it is handled. The four error and
warning enums that have it today (`PerformError`, `UnrollError`, `MidiError`, `MidiWarning`) lose
it.

### D3 — `#[non_exhaustive]` only where a generic case is the right answer

`#[non_exhaustive]` stays on a type only when its consumers *should* have a generic case for what
they do not know, and the type's documentation says so. Today:

- `ChangeSet` (ADR-0026: each consumer reads what its caches need and must ignore the rest);
- the command option enums meant to grow with steps of the plan, whose consumers fall back to a
  default (`MeterMode`, whose `Rewrite` mode arrives later; `Command`, `EventChange`,
  `MarkSpec`), as their documentation already explains.

Any new use states its reason next to the attribute.

## Rejected alternatives

- **`#[non_exhaustive]` on every error enum** (the review report's first suggestion): see the
  context. It protects users we do not have, and removes the exhaustive check the interface
  needs.
- **Keeping public modules and adding `#[doc(hidden)]`:** the second path would still work, so the
  file layout would still be part of the API, just hidden.
- **Private modules without `pub use` (the long paths only):** the root path is the one used
  almost everywhere already (8 uses of the long paths against hundreds of the short ones).

## Consequences

- One PR (review group P3) makes the modules of `core-time`, `core-model`, `playback` and `io`
  private, completes the `pub use` lists, changes the 8 long paths, turns into `pub(crate)` what
  `unreachable_pub` then reports, and removes `#[non_exhaustive]` from the four error and warning
  enums. Behaviour is unchanged; only paths change.
- `DOCUMENTATION.md` / `DOCUMENTAZIONE.md` and `docs/ARCHITECTURE.md` describe each crate's API
  through its root.
- The rule applies to every future library crate (`notation-layout`, `audio-engine`, `ui`, …).
- If Orchestra ever publishes a crate (a plugin SDK, for example), that crate gets its own ADR on
  stability, where `#[non_exhaustive]` will be weighed again.
