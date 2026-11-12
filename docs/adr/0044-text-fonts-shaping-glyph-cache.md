# ADR-0044 — Text: bundled fonts, shaping, measuring and a glyph cache

- **Status:** Accepted (owner, 2026-11-12; outline TX1–TX12 approved first, then the text with
  TX10's table of cases)
- **Date:** 2026-11-12
- **Implements:** ADR-0012 (bundled fonts, `harfrust` shaping, our own glyph cache), ADR-0006
  (same look everywhere, fonts inside the program), ADR-0036 B2–B3 (`skrifa` + `harfrust`, no
  hinting), ADR-0037 (text tokens are designed here), ADR-0043 (the glyph cache is built for
  `tiny-skia`).
- **Amends:** ADR-0036 B1 (the versions of `skrifa` and `harfrust`), ADR-0039 D4 (a new command,
  the glyph run), `docs/ARCHITECTURE.md` (a new crate, `orchestra-text`).

## Context

SCALETTA step F2.07 gives the interface its text: bundled fonts, shaping, a glyph cache,
measuring, and Bravura for musical symbols. It is one of the structures that are expensive to
change later, because every widget, the score and the DAW timeline will measure and draw text
through it. Two earlier measurements frame it:

- **The ADR-0012 spike:** 2000 Bravura glyphs drawn as shaped text at every frame took 243 ms,
  as cached outline paths 28.8 ms. Shaping and outline extraction must therefore never run when a
  frame is drawn.
- **The F2.01 spike** (`spikes/gui-blocks`): `harfrust` shapes the same `skrifa` font object
  that draws the outlines (one parse serves both), it shapes in font units only (the size is
  applied by us, `size / units_per_em`), and both Inter and Bravura are CFF fonts, so `skrifa`'s
  CFF path is the one we use. The picture was correct by eye: kerning, ligatures, accents, SMuFL
  glyphs.

Facts checked on crates.io on 2026-11-12:

| Crate | Latest | Published | License | Uses |
|---|---|---|---|---|
| `skrifa` | 0.50.0 | 2026-11-10 | MIT OR Apache-2.0 | `read-fonts` 0.47 |
| `harfrust` | 0.16.0 | 2026-11-12 | MIT | `read-fonts` 0.47 |
| `read-fonts` | 0.47.0 | 2026-11-10 | MIT OR Apache-2.0 | — |
| `unicode-linebreak` | 0.1.5 | 2023-07-25 | Apache-2.0 | no dependencies; Unicode 15.0 |
| `icu_segmenter` | 2.4.0 | 2026-10-22 | Unicode-3.0 | ICU4X data, 4.4 MB crate |

ADR-0036 B1 had pinned `skrifa` 0.49 + `harfrust` 0.15 (both on `read-fonts` 0.46); the new pair
shares `read-fonts` 0.47, so it is consistent.

Other facts that shape the design:

- Our `orchestra-i18n` keeps Fluent's default of wrapping placeables in the Unicode isolation
  marks U+2068 … U+2069 (`crates/i18n/src/lib.rs`). Inter has no glyphs for them: if the shaper
  did not treat them as default-ignorable they would show as boxes.
- `orchestra-notation-layout` (F2B) must read Bravura's metrics and outlines to engrave, and by
  `docs/ARCHITECTURE.md` it may depend only on `core-time` and `core-model`, never on `ui`.
- The six interface languages (en-US, en-GB, it, fr, de, es) are all left-to-right Latin script,
  fully covered by Inter. User content (track names, titles, later lyrics) can contain anything.
- The bundled fonts in `assets/fonts/` total about 8 MB; the program must not carry them all.

The owner approved the outline TX1–TX12 below on 2026-11-12, then accepted the text the same
day, after asking for TX10 to be explained in full.

## Decision

### TX1 — Versions: `skrifa` 0.50 + `harfrust` 0.16

ADR-0036 B1 is amended: the pair is `skrifa` 0.50.0 + `harfrust` 0.16.0 (both on `read-fonts`
0.47), with the features of ADR-0036 B2 unchanged (`default-features = false, features =
["std"]`). `harfrust` 0.16.0 was published the day this ADR was written, so the pair is
**checked again on crates.io at the start of F2.07b** and a newer patch release taken if one is
out (a patch release does not need a new ADR). The two crates are still updated together, never
one alone (ADR-0036 B1).

### TX2 — A new crate, `orchestra-text`; rasterising stays in `ui`

The workspace gains `crates/text` (`orchestra-text`), with **no internal dependencies** (it sits
at the bottom, beside `core-time`). It owns:

- the bundled font data (TX3) and the parsed faces;
- shaping (`harfrust`), line breaking (TX10) and the resulting **text layout** (TX9);
- measuring: advances, line metrics, the extent of a layout, and the mapping between text
  positions (clusters) and glyph positions, which the text field of F2.10 needs for the caret
  and the selection;
- glyph outlines (`skrifa`) delivered to an outline sink of its own (move, line, quadratic,
  cubic, close), so it knows nothing of `tiny-skia`;
- SMuFL glyph lookup by code point in Bravura, without shaping (a direct `cmap` lookup).

`orchestra-ui` (its module `draw`) owns **rasterising and the glyph cache** (TX8), because it is
the only place that knows `tiny-skia` (ADR-0039 Consequences). `orchestra-notation-layout` will
depend on `orchestra-text` for Bravura in F2B. `docs/ARCHITECTURE.md` and the crate list of
CLAUDE.md §2 gain the crate (the latter after the owner's confirmation, since CLAUDE.md is the
rules file).

Rejected: everything in `ui`, with `notation-layout` reading Bravura on its own: two readers of
the same fonts, two sets of metrics that could disagree, and a dependency of the engraving on a
second copy of the font data.

### TX3 — Which fonts are inside the program

Embedded with `include_bytes!` from `assets/fonts/`, unchanged (the OFL forbids modified
copies under the reserved names; we never subset them):

| Face | File | Size | Use |
|---|---|---|---|
| Inter Regular | `inter/Inter-Regular.otf` | 610 KB | interface text |
| Inter SemiBold | `inter/Inter-SemiBold.otf` | 630 KB | labels, emphasis |
| Inter Bold | `inter/Inter-Bold.otf` | 632 KB | headings |
| Bravura | `bravura/Bravura.otf` | 889 KB | musical symbols, in the interface and the score |
| JetBrains Mono Regular | `jetbrains-mono/JetBrainsMono-Regular.ttf` | 274 KB | code-like text (paths, error details) |

About **3.0 MB** of font data in the binary. Not embedded: Inter Italic, Bravura Text (3 MB:
musical symbols inside text use Bravura, positioned by us), Source Serif 4 and the other
JetBrains Mono styles, which stay for the guides only. Adding a face later is a small change
recorded in this ADR's successor, never a silent one (binary size, CLAUDE.md §5).

The faces are a closed set known at compile time (an enum), parsed once when the program starts
(a parse reads only the table directory; tables are read lazily), and shared by reference.

### TX4 — Characters the fonts do not have

A character missing from the face of its run is drawn as the face's **`.notdef` glyph** (glyph
0, a box in Inter), with its advance. There is **no fallback font** for now. This is a recorded
limit: a Chinese or Arabic track name shows boxes. A bundled fallback (for example a subset of
the Noto family) is evaluated when a user need appears, with its size measured; emoji and colour
glyphs stay out.

