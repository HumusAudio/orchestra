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
1 is filling them from the bottom: `orchestra-test-support` and, in `orchestra-core-time`, the
rational type, the position and duration types, the meter map and the units of real time exist
so far. The
dependency rules are summarised in `docs/ARCHITECTURE.md`; here is what each crate is for.

### orchestra-core-time

Exact musical time: positions and durations as fractions of a whole note, the single tempo map,
conversions to seconds, samples and video frames (ADR-0005). The conversion functions will be
called by the audio thread, so they must not allocate. No dependencies inside the workspace (in
`[dev-dependencies]` only `orchestra-test-support` and `proptest`, for the tests). So far it
contains the rational type (step F1.03), the position and duration types (F1.04) and the meter
map with beat groupings (F1.04b), the units of real time (F1.05: tempo, seconds, samples,
frames) and the tempo map with gradual changes and exact sample and frame indices (F1.05b).

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

The benchmarks (`benches/rational.rs`, step F1.03b) and their first numbers are described in
"Benchmarks and property tests" below.

#### Positions and durations

`src/position.rs` adds the two **domain types** of ADR-0018 §1, re-exported at the crate root:
`Position` (a point on the written timeline, in whole notes from the start of bar 0, which is the
pickup bar when there is one) and `Duration` (the signed distance between two positions). Both
are a `Rational` in disguise: same 16 bytes (`#[repr(transparent)]`), same cost. What they add is
**which operations make sense**:

```rust
use orchestra_core_time::{Duration, Position, Rational};

let quarter = Duration::from_rational(Rational::new(1, 4)?);
let beat_two = Position::ZERO.checked_add(quarter)?; // position + duration = position
let gap = beat_two.checked_duration_since(Position::ZERO)?; // position − position = duration
let dotted = quarter.checked_mul(Rational::new(3, 2)?)?; // duration × number = duration
let per_bar = Duration::from_rational(Rational::new(6, 8)?).checked_ratio(dotted)?; // 2
```

Adding two positions or multiplying a position does not compile: the module documentation has
two `compile_fail` doctests that prove it. With plain `Rational` everywhere, "bar start + note
position" by mistake would compile and land one bar too late; here the compiler stops it.

Some details:

- **Signs.** Both types accept negative values. A position before zero is a point before the
  score (the meter map will reject it, but a first note played early by a microtiming offset
  sounds there); a duration is negative when measured backwards. "A note lasts more than zero" is
  a rule of the model, not of the type.
- **The name `checked_duration_since`** follows `std::time::Instant::duration_since`: the
  receiver is the later position, the argument the earlier one.
- **`Duration::checked_ratio`** answers "how many times does this fit in that": the units
  cancel, so it returns a plain `Rational`. Its `floor` will tell the meter map which bar of a
  run of equal bars contains a position, so no remainder operation is needed on `Rational`.
- **Ways in and out:** `from_rational` and `as_rational` are explicit on purpose. The tempo map
  and the file format will use them, and seeing them in a diff means "a value is leaving (or
  entering) the type safety here".
- Every method is a `const fn` forwarding to the `Rational` operation, with the same errors.
  Negation calls `Rational`'s crate-private `negated` instead of the `-` operator, because
  clippy's `arithmetic_side_effects` cannot tell `-rational` (infallible) from an integer
  negation that might overflow.
- `Debug` prints `Position(3/8)` and `Duration(-1/4)`; no `Display` until F1.15.

The tests are in two files. `tests/position.rs` checks with proptest that each method returns
exactly what its `Rational` operation returns (errors included), then the algebra the types
exist for (`(p + d) − p = d`, `a − b = −(b − a)`, ratio × unit = duration), then hand-written
musical cases. A bug injected on purpose (`checked_sub` adding instead of subtracting) was caught
by two properties at once. `tests/position_no_alloc.rs` checks with the counting allocator that
nothing allocates, on success and error paths.

#### Meters and beat groupings

`src/meter.rs` holds two small types that answer two different questions about a bar
(ADR-0018 §2, ADR-0019):

- **`Meter`**: how the bar is *written*, the time signature printed in the score. From 1 to 8
  numerator groups and a denominator: 4/4 is `[4]/4`, the additive 3+2+2/8 is `[3, 2, 2]/8`. Any
  denominator from 1 to 65535 is accepted, not only powers of two (decision D04): 4/3 lasts
  four triplet half notes.
- **`BeatGrouping`**: how the bar is *counted*, if the default rule is not what the user wants.
  From 1 to 16 beats, each of `group/unit` whole notes, with a unit of its own:

```rust
use orchestra_core_time::{BeatGrouping, Meter, MeterChange, MeterMap};

let nine_eight = Meter::new(&[9], 8)?; // written "9/8"
let mixed = Meter::new(&[3, 2, 2], 8)?; // written "3+2+2/8"
// 9/8 still written "9/8", but counted 3+2+2+2 for the whole section:
let counted = BeatGrouping::new(&[3, 2, 2, 2], 8)?;
let map = MeterMap::new(MeterChange::with_grouping(nine_eight, counted))?;
// A 3/4 bar counted as two dotted quarters (hemiola): the grouping has its own unit.
let hemiola = BeatGrouping::new(&[3, 3], 8)?;
```

Both keep their groups in a fixed array (8 or 16 slots) plus a length, so they are `Copy` and
never allocate. The unused slots are always zero: that is what makes the derived equality and
hash correct. Invalid inputs (no groups, too many, a zero group, a zero denominator) are a
`MeterError`. `Debug` writes `Meter(3+2+2/8)` and `BeatGrouping(3+2+2+2/8)`.

The **default counting**, used when no grouping is given, lives in the meter map: an additive
meter has one beat per written group; a single numerator that is a multiple of 3 greater than 3
is compound (6/8 in two dotted quarters, 12/8 in four); anything else is simple, one beat per
unit. So 7/8 counts seven eighths: the owner chose not to guess 2+2+3 or 3+2+2, and the
interface will ask which grouping to use when a mixed meter is entered (ADR-0019, Consequences).

#### The meter map

`src/meter_map.rs` turns bars and beats into positions and back. It keeps the **source data**
that the document stores and the commands edit, in lists sorted by bar:

- meter changes (`MeterChange`: a meter and an optional grouping for its section), the first
  always at bar 0;
- irregular bars (`IrregularBar`: an explicit length and a `BarKind`): `AlignStart` (the start of
  a nominal bar, e.g. the incomplete bar before a repeat, or a bar longer than its meter),
  `AlignEnd` (the end of a nominal bar: the pickup), `Free` (meter suspended: a free cadenza);
- per-bar groupings, which win over the section's;
- the numbering rules: uncounted bars and restarts.

From these it derives **segments**, runs of bars with the same meter, length and counting, each
with its first bar and its start position already summed up:

```text
bars:      0      1    2    3    4    5   …      (bar 0 a 1/4 pickup, 3/4 from bar 3)
segments: [pickup][ 4/4 × 2 ][ 3/4 forever  …
starts:    0      1/4        9/4
```

So every lookup is a binary search (`partition_point`) followed by one or two exact divisions:
`locate` (position → bar and offset), `bar_start`, `position_of`, `beat_of` (point → beat, or
`None` in a free bar), `beat_position` (the inverse), `beat_range` (the beats that start inside a
bar, for the metronome), `display_number`. None of them allocates (`tests/meter_map_no_alloc.rs`).
The last segment continues forever: where the score ends is the model's business.

Some rules worth knowing:

