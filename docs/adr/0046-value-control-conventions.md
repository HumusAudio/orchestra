# ADR-0046 — Value-control conventions: wheel, double click to the default, fine adjustment

- **Status:** Proposed (owner's outline WE1–WE7 approved on 2026-11-12; this text awaits
  acceptance).
- **Date:** 2026-11-12
- **Implements:** ADR-0045 (the widget tree: routed events, bubbling, capture), ADR-0040 W5 (our
  input events), ADR-0007 (budgets).
- **Requested by:** the owner, after trying the Playback preview of F2.09c: "for every slider
  and knob, in the whole program, always: with the cursor over it the value also changes with the
  mouse wheel, a double click goes back to the default value, and with Ctrl held the adjustment
  is finer".

## Context

F2.09c gave the program its first value control, the slider. It follows the pointer, the
keyboard and the accessibility actions, but none of the habits that musicians bring from every
DAW and plugin: turning a control with the wheel while hovering it, double-clicking it to reset
it, holding a modifier to adjust it finely. The program will have hundreds of such controls
(faders, pan knobs, plugin parameters, velocity, tempo), so the behaviour must be one convention,
written once, that every value control follows, not something each widget improvises.

Two things the toolkit lacked were needed for it: the **click count** (`winit` reports presses
one by one) and the **modifiers during pointer events** (our pointer events carried none).

## Decision

**V1 — One convention for every value control, always.** Every widget that sets a number in a
range (today the slider; later knobs, faders, number fields with drag, plugin parameters) follows
V2–V6. A widget that cannot (an exception) says why in its documentation. Knobs use the same
rules with vertical drag (up increases).

**V2 — The wheel changes the value under the pointer, without the focus.** Each wheel notch moves
the **coarse step**: 2% of the range, rounded to whole steps, at least one step (fifty notches go
from one end to the other whatever the range). Away from the user increases. A touchpad or a
smooth wheel reports pixels: 40 logical pixels make a notch, and the remainder is kept for the
next event (a slow swipe still moves); a movement in the other direction drops the remainder so a
reversal acts at once. The control handles the wheel even at the ends of its range, so the area
around never scrolls under a control being turned.

**V3 — The primary modifier is the fine adjustment.** With the primary modifier held (Ctrl on
Linux and Windows, ⌘ on macOS: the platform's convention of ADR-0040 W5, which is what the owner's
"Ctrl" means on macOS):
- a wheel notch and an arrow key move **one step** instead of the coarse step;
- a drag moves **a tenth as fast**, and a press does not jump the value to the pointer (so a fine
  correction starts from the current value). Pressing or releasing the modifier during a drag
  re-anchors the drag at the pointer's last position and the value reached, so the value never
  jumps and the new speed applies from there.

The modifier is fixed for now. When shortcuts become customisable (see "Consequences"), it may
become a preference; the convention itself does not change.

**V4 — A double click restores the default.** Every value control has a default value: the one
the application gives (`default_value`), otherwise its initial value. The second press of a
double click sets it (and tells the application, like any change) and starts no drag, so a small
movement before the release does not undo the reset. A double click is two presses of the same
button within **400 ms** and **4 logical pixels** (the usual desktop defaults; reading the
system's own setting can come later without changing this ADR). The window counts the clicks
(`ClickCounter`) and every pointer-button event carries its count (`clicks`: 1, 2, …; a release
carries its press's count), so every widget sees the same notion of a double click.

**V5 — Shift+wheel belongs to scrolling.** With Shift held a value control ignores the wheel,
which bubbles (ADR-0045 WB7) to the scroll area around it (F2.09d): a long panel of controls can
always be scrolled even when the pointer rests on one.

**V6 — Keyboard, when focused.** The arrows move the coarse step (one step with the primary
modifier; right and up increase), Page Up and Page Down ten steps, Home and End to the ends.
Assistive technologies' increase and decrease move one step (what they announce as the step).

**V7 — Modifiers reach every widget.** The interface keeps the modifiers held (from the window's
modifier and key events) and gives them to every event's context (`EventCx::modifiers`), pointer
events included. When the window loses the focus it forgets them: the releases of keys held
during an Alt+Tab go to another window, and a modifier stuck "on" would leave the fine adjustment
active.

## Measurements

The work per event stays inside ADR-0045's budget (≤ 0.5 M instructions per event, drawing
included): the wheel and the fine drag reuse the slider's change path, measured by the benchmark
`slider_wheel_notch` alongside `slider_drag_move`: one notch over a slider in a panel of 40 rows
costs **0.36 M** instructions with drawing and the accessibility update (the slider being focused,
its focus ring is drawn again too); a drag move stays at 0.22 M.

## Alternatives considered

- **Shift as the fine modifier** (Bitwig, Ableton Live for some controls). Discarded: the owner
  asked for Ctrl, and Shift+wheel is the near-universal horizontal-scroll gesture we reserve for
  scroll areas (V5).
- **Alt+click to reset** (some plugins) instead of a double click. Discarded: the owner asked for
  the double click, and Alt+click is taken by the window manager on many Linux desktops.
- **A wheel step of exactly one step.** Discarded: on a range of 0–1000 by 1 it would take a
  thousand notches; the coarse step makes every control equally quick, and the fine modifier gives
  the single step.
- **Absolute fine drag** (the value under the pointer, with a scaled track). Discarded: it jumps
  when the modifier changes; the relative drag with re-anchoring never jumps.
- **Counting clicks in each widget.** Discarded: every widget would reimplement the timing and
  could disagree; counting once in the window is one rule for all.

## Consequences

- Every new value control is reviewed against V1–V6; its tests cover the wheel, the fine
  adjustment and the double click.
- `InputEvent::PointerButton` and `WidgetEvent::PointerButton` carry `clicks`; every constructor
  and pattern was updated in the same commit (CLAUDE.md §9).
- The owner also asked, together with this ADR, that **every keyboard shortcut be customisable
  by the user**. That is a separate decision (a command registry with per-platform default
  bindings, user overrides in the settings file, conflict detection), planned with the menus and
  the settings in SCALETTA; this ADR only notes that the fine modifier of V3 may join it.
