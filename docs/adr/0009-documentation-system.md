# ADR-0009 — Documentation system

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

The project is developed across many AI-assisted sessions by an Italian-speaking owner and is
open source. Documentation must keep sessions consistent, be readable by the owner in Italian and
by contributors in English, and give end users good manuals — without drifting apart.

## Decision

1. **Four documents:**
   - `DOCUMENTAZIONE.md` (Italian) and `DOCUMENTATION.md` (en-US): developer documentation,
     explaining every file/module conversationally and in depth, with real code snippets.
     **Same content and same structure** (same headings, same code blocks), different language.
   - `GUIDA.pdf` (Italian) and `GUIDE.pdf` (en-US): user guides, built from Typst sources in
     `docs/guide/it/` and `docs/guide/en/` (same files, same chapters) with a shared template.
2. **Parity is enforced by automation:**
   - a Claude Code **Stop hook** (`scripts/check-docs-updated.sh`) blocks the end of a turn if
     sources changed without both developer documents being updated, or if their structural
     signature (heading levels + exact code blocks) differs. "Changed" means *changed during the
     current turn*: a `UserPromptSubmit` hook (`scripts/turn-start.sh`) snapshots the working tree
     at the start of each turn (amended 2026-10-28 after the Step 11 tests showed that a
     branch-wide comparison stops blocking once the documents were touched anywhere on the
     branch);
   - `cargo xtask check` repeats the developer-docs parity check and checks that the two guide
     source trees have the same files and heading structure (so CI catches it too).
3. **Commit trailer `User-visible: yes|no`** on every commit, enforced by `.githooks/commit-msg`.
   With `yes`, the commit must also change the sources of **both** guides. `yes` means something
   visible to the user, or the way they work, changes. The trailer must be in the **last
   paragraph** of the message, together with any other trailer (`Co-Authored-By`), or git does not
   recognise it.
4. **PDFs are not committed.** `cargo xtask guides` builds them into `dist/`; CI builds them and
   publishes them as artifacts with limited retention.
5. **Typst** (0.15.1 installed locally) is the typesetting system: fast, scriptable, a single
   binary on every platform, open source (Apache-2.0). Fonts are bundled in `assets/fonts/` (OFL),
   and Typst is invoked with `--ignore-system-fonts`, so PDFs are identical on every machine.
6. **Screenshots** are generated headless by the app itself (`cargo xtask screenshots`), always in
   en-US, and shared by both guides: only the text changes.

## Alternatives rejected

- **mdBook/Sphinx for guides:** good for web docs, weaker for a curated print-quality PDF.
- **LaTeX:** excellent output, but heavy toolchain and slower iteration than Typst.
- **Committing PDFs:** binary churn in history and stale files.

## Consequences

- Every source change costs a documentation update in two languages; that is deliberate.
- Guide screenshots depend on the GUI toolkit supporting headless rendering (ADR-0012 criterion).
  *Resolved by ADR-0012: our own renderer draws into an off-screen pixmap; ADR-0014 fixes the
  light theme for screenshots.*
