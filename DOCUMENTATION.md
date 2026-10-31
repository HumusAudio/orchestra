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
├── assets/logo/            the only Orchestra logo (outlined SVG, single colour)
├── docs/                   ADRs, architecture, i18n process, user guide sources
├── spikes/                 Phase 0 evaluation prototypes (separate workspace, never shipped)
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

The **pedantic** group is enabled, with three lints silenced (`module_name_repetitions`,
`must_use_candidate`, `similar_names`) and a comment in `Cargo.toml` explaining each choice; the
owner approved this selection on 2026-10-30 (SCALETTA step F0.16, decision D01). The cast lints
stay on deliberately: audio and time code converts between sample counts, frames and floats, and
every lossy conversion must be visible.

On top of pedantic, a few lints are picked one by one from clippy's `restriction` group (never
enabled as a whole: its lints contradict each other by design) and from rustc:

```toml
string_slice = "deny"                     # slicing a str off a char boundary panics
get_unwrap = "deny"                       # v.get(i).unwrap() is an unwrap in disguise
unreachable = "deny"                      # unreachable!() is a panic with another name
allow_attributes_without_reason = "warn"  # every #[allow] says why
print_stdout = "warn"                     # libraries never print
print_stderr = "warn"
exit = "warn"                             # libraries never end the process
mem_forget = "warn"                       # nor leak on purpose
```

- The first three complete the "no panics in library code" rule, so they are `deny` like it.
- `allow_attributes_without_reason` means a silenced lint must carry its explanation:
  `#[allow(clippy::panic, reason = "...")]`. It is our "comment every choice" rule, checked.
- Libraries do not print: output belongs to whoever calls them. The `app` crate lifts the two
  print lints in its root (with a reason), because the executable is where terminal output
  belongs. This will matter for plugin bridges too, whose stdout must stay clean (ADR-0011).
- On the rustc side: `unreachable_pub` (an item marked `pub` that nobody outside the crate can
  reach looks like public API but is not: use `pub(crate)`), `unused_qualifications` and
  `missing_debug_implementations` (every public type implements `Debug`, for error messages and
  test failures).

Two stricter lints apply only to the crates that everything else trusts. Because a crate that
says `[lints] workspace = true` cannot add its own table, they are attributes at the top of the
crate root:

```rust
// crates/core-time/src/lib.rs
#![deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
// crates/core-model/src/lib.rs
#![deny(clippy::indexing_slicing)]
```

`indexing_slicing` forbids `v[i]` (it panics when `i` is out of range: use `get` or iterate);
`arithmetic_side_effects` forbids plain `+ - *` on integers in `core-time`, so every operation on
the rational type goes through checked arithmetic and the overflow policy of ADR-0005, designed in
step F1.01. `clippy.toml` exempts tests from `indexing_slicing` and from the print lints.

Rejected, with reasons in `Cargo.toml`: the `nursery` group (lints still in development, which
would break CI on toolchain updates) and `as_conversions` (the pedantic cast lints already flag
the dangerous conversions).

`unsafe_code` is not in the workspace table. Each crate's root starts with
`#![forbid(unsafe_code)]`, except `audio-engine`, `plugin-host` and `io`, the three crates that will
talk to C APIs. A workspace-level `forbid` could not be relaxed for just those three, and the
attribute makes the exception visible right at the top of the crate.

`xtask` does not inherit this table: it is a developer tool that prints and exits with error codes,
so it declares its own (documentation required, pedantic on, `unreachable_pub` and
`unused_qualifications` on, `unsafe` forbidden).

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

In Phase 0 every crate contained only its crate-level documentation (purpose, responsibilities,
what it must *not* do, allowed dependencies), except `orchestra-i18n`, which already works. Phase
1 is filling them from the bottom: `orchestra-test-support` and the rational type of
`orchestra-core-time` exist so far. The
dependency rules are summarised in `docs/ARCHITECTURE.md`; here is what each crate is for.

### orchestra-core-time

Exact musical time: positions and durations as fractions of a whole note, the single tempo map,
conversions to seconds, samples and video frames (ADR-0005). The conversion functions will be
called by the audio thread, so they must not allocate. No dependencies inside the workspace (in
`[dev-dependencies]` only `orchestra-test-support` and `proptest`, for the tests). So far it
contains the rational type (step F1.03); the meter map (F1.04) and the tempo map with the
conversions (F1.05) come next.

#### The rational type

