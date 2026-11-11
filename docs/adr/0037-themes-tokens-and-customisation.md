# ADR-0037 — Themes: tokens, ten built-in pairs, user customisation

- **Status:** Accepted (owner, 2026-11-10; T9 changed by the owner from light to dark). **T10 §7
  amended by ADR-0041** (2026-11-11): the guides' screenshots use Studio's dark half.
- **Date:** 2026-11-10
- **Amends:** ADR-0014, "Light and dark themes" §1, §3 and §7 (details in T10). Everything else in
  ADR-0014 is unchanged: no hard-coded colour, semantic tokens, WCAG 2.2 AA, the score page
  option, system detection in `platform/`, the logo in the theme's colour, the i18n process.

## Context

ADR-0014 asked for one light and one dark theme, built from semantic tokens and checked against
WCAG 2.2 AA, with the setting *System / Light / Dark*. Step F2.02a (SCALETTA) builds the theme
system before the first widget.

During the briefing of 2026-11-10 the owner looked at ten proposals, each a light + dark pair
drawn on the same mock window: three built on pure black and pure white with straight lines and
square corners, seven in different directions (the comparison page, version 1, is the owner's
private artifact "Temi di Orchestra"). Every proposal was first checked by a script against the
pairs of T4; two accents failed (the guides' amber on light paper, 2.68:1; Studio's orange) and
were darkened before the owner saw them.

The owner then chose to **keep all ten pairs** and let the user pick one, with **Studio** as the
default, and to let the user **customise every detail** of the interface: every colour, the
corners, the lines, the text. The answers to the questions of that day are recorded as T1–T11.

## Decision

### T1 — Ten built-in pairs; the user picks a pair and a mode

Each built-in theme is a **pair**: a light instance and a dark instance, both complete. The
user setting is two independent choices:

- the **pair**: one of the ten built-in pairs or a custom one (T7); default **Studio**;
- the **mode**: *System* (default, follow the operating system), *Light* or *Dark*.

The pairs, with their stable identifiers (never renamed, never reused; saved in the settings
file) and their idea:

| ID | Italian working name | Idea | Corners | Lines |
|---|---|---|---|---|
| `absolute` | Assoluto | pure black and pure white only; structure from lines; colour only for error, warning, success | 0 | 1 |
| `graphite` | Grafite | pure black and white for ink and paper, greys to separate the layers, no accent hue | 2 | 1 |
| `signal` | Segnale | black and white with thick lines and one signal red for what is active or selected | 0 | 2 |
| `parchment` | Pergamena | the colours of the PDF guides: night-blue ink, warm paper, amber | 4 | 1 |
| `slate` | Ardesia | cool bluish greys and a firm blue | 6 | 1 |
| `studio` | Studio | dark first, like mixers and DAWs: charcoal with a pilot-light orange | 3 | 1 |
| `forest` | Bosco | sage and moss greens | 8 | 1 |
| `lagoon` | Laguna | watery whites and a deep teal | 4 | 1 |
| `nocturne` | Notturno | deep violet and lavender, soft corners | 10 | 1 |
| `snow` | Neve | very light lilac greys, a raspberry accent, wide corners and double lines | 12 | 2 |

The colour values are those of the comparison page, version 1, with the shadow values of T3
added. They live in the code as constants (T6), which is their only source. The names shown to
the user are not these Italian working names: they are Fluent keys in the six catalogs, with the
glossary process (ADR-0008), added with the Preferences window (F2.12), the first place where
they are visible.

### T2 — Colour representation: sRGB, 8 bits per channel, straight alpha

A colour is four bytes, red, green, blue and alpha, in the sRGB encoding, **not** premultiplied:
the format `tiny-skia` and `softbuffer` work in, and the format a person types as `#rrggbbaa`.
Linear floating point was rejected (16 bytes per colour and a conversion at every draw, useful
only to the GPU path of F2.06, which can convert once when a theme is loaded). Only the shadow
(T3) is translucent in the built-in themes.

### T3 — The tokens

**Colour tokens (24):**

