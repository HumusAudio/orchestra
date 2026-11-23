# ADR-0049 — Our own title bar, only where the system draws none

- **Status:** Accepted (owner, 2026-11-23; outline TB1–TB13 presented first, with the owner's
  choices on TB1, TB4, TB5, TB11 and on a Preferences option, then this text). Precisions of
  F2.11b2 and F2.11b3 below.
- **Date:** 2026-11-23
- **Implements:** ADR-0036 B6 and B7 (Wayland decorations: `winit`'s own for now, our title bar
  later; the second `tiny-skia`), ADR-0040 (the window and the event loop; W3 the proxy, W5 our
  own input events), ADR-0045 (retained widgets; WG4 the pointer shapes), ADR-0048 (the menu bar,
  AM9, AM12), ADR-0038 (the system's preferences read through the Settings portal), ADR-0037
  (tokens), ADR-0008 (six languages), ADR-0006 (platform conventions), ADR-0007 (budgets).
- **Step:** F2.11b1 of SCALETTA (this ADR); F2.11b2 the probe, the bar, its buttons, dragging,
  the menus in the bar and the texts; F2.11b3 the edges and resizing, the buttons' layout read
  from the portal, the benchmark and the guides.

## Context

A window on a desktop has a frame: a title bar to read and drag, buttons to close, maximise and
minimise it, edges to resize it. Who draws it depends on the system:
- **X11**: the window manager, always. An application cannot help it, and does not need to.
- **Windows**: the system (DWM). A program may draw its own title bar, but then Windows 11's
  **Snap Layouts** (the menu shown while the pointer rests on Maximise) only appear if the window
  answers `WM_NCHITTEST` with `HTMAXBUTTON` over its own maximise button
  ([Microsoft Learn](https://learn.microsoft.com/windows/apps/desktop/modernize/apply-snap-layout-menu));
  `winit` does not do it, so it would be our own Win32 code.
- **macOS**: the system, with the "traffic light" buttons at the leading edge, which Apple's
  guidelines ask to keep visible and working even under a transparent title bar
  ([HIG, Windows](https://developer.apple.com/design/human-interface-guidelines/windows)).
- **Wayland**: the compositor *may* draw the frame (server-side decorations, negotiated with the
  optional `xdg-decoration` protocol); KDE's KWin, sway, Hyprland and most others do. **GNOME's
  mutter does not** and has declined to
  ([mutter #2342](https://gitlab.gnome.org/GNOME/mutter/-/issues/2342),
  [GNOME Discourse](https://discourse.gnome.org/t/proper-way-to-render-window-decorations-on-wayland-for-non-gtk-apps/6734)):
  there every application draws its own (client-side decorations).

Today (ADR-0036 B6) `winit` draws a simple Adwaita-like frame without a title through
`sctk-adwaita` (feature `wayland-csd-adwaita-notitle`) where the compositor draws none. It costs
+0.3 MB and a second `tiny-skia` (0.11, next to our 0.12; ADR-0036 B7), and it looks like
neither GNOME's applications nor ours. The plan approved with ADR-0036 is to replace it with our
own title bar, drawn with our widgets.

What the code offers, verified in the sources on 2026-11-23:
- `winit` 0.30.13 asks the compositor for server-side decorations and creates its own frame only
  when the compositor answers "client". It **does not tell the application which one it got**:
  `Window::is_decorated()` answers `true` in both cases. It offers what a client-side bar needs:
  `set_decorations(false)`, `drag_window()`, `drag_resize_window(direction)`, `set_maximized`,
  `set_minimized`, `is_maximized`, and `show_window_menu(position)`, which its documentation calls
  unsupported on Wayland but which its Wayland backend does implement (`xdg_toplevel.show_window_menu`).
- `softbuffer` 0.4.8, the latest release, presents Wayland buffers as `Xrgb8888`: **no
  transparency**. A client-side frame can therefore have no shadow, no rounded corners and no
  invisible resize margin outside the visible window. Transparency (`AlphaMode`, premultiplied
  on Wayland) was merged in March 2026 for `softbuffer` 0.5, not yet released.
- `wayland-client` 0.31 and `wayland-protocols` 0.32 are already in our dependency tree (through
  `winit` and `smithay-client-toolkit`), with `xdg-decoration`.
- GNOME's choice of title bar buttons is the setting `org.gnome.desktop.wm.preferences
  button-layout` (for example `appmenu:close`, GNOME's default, or
  `:minimize,maximize,close`), readable through the Settings portal
  (`org.freedesktop.portal.Settings.Read`), as `sctk-adwaita` already does
  ([xdp-gnome #95](https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/issues/95)). We
  already talk to that portal with `zbus` 5 for the light/dark preference (ADR-0038).
- AccessKit 0.25 has the role `TitleBar`.

How others decide:
- **GTK 4 / libadwaita** draw their own header bar on every Wayland compositor, KDE included.
- **Qt** uses the compositor's decorations where offered, its own plugin elsewhere.
- **Zed** (GPUI) asks for server-side decorations and draws its own when the compositor refuses
  ([commit #39313](https://git.secluded.site/zed/commit/a04b3d80c82ba4aa6321a4c3986e9dbcf78c87fa)).
- **Rerun** (egui) probes the compositor at start-up: an invisible toplevel asks for server-side
  decorations and reads the mode the compositor commits to, because merely seeing the
  `zxdg_decoration_manager_v1` global "is not enough"
  ([`re_ui/src/wayland.rs`](https://docs.rs/crate/re_ui/latest/source/src/wayland.rs)).
- **VS Code** and **MuseScore 4** on Windows merge the menus into the title bar; Ardour, REAPER
  and Dorico keep the system's frame.

## Decision

### TB1 — Where our title bar is used (owner's choice)

Only **where the system draws none**: on Wayland when the compositor does not give server-side
decorations (GNOME, and any compositor without `xdg-decoration`). Everywhere else the system's
frame stays: X11, Wayland compositors with server-side decorations (KDE, sway…), Windows,
macOS. Orchestra's identity is in the window's content (ADR-0014, ADR-0012); its frame follows
each system's conventions (CLAUDE.md §6), keeps Windows 11's Snap Layouts and macOS's traffic
lights without platform code of ours, and leaves KDE users their Breeze frame.

There is **no Preferences option** to force our bar elsewhere for now (owner's choice); the
environment variable of TB2 is for testing only.

### TB2 — Finding out whether the system draws a frame

At start-up on Wayland, before the window exists, a **probe** asks the compositor, the way
Rerun does: it connects with `wayland-client`, binds `xdg_wm_base` and
`zxdg_decoration_manager_v1`, creates a toplevel that never gets a buffer (so it never appears),
asks for `server_side` and reads the mode the compositor answers. The answer decides:
- **server-side** → the window is created with the system's decorations, as today;
- **client-side**, **no decoration manager**, **no answer**, or **any failure of the probe** →
  the window is created without decorations (`with_decorations(false)`) and draws our bar.
  Without a frame a window cannot be moved or closed with the mouse; a frame too many is the
  lesser fault.

The probe is a function behind a trait in the window layer's `platform/` module (CLAUDE.md §2),
compiled on Linux only; on X11, Windows and macOS the answer is "the system draws it" without
asking. `ORCHESTRA_DECORATIONS=system` or `=own` overrides the answer, for testing and as a
way out if a compositor lies. The probe costs two round trips to the compositor, expected to be
a few milliseconds and **measured in F2.11b2** against the start-up budget (ADR-0007). It uses
the crates already in the tree, with the same run-time loading of `libwayland-client`
(`wayland-backend`'s `dlopen`) that `winit` uses, so no system library becomes a link
dependency.

### TB3 — `sctk-adwaita` leaves

The feature `wayland-csd-adwaita-notitle` is dropped: the binary loses `sctk-adwaita` and the
second `tiny-skia` (ADR-0036 B7 closes). If the probe said "server-side" but the compositor
later switches to client-side (unlikely), `winit` falls back on its own plain frame from
`smithay-client-toolkit`: still a frame, so still usable.

### TB4 — What the bar holds (owner's choice on the buttons)

From the leading edge: the **menus** (TB5), then the **title** (the program's name, later the
project's), then the **window buttons**. Which buttons appear and on which side follows GNOME's
`button-layout` (TB2's portal), so the bar matches every other GNOME window: by default only
**Close**, on the right. Without a readable setting (another desktop, no portal) the same
default as GNOME. The setting's `appmenu` and `icon` items are ignored: the program has no
application menu or icon in its bar.

### TB5 — The menus inside the title bar (owner's choice)

**One row**: where our bar is drawn, the drawn menu bar of ADR-0048 sits in the title bar
itself, at the leading edge; the title is centred in the free space, which is the area that
drags the window. This saves a row of about 30 logical pixels that the score needs more than the
frame does (VS Code and MuseScore 4 do the same on Windows). Where the system draws the frame,
the menu bar stays its own row under it, as today. The bar's height is the menu bar's, a density
token (ADR-0037), so the menus need no second layout.

### TB6 — Behaviour

- A press of the primary button on the bar's free space starts `drag_window()`; a double click
  toggles maximised (GNOME's default action); a secondary click opens the system's window menu
  (`show_window_menu` at the pointer). What the compositor does with the keyboard (Super+↑,
  Alt+F4, Super+drag) is untouched.
- **Close** goes the way of the system's close button, `CloseRequested`, so that the question to
  save changes (later) applies to both; **Maximise** becomes **Restore** while maximised;
  **Minimise** calls `set_minimized(true)`.
- In **full screen** the bar is not drawn: the menus are reached with F10 and Alt (ADR-0048
  AM12).
- The bar's buttons are ordinary buttons of ours (ADR-0045): press, hover and the pointer shape
  as every button.

### TB7 — Edges and resizing, without transparency

Until `softbuffer` supports transparency there is no shadow and no resize margin outside the
window. So:
- a **resize band** of 6 logical pixels **inside** the window's edges and **corners** of 16
  logical pixels, each with its resize pointer shape (the horizontal, vertical and two diagonal
  shapes added to `PointerShape`, ADR-0045 WG4, note of F2.09e) and starting
  `drag_resize_window` with its direction; the band is a strip of the window the content does not
  receive, drawn as the window's background;
- a **border** of one physical pixel in a theme token, so the window stands apart from a window
  or desktop of the same colour;
- square corners;
- no band and no border while maximised or in full screen.

When `softbuffer` 0.5 is released and adopted (a dependency change of its own), a later step
adds the shadow and moves the band outside the visible window, as GNOME's own windows have it.

### TB8 — Themes and states

Only tokens (ADR-0037): the bar's background is the menu bar's; the title and the buttons'
symbols dim while the window does not have the focus (`WindowEvent::Focused`), as every system's
frame does; the symbols are drawn as shapes (no icon font), like the other controls.

### TB9 — Accessibility

The bar is an AccessKit node with the role `TitleBar` named with the window's title; its
buttons are `Button` nodes with their names (TB10) and the `Click` action. They are **outside
the Tab chain**: window management is the compositor's from the keyboard, and Tab must reach the
content without crossing three window buttons first. Assistive technologies still reach and
press them through the tree.

### TB10 — Texts

Four messages in the six catalogues: *Close*, *Maximise*, *Restore*, *Minimise*, with the
terminology research of `docs/i18n-process.md` in GTK's and Qt's catalogues (the names those
toolkits give their own title bar buttons). They are distinct glossary entries from macOS's
*Minimize* and *Zoom* (F2.11e2): different systems, different words.

### TB11 — Screenshots (owner's choice)

The guides' screenshots stay **without** a title bar (`cargo xtask screenshots` renders the
window's content), so they are the same whatever the reader's system; **one** screenshot shows
our bar, in the paragraph that explains it.

### TB12 — Tests and budgets

- The bar's **hit test** (which button, free space to drag, which edge or corner) with unit tests
  on every platform, in both directions of the layout (`button-layout` with the buttons on the
  left and on the right).
- The **decision** of TB2 with a probe answer injected (server, client, no manager, failure,
  each override); the probe itself needs a compositor and is checked by hand by the owner on
  GNOME (a test session or a virtual machine is enough), KDE Wayland and X11.
- Parsing of `button-layout` (GNOME's default, both sides, unknown names, empty).
- A **benchmark** of the redraw when the window gains or loses the focus (the bar only, never the
  whole window: the damage of ADR-0039), and the bar adds no allocation to the frame (ADR-0016).

### TB13 — X11, Windows and macOS

Unchanged. On macOS a transparent title bar with the native traffic lights over our content
(`with_titlebar_transparent`, `with_fullsize_content_view`) remains an idea for later, with its
own decision; it is not part of this ADR.

## Alternatives rejected

- **Our bar on every Wayland compositor** (GTK 4's and Zed's default): one look on Wayland, but
  it replaces KDE's frame, which KDE users have chosen and configured, for no gain of function.
- **Our bar everywhere**: the same look on every system, at the price of our own Win32 hit
  testing for Snap Layouts, and of hiding or reproducing macOS's traffic lights; against
  CLAUDE.md §6 (respect the system's conventions).
- **Keeping `sctk-adwaita`**: a frame without a title that matches no one, and a second
  `tiny-skia` in the binary.
- **`libdecor`** (the C library GNOME suggests for toolkit-less clients): a C dependency
  (CLAUDE.md §6) and a plugin system loaded at run time, to draw a GTK-looking frame around a
  program that draws everything else itself.
- **Deciding by `XDG_CURRENT_DESKTOP`**: a guess about the desktop instead of an answer from the
  compositor; wrong under nested sessions and for the many compositors it does not name.
- **Waiting for `softbuffer` 0.5**: GNOME users would keep the current frame for an unknown time;
  TB7 works without transparency and improves when it comes.
- **Two rows (title bar, then menu bar)**: about 30 pixels of vertical space taken from the score
  on every GNOME window.

## Consequences

- GNOME users get a title bar that looks like Orchestra's and follows their button layout, with
  the menus in it; everybody else keeps the system's frame.
- The binary loses `sctk-adwaita` and the second `tiny-skia` (ADR-0036 B7); it gains a small
  probe on crates already in the tree.
- Without transparency the bar has square corners, no shadow, and a resize band inside the
  window; a later step revisits TB7 when `softbuffer` 0.5 exists.
- The window layer learns to draw without decorations, which a future macOS transparent title
  bar or a Preferences option (both deferred) would reuse.
- Four new messages and glossary entries; one more screenshot in the guides.
- The probe and the bar are checked by hand on GNOME, KDE Wayland and X11 by the owner (CI has
  no compositor).

## Precisions of the implementation (F2.11b2, approved by the owner on 2026-11-23)

Before writing, the owner approved: F2.11b2 and F2.11b3 merged in one pull request, so `main`
never loses the mouse resizing that `sctk-adwaita` gave on GNOME (choice a); a `TitleBar` widget
holding the existing menu bar; the window's state as an input event and the title bar's wishes
as window requests; the probe run before the window exists. While writing, the rest:
- **TB2: who asks, and when.** The application asks `orchestra_ui::decorations()` once, before
  building its interface, and passes the answer both to the window (`WindowOptions::decorations`;
  with `Own` the window is created with `with_decorations(false)`) and to the interface it builds
  (`Demo::with_decorations`), so the two always agree. The probe opens its own short connection
  with `wayland-client` 0.31.15 and `wayland-protocols` 0.32.13 (`client`, `unstable`), the
  versions already in the lock file: nothing new is compiled. **Measured** by the owner on KDE
  Plasma (Wayland) with `cargo run -p orchestra-ui --example decorations_probe` (a debug build):
  answer "server-side", 1.5–3.1 ms per question over ten runs, within the start-up budget of
  ADR-0007 (3 s) by three orders of magnitude.
- **TB5: the menus in the bar.** The menu bar of ADR-0048 gains a **compact** form, as wide as its
  titles and drawing nothing of its own, which the `TitleBar` places after the leading buttons.
  The title is centred on the whole window when it fits in the free space there, otherwise in the
  free space; when even the free space is narrower than the title it is not drawn (no ellipsis:
  the window's name stays the bar's accessible name).
- **TB6: dragging.** A press on the free space starts the drag only after the pointer moved more
  than 4 logical pixels, not at the press: a drag taken by the compositor at the press would
  swallow the second click of a double click (GTK does the same).
- **TB6: full screen.** The title and the buttons disappear and the free space does nothing, but
  **the menus stay**, as the menu bar under a system frame stays in full screen (TB6 said the bar
  was not drawn; drawing nothing would leave the menus reachable only from the keyboard).
- **TB6, TB8: the window's side.** A widget asks the window through its event context
  (`EventCx::request_window`); the interface queues the requests and the application hands them on
  first (`Ui::take_window_request`), then its own Quit and Full screen. New `WindowRequest`s:
  `Drag`, `ToggleMaximized`, `Minimize`, `ShowWindowMenu { at }` (it loses `Eq`: the point is made
  of `f32`s). The window tells the application `InputEvent::WindowState { maximized, fullscreen }`
  when it opens and after every resize that changes it (`winit` has no event of its own for
  either).
- **TB9: tooltips.** The buttons' tooltips are their names, as GNOME's buttons have them.
- **TB10: the words.** GTK's, from its catalogues (`gtkwindowcontrols.c`, `gtkwindowhandle.c`),
  checked against Qt's (`QMdiSubWindow`): they agree except French Maximise, GTK "Maximiser", Qt
  "Agrandir"; our bar is drawn only on GNOME, so GTK's.

## Precisions of the implementation (F2.11b3, approved by the owner on 2026-11-23)

Before writing, the owner chose: the band inside the window on the left, the right and the
bottom, and **over** the title bar's highest pixels at the top rather than above them (choice 1);
GNOME's `button-layout` read once at start-up, a change seen at the next start (choice 2); the
guides' picture a strip of the bar alone (choice 3). While writing, the rest:
- **TB7: the band.** 6 logical pixels, corners of 16 along each edge (`band_edge`). The interface
  asks it before routing the pointer to any widget: over the band no widget is hovered and the
  pointer takes the edge's shape (four new `PointerShape`s: `ew-resize`, `ns-resize`,
  `nwse-resize`, `nesw-resize`); a primary press asks `WindowRequest::Resize(ResizeEdge)`, which
  the window turns into `winit`'s `drag_resize_window` (ignored with the system's frame). While a
  menu is open the press belongs to the menu (it closes it). Maximised or in full screen there is
  no band: the content takes the whole window again. `winit` does not report GNOME's tiled
  states, so a window tiled to half the screen keeps its band (a known, minor limit).
- **TB7: the line.** One `line-width` of `border` around the window, drawn by a `WindowFrame`, a
  root of its own in the widget arena recorded after the interface on layer 0: above the content
  (inset, so it never covers it), below every menu. Never hit by the pointer.
- **TB4: reading the layout.** `orchestra_ui::button_layout()` asks the desktop portal
  (`ReadOne`, then the older `Read`) on a thread of its own and waits at most 250 ms (ADR-0038
  S5's bound); KDE's portal does not expose GNOME's namespace, so there the answer is `None` and
  the bar uses GNOME's default. The parser skips `appmenu`, `menu`, `icon`, `spacer` and unknown
  names, keeps a repeated button once, and puts everything on the leading side without a colon,
  as GTK does; `:` alone hides every button, as in GTK. The program reads the setting in `main`,
  only for `Decorations::Own`, and passes it to the interface it builds, so tests and screenshots
  never depend on the desktop.
- **TB12: measured.** `title_bar_focus_change`, the panel of 200 widgets under our bar with three
  buttons, the window losing the focus: 0.39 M instructions (target 0.5 M).
- **TB11: the picture.** A scene `title-bar` of `cargo xtask screenshots`, 640 × 36 logical
  pixels: the program's four menus, its name, the three buttons of
  `appmenu:minimize,maximize,close`, in the frame. The guides still show placeholders for all
  their pictures; this one is named in its placeholder like the others.
