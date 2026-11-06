# ADR-0029 — Project file format v1: plain JSON, a schema of its own, strict loading

- **Status:** Accepted (owner's decision, 2026-11-06; SCALETTA F1.15, decision D10). The owner
  approved S1–S14 as recommended (one plain JSON file in version 1, a 1 GiB file limit), after
  confirming that compression can come later as an ordinary format version (S2), and added two
  precisions: the version-1 reader already recognises compressed containers (S5), and user
  settings are always written, only structural absences are omitted (S6).
- **Date:** 2026-11-06
- **Completes:** ADR-0010 (its "Proposed part": the concrete format). ADR-0010's accepted policy
  (format version in a header, migrations `vN → vN+1`, golden files, refusal of newer files,
  unknown fields never silently dropped) is kept unchanged and made concrete here.
- **Amends:** nothing.

## Context

ADR-0010 fixed the policy and left the concrete format to Phase 1, "when the model types exist".
They now exist: `core-model` holds the whole document (ADR-0022, ADR-0024, ADR-0025, ADR-0026,
ADR-0028). What must be decided:

1. the encoding (JSON, RON, XML, binary, a database, a format of our own);
2. the container (one file, a zip, a folder), and where audio and video will go later;
3. how the file schema relates to the model's memory layout;
4. how the version is read before the rest, and what happens to unknown or missing data;
5. how exact values (rationals, pitches, note values) and IDs are written;
6. what is saved and what is derived again at load;
7. migrations, golden files, atomic saving, the extension;
8. two items left open since F1.08: a cap on the number of tracks at load, and whether meter or
   tempo changes beyond the last bar are refused.

The criteria are those of ADR-0010: robustness, load time (≤ 1 s for 100 bars on the reference
laptop, ADR-0007), diffability, size, ease of migration, cross-platform paths.

### Measurements (spike `spikes/fileformat/`, details in its `RESULTS.md`)

The real normal score (100 bars, 10 staves, 7 444 noteheads) and heavy score (1 000 bars, 30
staves, 221 169 noteheads) of `core-model`'s tests, read through the public API into a file schema
and encoded. Native times on the development machine (the reference laptop is expected 2–3×
slower):

| | normal: size / read | heavy: size / read |
|---|---|---|
| JSON, readable values (`"C4"`, `"q"`, `"3/16"`) | 0.43 MB / 3.4 ms | 13.7 MB / 100 ms |
| JSON, structured values (`{"step":"C",...}`) | 1.09 MB / 5.9 ms | 35.0 MB / 213 ms |
| JSON, fully pretty-printed | 3.60 MB / 7.7 ms | 112.6 MB / 245 ms |
| RON | 0.93 MB / 29 ms | 28.3 MB / 1 038 ms |
| postcard (binary) | 0.18 MB / 2.3 ms | 6.0 MB / 64 ms |

Building the same documents with the real builder takes 3.0 ms and 86 ms: loading is dominated by
nothing in particular, and every candidate meets the 1 s budget for 100 bars by two orders of
magnitude. Compression (deflate, as in a zip) shrinks JSON 7–9× (heavy: 1.9 MB) at 50 ms (level
1) to 650 ms (level 6) when writing and ≤ 50 ms when reading. Two qualitative findings weigh more
than the numbers: RON's generic tree **cannot read our enums back**, so migrations on it are not
possible; postcard is **not self-describing** (an omitted field cannot be read back, and a
migration needs the old typed schema).

### What others do (from their documentation and file layouts)

- **MuseScore**: `.mscz` is a zip holding the score as XML (`.mscx`) plus thumbnails and settings;
  an uncompressed `.mscx` can be saved for version control.
- **Ardour**: an XML session file in a session folder; recorded and imported audio in a media
  subfolder, referenced by relative path.
- **REAPER**: `.rpp` is a plain-text project; media files are referenced, not embedded.
- **Audacity 3**: `.aup3` is one SQLite database, audio included.

Notation programs (small documents) favour one self-contained file; DAWs (gigabytes of audio) keep
media **beside** the project, never rewritten at every save.

## Decision (proposed)

### S1 — Encoding: JSON (RFC 8259), with `serde` and `serde_json`

UTF-8 JSON, read and written with `serde` 1.0.229 and `serde_json` 1.0.154 (checked on crates.io on
2026-11-06: MIT OR Apache-2.0, actively maintained, the most used Rust crates of their kind;
`serde_json` needs only `itoa`, `memchr`, `zmij`, `serde_core`; `serde_derive` brings `syn`,
`quote`, `proc-macro2`, already in our lock file through Gungraun's macros, so compile time grows
little). The release binary size added will be measured in F1.15b.

JSON is self-describing (migrations can work on the generic tree, S11), readable in any editor,
diffable with git, checkable with standard tools (`jq`), and the fastest text option measured.

Rejected: **RON** (3–5× slower, generic tree loses enums, Rust-only tooling); **binary** formats
(postcard, bincode, CBOR, MessagePack: not diffable or inspectable, harder migrations; their speed
is not needed); **XML** (larger and slower, no gain over JSON for our own data; MusicXML stays an
import/export format); **SQLite** (a C library, CLAUDE.md §6, and not diffable; worth reconsidering
only for media metadata, if ever); **a text format of our own** (a parser to write, test and
maintain forever, for no measured benefit); **facet** and other young reflection crates (not mature
enough, 0.x).

### S2 — Container in version 1: one plain JSON file, nothing else

Version 1 is a **single, uncompressed UTF-8 JSON file**. The model holds no media yet, so a
container would only hold one entry. The cost is size (normal score ≈ 0.5 MB, heavy ≈ 15–20 MB with
the layout of S3 and readable keys), acceptable on an SSD and for e-mail at normal sizes; the gain is a file that git diffs line
by line, golden files that can be read in a review, no zip dependency (`zip` 9.x requires
`getrandom`, `nix`, `indexmap`, `typed-path`, `windows-sys` even without features) and no
compression time at save.

The format already leaves the door open: the loader recognises the file by its first bytes (S5),
so a later version in a zip (first bytes `PK`) is an ordinary format version with its migration.
**Audio and video (F3, F5)** come with their own ADR; the direction recommended now, from the DAW
practice above, is media **beside** the project file in a project folder, referenced by relative
paths, never rewritten at every save.

Alternative: **a zip from day one** (like MuseScore), 7–9× smaller, at the price of the dependency
(or our own minimal zip reader and writer), of git diffs and of 50–650 ms per save on the heavy
score. Recommended only if small files matter more to the owner than diffability now.

### S3 — Layout: deterministic, one record per line

The writer is **deterministic**: the same document gives the same bytes (golden files, clean
diffs). Every list is written in the model's own stable order (bars in score order, events in chunk
order, marks by point then ID, ADR-0026 §2). The text is laid out with a small custom
`serde_json` formatter: the structure is indented, and **each record** (an event, a mark, a slur, a
tuplet…) sits **on one line**, so editing a note changes one line of the diff. The size is close to
the compact form (the measured fully pretty form is 3× larger). LF line endings, no BOM, a final
newline. The reader accepts any JSON whitespace (CRLF included, e.g. after a Windows checkout) and
ignores a leading UTF-8 BOM (some editors add one).