| Group | Tokens |
|---|---|
| Surfaces | `surface` (window background), `surface-raised` (panels), `surface-sunken` (fields, document area), `surface-overlay` (menus, pop-ups) |
| Lines | `border` (decorative separators), `control-border` (the edge that identifies a control) |
| Text | `text`, `text-muted`, `text-disabled` |
| Accent | `accent` (filled controls, toggles, slider fill), `accent-hover`, `accent-pressed`, `on-accent` (text on the accent), `accent-text` (the accent used as text or link on a surface) |
| Controls | `control`, `control-hover`, `control-pressed` |
| Selection and focus | `selection`, `on-selection`, `focus` |
| States | `error`, `warning`, `success` |
| Depth | `shadow` (colour of the overlay shadow; alpha 0 means no shadow) |

**Shape tokens (5)**, in logical pixels (multiplied by the window's scale factor when drawn;
whole numbers from 0 to 255):

| Token | Meaning | Built-in values |
|---|---|---|
| `corner-radius` | radius of controls, panels and menus | per pair, T1 table |
| `line-width` | width of borders and separators | per pair, T1 table |
| `focus-width` | width of the focus ring | 2 in every pair (the thickness WCAG 2.4.13, level AAA, asks for) |
| `shadow-blur` | blur radius of the overlay shadow | 18 where the pair has a shadow |
| `shadow-offset` | vertical offset of the overlay shadow | 6 where the pair has a shadow |

The pairs built on pure black and white (`absolute`, `signal`) have **no shadow** (alpha 0):
their depth comes from lines. The others use black at 18% alpha in light mode and at 50% in dark
mode. **Text tokens** (typeface among the bundled fonts, size scale, weights) are designed in
F2.07 together with the fonts; **density** (spacing) in F2.09 with the basic widgets. Tokens of
the notation and DAW views (`staff-line`, `notehead`, `playhead`, track colours…) come with the
steps that draw them. Every token added later must be given a value in all ten pairs and,
if it is a foreground or background, enter the pairs of T4.

### T4 — The contrast pairs

The tokens that must contrast are declared once, in a table of pairs **(foreground, background,
kind)**. Kind *text* needs 4.5:1 (WCAG 1.4.3), kind *graphic* needs 3:1 (WCAG 1.4.11: control
boundaries, the accent as a state indicator, the focus ring). The 31 pairs:

- `text` on the four surfaces and on `control`, `control-hover`, `control-pressed`; `text-muted`
  on the four surfaces; `accent-text`, `error`, `warning`, `success` on `surface` and
  `surface-raised`; `on-accent` on `accent`, `accent-hover`, `accent-pressed`; `on-selection` on
  `selection` (all *text*);
- `control-border` on `surface`, `surface-raised`, `surface-sunken`; `accent` on `surface` and
  `surface-raised`; `focus` on `surface`, `surface-raised`, `surface-sunken` (all *graphic*).

**Exempt** tokens, as WCAG allows: `border` (decorative), `text-disabled` (inactive controls),
`shadow` (not information). The contrast ratio is the WCAG 2.x formula on the relative
luminance of the linearised sRGB channels. A translucent colour, if a pair ever has one, is
first composited over the pair's background. The ratio is computed in `f64` (interface code,
not musical time, so ADR-0005 does not apply), and the comparison with the threshold is exact on
the computed value: no rounding to two decimals before comparing.

### T5 — What is tested

- Every instance of every built-in pair (20) passes every pair of T4; the test prints every
  failure, not only the first.
- Every colour token is in at least one pair or in the exempt list, so a new token cannot be
  forgotten.
- The contrast formula is checked against an oracle written in the test itself (the WCAG
  formula from its definition, with its own linearisation) and against known values (black on
  white 21:1, equal colours 1:1).
- A custom theme (T7) resolves to its base with its overrides applied, and to its base exactly
  when it has none.

### T6 — Where the code lives and how it is read