- **Beat numbers are 0-based** and counted from the virtual start of the nominal bar. A
  quarter-note pickup in 4/4 is beat 3 (the musician's "beat 4"), and `beat_range` gives `3..4`;
  an eighth pickup gives the empty range `4..4`, because the only beat it touches began before it.
- **A bar longer than its meter** repeats the pattern and keeps counting (owner's option "a"):
  5/4 in 4/4 has beats `0..5`; 3+2+2/8 stretched to 9/8 has a fourth beat of 3/8 cut to 2/8 by the
  barline.
- **Free bars** have no beats, but the meter stays recorded and resumes at the next regular bar;
  a meter change placed on a free bar is "the meter after the cadenza". They are counted in the
  numbering unless marked uncounted.
- **Numbering:** bar 0 is number 0 if it is a pickup, otherwise 1; uncounted bars have no number
  and do not advance the count; a restart sets the number of its bar.

**Editing is all or nothing.** Each edit (`set_meter_change`, `set_irregular_bar`,
`set_bar_grouping` and their `remove_*`) works on a copy of the source data, rebuilds the
segments and validates everything; only then does it replace the map:

```rust
fn edit<T>(&mut self, change: impl FnOnce(&mut Sources) -> T) -> Result<T, MeterMapError> {
    let mut sources = self.sources.clone();
    let result = change(&mut sources);
    let segments = build_segments(&sources)?;
    self.sources = sources;
    self.segments = segments;
    Ok(result)
}
```

So an edit that would leave some bar inconsistent (a pickup now longer than a new meter, a
per-bar grouping that no longer adds up) returns a typed `MeterMapError` and the map is exactly
as before, as the commands of ADR-0003 need. Every edit returns the previous value, which is what
an undo will store. The rebuild costs O(n log n) in the number of source entries and allocates,
which is fine on the editing path.

**Tests.** `tests/meter_map_cases.rs` has 20 hand-written cases, one per item of the SCALETTA
list (pickup, meter changes, compound, additive, 4/3, free cadenza over one and several bars,
hemiola, groupings that do not add up, numbering, errors). `tests/meter_map_oracle.rs` builds maps
from random sequences of edits and compares every bar and random points with a **naive oracle**
that walks bar by bar from the source data, with linear scans and explicit beat lists, and checks
that failed edits leave the map unchanged. Two bugs injected on purpose (compound from 3 instead
of from 6; summing one bar per segment instead of all of them) were caught by both files.

**Benchmarks** (`benches/meter_map.rs`) use a reference map of 256 bars with about 150 segments,
far more than a real score, and batches of 64 lookups; the map is built in Gungraun setup
functions, which are not measured. First numbers (development machine, 2026-10-31):

```text
locate            61609 instructions / 64 lookups ≈  960 each
bar_start         29850                            ≈  470
beat_of           59567                            ≈  930
beat_position     72693                            ≈ 1140
display_number     6557                            ≈  100
edit_meter_change 97488 for one edit (clone, rebuild, validation)
```

Most of the cost is the exact rational arithmetic (the gcds), not the search. A lookup of about a
thousand instructions is well under a microsecond on the reference hardware.

#### Inserting and deleting bars

The commands of F1.11b insert and delete bars (ADR-0022 §8, ADR-0026 §1), so the meter map has
two operations that are **exact inverses** of each other. `insert_bars(at, &span)` inserts the
bars of a `BarSpan` before bar `at` and moves every source entry from `at` on later by that many
bars; `remove_bars(at, count)` takes `count` bars out, returns what they carried (meter changes,
pickups, groupings, uncounted bars, numbering restarts, at indices relative to the span) and
moves the later entries back. Inserting the returned span gives back the very same map, which is
what an undo needs.

```rust
let mut map = MeterMap::new(MeterChange::new(Meter::new(&[4], 4)?))?;
map.set_meter_change(BarIndex::new(4), MeterChange::new(Meter::new(&[3], 4)?))?;
// Two empty 4/4 bars before bar 2: the 3/4 moves to bar 6, still on the same written bar.
map.insert_bars(BarIndex::new(2), &BarSpan::empty(2))?;
// Taking them out again returns what they carried (nothing) and gives back the same map.
let span = map.remove_bars(BarIndex::new(2), 2)?;
assert!(span.has_no_entries());
```

The new bars are exactly what the span says: with `BarSpan::empty(n)` they follow the meter in force
just before `at`, and an entry that was at `at` moves with its bar. Inserting at bar 0 moves the meter
change of bar 0 as well, so the span must then bring its own (the map always needs one at bar 0);
removing bars may drop the meter change that was in force after them. Deciding what the remaining
music should keep is the document command's job (ADR-0026 §6), not the map's. Both operations are
all or nothing and cost O(n log n) in the source entries, whatever the number of bars.

**Tests** (`tests/meter_map_bars.rs`): eight hand-written cases (new bars follow the meter in
force, an entry at the insertion point moves with its bar, inserting at the start needs a meter,
an entry outside its span is refused, removed entries leave with their bars, removing the start
needs a meter after it, past the last bar, zero bars) and two property tests against an oracle that
moves the entries one by one in `BTreeMap`s, checking that the bars before the edit point do not
move and that each operation undoes the other. Two injected bugs (an entry at the insertion point
left behind; the entries after a removal not moved back) were caught. **Benchmarks** (same file as
the lookups): inserting four bars in the middle of the reference map ≈ 95 000 instructions, removing
four ≈ 94 700, about one meter change.

#### Replacing a run of bars at once

The meter change that rewrites the music (F1.11c part 2, `Rewrite`) changes the lengths, kinds and
groupings of many bars together. One edit per bar would rebuild the map once per bar and could pass
through states the map refuses (a pickup briefly longer than a meter about to change). Since F1.11c
part 2a (ADR-0026 §11 Z1) `replace_span(at, &span)` replaces the source data of the `span.count()`
bars from `at` on with the span's, all at once, and returns the data they had: putting that back is
the exact inverse. The number of bars does not change. `BarSpan` gained the setters it lacked
(`set_irregular_bar`, `set_bar_grouping`, `set_uncounted`, `set_numbering_restart`), so a command
can build the new data of a run.

```rust
// 60 bars from bar 100 rewritten at once: a 3/4 on the first, a bar of a half as the last.
let mut span = BarSpan::empty(60);
span.set_meter_change(BarIndex::new(0), MeterChange::new(Meter::new(&[3], 4)?));
span.set_irregular_bar(
    BarIndex::new(59),
    IrregularBar { length: half, kind: BarKind::AlignStart },
);
let old = map.replace_span(BarIndex::new(100), &span)?;
// Putting the old data back is the exact inverse.
map.replace_span(BarIndex::new(100), &old)?;
```

There is no such operation for the tempo map (Z2): it is derived from the tempo marks once, at the
end of every transaction (ADR-0024 §11), which is already "all at once".

**Tests** (same file): two hand-written cases (a run replaced and put back; entries outside the span
and a run at bar 0 without a meter refused) and a property test against the oracle, with the span
taken from another random map; an injected bug (span entries moved to the wrong bar) was caught.
**Benchmark:** replacing 60 bars in the middle of the reference map ≈ 77 000 instructions, less than
one meter change, whatever the length of the run.

#### Tempo, seconds, samples and frames

`src/tempo.rs` and `src/clock.rs` hold the units of real time that the tempo map (step F1.05b)
uses (ADR-0020 §2, §5, §7):

- **`Tempo`**: a beat unit (a `Duration`: ♩ = 1/4, ♩. = 3/8) and a rational number of beats per
  minute (120, or `185/2` for 92.5). What matters for the sound is its **speed**,
  `unit × bpm / 60` whole notes per second, so ♩ = 120 and ♩. = 80 differ as written but go
  equally fast. At a constant tempo `seconds_for` and `duration_in` convert exactly in both
  directions.
- **`Seconds`**: an exact time, a newtype over `Rational`; `to_f64` exists for display only.
- **`SampleRate`**: a whole number of Hz, never zero. `sample_at(t)` gives the **nearest**
  sample, `⌊t·rate + 1/2⌋`, ties to the later one; `time_of_sample(n)` is exactly `n/rate`.
- **`FrameRate`**: a positive rational (24, 25, 30000/1001). `frame_at(t)` gives the frame that
  **contains** `t`, `⌊t·fps⌋`; `frame_start(k)` is exactly `k/fps`.

```rust
use orchestra_core_time::{Duration, FrameRate, Rational, SampleRate, Seconds, Tempo};

let rate = SampleRate::new(48_000)?;
let half_sample = Seconds::from_rational(Rational::new(1, 96_000)?);
assert_eq!(rate.sample_at(half_sample)?, 1); // a tie goes to the later sample
let ntsc = FrameRate::new(Rational::new(30_000, 1001)?)?;
assert_eq!(ntsc.frame_at(Seconds::from_rational(Rational::ONE))?, 29); // the frame containing 1 s
let quarter_120 = Tempo::new(Duration::from_rational(Rational::new(1, 4)?), Rational::from_integer(120)?)?;
let dotted_80 = Tempo::new(Duration::from_rational(Rational::new(3, 8)?), Rational::from_integer(80)?)?;
assert_eq!(quarter_120.speed()?, dotted_80.speed()?); // both 1/2 whole note per second
```

Why two different rules: a sample is an instant, so the nearest one is the right answer and the
error is at most half a sample; a frame is an interval of time shown on screen, so a time belongs
to the frame that contains it. Everything is exact rational arithmetic, so every platform gives
the same index. Indices are `i64` and may be negative (pre-roll).

Tests: `tests/clock.rs` checks the rounding rules **by their definition** with proptest (for
instance `(n − 1/2)/rate ≤ t < (n + 1/2)/rate` whenever `sample_at(t) = n`), that the inverses
land on their own index, that constant-tempo conversions round-trip and that ♩ = x has the speed
of ♩. = 2x/3; plus hand-written ties, negative times, one hour of NTSC and the errors. A bug
injected on purpose (rounding down instead of to the nearest sample) failed two tests.
`tests/clock_no_alloc.rs` checks that no conversion allocates.

#### The tempo map

`src/tempo_map.rs` turns written positions into seconds and back (ADR-0020, amended by
ADR-0021). Its **source data** is a list of `TempoChange`s sorted by position, at most one per
position, the first always an immediate change at position 0 (the initial tempo):

- `TempoChange::Immediate(tempo)`: from here on, this tempo;
- `TempoChange::Gradual(GradualChange { from, to, end })`: from here to `end` the speed moves
  linearly **in time** (constant acceleration, ADR-0020 §1) to `to`, which then holds. `from` is
  an optional starting tempo for "Meno mosso, poco a poco accel." (jump, then change); without it
  the change starts from the tempo in force.

A change strictly inside a gradual change is an error, but an immediate change exactly at its end
("rit. … a tempo") and two gradual changes back to back are fine. From the source data the map
derives **segments**, constant or gradual; after a gradual change that is not followed at once by
another change, an implicit constant segment holds its target tempo:

```text
changes:   0: ♩=120        8: rit. to ♩=60 until 12        16: ♩=96
segments: [ constant 120 ][ gradual 120 → 60 ][ constant 60 ][ constant 96 … forever
positions: 0               8                   12             16
```

The first segment also covers negative positions and times (the first tempo extended backwards);
the last continues forever.

**Start times on a grid.** Exact cumulated times would overflow any fixed-width fraction after
a few dozen gradual changes (the measurements are in ADR-0021). So each segment starts at the
previous start plus the **exact** duration of the previous segment, rounded to the nearest tick
of `TIME_GRID` = 705 600 000 ticks per second (the "flick", which every common sample rate and
frame rate divides). Since the previous start is already on the grid, only the duration needs
rounding, and start times are kept as plain integer ticks:

```rust
// `t_k` is on the grid, so `round(t_k + D) = t_k + round(D)`: integers only.
ticks = ticks
    .checked_add(rational_ticks(duration)?)
    .ok_or(OVERFLOW)?;
```

The error is at most half a tick (≈ 0.7 ns) per boundary and the same on every platform. Since a
segment then spans `[t_k, t_{k+1})`, which may differ from its exact duration by half a tick,
both conversions **clamp** to the segment, so that they never go backwards across a boundary.

**The two conversions** are a binary search on the segment starts (positions or times) followed
by O(1) arithmetic, without allocating:

- `time_at(position)` returns a `TimeAt`: `Exact(Seconds)` in constant segments and at the start
  of every segment, `Approximate(f64)` inside a gradual change, where the time is irrational. It
  is computed there as `τ(x) = 2x / (s₀ + √(s₀² + 4h·x))`, with `h = (s₁² − s₀²)/(4L)` half the
  acceleration: this form avoids the cancellation of the textbook one when `h` is small.
- `position_at(seconds)` is exact in constant segments; inside a gradual change it computes
  `s₀·τ + h·τ²` exactly with `i128` intermediates (the exact value rarely fits in an `i64`
  fraction) and rounds it to the nearest `1/TIME_GRID` of a whole note, about 2.8 ns at ♩ = 120.

```rust
let mut map = TempoMap::new(tempo(120)?)?;
let rit = GradualChange { from: None, to: tempo(60)?, end: at(6, 1)? };
map.set_change(at(2, 1)?, TempoChange::Gradual(rit))?;
assert_eq!(map.time_at(at(6, 1)?)?, TimeAt::Exact(secs(44, 3)?)); // 4 s + 32/3 s
assert!(matches!(map.time_at(at(4, 1)?)?, TimeAt::Approximate(_)));
```

The rounding to the grid splits off the integer part first, so that the product with the grid
stays small; for a time whose denominator does not divide the grid (say `n/3001` s) that product
can still overflow, and the fraction is then reduced by its greatest common divisor and the
product retried once. What remains is a typed overflow error: in the tests it happened for 1.3%
of such artificial times and never for sample, frame or decimal times.

**Editing** works as in the meter map: `set_change` and `remove_change` change a copy of the
source data, rebuild the segments and validate everything, replace the map only on success
(`TempoMapError`: `BeforeStart`, `NoTempoAtStart`, `GradualAtStart`, `NonPositiveGradual`,
`ChangeInsideGradual`, `Arithmetic`) and return the previous value for undo.

**Building in one go** (F1.09c, the bulk edit of ADR-0022 §8). Since the document derives its tempo
map from the tempo marks after every edit that touches them, `TempoMap::from_changes` builds a map
from the whole list of changes at once: it checks that the positions strictly increase
(`TempoMapError::Unsorted` otherwise), then applies exactly the rules of the edits. A property test
rebuilds every randomly edited map from its own list of changes and finds it identical.

```rust
// The document rebuilds its tempo map in one go from the changes derived from the tempo marks:
// the same map as one `set_change` per change, in O(t) instead of O(t²).
let map = TempoMap::from_changes(vec![
    (Position::ZERO, TempoChange::Immediate(andante)),
    (at(4, 1)?, TempoChange::Gradual(GradualChange { from: None, to: lento, end: at(5, 1)? })),
])?;
```

**Tests.** `tests/tempo_map_cases.rs` has 12 hand-computed cases: the ritardando of ADR-0020's
table (32/3 s), positions inside a change on and off the grid, boundaries rounded down (♩ = 77)
and up (♩ = 71) with their clamping, the optional starting tempo, back-to-back changes, the
errors with the map left unchanged, previous values, negative positions, monotonicity.
`tests/tempo_map_oracle.rs` compares random maps with an **oracle** that shares no code with the
library: its own `i128` fractions, its own segments rebuilt by a linear walk, linear scans, the
trapezoid form `τ·(s₀ + s(τ))/2` and another integral for the approximate times; it also checks
that every boundary is within half a tick per earlier boundary of the exact cumulated time.
Three bugs injected on purpose (no clamping, a wrong `h`, ties rounded down) were all caught; the
oracle also found the overflow on foreign denominators that the gcd fallback now handles.
`tests/tempo_map_no_alloc.rs` checks that no lookup allocates, the slow path included.

**Sample and frame indices.** `sample_at(position, rate)` gives the nearest sample and
`frame_at(position, fps)` the frame that contains the time, as an `IndexAt`: `Exact(n)` or, only if
the exact check overflows, `Estimated(n)` (the same idea as `TimeAt`: an approximation is never
hidden; the audio thread just calls `.value()`). In constant segments they round the exact time.
Inside a gradual change the time is irrational, but the index is not: index `n` covers a time
interval with rational ends, and the **position** reached at a rational time is exact. Since the
position grows with time, "the point sounds during index `n`" is the same as "the point lies
between the positions reached when index `n` begins and when it ends": two exact comparisons in
`i128`. The `f64` time gives the first guess, which the comparisons confirm or move by one, at
most four times (a fixed bound, for the audio thread):

```rust
for _ in 0..=MAX_CORRECTIONS {
    match segment.locate_in_index(x, n, grid, values) {
        Some(Ordering::Equal) => return Ok(IndexAt::Exact(n.min(cap))),
        Some(Ordering::Less) => n = n.checked_sub(1).ok_or(OVERFLOW)?,
        Some(Ordering::Greater) => n = n.checked_add(1).ok_or(OVERFLOW)?,
        None => break,
    }
}
Ok(IndexAt::Estimated(estimate.min(cap)))
```

`cap` is the index of the next segment's start: `time_at` clamps there, and the index has to agree
with it. Samples and frames share the code through a small `IndexGrid` (index `n` covers
`[(n − offset)/rate, (n + 1 − offset)/rate)`, with `offset` 1/2 for samples and 0 for frames).
Outside the change the comparison uses the start or end position, never the polynomial, which
for a slowing change would turn back. `position_of_sample(n, rate)` and `position_of_frame(k, fps)`
go back, through `position_at`.

**How often is the estimate needed?** `tests/tempo_map_fallback.rs` builds an unfriendly score of
2 h 16 min with 750 changes, 500 of them gradual, metronome marks with large prime factors and
several beat units, and asks for 12 834 points at 44.1, 48, 96, 192 and 384 kHz (384 kHz does not
divide the grid) and at 24, 25 and 30000/1001 fps: **no estimate at all**. The test fails if any
rate that divides the grid ever needs one.

The index tests: 5 more hand-computed cases (constant tempo, ties, a ritardando against a
hand-computed time, every sample of a change mapping back to itself, the clamping), a property
test that checks every index **by its definition** with the oracle's own fractions, plus
monotonicity and the round trip through `position_of_sample`. Five bugs injected on purpose into
the index code were caught; two of them only after adding two targeted cases (an index that
crosses the clamp, visible only with intervals shorter than a tick; a change shorter than a video
frame, where the polynomial turns back).

**Benchmarks** (`benches/tempo_map.rs`) use a reference map of 150 changes, two thirds gradual,
about 250 segments, with batches of 64 lookups. First numbers (development machine, 2026-11-01):

```text
time_at              57893 instructions / 64 lookups ≈  900 each
position_at         129175                            ≈ 2020
sample_at_constant  148272                            ≈ 2320
sample_at_gradual   288218                            ≈ 4500
frame_at            215446                            ≈ 3370
edit_tempo_change   572602 for one edit (clone, rebuild of every segment)
```

As in the meter map, most of the cost is the exact rational arithmetic (the gcds of large
denominators, since most boundaries are off the grid), not the search. 4500 instructions are
about 1.5 µs on the reference hardware; one edit is about 0.2 ms, far within the 16 ms of an
interface update.

### orchestra-core-model

The one project document (ADR-0003): tracks, notes with their two layers (written intent and
performance offsets, ADR-0004), the form of the piece, commands and the single undo/redo history.
It knows nothing about views, layout, audio or files. Its architecture (stable IDs, events as
bar-relative offsets in per-bar chunks, voices per instrument) is fixed by ADR-0022; the first
parts implemented are the pitch module, the skeleton of the document, the content of
the voices (events, tuplets, ties, clefs and key signatures) and the markings on the events with
the slurs.

#### Pitch, intervals and keys

`src/pitch.rs` (step F1.07, ADR-0023) holds the pure types every note's pitch is made of. A
`Pitch` is what a musician writes: a letter (`Step`, C to B), an `Alteration` and an octave. Two
pitches are equal only if they are **spelled** the same, because ADR-0004 keeps the written intent:
F♯4 and G♭4 are different notes that happen to sound alike, and `sounds_like` is the separate
question.

```rust
let b_sharp_3 = Pitch::new(Step::B, Alteration::SHARP, 3);
let c_4 = Pitch::new(Step::C, Alteration::NATURAL, 4);
assert_ne!(b_sharp_3, c_4); // spelled differently…
assert!(b_sharp_3.sounds_like(c_4)?); // …same sound
assert_eq!(c_4.midi()?.key, 60);
```

**The alteration is a rational number of semitones** (decision D07): 1 is a sharp, −1 a flat, 1/2
a quarter-tone sharp, 1/3 a sixth-tone sharp, up to a triple sharp or flat. That is how MusicXML
writes `<alter>`, so import and export are direct, and it fits the rest of the project, where
nothing is a float. Which glyph draws a quarter tone is the engraver's business (F2B); comma
accidentals of just intonation are not numbers of semitones and wait for tuning, which is a later
rule of `playback`.

**Octaves are scientific** (C4 is middle C) and change at C: B♯3 sounds like C4, C♭4 like B3. The
**sounding pitch** is exact, `12 × (octave + 1) + natural semitones + alteration` on the MIDI scale,
and `midi()` turns it into a `MidiNote`: the nearest key and a bend in semitones. A quarter tone
lies exactly between two keys; the rule takes the **lower** key and bends up by a half, so the
result is deterministic. A key outside 0–127 is a `PitchError::MidiOutOfRange`, never clipped.
Chromatic pitches take a fast path with plain integer sums.

An **`Interval`** is a pair (diatonic steps, semitones), both signed, the same pair MusicXML uses for
transposing instruments. Its **quality** is derived from the difference between its semitones and
those of the major or perfect interval with the same number: for unisons, fourths and fifths 0 is
perfect and ±n is n-times augmented or diminished; for the others 0 is major, −1 minor, +n
augmented, −(n+1) diminished. A microtonal interval has no standard quality and answers `None`.

```rust
let c4 = Pitch::new(Step::C, Alteration::NATURAL, 4);
let third = Interval::from_quality(Quality::Major, 3, false)?;
assert_eq!(c4.transposed(third)?, Pitch::new(Step::E, Alteration::NATURAL, 4));
// The B♭ clarinet sounds a major second lower than written:
let clarinet = Interval::from_quality(Quality::Major, 2, true)?;
let written = c4.transposed(clarinet.reversed())?; // D4
```

**Transposition** adds the steps to the pitch's place in the sequence of letters (which gives the
new letter and octave), then chooses the alteration that moves the sound by exactly the interval:
F♯4 up an augmented sixth is D𝄪5, never E5. A result beyond a triple sharp is an error.

**Keys** (`Key`, `TraditionalKey`, `Mode`) count sharps or flats in fifths, from −14 to +14 (keys
beyond seven, like D♯ major for a B♭ clarinet playing in C♯ major, come out of transposition), with
seven modes and "none". `Key` is an enum with one variant on purpose: free signatures (Bartók,
maqam…) were deferred by the owner and will be a second variant, at the MusicXML import (after
F2B) or at the first concrete need. The computations use the **line
of fifths** (… B♭ −2, F −1, C 0, G 1, D 2 …): a key of k fifths holds the seven places
`[k − 1, k + 5]`, so the alteration of each letter, the tonic of each mode and the transposition of a
key are one addition each. `Pitch::spell_midi` picks the spelling of a MIDI key inside a window of
twelve fifths, `[k − 4, k + 7]` (C major: C♯ E♭ F♯ A♭ B♭), shifted by one in minor so that the
leading tone is a sharp (A minor: G♯).

**Tests.** `tests/pitch_cases.rs` has 14 groups of hand-written cases a musician can check (B♯3 and
C♭4, quarter tones in MIDI, qualities from doubly diminished to doubly augmented, B♭ clarinet, horn
in F, piccolo, double bass, theoretical keys). `tests/pitch_oracle.rs` compares the library with an
oracle that shares no formula with it: natural notes by walking the major scale, keys by applying
one accidental at a time in the order of sharps, qualities from a textbook table, tonics as degrees
of the major scale; it also checks every MIDI key in every key and mode exhaustively. Five injected
bugs were caught; one of them (a wrong Lydian tonic) only after the tonic oracle was added.
`tests/pitch_no_alloc.rs` checks that nothing allocates.

**Benchmarks** (`benches/pitch.rs`, batches of 64, instructions per call): MIDI conversion ≈ 155 on a
mix with one quarter tone in eight (≈ 650 before the chromatic fast path), transposition ≈ 560,
interval and quality ≈ 800, spelling a MIDI key ≈ 100.

#### The document skeleton

`src/id.rs` and `src/document.rs` (step F1.08, ADR-0022 §1–3) hold the **structure** of the
document; the events inside the voices are described in the next section, the form arrives in
F1.10.

**IDs.** Every object that something else can point to has an ID: `TrackId`, `InstrumentId`,
`StaffId`, `VoiceId`, `BarId` (later steps add events, noteheads and spanners). They are distinct
types, so the compiler rejects a staff where a voice is expected, but underneath they are all
`u64` values from **one counter per document** that starts at 1 and only grows. An ID is never
handed out twice, not even after an undo, so selections, caches and redo steps can never confuse
two objects. The value is a `NonZeroU64`: 0 is never an ID, so an `Option<BarId>` takes 8 bytes
and not 16. Running out of IDs (impossible in practice: a billion objects per second for 584
years) is a typed error, never a wrap-around.

**Structure.** Document → tracks (only instrument tracks for now) → one `Instrument` per track,
with the name the user gave it (their own text, not a translation key), its transposition (the
written-to-sounding interval of ADR-0023 §7) and its **staves** (a staff is the whole set of lines
the notes are written on) and **voices**. A voice belongs to the instrument, not to a staff: it
has a *home staff*, and from F1.09 each note records the staff it is drawn on, so a melody moving
from the right hand to the left hand of a piano stays one voice.

**Bars.** The `BarList` is the ordered list of `BarId`s: it says which bar is which and how many
there are, and nothing else. Bar lengths, meters and numbers stay in the meter map, which the
document holds next to the tempo map: repeating them here would be the duplication ADR-0003
forbids. What is attached to a bar (from F1.09 the notes) is keyed by `BarId`, so inserting a bar
will move nothing else. An inverse index (`BarId` → position) answers in O(1) on average, with our
own hasher: one multiplication by 2⁶⁴/φ instead of the standard `SipHash`, which defends against
keys chosen by an attacker, while our keys come from our own counter.

**How a document is created.** With a `DocumentBuilder` (owner's decision, 2026-11-02): it is for
documents that do not exist yet (the file being opened, the reference score generator, tests and
benchmarks), while an existing document will change only through the commands of F1.11
(ADR-0003 §4). A new instrument gets **one voice per staff**, each at home on its own staff;
`add_voice` adds more on any staff of the same instrument. There is no default meter, tempo or key:
they are the caller's choice. Since F1.09b the builder takes the initial key (at concert pitch)
and one clef per staff instead of a number of staves, so a staff without clef or a score without
key cannot be built (owner's decision, 2026-11-02), and `build` returns a `Result`. Since F1.09c it
takes the initial **tempo mark** instead of a ready-made tempo map, and `new` returns a `Result`
too: the tempo map is derived from the marks (next sections).

```rust
let c_major = Key::traditional(0, Mode::Major)?;
let mut builder = DocumentBuilder::new(meter_map, initial_tempo_mark, c_major)?;
for _ in 0..8 {
    builder.add_bar()?;
}
// A piano: two staves with their clefs, so two voices, one at home on each staff.
let unison = Interval::new(0, Rational::ZERO);
let piano = builder.add_instrument_track("Piano", unison, &[Clef::TREBLE, Clef::BASS])?;
let upper = builder.document().track(piano).unwrap().instrument().staves()[0].id();
builder.add_voice(upper)?; // a second line in the right hand
let document = builder.build()?; // build() validates the document (next section)
assert_eq!(document.bars().len(), 8);
```

The builder takes the IDs in a documented order (a bar one; an instrument track the track's, the
instrument's, one per staff, one per voice), so the same calls always give the same document, IDs
included. Every method checks everything **before** creating anything: an error leaves the builder
as it was and consumes no ID.

**Limits** (owner's decision, 2026-11-02): at most **16 staves** and **256 voices** per instrument.
They are not musical limits (the interface may still offer four voices per staff, or any other
number) but technical ones: no real score gets close, and they stop a corrupt or malicious file
from asking for millions of voices when it is opened. Raising them changes a constant, not the file
format.

**Lookups.** Bars are many, so they have the hash index; tracks, staves and voices are few (a few
hundred in a large orchestral score) and are found by a linear scan, which is cheaper than keeping
another index up to date. The index from ID to location that ADR-0022 §1 describes is for events,
which are hundreds of thousands (F1.09).

**Tests.** `tests/document_cases.rs` has 12 hand-written cases (IDs in sequence across kinds, one
voice per staff, extra voices, transposing instruments, bar starts after a pickup, the limits at 16
and 256 with nothing consumed on refusal, unknown staves and bars, determinism, error texts);
`src/id.rs` and `src/document.rs` test the exhaustion of the counter from the inside.
`tests/document_oracle.rs` applies random sequences of builder calls to the library and to a naive
oracle written in the test (a plain counter and vectors of numbers), comparing the whole document
after every call, and checks that every ID is distinct and the bar index is the exact inverse of the
list. Four injected bugs (voices on the wrong staff, the voice limit off by one, the counter room
check removed, a wrong bar index) were all caught. `tests/document_no_alloc.rs` checks that no
lookup allocates.

**Benchmarks** (`benches/document.rs`, a skeleton the size of ADR-0022's heavy score: 1 000 bars,
20 instruments, 30 staves, 40 voices; batches of 64, instructions per call): `BarId` → index ≈ 42,
bar start ≈ 510 (index plus meter map search among 20 segments), voice by ID with its instrument ≈
280.

#### The content of the voices

`src/event.rs`, `src/content.rs`, `src/marks.rs` and `src/validate.rs` (step F1.09b, ADR-0024
§1–§4 and §9) fill the voices with music. The markings on the events, the performance offsets and
the slurs are in the next section, dynamics, hairpins and tempo marks in the one after it.

**Events.** A voice holds one **chunk** per bar, aligned with the bar list: a chunk is empty
(`None`, no memory used) or an `Arc` of a vector of events sorted by their **offset** from the
start of the bar (events at the same offset keep the order they were entered in). The `Arc` is for
snapshots (ADR-0022 §7): a background reader shares the chunks, and an edit copies only the chunk
it touches. An event is a `Chord` or a `Rest`, with its own `EventId` and the staff it is drawn on
(by default the voice's home staff, any staff of the same instrument for cross-staff notes). A
chord has one or more `Note`s, each with its own `NoteId`, its spelled pitch at concert pitch and
its tie; the noteheads are kept sorted by sound (F♯4 before G♭4 by letter), and a single note
costs no allocation of its own.

**Note values.** What is stored is the written value: a `BaseValue` (maxima to 1024th) and up to
four dots. The length in whole notes is **derived**, never stored: `NoteValue::length` builds it
with shifts, `Voice::metric_duration` multiplies the ratio of each tuplet that contains the event.
Storing the number too would be two copies of one fact (ADR-0003). A **whole-bar rest**
(`RestValue::WholeBar`) lasts whatever its bar lasts in the meter map, pickup bars included, so a
meter change cannot make it wrong.

**Tuplets.** `Tuplet::new(3, 2, eighth)` is "three eighths in the time of two". Tuplets live in a
table of their voice, and each event records the innermost tuplet that contains it; `parent` makes
the nesting a chain of at most four (with terms up to 255 nothing can overflow). A tuplet may cross a
barline: the note that would straddle it becomes two tied notes, both members of the same tuplet.
An incomplete tuplet is a valid state (it happens while typing).

**Ties.** A tie is a link stored on the notehead it starts from: `Tie::To(note)`, or
`Tie::LetRing` for a laissez-vibrer. The target may be in another voice of the same instrument
and spelled differently, as long as it sounds the same and comes later in score order (see
"Score order" below).

**Clefs and key signatures.** They cannot be events of a voice: a staff carries several voices,
and a voice may move between staves. Each staff has its initial clef and a list of clef changes at
(bar, offset); the document has its initial key (at concert pitch) and a list of key changes; an
instrument may override the key from a point on (`KeyOverride`: another signature, "no signature"
for horns written without one, or back to the score's key). `clef_at`, `key_at`,
`instrument_key_at` and `written_key_at` answer with a binary search; the written key of a
transposing instrument is derived from its transposition (a B♭ clarinet in C major shows D major).

```rust
// A triplet of eighths on beat 1, then a dotted quarter from 5/8 to the barline, tied to an
// eighth at the start of the next bar.
let eighth = NoteValue::plain(BaseValue::Eighth);
let triplet = builder.add_tuplet(voice, Tuplet::new(3, 2, eighth)?, None)?;
let g4 = Pitch::new(Step::G, Alteration::NATURAL, 4);
for i in 0..3 {
    // Offsets 0, 1/12 and 2/12 of a whole note.
    let offset = Duration::from_rational(Rational::new(i, 12)?);
    builder.add_chord(Placement::new(voice, bar0, offset).in_tuplet(triplet), eighth, &[g4])?;
}
let dotted_quarter = NoteValue::new(BaseValue::Quarter, 1)?;
let five_eighths = Duration::from_rational(Rational::new(5, 8)?);
let long = builder.add_chord(Placement::new(voice, bar0, five_eighths), dotted_quarter, &[g4])?;
let next = builder.add_chord(Placement::new(voice, bar1, Duration::ZERO), eighth, &[g4])?;
builder.set_tie(long.notes[0], Tie::To(next.notes[0]))?;
let document = builder.build()?;
// Derived, never stored: a dotted quarter lasts 3/8.
assert_eq!(document.metric_duration(long.event)?, Duration::from_rational(Rational::new(3, 8)?));
```

**What `build` checks.** Each builder method checks what it can see locally before creating
anything (the bar, voice, staff and tuplet exist and belong together, offsets are not negative,
nesting is at most four deep). `Document::validate`, which `build` runs, checks the rules that need
more: no two events of a voice overlap within a bar (gaps are fine, and an **overfull** bar is still
valid: its last notes simply reach past the barline), a whole-bar rest is alone, and every tie goes
to the same sound, later, in the same instrument. The first violation comes back as a typed
`Violation` naming the event or notehead.

**Lookups.** A derived index from `EventId` and `NoteId` to (voice, bar) finds any event or
notehead: one hash lookup, the voice scan, a scan of one chunk. `events(voice, bar)` gives a
chunk, `metric_duration` and `event_position` the derived time of an event. None of these
allocates.

**Tests.** `tests/voice_cases.rs` has 16 hand-written cases, one per case of ADR-0024 §13 in this
step (enharmonic chord, nested tuplets with 1/12 and 1/30, incomplete tuplet, tuplet across a
barline with a tied split, overfull bar, whole-bar rest after a pickup and a meter change, legal and
illegal ties, clef changes mid-bar, keys of a B♭ clarinet, a horn without signature and a violin
with an override, cross-staff notes, placement errors). `tests/voice_oracle.rs` compares random
voices with random nested tuplets against an oracle with its own `i128` fractions: every duration,
and whether `build` succeeds or which violation it reports; another property compares `clef_at`
with a linear scan. Six injected bugs (a wrong length for double dots, the parent tuplet ignored,
overlap at `<=`, the clef lookup off by one point, events at the same offset inserted in the wrong
order, the whole-bar rule loosened) were all caught; the insertion order only by the oracle.
`tests/voice_no_alloc.rs` checks that no lookup allocates and that `validate` makes a single
allocation.

**Benchmarks** (`benches/voice.rs`, 200 bars × 10 voices, 10 000 events with triplets and
two-note chords; instructions per call): duration while walking a system ≈ 320, duration by event
ID ≈ 625, event by ID ≈ 200, `clef_at` ≈ 470, `written_key_at` ≈ 555; `validate` of the whole
score ≈ 5.5 million (≈ 550 per event, about 2 ms).

#### Markings on the events and slurs

`src/marking.rs` and `src/slur.rs` (first part of step F1.09c, ADR-0024 §5–§7 and §10) add what is
written **on** the events, and the slurs. Dynamics, hairpins and tempo marks are in the next
section.

**Grace notes.** A grace chord is a chord with a `Grace`: an acciaccatura or an appoggiatura,
placed **before** the event it ornaments (at that event's offset) or **after** the event it
follows (at the offset where that event ends: the *Nachschlag*, or the closing notes of a trill).
It keeps its written value (the eighth that is drawn) but its metric duration is **zero**, so it
never moves the following events; how much time it takes from its neighbours when played is a
playback rule (F1.13). Since several events can now sit at one offset, a chunk is sorted by
(offset, **slot**): first the `After` graces of the event that ends there, then the `Before` graces
of the event that starts there, then that event. The slot is derived from the grace, never stored.
Graces cannot be in a tuplet (`DocumentError::GraceInTuplet`).

**Articulations, ornaments, fermatas, arpeggios.** They belong to the chord, drawn once per stem:
an `ArticulationSet` (a bit set in a `u16`: accent, marcato, staccato, staccatissimo, spiccato,
tenuto, portato, stress, unstress, soft accent; portato is its own mark, not staccato + tenuto), at
most one `Ornament` (trill with the optional accidental of its auxiliary note, turn, inverted
turn, mordent and upper mordent named after SMuFL because MusicXML's names are often swapped,
single-note tremolo with 1 to 4 strokes), a `Fermata` (short, normal, long, very long; also on a
rest, so a general pause is a whole-bar rest with a fermata) and an `Arpeggio` (plain, up, down, or
the non-arpeggio bracket). Setting one of the chord-only markings on a rest is
`DocumentError::NotAChord`.

**Performance offsets.** Every notehead carries its `PerformanceOffsets` inline: `onset` and
`length` as signed durations in **musical time** (a note a 64th early has `onset = −1/64`, so the
feel follows any tempo change) and `velocity` as a signed delta on the 16-bit scale of MIDI 2.0.
They are zero by default and stored exactly as given: clamping (a note before 0, a negative length,
a velocity off the scale) is done by playback on the result, so that nothing is lost when the
written dynamics change later.

**Slurs.** A `Slur` joins a start event and an end event and is kept in its **instrument**'s table,
sorted by ID, because it may join two voices of the instrument (a phrase passing between the hands
of a piano). Slurs may nest and share ends. A derived index answers "which slurs start or end on
this event" (`slurs_at`) with one hash lookup; up to two slurs per event it stores the IDs inline,
so it costs no allocation per entry.

**Score order.** With graces, "later" is no longer only "a later position": an acciaccatura and
its main note start at the same point. `Document::comes_before` compares events by position, then
by slot, then (inside one chunk) by their place in it; two events of different voices at the same
position and slot are simultaneous. Ties and slurs must go forwards in this order (owner's decision
of 2026-11-02), so an acciaccatura can be slurred or tied to its main note; for events without
graces the rule is the same as before.

```rust
// An acciaccatura slurred to an accented quarter with a trill; the quarter played a 64th early.
let quarter = NoteValue::plain(BaseValue::Quarter);
let main = builder.add_chord(Placement::new(voice, bar, Duration::ZERO), quarter, &[g4])?;
let grace = Grace { kind: GraceKind::Acciaccatura, attach: GraceAttach::Before };
let eighth = NoteValue::plain(BaseValue::Eighth);
let a4 = Pitch::new(Step::A, Alteration::NATURAL, 4);
let small = builder.add_grace_chord(Placement::new(voice, bar, Duration::ZERO), grace, eighth, &[a4])?;
builder.set_articulations(main.event, ArticulationSet::EMPTY.with(Articulation::Accent))?;
builder.set_ornament(main.event, Some(Ornament::Trill { auxiliary: None }))?;
let early = PerformanceOffsets { onset: Duration::from_rational(Rational::new(-1, 64)?), ..PerformanceOffsets::NONE };
builder.set_offsets(main.notes[0], early)?;
let slur = builder.add_slur(small.event, main.event, SlurPlacement::Auto)?;
let document = builder.build()?;
// The grace lasts zero and comes first in score order, although both start at the same point.
assert_eq!(document.metric_duration(small.event)?, Duration::ZERO);
assert!(document.comes_before(small.event, main.event)?);
assert_eq!(document.slurs_at(main.event), [slur]);
```

**What `validate` adds.** Rule 4: a `Before` grace has a main event at its offset in its voice
and bar, an `After` grace sits exactly where the previous main event of its chunk ends (several in
a row are fine); a grace inside a main event is reported as an overlap. Rule 5: both ends of a slur
exist and belong to its instrument, and the end comes after the start in score order. Both rules
run in the same single pass over the chunks and add no allocation.

**Tests.** `tests/marking_cases.rs` has 9 hand-written cases (articulation round trips and portato,
graces with zero duration and their slot order, a grace refused in a tuplet, graces without their
main event, ornaments with tremolo bounds and fermatas on rests, offsets per notehead, slurs that
nest and cross voices with the index, score order with graces and simultaneous voices, slur
errors that consume no ID). `tests/marking_oracle.rs` builds random bars of main notes and graces
with random slurs and compares them with an oracle that counts in sixteenths as plain integers: the
chunk order, whether `build` succeeds, `comes_before` on every pair, `slurs_at` on every event and
the zero duration of graces. Four injected bugs (slots swapped, the `After` rule switched off, the
chunk tie-break of the score order reversed, the index losing a third slur) were all caught by the
oracle. `tests/marking_no_alloc.rs` checks that no lookup allocates and that `validate` still makes
a single allocation.

**Benchmarks** (`benches/marking.rs`, 200 bars × 10 voices, 12 000 events with a grace before and
after each bar, 4 500 slurs; instructions per call): `slurs_at` ≈ 48, slur by ID ≈ 640,
`comes_before` ≈ 1 670; `validate` of the whole score ≈ 13.8 million (≈ 1 150 per event, about
4.5 ms). Rule 4 runs inside the pass of rule 1 and reuses the durations it computes: a separate
pass had doubled the cost of `validate`. On the score of `benches/voice.rs`, which has no graces or
slurs, `validate` went from ≈ 5.49 to ≈ 5.76 million (+5%, about 27 instructions per event for the
new checks); every lookup stayed within ±0.5%.

#### Dynamics, hairpins and tempo marks

`src/dynamic.rs` and `src/tempo_mark.rs` (second part of step F1.09c, ADR-0024 §8 and §11) add how
loud and how fast the music is written to be. Both are **position-anchored**: they sit at a
`ScorePoint` (a bar and an offset, new in `src/marks.rs`), not on a note, so a *p* under a rest or a
tempo change in the middle of a held chord is ordinary writing, and they follow their bar when bars
are inserted before it.

**Dynamics.** A `Dynamic` says a `DynamicMark`: a level of the scale (`DynamicLevel`, *pppppp* to
*ffffff*, ordered soft to loud so that `Ord` answers "is this louder?"), one of the accents and
combinations (*sf*, *sfz*, *sffz*, *fz*, *rf*, *rfz*, *fp*, *sfp*, *sfpp*, *pf*, *n*), or free text
(`OtherDynamic`, "subito p", with an optional level for playback). The standard names are Italian
in every language and are drawn, not translated; free text is the user's own and never empty
(`DocumentError::EmptyText`). Each instrument keeps its dynamics in one list sorted in score order.
A `DynamicPlacement` holds two separate things: the **scope** (the whole instrument, the usual case
including a piano's dynamic between its staves, or one voice, for divisi) and the **staff** it is
drawn on, which changes nothing in playback. There is at most one dynamic per scope at a point
(`DuplicateMark`); an instrument dynamic and a voice dynamic may share one. Nothing here is a
velocity: playback (F1.13) derives velocities from the dynamics, and the notes' velocity offsets are
deltas on top, so a test checks that adding dynamics never touches a note.

**Hairpins.** A `Hairpin` runs from a start point to an **excluded** end point, with a
`HairpinShape` (crescendo or diminuendo, wedge or text, *al niente*). The builder refuses one that
does not go forwards (`HairpinNotForward`); `validate` (rule 6) refuses two hairpins of the same
scope that overlap, in one pass over the instrument's list sorted by start, remembering the last end
per scope. Touching is fine ("cresc. up to here, dim. from here"), and so is a dynamic inside a
hairpin.

**Tempo marks** (choice C5 = T2 of ADR-0024). A `TempoMark` is what is printed, text and/or a
change: `TempoMarkChange::Immediate(Metronome)` (a tempo, printed or hidden: a hidden one is a DAW
user's tempo automation) or `Gradual(GradualMark)` (to a tempo, up to an explicit end point,
optionally jumping first to a starting tempo). The **tempo map is derived from the marks** and never
edited directly, so the score cannot say 120 and play 90. The rules approved by the owner (E1–E3):
the initial mark is a field of the document with its own `TempoMarkId`, takes the document's first
ID and must set a tempo at once (`InitialTempoNotImmediate`); a text-only mark changes nothing in
the map; a gradual change ends at its own point.

How the map is derived (`Document::walk_tempo_marks`, then `TempoMap::from_changes`): the marks are
read in score order; a gradual change must end after it starts and no other change may start inside
it, both judged in score order, on what the user sees (`Violation::GradualTempoNotForward`,
`Violation::TempoChangeInsideGradual`). Each point becomes the start of its bar plus its offset,
**clamped to the barline** (owner's decision of 2026-11-03): an overfull bar is a valid state, and
without the clamp a mark at offset 5/4 of a 4/4 bar would land after a mark at the start of the
next bar. Clamping keeps the order; two marks may then land on the same position, and the later
one in score order wins (the other stays written); a gradual change shrunk to nothing acts as an
immediate change to its target. `add_tempo_mark` tries the mark with the ID the counter would hand
out next (`IdCounter::peek`), derives the map, and takes the mark out again on failure: all or
nothing, no ID consumed. `validate` (rule 7) runs the same walk without building the map, so it
allocates nothing more.

```rust
// "Andante ♩ = 60" at the start, a ritardando over bar 4 and "a tempo" at bar 5.
let quarter = Duration::from_rational(Rational::new(1, 4)?);
let andante_tempo = Tempo::new(quarter, Rational::from_integer(60)?)?;
let andante = TempoMark::immediate(Some("Andante"), Metronome::shown(andante_tempo))?;
let mut builder = DocumentBuilder::new(meter_map, andante, c_major)?;
let bars: Vec<BarId> = (0..8).map(|_| builder.add_bar()).collect::<Result<_, _>>()?;
let rit = GradualMark { from: None, to: Tempo::new(quarter, Rational::from_integer(40)?)?, end: ScorePoint::start_of(bars[5]) };
builder.add_tempo_mark(ScorePoint::start_of(bars[4]), TempoMark::new(Some("rit."), Some(TempoMarkChange::Gradual(rit)))?)?;
builder.add_tempo_mark(ScorePoint::start_of(bars[5]), TempoMark::immediate(Some("a tempo"), Metronome::hidden(andante_tempo))?)?;
// A p for the whole piano under the upper staff, and a crescendo over bar 2.
builder.add_dynamic(piano, ScorePoint::start_of(bars[0]), DynamicPlacement::instrument(upper), DynamicMark::Level(DynamicLevel::P))?;
builder.add_hairpin(piano, ScorePoint::start_of(bars[2]), ScorePoint::start_of(bars[3]), DynamicPlacement::instrument(upper), HairpinShape::CRESCENDO)?;
let document = builder.build()?;
// The tempo map is derived: 0 → ♩ = 60, 4 → rit. to ♩ = 40 until 5, 5 → ♩ = 60.
assert_eq!(document.tempo_map().changes().len(), 3);
```

**Tests.** `tests/dynamic_cases.rs` (7 cases: the order of the levels, sorting and one dynamic per
scope, the placement checks with no ID consumed, free text, ADR-0024 §13's "a dynamic never touches
a note's offsets", hairpins that touch, overlap only within a scope) and `tests/tempo_mark_cases.rs`
(11 cases computed by hand: the initial mark, empty marks, text-only and hidden marks, a ritardando
with "a tempo", a starting tempo, a change inside a gradual one refused all or nothing, gradual
changes that do not go forwards, the points, meters, the clamping with "the later one wins", a
gradual change shrunk to nothing). `tests/tempo_mark_oracle.rs` compares random marks and hairpins
on random meters with an oracle that counts in eighths: whether each mark is accepted and with which
error, the builder unchanged after a refusal, the derived map, and the first hairpin overlap that
`build` reports. Seven injected bugs (no clamping, the earlier mark winning, a shrunk gradual change
kept, "inside" with `<=`, touching hairpins as an overlap, scopes merged, duplicate dynamics
allowed) were all caught, the first six by the oracle too. `tests/dynamic_no_alloc.rs` checks that
reading the lists, the mark in force at a point and the derived map never allocate, and that
`validate` makes at most two allocations.

**Memory per note** (owner's choice E7). `tests/memory.rs` builds a score shaped like the heavy one
of `spikes/model/` (1 000 bars, 30 staves, 41 voices, 221 169 noteheads; no tuplets) with the real
builder and measures what the document keeps with the new `net_bytes` of `orchestra-test-support`:
**257 bytes per note** (56.9 MB), events, derived indices and inline offsets included. The test fails
above **285** (the measure plus about 10%), so that a structure that grows is a deliberate decision.
Since F1.11b part 2b the score comes from `tests/common/score.rs`, shared with the editor's
benchmarks.

**Benchmarks** (`benches/dynamic.rs`; instructions): adding a tempo mark near the start of 1 000 bars
with 250 marks, half of them gradual, which derives the whole map again, ≈ 985 000 (about 0.3 ms,
within ADR-0024's estimate of 0.6 ms); the tempo mark in force at a point ≈ 620; `validate` of 200
bars × 10 instruments with 8 000 events, 2 000 dynamics and 2 000 hairpins ≈ 3.3 million. Against
the first part of F1.09c every existing benchmark of the program stayed within ±0.9%.

#### The form: repeats and voltas

`src/form.rs` (first part of step F1.10b, ADR-0025 §2–§3) adds the form of the piece: what is
written once and **unrolled** later by `orchestra-playback` (F1.12) into what is played. Markers,
jumps, bar repeats and pass rules are in the next section.

**On barlines, in the document.** A `RepeatStart` (‖:) belongs to the start of a bar, a
`RepeatEnd` (:‖) to its end, a `Volta` to a run of whole bars (ADR-0025 F1): nothing has an offset,
so a played pass is always a run of whole bars. A repeat in the middle of a written bar is written
by splitting the bar into two irregular bars, as ADR-0018 already allows. The form belongs to the
**document** (`Document::form`), not to an instrument: a repeat barline crosses the whole system.

**Passes.** A `PassSet` is a set of passes 1–64 in one `u64` (bit `k − 1` for pass `k`):
membership, union, disjointness and "is it exactly 1..=n?" are single bit operations. 64 is
`MAX_PLAYS`, the most times a section can be played (F10).

**Repeats.** An end repeat carries `times`, the total number of plays (2 by default, 2–64). An end
without a start goes back to the start of the score (F2); repeated sections never nest or overlap
(F3). **Voltas** form groups: a group starts at the volta holding pass 1, the following voltas come
right after it with higher passes, together they hold exactly 1..=n, there are at least two, and
**every volta but the last ends with an end repeat** of the section ("1. :‖ 2. :‖ 3."; the owner
corrected ADR-0025 on this point while it was being implemented). With voltas, `times` must be 2
or n: a contradiction is refused, not ignored (owner's decision of 2026-11-03).

**What `validate` adds.** Rule 8 merges the start and end repeats in score order and pairs them:
a start must not find another one open; an end without an open start is fine only as the first end
of the score, or as a further end of a volta group. Rule 9 reads the voltas once, splits them into
groups and checks each one. Neither allocates.

**Bar numbers** (`Document::bar_label`, ADR-0025 F9). One `BarNumbering` style per document:
`Continue` (the default: the number is the written bar), `SameAsFirstLettered` ("8a", "8b") and
`SameAsFirst` ("8", "8"). The plain number comes from the meter map with its uncounted bars and
restarts; the two other styles change it on top: the bars of a later volta take the numbers of the
first volta's counted bars (continuing if the later volta is longer: 8a | 8b 9b), the bars after a
group go back by what the group adds beyond its longest volta, and a numbering restart inside a
group wins over the style. The counted bars of a volta are counted with two binary searches in the
list of uncounted bars, so a label costs O(v log u) for the v voltas before it.

```rust
// ‖: bars 1–7 [1. bar 8 :‖] [2. bar 9]: the first example of ADR-0025 §3.
builder.add_repeat_start(bars[0])?;
builder.add_volta(bars[7], bars[7], PassSet::range(1, 1)?, true, None)?;
builder.add_repeat_end(bars[7], 2)?;
builder.add_volta(bars[8], bars[8], PassSet::range(2, 2)?, false, None)?;
builder.set_bar_numbering(BarNumbering::SameAsFirstLettered);
let document = builder.build()?;
// The second volta is "8b", and the bar after it is 9 again.
let label = document.bar_label(bars[8])?.unwrap();
assert_eq!((label.number, label.letter()), (8, Some('b')));
assert_eq!(document.bar_label(bars[9])?.unwrap().number, 9);
```

**Tests.** `tests/form_cases.rs` has 12 hand-written cases: pass sets, simple repeats, nesting
refused, voltas and "1.–3.", "1. :‖ 2. :‖ 3.", every volta rule, the builder's checks, and the bar
numbers of ADR-0025's examples in the three styles, with longer later voltas, several groups,
uncounted bars and restarts. `tests/form_oracle.rs` generates random sections, voltas and
mutations (half of the cases valid, half not) and compares them with an oracle that states the
rules again and numbers the bars **one by one** with a counter, as a copyist would, while the
library computes each number on its own with shifts. The oracle found a real case: a first volta
whose bars are all uncounted, where the library used the bar's own number; the voltas now start
from the first counted bar of the group. Nine injected bugs were all caught, seven by the oracle
from the start and the other two after adding the mutations that produce them (a group of one
volta, a contradicting `times`). `tests/form_no_alloc.rs` checks that labels and the form lists
never allocate and that rules 8 and 9 add no allocation to `validate`.

**Benchmarks** (`benches/form.rs`, 400 bars in 50 sections with 150 voltas, far more than a real
score; instructions per call): `bar_label` with `Continue` ≈ 112, with the lettered style ≈ 13 800
for bars after 25–50 volta groups (about 275 per group: a real score has a handful), from ≈ 18 800
before the counting with binary searches; `validate` of the form ≈ 178 000. Every existing benchmark
stayed within ±0.01% of `main`.

#### Markers, jumps, bar repeats and pass rules

`src/jump.rs` and `src/bar_repeat.rs` (second part of step F1.10b, ADR-0025 §4 and §6–§7)
complete the form.

**Markers and jumps.** A `Marker` is a segno or a coda at the start of its bar, or a Fine or a
"To Coda" at its end (`MarkerKind::at_start`). A `Jump`, at the end of its bar, is the triple that
every notation program uses, "go to / play until / continue at", as a `JumpSpec`: `to` (the start
of the score or a segno), `until` (a Fine or a "To Coda"), `then` (the coda after a "To Coda"),
plus the two choices of ADR-0025: `RepeatsAfterJump` (`Skip` by default: every repeated section
once, only its last volta; `Play`) and `JumpFires` (`AfterRepeats` by default: on the last pass
of the section containing the jump; `OnPass(n)`). Markers are referenced by **ID**, never by a
label, so a D.S.S. cannot point at the wrong segno. `JumpSpec::da_capo()`, `dal_segno(segno)`,
`.al_fine(fine)` and `.al_coda(to_coda, coda)` build the usual ones; a `text` replaces the derived
words for the jumps written as free text ("D.C. senza ritornelli"), and markers and voltas have
one too. At most one start marker, one end marker and one jump per bar.

**Bar repeats** (`BarRepeat`): `count` bars from `first`, on one **staff**, repeat the `span`
bars before them (`BarRepeatSpan::One`, `Two`, `Four`); `count` is a positive multiple of the
span. The repeated bars **store no events** on that staff: F1.12 will play the source bars, and an
edit of the source reaches every repetition, because there is no copy. **Pass rules** (`PassRule`):
a range of an instrument or a voice played only on the passes of a `PassSet` of the repeated
section containing it ("1st time tacet" = {2}).

**What `validate` adds.** Rule 10: the targets have the right kinds, the "until" marker lies
between the target and the jump in score order (otherwise the jump never stops there), and
`OnPass(n)` is only for a jump inside a repeated section with at least n plays. Rule 11: a bar
repeat has its source bars, its bars hold no event on its staff, two of one staff do not overlap,
and no form marking lies between its first source bar and its last repeated bar (otherwise "the
previous bars" would depend on the path taken); a pass rule lies inside one repeated section, its
passes are within the section's plays, and two of one scope do not overlap. Both need "the
repeated section containing this bar": `Document::section_at` merges the repeat barlines like rule
8, extends a section to the further ends of its voltas, stops at the first section after the bar
and computes the number of plays only for the section found.

```rust
// Segno at bar 2, a repeated chorus 2–4 with "To Coda" at the end of bar 3, D.S. al Coda at the
// end of bar 5, coda at bar 6: the jump is taken after the chorus's last pass, and the repeat is
// skipped after it (the classical convention, ADR-0025 F4).
let segno = builder.add_marker(bars[2], MarkerKind::Segno, None)?;
builder.add_repeat_start(bars[2])?;
let to_coda = builder.add_marker(bars[3], MarkerKind::ToCoda, None)?;
builder.add_repeat_end(bars[4], 2)?;
let coda = builder.add_marker(bars[6], MarkerKind::Coda, None)?;
builder.add_jump(bars[5], JumpSpec::dal_segno(segno).al_coda(to_coda, coda), None)?;
// Two bars of the piano's left hand repeated with the two-bar sign; the trumpet tacet the first
// time through the chorus.
builder.add_bar_repeat(lower_staff, bars[7], 2, BarRepeatSpan::Two)?;
builder.add_pass_rule(trumpet, ScorePoint::start_of(bars[2]), ScorePoint::start_of(bars[5]),
    DynamicScope::Instrument, PassSet::range(2, 2)?, Some("1st time tacet"))?;
let document = builder.build()?;
```

**Tests.** `tests/jump_cases.rs` has 11 hand-written cases: a D.S. al Coda around a repeat, a D.C.
al Fine with the repeats played and a free text, a jump on a chosen pass, two segni, the kinds of
the targets, the "until" between target and jump, `OnPass` outside a section or beyond its plays,
the builder's checks, bar repeats on the piano's lower staff with an overlap and a staff that is
not empty, the source and the form markings of a bar repeat, pass rules with their violations, and
a section with "1. :‖ 2. :‖ 3." that reaches its second volta. `tests/jump_oracle.rs` builds a
valid form with random markers, jumps, bar repeats, pass rules and left-hand notes (each case with
only some of them, about a third of the cases valid) and compares `build` with an oracle that
states rules 10 and 11 on plain integers. Ten injected bugs were all caught, eight by the oracle;
the generator first produced almost only invalid cases (109 valid out of 1 024, very few with
jumps), and was made to lean towards valid objects. `tests/form_no_alloc.rs` now has markers, a
jump and a bar repeat too: reading them never allocates, and `validate` still makes one allocation
(the pass-rule check allocates a small list of scopes only when an instrument has pass rules).

**Benchmarks** (`benches/form.rs`, same score): `validate` with a segno, a Fine and a D.S. al Fine
on pass 2 in each of the 50 sections ≈ 933 000 instructions, about 15 000 per jump for the section
lookup on a score far beyond real ones; it was ≈ 1.96 million before the lookup stopped at the
first section after the bar and computed the plays only once.

#### Edits, transactions and undo

Since F1.11b part 1 an existing document changes only through an **`Editor`** (ADR-0003 §4,
ADR-0026): the document, its one undo history and the transaction in progress. Four new modules:

- `edit`: the **primitive edits**, three verbs over one enum of object kinds plus the bar and meter
  operations and the values the document has once:

  ```text
  Edit = Insert(Object) | Remove(ObjectRef) | Replace(Object)
       | InsertBars { at, ids, meter } | RemoveBars { at, count }
       | Meter(MeterEdit) | SetInitial(Initial)
  ```

  Applying an edit (`Document::apply_edit`, crate-private) returns its **exact inverse**: an
  `Insert` gives the `Remove` of the same ID, a `Remove` the `Insert` of the whole object with its
  exact place, a `Replace` a `Replace` with the old content, `InsertBars` gives `RemoveBars` and
  back. Undo applies the inverses in reverse order, and what it returns is exactly the redo.
- `change`: the **`ChangeSet`** every transaction, undo and redo returns: the (voice, bar) cells
  whose events changed, the objects inserted, removed and replaced (bars included), the first bar
  from which the meter map may answer differently, the first position from which the tempo map
  differs, and whether the form changed. Each edit records its own entries, so no notification can
  be forgotten; the lists are sorted and deduplicated once at the end.
- `editor`: `Editor::transact` runs a closure as **one transaction = one step of undo**; if the
  closure returns an error, every edit it applied is reverted. The tempo map is derived again once,
  at the end, and only if a tempo mark or a bar start changed; if it fails, the transaction is
  reverted too. The history is **linear** and bounded by **memory** (`DEFAULT_HISTORY_LIMIT`,
  128 MB, the most recent step always kept), and the document gains a **revision** number.
- `editor_tests` (tests only, see below).

```rust
let mut editor = Editor::new(document);
// One transaction = one step of undo. `?` on a refused edit reverts the whole transaction.
let changes = editor.transact(|tx| {
    tx.apply(Edit::Remove(ObjectRef::Event(grace)))?;
    tx.apply(Edit::Remove(ObjectRef::Slur(slur)))
})?;
assert_eq!(changes.removed().len(), 2);
editor.undo()?;   // both come back, with the same IDs and in the same places
editor.redo()?;   // and go again
```

**What an edit checks.** The structure: the right container, sorted lists, derived indices in
step, IDs unique and already handed out by the counter (an ID not handed out yet would collide with
a later object). The musical invariants (no overlaps in a voice, ties to the same pitch, a valid
form) are the commands' job, in part 2. One rule follows: **an insertion checks only what no other
edit can take away**. Bars can only be removed when nothing refers to them (`BarsNotEmpty`),
tuplets only when no event uses them (`TupletInUse`); a slur, on the other hand, does not check
that its events exist, nor a jump that its markers do, because a later edit of the same
transaction may have removed them, and then undo would fail.

**Orders that do not depend on history.** Where a list may hold several objects at one point
(dynamics, hairpins, voltas, bar repeats, pass rules) they are sorted by (point, ID), and the
"slurs at an event" index by ID: removing an object and inserting it back gives an identical list.
Events are the exception, because at one (offset, slot) their order is musical (grace notes in a
row): a removed event remembers its place in the chunk (`Chunk::remove` and `Chunk::insert_at`).
An emptied chunk goes back to "no allocation", so it compares equal to a chunk that never held
anything. `Document::same_content` compares two documents ignoring the ID counter and the revision,
which only grow; `Document::indices_are_consistent` rebuilds every derived index and compares.

**Tests** (`src/editor_tests.rs`, inside the crate because the objects' constructors are
crate-private): nine hand-written cases (grace notes back in their order after an undo, a failed
transaction, an empty transaction, the change sets of the two maps, busy bars, IDs never handed
out, the bounded and linear history, structural refusals) and a property test: a rich document (every
kind of object) receives random transactions of random edits, many of them refused, interleaved with
undos and redos, against a model made of one snapshot per step of the history; after every step
the indices must agree with a rebuild from scratch and the tempo map with the marks. The test found
at once that a transaction may also fail at the end, when a moved tempo mark lands inside a gradual
change: the model now expects that. Five injected bugs (undo in the wrong order, notehead index left
behind by a replacement, slur index entry left behind, no chunks for new bars, a removed event
forgetting its place) were all caught; the second one only after the generator learned to change
the noteheads of a chord. The benchmarks of the editor came with part 2b (see below).

#### Commands, gestures and the local check

Since F1.11b part 2a the document is changed with **commands** (module `command`, ADR-0026 §1, §4,
§6 and the precisions of §11): a `Command` says *what* the user wants, as data, and expands into
primitive edits inside a transaction. The history keeps the edits' inverses, never the command, so
redo never runs a command again and never hands out new IDs.

```rust
let mut editor = Editor::new(document);
// One command = one step of undo.
let changes = editor.execute(&Command::EnterEvent(NewEvent {
    at: Placement::new(flute, bar_4, quarter * 3),
    body: NewBody::Chord { value: dotted_half, pitches: vec![g4], grace: None },
}))?;
// A dotted half from the last beat of a 4/4 bar: a quarter tied to a half in the next bar.
// Several commands in one step (a paste):
editor.transact(|tx| {
    tx.execute(&Command::DeleteEvent(a))?;
    tx.execute(&Command::DeleteEvent(b))
})?;
```

The commands of part 2a:

- `EnterEvent(NewEvent)`: a chord, a grace chord or a rest, **only into free space** (V2: over
  another event it is refused). Across a barline it becomes a **tie chain** (V3): cut at every
  barline, each part written with the longest values first (at most two dots: five eighths are a
  half and an eighth), the noteheads of a chord tied pitch by pitch; a rest becomes several rests.
  Past the end of a **free** bar the bar grows instead (V5). A tuplet note across a barline
  was refused (V4) until F1.11c part 2a, which splits it inside its tuplet (see below). Refused:
  past the last bar (W3), a length no value can write (`NotWritable`).
- `DeleteEvent(EventId)`: leaves a **gap** (V6: no rest is written, engraving fills it, like
  Dorico's implicit rests), and takes along the grace notes ornamenting the event, the slurs on it
  and the tuplets left empty; the ties leading into it become `Tie::None`.
- `ChangeEvent { event, change }`: one aspect per `EventChange` variant (move, value, pitch, add or
  remove a notehead, tie, articulations, ornament, fermata, arpeggio, performance offsets, staff).
  A move takes the grace notes along and **never splits** (W1: past the barline the bar becomes
  overfull, a valid state); a new pitch changes the **whole tie chain** (W2); removing the last
  notehead deletes the event.
- `InsertBars { at, count }`: empty bars with the meter, clef, key and tempo in force before them;
  ranges spanning the point grow; between two voltas of a group the new bars join the earlier one
  (X11, part 2b). Refused from the first bar of a bar repeat's source to its end (W6).
- `DeleteBars { at, count }`: the events go (as `DeleteEvent`), the dynamics too; the meter, clef,
  key and tempo last in force inside the run move to the first bar after it (at the start of the
  score they become the initial values); hairpins, pass rules, voltas, bar repeats and gradual
  tempo changes are **clipped** (W4: if the run starts the score in the middle of a gradual change,
  the score starts at the tempo the change arrives at, because a gradual change cannot open the
  score); then a **cascade** (W5) runs the form checks of `validate`
  again and deletes the marking each one reports, until the form is valid. At least one bar stays.

**Atomic commands.** A command that fails is reverted at once, even inside a larger transaction:
the editor remembers where its edits began and applies their inverses (`Editor::execute_open`).

**The local check** (`Document::check_cells`, in `validate.rs`). The event commands do not check
the musical rules one by one: they emit their edits and then the editor runs, **only on the cells
(voice, bar) the command touched**, the rules of `validate` a change of events can break: no
overlap, a whole-bar rest alone, grace notes with their main event, the ties starting there and
those **arriving** there, the slurs on those events, no event under a bar repeat. That is why
entering a note over another one fails with `Violation::Overlap`. To find the arriving ties
without scanning the instrument, the document gains a derived **backward tie index**
(`tie_index`, target → sources, with `Document::ties_into`), kept up to date by every edit like
the slur index. Its entries are an `IdRefs<T>`, the small sorted set (two IDs inline) that the
slur index already used, now generic in `id.rs`. To check the per-chunk rules, `validate` was split:
`check_chunk` checks one cell, and `validate` calls it for every cell.

**Gestures** (a drag, ADR-0026 §4):

```rust
editor.begin_gesture()?;
for offset in mouse_positions {
    // Each update is applied live and returns its own change set, for the views.
    let live = editor.update_gesture(&Command::ChangeEvent {
        event: note,
        change: EventChange::Move { bar, offset },
    })?;
}
editor.end_gesture()?;     // the whole drag is one step of undo
// or: editor.cancel_gesture()?;  (Esc) back to the state before the drag
```

While a gesture is open `transact`, `execute`, `undo` and `redo` are refused with
`CommandError::GestureOpen`. **Compaction:** the inverse of a `Replace` of an object already
replaced in the same gesture is dropped, so a 200-update drag keeps one old value (measured: under
1 KB of history). A refused update is reverted alone and the gesture stays open; the tempo map is
derived again after every update that moves it.

**Tests** (`src/command_tests.rs`): nineteen hand-written cases, one per rule above, and a property
test: random sequences of random commands on the rich document of `editor_tests`, with undos and
redos; after **every** command the full `validate` must pass and the indices agree with a rebuild,
a refused command must change nothing, and undoing everything must give back the original. In
one run (proptest picks new cases each time): 892 commands carried out and 332 refused, with 13
different kinds of refusal. Seven injected bugs (ties into a deleted note not cleared, no compaction, clef not carried,
no cascade, the local check skipping slurs or arriving ties, grace notes not deleted, the wrong tempo at
the start after a cut accelerando) were all caught.

#### Commands on marks, the form and tuplets

F1.11b part 2b completes the commands of ADR-0026 §9 (module `mark_command`, with the precisions
X1–X11 of ADR-0026 §11, approved by the owner on 2026-11-05). A mark is described by a
**`MarkSpec`**: what it is and where it sits, **without an ID**. The ID of a new mark comes from
the document's counter, and a changed mark keeps its own, so the same description serves both
adding and changing, and nobody builds an object with an ID of their own choosing.

```rust
// A dynamic: described without an ID; the ID comes from the counter.
let changes = editor.execute(&Command::AddMark(MarkSpec::Dynamic {
    instrument: piano,
    at: ScorePoint::start_of(bar_3),
    placement: DynamicPlacement::instrument(lower_staff),
    mark: DynamicMark::Level(DynamicLevel::F),
}))?;
let dynamic = changes.inserted()[0];
// Moved to bar 4: same ID, same kind, the whole new description (X5).
editor.execute(&Command::ChangeMark { mark: dynamic, to: forte_at_bar_4 })?;
// Removed; the form markings it leaves invalid would go in cascade (X2).
editor.execute(&Command::RemoveMark(dynamic))?;
// A first and a second ending, with the end repeat the first one lacks (X1).
editor.execute(&Command::AddVoltas(vec![first_ending, second_ending]))?;
```

The new commands:

- `AddMark(MarkSpec)`, `ChangeMark { mark, to }`, `RemoveMark(ObjectRef)`: every mark (slurs,
  dynamics, hairpins, tempo marks, clef and key changes, key overrides) and every form marking
  (repeat barlines, voltas, markers, jumps, bar repeats, pass rules). **Adding, moving and
  changing never delete anything** (X3): a result that breaks a rule is refused with the typed
  violation (a hairpin overlapping another one of the same scope, a tempo mark inside a gradual
  change, a jump whose marker is missing, a bar repeat over bars that hold music, X4). A change
  keeps the ID and the **kind** (X5: a dynamic cannot become a hairpin, `MarkKindMismatch`); a drag
  sends it inside a gesture, and the compaction keeps one old value. **Removing** deletes in
  **cascade** the form markings it left invalid (X2, like deleting bars): removing the segno takes
  the D.S. along, in the same step. Events, tuplets, bars and the initial values are not marks
  (`NotAMark`): they have commands of their own.
- `AddRepeat { start, end, times }` and `AddVoltas(Vec<NewVolta>)` (X1): a volta alone is never
  valid (a group has at least two, with the end repeat of the section), so voltas are added **a
  whole group at a time**, together with the end repeats the group lacks (with the group's number
  of plays), and a repeated section with its two barlines.
- `AddTuplet`, `ChangeTuplet`, `RemoveTuplet` (X7): a tuplet is added **empty** (an incomplete
  tuplet is a valid state) and filled with `EnterEvent`; changing its ratio or look replaces it
  **in place** (the new `Document::replace_tuplet_in_place` edit path, which names the cells of
  its events because their durations change, so the local check refuses a ratio that makes them
  overlap); removing it takes along the events it contains and the tuplets nested in it.
- `SetInitial(Initial)` (X8): the initial tempo (immediate), key, a staff's clef, the numbering
  style; set, never removed. A new key or clef **changes no pitch** (X6): pitches are stored as
  spelled at concert pitch, and respelling will be a command of its own.
- `SetBarGrouping`, `SetUncounted`, `SetNumberingRestart` (X9): the per-bar data that does not move
  a barline. Irregular and free bars and meter changes, which do, come with F1.11c.

**Between two voltas (X11).** The property test found that `InsertBars` (part 2a) put new bars
between a first and a second ending, outside both, leaving the group invalid: no range spans that
point. The owner chose that the new bars **join the earlier volta**, and the end repeat that closed
it moves to the end of the last new bar.

**The local check, second half** (`Document::check_objects`, X10). After every command except the
bar commands and `RemoveMark` (which keep the rules by construction or with the cascade),
`command::check` runs `check_cells` on the cells and `check_objects` on the marks the command
inserted or replaced: the ends of a slur, the hairpins of its instrument, the walk over the tempo
marks once if a tempo mark changed, the form checks once if a form marking changed. Without it a
tempo mark inside a gradual change would only fail when the transaction derives the tempo map at
its end, taking the good commands of the same transaction down with it.

**Tests** (`src/mark_command_tests.rs`): sixteen hand-written cases, one per decision X1–X11, and
the property test of `command_tests`, which now draws from 21 kinds of command (12 new ones: marks
added, moved and removed, slurs, tempo marks, repeats, volta groups, markers, jumps, bar repeats,
tuplets, clefs, keys, per-bar data). 12 000 random sequences passed after the fix of X11. Seven
injected bugs (no hairpin check, no cascade on removal, the tuplet cells not named, the end repeat
of a volta group not added, the end repeat not moved by X11, no form check, no tempo check) were
all caught; the last one only after a test of a refused tempo mark inside a larger transaction was
added.

#### Benchmarks of the editor and memory of the history

The normal and heavy scores (100 bars × 10 staves, 1 000 bars × 30 staves) are generated by
`tests/common/score.rs`, shared with `#[path]` by `tests/memory.rs` (which measures the same 257
bytes per note as before), `tests/history_memory.rs` and `benches/editor.rs`; an optional empty
**editing instrument** at the end receives the notes the benchmarks enter. `benches/editor.rs`
counts instructions on both scores (heavy score; batches of 64 divided by 64): entering a note
end to end ≈ 8 700, adding a dynamic ≈ 5 200, undoing a 200-note paste ≈ 385 000, inserting a
bar ≈ 132 000, deleting one ≈ 978 000 (with clipping, carrying and cascade), one update of a
1 000-update gesture ≈ 14 500. The setup checks that every command of a batch succeeds, so that
no refusal is measured by mistake. Everything is far under the 1 ms of a note and the 16 ms of
ADR-0007.

`tests/history_memory.rs` measures the heap one step of the history really holds: an editor with a
limit of 0 keeps only the last step, and turning it back into its document frees exactly the
history. Its first run found that the steps kept the spare capacity of their vectors (a 200-note
paste held 53 kB for an estimate of 42 kB): `Step::undoing` now shrinks them, and the estimate is
exact but for the texts it does not count:

```text
step                                         estimate     measured
one small edit (overhead)                         208          336
one note entered                                  208          336
200 notes pasted                                41600        41728
1 000-update gesture                              208          336
100 bars of the heavy score deleted           4567688      4567896
```

The difference of 128 bytes is the deque of the history itself. The test fails if the estimate
falls below 90% of the measure, so the 128 MB limit stays honest.

#### Meter changes and irregular bars (KeepBars)

F1.11c part 1 adds the commands that change how long bars are (module `meter_command`,
ADR-0026 §7 with the precisions Y1–Y7 of §11, approved by the owner on 2026-11-05):
`ChangeMeter { bar, change, mode }` sets a meter change, or removes it with `None` (never at bar
0), and `SetIrregularBar { bar, irregular, mode }` makes one bar longer or shorter, a pickup or a
free bar, or regular again. Both take a **`MeterMode`**, which says what happens to the music
already written. This part implements **`KeepBars`**: every bar keeps its events, marks and form
markings at their offsets, only the lengths change, and the music after the change moves in time.
`Rewrite`, the default the interface will offer (the music stays where it sounds and the barlines
move), comes in part 2; `MeterMode` is `#[non_exhaustive]` for that.

```rust
// A 3/4 from bar 9: the bars keep their music, the bars after them move in time.
editor.execute(&Command::ChangeMeter {
    bar: bar_9,
    change: Some(MeterChange::new(Meter::new(&[3], 4)?)),
    mode: MeterMode::KeepBars,
})?;
// The first bar becomes a pickup of one quarter.
editor.execute(&Command::SetIrregularBar {
    bar: first_bar,
    irregular: Some(IrregularBar { length: quarter, kind: BarKind::AlignEnd }),
    mode: MeterMode::KeepBars,
})?;
```

What the command does beyond the meter map edit:

- **Y3**: the per-bar groupings of the section that no longer add up to the new meter are removed
  in the same step (they described how the old meter was counted); a free bar loses its grouping.
- **Y4**: a pickup longer than the new meter, or a section grouping that does not add up, is
  refused with the meter map's error (`PickupLongerThanMeter`, `GroupingLengthMismatch`).
- **Y5**: inside a bar repeat the change is refused (`InsideBarRepeat`), because a repeated bar
  would differ in length from its source; a meter change exactly at the first source bar changes
  every bar of the range alike and is accepted.
- **Y7**: a shortened bar may become overfull (valid, ADR-0022 §4), but if a note past the new
  barline is tied or slurred into the next bar, its tie or slur no longer goes forward in score
  order, which is by position (ADR-0024): the change is refused with `TieNotForward` or
  `SlurNotForward`. The property test found the case; the owner chose the refusal. The check
  (`check_overfull`) looks at the last event of each chunk of the changed bars and runs the local
  check only on the overfull cells.

Tempo marks past a shortened barline need nothing: the tempo map already clamps them to their
barline (F1.09c).

**Y8, a correction of `InsertBars`.** The property test also found that a pass rule ending
exactly at the start of bar *i* (its end is exclusive) grew over bars inserted before *i*, and
could reach out of its repeated section. The owner chose that an end lying exactly at the
insertion point does not span it: hairpins, pass rules and gradual tempo changes ending there keep
ending before the new bars, anchored to the start of the first one.

**Tests** (`src/meter_command_tests.rs`): eight hand-written cases (the music moving, a removed
change, the stale groupings, the contradictions, the bar repeat, irregular and free bars, the tie
of Y7, the ends of Y8) and two new kinds of command in the property test (23 in all; 12 000 sequences passed).
Six injected bugs (no Y7 check, stale groupings kept, no bar-repeat check, the first source bar
refused, a free bar keeping its grouping, no Y8) were all caught. **Benchmarks** (`benches/editor.rs`,
heavy score): a 3/4 at the middle bar ≈ 5.7 million instructions (its section of 60 bars becomes
overfull in every voice and is checked), a bar of two quarters ≈ 122 000: a few milliseconds at
most, under the 16 ms of ADR-0007.

#### Tuplet notes across a barline

Since F1.11c part 2a (ADR-0026 §11 Z3–Z5) a note of a tuplet that crosses a barline is no longer
refused (V4 is lifted): it is split into tied notes that **stay in the same tuplet**, as ADR-0024
(C4) prescribes and Dorico does ("spans barline"). The part left in each bar is brought back to the
tuplet's own written values, dividing it by the product of the ratios of its tuplet chain (nested
tuplets included), and written with the longest values first, as outside a tuplet (V3). A part no
value with at most two dots can write is refused (`NotWritable`); the error `TupletAcrossBarline`
is gone.

```rust
// A triplet eighth (1/12 in the bar) entered 1/24 before the barline:
editor.execute(&Command::EnterEvent(NewEvent {
    at: Placement::new(voice, bar_1, Duration::from_rational(Rational::new(23, 24)?))
        .in_tuplet(triplet),
    body: NewBody::Rest(RestValue::Value(NoteValue::plain(BaseValue::Eighth))),
}))?;
// Now two triplet sixteenths: 1/24 before the barline, 1/24 after it, both in the triplet.
```

One function does it (`command::split_values`, with `tuplet_ratio`), shared by `EnterEvent` now and
by the meter change that rewrites the music in part 2b (Z5). **Tests** (`src/command_tests.rs`): three
hand-written cases (a triplet note across a barline, a part of two values spaced by the ratio, a
triplet nested in a quintuplet), the old refusal case turned into a `NotWritable` case, and the
property test now entering notes inside tuplets. Two injected bugs (the part not divided by the
ratio, the values of a part not spaced by it) were caught, the second only by the case written for
it: a gap between two values is a valid state, so `validate` cannot see it.

#### Rewrite: the music stays where it sounds

F1.11c part 2b adds the meter change that rewrites the music (module `rewrite`, ADR-0026 §7 with
the precisions R1–R11 of §11, approved by the owner on 2026-11-05): `MeterMode::Rewrite`, the mode
the interface will offer by default, as Dorico, MuseScore and Sibelius do. The music keeps its
place in time and the barlines move, from the bar of the change up to the next meter change, the
next free bar or the end of the score (the **region**, R1).

```rust
// 8 bars of 4/4; a 3/4 at bar 3 rewrites bars 3–7 (20 quarters) into new bars.
editor.execute(&Command::ChangeMeter {
    bar: bar_3,
    change: Some(MeterChange::new(Meter::new(&[3], 4)?)),
    mode: MeterMode::Rewrite,
})?;
// A whole note of old bar 3 is now a dotted half tied to a quarter of bar 4; the Fine at the end
// of old bar 4 split a bar of 3/4 into 2/4 + 1/4 (the second uncounted), so the form plays as it did.
```

How it works, in order (`rewrite::rewrite`):

1. **The region and the new grid** (R1–R4): bars of the meter from the start of the region, an
   irregular last bar with what is left so that nothing after the region moves (at the end of the
   score the last bar stays full), a pickup or free bar at the change kept as the first bar. The
   same algorithm serves `SetIrregularBar` in `Rewrite` mode (adding a pickup to music already
   written shifts every barline) and removing a change.
2. **The splits of the form** (R9): an old barline that carries a repeat, a volta, a marker or a
   jump and is no barline of the grid splits the new bar holding it into two irregular bars, the
   second uncounted, as ADR-0025 §1 writes a mid-bar repeat.
3. **New bars** at the end of the region if the grid has more bars.
4. **The events** (R5, R6, R8), all read before anything moves (the new bars reuse the region's
   bar IDs, so reading an old bar after moves into it began would read moved events again: the
   first version did, and every test failed): each moves with one `Replace`; one that now crosses
   a barline becomes a tie chain inside its tuplet (`split_values`, Z5), its first part keeping
   its IDs and markings; its ties go on from the last part, and so do the slurs that ended on it;
   `After` graces stay at the end of their bar; whole-bar rests become written rests.
5. **The marks and the form markings** (R7, R9): removed, then inserted again at their new places
   (two phases, so that a mark never lands where another has not left yet).
6. **The meter map** of the region in one bulk edit (`MeterEdit::Span`, Z1), then the bars the grid
   no longer needs removed.

A bar repeat that meets the region refuses the rewrite (R10). Then the local check verifies every
cell and mark touched.

**Cost** (R11, `benches/editor.rs`):

```text
operation (heavy score)                       instructions
rewrite of 60 bars × 41 voices (≈ 14 000 events)   ≈ 118 000 000
the same before the fixes found by the profile     ≈ 200 000 000
rewrite of 10 bars of the normal score             ≈   5 300 000
```

The profile showed two waste sources, now gone: the cells of the change set (every moved event
recorded its two cells, and the list was sorted with thousands of duplicates: `ChangeSet::cell` now
drops a cell equal to one of the last two, and the editor deduplicates the cells before the local
check), and a scan of every new bar per event (now two binary searches). What remains is about 8 000
instructions per event moved. The owner accepted it for now: a rewrite of a long section of the
heavy score may take more than 16 ms; it will be measured in real time with the interface (F2.23).
The history holds 2.9 MB for the 60 bars (`tests/history_memory.rs`, estimate exact).

**Tests** (`src/meter_command_tests.rs`): nine hand-written cases (the reference case with the Fine
split and the D.S. at the end, the stop at the next meter change with an irregular last bar, whole-
bar rests cut and aligned, a slur end moved, a tie and an `After` grace kept, a pickup added and
kept, the bar repeat refusal), and `Rewrite` in the property test, half of whose cases now start
without the bar repeat of the rich document so that rewrites are carried out (12 000 sequences
passed). Seven injected bugs were caught, five of them only after a case was written for them (the
property test's documents had no tied split note, no `After` grace, no numbering restart, no aligned
whole-bar rest, no section ending before the next meter change).

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

`count_allocations` returns the result together with an `AllocationStats` with five counters:
allocations, reallocations, deallocations, bytes requested and bytes given back (a reallocation
gives back the old block and takes a new one). For example, from `tests/counting.rs`:

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

**How much memory a structure keeps** (F1.09c). `net_bytes()` is the bytes requested minus the
bytes given back over the measured window: the growth of the live heap, in which temporary buffers
cancel out. `core-model`'s memory test uses it to measure the bytes per note of the heavy score.
It counts what the program asks for, not the allocator's rounding: a lower bound of the resident
memory, the same on every platform. The extra counter makes the counter's own benchmarks slower
(snapshots ≈ 1 167 → 1 423 instructions, one counted allocation ≈ 7 641 → 8 025): a test-only cost,
accepted with the `Bench-regression: accepted` trailer of ADR-0017.

```rust
let (document, stats) = count_allocations(build_the_heavy_score).unwrap();
// What the document keeps: allocated minus given back, temporary buffers cancel out.
let per_note = stats.net_bytes() / i128::try_from(noteheads(&document)).unwrap();
assert!(per_note <= CEILING_BYTES_PER_NOTE);
```

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
cargo xtask bench --compare main  # same, plus the regression gate against main (ADR-0017)
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
counting::counting::snapshots
  Instructions:                                       1167|1167                 (No change)
counting::counting::allocations_of_64_bytes
  Instructions:                                       7641|7641                 (No change)
counting::counting::empty_measurements
  Instructions:                                      10067|10067                (No change)
```

Each benchmark runs its operation on a **batch of 64** inputs (the counts above are for 64
operations: divide by 64 for one). A single operation is a few dozen instructions, and one
instruction more or less can appear when *unrelated* code in the same binary changes (the
compiler inlines and lays out code differently): on a 15-instruction benchmark that would already
be 7%, enough to trip the regression gate described below. Batches put the counts in the
thousands, where such drift is a fraction of a percent.

The benchmarks of the rational type (`crates/core-time/benches/rational.rs`, step F1.03b) cover
every operation that ADR-0015 §9 names. Their inputs are `const` arrays computed at compile time
(the constructors are `const fn`), read through `black_box` so that the compiler cannot compute
the results in advance too. An excerpt of a run:

```text
rational::rational::add_same_denominator
  Instructions:                                      12686|12686                (No change)
rational::rational::add_wide_intermediate
  Instructions:                                      77147|77147                (No change)
rational::rational::cmp_different_denominators
  Instructions:                                       1410|1410                 (No change)
```

The first numbers, per operation (count ÷ 64), on the development machine:

| Benchmark | Instructions per operation |
|---|---|
| `add_same_denominator` | ~198 |
| `add_different_denominators` | ~233 |
| `add_wide_intermediate` (intermediates beyond 64 bits) | ~1205 |
| `sub_different_denominators` | ~232 |
| `mul` | ~196 |
| `div` | ~207 |
| `cmp_same_denominator` | ~18 |
| `cmp_different_denominators` | ~22 |
| `new_with_reduction` | ~99 |
| `overflow_errors` (one sum + one product that overflow) | ~910 |

Three things they show. Comparisons are an order of magnitude cheaper than arithmetic, as
expected (no gcd). The equal-denominator fast path of the sum saves less than hoped, because the
final reduction (a gcd) dominates the cost of both paths. And the 128-bit gcd costs about five
times the 64-bit one, which confirms the fast path of ADR-0015 §9. None of this needs action
today: these numbers are the reference against which future changes are judged, and possible
optimisations are noted in the ROADMAP.

The case "beyond 64 bits" needs carefully built inputs: `x = a/(G·P)` and `y = c/(G·Q)` with
`G = 2³⁰` and two primes `P`, `Q` just below 2²⁰, so that the common denominator is about 2⁷⁰.
`c` is chosen (with a modular inverse computed at compile time) so that the numerator of the sum
is a multiple of `G`, and the reduced result has a denominator of about 2⁴⁰, which fits. All 64
inputs were checked with an independent script before being used.

**The regression gate (ADR-0017).** `cargo xtask bench --compare <rev>` measures two versions of
the code in the same run: first the **merge base** of `HEAD` and `<rev>` (for a branch and
`main`: the commit of `main` the branch started from), then the working tree, and fails if an
instruction count grew by more than **5%**. The logic is in `xtask/src/bench.rs`:

- the base commit is exported with `git archive <commit> | tar -x` into `target/bench-base/`
  (no `git worktree`, which would write into `.git/`, read-only in the sandbox), and built in its
  own `target/bench-base-target/`. Sharing the usual `target/` would compile the dependencies
  only once, but it is wrong: cargo gives the artifacts of the two trees the same names (they are
  derived from paths relative to the workspace root), and in the first CI run of F1.03b its
  freshness check, based on file dates, ran the base commit's benchmark binaries for the current
  tree;
- its numbers are saved as the Gungraun baseline `base` in a fresh `target/gungraun-compare/`
  (`GUNGRAUN_SAVE_BASELINE`), then the working tree runs with `GUNGRAUN_BASELINE=base` and
  `GUNGRAUN_CALLGRIND_LIMITS=ir=5%`;
- a benchmark that the base does not have passes (nothing to compare), and so does every
  improvement;
- if the `HEAD` commit message has the trailer `Bench-regression: accepted` (with the reason in
  the message), the comparison is still printed but no limit is applied.

Why the merge base and not the tip of `main`: a branch must be judged on its own changes; if
`main` moved on, its tip contains other people's work. Why measure both in the same run: counts
are deterministic for a given binary, but they depend on the compiler, Valgrind and the machine,
so only two measurements taken together are comparable. To check that the gate works, a
regression was introduced on purpose during development (the snapshot benchmark taking two
snapshots instead of one):

```text
counting::counting::snapshot
  Instructions:                                         37|15                   (+146.667%) [+2.46667x]
Performance has regressed: Instructions (15 -> 37) regressed by +146.667% (>+5.00000%)
xtask: `cargo bench --workspace --locked --bench *` failed (exit status: 3)
If an instruction count grew by more than the limit (ir=5%) on purpose, explain why in the commit message and add the trailer `Bench-regression: accepted` (ADR-0017).
```

Measuring the working tree, not only `HEAD`, means the gate can be tried before committing:
`cargo xtask bench --compare HEAD` compares uncommitted changes with the last commit. In CI the
gate runs on every branch except `main`, and the `bench (instruction counts)` job is a required
check of the `main` ruleset, so a branch that regresses cannot be merged without the trailer.
The price is small: in the CI runs of F1.03b the job took about 49 s instead of 35 s.

A benchmark whose meaning changes gets a new name. In F1.03b the three benchmarks of the
counter became batches and were renamed (`snapshot` → `snapshots`, and so on): Gungraun keys
baselines by name, so under the old names the comparison would have reported a meaningless
+7000%.

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
  uploads them (with one `summary.json` per benchmark) as the `benchmarks` artifact. On every
  branch except `main` it runs `cargo xtask bench --compare origin/main` instead, the regression
  gate of ADR-0017 (with the whole git history, `fetch-depth: 0`, because the merge base must be
  available); the summary page then shows only the comparison. On Windows and macOS the **check** job runs `cargo xtask bench` too, which there only compiles
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

`spikes/` is a separate Cargo workspace with time-boxed prototypes: those of Phase 0, Step 13 and
the model storage prototype of step F1.06. It is excluded from the main workspace
(`exclude = ["spikes"]` in the root `Cargo.toml`), never built by `cargo xtask check` or CI, and
never shipped: its job was to measure and decide. The decisions and all the numbers are in
ADR-0011, ADR-0012 and ADR-0022 (with `spikes/model/ANALYSIS.md`); this section explains how to run
the spikes again.

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

### The model storage spike

`spikes/model` (step F1.06) compares three ways to store the events of a voice, with the real
`core-time` types: P1, one vector per voice sorted by absolute position; P2, one chunk per bar
with offsets relative to the bar, bars named by index; P3, the same chunks keyed by stable bar IDs
behind `Arc`. `src/lib.rs` generates two deterministic scores (100 bars × 10 staves, 8 690 notes;
1 000 bars × 30 staves, 255 776 notes) and the meter map edits, `tests/agree.rs` checks that the
three hold the same music after every operation, `benches/model.rs` counts instructions and
`src/bin/report.rs` measures memory, allocations and wall-clock times:

```sh
cd spikes
cargo test --release -p model
GUNGRAUN_ENVS=TMPDIR cargo bench -p model
cargo run --release -p model --bin report
```

The `spikes` workspace strips release binaries, so its `[profile.bench]` keeps symbols: Callgrind
finds the measured function by name, and without symbols every count is 0. The numbers and the
choice of P3 are in `spikes/model/ANALYSIS.md` and ADR-0022.

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