`Rational` (in `src/rational.rs`, re-exported at the crate root together with `RationalError`)
is the number every other crate will use for "when" and "how long": a fraction of two `i64`,
measured in whole notes. A quarter note is `1/4`, an eighth-note triplet `1/12`, bar 3 of a 4/4
piece starts at `2/1`. It was designed in ADR-0015; here is how it works in practice.

```rust
use orchestra_core_time::{Rational, RationalError};

let triplet_eighth = Rational::new(1, 12)?;
let three = triplet_eighth.checked_add(triplet_eighth)?.checked_add(triplet_eighth)?;
assert_eq!(three, Rational::new(1, 4)?); // exactly a quarter note
assert_eq!(Rational::new(6, -8)?.numerator(), -3); // always reduced: -3/4
assert_eq!(Rational::new(1, 0), Err(RationalError::DivisionByZero));
```

**Always in lowest terms.** The two fields are private, and every constructor and every
operation leaves the value with three properties (the *invariants*): the denominator is at least
1 (the sign lives in the numerator), numerator and denominator have no common factor (so zero is
always `0/1`), and the numerator is never `i64::MIN`. Since every value has exactly one way of
being written, equality and hashing are just "same fields", and the third rule means that `-x`
and `abs` can never overflow: `Neg` is the only operator the type implements.

**Why no `+`, `-`, `*`, `/`.** An operator must return a value, so on overflow it could only
crash, wrap around or saturate, and a wrapped or saturated position is a wrong note in the
wrong place, silently. So the operations that can fail are methods (`checked_add`,
`checked_sub`, `checked_mul`, `checked_div`, `checked_recip`) returning
`Result<Rational, RationalError>`, with two errors: `Overflow` and `DivisionByZero`. The error
texts (`Display`) are English for developers; the interface will translate each variant through
a Fluent key, as for every user-visible text.

**Exact even with huge numbers.** Each operation widens to `i128`, computes there, reduces, and
only at the end checks that the result fits back into `i64`. So it fails only when the *reduced*
result does not fit: `(i64::MAX/3) · (3/i64::MAX)` gives `1`, although the plain product of the
numerators would be enormous. This is what an existing crate such as `num-rational` does not do
(it fails as soon as an intermediate step overflows). The sum, for example:

```rust
pub const fn checked_add(self, rhs: Self) -> Result<Self, RationalError> {
    // Fast path: equal denominators (very common: notes in the same tuplet, integer bars).
    if self.den == rhs.den {
        let num = or_overflow!(wide(self.num).checked_add(wide(rhs.num)));
        return Self::from_wide(num, wide(self.den));
    }
    // ... otherwise (a·(d/g) + c·(b/g)) / ((b/g)·d) in i128, then reduce and narrow.
}
```

The products of two `i64` are below 2¹²⁶ and their sum below 2¹²⁷, so nothing can overflow in
`i128`; the code uses `checked_*` all the same, so that a mistake in that reasoning would become an
error instead of a wrong value. The same bound makes the comparison (`Ord`) exact and infallible:
`a/b < c/d` is `a·d < c·b` computed in `i128`, with a shortcut when the denominators are equal.

**Greatest common divisor.** Reduction uses the binary (Stein) algorithm, which only needs
shifts and subtractions instead of the slow integer division of Euclid's algorithm, and a 64-bit
version when both numbers fit in 64 bits (almost always: musical denominators are small).
Multiplication reduces *crosswise* before multiplying (`a` with `d`, `c` with `b`), so it needs
no final reduction. Cost: O(log n) for the arithmetic, O(1) for comparisons, and **no
allocation** anywhere. The instruction-count benchmarks are step F1.03b.

**`const fn`.** Constructors and arithmetic can run at compile time, so a fixed value is checked
when the program is built:

```rust
const QUARTER: Rational = match Rational::new(1, 4) {
    Ok(r) => r,
    Err(_) => panic!("1/4 is a valid rational"),
};
```

Being `const` has a price in verbosity: `?` and `TryFrom` cannot be used inside a `const fn` yet,
so the file has two small macros (`const_try!`, `or_overflow!`) that do the job of `?`, and a few
`as` casts each with its reason. The other methods are `abs`, `signum`, `is_zero`,
`is_positive`, `is_negative`, `is_integer`, `floor`/`ceil`/`trunc` (towards `i64`, they cannot
fail) and `to_f64`, an approximation for display only that must never flow back into the model.
There is deliberately no `Display` and no parsing: the text form is chosen with the file format
(F1.15). `Debug` prints `Rational(3/8)`.