Everything is in `orchestra-ui`, module `theme`, with no new dependency and no platform code
(system detection is F2.02b). A theme instance is a fixed array of colours indexed by an enum of
the colour tokens plus a struct of the shape tokens: reading a token is an array access, O(1),
no hashing, no allocation. The built-in instances are `const` data. A widget never holds a colour
of its own: it asks the active instance at draw time, so switching theme is a full redraw and
nothing more (ADR-0014 §1, no restart).

### T7 — Custom themes: a base pair plus overrides

A custom theme is a **base pair** (one of the ten IDs) plus, for each mode, a list of
**overrides** (token, value), colour or shape. Resolving it copies the base instance and applies
the overrides; it happens when the theme is chosen or edited, never per frame. Storing
differences instead of a full copy means that tokens added in future versions come from the
base automatically, with no migration of the user's themes (the same idea as performance
offsets stored as deltas, ADR-0004). Built-in pair IDs are never removed, so a base always
exists.

### T8 — Custom themes may break the contrast: a warning, never a block

The built-in pairs are guaranteed by T5. A custom theme is the user's choice: the theme editor
(F2.12b) shows at once which pairs fall below their threshold and by how much, with a button to
restore the base value, but **saving is never refused**. The check is the same public function
the tests use.

### T9 — When the mode is *System* and the system says nothing

If the operating system's preference cannot be read (no portal, an old system, F2.02b), the
**dark** instance is used (owner's choice, 2026-11-10): the default pair, Studio, is designed dark
first, as a studio tool. The guides' screenshots still use the light instance (T10), chosen
explicitly rather than through the system. *(Since ADR-0041 S3 they use the dark instance.)*

### T10 — Amendments to ADR-0014

- **§1:** the setting is a pair (ten built-in or custom, default Studio) and a mode (*System*,
  *Light*, *Dark*, default *System*), instead of the mode alone. Switching either takes effect
  immediately.
- **§3:** WCAG 2.2 AA is guaranteed for the built-in pairs, by an automated test; a custom
  theme is checked and warned about, never blocked (T8). Adding a high-contrast theme later still
  only means adding a pair.
- **§7:** the guides' screenshots use **Studio's light instance** (the default pair in light
  mode). *Amended by ADR-0041 S3 (owner, 2026-11-11): Studio's **dark** instance.*

### T11 — Where the rest goes

- Theme names and the words *System / Light / Dark* in the six catalogs, the settings file
  (versioned, with migrations, CLAUDE.md §9) that saves the choice and the custom themes:
  F2.12.
- The theme editor (every token, live preview, live contrast, import and export of a theme
  file): new step F2.12b, after the Preferences window, because it needs the basic widgets
  (F2.09) and the text field (F2.10).
- Text tokens: F2.07. Density: F2.09.

## Alternatives rejected

- **One light and one dark theme only** (ADR-0014 as written): the owner chose to keep every
  proposal and let the user choose.
- **Shape as one rule for the whole interface:** the owner's first idea; replaced, after seeing
  the proposals, by a shape per pair that the user can change.
- **Linear floating-point colours:** see T2.
- **Custom themes as full copies:** every token added in a later version would need a migration
  of every saved theme (T7).
- **Refusing to save a custom theme below WCAG:** less freedom for a choice that concerns only
  the user's own screen (T8); doing no check at all would hide a problem the user may not see.
- **Themes as files loaded at start-up:** parsing and I/O at every launch, and errors found only
  at run time; the built-in pairs are constants checked by the compiler and by T5.

## Consequences

- F2.02a implements T2–T7 and T9 without a window: types, the ten pairs, overrides, the contrast
  check and the tests of T5.
- Ten pairs mean every future token needs twenty values; the test of T5 makes a missing one a
  compile or test error, never a silent default.
- The Parchment pair carries the guides' colours with a darker amber in light mode. The guides
  are unchanged by this ADR, but they use the amber `#c8892b` on the paper `#f6f3ec` also as
  text (links, the "Figure N" of captions), at 2.68:1, below 4.5:1: recorded as an open item in
  `ROADMAP.md`, to be fixed in the guides' template with the owner's ok.
- SCALETTA gains step F2.12b and notes in F2.07 (text tokens) and F2.09 (density).
