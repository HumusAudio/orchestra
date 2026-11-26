# ADR-0052 — Theme files: one custom theme in a versioned file of its own

- **Status:** Accepted (owner, 2026-11-26; choices 1–7 of F2.12b2b presented in the briefing, the
  owner approved every one, then this text).
- **Date:** 2026-11-26
- **Implements:** ADR-0037 T7 (custom themes: a base pair and its overrides) and its precisions of
  F2.12b1–F2.12b2a (colours per half; text, lines and density the same in both halves), ADR-0050
  DG4 (the system's file choosers) and DG6 (the settings file: JSON, versioned, read leniently),
  ADR-0029 S13 (safe saving), the migration engine shared since F2.12a2, CLAUDE.md §9 (every file
  format has a version and migrations from the first day). Modelled on ADR-0051 (shortcut files).
- **Step:** F2.12b2b of SCALETTA.

## Context

The theme editor (F2.12b1–F2.12b2a) makes custom themes, kept in the settings file with the
user's other choices. A musician also wants to **carry** a theme to another computer, give it to
a colleague or a class, or keep it aside before trying changes. As for the shortcuts (ADR-0051),
the settings file is not the vehicle (it holds the language, the shortcuts, the tuning, all
personal), and a new file format is permanent: it is decided here, with its version and golden
files from the start.

## Decision

### TF1 — The format

A UTF-8 JSON file holding **one** custom theme:

```json
{
  "orchestra_theme": 1,
  "theme": {
    "name": "Night",
    "base": "studio",
    "light": [{ "color": "accent", "value": "#3366ccff" }],
    "dark": [{ "color": "accent", "value": "#5b9bf0ff" }, { "size": "size-body", "value": 14 }]
  }
}
```

- `orchestra_theme` is the format's version (an integer), the first field, read before anything
  else, as the settings file's `orchestra_settings` and the shortcut file's `orchestra_shortcuts`.
- `theme` is exactly a settings file's custom theme (`CustomTheme`): its name, its base pair's
  stable identifier, and the overrides of each half, written and read by the settings file's own
  code, so a theme means the same thing in both files.
- The extension is `.orchestra-theme`; the code lives in `orchestra-io`
  (`orchestra_io::theme_file`), with the shared migration engine (an empty chain today) and a
  golden file of version 1.

### TF2 — Exporting: the chosen custom theme

*Export…* writes the custom theme chosen in the editor, as the settings hold it (only its
differences from the base, T7). A built-in pair cannot be exported: there is nothing of the
user's in it, and its base alone is a name every Orchestra already knows; the button is disabled
then. The name proposed is the theme's own (`Night.orchestra-theme`); the extension is added when
the typed name lacks it, and a file the chooser did not confirm is asked about before being
replaced, as *Save as* does (F2.12a5b). Writing is safe (a temporary file renamed over the
target).

### TF3 — Importing: always a new theme

*Import…* reads a file and **adds** its theme to the custom ones; it never replaces one. Nothing
the user had can be lost, so no question is asked. The name is made unique if it is taken
("Night (2)", the editor's rule) and the imported theme is chosen at once, so the user sees it. A
file whose theme has no name takes the file's name without the extension.

### TF4 — Reading leniently

As the settings file (DG6): an override with a wrong shape is dropped alone; unknown fields are
ignored. Then, in the application:

- a **base** this Orchestra does not know becomes Studio, the default pair;
- overrides of tokens this Orchestra does not know are **kept** (a newer Orchestra reading the
  theme again finds them, as unknown actions in ADR-0051) but do nothing here;
- **text, lines and density** that differ between the halves (a file written by hand) are made
  the same, the light half's winning, as ADR-0037's precision of F2.12b2a says.

Afterwards a short report says how many entries were not used (dropped, unknown, or a base
replaced), only when there are some: *Entries of "File" not used: 3*.

### TF5 — Failing clearly

The whole file fails, with translated texts naming it and nothing changed, when it cannot be read
(missing, not allowed, other I/O), is larger than **1 MiB** (a theme takes a few kilobytes; the
limit, checked before reading, keeps a wrong file chosen by mistake from being read whole), is not
JSON, does not start with `orchestra_theme` or holds no `theme` object, or was written by a newer
Orchestra.

### TF6 — Where

*Import…* and *Export…* beside *Duplicate* and *Delete* in the Preferences' Appearance section.
The demonstration remembers who asked for a file chooser (the project, the shortcut editor or the
theme editor), so each answer goes back to its asker.

## Alternatives rejected

- **Several themes per file**: a file then needs choices on import (which ones, what to do with
  names taken); one theme per file is what people pass around, and several files cover the rest.
- **Exporting a built-in pair** (its base and no override): a file that says nothing every
  Orchestra does not already know.
- **Importing by replacing a theme of the same name**: the user would lose their own theme
  without noticing; a numbered name keeps both.
- **The resolved theme** (every token's value) instead of the differences: the file would freeze
  the base's values of today, and tokens added later would not come from the base (T7).
- **A precision of ADR-0037** instead of an ADR: a permanent format deserves its own record
  (CLAUDE.md §9), as ADR-0051 for the shortcuts.

## Consequences

- One more versioned format, with its golden file and migrations; it shares the settings file's
  reader and writer of a custom theme.
- A theme made on one system looks the same on the others (the fonts are bundled, ADR-0044).
- Unknown tokens survive a round trip through an older Orchestra.
