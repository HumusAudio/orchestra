# ADR-0048 — Actions, shortcuts, menus and popups: one registry, contexts, one menu model, a popup layer

- **Status:** Accepted (owner, 2026-11-17; outline AM1–AM15 presented first, with the owner's
  choices on AM5, AM8, AM9 and AM14, then the text).
- **Date:** 2026-11-17
- **Implements:** ADR-0045 (retained widgets; WA3 leaves the table of window-level shortcuts to
  the menus; WB9 leaves the popups' outside click to them; WH1 overlays), ADR-0040 W5 (our own
  input events), ADR-0026 (commands and one undo history), ADR-0047 TF3 and TF7 (editing keys
  fixed inside a field, the field's local undo), ADR-0046 V3 (the fine modifier), ADR-0036 B5
  (`muda` only on macOS), ADR-0008 (six languages, Fluent), ADR-0006 (platform conventions),
  ADR-0007 (budgets).
- **Step:** F2.11a of SCALETTA (this ADR); F2.11c the popup layer and the tooltip, F2.11d the
  drawn menu bar and context menus, F2.11e `muda` on macOS. The settings file that stores the
  user's shortcuts comes with F2.12, the shortcut editor with F2.12c.

## Context

Every view of Orchestra will offer the same things three ways: from a menu, from a keyboard
shortcut and, often, from a button or a context menu. The owner asked on 2026-11-12 (recorded in
ADR-0046) that **every shortcut of a command be customisable**. Step input in the score (F2.22)
will use keys without modifiers (letters for pitches, digits for durations), as notation programs
do. ADR-0012 has us draw our own interface, so the menu bar, the menus, the context menus and the
tooltips are ours on Linux and Windows; on macOS the menu bar belongs to the system (ADR-0036 B5).

What exists already:
- Keys go to the focused widget and **bubble** to the root when it does not handle them (ADR-0045
  WA3); "window-level shortcuts are handled" there, "their table comes with the menus". A focused
  text field swallows plain character keys and Space and lets the primary modifier alone bubble
  (ADR-0047, precision of F2.10b2).
- `InputEvent::Key` carries the **logical** key only (`Key::Character`, `Key::Named`) and the
  abstract modifiers (`Modifiers { primary, secondary, shift, alt }`, Ctrl or ⌘ as the platform
  wants). `winit` 0.30.13's `KeyEvent` also has `physical_key`.
- Overlays (ADR-0045 WH1) let a widget own a second segment; a modal layer blocks the pointer
  outside it (WB9), and WB9 notes that a menu may close on such a click.
- Checked on 2026-11-17:
  - `muda` 0.21.2 (2026-10-08, Apache-2.0 OR MIT, MSRV 1.90) is still the latest release. It takes
    accelerators by logical key (`KeyAccelerator`) or physical key (`Accelerator`, which "cannot
    represent all shortcuts found on non-U.S. keyboard layouts"), reports activations on its own
    channel (`MenuEvent::receiver`, `set_event_handler`), and has the macOS application menu
    calls (`init_for_nsapp`, `set_as_windows_menu_for_nsapp`, `set_as_help_menu_for_nsapp`).
  - `winit` 0.30.13 has **no popup windows on Wayland** (no `xdg_popup` anywhere in its sources);
    `with_parent_window` exists for X11 and Windows only.
  - AccessKit 0.25.2 has the roles `MenuBar`, `Menu`, `MenuItem`, `MenuItemCheckBox`,
    `MenuItemRadio` and `Tooltip`, the properties `keyboard_shortcut`, `access_key` and
    `has_popup`, and the action `ShowContextMenu`.

The word **command** is taken: ADR-0026 calls *commands* the changes to the document, with the one
undo history. Many menu entries change no document (Zoom, Preferences, Quit); others produce a
command. The things a menu, a shortcut or a button invoke are therefore called **actions** here
(owner's choice, 2026-11-17).

## Decision

### AM1 — Actions, in one registry

An **action** is something the user can invoke by name. It has:
- a **stable identifier** in text (`edit.undo`, `file.save`, `view.zoom-in`): lower case, a group
  and a name separated by a dot. It is what the settings file stores (AM7), so it never changes
  once released; renaming one needs a migration of the settings file (CLAUDE.md §9);
- an internal **index** (`ActionId`, a small integer given at registration) for every lookup,
  so no string is compared or hashed while the program runs;
- a **name** (a Fluent key; AM10 for the menus' mnemonics), a **context** (AM3) and the
  **default shortcuts** of each platform (AM6), possibly none.

The **registry** is one table for the whole program. The generic machinery (`ActionId`,
`Shortcut`, `Context`, the registry, the menu model of AM9, the lookup of AM4) is in
`orchestra-ui`, which knows nothing of music; the concrete list of actions is in `orchestra-app`,
which registers them at start-up. An action that changes the document produces **one command** of
ADR-0026 (one undo step); what it does is the app's code, called with the action's index. There is
no second registry per view: a conflict between two views' shortcuts would then be invisible.

### AM2 — The state of actions

Each action has a **state**: enabled or disabled, and, for the toggles and choices, checked or
not. The app keeps the states in a table indexed by `ActionId` and updates it at the end of every
batch of events (ADR-0040 W2), marking only the actions whose state changed (Undo becomes enabled
after the first edit). The drawn menus read the table when they record; on macOS the copy kept for
`muda` changes only the items marked (AM9). A **disabled action ignores its shortcut**: the key is
consumed (nothing else happens) and no action runs, as in every desktop program.

### AM3 — Contexts

A shortcut belongs to one **context**: *Global* (always), or the context of a view: *Notation*,
*DAW*, later *Video* and others as views arrive. A view's root widget declares its context; the
**active contexts** are found by walking from the focused widget to the root, innermost first,
and *Global* is always last. A key is looked up in that order: a context's shortcut **covers** a
global one with the same keys while the focus is inside that view. This is what lets step input
take plain letters in the score without stealing them from the rest of the program.

### AM4 — The path of a key

A key press goes to the focused widget and bubbles as today (ADR-0045 WA3). Only if it reaches the
root unhandled is it looked up in the registry, in the active contexts of AM3. A widget therefore
always wins over a shortcut: arrows move inside a list, a focused text field keeps its letters,
Space and editing keys (ADR-0047 TF3), and only what it lets bubble (Ctrl+S) can reach an action.
Releases and repeats of a key that ran an action are consumed too; a repeat runs the action again
only for the actions marked *repeatable* (zoom, nudges), not for Save.

The lookup is a hash table from (context, shortcut) to `ActionId`, built when the registry or the
user's shortcuts change. A key costs one probe per active context (usually two or three): O(1)
each, no allocation.

### AM5 — What a shortcut is (owner's choices)

A **shortcut** is the abstract modifiers of `Modifiers` (*Primary*, *Secondary*, *Shift*, *Alt*)
plus **one key**. **Single combinations only** (owner's choice): no sequences such as "Ctrl+K,
then S". Sequences can be added later; the settings file has a version (AM7), so the change would
be a migration.

The key is the **logical key**, the character the keyboard layout gives (owner's choice): Ctrl+Z
is the key labelled Z wherever the layout puts it, as all three platforms do for their own
shortcuts. A letter is compared without case; Shift is a modifier of its own. Two fallbacks use
the **physical key**, which `InputEvent::Key` therefore gains (a precision of ADR-0040 W5, in the
step that writes the code):
- **(i) non-Latin layouts:** when the logical key is a letter outside the Latin script (Cyrillic,
  Greek…), the shortcut is looked up with the Latin letter of the US layout at that physical
  position, as GTK and Qt do, so Ctrl+Z still undoes with a Russian layout;
- **(ii) the row of digits:** the keys of the digit row match digits by position. On French AZERTY
  a digit needs Shift, and step input (F2.22) needs the durations on that row; a plain press of
  the key that types "&" (and "1" with Shift) counts as the digit 1 for shortcuts, as the same
  key does on QWERTY.

The numeric keypad's digits are separate keys (`Numpad1` …), so the two can be bound apart, as
notation programs do.

### AM6 — Default shortcuts, per platform

Each action lists its default shortcuts for **Linux**, **Windows** and **macOS** (a table chosen
by value, not by `cfg`, as ADR-0047 does for the editing keys, so Linux CI tests the three). An
action may have several (Redo on Windows has two). The table grows with the actions, step by step;
the conventions it starts from:

| Action | Linux | Windows | macOS |
|---|---|---|---|
| Undo | Ctrl+Z | Ctrl+Z | ⌘Z |
| Redo | Ctrl+Shift+Z | Ctrl+Y, Ctrl+Shift+Z | ⇧⌘Z |
| Cut, Copy, Paste, Select all | Ctrl+X, C, V, A | Ctrl+X, C, V, A | ⌘X, C, V, A |
| New, Open, Save | Ctrl+N, O, S | Ctrl+N, O, S | ⌘N, O, S |
| Save as | Ctrl+Shift+S | Ctrl+Shift+S | ⇧⌘S |
| Preferences | Ctrl+, | Ctrl+, | ⌘, |
| Quit | Ctrl+Q | Alt+F4 (the window's) | ⌘Q |
| Full screen | F11 | F11 | ⌃⌘F |
| Help | F1 | F1 | ⌘? |

The editing ones also reach a focused field (AM13). Alt+F4 on Windows is the system's own key for
closing a window: `winit` reports it as a close request, so the action is reached that way.

### AM7 — The user's shortcuts

The user can **add**, **remove** and **replace** the shortcuts of any action, and **restore** an
action's defaults, or all of them. The **settings file** (F2.12, with its version and migrations)
stores **only the differences** from the defaults: for each changed action its identifier (AM1)
and its full list of shortcuts, written with the abstract modifiers (`Primary+Shift+Z`), so the
same file means the same thing on every system; an empty list means "no shortcut". Until F2.12
the differences exist only in the API and in tests.

**Conflicts.** Two actions with the same shortcut in the same context are a conflict. The shortcut
editor (F2.12c) shows it at once and asks which action keeps the shortcut. A settings file that
contains one anyway (written by hand, or by an older version whose defaults changed) is loaded with
the **later entry** winning; the other action loses that shortcut and a warning is logged, never
an error that would stop the program. A context's shortcut that covers a global one (AM3) is
**not** a conflict, but the editor shows it. An unknown action identifier in the file (an action
removed) is kept in the file and ignored.

### AM8 — Keys that stay fixed

These keys cannot be given to actions; the registry refuses them, and the shortcut editor does
not offer them:
- the **navigation of the widgets**: Tab and Shift+Tab, the arrows, Enter, Escape, and Space on
  the controls (ADR-0045 WA4, ADR-0046): system conventions that assistive technologies and
  keyboard users rely on. Inside a view they may mean something of the view (arrows move the
  selection in the score), but as the widget's handling (AM4), not as actions;
- the **editing keys inside a text field** (ADR-0047 TF3);
- the keys that **open the menus** (AM12): F10 and Alt alone, Alt+letter for the mnemonics on
  Linux and Windows, the Menu key and Shift+F10.

The **fine adjustment's modifier** (ADR-0046 V3, *Primary*) is not an action: it becomes an
**option of the Preferences** (F2.12), outside the registry (owner's choice).

### AM9 — One menu model

The menus are described **once**, as a tree of items: an **action** item (its name, shortcut,
state and check come from the registry), a **submenu** (a name and children) and a
**separator**. The drawn menu bar (Linux, Windows) and the system menu bar through `muda` (macOS)
are both built from it; context menus use the same items. A submenu or a list of items can be
filled by the app when it opens (the recent files).

Some items have a **role**: *About*, *Preferences*, *Quit*, and *Window* and *Help* for
submenus. On macOS they go where the system wants them (the application menu, `muda`'s
`set_as_windows_menu_for_nsapp` and `set_as_help_menu_for_nsapp`); elsewhere they stay where the
model puts them. On Linux and Windows the Preferences are in **Edit → Preferences** (owner's
choice), as in MuseScore, GIMP and Ardour.

On macOS `muda`'s activations arrive on its channel; the window turns each into the same
"action invoked" the drawn menus produce, through the event loop's proxy (ADR-0040 W3), so the
app handles one kind of event. Its shortcuts are given to `muda` as `KeyAccelerator`s (logical
keys), the same as AM5.

### AM10 — Menu names and mnemonics

A menu item's name is the action's Fluent key; a submenu has its own. On Linux and Windows each
name has a **mnemonic**, the letter that opens or chooses it with Alt (in the bar) or alone (in an
open menu). The letter is a Fluent **attribute** of the message (`.mnemonic`), chosen by the
translator for each language, not a marker inside the text: the name stays clean for assistive
technologies, the search of actions and the tooltip. A test checks, in all six languages, that
each mnemonic is a letter of its name and that it is **unique among its siblings**. macOS has no
mnemonics; the attribute is ignored there.

The letter is **underlined only while Alt is held** (or after F10), as Windows and KDE do by
default; assistive technologies get it as the node's `access_key`.

### AM11 — The popup layer

Menus, submenus, context menus and tooltips are drawn in **one stack of layers above the main
tree**, inside the main window: each layer has its own root and segments, recorded after the
tree, and the pointer is tested against the topmost layer first. They **stay inside the window**:
a popup that would cross its edge is moved inside, and a submenu opens on the other side when
there is no room; in a window too small for it a popup can still be cut (a recorded limit, the
consequence of having no popup windows on Wayland).

A **menu layer** is modal in the sense of WB9, with its precision: a press **outside** an open
menu closes the whole menu **and is consumed** (it reaches nothing below), as GTK and Windows do.
Opening a menu **does not move the focus** away from the focused widget: the menu has a focus of
its own for its items, and when it closes (by choosing, by Escape or by a click outside) the
keyboard goes back to the same widget, so an Edit item acts on the field that had the focus
(AM13). A tooltip layer takes no input (AM14).

### AM12 — Using the menus

- **Menu bar** (Linux, Windows): F10, or Alt pressed and released alone, focuses its first menu;
  Alt+mnemonic opens one. Left and Right move between the menus, Down or Enter opens one, Escape
  closes back to the bar, a second Escape leaves it.
- **In an open menu:** Up and Down (wrapping, skipping separators and disabled items, which stay
  visible and readable), Home and End, Enter or Space chooses, Right opens a submenu and Left
  closes it, the mnemonic letters alone choose. Disabled items are not chosen.
- **With the pointer:** a press on a menu's title opens it; while a menu is open, moving over
  another title opens that one; a submenu opens after the pointer rests on its item for about
  **250 ms**, or at once on a press; choosing happens on release, so press-drag-release works.
- **Context menus** open with the secondary button, **on release on Windows and on press on Linux
  and macOS** (their conventions), with the Menu key, with Shift+F10, and with AccessKit's
  `ShowContextMenu`; from the keyboard they open at the focused widget, not at the pointer.
- **Accessibility:** the roles `MenuBar`, `Menu`, `MenuItem`, `MenuItemCheckBox` and
  `MenuItemRadio`; each item's shortcut in `keyboard_shortcut` (written in the platform's own
  style), its mnemonic in `access_key`; titles and items with a submenu say `has_popup`.

### AM13 — The Edit menu and the focused field

Cut, Copy, Paste, Select all, Undo and Redo ask the **focused widget** first. A text field
answers: the six act on its text and on its **local undo** (ADR-0047 TF7, TF12), and their state
(AM2) is the field's (Copy and Cut need a selection, Undo a local history). With the focus in a
field, Undo and Redo **never** reach the document's history, also from the menu: Undo is disabled
while the field's own history is empty, the rule TF7 already gives to Ctrl+Z. Without a focused
widget that answers, Undo and Redo act on the document (ADR-0026); Cut, Copy and Paste on the
selection of the view, when views have one.

### AM14 — Tooltips

A **tooltip** appears after the pointer has **rested 500 ms** on a control that has one; moving
then to another such control shows its tooltip at once, for 500 ms after the previous one went
away. It goes away on any key or button press, when the pointer leaves the control, or with Escape;
the pointer can move onto the tooltip without it disappearing (WCAG 2.2, 1.4.13: dismissible,
hoverable, persistent). It is placed under the pointer's hot spot, inside the window (AM11).

It shows the **full text** of a control shortened with an ellipsis (F2.09f, `is_elided()`) and,
on a control tied to an action, the action's **name and shortcut**. It appears for the pointer
only, **not** when a control gets the focus from the keyboard (owner's choice). It is not a node
for assistive technologies: they already have the full text (ADR-0045, F2.09f), and the shortcut
goes into the control's `keyboard_shortcut`. Its text never takes the focus.

### AM15 — Budgets and tests

- A key that reaches the registry: no allocation, under a microsecond for the lookup
  (instruction counts in F2.11d).
- Opening a menu: one new layer, a few segments, well within the 50 ms from input to frame of
  ADR-0007; moving between items re-records two items only.
- Showing or hiding a tooltip: one layer's segment, damaged alone.
- Headless tests for everything: the lookup in contexts, the fallbacks of AM5 with made-up
  physical keys, conflicts and the loading rule of AM7, the fixed keys of AM8, menu keyboard and
  pointer handling, the Edit items on a focused field, the tooltip's timing with made-up times
  (ADR-0045 WH2), the accessibility tree read with `accesskit_consumer`; the mnemonics' test in
  the six catalogues. On macOS `muda` is compiled and its menu built in CI (F2.11e).

## Alternatives rejected

- **Popup windows of the system** for menus and tooltips: impossible on Wayland with `winit`
  0.30 (no `xdg_popup`), and two behaviours on three platforms would break ADR-0006's single
  identity.
- **`muda` on Linux and Windows too**: it needs GTK on Linux (ADR-0036 B5), and a native menu bar
  on Windows would look unlike the rest of our interface.
- **Shortcuts by physical key** as the general rule: Ctrl+Z would move on AZERTY and QWERTZ, against
  what the platforms and their users expect; kept only for the two fallbacks of AM5.
- **Sequences of keys** now: not needed by any planned action; postponed (owner's choice).
- **One registry per view**: conflicts across views would be invisible, and the shortcut editor
  would need to merge them anyway.
- **The mnemonic as `&` inside the text**: the name would carry a marker everywhere else it is
  used (assistive technologies, search, tooltips), and translators could break it unnoticed.
- **Tooltips on keyboard focus**: postponed (owner's choice); screen readers already have the text.
- **Calling them "commands"**: the word is ADR-0026's, for changes to the document.

## Consequences

- New in `orchestra-ui` (F2.11c–d): `ActionId`, `Shortcut`, `Context`, the registry and its
  lookup, the action-state table, the menu model, the popup layer stack, the menu bar, menus and
  context menus, the tooltip. `InputEvent::Key` gains the physical key (precision of ADR-0040 W5).
- New in `orchestra-app`: the list of actions with their defaults (AM6), their Fluent names and
  mnemonics in the six catalogues, with the glossary research of CLAUDE.md §7 (F2.11d).
- `muda` 0.21.2 enters as a macOS-only dependency in F2.11e (ADR-0036 B5), with its size and
  build time measured there in CI; duplicate versions it brings go into `deny.toml` with their
  exit.
- F2.12 adds to the settings file the user's shortcuts (AM7) and the option of the fine
  modifier (AM8); F2.12c builds the shortcut editor on the registry, with the conflict rules of
  AM7.
- Recorded limits: single combinations only; popups cannot leave the window; tooltips only for
  the pointer.
