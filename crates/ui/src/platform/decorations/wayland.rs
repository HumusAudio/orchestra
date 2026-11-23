// SPDX-License-Identifier: GPL-3.0-or-later

//! The Wayland probe of the decorations (ADR-0049 TB2, F2.11b2): asks the compositor whether it
//! draws the windows' frames, before our window exists. Linux only.
//!
//! # How
//! 1. Connect to the compositor named by the environment (`WAYLAND_DISPLAY`, as `winit` does). No
//!    connection: `winit` will use X11, so [`ProbeAnswer::NotWayland`].
//! 2. First round trip: read the globals, and bind `wl_compositor`, `xdg_wm_base` and, if the
//!    compositor offers the optional protocol, `zxdg_decoration_manager_v1`.
//! 3. Make a surface, its `xdg_surface` and `xdg_toplevel`, ask the toplevel's decoration object
//!    for `server_side`, and commit **without a buffer**: an unmapped toplevel is never shown.
//! 4. Second round trip: the compositor configures the toplevel and sends the decoration mode it
//!    chose. That mode is the answer.
//! 5. Destroy everything in reverse order and drop the connection.
//!
//! Merely seeing the decoration manager is not enough: a compositor may offer the protocol and
//! still answer `client_side` (Rerun's viewer found that; the protocol allows it), so the mode is
//! always read back.
//!
//! # Why a connection of our own
//! `winit` has not connected yet when we ask (the answer decides how the window is created), and
//! it does not lend its connection anyway. A second, short-lived connection costs one socket and
//! two round trips, once per run.
//!
//! # Threads
//! Everything runs on the calling thread and ends before returning; `xdg_wm_base`'s `ping` is
//! answered, so a compositor that pings during the probe does not take us for frozen.

use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum, delegate_noop};
use wayland_protocols::xdg::decoration::zv1::client::zxdg_decoration_manager_v1::ZxdgDecorationManagerV1;
use wayland_protocols::xdg::decoration::zv1::client::zxdg_toplevel_decoration_v1::{
    self, Mode, ZxdgToplevelDecorationV1,
};
use wayland_protocols::xdg::shell::client::xdg_surface::{self, XdgSurface};
use wayland_protocols::xdg::shell::client::xdg_toplevel::XdgToplevel;
use wayland_protocols::xdg::shell::client::xdg_wm_base::{self, XdgWmBase};

use super::ProbeAnswer;

/// What the probe learns from the compositor's events.
#[derive(Debug, Default)]
struct State {
    /// The compositor, bound at the first round trip.
    compositor: Option<WlCompositor>,
    /// The XDG shell, bound at the first round trip.
    wm_base: Option<XdgWmBase>,
    /// The decoration manager, if the compositor offers the protocol.
    decoration_manager: Option<ZxdgDecorationManagerV1>,
    /// The mode the compositor chose for our toplevel, if it said.
    mode: Option<WEnum<Mode>>,
}

/// The highest versions of the globals the probe understands. Binding at most these keeps a
/// newer compositor from sending events this code does not know.
const COMPOSITOR_VERSION: u32 = 4;
/// See [`COMPOSITOR_VERSION`].
const WM_BASE_VERSION: u32 = 2;
/// See [`COMPOSITOR_VERSION`].
const DECORATION_VERSION: u32 = 1;

/// Asks the compositor (see the module documentation).
pub(super) fn probe() -> ProbeAnswer {
    // No compositor, or not reachable: winit will not use Wayland either.
    let Ok(connection) = Connection::connect_to_env() else {
        return ProbeAnswer::NotWayland;
    };
    let mut queue = connection.new_event_queue::<State>();
    let handle = queue.handle();
    let _registry = connection.display().get_registry(&handle, ());
    let mut state = State::default();
    // First round trip: the registry lists the globals, `Dispatch<WlRegistry>` binds ours.
    if queue.roundtrip(&mut state).is_err() {
        return ProbeAnswer::Failed;
    }
    let (Some(compositor), Some(wm_base)) = (state.compositor.take(), state.wm_base.take()) else {
        // A Wayland compositor without the XDG shell cannot show winit's window either; the
        // answer hardly matters, and drawing our bar is the safe one.
        return ProbeAnswer::Failed;
    };
    let Some(manager) = state.decoration_manager.take() else {
        wm_base.destroy();
        return ProbeAnswer::NoManager;
    };
    // A toplevel that is never given a buffer: it is configured, never mapped, never shown.
    let surface = compositor.create_surface(&handle, ());
    let xdg_surface = wm_base.get_xdg_surface(&surface, &handle, ());
    let toplevel = xdg_surface.get_toplevel(&handle, ());
    let decoration = manager.get_toplevel_decoration(&toplevel, &handle, ());
    decoration.set_mode(Mode::ServerSide);
    surface.commit();
    // Second round trip: the configure events, the decoration's mode among them.
    let answer = match queue.roundtrip(&mut state) {
        Err(_) => ProbeAnswer::Failed,
        Ok(_) => match state.mode {
            Some(WEnum::Value(Mode::ServerSide)) => ProbeAnswer::ServerSide,
            Some(WEnum::Value(Mode::ClientSide)) => ProbeAnswer::ClientSide,
            // A mode this version of the protocol does not know: treat it as "not ours".
            Some(_) => ProbeAnswer::Failed,
            None => ProbeAnswer::NoAnswer,
        },
    };
    // Reverse order of creation: the protocol requires the decoration object to go before its
    // toplevel, and the toplevel before its surfaces.
    decoration.destroy();
    toplevel.destroy();
    xdg_surface.destroy();
    surface.destroy();
    manager.destroy();
    wm_base.destroy();
    // Send the destructions before the connection is dropped; a failure changes nothing now.
    let _flushed = connection.flush();
    answer
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        (): &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        // Only the globals the probe needs; each bound at most at the version it understands.
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" => {
                state.compositor =
                    Some(registry.bind(name, version.min(COMPOSITOR_VERSION), handle, ()));
            }
            "xdg_wm_base" => {
                state.wm_base = Some(registry.bind(name, version.min(WM_BASE_VERSION), handle, ()));
            }
            "zxdg_decoration_manager_v1" => {
                state.decoration_manager =
                    Some(registry.bind(name, version.min(DECORATION_VERSION), handle, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm_base: &XdgWmBase,
        event: xdg_wm_base::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // A compositor pings to know whether a client is alive; answering keeps us "responsive".
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm_base.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, ()> for State {
    fn event(
        _: &mut Self,
        xdg_surface: &XdgSurface,
        event: xdg_surface::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // Acknowledging the configure is what a well-behaved client does; the surface still gets
        // no buffer, so it is never shown.
        if let xdg_surface::Event::Configure { serial } = event {
            xdg_surface.ack_configure(serial);
        }
    }
}

impl Dispatch<ZxdgToplevelDecorationV1, ()> for State {
    fn event(
        state: &mut Self,
        _: &ZxdgToplevelDecorationV1,
        event: zxdg_toplevel_decoration_v1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The answer of the probe.
        if let zxdg_toplevel_decoration_v1::Event::Configure { mode } = event {
            state.mode = Some(mode);
        }
    }
}

// The objects whose events the probe does not need (a surface's enter/leave, the toplevel's own
// configure and close, the managers have none).
delegate_noop!(State: ignore WlCompositor);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: ignore XdgToplevel);
delegate_noop!(State: ignore ZxdgDecorationManagerV1);
