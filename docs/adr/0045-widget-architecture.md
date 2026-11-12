# ADR-0045 — Widget architecture: a retained tree, our own layout, routed events, AccessKit per widget

- **Status:** Proposed (the owner approved the outline WA1–WA8 on 2026-11-12; this text awaits
  acceptance). Closes decision D12 of SCALETTA on acceptance (its clipboard part moves to F2.10).
- **Date:** 2026-11-12
- **Implements:** ADR-0012 (our own interface, no toolkit), ADR-0039 (the segmented display
  list: a widget is a segment), ADR-0040 W5 and W7 (our input events; the AccessKit adapter at
  the preparation point), ADR-0026 §5 (change sets drive the views), ADR-0037 and ADR-0044 TX11
  (widgets draw with theme tokens and text tokens), ADR-0007 (budgets).

## Context

SCALETTA step F2.08 fixes how the interface's pieces are organised before the first widgets
(F2.09) are written: whether widgets are **retained** (objects that live between frames) or
**immediate** (a function that describes the interface again at every event), how they are laid
out, how events reach them, how focus moves, and how each is exposed to assistive technologies
through AccessKit from the start. It is one of the structures that are hard to change later
(SCALETTA §1.3): every widget, panel and view will be written against it.

The decision was taken on measurements. The spike `spikes/widgets` (`RESULTS.md`, 2026-11-12)
built the same settings panel of 198 widgets (66 rows of label, checkbox and button) on our real
display list, text crate and renderer, three ways, and counted instructions (1 ≈ 1 ns on the
reference laptop):

| Case | Retained | Immediate | Immediate + text cache |
|---|---|---|---|
| click on a checkbox, drawing included | **0.17 M** | 33.2 M | 27.2 M |
| pointer onto a button | **0.25 M** | — | 27.2 M |
| click, interface work only | **0.002 M** | 7.26 M | 1.24 M |

An immediate interface re-records every segment at every event, so the display list damages
and redraws the whole panel (≈ 26 M); avoiding that requires comparing each widget's new
recording with the previous one, which is retained state by another name. Without a text cache
it also shapes every text again (≈ 45 000 instructions each). AccessKit updates carry only the
nodes that changed: one with retained widgets, the whole tree with immediate ones.

For layout, `taffy` 0.15.1 (MIT, flexbox and grid, +362 KB) lays the panel out in 1.6 M
instructions (0.31 M after one change), against 3 300 for a fixed column of our own. Facts
checked on crates.io on 2026-11-12: `accesskit` 0.25.2, `accesskit_winit` 0.34.2 and
`accesskit_unix` 0.24.1 are unchanged since ADR-0036 and still on zbus 5.19, so ADR-0038 S7's
exceptions in `deny.toml` stay; `arboard` 3.7.0 is the clipboard candidate for F2.10.

The owner approved the outline WA1–WA8 below on 2026-11-12.

## Decision

### WA1 — Retained widgets (D12)

The interface is a **tree of widgets that live between frames**. Each widget keeps its state
(checked, hovered, pressed, its text layout, its value), its rectangle from the last layout, and
whether it needs to be laid out or recorded again. An event changes the widgets it concerns;
only those re-record their segment (WA6) and only their AccessKit nodes are sent (WA5).

