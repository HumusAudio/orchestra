# ADR-0053 — The interface's language while the program runs: views rebuilt from the model, the system's languages read

- **Status:** Accepted (owner, 2026-11-27; recommendations 1–8 of F2.13 presented in the briefing,
  the owner approved every one; this text writes them down).
- **Date:** 2026-11-27
- **Implements:** ADR-0008 (six languages, Fluent; it said nothing about *when* the language can
  change, nor how the system's is found), ADR-0050 DG8 (Preferences applied at once: the language
  was the one exception, "at the next start", until this step), ADR-0045 (retained widgets),
  ADR-0048 (one registry of actions, one menu model), CLAUDE.md §1 (views are projections of one
  model) and §6 (platform code behind traits in `platform/`).
- **Step:** F2.13 of SCALETTA.

## Context

Since F2.12a4b1 the Preferences offer *System language* and the six languages, but a choice
applied at the next start, and *System language* meant en-US. A musician who opens the program in
the wrong language should be able to fix it at once; one who shares a computer should find their
own language without looking for it.

The interface is made of **retained widgets** (ADR-0045) built with texts already formatted from
the catalogues: a widget holds the string, not the message's key. Changing the language therefore
means either teaching every widget its key (and its arguments) so it can format itself again, or
building the widgets again with the new texts. The program has, besides, a few pieces of state
that are not widgets and must survive: the open project with its undo history, the settings, the
registry of actions with the user's shortcuts, the theme.

## Decision

### LR1 — A new language rebuilds the views from the model and the settings

A language chosen in the Preferences applies **at once** (DG8 now holds for every preference).
The interface is shown again in the new language by **building its views again**, not by
retranslating each widget:

1. the texts are formatted from the new catalogue (one object of texts per view, as before);
2. what is **not** a view keeps living where it lives and is only **renamed in place**: the
   registry of actions gets its new names, mnemonics and key names (`Registry::set_texts`,
   `set_mnemonics`), so the user's shortcuts and the actions' states stay; the drawn menu bar gets
   the new model (`Ui::set_menu_bar_model`), our title bar's buttons their new names
   (`Ui::set_window_button_names`), the window's title follows the open project; on macOS the
   window builds the system's menu bar again (`WindowRequest::RebuildSystemMenu`);
3. the **views** (today: the demonstration's content) are removed and built again with the new
   texts, from the model and the settings; **view state that must survive** (a divider's place, a
   value that belongs to no model yet) is read before the old widgets go and handed to the new
   ones, never kept twice;
4. the Preferences, open while the user chose, open again on the section they showed, the focus
   on the language's drop-down.

The open project, its undo history, the settings and the theme are never touched: they are the
model and the settings, which the views only show (CLAUDE.md §1).

A change **waits** while a dialog other than the Preferences, or a file chooser, is open: the
language is chosen in the Preferences, where neither can be, and closing a question that waits
for its answer would lose it. (The API answers "not changed" then.)

**Why not keys in the widgets (rejected).** Every widget, every dialog and every view would have
to keep its messages' keys and arguments and format them again: 3–4 sessions through all of
`orchestra-ui` and the app, and a second way to get a text that every future widget must follow.
Rebuilding takes one function per view, and it is the shape the real views need anyway: the
notation and DAW views are projections of the model (CLAUDE.md §1) and will be rebuilt or
re-laid-out from it for many other reasons (a part shown or hidden, a layout change).

**The rule for future views:** a view must be able to be **built from the model, the settings
and its own small view state**; whatever of its state must survive a rebuild (scroll position,
selection, zoom) lives outside its widgets or is captured and handed back, as in point 3.

### LR2 — The system's languages come from `sys-locale`, behind a trait

`orchestra-ui`'s `platform::language` reads the user's languages, most preferred first, behind
the trait `LanguageSource`:

- `SystemLanguages` uses **`sys-locale` 0.3.2** (MIT OR Apache-2.0, 1Password; checked on
  crates.io on 2026-11-27, the latest, 47 million downloads): on Linux the environment, as
  gettext reads it (`LANGUAGE`, a list, then `LC_ALL`, `LC_MESSAGES`, `LANG`), with the codeset
  and modifier cut; on Windows `GetUserPreferredUILanguages`; on macOS
  `CFLocaleCopyPreferredLanguages`. It has no dependency on the three systems and declares the
  system's functions itself: no native library to install, nothing for `docs/native-deps.md`,
  and the `unsafe` stays inside it.
- `FixedLanguages` answers a list given in advance: what the interface holds until the program
  hands it the system's, so no test and no screenshot depends on the machine (screenshots stay
  en-US).

**Not chosen:** our own code for the three systems (the same calls, but with `unsafe` and FFI
of our own in Windows and macOS code we can only test in CI); `fluent-langneg` (ADR-0008
already rejected it: our rule is a few lines).

### LR3 — Matching a system's language to ours

The pure rule lives in `orchestra-i18n` (`Locale::matching`, `Locale::negotiate`), where it is
tested without a system:

- the tags are read in BCP 47 and POSIX forms (`de-CH`, `it_IT.UTF-8`, `de_DE@euro`), without
  regard to case;
- `it`, `fr`, `de`, `es` give that language whatever the region (`it-CH`, `de-AT`, `fr-CA`,
  `es-419`);
- `en` gives **en-GB** for Great Britain, Australia, New Zealand, Ireland, South Africa and India
  (the British musical words: *crotchet*, *bar*), **en-US** for every other region, Canada
  included (*quarter note*, *measure* there), and for `en` without a region;
- the first of the user's languages that one of ours serves wins, in the user's order; none →
  en-US, the source language. `C` and `POSIX` are skipped.

Matching allocates nothing (a test checks it).

### LR4 — When the system is asked

At start-up (the language *System language* stands for, the default of a new settings file), and
again **whenever the user chooses *System language*** in the Preferences. A change of the system's
language while the program runs is **not followed**: on Linux the environment of a running
process never changes, and on Windows and macOS such a change asks the user to sign out or to
restart programs anyway.

### LR5 — *System language* names the language it gives

The drop-down's first value reads *System language (Deutsch)*: the language the system's languages
give, named in its own language as the six others are (choice 1 of F2.12a4b1), as an argument of
the message `prefs-language-system` (no concatenation). The note "applies at the next start" is
gone from the six catalogues.

### LR6 — Cost

The whole change, from the choice to the drawn frame with the Preferences open again (one
catalogue parsed with en-US's fallback, every text formatted, the views built again, laid out,
recorded and drawn, the accessibility update), is measured by the benchmark `language` of
`orchestra-app`: **17.2 M instructions** when written (about 10 ms on the reference laptop, well
inside the 50 ms from input to frame of CLAUDE.md §5); about 0.6 ms natively on the owner's
machine without the Preferences open. It runs once per change, never on a frame's path.

## Consequences

- Every preference now applies at once; the guides say so.
- New views must follow LR1's rule (built from the model, the settings and a small view state);
  the notation and DAW views (F2B, F3) will be checked against it.
- Assistive technologies see new widget IDs after a change: the focus moves to the language's
  drop-down, which a screen reader reads in the new language (to be checked by hand with Orca in
  F2.13).
- `sys-locale` is a new dependency of `orchestra-ui` (small, no transitive crates on the three
  systems); `cargo deny` accepts its licence.
- After a change, closing the Preferences gives the focus back to nobody (the widget that had it
  before they opened was rebuilt, so it no longer exists).
