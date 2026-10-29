# Orchestra logo

`orchestra-logo.svg` is **the only logo of Orchestra**. It is used everywhere a logo appears: the
program (window and application icons, About box, splash), the user guides (cover), the website,
the repository. Rules (ADR-0014):

1. **Never change its shape**: no redrawing, cropping into a different composition, distorting,
   re-typesetting the wordmark, adding outlines or effects, or using a different logo.
2. **Colours may change** to suit the context (light theme, dark theme, cover backgrounds…). The
   file has a single fill colour, `#000000`; recolour by replacing that exact string in the SVG
   text before rendering. The guide template does this in its `logo(colour)` function.
3. The artwork keeps the square canvas and the margins its author drew; scale it uniformly.

## Provenance

Designed by Emanuele Rainoldi, the project owner, in Inkscape 1.4.4 (`Orchestra.svg`, exported as `Orchestra.pdf`,
`Orchestra.png`, 2026-10-22). In the original SVG the wordmark "ORCHESTRA" was live text in the
*Alte Haas Grotesk Bold* font, which would render with a different font wherever that font is not
installed (including our guides, built with `--ignore-system-fonts`). This file was therefore
produced from the author's own PDF export with `pdftocairo -svg`, which turns the embedded glyphs
into outlines, and then normalised to the single colour `#000000`.

Verified on 2026-10-29: rendered at 1000 × 1000 px with `rsvg-convert`, this file differs in
**0 pixels** (fuzz 25 %) from both the author's PNG export and the author's original SVG.

## License — all rights reserved

Copyright © 2026 Emanuele Rainoldi. **All rights reserved** (owner's decision, 2026-10-29, ADR-0014).

The logo is **not** covered by the GNU GPL that applies to Orchestra's source code. You may not
copy, modify or use it outside the Orchestra project without written permission. The code
remains free software: forks are welcome, but a fork distributed as a different project must not
use this logo.
