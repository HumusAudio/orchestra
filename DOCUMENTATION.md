# Orchestra — developer documentation

This is the developer documentation of Orchestra, written in American English. Its Italian twin is
`DOCUMENTAZIONE.md`: the two files say the same things with **the same structure** (same headings
at the same levels, same code blocks, byte for byte); only the language of the prose changes. A
Claude Code hook and `cargo xtask check` both verify this, so the two can never drift apart.

The *why* behind each big decision is in the ADRs (`docs/adr/`); the crate map is in
`docs/ARCHITECTURE.md`; the state of the work is in `ROADMAP.md`. This document explains *what is
in each file and how it works*, in a conversational way, with real snippets. It grows with the
code: whoever changes a source file updates the matching section here, in both languages, in the
same step.

## Where things are

The repository after Phase 0 looks like this:

```text
orchestra/
├── Cargo.toml              workspace manifest: members, shared metadata, lints, dependencies
├── Cargo.lock              exact dependency versions (committed: we build an application)
├── rustfmt.toml            formatter settings (edition repeated for single-file rustfmt)
├── clippy.toml             clippy settings (test exemptions, domain words for doc lints)
├── deny.toml               cargo-deny policy: licenses, sources, duplicates, advisories
├── .cargo/config.toml      the `cargo xtask` alias
├── .gitattributes          LF line endings everywhere, binary assets untouched
├── .gitignore              build output, generated PDFs, editor files, secrets
├── .github/workflows/      CI: cargo xtask check on 3 OS, guides as artifacts
├── .githooks/commit-msg    enforces the `User-visible: yes|no` trailer
├── .claude/settings.json   Claude Code: sandbox network, rustfmt and documentation hooks
├── scripts/                helper scripts used by the hooks
├── crates/                 the program, one crate per responsibility
├── xtask/                  project automation (`cargo xtask …`)
├── assets/fonts/           bundled OFL fonts (text, code, SMuFL music font)
├── docs/                   ADRs, architecture, i18n process, user guide sources
├── CLAUDE.md               permanent rules for every development session
├── ROADMAP.md              status, next steps, open decisions
├── DOCUMENTAZIONE.md       this document, in Italian
└── DOCUMENTATION.md        this document, in English
```

## The Cargo workspace

### The root manifest

`Cargo.toml` declares the workspace. A few choices deserve an explanation.

**Directories and package names.** Each crate lives in `crates/<responsibility>/` (the automation
tool in `xtask/`, as the cargo-xtask convention wants). Package names carry an `orchestra-`
prefix: `crates/io` is the package `orchestra-io`, imported in Rust as `orchestra_io`. Bare names
like `io` or `ui` would be confusing next to `std::io` and would clash with crates.io if we ever
published something.

**Shared metadata.** Version, edition, minimum Rust version, license and repository are written
once in `[workspace.package]` and inherited by every crate:

```toml
[workspace.package]
version = "0.0.0"
edition = "2024"
rust-version = "1.99"
license = "GPL-3.0-or-later"
repository = "https://github.com/HumusAudio/orchestra"
publish = false
```

Edition 2024 is the newest stable edition (rustc 1.99.1 rejects `2027`, checked on 2026-10-28).
`rust-version` is the toolchain we develop with; it is bumped deliberately, together with the CI
toolchain, instead of drifting. `publish = false` prevents an accidental `cargo publish`.

**Internal dependencies.** Every crate is listed in `[workspace.dependencies]` with its path, and
members refer to it with `orchestra-core-time.workspace = true`. Moving a crate then means
changing one line. External dependencies are listed in the same table, each with a comment saying
who checked it and where it is justified.

**Resolver 3.** It is already the default for edition 2024, but we write it explicitly: it makes
dependency resolution aware of `rust-version`, preferring versions that compile with our minimum
toolchain.

### Lints: the rules the compiler enforces

The `[workspace.lints]` tables turn the code-style rules of `CLAUDE.md` into compiler errors, so
they do not depend on anybody's memory:

```toml
[workspace.lints.rust]
missing_docs = "deny"
unsafe_op_in_unsafe_fn = "deny"

[workspace.lints.clippy]
missing_docs_in_private_items = "deny"
undocumented_unsafe_blocks = "deny"
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
```

