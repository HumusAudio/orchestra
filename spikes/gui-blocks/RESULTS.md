# Spike F2.01 — the UI building blocks of ADR-0012, measured together

Run on 2026-11-09 on the development machine (Intel N150, 4 cores, Rust 1.99.1), `release`
profile with one codegen unit like the product (ADR-0035), binaries stripped (the `spikes`
profile). Versions checked on crates.io the same day. ADR-0036 is the decision built on these
numbers.

```sh
cd spikes
cargo build --release -p gui-blocks                     # every block (default features)
./target/release/gui-blocks headless out/blocks.png     # no window, no display
./target/release/gui-blocks window                      # O = open-file dialog, Esc = quit
cargo build --release -p gui-blocks --no-default-features --features dialogs,csd   # without AccessKit
```

## What the binary contains

One line of text in each interface language and some Unicode shaping cases, in Inter, shaped
by `harfrust` and drawn from `skrifa` outlines by `tiny-skia`; five SMuFL glyphs from Bravura;
the PNG written headless. The window mode presents the same pixmap with `winit` + `softbuffer`,
publishes a two-node AccessKit tree, opens the `rfd` dialog on `O`, and on macOS installs a
`muda` menu. Both fonts are embedded (`include_bytes!`): **1.50 MB** of the binary is font data
(Inter 0.61 MB, Bravura 0.89 MB), which the product embeds anyway (ADR-0006).

The headless mode works in the sandbox (picture checked by eye: correct kerning, ligatures,
accents and SMuFL glyphs). The **window mode could not be tried here**: the sandbox has no
compositor (`WaylandError(Connection(NoCompositor))`). The owner runs it on their desktop.

## Size, crates, build time

| variant (features) | binary (stripped) | crates Linux / Windows / macOS |
|---|---|---|
| everything (`a11y`, `dialogs`, `csd`) | **9.64 MB** | 163 / 70 / 82 |
| without AccessKit | 6.43 MB | 94 / 52 / 76 |
| without rfd | 9.58 MB | 161 / 69 / 79 |
| without Wayland decorations (`csd`) | 9.34 MB | 160 / 70 / 82 |
| only window + drawing + text | 6.34 MB | 92 / 51 / 73 |
| rfd 0.16 (ashpd + zbus) instead of 0.17 | 10.57 MB | 192 / 73 / 82 |

Crate counts are distinct packages in `cargo tree -e normal --target <t>`, so they include build
scripts and proc macros. Sizes are Linux x86-64 only: Windows and macOS sizes come with the CI of
F2.04.

Clean release build, everything: **213 s**; without AccessKit: **137 s** (the ADR-0012 spike's
`gui-own`, with neither text shaping nor accessibility: 98 s on the same machine).

What it costs, read off the table:
- **AccessKit on Linux: +3.2 MB, +69 crates, +76 s of clean build.** It talks to AT-SPI over
  D-Bus with `zbus` and `atspi`, an async stack (`async-io`, `async-executor`, `futures`, `serde`,
  proc macros). On Windows +18 crates, on macOS +6. Accessibility is required (ADR-0012), so this
  is the price, not a choice. We use the `async-io` executor, not `tokio`.
- **rfd 0.17: +0.07 MB, +2 crates.** Since 0.17 rfd reaches the XDG portal through **libdbus-1
  loaded with dlopen**, not `ashpd`/`zbus`. Version 0.16 (ashpd) would cost +0.9 MB and +29 more
  crates even though `zbus` is already there for AccessKit, so 0.17 is clearly better.
- **Wayland client-side decorations (`sctk-adwaita`): +0.3 MB** and a second `tiny-skia` (0.11).
- Window, presentation, drawing, text and the fonts: 6.34 MB, of which 1.5 MB is font data.

## What links and what is loaded at run time (Linux)

`ldd` of the binary: only `libc`, `libm`, `libgcc_s`. Everything else is opened with dlopen when
it is needed, so the binary starts on a system without X11 or without Wayland:

- `winit` (X11): `libX11`, `libX11-xcb`, `libXcursor`, `libXi`, `libxcb`, `libxkbcommon-x11`;
- `winit` and `softbuffer` (Wayland): `libwayland-client`; both: `libxkbcommon`;
- `rfd`: `libdbus-1` (if it is missing, rfd logs an error and falls back to running `zenity`,
  if installed).

