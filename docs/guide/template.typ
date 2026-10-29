// SPDX-License-Identifier: GPL-3.0-or-later
//
// Shared template of the Orchestra user guides (GUIDA.pdf in Italian, GUIDE.pdf in en-US).
//
// Both guides import this file, so the look is defined in exactly one place and the two PDFs can
// only differ in their text (ADR-0009). Compiled by `cargo xtask guides` with:
//   typst compile --root <repo> --ignore-system-fonts --font-path <repo>/assets/fonts …
// `--root` is the repository, so absolute paths like "/assets/screenshots/x.png" work from every
// chapter; `--ignore-system-fonts` guarantees the same PDF on every machine.

// ------------------------------------------------------------------------------------------------
// Design tokens
// ------------------------------------------------------------------------------------------------

/// Palette: a deep "ink" blue for structure, a warm brass accent (orchestral, and readable on
/// white), soft paper tones for boxes. Contrast of `ink` on white is above WCAG AAA.
#let palette = (
  ink: rgb("#1f2547"),
  accent: rgb("#c8892b"),
  muted: rgb("#5b6178"),
  paper: rgb("#f6f3ec"),
  rule: rgb("#d9d4c7"),
  note-bg: rgb("#eef2fb"),
  note-bar: rgb("#3d5bb5"),
  tip-bg: rgb("#f4f8ee"),
  tip-bar: rgb("#5f8a2e"),
  warn-bg: rgb("#fbf1e6"),
  warn-bar: rgb("#c8892b"),
)

/// Font families, all bundled in assets/fonts (see its README for licenses).
#let fonts = (
  body: "Source Serif 4",
  heading: "Inter",
  mono: "JetBrains Mono",
  music: "Bravura",
  music-text: "Bravura Text",
)

/// A few SMuFL code points (Standard Music Font Layout) used for decoration and inline symbols.
/// Code points from the SMuFL specification, e.g. U+E050 gClef, U+E1D5 noteQuarterUp.
#let smufl = (
  g-clef: "\u{E050}",
  quarter-note: "\u{E1D5}",
  // "Metronome mark" variants (U+ECA0–ECBF) are designed to sit on the text baseline at text
  // size, which is what we want for notes mentioned inside a sentence.
  text-quarter-note: "\u{ECA5}",
  eighth-note: "\u{E1D7}",
  half-note: "\u{E1D3}",
)

/// Writes a music symbol inline with the text, using the text-optimised SMuFL font.
/// Use the `text-*` symbols of `smufl` (metronome-mark glyphs), which sit on the baseline.
#let music(symbol) = text(font: fonts.music-text, size: 1.5em, symbol)

// ------------------------------------------------------------------------------------------------
// Reusable blocks for chapters
// ------------------------------------------------------------------------------------------------

/// Generic callout box: a coloured bar on the left, a bold label and the content.
#let callout(label, body, bg: palette.note-bg, bar: palette.note-bar) = block(
  width: 100%,
  fill: bg,
  inset: (left: 14pt, rest: 10pt),
  radius: (right: 4pt),
  stroke: (left: 3pt + bar),
  breakable: false,
)[
  #text(font: fonts.heading, weight: "semibold", size: 9pt, fill: bar, upper(label))
  #v(-4pt)
  #body
]

/// "Note" box. The label is passed by each guide in its own language.
#let note(label, body) = callout(label, body)
/// "Tip" box.
#let tip(label, body) = callout(label, body, bg: palette.tip-bg, bar: palette.tip-bar)
/// "Warning" box.
#let warning(label, body) = callout(label, body, bg: palette.warn-bg, bar: palette.warn-bar)

/// A screenshot generated headless by `cargo xtask screenshots` (always in en-US, shared by both
/// guides): framed, with a soft shadow-like border and a caption. `path` is absolute from the
/// repository root, e.g. "/assets/screenshots/main-window.png".
#let screenshot(path, caption, width: 100%) = figure(
  block(
    radius: 4pt,
    clip: true,
    stroke: 0.6pt + palette.rule,
    image(path, width: width),
  ),
  caption: caption,
  kind: image,
)

/// A placeholder used until real screenshots exist (the GUI toolkit is not chosen yet,
/// ADR-0012): same frame and caption as a real screenshot, so the layout stays representative.
#let screenshot-placeholder(label, caption, height: 6cm) = figure(
  block(
    width: 100%,
    height: height,
    radius: 4pt,
    fill: palette.paper,
    stroke: (paint: palette.rule, thickness: 0.8pt, dash: "dashed"),
    align(center + horizon, text(font: fonts.heading, size: 10pt, fill: palette.muted, label)),
  ),
  caption: caption,
  kind: image,
)

// ------------------------------------------------------------------------------------------------
// Cover page
// ------------------------------------------------------------------------------------------------

