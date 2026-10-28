# ADR-0010 — Project file format: versioned, with migrations from day one

- **Status:** Accepted for the versioning/migration policy · **Proposed** for the concrete format
  (decided in Phase 1, when the model types exist)
- **Date:** 2026-10-28

## Context

Users must be able to open old projects in new versions of Orchestra forever. Formats always
change; without versioning and migrations from the very first file, the first change is a break.

## Decision (Accepted part)

1. Every project file carries a **format version number** (a single monotonically increasing
   integer, independent of the application version) in a header that can be read without parsing
   the rest.
2. **Migrations** are a chain of pure functions `vN → vN+1`. Opening a file of version k applies
   migrations k → current in memory; saving always writes the current version.
3. Every format change ships with its migration **and** a **golden file** of the previous version
   under `crates/io/tests/golden/vN/`, plus a test that loads every golden file of every past
   version and checks the result.
4. A file with a version **newer** than the program understands is refused with a typed error
   ("made with a newer Orchestra"), never partially loaded.
5. Unknown fields are not silently dropped within a version (forward-compatibility policy to be
   detailed with the format).

## Proposed part (to be decided in Phase 1)

Candidates to evaluate with real model types: a structured text format (e.g. a self-describing
JSON/RON-like encoding, diff-friendly, good for version control and debugging) for the musical
document, possibly in a container (zip or directory bundle) that also holds audio and video media
or references to them. Criteria: robustness, load time (≤ 1 s for 100 bars, ADR-0007), diffability,
size, ease of migration, cross-platform paths.

## Alternatives rejected

- **No version until 1.0:** the classic mistake; the first change becomes a forced incompatibility.
- **Application version as format version:** couples unrelated release cadences.

## Consequences

- Golden files accumulate in the repository; they are small and form the compatibility contract.