### TX5 — Left to right only

No bidirectional text now: none of the six languages needs it. A Hebrew or Arabic string is
shaped and drawn left to right (with boxes, TX4). Recorded limit, together with TX4. The layout
type keeps the direction as a field set to left-to-right, so adding bidi later does not change
its shape.

### TX6 — No hinting, grayscale anti-aliasing

Outlines are drawn **unhinted** (ADR-0036 B2) and anti-aliased **in grayscale**: no LCD
sub-pixel rendering, which depends on the screen's sub-pixel order and orientation and would
make the same text differ between screens and screenshots. Glyph coverage is blended with the
paint like any other fill (D3 tokens, with the extra opacity).

**Gamma, open for F2.07b (owner, by eye):** blending coverage directly in sRGB values, as all our
other shapes do, is the default. Text drawn that way may look thin in light-on-dark themes. In
F2.07b the owner compares screenshots of (a) plain coverage and (b) a fixed contrast correction
of the coverage (a 256-entry table, the method of FreeType's and Skia's text gamma) in a light
and a dark theme, and chooses; the choice is recorded in this ADR's precisions.

### TX7 — Positioning: a quarter of a pixel across, whole pixels down

At draw time a glyph's horizontal position in device pixels is rounded to the nearest **quarter
pixel**: each glyph has at most four rasterised variants, and text spacing stays even at every
scale. The **baseline** is rounded to a whole device pixel, so a line of text sits on the pixel
grid like the snapped lines of ADR-0039 D7. Layout itself (TX9) works in unrounded logical
pixels; only drawing rounds.