AccessKit (`zbus`) and the fonts and drawing stack are pure Rust. On Windows and macOS everything
goes through the system APIs (`windows-sys`/`windows`; `objc2` bindings of AppKit, Core Graphics,
Quartz Core): platform APIs, listed for transparency.

## Licenses and duplicates (`cargo deny` with the project's `deny.toml`)

`licenses ok` for every feature on the project's targets.

`bans` **fails**: the project denies duplicate versions (`multiple-versions = "deny"`), and these
are real duplicates (only the normal and build-time duplicates of the same version are ignored):

| target | duplicate | why |
|---|---|---|
| all | `miniz_oxide` 0.8 / 0.9 | `png` 0.18 uses 0.8 directly and `flate2` (0.9) as well |
| all (build only) | `syn` 2 / 3 | proc macros of the async stack and of `bytemuck` moved to syn 3; `jni`, `num_enum`, `phf`, `enumflags2` did not |
| Linux | `rustix` 0.38 / 1, `linux-raw-sys` 0.4 / 0.12 | `winit` 0.30 (and its `smithay-client-toolkit`, `calloop`) |
| Linux | `tiny-skia` 0.11 / 0.12 | `sctk-adwaita` (the Wayland decorations) |
| Windows | `windows-sys` 0.52 / 0.61 | `winit` 0.30 |
| macOS | `objc2` 0.5 / 0.6 (+ `block2`, `objc2-foundation`, `objc2-app-kit`; `bitflags` 1 / 2) | `winit` 0.30 and `accesskit_macos` 0.27 on 0.5; `softbuffer`, `rfd`, `muda` on 0.6 |

Almost all of them come from `winit` 0.30, which is old: `winit` 0.31 has been in beta since
November 2025 (beta 4 on 2026-10-24), and `accesskit_winit` 0.34 still requires 0.30.

## API notes for the real code

- `skrifa` 0.49 / `read-fonts` 0.46 are in the middle of a new API: `skrifa::Font` (= the
  `read-fonts` font model) with `ScalerFont`/`PathSink`, and the old `FontRef`/`MetadataProvider`
  kept but hidden from the docs. `harfrust` 0.15 shapes the same `Font` directly
  (`ShaperFont::new(&font)`), so one parsed font serves both. Both crates publish a breaking
  minor version every 2–4 weeks: we pin the minor version and update them together.
- `harfrust` shapes in font units only (no font size): scale advances by `ppem / units_per_em`.
- `accesskit_winit::Adapter` **panics** if it is created after the window has been shown: the
  window must be created invisible, the adapter attached, then the window shown.
- Inter's outlines are CFF, Bravura's too: the CFF path of skrifa is exercised.

## F2.02b — reading the system light/dark preference (2026-11-10)

Three candidates, each a feature of this spike (`src/theme.rs`); `gui-blocks theme` prints the
preference, then every change. ADR-0038 is the decision built on these numbers.

```sh
cd spikes
cargo build --release -p gui-blocks --features theme-zbus        # our code on zbus 5.19
cargo build --release -p gui-blocks --features theme-darklight   # dark-light 3.0.0
./target/release/gui-blocks theme
```

| variant (on top of the default features) | binary (stripped) | crates Linux / Windows / macOS |
|---|---|---|
| default features, measured again today | 10.82 MB | 163 / 70 / 82 |
| + our code on `zbus` 5.19 (`async-io`, no default features) | 11.44 MB (+0.62) | 163 (+0) on Linux |
| + `dark-light` 3.0.0 | 11.71 MB (+0.88) | 165 / 74 / 85 |
| + `mundy` 0.2.3 (colour scheme only) | not built | 165 on Linux |

- The default build is 10.82 MB today instead of 9.64 MB on 2026-11-09, with the same
  `Cargo.lock` entries for the blocks; the differences above come from builds of the same day and
  compare with each other, not with the table of F2.01.
- The `zbus` feature is not limited to Linux in this spike, so its Windows/macOS counts are
  meaningless; in the product it is a Linux-only dependency.
- `dark-light` on macOS polls `AppleInterfaceStyle` every second on a thread; on Windows it waits
  on `RegNotifyChangeKeyValue` (event-driven); on Linux it uses `zbus` with the proxy macros.
- `winit` 0.30.13: `ActiveEventLoop::system_theme()` answers on Windows and macOS before any
  window exists, `None` on Linux (X11 and Wayland).
- In the sandbox the session bus is not reachable (`Operation not permitted`): both candidates
  report the error and exit cleanly. On the owner's desktop both read the initial value and every
  change; the portal sent **each change three times**.