### S4 — The file schema is separate from the model's memory layout

The schema is a set of plain types in `orchestra-io` (`io::project::v1`), with the serde derives
**only there**. `core-model` stays free of `serde` and gains two serde-free entry points:

- reading: the existing public read API, plus what is missing to see every saved field (e.g. the
  ID counter's next value, pass-offset tables, form lists);
- loading: `Document::from_parts(DocumentParts) -> Result<Document, LoadError>`, where
  `DocumentParts` is a plain public description of everything saved (S7) with raw `u64` IDs. It
  restores the IDs exactly (ADR-0022 §1: IDs are saved), checks that they are unique and of the
  right kind and that the counter is above all of them, runs the full `validate`, rebuilds every
  derived datum (tempo map, indices), and fails as a whole.

Why: a refactoring of the model (a field renamed, a struct split, a new cache) must **never**
change the file by accident; with derives on the model it would, silently, breaking ADR-0010's
migration discipline. A change of the file is then always a deliberate edit of `io::project`, with
a version bump and a migration. The price is a conversion layer (one function each way per type),
written once and covered by the round-trip tests (S12).

Rejected: serde derives in `core-model` (couples format and memory layout, and deserialisation
would bypass the model's constructors); loading through `DocumentBuilder` (it issues new IDs, and
ADR-0022 requires that IDs survive saving).

### S5 — Identification and version before the rest

The top-level object's **first member** is `"orchestra_format": 1` (the format version, an integer
independent of the program's version, ADR-0010 §1), followed by `"written_by": "Orchestra 0.1.0"`
(informational only, never used to decide anything). The reader first runs a tiny prefix scanner
(whitespace, `{`, that key, `:`, an integer) on the first bytes, without parsing the rest:

- prefix missing → `LoadError::NotAProject`;
- version > supported → `LoadError::NewerFormat { found, supported }` ("made with a newer
  Orchestra", ADR-0010 §4), nothing loaded;
- version = current → typed reading (fast path);
- version < current → migrations (S11);
- the first bytes are a **zip** (`PK\x03\x04`) or **gzip** (`\x1F\x8B`) signature →
  `LoadError::NewerFormat` too, not `NotAProject`: a later version may put the document in a
  compressed container (S2), and an older Orchestra opening such a file must say "made with a newer
  Orchestra", which is true, instead of "not a project", which would mislead. The check costs a
  comparison of four bytes (owner's precision, 2026-11-06).

Because the writer always puts the key first, a hand-edited file that moves it is refused as "not a
project": strict, simple, and stated in the error message.

### S6 — How values are written

- **No floating-point numbers, ever**: everything stored is exact (ADR-0005). The only floats of the
  model are computed (inside gradual tempo changes) and never stored.
- **Rationals** as canonical strings: `"3/16"`, `"-1/8"`, integers without denominator (`"0"`,
  `"2"`). Canonical = reduced, positive denominator; the reader **refuses** non-canonical forms
  (`"2/4"`, `"3/1"`, spaces), so that one value has one spelling and diffs stay stable. This is
  also the textual form of `Rational` deferred by F1.03: `Display` and a strict `FromStr` in
  `core-time`, with typed errors.
- **IDs** as JSON integers (`u64`). Values above 2^53 would lose precision in JavaScript-based tools
  such as some `jq` builds, never in our reader; a counter that large is not a practical concern.
- **Pitches** as one string: letter, alteration, scientific octave: `"C4"`, `"F#5"`, `"Bb3"`,
  `"Cx4"` (double sharp), `"Dbb2"`, `"G#x3"` and `"Abbb3"` (triple), `"C-1"`; a non-chromatic
  alteration in brackets with its exact semitones: `"E[-1/2]4"` (a quarter-tone flat, ADR-0023).
- **Note values** by their usual numbers, with dots: `"4"` quarter, `"8."` dotted eighth, `"1"`,
  `"2"`, … `"1024"`, and the words `"breve"`, `"longa"`, `"maxima"` (as LilyPond does). Numbers are
  language-neutral; the measured compact values halve the reading time.
- **Enumerations** (clefs, articulations, modes, dynamics…) as lower-case English identifiers
  (`"treble"`, `"staccato"`, `"dorian"`), each with an **explicit** name in the schema: renaming a
  Rust variant never changes the file. They are file identifiers, never shown to the user, never
  translated.
- **Free texts** (instrument names, text tempo marks, jump texts) as JSON strings, as typed, any
  language.
- **Structural absences omitted, user settings always written** (owner's precision, 2026-11-06).
  Two kinds of default are told apart:
  - a field that only says that **something is absent** (no dots, no tie, no fermata, no
    ornament, no grace, no tuplet, zero performance offsets, an empty list…) is **not written**
    when absent, and the reader fills the absence in: `{"id":145,"pitch":"C4"}` instead of
    `{"id":145,"pitch":"C4","tie":"none","offsets":{"onset":"0","length":"0","velocity":"0"}}`.
    Files get smaller and a diff shows only what was really set. "Absent" can never change
    meaning, but the rule stays: changing what an omitted field means is a format change (new
    version and migration);
  - a **setting the user can choose** whose program default may be revised (the tolerance
    threshold of ADR-0028, 1/32 today and still to be validated with users; the bar-numbering
    style; every future project option) is **always written**, even at its default value. If the
    program's default changes one day, every saved project keeps the value it was written with,
    with no migration needed.

### S7 — What is saved, and what is not

Saved, all of it with its IDs: the ID counter's next value; the meter map (segments, irregular and
free bars, beat groupings, numbering exceptions); the bar list; the initial tempo mark and the
tempo marks; the initial key and the key changes; the form (repeat barlines, voltas, markers, jumps,
bar-numbering style); the tracks with their instrument (name, transposition, key overrides, staves
with initial clef and clef changes, voices with home staff); per voice the bar chunks with their
events and the tuplet table; per instrument slurs, dynamics, hairpins, bar repeats, pass rules and
pass-offset tables; the tolerance settings (ADR-0028).

Not saved, derived again at load: the tempo map (from the tempo marks), every index (events, notes,
slurs, backward ties), the revision number. Not saved by decision: the undo history (D08, ADR-0026),
the playback results (`Unrolled`, `Performance`), anything of the interface (window layout, zoom,
selection: a later, separate and optional section if ever wanted).

The file is organised by **owner**, mirroring the model: `document` (counter, maps, marks, form,
bars), `tracks` (each with its instrument, staves, voices and, per voice, `cells` keyed by bar ID),
`settings` (tolerance). Version 1 carries no project metadata (title, composer): the model has none
yet; it will come with a format version of its own.

### S8 — Strict loading: refuse, never guess, never load half

- **Unknown fields are refused** (`deny_unknown_fields`) with an error naming the JSON path
  (ADR-0010 §5: never silently dropped). Within a version an unknown field can only be a bug or a
  corruption; every addition is a new version.
- Missing required fields, wrong types, non-canonical values, unknown enum names → typed errors with
  the path.
- After parsing, `Document::from_parts` (S4) checks the IDs and runs the full `validate`. Any error
  refuses the whole file; nothing is ever partially loaded (ADR-0010 §4).
- `LoadError` variants are typed and English for developers; the interface maps them to Fluent keys
  (as for `RationalError`, F1.03).
- The reader never panics, whatever the input (tested on truncated and corrupted files, S12).

### S9 — Limits at load (open item from F1.08)

Against corrupt or hostile files, beside the existing 16 staves and 256 voices per instrument: at
most **1 024 tracks**; a file larger than **1 GiB** is refused before reading; `serde_json`'s
default nesting limit (128) is kept. These are technical limits, not musical ones, and every one has
its typed error. The numbers are a proposal: the heavy score has 28 tracks and ≈ 15–20 MB.

### S10 — Meter and tempo changes beyond the last bar (open item from F1.08)

A meter-map entry starting at or after the last bar's end is **refused** by a new rule 13 of
`validate`, so a file cannot carry one. Tempo marks already cannot: they live at (bar ID, offset).
F1.15b adds to the command property test the check that no command produces such an entry (deleting
the last bars must take their meter changes along, as ADR-0026 already requires).

### S11 — Migrations on the generic tree

A file of the current version is read straight into the typed schema (the fast path measured
above). An older file is read into a `serde_json::Value`, passed through the chain of migrations
`fn(Value) -> Result<Value, MigrationError>` (`v1 → v2`, `v2 → v3`, …, ADR-0010 §2), then into the
current typed schema. Only one typed schema exists, the current one: old versions live on as their
migrations and their golden files. The tree path measured ≈ 3× slower (heavy: 0.31–0.72 s), paid
only on the first opening of an old file, after which it is saved in the current version.

### S12 — Golden files and tests

- `crates/io/tests/golden/v1/`: hand-reviewed `.orchestra` files committed once and **never
  regenerated** (ADR-0010 §3): one small file per object kind and one "everything" file using every
  kind; a test loads each and compares it with the document the same test builds by code, IDs
  included.
- Round trip: a property test on documents made by random command sequences (the existing
  generators): `load(save(d))` has the same content and the same IDs, and `save(load(f)) == f` byte
  for byte.
- Robustness: every truncation of a golden file, random byte flips and random JSON values never
  panic and always give a typed error.
- Benchmarks (ADR-0017): save and load of the normal and heavy scores, in instructions; budget: the
  normal score's load ≤ 1 s on the reference laptop with a wide margin (measured here ≈ 10 ms).

### S13 — Saving safely, cross-platform

Saving writes a temporary file in the **same directory**, flushes and syncs it, then renames it over
the target (`std::fs::rename`: an atomic replacement on Linux and macOS; on Windows a replacing
rename, atomic on NTFS in practice but not guaranteed by the documentation, to be tested there in
F1.15b); on Unix the directory is synced too. A failed save leaves the
previous file intact and the temporary file removed. Version 1 contains **no file paths**; the rules
for paths (relative, `/` separators, UTF-8, Unicode NFC, case) come with the media ADR. Autosave and
crash recovery stay in F2.14 (D13).

### S14 — Extension and split of the step

- Extension **`.orchestra`** (`Name.orchestra`): explicit, no clash with `.orc` (Csound's orchestra
  files, rejected) or `.orch`; a MIME type and the desktop registrations come with packaging (F6),
  where the extension is also checked against public registries.
- The step is split as approved in the briefing: **F1.15b** — the schema, `from_parts` and the read
  API, the textual `Rational`, rule 13 and the limits, save and load with atomic saving, round-trip
  and robustness tests, benchmarks; **F1.15c** — the golden files of version 1, the migration
  machinery (tested with a test-only `v1 → v2`), the documentation, the PR.

## Alternatives rejected

Listed with each choice: RON, binary encodings, XML, SQLite, a format of our own (S1); a zip in
version 1 (S2, kept as the main alternative); serde derives on the model, loading through the
builder (S4); keeping or dropping unknown fields (S8); one typed schema per version (S11: more code
for no benefit while migrations on the tree work).

## Consequences

- `orchestra-io` gains `serde` and `serde_json`; `core-model` gains `DocumentParts`,
  `Document::from_parts`, `LoadError` and read accessors, but no serialisation dependency.
- `core-time` gains `Display`/`FromStr` for `Rational` (the canonical text form).
- Every future change of the saved data is a deliberate edit of `io::project`, a version bump, a
  migration and a new golden directory; a changed documented default is such a change too.
- Files are diffable and reviewable, at the price of ≈ 0.5 MB for 100 bars; a zip can come later as
  a normal version change, and media will live beside the file.
- The loader is strict: a hand-edited file with a typo is refused with a message naming the place,
  never half-loaded.