- `missing_docs` (rustc) covers the public API; `missing_docs_in_private_items` (clippy) covers
  everything else: every function, struct, field and constant, even private, needs a doc comment.
- `undocumented_unsafe_blocks` requires a `// SAFETY:` comment on every `unsafe` block.
- `unwrap_used`, `expect_used` and `panic` forbid panicking shortcuts in library code: errors are
  typed and returned. `clippy.toml` lifts these three inside `#[test]` functions, where a failed
  assertion *is* the way to report an error.
- `dbg_macro`, `todo` and `unimplemented` are warnings locally, but `cargo xtask check` runs clippy
  with `-D warnings`, so in CI they are errors too.

The **pedantic** group is enabled as a proposal, with three lints silenced
(`module_name_repetitions`, `must_use_candidate`, `similar_names`) and a comment in `Cargo.toml`
explaining each choice; the selection is still an open decision in `ROADMAP.md`. The cast lints
stay on deliberately: audio and time code converts between sample counts, frames and floats, and
every lossy conversion must be visible.

`unsafe_code` is not in the workspace table. Each crate's root starts with
`#![forbid(unsafe_code)]`, except `audio-engine`, `plugin-host` and `io`, the three crates that will
talk to C APIs. A workspace-level `forbid` could not be relaxed for just those three, and the
attribute makes the exception visible right at the top of the crate.

`xtask` does not inherit this table: it is a developer tool that prints and exits with error codes,
so it declares its own (documentation required, pedantic on, `unsafe` forbidden).

### Formatter, clippy and the cargo alias

`rustfmt.toml` repeats the edition. That looks redundant, but the PostToolUse hook runs `rustfmt`
on single files, outside of cargo, and without that line rustfmt would assume edition 2015.

`clippy.toml` holds the test exemptions described above and the list of domain words the
`doc_markdown` lint must not ask to put in backticks (SMuFL, MusicXML, PipeWire, CoreAudio…).

`.cargo/config.toml` defines the alias that makes `cargo xtask` work without installing anything:

```toml
[alias]
xtask = "run --quiet --package xtask --"
```

### cargo-deny: licenses, sources, duplicates, vulnerabilities

`deny.toml` (schema of cargo-deny 0.20) is the dependency policy of ADR-0002:

- **licenses**: only licenses compatible with distributing Orchestra under GPL-3.0-or-later
  (MIT, Apache-2.0, BSD, ISC, Zlib, MPL-2.0, LGPL, GPL-3.0, GPL-2.0-*or-later*, Unicode-3.0…).
  GPL-2.0-*only* is not compatible and is therefore absent;
- **bans**: two versions of the same crate are an error (`multiple-versions = "deny"`), because
  they cost compile time and binary size; wildcard versions are forbidden;
- **sources**: only crates.io;
- **advisories**: known vulnerabilities and unmaintained crates from the RustSec database.

The graph is checked for all six targets we will ship (Linux, Windows, macOS on x86_64 and
aarch64), so a Windows-only dependency with a bad license is caught on Linux too. The RustSec
database is kept in `target/advisory-dbs` instead of `~/.cargo`, which is read-only inside the
Claude Code sandbox.

### Line endings and ignored files

`.gitattributes` stores every text file with LF and checks it out with LF on every system,
Windows included. Shell scripts must be LF or bash refuses them, and the documentation parity
hash must be identical everywhere. Fonts, images, PDFs and audio files are marked binary.

`.gitignore` excludes `target/`, `dist/` (the generated PDFs are never committed), editor folders
and `.env` files.

## The crates

In Phase 0 every crate contains only its crate-level documentation (purpose, responsibilities,
what it must *not* do, allowed dependencies), except `orchestra-i18n`, which already works. The
dependency rules are summarised in `docs/ARCHITECTURE.md`; here is what each crate is for.

### orchestra-core-time

Exact musical time: positions and durations as fractions of a whole note, the single tempo map,
conversions to seconds, samples and video frames (ADR-0005). The conversion functions will be
called by the audio thread, so they must not allocate. No dependencies inside the workspace.

