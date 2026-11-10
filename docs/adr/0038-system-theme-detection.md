# ADR-0038 — Reading the system's light/dark preference: our portal code on Linux, winit elsewhere

- **Status:** Accepted (owner, 2026-11-10; S4 option (a) and S5's 250 ms chosen by the owner)
- **Date:** 2026-11-10
- **Implements:** ADR-0014 §5 (system preference detection in `platform/`, changes observed while
  the program runs), within ADR-0006 §2 (platform code only in `platform/` behind traits) and
  ADR-0037 T9 (*System* with no answer gives dark). Closes the point ADR-0036 B10 left to this
  step.

## Context

The user's mode setting is *System / Light / Dark*, default *System* (ADR-0014 §1, ADR-0037 T1).
`ThemeMode::resolve(Option<Appearance>)` already exists in `orchestra-ui` (F2.02a): it needs the
operating system's preference, or `None` when there is none. Step F2.02b (SCALETTA) produces that
`Option` and its changes. ADR-0014 §5 names the sources: on Linux, the XDG desktop portal
(`org.freedesktop.portal.Settings`, namespace `org.freedesktop.appearance`, key `color-scheme`,
a `u32`: 0 = no preference, 1 = dark, 2 = light), which is a D-Bus service; on Windows, the
`AppsUseLightTheme` registry value; on macOS, `NSApp.effectiveAppearance`.

On 2026-11-10 we measured the candidates in `spikes/gui-blocks` (feature `theme-*`, results in
`spikes/gui-blocks/RESULTS.md`). The figures are differences against the same binary with every
building block of ADR-0036 (Linux x86-64, release, one codegen unit, stripped):

| Candidate | Extra crates Linux / Windows / macOS | Linux binary | Notes |
|---|---|---|---|
| libdbus-1 loaded with dlopen, as `rfd` 0.17 does | 0 | — | `rfd`'s D-Bus code is private: we would write about 600 lines of `unsafe` FFI, and `orchestra-ui` is `#![forbid(unsafe_code)]` |
| our code on `zbus` 5.19, with AccessKit's features | **0** / 0 / 0 | +0.62 MB | about 60 lines, no `unsafe`; the async stack is already there for AccessKit (ADR-0036) |
| `dark-light` 3.0.0 | +2 / +4 / +3 | +0.88 MB | all three platforms, but on macOS a thread **polls every second** and reads `AppleInterfaceStyle`, not `effectiveAppearance`; errors are strings |
| `mundy` 0.2.3 | +2 on Linux (Windows/macOS not measured cleanly) | not built | far more than we need (contrast, accent colour, motion…) |
| `winit` 0.30 | 0 | — | Windows and macOS: exactly ADR-0014 §5; Linux: always `None` |

From `winit` 0.30.13's source: `ActiveEventLoop::system_theme()` works **before any window
exists** on Windows (`should_use_dark_mode`, the registry) and on macOS (`effectiveAppearance`),
and both send `WindowEvent::ThemeChanged`. On Linux it returns `None` on X11 and Wayland, and
`Window::theme` returns only what the application set.

The owner ran both Linux candidates on their desktop (2026-11-10): the initial value and every
change were read correctly. The portal sent **each change three times** (`changed: light` ×3):
the code must pass on only real transitions. In the sandbox the D-Bus socket is not reachable
(`Operation not permitted`), and both candidates reported it as an error without crashing.

`zbus` 6.2.1 exists (2026-11-01), but AccessKit's `accesskit_unix` 0.24.1 uses 5.19 and the
project denies duplicate versions; 6.2 is also under two weeks old.

## Decision

### S1 — One trait in `orchestra-ui/src/platform/`

A small trait, the only door to the system preference:

- read the preference now: `Option<Appearance>` (`None` = no answer, no portal, no preference);
- observe it: a callback receives each **real transition** (a value equal to the last one
  delivered is dropped), until the returned handle is dropped.

The callback is `Send`: it runs on a thread of ours and will be bridged to the UI event loop in
F2.04 (`winit`'s `EventLoopProxy`). Nothing else in the code knows which platform answered.

### S2 — Linux: our own code on `zbus` 5.19

- Dependency: `zbus = { version = "=5.19.0", default-features = false, features = ["async-io"] }`
  under `[target.'cfg(target_os = "linux")'.dependencies]`: the same version and features as
  AccessKit, so it adds no crate once AccessKit is in, and our tree shares one async executor
  stack (`async-io`, no `tokio`). No `blocking-api`, no proxy macros of ours.
- Reading: `ReadOne(namespace, key)` on the portal. If the portal is older and lacks `ReadOne`
  (`UnknownMethod`), fall back to the deprecated `Read`, whose value is wrapped in one more
  variant.
- Observing: a match rule on the `SettingChanged` signal of the portal's path and interface, read
  as a message stream on a dedicated thread (`zbus::block_on`). The thread filters namespace and
  key, decodes the value, drops repeats (S1) and calls the callback.
- Our own D-Bus connection, separate from AccessKit's (which keeps its own inside its adapter):
  one more socket, nothing else.
- Errors never reach the user as a failure: any error, at start or while observing, means "no
  answer" (`None`), the theme falls back to T9 and the error is recorded for diagnostics.

### S3 — Windows and macOS: `winit`, in F2.04

When `winit` enters the workspace (F2.04), the same trait is implemented with
`ActiveEventLoop::system_theme()` for the initial value and `WindowEvent::ThemeChanged` for the
changes. No extra dependency, no polling, and on macOS the source ADR-0014 §5 names. The program
never calls `Window::set_theme` with a fixed value, because on both platforms that stops
`ThemeChanged`. Until F2.04 these two platforms answer `None` (T9: dark), which is harmless
because there is no window yet.

### S4 — The value 0, "no preference", means light

The portal's 0 means "no preference". On GNOME, the *Default* (light) style of the Settings app
is stored as `default`, which the portal reports as **0**, not 2 (to be confirmed on a GNOME
machine). Read as "no answer", it would show the **dark** theme to a GNOME user who chose light.
So **0 → light** (owner's choice, 2026-11-10): the portal answered, and on the desktops that send
0 the appearance the user sees is light. T9's dark stays for "no answer at all": no portal, an
error, an unknown number, Windows and macOS before F2.04.

The alternative, 0 → no answer → dark (T9 applied literally), was rejected for the GNOME case
above.

### S5 — Startup must not wait for D-Bus

The first answer can be slow: when the portal is not running, D-Bus may try to start it and the
call can wait up to D-Bus's own timeout (25 s by default). Startup must stay under 3 s (CLAUDE.md
§5). So the first read waits at most **250 ms**; if no answer has arrived, the program starts with
T9 (dark) and switches as soon as the answer comes, through the same callback as the changes.
250 ms (owner's choice, 2026-11-10) is the largest wait that leaves startup well inside its budget, and long enough for a
running portal (a local round trip of a few milliseconds).

### S6 — Tests

Automatic tests cover what does not need a bus: decoding the portal's values (0, 1, 2, unknown
numbers, wrong types, the double variant of `Read`), dropping repeated values, and the error →
`None` rule. The real reading needs a session bus with a portal, which CI runners do not have: it
is checked by hand by the owner on their desktop, like the window in F2.01. On a machine without
a bus, a test checks that opening the source gives `None` quickly and does not panic.

## Alternatives rejected

- **libdbus-1 with dlopen, our own FFI:** zero crates, but about 600 lines of `unsafe` in a crate
  that forbids it, to save nothing (the `zbus` stack is already paid for by AccessKit).
- **`dark-light` everywhere:** simplest to wire, but macOS polls every second (a wake-up per
  second for the whole session, against CLAUDE.md §5), uses an older source than ADR-0014 §5,
  and costs 2–4 crates per platform. Our Linux code is about as long as the glue it would need.
- **`mundy`:** a larger API for preferences we do not use yet (SCALETTA §1.6: no work "for
  later"). Worth a look again if we ever read contrast or reduced motion from the system.
- **`winit` alone, on Linux too:** it has no Linux implementation in 0.30.
- **`zbus` 6:** a second `zbus` next to AccessKit's 5.19, and too new.

## Consequences

- `zbus` and its async stack enter the workspace on Linux in this step, before AccessKit
  (ADR-0036 B1: each crate arrives in the step that needs it). When AccessKit arrives it adds
  none of them again. `cargo deny` must stay green; any duplicate is listed in `deny.toml` with
  its exit (ADR-0036 B7).
- No new native library: `zbus` is pure Rust and speaks the D-Bus protocol on the socket itself.
- F2.04 gains a task: the Windows and macOS implementations of S1 with `winit` (S3), and the
  bridge from our callback to the event loop.
- On a Linux desktop without a portal (some minimal window managers), *System* shows dark (T9),
  and the user can choose *Light* or *Dark* by hand.