Because `core-time` forbids plain `+ - *` on integers (`clippy::arithmetic_side_effects`), every
integer operation in the file is either `checked_*` or `wrapping_*` with a comment that proves it
cannot wrap. One curiosity: clippy rejects `wrapping_div` too (it could panic on a zero divisor),
so `trunc` is written as `floor` or `ceil` depending on the sign.

#### How it is tested

Three test files in `crates/core-time/tests/`, each its own binary:

- `rational_oracle.rs`: **property tests** with proptest against an **oracle**, an independent
  exact calculation written in the test itself in `i128`. The oracle uses the textbook formulas
  (`a/b + c/d = (a·d + c·b)/(b·d)`) and Euclid's gcd, while the library uses other algorithms
  (division by the gcd first, cross-reduction, Stein): if one of the two is wrong, they disagree.

  ```rust
  /// Oracle sum: `(a·d + c·b) / (b·d)`.
  fn oracle_add(x: Rational, y: Rational) -> Result<(i64, i64), RationalError> {
      let ((a, b), (c, d)) = (wide(x), wide(y));
      expected(a * d + c * b, b * d)
  }
  ```

  Uniformly random `i64` almost never land on the interesting cases, so the inputs come from a
  mix of generators: values near `i64::MAX` and `i64::MIN`, powers of two, products of small
  primes (like tuplets of tuplets), and *related* pairs (equal, opposite, reciprocal, same large
  denominator) where intermediate values are huge but the result fits. Each property runs 2048
  cases. To be sure that the tests really catch mistakes, two bugs were introduced on purpose
  during development (a `<` instead of `<=` in the range check, a wrong sign rule): both were
  found at once, with a minimal counter-example.
- `rational_cases.rs`: the edge cases written by hand, checkable on paper: `i64::MIN` as input,
  results that fit only after reduction, the overflows that must be reported (`-i64::MAX − 1`
  would be exactly `i64::MIN`), rounding on both sides of zero, the `Debug` form, a `const`
  dotted quarter.
- `rational_no_alloc.rs`: installs the counting allocator of `orchestra-test-support` and checks
  that no operation touches the heap, on the success paths and on the error paths.

Inside `src/rational.rs` a small `#[cfg(test)]` module checks the private helpers (Stein against
Euclid, the range check).

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
plugin in its own process and a host layer of our own for each format (ADR-0011). The audio engine runs the real-time graph
under the strict rules of ADR-0007 and offers a null/offline backend for tests. Both may contain
`unsafe` code, always with a `// SAFETY:` comment.

### orchestra-io, orchestra-ui, orchestra-video, orchestra-app

`io` handles the versioned project file with migrations (ADR-0010), MIDI and MusicXML. `ui` draws
the views and turns input into commands (no GUI toolkit: widgets and drawing are ours, ADR-0012). `video` is intentionally empty until
the last phase. `app` builds the `orchestra` executable and only wires the others together.

### orchestra-test-support: counting allocations in tests

A crate that exists **only for tests and benchmarks** (ADR-0016). Every other crate may use it,
but only in `[dev-dependencies]`: `cargo xtask check` fails otherwise, so it can never end up in
the program. It depends on nothing in the workspace, which is why even `core-time`, at the bottom
of the graph, can use it.