### orchestra-core-model

The one project document (ADR-0003): tracks, notes with their two layers (written intent and
performance offsets, ADR-0004), the form of the piece, commands and the single undo/redo history.
It knows nothing about views, layout, audio or files.

### orchestra-playback

Derives what is played from what is written: unrolled repeats and jumps, merged ties, dynamics
turned into velocity and curves, the tolerance threshold that decides whether a DAW move is
microtiming or a new written position. Its output is a cache, never a second editable copy.

### orchestra-notation-layout

Incremental engraving into positioned glyphs: plain geometry, no drawing, so it can be tested
headless. Editing one note re-lays out only what is affected.

### orchestra-plugin-host and orchestra-audio-engine

The plugin host exposes our own `PluginHost` interface over CLAP, VST3, LV2 (and AU), with every
plugin in its own process (ADR-0011, still Proposed). The audio engine runs the real-time graph
under the strict rules of ADR-0007 and offers a null/offline backend for tests. Both may contain
`unsafe` code, always with a `// SAFETY:` comment.

### orchestra-io, orchestra-ui, orchestra-video, orchestra-app

`io` handles the versioned project file with migrations (ADR-0010), MIDI and MusicXML. `ui` draws
the views and turns input into commands (toolkit in ADR-0012). `video` is intentionally empty until
the last phase. `app` builds the `orchestra` executable and only wires the others together.

### orchestra-i18n: the interface in six languages

This is the first crate with real code. It owns every user-visible string, in Fluent catalogs for
en-US (source), en-GB, it, fr, de and es (ADR-0008).

#### Catalogs

Each language has a folder in `crates/i18n/locales/` with a `main.ftl` file. Messages are found by
key, never by English text; numbers and other values are variables, and plurals are written for
each language's own rules:

```ftl
status-measures-selected =
    { $count ->
        [one] { $count } measure selected
       *[other] { $count } measures selected
    }
```

The en-GB catalog is a separate file, not a spelling patch: in it the same message says "bars",
because in British usage a measure is a *bar*.

The catalogs are compiled into the program with `include_str!`. That avoids file access at
start-up (faster, nothing to install next to the executable, no paths to get wrong on Windows)
at the price of a rebuild of this crate when a catalog changes, which is negligible.

#### The glossary

`crates/i18n/glossary.toml` lists every musical and technical term with the correct word in each
language, the sources consulted and a note on the choice. Musical terms are not translated
literally: a quarter note is a *crotchet* in British English, a *semiminima* in Italian, a *noire*
in French. The first 45 terms were researched in the UI translation catalogs of MuseScore
(notation) and Ardour (DAW), in Apple's localised Logic Pro guide and in Wikipedia's interlanguage
links. Where sources disagree the entry says so and carries `review = true`, waiting for a
native-speaking musician. The procedure for new texts is in `docs/i18n-process.md`.

#### The Localizer

The public API is small: a closed `Locale` enum and a `Localizer` built for one locale.

```rust
use orchestra_i18n::{FluentArgs, Locale, Localizer};

let it = Localizer::new(Locale::It)?;
let mut args = FluentArgs::new();
args.set("count", 3);
let text = it.format("status-measures-selected", Some(&args))?;
```

`Localizer::new` parses the catalog of the requested language and of its fallbacks (en-GB falls
back to en-US, every other language to en-US too) into a chain of Fluent bundles; `format`
uses the first bundle of the chain that knows the key. The bundles are the `concurrent` variant,
so a `Localizer` can be shared between threads.

`format` is strict: an unknown key, a message without a value or a formatting error (for example
a missing `$count`) is a typed `I18nError`, not a best-effort string with `{$count}` in it. Fluent
also wraps every placeable in the invisible Unicode isolation marks U+2068/U+2069: we keep them,
because they make mixed left-to-right and right-to-left text (a track named in Arabic, say) render
correctly.

#### The tests

`crates/i18n/tests/parity.rs` parses all six catalogs with `fluent-syntax` and fails if a key is
missing (or left over) in any language, if a message uses different variables than in en-US, or
if a message cannot be formatted in some language with sample values that reach every plural
category.

