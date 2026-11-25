# ADR-0040 — The window and the event loop: on-demand frames, our own input events

- **Status:** Accepted (owner, 2026-11-10)
- **Date:** 2026-11-10
- **Implements:** ADR-0012 (our UI on low-level blocks), ADR-0036 (`winit` 0.30.13, `softbuffer`
  0.4.8, their features; B6 decorations, B7 duplicates, B9 the order of window creation), ADR-0039
  (presenting the drawing), CLAUDE.md §5 (efficiency) and §6 (primary modifier). Step F2.04a of
  SCALETTA; F2.04b adds the system theme on Windows and macOS (ADR-0038 S3) on this structure.

## Context

F2.03 gave us a display list and a CPU renderer that draws into an in-memory image. F2.04a puts
that image in a real window and brings input in. The structure chosen here (who owns the loop,
when a frame is drawn, how other threads wake the interface, what an input event looks like) is
hard to change once widgets and views are built on it.

Facts checked on 2026-11-10: `winit` 0.30.13 and `softbuffer` 0.4.8 are still the versions of
ADR-0036 (`winit` 0.31 is still a beta). In `winit` 0.30 the application implements
`ApplicationHandler<T>`; `T` is a user event sent from any thread through an `EventLoopProxy<T>`.
`softbuffer`'s `Buffer::present_with_damage` presents only some rectangles, and `Buffer::age()`
says how many frames old the buffer's contents are (0 = unknown), because the system may rotate
two or three buffers.

The owner chose on 2026-11-10: split F2.04 into F2.04a and F2.04b; the structure below; the
window order of ADR-0036 B9 tested now with an empty hook, AccessKit itself still entering with
the first widget (ADR-0036 B1); a demonstration window to try by hand; manual tests only on the
owner's Linux desktop until decision D18.

## Decision

### W1 — `winit` lives only in `orchestra-ui`

The module `window` of `orchestra-ui` owns everything `winit` and `softbuffer`: the event loop,
the window, the surface, the translation of events. Outside it only **our own types** circulate
(W5), so the rest of the interface, and the widgets of F2.08, never see `winit`. Updating to
`winit` 0.31 later (ADR-0036 B7) touches this module only. The executable `orchestra-app` just
calls `orchestra_ui::run(...)` with what to show.

### W2 — Frames on demand

The loop **sleeps until an event arrives** (`ControlFlow::Wait`): no frame is drawn when nothing
changed, so an idle Orchestra uses no CPU and does not drain a laptop's battery. A frame is
requested only when the display list has damage (ADR-0039 D5), or the window's size, scale or
theme changed (all damage). Continuous frames (the playhead during playback, F3) will be asked
for explicitly while they are needed, with the screen's refresh rate as the limit.

### W3 — Waking the interface from other threads

Other threads talk to the interface with **our own event type** sent through `winit`'s
`EventLoopProxy`: today the system appearance (ADR-0038: the portal's thread on Linux), later the
audio engine's meters and transport. The proxy is the only cross-thread entry; the interface's
state is only ever touched on the event loop's thread, so it needs no locks.

### W4 — Presenting: our frame, then the damaged rectangles

The interface keeps **its own full frame**: the `Image` of ADR-0039, in device pixels. A frame is
drawn by rendering only the damage into it (ADR-0039 D5), then copying into `softbuffer`'s buffer,
converting premultiplied RGBA to `0RGB`:

- when the buffer's age is **1** (it holds exactly the previous frame), only the damaged
  rectangles are converted and copied, and presented with `present_with_damage`;
- otherwise (age 0 = unknown, or older than one frame), the **whole** frame is copied and
  presented.

Keeping our own frame makes partial drawing correct whatever the system does with its buffers,
and costs one image of memory (8 MB at 1920 × 1080). The surface's colour is opaque by design
(every theme's `surface` is opaque), so dropping alpha in the conversion is exact.

### W5 — Our own input events

The window translates `winit`'s events into ours, in **logical pixels**:

- **pointer**: moved, pressed, released (left, right, middle, back, forward), left the window;
- **wheel**: by lines or by pixels (touchpads), as the system says;
- **keyboard**: a key pressed or released, as the **logical key** (a character or a named key:
  arrows, Enter, Escape, Tab, function keys…), with whether it is a repeat;
- **modifiers** with an abstract **primary** modifier (Ctrl on Linux and Windows, Cmd on macOS,
  CLAUDE.md §6) and a secondary one (the other of the two), plus Shift and Alt (Option);
- **focus** gained or lost.

Text input and IME (composed characters, Asian input methods) come with the text field in F2.10,
because they need a text cursor to place the composition window. Shortcuts are written with
`primary`, never with Ctrl or Cmd, so one table works everywhere.

### W6 — Scale and size

The window reports its scale factor (DPI); the interface records the display list in logical
pixels and draws at that scale (ADR-0039 D2). A change of scale (moving the window to another
screen) or of size reallocates the frame, resizes the surface and damages everything. A window
minimised to zero size draws nothing until it has a size again.

### W7 — The order of window creation (ADR-0036 B9)

The window is always created **invisible**, then a **setup hook** runs, then the window is shown.
AccessKit's adapter, which panics when it is created after the window was first shown, will attach
in that hook when it enters with the first widget (F2.08–F2.09, ADR-0036 B1). Until then the hook
is empty. The order is checked by a test now (W9).

### W8 — The demonstration window

`cargo run -p orchestra-app` opens a window that draws with ADR-0039's commands only (no text
until F2.07): the theme's surface, a few controls in their colours, a shape that follows the
pointer (one segment re-recorded per move, so only its area is redrawn) and the state of the
modifiers. It reads the system's light/dark preference once at start (ADR-0038; changes are
followed from F2.04b) and redraws on resize and on scale changes. It exists to check presentation, damage, input and DPI by eye; it is
replaced by the real interface as the widgets arrive.

