# ADR-0047 — The text field: byte offsets on graphemes, platform editing keys, a local undo, IME, the clipboard

- **Status:** Accepted (owner, 2026-11-13; outline TF1–TF13 presented first, with the owner's
  choices on TF2, TF8, TF12 and the primary selection's postponement, then the text). Precisions
  of F2.10b1 below.
- **Date:** 2026-11-13
- **Implements:** ADR-0045 (retained widgets, routed events, focus, AccessKit per widget; WA8
  leaves the clipboard to this step), ADR-0044 (text: shaping, clusters, left to right only,
  TX5), ADR-0040 W2 and W5 (frames on demand, our own input events), ADR-0026 (one undo history),
  ADR-0037 (tokens and contrast pairs), ADR-0007 (budgets), ADR-0006 (platform conventions).
- **Step:** F2.10a of SCALETTA (the ADR and the clipboard measures); F2.10b realises the field
  without the clipboard and the IME, F2.10c the clipboard and the IME.

## Context

Every view of Orchestra needs to type text: track and instrument names, the search of commands,
the fields of the Preferences and of the dialogs, later lyrics, titles and expression texts in
the score. ADR-0012 chose to draw our own interface, so the text field, with everything a user
expects from one (caret, selection, keyboard and mouse conventions, clipboard, input methods for
languages that compose characters, a local undo, assistive technologies), is ours to build.

What exists already:
- `orchestra-text` shapes and wraps a text into a `TextLayout` whose glyphs carry the UTF-8 byte
  offset of their cluster (ADR-0044 TB4), and an empty text still has one line "so a caret has
  somewhere to stand". It lays out left to right only (TX5).
- `icu_segmenter` 2.4 is in the workspace for line breaking (ADR-0044 TX10). The same crate
  gives grapheme clusters (`GraphemeClusterSegmenter::new()`) and words
  (`WordSegmenter::new_for_non_complex_scripts`, rule-based, no dictionary data), so no new
  dependency is needed for boundaries.
- `winit` 0.30.13 reports the text a key produces (`KeyEvent::text`, which handles dead keys and
  compose sequences), input-method events (`Ime::Enabled`, `Preedit`, `Commit`, `Disabled`),
  and lets the window enable the input method and tell it where the caret is
  (`set_ime_allowed`, `set_ime_cursor_area`). On macOS the input method must be enabled even to
  receive dead-key text.
- AccessKit 0.25 describes editable text with a `TextInput` node and `TextRun` children
  (per-character byte lengths, positions, widths, word starts, a text selection) and the actions
  `SetValue`, `ReplaceSelectedText` and `SetTextSelection`; its AT-SPI, UIA and macOS adapters
  all implement them.
- The pointer shapes of F2.09e (ADR-0045 WG4) are ready to gain the text beam.

### The clipboard, measured (2026-11-13)

Candidates checked on crates.io the same day: `arboard` 3.7.0 (2026-10-15, MIT OR Apache-2.0,
1Password, by far the most used), `smithay-clipboard` 0.7.3 (2025-11-02, MIT), `window_clipboard`
0.5.1 (2025-12-12, MIT, iced's wrapper of the former), `copypasta` 0.10.2 (2025-04-25),
`clipboard-rs` 0.3.5 (2026-06-30). Measured on the real application (`orchestra-app`, release
profile, stripped, Linux), each variant with a probe that reads and writes the clipboard; build
times are incremental, so indicative:

| Variant | Binary | Crates added | Build | Pure Wayland |
|---|---|---|---|---|
| A. `arboard`, no default features (X11) | +187 KB | 5 | ≈ 0 s | through XWayland only |
| B. A + feature `wayland-data-control` | +561 KB | 13 (`wl-clipboard-rs`, `petgraph`, `nom`, `tree_magic_mini`…) | ≈ +2 s | native where the compositor offers *data-control*, XWayland elsewhere |
| C. A + `smithay-clipboard` on Wayland | +661 KB | 11 (a second `smithay-client-toolkit`, 0.20 beside `winit`'s 0.19, and `calloop`) | ≈ +80 s | native everywhere (standard `wl_data_device` on the window's own connection) |

*Data-control* is a protocol meant for clipboard managers. As listed by wayland.app on
2026-11-13, `ext-data-control-v1` is offered by KWin 6.7, Sway 1.11, Hyprland, COSMIC and niri,
not by Mutter (GNOME) nor Weston; the deprecated `wlr-data-control` is no better for GNOME. With
variant B, `arboard` falls back to the X11 clipboard where data-control is missing, which on
GNOME means XWayland (started on demand by Mutter, which keeps the two clipboards in sync).
Variant C needs the window's `wl_display` pointer, an `unsafe` call in `orchestra-ui`, whose
`#![forbid(unsafe_code)]` would need an exception of its own. None of the three brings a new
native library: `libwayland-client` is already loaded with dlopen by `winit`
(`docs/native-deps.md`), and `wayland-backend` is shared.

## Decision

### TF1 — The text and its positions

The field's text is a `String`. The caret and the selection are **UTF-8 byte offsets** into it:
the selection is an *anchor* (where it started) and a *focus* (where the caret is); they are
equal when nothing is selected. Every offset the field stores lies on a **grapheme cluster
boundary** (UAX #29, `icu_segmenter`), so the caret never stands between a letter and its accent
or inside an emoji sequence; words for Ctrl+arrows and double clicks come from the word
segmenter without dictionaries (good for the six languages; Thai or Chinese words would need
the dictionary data, a recorded limit with TX4–TX5). Left to right only (ADR-0044 TX5): the
visual order is the logical order, so moving right always means a larger offset.

Byte offsets rather than character or grapheme indices because they are what `String` slices
by, what `TextLayout`'s clusters carry, what `winit`'s preedit cursor reports and what AccessKit's
character lengths are counted in: one unit everywhere, no conversion.

Caret geometry comes from the layout's clusters: `caret_x(offset)` (the x of the cluster boundary
at `offset`; inside a ligature, the cluster's advance is split evenly among its graphemes) and
`hit_test(x)` (the nearest grapheme boundary). They belong to `orchestra-text` (ADR-0044 TB4).

### TF2 — One line now (owner's choice)

The field edits **one line**. Enter commits (TF7) instead of inserting a line break; a pasted
text has its line breaks replaced by spaces. A multi-line field (vertical movement keeping the
column, Page Up/Down, wrapping) comes when a real need appears (project notes, long texts):
nothing in TF1 or TF3–TF13 prevents it, since `TextLayout` already has lines. SCALETTA §1 rule 6.

### TF3 — Editing keys: the platform's, fixed inside the field

The keys follow the platform's convention (ADR-0006) through the abstract modifiers of ADR-0040
W5:

| Action | Linux, Windows | macOS |
|---|---|---|
| Previous / next grapheme | ← / → | ← / → |
| Previous / next word | Ctrl+← / Ctrl+→ | ⌥← / ⌥→ |
| Start / end of the line | Home / End | ⌘← / ⌘→, Home / End |
| Extend the selection | Shift + any of the above | Shift + any of the above |
| Delete before / after | Backspace / Delete | ⌫ / ⌦ (Fn+⌫) |
| Delete the word before / after | Ctrl+Backspace / Ctrl+Delete | ⌥⌫ / ⌥⌦ |
| Select all | Ctrl+A | ⌘A |
| Cut, copy, paste | Ctrl+X, C, V (and Shift+Delete, Ctrl+Insert, Shift+Insert) | ⌘X, C, V |
| Undo, redo | Ctrl+Z, Ctrl+Shift+Z, Ctrl+Y | ⌘Z, ⌘⇧Z |

With a selection, ← and → collapse it to its start or end; typing replaces it. These keys are
**fixed inside a focused field**, like Tab and the arrows of every widget: they are the
platform's text conventions, which the owner's reading of "every shortcut is customisable"
(2026-11-12) leaves out. When the command registry and the Edit menu arrive (F2.11), the menu's
Cut, Copy, Paste and Undo act on the focused field; a remapped *command* shortcut never changes
what a key does inside a field. Unhandled keys (Escape aside, TF7) bubble as before (WB7).

### TF4 — The mouse

A press puts the caret at the grapheme boundary nearest the pointer (`hit_test`), and focuses the
field; dragging (with capture) selects, and past the field's edges scrolls the text (TF10). A
double click selects the word under the pointer (dragging then extends by words), a triple click
selects everything (the click count of ADR-0046 V4). Shift+press extends the selection from its
anchor. Over the editable text the pointer takes the **text beam** (`PointerShape::Text`, mapped
to `winit`'s `CursorIcon::Text`; ADR-0045 WG4). A disabled field shows the arrow and takes no
focus.

### TF5 — Text input events

Two new input events (ADR-0040 W5):
- `InputEvent::Text(String)`: the text a key press produced, from `KeyEvent::text`, not from
  the logical key: dead keys and compose sequences then work, and Ctrl+letter (no text, or a
  control character) does not type. Control characters are dropped.
- `InputEvent::Ime(…)`: `Enabled`, `Preedit { text, cursor }`, `Commit(text)`, `Disabled`.

Both go to the focused widget only (WA3). A new hook, `Widget::accepts_text()`, says whether the
focused widget takes text; the interface reports it (`Ui::wants_text_input`), and the window
calls `set_ime_allowed` when the answer changes, so the input method is on exactly while a field
has the focus (on macOS this is also what makes dead keys work; on X11 enabling it turns off
dead-key reporting in key events, which is why text comes from `KeyEvent::text`).

### TF6 — Input methods (IME)

While the input method composes, its **preedit** text is shown at the caret, underlined with a
one-pixel line in the text's colour, with the IME's own cursor inside it; it is not part of the
field's text, not selectable and not in the undo history. `Commit` inserts its text as typing
does (one undo step). The interface tells the window where the caret is (a rectangle in logical
pixels), and the window passes it to `set_ime_cursor_area` whenever it moves, so the candidate
window opens beside the caret (X11 uses only its position). Clicking elsewhere, losing the focus
or pressing Escape during a composition ends it the way the platform does (the IME sends its
`Commit` or an empty `Preedit`).

Risk, handled in F2.10c with tests by hand on X11 and Wayland (with Fcitx5 or IBus) and in CI
on Windows and macOS: some platforms send a key's text both as a key event and as an IME commit,
others send plain keys only as key events even with the input method on. Two candidate rules,
chosen in F2.10c by what those tests show (and written there as a precision of this ADR): (a)
with the input method enabled, insert only `Ime::Commit` and ignore `InputEvent::Text`, on the
platforms where every key reaches the IME; (b) insert both, dropping a `Text` that repeats the
`Commit` just received for the same key press.

### TF7 — Undo inside the field, and the document's undo

A focused field keeps its **own undo history** of its edits, dropped when it loses the focus.
Steps are grouped deterministically (ADR-0026 rejects grouping by time): consecutive typing is
one step until a word boundary is crossed (a space or punctuation after letters), the caret is
moved, the selection changes, or the kind of edit changes (typing, deleting backwards, deleting
forwards); a paste, a cut and a "delete word" are steps of their own. Redo is cleared by a new
edit. Ctrl+Z on an empty local history does **nothing**: it never reaches the document's undo
from inside a field, because undoing a note while typing a name would be a surprise.

A field that edits the **document** (a track's name) changes it once, at the **commit**: Enter,
or the focus leaving the field. The commit sends one message (`on_commit`), which the view turns
into **one command** of the single history (ADR-0026), only if the text changed. **Escape**
restores the text the field had when it got the focus and commits nothing; a second Escape
bubbles (to close a dialog). Fields whose text is not document data (a search) can also send a
message at every edit (`on_change`), for live filtering.

### TF8 — The caret blinks, and stops (owner's choice)

The caret is a 1.5-pixel bar (`text` colour, rounded to device pixels) the height of the line.
It **blinks**: 530 ms on, 530 ms off (the Windows default, close to GTK's 1200 ms period), shown
solid again at every edit or move, and it **stops blinking, visible, after 10 s without input**
(as GTK does), so an idle window goes back to sleep. Blinking needs the window to wake without
an event: a **precision of ADR-0040 W2**, a timed wake-up (`ControlFlow::WaitUntil`) that the
app asks for (`WindowApp::next_wakeup`), used only while a focused field blinks; each blink
re-records the caret's small segment, so only it is redrawn. A preference to turn the blink off
comes with the Preferences (F2.12); without focus the caret is hidden.

### TF9 — The selection's look

The selected range is a `selection` rectangle behind the text; the selected glyphs are drawn
again in `on-selection`, clipped to that rectangle, so a glyph cut by the selection's edge is
correct on both sides. The pair `on-selection` on `selection` is already one of the WCAG
contrast pairs (ADR-0037). Without focus the selection is kept but not drawn (the platform
convention for single-line fields).

### TF10 — Scrolling inside the field

When the text is wider than the field, it scrolls horizontally to keep the caret in view, with a
margin (a few characters) so the text before it can be seen; deleting at the end brings the text
back to fill the field. A field inside a scroll area asks the areas around it to show it when
it gets the focus (`Widget::scroll_into_view`, WF5). The field's padding is at least the focus
ring's reach (WF4).

### TF11 — Accessibility

The field is a `TextInput` node (named by `labelled`, as the slider) with its value, and one
`TextRun` child for its line, carrying the per-grapheme byte lengths, their x positions and
widths, the word starts and the text selection; the runs are rebuilt with the field's layout and
sent only when they change (WC3). Assistive technologies may set the value, replace the selected
text and set the selection; each goes through the same edit paths as the keyboard (and into the
local undo). A password field (`PasswordInput`, no clipboard, masked runs) is not built now.

### TF12 — The clipboard: `arboard` with data-control (owner's choice)

Variant B of the measures: `arboard` 3.7.0 without default features (no images) plus
`wayland-data-control`. Native on Windows, macOS, X11 and on the Wayland compositors that offer
`ext-data-control` (KDE Plasma, the owner's desktop, Sway, Hyprland, COSMIC, niri); on GNOME it
falls back to the X11 clipboard through XWayland. It lives in `orchestra-ui`'s `platform/`
behind a trait (`ClipboardSource`, text only: read, write), so variant C can replace it
without touching any widget if GNOME users meet problems; the trait also gives tests an
in-memory clipboard, since no test may need a display (CLAUDE.md §3).

The window owns **one** clipboard object for the whole session: on Linux the application that
copied serves the paste while it lives (`arboard` keeps a thread for it), so a short-lived object
would lose what was copied. What was copied is lost when Orchestra closes, unless a clipboard
manager keeps it (recorded limit, as for every X11 and Wayland application). Reading the
clipboard can block for a moment (an X11 round trip); it happens only on a user's paste, on the
interface thread, never on the audio thread.

The Linux **primary selection** (select to copy, middle click to paste) is **postponed** by the
owner and recorded: the trait gains it later (`arboard` supports it on X11 and with
data-control).

### TF13 — Budgets and tests

- A key typed in a field of 60 characters, with shaping, the caret and the redraw of the field,
  stays under **0.5 M instructions** like every event (ADR-0045, WF2); a caret blink under
  0.1 M. Benchmarks in F2.10b, with the field in the 200-widget panel.
- Typing, moving and deleting do not allocate once the text and the layout have their capacity
  (a test with the allocation counter, ADR-0016); the undo history allocates per step, which is
  allowed: it is not a hot path.
- Property tests: random edits and moves keep every offset on a grapheme boundary and inside the
  text; undo of every step returns the text and the selection exactly; the field and a naive
  `String` model agree.
- Headless tests of every key of TF3 on each platform's table (the convention is chosen by a
  value, not by `cfg`, so Linux CI tests the macOS table too), of the mouse (TF4), the IME
  sequence (TF6) with synthetic events, the commit and Escape (TF7), the blink's wake-ups (TF8),
  and the accessibility node (TF11).

## Precisions of the implementation (F2.10b1, 2026-11-13)

Found while writing the editing core, within the decisions above:
- **TF1.** The boundaries are free functions of `orchestra-text` (`next_grapheme`,
  `prev_word_start`, `word_at`…), each O(n) from the text's start, no allocation. The layout
  records **caret stops** (the pen at each cluster's start and at each line's end);
  `caret_x` is a binary search on them (252 instructions on 40 characters) and interpolates only
  inside a ligature; `hit_test` walks the UAX #29 boundaries once alongside the stops (6 977
  instructions; a first version was O(n²), 225 585, and was rewritten). The candidates are UAX #29
  boundaries, not the shaper's clusters, which can split a flag. Recording the stops costs shaping
  1.7% more.
- **TF2.** Pasted tabs become spaces, like line breaks.
- **TF3.** Ctrl+→ goes to the end of the word on Linux and macOS, to the start of the next word on
  Windows (`EditConvention::word_forward_to_start`); ↑ and ↓ go to the ends of the field on macOS
  and are not mapped on Linux and Windows (they bubble); Shift+Backspace deletes like Backspace;
  macOS's ⌃ (Emacs-style) keys are not mapped; keys with modifiers the tables do not use map to
  nothing and bubble. Without Shift, a word or line movement collapses a selection to its edge
  and goes on from there.
- **TF5.** `typed_text` drops control characters and the text of keys pressed with the primary
  modifier without Alt (⌘A types "a" on macOS; AltGr is Ctrl+Alt on Windows); the text event
  follows its key event. A field swallows plain character keys so they do not reach single-key
  shortcuts (F2.10b2).
- **TF7.** The history stores every edit's texts in one arena with ranges; typing and forward
  deletions extend the last edit, backward deletions add edits to the group. After an edit the
  caret goes to the grapheme boundary at or after it (a typed regional indicator can make a flag
  with the next one).
- **TF13.** Measured: a typed key 4 175 instructions in the editing core, a word movement 12 206;
  editing allocates nothing within `TextEditor::reserve`'s capacity (tested).

## Alternatives rejected

- **Character or grapheme indices** instead of byte offsets: one more conversion at every
  boundary with the layout, `winit` and AccessKit, for nothing in return.
- **Grouping undo steps by time** (typing within 500 ms is one step): non-deterministic and
  untestable, the reason ADR-0026 rejects it for the document.
- **Ctrl+Z falling through to the document's undo** when the field's history is empty: a surprise
  that can undo musical work from a name field.
- **Editing keys in the command registry**: they are the platform's text conventions, part of
  what users expect from every field, and remapping Ctrl+← in one program only breaks habits.
- **Typed text from the logical key** (`Key::Character`): dead keys and compose sequences would
  not work, and on X11 with the IME enabled they are not reported at all.
- **A steady caret**: offered to the owner, who chose blinking with the 10 s stop.
- **A multi-line field now**: offered to the owner, postponed until a need appears.
- **Variants A and C of the clipboard** (measures above): A leaves pure Wayland sessions without
  a clipboard; C costs a second toolkit copy, 80 s of build and an `unsafe` exception, for GNOME
  only, where B already works through XWayland.

## Consequences

- `arboard` 3.7.0 (with `wl-clipboard-rs` 0.9.5 and its dependencies) enters `orchestra-ui` in
  F2.10c; duplicate versions it brings go into `deny.toml`'s `skip` with their exit (ADR-0036
  B7); `docs/native-deps.md` notes that it adds no native library.
- New in the toolkit: `InputEvent::Text` and `InputEvent::Ime`, `Widget::accepts_text`,
  `PointerShape::Text`, `WindowApp::next_wakeup` (precision of ADR-0040 W2) and the IME calls of
  the window, `caret_x`/`hit_test` in `orchestra-text`, the `ClipboardSource` trait.
- The field's visible texts (placeholders, the accessibility names) come from the catalogues of
  the views that use it; the field itself writes none.
- Recorded limits: one line; left to right; words of complex scripts without dictionaries; no
  primary selection; the clipboard's content lost at exit without a clipboard manager; on GNOME
  the clipboard goes through XWayland.