`crates/i18n/tests/no_hardcoded_strings.rs` scans the `ui` and `app` crates for string literals
that look like interface text (a capital letter and at least two words). It is a heuristic for the
obvious cases; a developer-only string can be exempted with a `// i18n-ignore: <reason>` comment
on the same line.

## Automation

### cargo xtask

`xtask/` is a small Rust program with no dependencies, run through the alias described above:

```sh
cargo xtask setup        # use the versioned git hooks (once per clone)
cargo xtask check        # everything CI checks
cargo xtask guides       # build dist/GUIDA.pdf and dist/GUIDE.pdf
cargo xtask screenshots  # not implemented yet (needs the GUI toolkit)
cargo xtask bench        # not implemented yet (no hot paths yet)
```

`check` runs, in this order: `cargo fmt --check`, the documentation and guide parity checks,
`cargo clippy -D warnings`, `cargo nextest` (which includes the translation parity tests), the
doctests (nextest does not run them) and `cargo deny check`. It stops at the first failure.

The parity checks live in `xtask/src/parity.rs`. For the two developer documents they compute the
same structural signature as the Stop hook: the sequence of heading levels plus the exact content
of code blocks. For the guides they also require the same files in `docs/guide/it/` and
`docs/guide/en/`, and in each pair of Typst files the same headings, `#include`s and screenshots.
On failure they point at the first difference.

`screenshots` and `bench` deliberately fail with an explanation instead of doing nothing, so
nobody mistakes them for working steps.

### Claude Code hooks

The project's `.claude/settings.json` configures three hooks (and the sandbox, see the last section):

- **PostToolUse on Edit/Write**: when Claude writes a `.rs` file, `rustfmt` formats it at once.
  The hook reads the file path from the JSON on its standard input:

```sh
FILE=$(jq -r '.tool_input.file_path // empty'); case "$FILE" in *.rs) rustfmt "$FILE" 2>/dev/null ;; esac; exit 0
```

- **UserPromptSubmit**: when a new turn starts, `scripts/turn-start.sh` takes a snapshot of the
  working tree and stores its git tree id in `.git/claude-turn-start-<session>`. The snapshot is
  made by `scripts/worktree-snapshot.sh` with a temporary copy of the index (`git add -A` into
  it, then `git write-tree`), so HEAD, branches, the real index and the stash are never touched.
- **Stop**: when Claude is about to end its turn, `scripts/check-docs-updated.sh` takes a new
  snapshot and compares it with the one from the start of the turn, so it sees exactly the files
  changed *in this turn*, including files committed during the turn. If any source changed
  (`.rs`, `.toml`, `.ftl` under `crates/` or `xtask/`) and `DOCUMENTAZIONE.md` or
  `DOCUMENTATION.md` was not touched, or the two have different structures, it answers
  `{"decision": "block", "reason": …}` and Claude has to fix it before finishing. A
  `stop_hook_active` flag prevents an endless loop. Without a snapshot it falls back to every
  change on the branch since `origin/main`; that was the original design, and the Phase 0 tests
  showed its flaw: once the documents had been touched anywhere on the branch, it never blocked
  again.

### The commit-msg hook

`.githooks/commit-msg` refuses any commit without a `User-visible: yes` or `User-visible: no`
trailer; with `yes` the commit must also change the sources of both guides (`docs/guide/it/` and
`docs/guide/en/`). Merge, revert and fixup commits are exempt.

Git only recognises trailers in the **last paragraph** of the message, so `User-visible` must be
in the same block as any other trailer:

```text
Add i18n infrastructure (step 9)

Longer explanation of the change.

User-visible: no
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
```

Git does not version `.git/config`, so every clone must run `cargo xtask setup` once to activate
the hook.

### Continuous integration

`.github/workflows/ci.yml` runs on every push to any branch:

- **check**, on `ubuntu-latest`, `windows-latest` and `macos-latest`: installs Rust 1.99.1 (the
  same version as `rust-version`), restores the build cache, installs prebuilt `cargo-nextest` and
  `cargo-deny`, and runs `cargo xtask check`. The three platforms run independently
  (`fail-fast: false`), so a Windows failure does not hide a macOS one;
