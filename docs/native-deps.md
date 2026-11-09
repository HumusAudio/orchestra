# Native (C/C++) dependencies

Orchestra is written in Rust. A C or C++ library is allowed **only when no reasonable Rust
alternative exists** (CLAUDE.md §6, ADR-0001). Every such exception is recorded here, *before* it
is merged, with its justification, license and a replacement plan.

"Native dependency" includes libraries linked through `-sys` crates, libraries loaded at run time
(`dlopen`), and system frameworks we bind to directly. Platform APIs reached through the operating
system's own interface (e.g. ALSA, WASAPI, CoreAudio) are listed too, because they constrain
portability, even though they cannot be "replaced".

## Registry

The entries below were recorded with ADR-0036 (F2.01), before the crates that load them enter the
workspace. Each crate arrives in its own SCALETTA step (ADR-0036 B1).

### X11 client libraries (via `winit` and `softbuffer`)

- **Used by:** `orchestra-ui` (window, input and presentation on an X11 session).
- **What for:** opening windows, receiving keyboard, mouse and IME input, setting the cursor,
  presenting pixels on X11. `winit` opens `libX11`, `libX11-xcb`, `libXcursor`, `libXi`,
  `libxcb` and `libxkbcommon-x11`; `softbuffer` opens `libX11`/`libxcb` for presentation.
- **Why no Rust alternative:** these libraries are the X11 system's own client interface.
  `x11rb` speaks the protocol in Rust, but winit 0.30 still uses Xlib for input and IME.
- **License:** MIT/X11 (X.Org libraries), MIT (libxkbcommon). Compatible with GPL-3.0-or-later:
  permissive licenses, and they are only loaded at run time, never shipped by us.
- **Platforms:** Linux only, loaded at run time with dlopen (`x11-dl`, `xkbcommon-dl`,
  `x11rb/dl-libxcb`, `tiny-xlib/dlopen`), only on an X11 session. Windows · macOS: not used.
- **Build requirements:** none (no headers, no pkg-config: dlopen).
- **Binary size / compile time impact:** none from the libraries; the Rust side is measured with
  the whole interface in `spikes/gui-blocks/RESULTS.md`.
- **Replacement plan:** none needed while X11 exists; a Wayland-only session never loads them.
- **Added:** 2026-11-09, ADR-0036.

### Wayland client library and libxkbcommon (via `winit` and `softbuffer`)

- **Used by:** `orchestra-ui` (window, input and presentation on a Wayland session).
- **What for:** `libwayland-client` is the connection to the compositor (winit's
  `smithay-client-toolkit`, softbuffer's shared-memory buffers). `libxkbcommon` turns key codes
  into characters with the user's keyboard layout, on both X11 and Wayland.
- **Why no Rust alternative:** `wayland-backend` has a pure-Rust implementation of the protocol,
  but softbuffer's Wayland support depends on `wayland-sys`, the binding to the system library,
  so the pure-Rust backend cannot be used with it. Keyboard layouts are only available through
  `libxkbcommon` and its database.
- **License:** MIT (both). Compatible with GPL-3.0-or-later.
- **Platforms:** Linux only, loaded at run time with dlopen (`wayland-sys/dlopen`,
  `xkbcommon-dl`). Windows · macOS: not used.
- **Build requirements:** none.
- **Binary size / compile time impact:** none from the libraries.
- **Replacement plan:** re-evaluate the pure-Rust Wayland backend when winit 0.31 arrives.
- **Added:** 2026-11-09, ADR-0036.

### libdbus-1 (via `rfd`)

- **Used by:** `orchestra-ui` (native file dialogs on Linux, from F2.12).
- **What for:** `rfd` 0.17 sends the "open file"/"save file" request to the XDG desktop portal,
  a D-Bus service, through `libdbus-1`.
- **Why no Rust alternative:** there is one, `zbus` (through `ashpd`, used by `rfd` ≤ 0.16), and
  it was measured. It costs +0.9 MB and +29 crates more than `rfd` 0.17 (ADR-0036 B4).
  `libdbus-1` is present on every desktop that has a portal.
- **License:** AFL-2.1 OR GPL-2.0-or-later. Compatible with GPL-3.0-or-later through the
  GPL-2.0-or-later option. Only loaded at run time, never shipped by us.
- **Platforms:** Linux only, loaded with dlopen (`libdbus-1.so.3`). If it is missing, rfd tries
  `zenity`, otherwise the dialog does not open; Orchestra must then show its own error (F2.12).
  Windows · macOS: not used (system dialogs).
- **Build requirements:** none.
- **Binary size / compile time impact:** none from the library.
- **Replacement plan:** back to a pure-Rust D-Bus if `rfd` offers it again at a reasonable cost,
  or if we write the portal call ourselves on the `zbus` that AccessKit already brings.
- **Added:** 2026-11-09, ADR-0036.

### Windowing, graphics and accessibility system APIs (Windows and macOS)

- **Used by:** `orchestra-ui`, through `winit`, `softbuffer`, `accesskit_windows`/
  `accesskit_macos`, `rfd`, `muda`.
- **What for:** on Windows, Win32 windows and input, GDI presentation, UI Automation, the common
  file dialog (`windows-sys`, `windows`). On macOS, AppKit windows, menus and dialogs, Core
  Graphics/Quartz Core presentation, NSAccessibility (`objc2` bindings).
- **Why no Rust alternative:** they are the operating systems' own interfaces.
- **License:** system components, not distributed by us.
- **Platforms:** Windows and macOS respectively, linked as system libraries/frameworks.
- **Build requirements:** none beyond the standard Rust targets.
- **Binary size / compile time impact:** measured by CI from F2.04.
- **Replacement plan:** none (platform APIs).
- **Added:** 2026-11-09, ADR-0036.

## Development tools (never linked into the program)

Native programs that the **development workflow and CI** need, but that are never linked into,
loaded by or shipped with Orchestra. They are not exceptions to ADR-0001; they are listed for
transparency, because a contributor has to install them.

### Valgrind

- **Used by:** `cargo xtask bench` (Gungraun runs each benchmark under Valgrind's Callgrind and
  reports instruction counts; ADR-0016).
- **License:** GPL-2.0-or-later. Only executed as a separate program, never linked.
- **Platforms:** Linux only (Arch/CachyOS: `pacman -S valgrind`; Ubuntu CI: `apt-get install
  valgrind`). On Windows and macOS benchmarks are compiled but not run (ADR-0016 §2).
- **Companion tool:** `gungraun-runner` (Rust, Apache-2.0 OR MIT), installed with
  `cargo install gungraun-runner --version <same as gungraun in Cargo.toml> --locked`.
- **Added:** 2026-10-31, ADR-0016.

## Entry template

Copy this block for every new native dependency:

```markdown
### <library name> (via `<crate-name>`)

- **Used by:** <workspace crate(s)>
- **What for:** <one paragraph>
- **Why no Rust alternative:** <the alternatives we evaluated and why they are not enough>
- **License:** <SPDX identifier> — compatible with GPL-3.0-or-later because <reason/source>
- **Platforms:** Linux <how it is obtained: pacman package / vendored / system> ·
  Windows <…> · macOS <…>
- **Build requirements:** <headers, pkg-config names, toolchains>
- **Binary size / compile time impact:** <measured numbers>
- **Replacement plan:** <what would let us drop it, and when we re-evaluate>
- **Added:** <YYYY-MM-DD>, ADR-<NNNN> (if any)
```