### TX8 — The glyph cache

The `tiny-skia` backend keeps rasterised glyphs between frames:

- **Key:** face, glyph ID, size in device pixels per em (quantised to 1/64 of a pixel, the
  26.6 fixed point FreeType uses), horizontal quarter-pixel offset (0–3).
- **Value:** an 8-bit coverage mask with its offset from the glyph origin, rasterised once from
  the `skrifa` outline by `tiny-skia` with anti-aliasing.
- **Bound:** **8 MB** of masks in total; when a new mask would exceed it, the least recently
  used masks are evicted. A glyph whose mask would be larger than **256 × 256** device pixels
  (a strong zoom on the score) is not cached: it is filled directly as a path from its outline,
  the same way shapes are drawn.
- **Use:** blending a cached mask honours the clip, the transform's translation and the damage
  exactly like the other commands. Drawing from the cache and drawing the same glyph freshly
  give the **same bytes** (tested).
- **Allocation:** filling the cache allocates; a frame whose glyphs are all cached does not
  (tested, ADR-0016).

### TX9 — Shaping before recording; the glyph run command

Text is shaped **once**, outside drawing:

1. The caller builds a **`TextLayout`** in `orchestra-text` from a string, a style (face, size
   in logical pixels, OpenType features such as `tnum`) and an optional maximum width. The
   layout holds the positioned glyphs (face, glyph ID, x and y in logical pixels relative to the
   layout's origin), its lines with their metrics, its extent, and the cluster mapping (TX2). A
   layout can be rebuilt in place: rebuilding one of the same size does not allocate (its
   vectors and `harfrust`'s buffers are reused). Shape plans are kept per face and feature set.
2. The caller records it into its segment with the recorder: a new command, **`GlyphRun`**
   (ADR-0039 D4 gains it), with the face, the size, the origin, the paint (a colour token, D3)
   and a range of a **glyph arena** in the segment, like the path arena of D1. Recording copies
   glyphs into the arena: no allocation per run once the segment's capacity is reached.
3. The run's bounds are its glyphs' advance extent widened by the face's bounding box (from the
   `head` table), so damage and culling (D5) never cut a glyph that overhangs its advance.

Consequence for themes: colours stay resolved at draw time (D3), so a colour change still only
redraws; but a change of a **text token** (TX11) changes sizes and faces, so it needs a new
layout and a re-recording of the segments that show text.

Shaping runs on the interface thread, never on the audio thread (ADR-0007). Each layout is one
run per style; the script and language are guessed from the text by `harfrust` (Latin, Greek and
Cyrillic, all in Inter). Default-ignorable characters, including Fluent's U+2068 … U+2069, are
hidden by the shaper (HarfBuzz's default behaviour, which `harfrust` ports: an invisible glyph
with zero advance) and produce neither ink nor advance; the cluster mapping still accounts for
them, so the caret skips them (tested).

### TX10 — Line breaking

A layout with a maximum width wraps its text into lines. The cases it must get right:

| Case | Example | Behaviour |
|---|---|---|
| Ordinary space | `Save the project` | a line may break there |
| **No-break** space (U+00A0, narrow U+202F) | French `Mesure ?`, `« Allegro »`; everywhere `120 BPM`, `4 bars` | never a break there: a lone `?` or `»` at the start of a line is a typographic error |
| Hyphen | `pre-roll` | a line may break **after** it |
| Soft hyphen U+00AD | German `Takt­art­wechsel`, with break points placed by the translator in the catalogue | invisible while the word fits; a visible hyphen only where the line breaks |
| Zero-width space U+200B | a break point with no visible sign | a line may break there |
| A word longer than the line | a file name, a URL, a German compound without soft hyphens | last resort: break at the last grapheme that fits |
| Combining marks | `é` as one character or as `e` + combining accent | never split a letter from its marks (breaks only between graphemes, as given by the shaper's clusters) |

Not in scope: **automatic hyphenation** with dictionaries (as typesetting programs do), which is
heavy and not needed for interface texts; German long words are handled with soft hyphens in the
catalogues.

**How it fits with shaping (TX9).** The paragraph is shaped **once**; the break opportunities
are found; the advances are summed by cluster, and when a line exceeds the width it is cut at
the last allowed opportunity. Spaces at the end of a line hang past the width and are not
counted. Only a line ending at a soft hyphen is shaped again, to add the visible hyphen. The cost
is linear in the length of the text, with no allocation when a layout is reused.

**Where the break opportunities come from.** The standard is the Unicode Line Breaking Algorithm
(UAX #14): a class per character and about fifty rules on pairs of classes, covering every
script (Chinese breaks between almost any two characters, Thai has no spaces). The candidates:

| | `unicode-linebreak` | `icu_segmenter` | Our own subset |
|---|---|---|---|
| Coverage | full UAX #14 | full UAX #14, plus dictionaries for Thai, Burmese and similar scripts | spaces, hyphens, soft hyphen, zero-width space; no-break spaces stay joined |
| Maintenance | stopped in **2023**, Unicode 15.0 | active (ICU4X, 2026-10) | ours |
| Size | small, no dependencies | large: the rules' data, and the dictionaries if used; **to be measured** | a few dozen lines |
| Where it errs | almost never (only recent rules for some Indic scripts) | almost never | Chinese and Japanese in user text: no spaces, so only the last-resort break |

**The rule:** a crate only if it is **maintained and of reasonable size** (CLAUDE.md §5, §10),
otherwise our own subset. `unicode-linebreak` fails the first criterion. In F2.07b
`icu_segmenter` is measured **without dictionaries** (binary size and clean build time) and the
owner chooses with the numbers: the full standard if it is light, our subset otherwise. The
subset is enough for the six interface languages, because their texts are ours and punctuation
is always attached to its word except in French, which uses exactly the no-break spaces. If the
subset is chosen, its limit (Chinese and Japanese in user text) is recorded with those of TX4
and TX5.

### TX11 — Text tokens

ADR-0037 gains the text tokens, customisable through the same theme modifications as the colour
and shape tokens, **choosing only among the bundled faces** (ADR-0006: the same look everywhere):

- **Families:** `ui` (Inter), `numeric` (Inter with tabular figures, `tnum`, for counters, bar
  numbers, times), `code` (JetBrains Mono), `music` (Bravura).
- **Sizes**, in logical pixels: `caption` 11, `small` 12, `body` 13, `subheading` 15, `heading`
  18, `title` 22.
- **Weights:** `regular` (400), `semibold` (600), `bold` (700), each mapped to the embedded face
  of that weight (TX3). A weight with no face in a family (JetBrains Mono has only Regular)
  falls back to that family's regular face.
- **Line height:** the face's own metrics (ascender, descender and line gap), from the `OS/2`
  typographic metrics when the face sets `USE_TYPO_METRICS` (checked on 2026-11-12: Inter and
  JetBrains Mono do; Inter's are 1984 / −494 / 0 on 2048 units, a line of 1.21 em), otherwise
  from `hhea`; not a token for now. Bravura's line metrics (±2012 on 1000 units) are not used
  for interface lines: musical symbols inside a line of text take the line's metrics.

The tokens are introduced in F2.07c; the widgets (F2.09) use them.

### TX12 — Tests and benchmarks

- **`orchestra-text`:** shaping of strings in the six languages (kerning pairs and ligatures of
  Inter, accents); the isolation marks give no glyph and no advance; a missing character gives
  `.notdef`; wrapping (German compound words, a word longer than the width, soft hyphens);
  cluster mapping round trips; known metrics of Inter and Bravura; rebuilding a layout of the
  same size does not allocate.
- **`ui`:** pixel tests of glyph runs (probes inside a stem and outside the glyph; cached against
  fresh drawing byte for byte; quarter-pixel variants differ; the baseline on whole pixels;
  clipped and partially redrawn runs), with the one-unit tolerance across platforms of ADR-0039
  D10; a frame drawn from a warm cache does not allocate; eviction keeps the cache under 8 MB.
- **Benchmarks** (Gungraun, instruction counts): shaping one line of about 40 characters;
  laying out a wrapped paragraph; drawing a frame with 200 labels from a warm cache and from a
  cold one; and ADR-0039 D10's reference scene with its noteheads and accidentals as Bravura
  glyphs from the cache, whose whole frame must stay within **33 M instructions** (if it does
  not, an ADR on the budget is opened, as SCALETTA F2.07 says).
- **Size:** binary size and clean build time with and without text, measured in F2.07b
  (CLAUDE.md §5).

## Alternatives rejected

- **Everything in `ui`** (TX2): see there.
- **Shaping at draw time**, the simplest API: 243 ms against 28.8 ms in the ADR-0012 spike.
- **A glyph atlas** (one big texture with all glyphs packed), the usual GPU technique: on the
  CPU a set of separate masks is simpler to evict and needs no packing; an atlas can come with a
  GPU renderer (ADR-0042 R2).
- **Caching glyphs as paths** instead of masks: still rasterises every glyph at every frame (the
  28.8 ms of the spike); masks skip that.
- **LCD sub-pixel anti-aliasing** (TX6): different on every screen, wrong in screenshots and
  rotated screens.
- **Hinting**, even `skrifa`'s autohinter: different shapes at different sizes, and ADR-0036 B2
  already removed the feature.
- **All the bundled fonts in the program** (about 8 MB): most of them are only for the guides.
- **Fonts from the system**: they contradict ADR-0006 (the same look everywhere).

## Consequences

- New crate `orchestra-text` in the workspace (`docs/ARCHITECTURE.md`, its documents, CLAUDE.md
  §2 with the owner's confirmation), with `skrifa` and `harfrust`; `orchestra-ui` depends on it.
- About 3.0 MB of font data in the binary, plus the code of the two crates (measured in F2.07b).
- `ADR-0039 D4` gains `GlyphRun`; the segment gains a glyph arena.
- ADR-0037 gains the text tokens (TX11); changing them re-records the segments with text.
- Recorded limits: no fallback font (TX4), left to right only (TX5).
- Step plan: **F2.07b** — `orchestra-text` (faces, shaping, layout, wrapping, metrics,
  outlines), the glyph run, the cache, the gamma choice by eye; **F2.07c** — the text tokens,
  text in the reference scene and the screenshot catalogue, the 33 M-instruction budget.
- F2B uses `orchestra-text` for Bravura; Bravura's SMuFL metadata file (`bravura_metadata.json`)
  enters in F2.17, not here.

## Precisions of the implementation (F2.07b, approved by the owner on 2026-11-12)

**Part 1 (`orchestra-text`), TB1–TB8.** `icu_segmenter` 2.4 without dictionaries was measured
(+52 KB stripped, +14 crates, +2.6 s of clean build) and chosen over our own subset (TX10). The
crate has one module per responsibility; the faces live in an explicit `Faces` value, with no
global state; layouts are in logical pixels, y down, unrounded; each glyph carries the UTF-8 byte
offset of its cluster (caret functions come in F2.10); lines are filled greedily; at a soft hyphen
where a line breaks, the face's U+2010 (or U+002D) glyph is appended without reshaping. Found while
implementing: `icu_segmenter` exposes which breaks are mandatory only behind its `unstable`
feature, so mandatory breaks are derived from UAX #14 LB4–LB5 (right after a hard line break
character); white space inside a word cut by the last resort of TX10 hangs like any space; Inter
maps some Private Use Area code points to glyphs of its own, so SMuFL symbols are looked up in
Bravura only.

**Part 2 (glyph runs and the cache), TC1–TC10.** The renderer owns a copy of the faces
(`CpuRenderer::new(faces)`), and the window receives them in `WindowOptions::faces`. The recorder
has `text(&TextLayout, origin, paint)` and `glyphs(face, size, face_box, glyphs, paint)`, both
recording `GlyphRun` into a per-segment glyph arena; the run's bounds come from
`TextLayout::bounds` (the glyph origins widened by the face's box, `GlyphBox`) or from the box the
caller passes. The cache's key is (face, glyph, size in 1/64 device pixel, quarter); its masks are
`tiny-skia` anti-aliased fills of the shifted outline; lookups use a small FxHash-style hasher;
when the 8 MB bound would be exceeded, the least recently used masks are evicted in one batch down
to three quarters of it. Masks are blended with the paint and the clip with one rounding per
channel, as shadows are; fully covered pixels of an opaque colour are copied with the same bytes.
Positions are rounded in the frame's device coordinates, so partial redraws equal full frames byte
for byte. A glyph whose mask would exceed 256 × 256 device pixels is filled as a path, and since
that depends on the scale, a glyph run always counts as a shape a partial redraw includes whole.

**Text contrast (TX6).** A 256-entry table maps a glyph pixel's coverage `c` to `c^(1/γ)`; the
owner chooses γ by eye among 1 (plain), 1.2 and 1.45, on screenshots of Studio light and dark at
scales 1 and 2. *Pending at the time of writing.*
