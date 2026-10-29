# ADR-0014 — Visual identity: one logo, light and dark themes from the start

- **Status:** Accepted (owner's decision, 2026-10-29)
- **Date:** 2026-10-29

## Context

The owner designed the Orchestra logo (an ellipse with an off-centre "O" counter, above the
wordmark ORCHESTRA) and asked that it be the only logo ever used, recoloured as needed but never
reshaped, starting with the cover of the user guides. The owner also asked that the program offer
light and dark display modes from the very beginning. ADR-0006 already requires the same visual
identity on every platform with an interface drawn by us; ADR-0012 makes every pixel of that
interface ours, so theming is entirely our responsibility.

## Decision

### Logo

1. `assets/logo/orchestra-logo.svg` is the **only** logo: program icons, About box, splash,
   guides, website, repository.
2. Its **shape never changes**; only its colour may. The file has one fill colour (`#000000`) and
   is recoloured by replacing that value (see `assets/logo/README.md`). The wordmark is stored as
   outlines, so it does not depend on any installed font.
3. **License: © 2026 Emanuele Rainoldi, all rights reserved** (owner's decision). The logo is not covered by the GPL of
   the source code (ADR-0002 covers code, not the project's brand). Forks may reuse the code but
   must not use this logo when distributed as a different project. This is the usual
   arrangement for free software brands (e.g. Firefox, Thunderbird).
4. Platform icon files (`.ico`, `.icns`, PNG sizes, freedesktop SVG) are **generated** from this
   file by automation when packaging arrives, never drawn by hand.

### Light and dark themes

1. The interface has a **light** and a **dark** theme from its first widget. The user setting is
   *System* (default: follow the operating system), *Light* or *Dark*, and switching takes effect
   immediately, without restarting.
2. **No colour is hard-coded** in widgets or renderers: everything reads **semantic tokens** from
   the active theme (e.g. `surface`, `surface-raised`, `text`, `text-muted`, `accent`,
   `selection`, `staff-line`, `notehead`, `playhead`, `track-colour[n]`…). A theme is a complete
   set of tokens; light and dark are two instances. Adding a high-contrast theme later must only
   mean adding a third instance.
3. Every theme must meet **WCAG 2.2 AA** contrast for text (4.5:1, 3:1 for large text and
   essential graphics); checked by an automated test over the token pairs, like the translation
   parity tests.
4. **The score page** follows the theme by default (dark paper and light ink in dark mode), with an
   independent option to always show the score on white paper, because many musicians prefer the
   printed-page look. Printing and PDF export always use light colours.
5. **System preference detection** lives in `platform/` modules behind a trait (ADR-0006):
   xdg-desktop-portal `org.freedesktop.appearance color-scheme` on Linux, the
   `AppsUseLightTheme` setting on Windows, `NSApp.effectiveAppearance` on macOS; changes are
   observed while the program runs.
6. The logo uses the theme's text colour (or its accent) so it is readable on both backgrounds.
7. Screenshots for the user guides are generated in the **light** theme (ADR-0009 already fixes
   their language as en-US); a guide section that explains the themes may add dark examples.
8. The words "System", "Light", "Dark" and any theme-related text follow the i18n process
   (ADR-0008): glossary first, six catalogs.

## Alternatives rejected

- **Light theme only, dark later:** retrofitting themes into a codebase with literal colours is a
  long, error-prone search-and-replace; tokens from day one cost almost nothing.
- **Inverting colours automatically for dark mode:** produces poor contrast and wrong accents;
  each theme is designed explicitly.
- **Keeping the wordmark as live text in the SVG:** it renders with a fallback font wherever
  *Alte Haas Grotesk* is missing, i.e. the shape would change.

## Consequences

- The UI foundation of Phase 2 starts with the theme system (tokens, light and dark instances,
  contrast test, system detection), before the first widget.
- The renderer (and the notation layer) receives colours only through the theme.
- `cargo xtask screenshots`, when it exists, renders in the light theme.
- Because the logo is not free, distributions with strict free-software policies (e.g. Debian
  main) may need to package Orchestra with the logo replaced; the build must therefore keep the
  logo in `assets/logo/` only, loaded from one place, so a packager can swap it without patching
  code. The repository README states that the logo is excluded from the GPL.
