# ADR-0050 — Dialogs, the settings file and the Preferences

- **Status:** Accepted (owner, 2026-11-23; outline DG1–DG10 presented first, the owner followed
  every recommendation, then this text, accepted with its three points: the buttons' order on KDE
  corrected, the logo converted by our own code, the playback values as the user's settings).
  Precisions of F2.12a2 and of the later steps below.
- **Date:** 2026-11-23
- **Implements:** ADR-0045 WA4 and WB9 (modal layers: the focus trapped, the pointer blocked),
  ADR-0048 AM7 (the user's shortcuts in the settings file), AM9 (the *About* role), AM11 (the
  popup layer), ADR-0037 T1 and T7 (the pair, the mode, custom themes), ADR-0047 TF8 (the caret's
  blink can be turned off), ADR-0046 V3 (the fine modifier), ADR-0027 §14–§15 (the playback values
  tuned by ear), ADR-0036 B4 (`rfd` and libdbus-1), ADR-0029 (the project file, its safe saving),
  ADR-0010 (versions and migrations), ADR-0008 (six languages), ADR-0006 (platform conventions).
- **Amends:** ADR-0037 T3 (one shape token more, `dialog-frame`: DG2 as amended in F2.12a3a; the
  colour token `scrim` first planned here was dropped with the veil); ADR-0036 (the `rfd` version:
  DG4).
- **Step:** F2.12a1 of SCALETTA (this ADR); the realisation in F2.12a2–a5 (Consequences).

## Context

F2.12 brings the program its first **dialogs**: the file chooser, the questions the program asks
("Save the changes?"), the error messages, the **Preferences** and *About Orchestra*. Notes
gathered in earlier steps add to it:
- the **settings file**, with a version and migrations (CLAUDE.md §9), which stores the pair and
  the mode of the theme and the custom themes (ADR-0037), the user's shortcuts (ADR-0048 AM7),
  the fine modifier (ADR-0046 V3, ADR-0048), the caret's blink (ADR-0047 TF8) and the playback
  values of ADR-0027 §14–§15 (owner, 2026-11-08);
- the names of the ten pairs and the words *System / Light / Dark* in the six catalogues;
- the *About* action with its window (F2.11e2, owner's choice 1): role *About*, so in the
  Orchestra menu on macOS and in Help elsewhere;
- with *Light* or *Dark* chosen by hand, the **system's title bar** on Windows and macOS still
  follows the system (seen in F2.04b, postponed to here).

What exists, verified on 2026-11-23:
- The widget core already has **modal layers**: `Ui::open_modal` / `close_modal` trap the focus
  inside the layer (WA4), send keys, typed text and the IME to it, and block the pointer outside it
  (WB9). Menus, context menus and tooltips are layers of one stack above the main tree, inside the
  window (ADR-0048 AM11), because `winit` 0.30 has no popup windows on Wayland.
- The theme has `surface-overlay` for menus and pop-ups, but **no colour for veiling what lies
  under a dialog**.
- `orchestra-ui` does not depend on `orchestra-io`; `orchestra-app` depends on both. `io` already
  writes files safely (`safe_write`: temporary file in the same directory, sync, atomic rename,
  directory sync; ADR-0029 S13) and has a `platform/` module.
- The actions `file.new`, `file.open`, `file.save`, `file.save-as` and `edit.preferences` are in
  the registry, with their shortcuts (ADR-0048 AM6); none has an *About* role yet.
- The program starts in en-US (`Locale::EnUs` in `main.rs`); following the language at runtime and
  reading the system's is F2.13.
- `rfd` on crates.io: 0.17.2 (2026-01-12, the version of ADR-0036) and **0.18.0** (2026-11-07,
  MIT), now more than two weeks old. With `default-features = false` and `xdg-portal` the
  dependencies are the same as 0.17's except `pollster` 0.4 → 1.0; on macOS it adds
  `objc2-uniform-type-identifiers`. The GTK backend (now also GTK 4) stays out, as ADR-0036
  requires.
- For the configuration directory: `dirs` 7.0.0 (2026-09-05, MIT OR Apache-2.0, through
  `dirs-sys`), `directories` 6.0.0, `etcetera` 0.11.0.
- The logo, `assets/logo/orchestra-logo.svg`, is outlines only (the wordmark already converted to
  paths), one fill colour `#000000`; there is no SVG renderer in the program.

## Decision

### DG1 — Our dialogs are modal layers inside the main window (owner: as recommended)

Every dialog of ours (questions, errors, Preferences, About) is a **modal layer** of the AM11
stack, inside the main window, never a second `winit` window. A dialog is pushed on the stack like
a menu; a menu or a tooltip opened from inside a dialog (the context menu of a field in the
Preferences) goes above it, and a dialog opened from a dialog (an error over the Preferences)
goes above that one. Closing a dialog gives the focus back to where it was before it opened (WA4).

Why: the mechanism exists and is tested; Wayland would not let us place a child window anyway;
one window keeps one renderer, one AccessKit adapter and one damage tracker. **Limit:** a dialog
cannot leave the main window, and in a very small window it is clipped like a popup (ADR-0048
AM11's recorded limit); its content then scrolls (DG2).

### DG2 — How a dialog looks and behaves (owner: as recommended)

- Under the topmost dialog, the whole window is covered by a **veil**, the new colour token
  **`scrim`** (group *Depth* of ADR-0037 T3, an amendment): black with alpha, darker in the dark
  instances. It carries no text, so it joins no WCAG pair (T4); a custom theme gets it from its
  base (T7). The dialog is a `surface-overlay` panel with the corner, line and shadow tokens of
  the menus, centred in the window, with a **title**, the **content** (in a scroll area when it
  does not fit) and a row of **buttons** at the bottom.
- **Order of the buttons**, as each system writes them: affirmative **first** on Windows and on
  KDE Plasma (Qt's layout there: *OK, Cancel*); affirmative **last** on macOS, GNOME and the other
  Linux desktops (*Cancel, OK*). On Linux the desktop is read once at start from
  `XDG_CURRENT_DESKTOP` (KDE → affirmative first), behind the trait of `platform/`.
  *(Correction of the outline, which put KDE with GNOME.)*
- **Enter** activates the **default button** (drawn with the accent), unless the focus is on
  another button, which then is pressed; **Escape** activates the **cancel** button, or closes the
  dialog when it has only one. A destructive button (*Don't save*, *Delete*) is never the
  default. A click outside the dialog does nothing and is consumed (WB9): unlike a menu, a dialog
  is never closed by a stray click.
- The window's close request (the title bar's button, Alt+F4) while a dialog is open is treated
  as the program's close (DG5), never as a hidden *Cancel*.
- **Accessibility:** the dialog is a node with the role `Dialog` (an alert: `AlertDialog`), named
  by its title, with `modal`; the message of a question is its description.

### DG3 — Questions and errors are our dialogs, not the system's (owner: as recommended)

The questions ("Save the changes to «Name» before closing?") and the error messages are drawn by
us with DG2. The system's message boxes (`rfd::MessageDialog`) are not used: on Linux they need
GTK or `zenity`, and nowhere would they follow our themes, our catalogues or our button
conventions. A small set of **dialog builders** in `orchestra-ui` (a question with two or three
answers, an error with one) is shared by every caller; every text comes from the catalogues, and
an error shows the translated text of its typed variant (ADR-0029 errors, the `Display` stays for
developers).

### DG4 — File choosers: `rfd` 0.18, asynchronous, with the main window as parent (owner: as recommended)

- **`rfd` 0.18.0**, `default-features = false`, `xdg-portal` (amends ADR-0036's table; the
  version is measured again in F2.12a5: size, crates, build time, as ADR-0036 B1 asks). On
  macOS and Windows the native dialogs, on Linux the XDG desktop portal's.
- The **asynchronous** dialog (`AsyncFileDialog`) runs on a worker thread; the chosen path comes
  back to the event loop as a user event through `EventLoopProxy` (ADR-0040 W3), so our window
  keeps redrawing and answering the compositor while the chooser is open. The **main window is
  its parent** (`raw-window-handle`), so the system keeps the chooser above it and modal where it
  can.
- While a chooser is open the program puts an **empty modal layer** over its window: our input is
  blocked even where the system does not make the chooser modal, so a second chooser or an edit
  under it cannot start.
- Filter `Orchestra project (*.orchestra)` (ADR-0029 S14), the name in the catalogues; *Save as*
  proposes the current name, or the localised "Untitled".
- **When the chooser cannot open** (Linux without libdbus-1 or without a FileChooser portal, B4),
  the user gets our error dialog, never silence. If `rfd` cannot tell "cancelled" from "failed",
  the program first checks that the portal's FileChooser interface answers, with the `zbus`
  connection it already has (ADR-0038), and shows the error itself.

### DG5 — New, Open, Save and Save as are connected to the project file (owner: as recommended)

F2.12 gives the dialogs a real use, though the document is not shown yet (F2.21):
- `orchestra-app` holds the **open document**: the `Editor` (ADR-0026) and the file's path, or
  none for a new document. **Modified** means the editor's revision differs from the revision at
  the last save or open.
- The **window title** is `Name — Orchestra` (`Untitled — Orchestra` for a new document), with a
  modification mark while modified: `• Name — Orchestra` on Linux and Windows and in our title
  bar (ADR-0049). On macOS the system's *edited* dot in the close button would be the convention,
  but `winit` 0.30 does not expose it: the title carries the same mark there, a recorded limit.
- **New** makes the smallest valid document (its exact content is fixed in F2.12a5 with the
  model's rules); **Open** reads with `orchestra-io` (ADR-0029), **Save** writes over the file's
  path (or asks like *Save as* when there is none), **Save as** asks for a path.
- **Unsaved changes:** before New, Open, closing the window and Quit, a modified document asks
  "Save the changes to «Name»?" with *Save* (default), *Don't save*, *Cancel*.
- **Errors** (a file from a newer Orchestra, a damaged file, a write refused) are error dialogs
  with the typed error's translated text; a failed save leaves the previous file intact (S13) and
  the document still modified.
- Opening and saving run on the interface thread while they fit the budget of CLAUDE.md §5 (the
  phase benchmark already measures the opening, F1.16c); if the reference hardware needs more
  than 50 ms for the heavy score, they move to a worker thread in that step, with the same
  blocking layer as DG4.

### DG6 — The settings file: JSON, versioned, read leniently, never overwritten by an older Orchestra (owner: as recommended)

- **One file**, `settings.json`, plain JSON like the project file (ADR-0029 S1), written with
  `safe_write`. Its first field is the **format version** (an integer from 1), with migrations
  `vN → vN+1` and golden files of every version, as CLAUDE.md §9 requires for every format.
- The **schema** lives in `orchestra-io` (a `settings` module beside `project`), made of its own
  plain types: identifiers and strings (`"studio"`, `"dark"`, `"Primary+Shift+Z"`), numbers,
  lists. `orchestra-app` turns them into the types of `orchestra-ui` and back; `ui` and `io` stay
  independent of each other.
- **Lenient reading**, unlike the project file: settings must never stop the program from
  starting. A missing file means the defaults; an unknown field, a wrong type or a value out of
  range gives the **default for that entry** and a logged warning, the rest is kept; a file that
  is not JSON at all is renamed `settings.json.broken` (best effort) and the defaults are used.
  ADR-0048 AM7's rules for shortcuts stay as written (the later entry wins, unknown actions are
  kept and ignored).
- **A file from a newer Orchestra** is read for what this version understands and is **never
  written over**: changes made in this session apply but are not saved, and the Preferences say
  so once. This keeps ADR-0010's spirit (nothing the user wrote is silently dropped) without
  refusing to start.
- **What is stored**, only the user's choices, never what can be derived: the theme's pair and
  mode and the custom themes as base plus overrides (ADR-0037 T7), the interface language or
  *System*, the caret's blink, the fine modifier, the shortcuts' differences (AM7), the playback
  values of ADR-0027 §14–§15 that differ from the defaults. Every entry has a default, so a
  missing entry is never an error.
- **When it is written:** about half a second after the last change (one write for a slider's
  drag) and when the program closes. Two Orchestra windows of two processes: the last to write
  wins (a recorded limit; one process per user is not enforced).

### DG7 — Where the file lives: our own code in `io`'s `platform/` (owner: as recommended)

| System | Directory |
|---|---|
| Linux and other Unix | `$XDG_CONFIG_HOME/orchestra/`, or `$HOME/.config/orchestra/` when it is unset or not absolute (XDG Base Directory) |
| Windows | `%APPDATA%\Orchestra\` (the roaming application data) |
| macOS | `$HOME/Library/Application Support/Orchestra/` (Apple keeps `Library/Preferences` for the system's own preference lists) |

Three small functions in `io`'s `platform/`, built with `std::path`, reading only environment
variables, so `io` needs **no `unsafe`** and no new crate. If none of them gives a directory (no
`HOME`, no `APPDATA`), the program runs with the defaults and does not save, with one warning.
`dirs` 7.0 is the fallback if a case appears that the variables cannot answer (Windows' known
folder API); the step that writes the code records what was measured.

### DG8 — Preferences apply at once; there is no OK, Cancel or Apply (owner: as recommended)

Every change in the Preferences **applies immediately** (a theme is seen live, the blink stops at
once) and is saved by DG6. The window has one **Close** button; Escape and the shortcut that
opened it close it too. Why: one behaviour on every system (GNOME and macOS work this way, and
Windows' own Settings app does too), a live preview without a second copy of the settings, no
pending state to keep consistent. The values that cannot apply at once (the language until F2.13)
say so beside the control.

### DG9 — The Preferences' structure (owner: as recommended)

A large dialog (it follows the window's size, with a minimum), a **list of sections** on the left
and the section's content on the right; the list is reached with the keyboard like any list
(arrows, Tab to the content). Sections in F2.12:
- **Appearance**: the pair (the ten names in the six catalogues, ADR-0037), the mode *System /
  Light / Dark*, the caret's blink. The theme editor (F2.12b) joins this section later.
- **Language**: *System* or one of the six languages, each written in its own language
  ("English (US)", "English (UK)", "Italiano", "Français", "Deutsch", "Español"); until F2.13 the
  choice applies at the next start.
- **Playback**: the values of ADR-0027 §14–§15 (accent steps, the attack profiles' holds and
  returns), with the values tuned by ear as defaults and a button that restores them. Being in
  the Preferences (owner, 2026-11-08), they are the **user's** settings, not the document's: the
  same project may sound different with two users' settings. Values carried by a document, if
  wanted, would be a later decision (and a project-file version).
- **Keyboard**: the fine modifier (ADR-0046 V3; *Primary* by default, or *Shift*, or *Alt/Option*)
  and the place where the shortcut editor of F2.12c will go.

The action `edit.preferences` opens it (*Settings…* in macOS's application menu, ADR-0048). With
*Light* or *Dark* chosen by hand, the window calls `Window::set_theme` with the same mode, so the
system's title bar on Windows and macOS matches; back to *System* it calls `set_theme(None)`,
because while a theme is set those systems stop reporting their changes (the note of F2.04b).

### DG10 — About Orchestra (owner: as recommended)

A new action **`help.about`** with the role *About* (ADR-0048 AM9): first in the Orchestra menu on
macOS, last in Help on Linux and Windows. Its dialog shows:
- the **logo**, recoloured with the theme's `text` colour and never reshaped (assets/logo/README.md):
  its paths are read from the SVG by our own small converter into the display list's paths, no
  SVG renderer is added as a dependency; how the converter runs (at build time or once into a
  committed file) is fixed in F2.12a3;
- the **version**, the licence (*GNU GPL 3.0 or later*, with the note that the logo is not under
  it), a link to the source repository;
- the **bundled fonts** with their licences (ADR-0044).

A full list of the third-party crates' licences is not part of this step (it comes with packaging,
F6).

## Alternatives rejected

- **Separate `winit` windows for dialogs (DG1 B):** a dialog could leave the main window, but on
  Wayland it could not be placed or kept above its parent, and every window needs its own
  renderer, surface, AccessKit adapter and damage tracking. Rejected for now; nothing here forbids
  a separate Preferences window later.
- **The system's message boxes (`rfd::MessageDialog`):** GTK or `zenity` on Linux, no themes, no
  catalogues of ours, a different look on every system.
- **Only the infrastructure in F2.12 (DG5 a):** less work, but the modal layers and the file
  choosers would have no real user until F2.21, and the unsaved-changes question would be untested
  against a real document.
- **OK / Cancel / Apply (DG8 B):** the Windows convention, but it needs a pending copy of every
  setting and a second meaning for every control ("changed but not applied").
- **A strict settings file like the project file:** a settings file damaged or written by a newer
  version would stop the program from starting, for values that all have defaults.
- **`dirs` or `directories` from the start (DG7):** a crate (and a possible second `windows-sys`)
  for three environment variables; kept as the fallback.
- **An SVG renderer (`resvg`/`usvg`) for the logo:** a large dependency for one image made only of
  filled paths.

## Consequences

- SCALETTA: F2.12 is split in **F2.12a1** (this ADR), **F2.12a2** (the settings file: schema,
  lenient reading, migrations, golden files, the directory, the delayed write), **F2.12a3** (the
  dialogs: the layer in the stack, `scrim`, buttons, the question and error builders,
  accessibility, *About* with the logo), **F2.12a4** (the Preferences: sections, live application,
  `set_theme`, the blink, the fine modifier, the playback values, the language at the next start),
  **F2.12a5** (file choosers with `rfd` 0.18 and New / Open / Save / Save as with the
  unsaved-changes question and the window title).
- `orchestra-io` gains a second file format, with the same obligations as the first (version,
  migrations, golden files, round trip tests, a benchmark of reading and writing).
- `orchestra-ui` gains a token (`scrim`) in every one of the ten pairs and in the theme tests.
- New texts in the six catalogues with terminology research: the dialogs' buttons (*Save*,
  *Don't save*, *Cancel*, *Close*), the questions, the errors, the Preferences' sections and
  options, the pairs' names, *System / Light / Dark*, *About Orchestra*, *Untitled*, the file
  filter.
- The guides gain the Preferences' chapter and the opening and saving of a project, with
  screenshots (headless: the dialogs are layers of the same window, so `cargo xtask screenshots`
  renders them like any scene).
- Recorded limits: dialogs stay inside the main window; macOS has the modification mark in the
  title instead of the close button's dot; two processes writing the settings file, the last one
  wins; the language applies at the next start until F2.13.

## Precisions of the implementation (F2.12a2, approved by the owner on 2026-11-23)

Choices S1–S9 were approved by the owner before the code (all as recommended). Writing it gave
these precisions:
- **P1, the shortcuts are a list.** `"shortcuts": [ { "action": "file.save", "keys": ["Primary+s"] } ]`
  instead of the object `{ "file.save": [...] }` shown with S1: `serde_json` reads an object into a
  sorted map and keeps only the last of two equal keys, so AM7's "the later entry wins a conflict"
  would have no "later". A custom theme's overrides were already a list (T7).
- **P2, letters in lower case.** A shortcut's settings text writes the letter as the shortcut holds
  it, lower case with Shift as its own modifier (`Primary+Shift+z`, precision A of F2.11d1); AM7's
  example `Primary+Shift+Z` reads to the same shortcut. Modifiers are written Primary, Secondary,
  Alt, Shift and read in any order; a hand-written `Ctrl+s` is refused, not guessed.
- **P3, what is "broken".** A file is moved aside (S6) when it is larger than 1 MiB, does not start
  with `{ "orchestra_settings": N`, is not JSON, or is an older version whose migration fails. A
  file that cannot be read (permissions, a disk error) is not moved and is not written over during
  the session, like a newer one.
- **P4, a half-understood entry is dropped whole.** An override with an unknown kind or an extra
  key, a custom theme without a name or a base, a shortcut entry without `action` or `keys` are
  dropped with a warning; a repeated custom theme name keeps the first. A shortcut text that cannot
  be read is dropped alone, the action's other shortcuts are kept.
- **P5, not wired yet.** At start-up only the theme's pair and mode are applied. The shortcuts,
  the language, the blink and the fine modifier are read, kept and written back unchanged, and
  applied when the Preferences and F2.13 use them; the store's timed wake-up is hooked to the
  window with the first change the Preferences make (F2.12a4). The conversion of every entry is
  written and tested now.
- **P6, the shared engine.** The migration engine and the version scanner of the project file
  moved to `crates/io/src/migration.rs` and serve both files, each with its own version key;
  `orchestra_io::MigrationError` keeps its path.

## Amendment of DG2 and precisions of the implementation (F2.12a3a, owner's choices of 2026-11-23)

Choices C1–C9 were approved by the owner before the code (all as recommended): dialogs as layers
of the popup stack sharing the layer counter with the menus (C1); the panel centred, 320–480
logical pixels wide, its texts in a vertical scroll area and its buttons always visible (C2); the
buttons' order from the system, KDE Plasma recognised in `XDG_CURRENT_DESKTOP` (C3); the focus on
the default button, Enter, Escape, Tab inside, no mnemonics on the buttons (C4); `Ui::open_dialog`
with a `DialogSpec` and `Ui::open_dialog_with` for a panel of the caller's (C5); no icon (C6); only
the texts *OK*, *Cancel*, *Close* now (C7); nothing in the program opens a dialog until *About*
(F2.12a3b), which brings the screenshot and the guides (C8); the tests and benchmarks (C9).

- **The veil is replaced by a frame (amends DG2).** Measured: blending the translucent `scrim` over
  every pixel cost about 43 instructions per pixel in `tiny-skia`, so opening a dialog in a 1000 ×
  1000 window cost about 47 M instructions (about 90 M at 1080p), over the frame's budget of 33 M.
  The owner saw screenshots with and without the veil and with a thick accent frame, and chose
  **the frame and no veil**: the panel is drawn with a frame in the `accent` colour as wide as a
  new shape token, **`dialog-frame`** (3 logical pixels in every pair; at least 1 when a custom
  theme sets 0), and nothing is drawn over the window behind. Opening a dialog then costs about
  13 M instructions whatever the window's size (the panel's texts and glyphs included). The colour
  token `scrim` is not added; the window behind is not dimmed (a recorded limit: only the frame,
  the default button and the focus tell that it is blocked).
- **The root of a dialog's layer** spans the window and draws nothing; it centres the panel and
  consumes every pointer event outside it (WB9 already blocks them; the root makes it explicit).
- **Keys behind a dialog.** While a dialog is open the menu bar's keys (Alt, F10, the mnemonics)
  and every action's shortcut are off, except the standard editing actions on a focused field
  (ADR-0048 AM13).
- **Closing.** A dialog's buttons are ordinary buttons made with `closing_dialog()`: they send
  their message and ask (`EventCx::close_dialog`) for their dialog to close at the next frame, so
  the message is queued before the dialog goes; Enter and Escape on the panel do the same with the
  default's and the cancel button's messages.
- **Accessibility.** `AccessInfo` gains `description` and `modal`; the panel is a modal `Dialog`
  or `AlertDialog` named by its title and described by its message; the dialogs' roots are
  children of the window node after the root and before the menus.

## Precisions of the implementation (F2.12a3b, owner's choices of 2026-11-24)

Choices 1–4 of F2.12a3b were approved by the owner before the code (all as recommended):

- **1, the logo's converter runs at build time (fixes DG10's open point).** `crates/ui/build.rs`
  reads `assets/logo/orchestra-logo.svg` with our own converter (`crates/ui/build/logo_svg.rs`, no
  dependency) and writes the paths into `OUT_DIR/logo_paths.rs`, **never committed**: the logo is
  all rights reserved, not GPL, so its shape must not live in a source file with the GPL's SPDX
  header (CLAUDE.md §10), and a committed copy could drift from the asset. The converter supports
  exactly what the file uses (a square `viewBox` at 0 0, glyphs in `<defs>` placed by `<use x y>`,
  the absolute `M L C Z`, the non-zero rule) and refuses anything else with a typed error, so a
  future logo that needs more fails the build rather than being drawn with another shape. The
  script also checks that the logo stays inside its canvas, measuring each curve's extrema (a
  control point of today's drawing lies outside the canvas, the curve does not). The library's
  tests compile the converter too and check that the generated table is what it makes of the logo.
- **2, the repository's address is plain text.** No clickable link until a link widget and a way
  to open the system's browser (a dependency to evaluate) exist; noted for a later step.
- **3, the version is `CARGO_PKG_VERSION` only.** The commit hash comes with packaging (F6): a
  source archive has no `.git` to read it from.
- **4, the dialog's content.** The logo (128 logical pixels, the theme's `text` colour, uniform
  scale), the title *About Orchestra*, the version, and as details the licence, the note that the
  logo is not under it, the address and the bundled fonts (Inter, Bravura, JetBrains Mono, SIL Open
  Font License 1.1); one button, *Close* (role *cancel*: Escape presses it and it has the focus).

What the dialogs gained for it: `DialogKind::Information` (a plain `Dialog` for assistive
technologies), `DialogSpec::logo(label)` (the public widget `Logo`, an `AccessRole::Image` named by
its label) and any number of details (`DialogSpec::details`, the builder `detail` adding one). The
action `help.about` has the *About* role and no shortcut on any platform; `main.rs` now hands the
interface `system_button_order()` at start-up. Opening the About dialog over the benchmark panel
costs about 24 M instructions, 11 M more than a question, almost all of it the logo's fill: once
per opening, within the frame's budget.

## Precisions of the implementation (F2.12a4a, owner's choices of 2026-11-24)

The owner split F2.12a4 in F2.12a4a (the structure and Appearance) and F2.12a4b (Language,
Playback, Keyboard, the saved shortcuts, the screenshot and the guides' chapter), and approved
choices 1–8 as recommended:

- **1, the theme changes at run time.** A new `WindowRequest::SetTheme { pair, mode }` (a precision
  of ADR-0040): the window's theme state takes the new choice and remembers the system's last
  answer (back to *System* the system's half shows at once), redraws everything only when what it
  draws changed, and calls `winit`'s `set_theme` as DG9 says (also when the window opens with a mode
  chosen by hand); back to *System* on Windows and macOS the window's own theme is fed to the
  reporter, which drops a repeat. The application applies the pair's density, text and shape
  tokens with `Ui::set_theme`.
- **2, the settings live in the application.** `orchestra_ui::run` borrows the app (`&mut A`; a
  `&mut A` is a `WindowApp` by a forwarding implementation), so the program keeps the
  demonstration, hands it the `SettingsStore` (`attach_settings`) and takes it back after the loop
  to close it (`take_settings`, `close`), whatever way the loop ended. The window wakes for the
  earlier of the interface's and the store's wake-ups (P5 of F2.12a2 done).
- **3, three widgets.** `DropDown` (its list a menu of the interface made of `MenuItem::Choice`
  items, a precision of ADR-0048 AM9; the chosen value comes back as `WidgetEvent::Chosen`),
  `RadioGroup` (segments in one box, one Tab stop, the arrows choose at once) and `ListBox` (the
  sections). Each is one accessibility node with its value in `AccessInfo::value_text` (roles
  `ComboBox`, `RadioGroup`, `ListBox`; the list's values `MenuItemRadio`).
- **4, a custom theme in use stays in the list** after the ten built-in pairs; choosing among custom
  themes comes with the theme editor (F2.12b).
- **5, `Ui::set_caret_blink`.** Off, a field's caret stays solid and asks no wake-ups (ADR-0047
  TF8's blink becomes the user's choice); on again, it blinks from the next key, click or focus.
- **6, a large dialog with free content.** `Ui::open_large_dialog(LargeDialog { … })`: 720 × 520
  logical pixels within the window, at least 480 × 320 while the window allows it, a title, a
  content column the application fills, one *Close*. The action named `closed_by` is the only
  shortcut that works behind a dialog: while that dialog is on top it closes it, through the panel's
  own Escape (DG8: "the shortcut that opened it closes it too").
- **7, only Appearance now**: its pair, mode and blink; the other sections arrive with F2.12a4b.
- **8, the pairs' names** in the six catalogues, shown to the owner and approved: translated where
  they are common words (*Foresta*, *Wald*, *Bosque*…), *Studio* kept (es *Estudio*), *Nocturne*
  kept in French and German.

Measured: opening the Preferences over the benchmark panel about 10.4 M instructions, opening the
drop-down's list about 10.3 M, both within the frame's budget.

## Precisions of the implementation (F2.12a4b1, owner's choices of 2026-11-24)

The owner split F2.12a4b in F2.12a4b1 (Language, Keyboard, the saved settings at start-up, the
guides' chapter) and F2.12a4b2 (Playback: the values of ADR-0027 as a configuration of the
`playback` crate), and approved choices 1–6 as recommended:

- **1, the languages' names** are in the six catalogues, each written in its own language and the
  same in all (`language-en-us` … `language-es`), after *System language*; a drop-down, with the
  muted note that a new language applies at the next start (until F2.13). `main.rs` reads the
  language from the settings for the texts.
- **2, the Keyboard section** holds the modifier of fine adjustment only (a group of three options,
  the keys named as the platform writes them: the catalogue's names, Apple's symbols on macOS);
  the shortcut editor joins it with F2.12c.
- **3, Shift as the fine modifier**: Shift+wheel over a value control then adjusts finely; the area
  around scrolls with the wheel outside the control (a precision of ADR-0046 V5).
- **4, at start-up** the settings' blink, fine modifier and shortcuts apply when the program hands
  the store to its view; what cannot be used is reported on the standard error (S4 of F2.12a2).
- **5, the guides** have a chapter *Preferences* with placeholders for the pictures (ADR-0041 S7:
  the demonstration's window stays out of the guides); the scenes `preferences-dialog`,
  `preferences-language` and `preferences-keyboard` exist for when the pictures come in.
- **6**: the theme's change at run time (F2.12a4a) was checked by hand by the owner on 2026-11-24.

## Precisions of the implementation (F2.12a4b2, owner's choices of 2026-11-24)

Choices 1–6 approved as recommended: only the eighteen values of ADR-0027 §14–§15 (choice 1;
ADR-0027 §17 T1–T4 for the engine, choice 2); stable names with the unit in the file's `playback`
map, steps on the 0–127 scale and times in whole milliseconds, only the values that differ from the
defaults, unknown names kept, values out of range clamped and reported (choice 3); a slider with
its value written beside it, whole milliseconds so no decimal separator is needed (choice 4); the
section scrolls, in three groups (articulation accents, dynamic accents, changes of level), with
*Restore defaults* for the playback values only (choice 5); a *Playback* part in the guides'
chapter, saying the values are personal (choice 6). The sections are now Appearance, Language,
Playback, Keyboard, as DG9 lists them. Nothing plays yet in the window (F3): the program keeps the
values and computes the tuning from them when it is needed.


## Precisions of the implementation (F2.12a5a, owner's choices of 2026-11-24)

SCALETTA v0.92 split F2.12a5 in F2.12a5a (the file choosers, DG4) and F2.12a5b (the open project,
DG5). Choices 1–6 of F2.12a5a approved as recommended:

- **1, `rfd` 0.18.0 with `xdg-portal` and `wayland`** (measured again, as DG4 asked). Without
  `wayland`, `rfd` gives the portal no parent window on Wayland (the xdg-foreign handle is behind
  that feature) and the chooser would float free of our window. The Wayland crates are winit's, in
  the same versions: the feature adds no crate. New crates per platform: Linux +2 (`rfd`,
  `pollster`), Windows +1 (`rfd`), macOS +4 (`rfd`, `block2` 0.6, `dispatch2`,
  `objc2-uniform-type-identifiers`; `block2` 0.6 sits next to winit 0.30's 0.5, listed in
  `deny.toml` with winit 0.31 as its exit, ADR-0036 B7). On Linux the `orchestra` release binary
  grows by 245 288 bytes (16 040 520 → 16 285 808, +1.5%), by 181 248 once stripped (13 122 296 →
  13 303 544, +1.4%), this step's own code and texts included; Cargo's times give `rfd` 6.7 s of a
  clean release build, `pollster` and `rfd`'s build script about 1 s together.
- **2, telling "cancelled" from "failed"** (Linux only). `rfd` answers `None` in both cases (its
  portal call fails, it falls back on `zenity`, which may be missing too). Before showing a chooser
  the worker thread reads the `version` property of `org.freedesktop.portal.FileChooser` with
  `zbus` (a connection of its own, calls limited to 5 s); only when that fails does it look for
  `zenity` in the `PATH`. Neither: the answer is *unavailable* at once and the application shows
  its error dialog (*Files can't be chosen*, which names `xdg-desktop-portal` and the desktop's
  back end). Otherwise `rfd` runs and its `None` means *cancelled*. **Recorded limit:** a portal
  that answers on a system without libdbus-1 and without `zenity` still ends in silence (the
  portal is a D-Bus service, so its desktops ship libdbus-1); `docs/native-deps.md` says so too.
  Windows and macOS are not probed.
- **3, where the code lives.** `orchestra-ui`'s `platform/file_chooser`: `FileRequest`,
  `FileChoice` (`Chosen`, `Cancelled`, `Unavailable`) and the `FileChooser` trait, with `rfd` in
  one file. The window asks the app for a request after every batch of events
  (`WindowApp::take_file_request`, so a shortcut, a menu item, an assistive technology's action and
  macOS's menu all reach it) and gives the answer back with `WindowApp::file_chosen`, through the
  proxy (`ShellEvent::FileChosen`). The chooser's future is created on the main thread on Windows
  and macOS (`AppKit` must be called there; the sheet does not block) and on the worker after the
  probe on Linux; a worker thread waits for it with a twelve-line executor of our own on
  `std::task::Wake` (no new crate for Windows and macOS). If the worker cannot start, the answer
  is *unavailable*. `FileRequest` is not a `WindowRequest`: it carries texts, and `WindowRequest`
  stays `Copy`.
- **4, the blocking layer** is a layer of the dialogs' stack with an empty panel: nothing drawn, the
  pointer, the keys, typed text and Tab reach nothing, the menu bar and the shortcuts are off as
  under any dialog. It is not one of `Ui::dialogs` and the window node does not list it for
  assistive technologies (the system's chooser is the dialog to read). `Ui::choose_file` refuses a
  second request until `Ui::file_chosen`; the window starts one chooser at a time and **ignores
  the requests to close it** while one is open (the system's close button, Alt+F4, Quit), as
  Windows and macOS do with a modal chooser's parent. Input behind the layer allocates nothing (a
  test).
- **5, Open and Save as** are enabled and open the chooser (*Open project*, *Save project as*, the
  filter *Orchestra project* on `*.orchestra`, `orchestra_io::PROJECT_EXTENSION`; *Save as*
  proposes `Untitled.orchestra`). The chosen path is kept but not used until F2.12a5b, which also
  adds the extension to a saved name that lacks it (not every system adds it). New and Save stay
  disabled until then. The guides' chapter *Dialogs* has a part on the file choosers.
- **6, the texts** (`file-untitled`, `file-filter-project`, `file-open-title`, `file-save-title`,
  `file-chooser-unavailable-title`, `file-chooser-unavailable-message`) are in the six catalogues;
  the glossary gains *project* and *untitled*.

The owner checked a real chooser by hand on KDE Plasma, on Wayland and on X11, on 2026-11-24: it
works.

## Precisions of the implementation (F2.12a5b, approved by the owner on 2026-11-25)

DG5 is implemented by `crates/app/src/project.rs` (the logic, without widgets) and the
demonstration (the dialogs, the choosers, the title). The owner followed every recommendation
(choices 1–7) and, after a bug was found, chose both remedies of the last point.

- **1, the new document**: 4/4, ♩ = 120 not printed, C major, **one empty bar**, no tracks;
  constants of the app until the "new score" dialog exists. The proposal was no bar; the tests
  found that inserting bars into a document without bars broke validation rule 13, and the owner
  chose one bar (the model keeps at least one everywhere else) **and** the fix of the model
  (ADR-0026's precision of F2.12a5b).
- **2, closing**: Quit, the system's close button and our title bar's go through the open project
  (ADR-0040's and ADR-0049's precisions of F2.12a5b). With unsaved changes the question opens and
  the window stays; its answer closes it, or not. A second request while the question is open is
  refused without a second question.
- **3, the extension**: `.orchestra` is added to a chosen name whose extension is not already that
  (in any case: `Song.txt` becomes `Song.txt.orchestra`, `Song.` becomes `Song.orchestra`), on the
  `OsStr`. If the name so made is an existing file that the chooser did not confirm, we ask *“…”
  already exists. Do you want to replace it?* (*Replace* destructive, *Cancel*; with no default
  button the focus starts on *Cancel*, so Enter and Escape both keep the file).
- **4, modified** is DG5 as written (the revision differs from the saved one): changing and
  undoing still counts as modified, a recorded limit to review with note entry (F2.22).
- **5, reading and writing on the interface's thread**, synchronously: about 5.5 ms to save and
  11 ms to load the normal reference score (`orchestra-io`'s benchmarks), 168 / 396 ms for the
  heavy one. A worker thread comes with the automatic save (F2.14).
- **6, the errors**: one title per operation naming the file, one message per reason the user can
  act on (missing, not allowed, unreadable, too large, not a project, newer, damaged; not allowed,
  disk full, unwritable, internal), the path as the detail, and under a failed save "the file that
  was already there, if any, has been left as it was". The developers' details (`Display`) are not
  shown and wait for a log (the open decision S4 of F2.12a2). A failed open keeps the project; a
  failed save keeps it modified and drops what waited (New, Open, closing).
- **7, the question** *Save the changes to “Name”?* (*Untitled* for a project never saved), *Your
  changes will be lost if you don't save them.*, with *Save* (default), *Don't save*
  (destructive), *Cancel*. *Save* on a project never saved opens the *Save as* chooser first; what
  waited runs only after a save that succeeded or after *Don't save*.
- **The window title**: `Name — Orchestra`, with `• ` in front while modified, the same mark in
  every language (`window-title`, `window-title-modified`). The texts that hold a name are
  formatted once with a private-use placeholder (U+E000) and filled when shown, because the
  demonstration keeps formatted texts, not the localizer; Fluent still places the name, and a test
  checks that every language has exactly one placeholder.
- **The texts**: 25 new messages in the six catalogues; the glossary gains *don't save* (MuseScore
  Studio) and *replace* (GTK 4's file chooser, MuseScore Studio).

The owner checked New, Open, Save, Save as, the title and the errors by hand on 2026-11-25: it
works.

## Precisions of the implementation (F2.12c1, approved by the owner on 2026-11-25)

- **The Keyboard section** (DG9, choice 1): under the modifier of fine adjustment, the heading
  *Shortcuts*, a search field (choice 4: by name and by shortcut text, ignoring case, not
  accents), the list of every command (the disabled ones too) in the menus' order with its
  shortcuts at the right, taking the room left and scrolling; for the selected command, *Shortcuts
  of "Name"*, the list of its shortcuts ("No shortcut" when none) with *Add…*, *Remove* and
  *Restore defaults*; the capture box's row, with the reason a key cannot be assigned beside it;
  *Restore all shortcuts*. The rules of the editor are ADR-0048's precisions of F2.12c1.
- **Each section's grid has as many lines as the section uses**, so no empty line leaves a gap.
- **The questions about conflicts open over the Preferences** (dialogs stack, DG1); their answer
  comes back to the editor.
- **The texts** (choice 7): 17 new messages in the six catalogues; the glossary gains *shortcut*,
  *command*, *reassign*, *add*, *remove* and *search*. Texts holding several values (a command, its
  owner, a shortcut) use `Template`, F2.12a5b's placeholders generalised to several variables.

## Precisions of the implementation (F2.12b1, approved by the owner on 2026-11-26)

- **The Appearance section scrolls** (DG9): the theme editor's 24 colour rows do not fit a small
  window. The editor stands under the theme, the mode and the blinking cursor.
- **The theme drop-down lists every custom theme** after the ten built-in pairs, in the settings'
  order; a custom choice whose theme is missing shows Studio, as the window draws it.
- **Every change of the editor is applied and saved at once** (DG8): the whole window is the
  preview; deleting a theme asks first.