- **The tree** is an arena owned by the window's application (`WindowApp`, ADR-0040): widgets in
  a vector, each with its parent and its children, found by a `WidgetId` (a non-zero `u64` from
  one counter, never reused, as the model's IDs are, ADR-0022). Removing a widget removes its
  subtree. No reference counting, no interior mutability: the tree is mutated through itself.
- **A widget** implements one trait with five duties: measure (WA2), arrange (WA2), record its
  segment (WA6), handle an event (WA3) and describe its AccessKit node (WA5). Containers (WA2)
  are widgets too.
- **What happens on a frame**: events are handled; widgets that changed size are laid out again
  (WA2); widgets that changed appearance re-record their segment; changed AccessKit nodes are
  sent; the renderer draws the damage (ADR-0039 D5). An idle interface does nothing (ADR-0040 W2).

Rejected:
- **Immediate widgets** (egui's model): 27–33 M instructions per event on the spike's panel
  against 0.17 M, the whole AccessKit tree at every event, and texts that must be cached anyway.
- **A declarative layer with reconciliation** (Elm, iced, Xilem: the application describes the
  interface at every state change and a reconciler updates the retained tree): convenient for
  application code, but a second engine to write and keep correct before the first widget. It can
  be added later **on top of** this retained tree if writing views proves verbose; this ADR does
  not prevent it.

### WA2 — Our own layout: stack, row, column, grid

Layout is ours, in two passes over the tree, both linear:

1. **Measure** (bottom up): each widget answers, for given **constraints** (a minimum and a
   maximum width and height, logical pixels), the size it wants. A text widget measures its
   `TextLayout` (wrapping at the maximum width when it may wrap); a container combines its
   children's answers.
2. **Arrange** (top down): each container gives each child its rectangle.

The containers are **stack** (children on top of each other), **row**, **column** (with spacing,
padding and cross-axis alignment) and **grid** (rows and columns of tracks). Each child or track
has a size rule: **fixed** (pixels), **content** (what the child measures) or **fill** (a share
of the remaining space, by weight). The spacing values come from the density tokens of F2.09.

**Incremental**: a widget whose size may have changed (its text changed, a child was added) marks
itself and its ancestors as needing layout; the next frame measures and arranges only those
paths, and a container whose measured size did not change stops the propagation upwards. Each
widget caches its last constraints and answer. Complexity: O(widgets on the changed paths); a full
layout is O(widgets).

Rejected: **`taffy`** (CSS flexbox and grid): 500 times the cost of a column on the spike's panel
(1.6 M against 3 300), +362 KB, and a generality (CSS's full sizing algorithm) that panels,
inspectors and dialogs do not need. The score and the timeline lay themselves out with their own
engraving and time maps (F2B, F3), not with a widget layout.

### WA3 — Events: hit testing, capture, focus, bubbling

The window's input events (ADR-0040 W5) are routed as follows:

- **Pointer**: the event goes to the **topmost widget under the pointer**, found by walking the
  tree from the root with the rectangles of the last layout, last child first (the one drawn on
  top). Hovering is tracked: when the widget under the pointer changes, the old one gets
  "pointer left" and the new one "pointer entered", so each can re-record its hover state.
- **Capture**: a widget that handles a button press **captures** the pointer until the button is
  released: during a drag (a slider, a scroll bar, a note in the score) it receives every move and
  the release, even outside its rectangle.
- **Keyboard and text input**: they go to the **focused** widget (WA4); a widget that does not
  handle a key lets it **bubble** to its parent, up to the root, where window-level shortcuts are
  handled (their table comes with the menus, F2.11). Text input and IME go to the focused widget
  only (F2.10).
- **Wheel**: to the widget under the pointer, bubbling up to the nearest one that scrolls.

Each handler says whether it handled the event; routing and hit testing allocate nothing.

### WA4 — Focus

One widget at most has the keyboard focus. **Tab** moves it to the next focusable widget in tree
order (depth first, as the reader sees the layout), **Shift+Tab** to the previous one; a click on
a focusable widget focuses it. The focused widget draws a ring with the `focus` colour and the
`focus-width` shape token (ADR-0037). A **modal** subtree (a dialog, F2.12) **traps** the focus:
Tab cycles inside it until it closes, and the focus returns to where it was before it opened.
Focus changes are sent to AccessKit (WA5).

### WA5 — AccessKit: one node per widget, incremental updates

- Every widget is an AccessKit node whose `NodeId` is its `WidgetId` (stable for its life). It
  describes its role, label, value, state (checked, disabled, focused) and bounds; containers list
  their children.
- The first frame sends the whole tree; later frames send **only the nodes that changed** (a
  widget marks its node dirty when its description changes) plus the focus. On the spike's panel
  that is one node per click.
- AccessKit's **actions** (click, focus, set value, scroll) are delivered to the target widget as
  ordinary events, so a screen reader and a mouse go through the same code.
- The `accesskit_winit` adapter is attached at the preparation point of ADR-0040 W7 (window
  created invisible, adapter attached, then shown). It enters the workspace with the first widget
  (F2.09), with `accesskit` 0.25 and `accesskit_winit` 0.34 as ADR-0036 B1 fixed them; ADR-0038
  S7's zbus 5 exceptions stay until AccessKit moves to zbus 6.

### WA6 — One segment per widget

A widget's display-list segment is its `SegmentId`, equal to its `WidgetId`: re-recording one
widget damages only its old and new area (ADR-0039 D1). A container that draws something (a
panel's background, a scroll area's frame) records its own segment, drawn below its children's;
one that only arranges draws nothing. Recording order is tree order, so children are on top of
their parent.

### WA7 — Talking to the application: typed messages, commands, change sets

Widgets do not call application code. A widget that wants something (a button pressed, a value
dragged) **emits a typed message** of the application's own message type; after the event the
application receives the messages in order and acts: changing other widgets, or, for anything
that changes the document, executing a **command** on the model (ADR-0003, ADR-0026). The model's
**change set** of each transaction (ADR-0026 §5) tells the views which widgets to update; views
never compare the model with a copy (CLAUDE.md §1: no duplicated data between views).

Rejected: **callbacks stored in widgets** (closures capturing application state): they need
shared mutable state (`Rc<RefCell<…>>`) across the tree, which makes ownership and testing hard,
and they hide the order in which things happen.

### WA8 — The clipboard is decided in F2.10

The clipboard crate (`arboard` 3.7.0 is the candidate, MIT OR Apache-2.0) is chosen with the text
field in F2.10, where its native dependencies and its behaviour on Wayland can be measured. D12's
clipboard part moves there.

### Theme, text and scale

- A **colour** change of the theme redraws everything without re-recording (ADR-0039 D3); a
  change of the **text tokens** lays every text out again and re-lays out (ADR-0044 TX9); a
  **density** change (F2.09) re-lays out.
- Widgets work in logical pixels; the window's scale is applied by the renderer (ADR-0039 D2).

### Tests and benchmarks (for F2.09)

- **Unit tests** of layout (each container and size rule, constraints, incremental invalidation
  stopping where sizes do not change), hit testing (topmost, overlapping, capture), focus order
  (Tab, Shift+Tab, modal trap), bubbling, and AccessKit updates (whole tree first, then only
  changed nodes).
- **Headless interaction tests**: a panel driven by synthetic input events, checked through its
  display list and pixels, as the screenshots do (ADR-0041).
- **No allocation** while routing an event and re-recording a widget of the same size.
- **Benchmarks** (instruction counts) on a 200-widget panel like the spike's: first frame, click,
  hover, a text change that re-lays out one row; targets from the spike: an event under 0.5 M
  instructions drawing included, far below the 50 ms from input to frame (CLAUDE.md §5).

## Alternatives rejected

See WA1 (immediate; declarative with reconciliation), WA2 (`taffy`), WA7 (callbacks).

## Consequences

- SCALETTA F2.09 builds the widget tree, the layout containers, routing and focus, the AccessKit
  adapter, and the first widgets (label, button, checkbox, switch, slider, scroll area, splitter)
  on this ADR. AccessKit (`accesskit`, `accesskit_winit`) enters the workspace there.
- The demonstration window of ADR-0040 W8 is replaced by real widgets as soon as they exist.
- A declarative layer remains possible later, on top of the retained tree.
- Decision D12 closes on acceptance; the clipboard part moves to F2.10.