### W9 — Tests

CI runners and the development sandbox have **no display**, so no test opens a window. The logic
is kept out of the window code and tested on the three platforms: the translation of keys,
buttons and modifiers (the primary modifier on each platform), the conversion of pixels from
premultiplied RGBA to `0RGB`, the choice between partial and full copies by buffer age, the
damage in device pixels for `present_with_damage`, the frame's reallocation on resize and scale,
and the order of W7 (with a fake window). The window itself is checked by hand by the owner on
Linux (X11 and Wayland). Windows and macOS are compiled and tested by CI but not looked at until
decision D18 (machines for manual tests) is taken; accepted by the owner until milestone M2A.

## Precisions of the implementation (F2.12a5b, approved by the owner on 2026-11-25)

- **The app decides about closing.** `WindowApp::close_requested(&mut self, list) -> bool`
  (default `true`) is asked when the user asks to close the window: `WindowEvent::CloseRequested`
  (the system's button, Alt+F4, the window menu) and the new `WindowRequest::RequestClose`. `true`
  closes now; `false` keeps the window, and the app, having shown what it needed (the question
  about unsaved changes, ADR-0050 DG5), closes it later with `WindowRequest::Close`, which closes
  without asking again. Nothing blocks the loop while the question is open. Still not asked while
  a file chooser is open (ADR-0050, choice 4 of F2.12a5a).
- **The title follows the app.** `WindowApp::title(&self) -> Option<&str>` (default `None`: the
  options' title stays) is asked after every batch of events; the window keeps a copy and calls
  `winit`'s `set_title` only when the text changed, and opens with the app's title if it already
  has one. `WindowRequest` stays `Copy`: the title is read, not sent.

## Alternatives rejected

- **`winit` types used across the interface:** every widget would depend on `winit`'s API, and
  `winit` 0.31 changes it a lot (ADR-0036 B7).
- **A continuous frame loop** (`ControlFlow::Poll`, redraw every vsync): simple, but an idle
  editor would keep a core busy and drain batteries.
- **Copying only the damage into `softbuffer` without our own frame:** wrong whenever the system
  rotates buffers (age > 1), and age 0 happens after every resize.
- **AccessKit in F2.04:** about 69 crates and 3.2 MB before any widget exists to describe
  (ADR-0036 B1); the order it needs is prepared and tested instead.

## Consequences

- `winit` 0.30.13 and `softbuffer` 0.4.8 enter the workspace in `orchestra-ui`, with ADR-0036's
  features. Their duplicate versions go into `deny.toml`'s `skip`, each with its exit (ADR-0036
  B7). Their native libraries are already in `docs/native-deps.md` (ADR-0036 B8).
- `orchestra-app` gains its first real code: start the interface.
- F2.04b implements the system theme on Windows and macOS on top of W3 (ADR-0038 S3), and bridges
  the Linux portal's callback to the proxy.

## Precision of the implementation (F2.09e, approved by the owner on 2026-11-13)

**The pointer's shape.** `WindowApp` gains `pointer_shape()` (the arrow by default; an app built
on `Ui` answers `Ui::pointer_shape`, ADR-0045 WG1–WG4). The window asks after every input event,
layout and assistive-technology action, and calls `Window::set_cursor` only when the answer
differs from the shape it last set (a new window starts with the arrow, `winit`'s default). Our
`PointerShape` is translated to `winit`'s `CursorIcon` in one function of `window/pointer.rs`, as
W1 and W5 require; `ColResize` and `RowResize` are used for the divider's shapes, which `winit`
maps to the native resize shapes on every platform. Nothing is done when the pointer leaves the
window: the shape belongs to the window, and the system restores it on re-entry.

## Precision of the implementation (F2.10b2, approved by the owner on 2026-11-14)

**Timed wake-ups (W2).** A text field's caret blinks (ADR-0047 TF8), so the window must wake
without an event. W2 stays the rule (the loop waits), with one addition: `WindowApp` gains
`next_wakeup() -> Option<Instant>` (`None` by default) and `advance(now, list)` (nothing by
default). The window calls `advance` with the real clock at the start of every batch of events
(`ApplicationHandler::new_events`, except at the loop's start), so the events of a batch share
one time and a wake-up that came is handled before them; after the batch (`about_to_wait`) it
sets `ControlFlow::WaitUntil(t)` when the app has a wake-up and `ControlFlow::Wait` otherwise.
Widgets never read the clock: `Ui::advance` gives them the time (`EventCx::now`), so blinking and
its 10 s stop are tested with made-up times. Without a wake-up (no focused field, or a caret that
stopped blinking) the loop waits for input exactly as before.

## Precision of the implementation (F2.10c1, approved by the owner on 2026-11-15)

**The clipboard at the setup point (W7).** ADR-0047 TF12 says the window owns one clipboard for
the whole session, but widgets live in the application's `Ui`, below the window. The window
creates the system's clipboard once, when the window is created (the display server is then known
to be there), and hands it to the application before the first layout:
`WindowApp::attach_clipboard(Box<dyn ClipboardSource>)`, which drops it by default; an application
built on `Ui` passes it on with `Ui::set_clipboard`. It is still one object for the session. When
the system has no clipboard the method is not called and the interface keeps its own in memory.

## Precision of the implementation (F2.10c2, approved by the owner on 2026-11-17, after the hand tests)

**The input method (W5, ADR-0047 TF5–TF6).** `winit`'s `WindowEvent::Ime` becomes our
`InputEvent::Ime(ImeEvent)` (`Enabled`, `Preedit { text, cursor }`, `Commit`, `Disabled`; the
preedit's cursor is an ordered byte range checked against the text, `None` when invalid). The
window asks the application `WindowApp::text_input() -> Option<TextInput>` after every input
event, layout and assistive-technology action, as it asks the pointer's shape, and calls the
system only when the answer changes: `set_ime_allowed(true)` when it becomes `Some`,
`set_ime_allowed(false)` when it becomes `None`, `set_ime_cursor_area` (logical pixels) when the
caret's area moves, and `set_ime_allowed(false)` then `true` when `TextInput::reset` changed
(`winit` 0.30 has no call that ends a composition). The default answer is `None`: the input
method stays off.

## Precision of the implementation (F2.11d1, approved by the owner on 2026-11-18)

**The physical key (W5, ADR-0048 AM5).** `InputEvent::Key` gains `physical: PhysicalKey`, from
`winit`'s `KeyEvent::physical_key`: `Letter(0..=25)` and `Digit(0..=9)` where the US layout has
them, `Numpad(0..=9)`, and `Other` for every other key and for one the system could not place.
Only the positions the shortcuts' fallbacks need are named (no keys "for later"). The logical
key stays the one widgets read; the physical one is for the action registry. `NamedKey` gains
`ContextMenu` (the Menu key), which AM8 keeps from actions and AM12 will use.

**Requests to the window.** `WindowApp::take_window_request() -> Option<WindowRequest>` (default
`None`) lets the application ask its window to `Close` (end the loop) or `ToggleFullScreen`
(borderless on the current monitor, `Fullscreen::Borderless(None)`: the system's video mode never
changes). The window asks after every window event, until the answer is `None`, so requests are
carried out in the order the actions ran (Quit and Full screen, ADR-0048 AM6).

## Precisions of the implementation (F2.12a4a, approved by the owner on 2026-11-24)

- **The theme changes at run time.** `WindowRequest::SetTheme { pair, mode }` asks the window to
  draw with another pair or mode (the Preferences, ADR-0050 DG9): a full redraw when what it draws
  changed, `winit`'s `set_theme` with the forced half (or none for *System*). Details in ADR-0050's
  precisions of F2.12a4a.
- **`run` borrows the app.** `run(&mut app, options)`: the caller keeps the app and what it owns
  (the settings file, closed after the loop). A `&mut A` is itself a `WindowApp`, forwarding every
  method.
