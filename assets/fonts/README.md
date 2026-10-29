# Bundled fonts

Every font shipped with Orchestra (in the program and in the user guides) lives here, so that the
interface and the PDFs look identical on every machine (ADR-0006, ADR-0009). Typst is always run
with `--ignore-system-fonts --font-path assets/fonts`.

All fonts are under the **SIL Open Font License 1.1**, which allows bundling and redistribution
with GPL software (ADR-0002). Each folder keeps the license text exactly as published. The OFL
forbids selling the fonts on their own and using the *Reserved Font Names* for modified versions:
we never modify them.

| Folder | Family (as Typst names it) | Version | Use | Source (license verified 2026-10-28 via GitHub API: `OFL-1.1`) |
|---|---|---|---|---|
| `source-serif-4/` | Source Serif 4 | 4.005 (2023-01-20) | Guide body text | <https://github.com/adobe-fonts/source-serif/releases/tag/4.005R> |
| `inter/` | Inter | 4.1 (2024-11-16) | Guide headings, captions, UI candidate | <https://github.com/rsms/inter/releases/tag/v4.1> |
| `jetbrains-mono/` | JetBrains Mono | 2.304 (2023-01-14) | Code blocks | <https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304> |
| `bravura/` | Bravura, Bravura Text | 1.482 (2026-08-24) | SMuFL music font: notation, music symbols in text | <https://github.com/steinbergmedia/bravura/releases/tag/bravura-1.482> |

Only the weights we use are included (static OTF/TTF, not the variable fonts), to keep the
repository small: about 8 MB in total.

**Bravura** is the reference font of the SMuFL standard (Standard Music Font Layout), so its glyph
code points and metadata are what `orchestra-notation-layout` will target; *Bravura Text* is the
variant designed to be mixed with ordinary text (e.g. "♩ = 120" in a guide).

To add a font: verify its license (OFL or more permissive), add the files and the license text in
a new folder, and add a row to this table.