What it offers is a **global allocator that counts**: `CountingAllocator` passes every request on
to the system allocator, and before doing so it adds one to a counter. ADR-0007 requires it ("a
test fails if the audio thread allocates") and ADR-0015 needs it to prove that the rational type
never allocates. A test file registers it with one line and then measures closures:

```rust
// At the top of a file under tests/ (each file there is its own binary):
use std::hint::black_box;

orchestra_test_support::install_counting_allocator!();

#[test]
fn comparison_never_allocates() {
    let a = black_box(3_u64);
    let b = black_box(4_u64);
    // Fails the test if the closure allocates, reallocates or frees on this thread.
    let less = orchestra_test_support::assert_no_allocations(|| a < b);
    assert!(less);
}
```

`count_allocations` returns the result together with an `AllocationStats` with four counters:
allocations, reallocations, deallocations and bytes requested. For example, from
`tests/counting.rs`:

```rust
let ((), stats) = count_allocations(|| {
    let v: Vec<u32> = Vec::with_capacity(16);
    drop(black_box(v));
})
.unwrap();
// 1 allocation of 16 × 4 bytes, 1 deallocation.
assert_eq!(stats.bytes_allocated, 64);
```

Three details that matter:

- **Counters are per thread.** Tests run in parallel: with a single global counter, a test that
  checks "this does not allocate" would fail at random whenever another test allocates at the
  same moment. Each thread has its own counters (a `thread_local!` with `const` initialisation and
  no destructor, so reading it never allocates, otherwise the allocator would recurse forever).
  This is also what the audio-thread test of Phase 3 will need.
- **Freeing counts too.** `assert_no_allocations` fails on allocations, reallocations *and*
  deallocations: `free` can take a lock inside the system allocator, so the audio thread must not
  call it either.
- **No vacuous passes.** If a test file forgets `install_counting_allocator!()`, every counter
  stays at zero and "does not allocate" would always pass. So `count_allocations` first makes a
  tiny probe allocation and checks that the counter moved; if not, it returns
  `NotInstalledError` (and `assert_no_allocations` fails the test). `tests/not_installed.rs` is a
  test binary without the allocator that checks exactly this.

It is the fourth crate allowed to contain `unsafe`, because the `GlobalAlloc` trait is `unsafe`
by definition; the only unsafe operations are the forwarding calls to the system allocator, each
with its `// SAFETY:` comment.

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
on the same line. When our own widgets exist (ADR-0012), the plan is a stronger, type-level
guarantee: their label APIs will accept only message keys, not arbitrary strings.

## Automation

### cargo xtask

`xtask/` is a small Rust program with no dependencies, run through the alias described above:

```sh
cargo xtask setup        # use the versioned git hooks (once per clone)
cargo xtask check        # everything CI checks
cargo xtask guides       # build dist/GUIDA.pdf and dist/GUIDE.pdf
cargo xtask screenshots  # not implemented yet (needs the UI layer, ADR-0012)
cargo xtask bench        # instruction-count benchmarks (Linux; compile-only elsewhere)
```

`check` runs, in this order: `cargo fmt --check`, the documentation and guide parity checks,
`cargo clippy -D warnings`, `cargo nextest` (which includes the translation parity tests), the
doctests (nextest does not run them), `cargo deny check` and finally the **test-only crates
check**. It stops at the first failure.

The last step makes sure that `orchestra-test-support` is never a normal or build dependency of
any crate. It runs `cargo tree --invert orchestra-test-support --edges normal,build --target all`:
the output lists the crate and everything that depends on it through normal or build edges (dev
edges are excluded), on every target platform. For a correct workspace the output is exactly one
line, the crate itself; every further line is an offender, and the error names it.

The parity checks live in `xtask/src/parity.rs`. For the two developer documents they compute the
same structural signature as the Stop hook: the sequence of heading levels plus the exact content
of code blocks. For the guides they also require the same files in `docs/guide/it/` and
`docs/guide/en/`, and in each pair of Typst files the same headings, `#include`s and screenshots.
On failure they point at the first difference.

`screenshots` deliberately fails with an explanation instead of doing nothing, so nobody
mistakes it for a working step.

### Benchmarks and property tests

The tools were chosen in ADR-0016 after a comparison (versions, licenses, dependency counts and
build times are in the ADR).

**Benchmarks: Gungraun.** `cargo xtask bench` runs `cargo bench --workspace --bench '*'` (only the
`[[bench]]` targets, not the empty default harness of every library). Each benchmark file has
`harness = false` in its `Cargo.toml` and ends with Gungraun's `main!`, which hands the work to the
`gungraun-runner` program: it runs every benchmark function **once** under Valgrind's Callgrind
and counts the instructions executed. The count is the same on every run of the same binary,
which is why ADR-0007 prefers it to times in seconds (noisy on shared CI machines). Valgrind only
exists on Linux: on Windows and macOS `cargo xtask bench` only compiles the benchmarks
(`--no-run`), so they cannot break there unnoticed.

What you need to install, once, on Linux:

```sh
sudo pacman -S valgrind                                       # Arch / CachyOS
cargo install gungraun-runner --version 0.20.0 --locked      # same version as gungraun
cargo xtask bench
```

The runner must have **exactly** the version of the `gungraun` library, which is why Cargo.toml
pins it with `=0.20.0`: an innocent `cargo update` must not move it. When it is bumped, the runner
is reinstalled with the same version and `GUNGRAUN_RUNNER_VERSION` in `ci.yml` follows.

The first benchmark (`crates/test-support/benches/counting.rs`) measures the counter itself and
proves that the whole chain works. A run prints, for each benchmark, the count and the
difference from the previous run (kept in `target/gungraun/`):

```text
counting::counting::snapshot
  Instructions:                                         15|15                   (No change)
counting::counting::allocate_and_free_64_bytes
  Instructions:                                        255|255                  (No change)
counting::counting::measure_empty_closure
  Instructions:                                        163|163                  (No change)
```

Two practical notes. Gungraun's `#[library_benchmark]` rejects doc comments on the benchmark
functions (they are `#[doc]` attributes), so those functions have plain `//` comments and the file
lifts the documentation lints for the code the macros generate. And Gungraun clears the
environment of what it runs, so Valgrind would write its temporary files to `/tmp`: where `/tmp`
is not writable (the Claude Code sandbox) xtask forwards `TMPDIR` through `GUNGRAUN_ENVS`.

**Property tests: proptest.** A property test states a rule that must hold for *every* input
(for example "allocating n buffers counts n allocations and their total size in bytes") and
proptest checks it on hundreds of random inputs. When it finds a failure it **shrinks** the input
to the simplest case that still fails, and saves its seed in a `proptest-regressions/` file next
to the test: those files are committed, so the case is re-run forever. It is used without its
default features (`fork` and `timeout` would bring in more crates we do not need).

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
  (`.rs`, `.toml`, `.ftl` under `crates/` or `xtask/`; the root `Cargo.toml`, `deny.toml`,
  `clippy.toml`, `rustfmt.toml`, `.cargo/config.toml`; the CI workflows, `scripts/*.sh` and
  `.githooks/` — everything these documents explain, except `Cargo.lock` and `spikes/`) and `DOCUMENTAZIONE.md` or
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

`.github/workflows/ci.yml` runs on every push to any branch, on demand (`workflow_dispatch`) and
**once a week on `main`**, on Monday at 06:00 UTC:

```yaml
schedule:
  - cron: "0 6 * * 1"
```

Why a weekly run if the code has not changed? Because the world around it changes: GitHub updates
its runner images, and the RustSec advisory database that `cargo deny` downloads gains new
entries. Without this run, a platform could turn red silently and we would only find out at the
next pull request. Two GitHub rules apply: the failure email goes to whoever last edited the
`cron` line, and scheduled runs are switched off after 60 days without activity in the repository
(GitHub sends an email first).

The jobs:

- **check**, on `ubuntu-latest`, `windows-latest` and `macos-latest`: installs Rust 1.99.1 (the
  same version as `rust-version`), restores the build cache, installs prebuilt `cargo-nextest` and
  `cargo-deny`, and runs `cargo xtask check`. The three platforms run independently
  (`fail-fast: false`), so a Windows failure does not hide a macOS one;
- **guides**, on Linux: installs Typst 0.15.1, runs `cargo xtask guides` and uploads
  `GUIDA.pdf` and `GUIDE.pdf` as the `user-guides` artifact, kept for 30 days;
- **bench**, on Linux: installs Valgrind (`apt-get`) and the prebuilt `gungraun-runner` of the
  pinned version, runs `cargo xtask bench`, writes the numbers into the run's summary page and
  uploads them (with one `summary.json` per benchmark) as the `benchmarks` artifact. The numbers
  are not enforced yet: comparing a pull request against `main` with thresholds comes in F1.03.
  On Windows and macOS the **check** job runs `cargo xtask bench` too, which there only compiles
  the benchmarks.

Only the standard GitHub-hosted runners are used: they are free for public repositories, while
larger runners are billed even there. Every third-party action is pinned to a full commit hash,
with the tag in a comment, because a tag can be moved by whoever controls the action's repository:

```yaml
- uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
```

The workflow only has read access to the repository, and a newer push to the same branch cancels
the older run. The event name is part of the cancellation group
(`ci-${{ github.ref }}-${{ github.event_name }}`), so a merge into `main` and the weekly run never
cancel each other.

## The user guides

The user guides are written in Typst. `docs/guide/template.typ` defines the whole look once: an
ink-blue cover with the Orchestra logo in white, running headers, chapter openings
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
until our UI layer exists, chapters use `screenshot-placeholder`, which has the same frame
and caption as a real screenshot. Typst's own words ("Figure", "Contents") follow the `lang`
setting, so the template contains no text in any language.

`cargo xtask guides` compiles both PDFs into `dist/` with `--ignore-system-fonts`, so only the
fonts in `assets/fonts/` are used and the output is identical on every machine. The PDFs are never
committed; CI publishes them as artifacts.

### The logo and the themes

`assets/logo/orchestra-logo.svg` is the only logo of Orchestra (ADR-0014): its shape never
changes, only its colour. The owner's original drawing kept the wordmark as live text in the
Alte Haas Grotesk font, which would be replaced by another font on any machine without it, so the
repository version was made from the owner's own PDF export with `pdftocairo -svg`, which turns
the glyphs into outlines; rendered at 1000 × 1000 px it differs from the original in 0 pixels.
Everything in the file is filled with one colour, `#000000`, and recolouring means replacing that
string before rendering. The guide template does exactly this:

```typ
#let logo(colour, width: 100%) = {
  let svg = read("/assets/logo/orchestra-logo.svg").replace("#000000", colour.to-hex())
  image(bytes(svg), format: "svg", width: width)
}
```

The same ADR fixes that the program has a light and a dark theme from its first widget: widgets
and renderers never use literal colours, only semantic tokens of the active theme; the default
follows the operating system, and the score can either follow the theme or stay on white paper.

Unlike the code, the logo is **not** under the GPL: all rights reserved (owner's decision). The
repository README says so, and the logo lives only in `assets/logo/` so that a distribution that
requires fully free artwork can replace it without touching the code.

### Bundled fonts

`assets/fonts/` holds four families, all under the SIL Open Font License 1.1 (verified through the
GitHub API, recorded with versions and sources in `assets/fonts/README.md`): Source Serif 4 for
body text, Inter for headings, JetBrains Mono for code, and Bravura (with Bravura Text), the
reference font of the SMuFL music-font standard, which the notation engine will also use.

## Evaluation spikes

`spikes/` is a separate Cargo workspace with the time-boxed prototypes of Phase 0, Step 13. It is
excluded from the main workspace (`exclude = ["spikes"]` in the root `Cargo.toml`), never built
by `cargo xtask check` or CI, and never shipped: its job was to measure and decide. The decisions
and all the numbers are in ADR-0011 and ADR-0012; this section explains how to run the spikes
again.

### The GUI spike

`spikes/scene` is the common test scene: exactly 2000 SMuFL glyphs from Bravura on 10 staves, a
menu bar, one sentence per interface language in Inter, and a deterministic path of 120 views that
scroll and zoom. Each candidate (`gui-own`, `gui-iced`, `gui-egui`, `gui-slint`) draws it in a
`headless` mode (PNG without any window) and a `bench` mode (frame times on the CPU):

```sh
cd spikes
cargo build --release
./target/release/gui-own headless out/own.png
./target/release/gui-own bench
ICED_TEST_BACKEND=tiny-skia ICED_GLYPHS=paths ./target/release/gui-iced bench
```

Two lessons are worth keeping. First, music glyphs must be drawn as cached outlines, never as
"text" shaped every frame: in iced that alone made frames go from 243 ms to 29 ms. Second, every
toolkit needed our own score renderer anyway (egui cannot fill concave shapes, Slint has no
canvas), which is part of why ADR-0012 chose no toolkit at all. `gui-own/src/render.rs` is the
tiny-skia renderer the decision builds on: glyph outlines are parsed once with ttf-parser, cached
per character, and only transformed and filled at draw time.

### The plugin hosting spike

`spikes/plugin-host` is one executable with three roles: `scan`, `host` and `bridge`. The host
starts one bridge process per plugin, shares a memory-mapped file with it for the audio, and
triggers each block with one byte on the bridge's stdin; the bridge answers with one byte on a
private copy of its stdout (plugins print on stdout, so fd 1 is redirected to stderr). The host
measures the round trip, kills the bridge in the middle of the stream, checks that it survives and
restarts it. A `gui` mode opens the plugin editor in a separate X11 window owned by the bridge.

```sh
cd spikes
./target/release/plugin-host scan
./target/release/plugin-host host lv2 "Compressor Stereo"
./target/release/plugin-host gui vst3 "ZL Equalizer 2" 60
```

The spike used truce-rack inside the bridge; ADR-0011 decided to write our own host layer on the
same low-level bindings instead, keeping the bridge architecture that the measurements validated.

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