- **guides**, on Linux: installs Typst 0.15.1, runs `cargo xtask guides` and uploads
  `GUIDA.pdf` and `GUIDE.pdf` as the `user-guides` artifact, kept for 30 days.

Only the standard GitHub-hosted runners are used: they are free for public repositories, while
larger runners are billed even there. Every third-party action is pinned to a full commit hash,
with the tag in a comment, because a tag can be moved by whoever controls the action's repository:

```yaml
- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
```

The workflow only has read access to the repository, and a newer push to the same branch cancels
the older run.

## The user guides

The user guides are written in Typst. `docs/guide/template.typ` defines the whole look once: an
ink-blue cover with a staff drawn in the Bravura music font, running headers, chapter openings
with a large brass number, code blocks, "note"/"tip"/"warning" boxes and a `screenshot` helper
that frames an image and adds a caption. The two guides only import it:

```typ
#import "/docs/guide/template.typ": guide

#show: guide.with(
  title: "Orchestra",
  // subtitle, edition, lang: set by each guide in its own language
)

#include "chapters/01-introduction.typ"
```

`docs/guide/it/` and `docs/guide/en/` contain the same files with the same chapters. Screenshots
will be generated headless by the application, always in English, and shared by both guides;
until the GUI toolkit is chosen, chapters use `screenshot-placeholder`, which has the same frame
and caption as a real screenshot. Typst's own words ("Figure", "Contents") follow the `lang`
setting, so the template contains no text in any language.

`cargo xtask guides` compiles both PDFs into `dist/` with `--ignore-system-fonts`, so only the
fonts in `assets/fonts/` are used and the output is identical on every machine. The PDFs are never
committed; CI publishes them as artifacts.

### Bundled fonts

`assets/fonts/` holds four families, all under the SIL Open Font License 1.1 (verified through the
GitHub API, recorded with versions and sources in `assets/fonts/README.md`): Source Serif 4 for
body text, Inter for headings, JetBrains Mono for code, and Bravura (with Bravura Text), the
reference font of the SMuFL music-font standard, which the notation engine will also use.

## Lessons from Phase 0

Things that are not obvious and cost time when you meet them for the first time:

- **The Claude Code sandbox** filters network traffic through an HTTP proxy. SSH (`git push` on
  port 22) and the `gh` token in the system keyring (reached through D-Bus) do not work inside it,
  so `git push`, `git fetch`, `git pull`, `git ls-remote` and `gh` are listed in
  `sandbox.excludedCommands`. That applies only to a command run *on its own*: `cd … && git push`
  stays sandboxed and fails.
- Inside the sandbox `~/.cargo`, `~/.rustup` and `.git/config` are read-only. New dependencies are
  downloaded with `cargo fetch` (run on its own, it is in `sandbox.excludedCommands`; after it,
  builds work with `--offline`),
  toolchain updates are done by the owner with `rustup update stable`, and `cargo xtask setup` runs
  outside the sandbox.
- CI caches `target/` with rust-cache, which prunes files it does not recognise as build output:
  the cargo-deny database we keep in `target/advisory-dbs` came back empty on the second run and
  `cargo deny` failed with "failed to load any advisories". The workflow now deletes that folder
  before `cargo xtask check`.
- `gh` inside a pipeline (`gh … | grep`) runs inside the sandbox, without its token: GitHub then
  answers as an anonymous user (60 requests per hour, no access to job logs). Run `gh` on its own.
- Commit trailers must be in the last paragraph of the message (see the commit-msg hook).
- `rustfmt` on a single file fails if that file declares a `mod` whose file does not exist yet,
  so the formatting hook silently skips it: write module files before the `mod` line that uses
  them, or run `cargo fmt --all` afterwards (`cargo xtask check` catches it anyway).
- Wikipedia's interlanguage links are a good start for terminology, but they can mislead: English
  "Bar (music)" links to Spanish "Barra de compás", which is the *barline*; the measure is
  *compás*. Software UI catalogs (MuseScore, Ardour) are better evidence of what users actually
  read.
- In Typst a single `*` is bold and `_` is italic; `**text**` produces an empty bold and a warning.