/// The Orchestra logo (assets/logo/orchestra-logo.svg) in any colour.
///
/// The logo is the ONLY one allowed and its shape must never change (ADR-0014): the SVG has a
/// single fill colour, `#000000`, and recolouring replaces exactly that string in the file text
/// before Typst decodes it. Paths, proportions and the outlined wordmark are untouched.
#let logo(colour, width: 100%) = {
  let svg = read("/assets/logo/orchestra-logo.svg").replace("#000000", colour.to-hex())
  image(bytes(svg), format: "svg", width: width)
}

/// The cover: dark full-bleed page with the logo in white, the subtitle and the edition line.
/// The guide title is not repeated as text: the logo already contains the wordmark.
#let cover(subtitle, edition) = page(
  margin: 0pt,
  fill: palette.ink,
  header: none,
  footer: none,
)[
  // The logo artwork sits inside a square canvas with generous margins (as drawn by its author),
  // so it is placed as a square and centred horizontally.
  #place(top + center, dy: 4.2cm, logo(white, width: 13cm))
  #place(top + center, dy: 17.6cm, text(
    font: fonts.heading,
    size: 13pt,
    weight: "semibold",
    fill: palette.accent,
    tracking: 3pt,
    upper(subtitle),
  ))
  #place(bottom + center, dy: -2.2cm, text(font: fonts.heading, size: 9pt, fill: white.darken(25%), edition))
]

// ------------------------------------------------------------------------------------------------
// Main template
// ------------------------------------------------------------------------------------------------

/// Applies the guide's style to the whole document.
///
/// - `lang`, `region`: drive hyphenation, quotes and Typst's own localised words ("Figure",
///   "Contents"…), so the template has no hard-coded language.
/// - `title`: document metadata and running header (the cover shows the logo instead).
/// - `subtitle`, `edition`: cover texts, passed by each guide in its language.
#let guide(title: "", subtitle: "", edition: "", lang: "en", region: none, body) = {
  set document(title: title + " — " + subtitle)
  set text(font: fonts.body, size: 10.5pt, lang: lang, region: region, fill: palette.ink.darken(20%))
  set par(justify: true, leading: 0.68em, spacing: 1.1em)

  cover(subtitle, edition)

  set page(
    paper: "a4",
    margin: (top: 2.6cm, bottom: 2.4cm, inside: 2.4cm, outside: 2.4cm),
    header: context {
      // Running header: guide title on the left, current chapter on the right, from page 3 on.
      // Chapter opening pages carry no running header (the big chapter title is the header).
      let opening = query(heading.where(level: 1)).any(h => h.location().page() == here().page())
      if here().page() > 2 and not opening {
        let chapters = query(selector(heading.where(level: 1)).before(here()))
        set text(font: fonts.heading, size: 8pt, fill: palette.muted)
        grid(
          columns: (1fr, 1fr),
          align(left, title),
          align(right, if chapters.len() > 0 { chapters.last().body }),
        )
        v(-6pt)
        line(length: 100%, stroke: 0.4pt + palette.rule)
      }
    },
    footer: context {
      set text(font: fonts.heading, size: 8pt, fill: palette.muted)
      align(center, counter(page).display("1"))
    },
  )
  counter(page).update(1)

  // Headings: Inter, ink colour; chapters start on a new page with a large accent number.
  set heading(numbering: "1.1")
  show heading: set text(font: fonts.heading, fill: palette.ink)
  show heading.where(level: 1): it => {
    pagebreak(weak: true)
    v(1.5cm)
    if it.numbering != none {
      text(size: 40pt, weight: "bold", fill: palette.accent, counter(heading).display("1"))
      v(-14pt)
    }
    text(size: 24pt, weight: "bold", it.body)
    v(4pt)
    line(length: 3cm, stroke: 2pt + palette.accent)
    v(14pt)
  }
  show heading.where(level: 2): set text(size: 14pt, weight: "semibold")
  show heading.where(level: 3): set text(size: 11.5pt, weight: "semibold")

  // Code: JetBrains Mono on a paper-coloured block.
  show raw: set text(font: fonts.mono, size: 8.8pt)
  show raw.where(block: true): it => block(
    width: 100%,
    fill: palette.paper,
    inset: 10pt,
    radius: 4pt,
    stroke: 0.5pt + palette.rule,
    it,
  )

  // Figure captions: small Inter, muted, with the localised "Figure N" in the accent colour.
  show figure.caption: it => text(font: fonts.heading, size: 8.5pt, fill: palette.muted)[
    #text(fill: palette.accent, weight: "semibold")[#it.supplement #context it.counter.display(it.numbering)]#it.separator#it.body
  ]
  set figure(gap: 8pt)

  // Links in the accent colour.
  show link: set text(fill: palette.note-bar)

  // Table of contents on its own page, chapters in bold.
  {
    show outline.entry.where(level: 1): it => {
      v(6pt)
      strong(it)
    }
    set text(font: fonts.heading, size: 10pt)
    outline(depth: 2, indent: auto)
  }

  body
}
