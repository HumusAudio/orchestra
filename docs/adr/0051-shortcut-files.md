# ADR-0051 — Shortcut files: the whole keyboard in a versioned file of its own

- **Status:** Accepted (owner, 2026-11-25; choices 1–7 of F2.12c2 presented first, the owner followed every recommendation, then this text).
- **Date:** 2026-11-25
- **Implements:** ADR-0048 AM7 (the user's shortcuts, the loading rules), ADR-0050 DG4 (the
  system's file choosers), DG6 (the settings file: JSON, versioned, read leniently), ADR-0029
  S13 (safe saving) and the migration engine shared since F2.12a2, CLAUDE.md §9 (every file
  format has a version and migrations from the first day).
- **Step:** F2.12c2 of SCALETTA.

## Context

The shortcut editor (F2.12c1) changes the shortcuts and the settings file keeps the user's
differences from the defaults (AM7). A musician also wants to **carry** a keyboard layout to
another computer, share it with a colleague or a class, and get back a layout after trying
another one. The settings file is not the right vehicle: it holds the theme, the language, the
playback's tuning, all personal. A file of its own, made only for the shortcuts, is what every
notation program and DAW offers (MuseScore's shortcut files, Cubase's key commands, Logic's key
command sets).

A new file format is permanent (CLAUDE.md §9): it needs a version, migrations and golden files
from the start, so it is decided here rather than as a precision.

## Decision

### SF1 — The format

A UTF-8 JSON file, the same family as the settings file (DG6):

```json
{
  "orchestra_shortcuts": 1,
  "shortcuts": [
    { "action": "file.new", "keys": ["Primary+n"] },
    { "action": "edit.redo", "keys": ["Primary+Shift+z", "Primary+y"] },
    { "action": "help.contents", "keys": [] }
  ]
}
```

- `orchestra_shortcuts` is the format's version (an integer), the first field, read before
  anything else, as the settings file's `orchestra_settings`.
- Each entry is exactly a settings file's shortcut entry (`ShortcutEntry`): the action's stable
  identifier (AM1) and its whole list in the settings text with the **abstract modifiers**
  (`Primary+Shift+z`, `Shortcut::settings_text`), so the same file means the same thing on Linux,
  Windows and macOS. An empty list means "no shortcut".
- The code lives in `orchestra-io` (`orchestra_io::shortcuts`), next to the settings file, with
  the shared migration engine (an empty chain today) and golden files of version 1.

### SF2 — What is exported: the whole keyboard

Every action of the registry with its **whole list**, in registration order, not only the
differences from the defaults; then the kept entries of unknown actions (AM7). The file then
means the same whatever the defaults of the version that reads it: a layout carried to a newer
Orchestra whose defaults changed stays the layout that was exported. The settings file keeps
storing only the differences (AM7), which is right for it: the user's own changes follow the
program's new defaults.

### SF3 — What importing does

Importing **replaces** every shortcut: the registry starts from the defaults and applies the
file's entries in order (`Registry::load_overrides`, AM7's rules): the later entry wins a
conflict, fixed keys are dropped, unknown actions are kept (and saved in the settings, so a newer
Orchestra finds them). Actions the file does not name keep their defaults. Then the settings file
receives the differences, as after any change (DG8).

- **A question first**, only when the current shortcuts are not the defaults (there is something
  to lose): *Replace your shortcuts with those of "File"?*, with *Replace* (destructive) and
  *Cancel*.
- **A short report after**, only when something was not used (entries that could not be read,
  keys that are not shortcuts, fixed keys, conflicts): *3 entries of "File" were not used.* The
  details are for developers (the log, decision S4 of F2.12a2).

### SF4 — Reading leniently, failing clearly

As the settings file (DG6): an entry with a wrong shape is dropped alone (and counted, SF3).
Whole-file failures are errors with translated texts naming the file, as the project's
(F2.12a5b): it cannot be read (missing, not allowed, other I/O), it is larger than **1 MiB** (a
whole keyboard takes a few kilobytes; the limit, checked before reading, keeps a wrong file chosen
by mistake from being read whole), it is not JSON or not a shortcut file (no
`orchestra_shortcuts`), or it was written by a newer Orchestra. Nothing changes then.

### SF5 — The extension and the choosers

`.orchestra-shortcuts`, with the filter *Orchestra shortcuts* (translated). *Export…* proposes the
translated name *Shortcuts* (`Shortcuts.orchestra-shortcuts`), adds the extension when the typed
name lacks it, and asks before replacing a file the chooser did not confirm, as *Save as* does
(F2.12a5b). Writing is safe (a temporary file renamed over the target, ADR-0029 S13).

### SF6 — Where

*Import…* and *Export…* in the Preferences' Keyboard section, beside *Restore all shortcuts*. The
demonstration remembers who asked for a file chooser (the project or the shortcut editor), so
each answer goes back to its asker.

## Alternatives rejected

- **Exporting only the differences** (as the settings file): a shorter file, but its meaning
  changes with the defaults of the version that reads it; a shared layout would silently differ.
- **Importing as an addition** to the current shortcuts: the result would depend on what the
  user had before, and conflicts between the two would be resolved in ways hard to predict.
- **The settings file itself as the exchange format**: it carries personal settings that must not
  travel (the theme, the language, the tuning), and importing it would mean choosing parts.
- **A precision of ADR-0048 or ADR-0050** instead of an ADR: a permanent format deserves its own
  record (CLAUDE.md §9).

## Consequences

- One more versioned format, with its golden files and migrations; its reader shares the
  settings file's lenient entry reader.
- A layout made on one system works on the others (abstract modifiers), except the keys a
  platform fixes (AM8), which are dropped there and counted.
- Unknown actions survive a round trip through an older Orchestra.

## Implementation (F2.12c2)

Written as decided above; the owner checked exporting, importing (with the question and the
report) and a file that is not a shortcut file by hand on 2026-11-25: it works.
