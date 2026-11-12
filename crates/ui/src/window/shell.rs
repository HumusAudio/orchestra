// SPDX-License-Identifier: GPL-3.0-or-later

//! The event loop's handler (ADR-0040): owns the window, the surface, the frame and the display
//! list, translates `winit`'s events into ours for the [`WindowApp`], draws frames on demand, and
//! follows the system's light/dark preference (`super::appearance`).

use std::num::NonZeroU32;
use std::rc::Rc;

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use super::appearance::{ShellEvent, ThemeState, appearance_from_winit};
use super::input::{self, InputEvent, Modifiers};
use super::order::open_in_order;
use super::present::{CopyPlan, copy_region, copy_whole, softbuffer_rect};
use super::{WindowApp, WindowError, WindowOptions};
use crate::draw::device::DeviceRegions;
use crate::platform::appearance::{Reporter, SystemObservation, observe_system};
use crate::{AppearanceWatch, CpuRenderer, DisplayList, Image, Point, Rect, Renderer};

/// The window and what draws into it, once it exists.
struct Open {
    /// The window. `Rc` because the surface holds it too.
    window: Rc<Window>,
    /// `softbuffer`'s connection to the display; kept alive with the surface.
    _context: Context<Rc<Window>>,
    /// Where the frame is presented.
    surface: Surface<Rc<Window>, Rc<Window>>,
    /// Our full frame in device pixels (ADR-0040 W4); `None` while the window has no area.
    frame: Option<Image>,
    /// Device pixels per logical pixel.
    scale: f64,
    /// AccessKit's adapter (ADR-0045 WA5), created before the window was shown (W7).
    access: accesskit_winit::Adapter,
}

/// The handler of the event loop.
pub(crate) struct Shell<A> {
    /// What the window shows.
    app: A,
    /// The options it was started with.
    options: WindowOptions,
    /// The theme in use: the pair and mode of the options, and the half the system selects.
    theme: ThemeState,
    /// The way into our own event loop from other threads (ADR-0040 W3); a clone goes into the
    /// appearance callback.
    proxy: EventLoopProxy<ShellEvent>,
    /// The observation of the system's preference, started in the first `resumed` and kept for
    /// the whole session: dropping it would stop the changes (ADR-0038 S1).
    appearance: Option<AppearanceWatch>,
    /// Where `winit`'s `ThemeChanged` is passed on (Windows, macOS: ADR-0038 S3); `None` on Linux,
    /// where the portal's thread reports by itself, and before the first `resumed`.
    appearance_feed: Option<Reporter>,
    /// What to draw.
    list: DisplayList,
    /// How to draw it.
    renderer: CpuRenderer,
    /// The window, once created (`winit` creates windows only after `resumed`).
    open: Option<Open>,
    /// The modifiers held, kept to attach them to key events.
    modifiers: Modifiers,
    /// The pointer's last position, logical pixels, for button events.
    pointer: Point,
    /// `true` when the system said the window's pixels on screen may be lost (it became visible
    /// again): the next frame copies the whole frame (`CopyPlan::choose`), then this is reset.
    pixels_lost: bool,
    /// The damage rectangles for `present_with_damage`, reused from frame to frame.
    present_rects: Vec<softbuffer::Rect>,
    /// The error that stopped the loop, returned by [`super::run`].
    pub(crate) error: Option<WindowError>,
}

impl<A: WindowApp> Shell<A> {
    /// A handler for `app`, with no window yet. `proxy` belongs to the event loop that will run it.
    pub(crate) fn new(app: A, options: WindowOptions, proxy: EventLoopProxy<ShellEvent>) -> Self {
        let renderer = CpuRenderer::new(options.faces.clone());
        Self {
            app,
            theme: ThemeState::new(options.pair, options.mode),
            options,
            proxy,
            appearance: None,
            appearance_feed: None,
            list: DisplayList::new(),
            renderer,
            open: None,
            modifiers: Modifiers::default(),
            pointer: Point::default(),
            pixels_lost: false,
            present_rects: Vec::new(),
            error: None,
        }
    }

    /// Stops the loop with `error`.
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: WindowError) {
        self.error = Some(error);
        event_loop.exit();
    }

    /// Starts observing the system's preference (ADR-0038) and takes its first answer, before the
    /// window exists, so the first frame already has the right half of the pair.
    ///
    /// On Linux this waits at most 250 ms for the portal (ADR-0038 S5); on Windows and macOS the
    /// answer is `winit`'s, read here and at once. Every later change arrives as a
    /// [`ShellEvent::SystemAppearance`] through the proxy (`user_event`).
    fn start_appearance(&mut self, event_loop: &ActiveEventLoop) {
        let proxy = self.proxy.clone();
        let SystemObservation { watch, feed } = observe_system(
            event_loop.system_theme().map(appearance_from_winit),
            Box::new(move |value| {
                // Fails only when the event loop has already ended, that is while the program is
                // closing: there is no window left to theme, so the change is rightly dropped.
                let _closed = proxy.send_event(ShellEvent::SystemAppearance(value));
            }),
        );
        self.theme.set_system(watch.current());
        self.appearance = Some(watch);
        self.appearance_feed = feed;
    }

    /// Applies a new answer of the system: when the half to draw with changes, everything is
    /// damaged and a frame is requested (ADR-0037 T6). Layout does not depend on colours, so
    /// nothing is re-recorded.
    fn apply_system_appearance(&mut self, system: Option<crate::Appearance>) {
        if self.theme.set_system(system) {
            self.list.damage_all();
            if let Some(open) = self.open.as_ref() {
                open.window.request_redraw();
            }
        }
    }

    /// Creates the window (invisible), its surface and the first frame, then shows it (W7).
    fn open_window(&mut self, event_loop: &ActiveEventLoop) -> Result<(), WindowError> {
        let attributes = Window::default_attributes()
            .with_title(self.options.title.clone())
            .with_inner_size(LogicalSize::new(
                f64::from(self.options.width),
                f64::from(self.options.height),
            ))
            .with_visible(false);
        let (window, surface) = open_in_order(
            || {
                event_loop
                    .create_window(attributes)
                    .map(Rc::new)
                    .map_err(|error| WindowError::CreateWindow {
                        detail: error.to_string(),
                    })
            },
            // The setup hook (ADR-0040 W7): the surface and AccessKit's adapter, which panics if
            // the window was ever shown, so it is created here, between creation and showing.
            |window| -> Result<_, WindowError> {
                let context =
                    Context::new(Rc::clone(window)).map_err(|error| surface_error(&error))?;
                let surface = Surface::new(&context, Rc::clone(window))
                    .map_err(|error| surface_error(&error))?;
                let access = accesskit_winit::Adapter::with_event_loop_proxy(
                    event_loop,
                    window,
                    self.proxy.clone(),
                );
                Ok((context, surface, access))
            },
        )?;
        let (context, surface, access) = surface?;
        // The system may have changed between `system_theme()` and the window's creation, and no
        // `ThemeChanged` is promised for that moment: the window's own theme (Windows, macOS,
        // which follow the system because we never call `set_theme`) closes the gap. A repeat is
        // dropped by the reporter. On Linux there is no feed, and `winit` would only echo what the
        // application set.
        if let (Some(feed), Some(theme)) = (self.appearance_feed.as_ref(), window.theme()) {
            feed.answer(Some(appearance_from_winit(theme)));
        }
        let scale = window.scale_factor();
        let size = window.inner_size();
        self.open = Some(Open {
            window,
            _context: context,
            surface,
            frame: None,
            scale,
            access,
        });
        self.resize(size)?;
        Ok(())
    }

    /// Adapts to a new size or scale (W6): reallocates the frame, resizes the surface, asks the
    /// app to lay the interface out again, and damages everything.
    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<(), WindowError> {
        let Some(open) = self.open.as_mut() else {
            return Ok(());
        };
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            // Minimised or zero-sized: nothing to draw until it has an area again.
            open.frame = None;
            return Ok(());
        };
        open.surface
            .resize(width, height)
            .map_err(|error| surface_error(&error))?;
        open.frame = Some(Image::new(size.width, size.height).map_err(WindowError::Draw)?);
        let bounds = Rect::new(
            0.0,
            0.0,
            input::to_logical(f64::from(size.width), open.scale),
            input::to_logical(f64::from(size.height), open.scale),
        );
        self.app.layout(bounds, &mut self.list);
        self.list.damage_all();
        open.window.request_redraw();
        Ok(())
    }

    /// Draws the damage into the frame and presents it (W4).
    fn paint(&mut self) -> Result<(), WindowError> {
        let Some(open) = self.open.as_mut() else {
            return Ok(());
        };
        let Some(frame) = open.frame.as_mut() else {
            return Ok(());
        };
        let damage = self.list.take_damage();
        #[allow(
            clippy::cast_possible_truncation,
            reason = "scale factors are small numbers like 1, 1.25 or 2; f64 is only winit's type"
        )]
        let scale = open.scale as f32;
        let (width, height) = (frame.width(), frame.height());
        let regions = DeviceRegions::new(&damage, scale, width, height);
        self.renderer
            .render(&self.list, self.theme.theme(), &damage, scale, frame)
            .map_err(WindowError::Draw)?;
        let mut buffer = open
            .surface
            .buffer_mut()
            .map_err(|error| surface_error(&error))?;
        let plan = CopyPlan::choose(buffer.age(), &regions, width, height, self.pixels_lost);
        self.pixels_lost = false;
        match plan {
            CopyPlan::Whole => {
                copy_whole(frame, &mut buffer);
                buffer.present().map_err(|error| surface_error(&error))?;
            }
            CopyPlan::Damage => {
                self.present_rects.clear();
                for region in regions.iter() {
                    copy_region(frame, region, &mut buffer);
                    self.present_rects.extend(softbuffer_rect(region));
                }
                buffer
                    .present_with_damage(&self.present_rects)
                    .map_err(|error| surface_error(&error))?;
            }
        }
        Ok(())
    }

    /// Hands an input event to the app; the segments it re-records damage the list.
    fn deliver(&mut self, event: &InputEvent) {
        self.app.input(event, &mut self.list);
    }

    /// Sends AccessKit what changed in the interface, if an assistive technology is listening
    /// (WC6). The app is asked only then: without a listener its marks just wait (WC3).
    fn push_access(&mut self, full: bool) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let app = &mut self.app;
        let title = &self.options.title;
        open.access.update_if_active(|| {
            app.access_update(full).map_or_else(
                || crate::widget::access::window_only(title),
                crate::AccessUpdate::into_tree_update,
            )
        });
    }

    /// An AccessKit request (WC5, WC6).
    fn handle_access(&mut self, event: accesskit_winit::WindowEvent) {
        match event {
            accesskit_winit::WindowEvent::InitialTreeRequested => self.push_access(true),
            accesskit_winit::WindowEvent::ActionRequested(request) => {
                if let Some(action) = crate::widget::access::action_from(&request) {
                    self.app.access_action(action, &mut self.list);
                    self.push_access(false);
                    if !self.list.damage().is_empty()
                        && let Some(open) = self.open.as_ref()
                    {
                        open.window.request_redraw();
                    }
                }
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
        }
    }

    /// The scale in effect, 1 before the window exists.
    fn scale(&self) -> f64 {
        self.open.as_ref().map_or(1.0, |open| open.scale)
    }
}

impl<A: WindowApp> ApplicationHandler<ShellEvent> for Shell<A> {
    /// Creates the window the first time the application is resumed (`winit` 0.30 allows window
    /// creation only from here on), after starting to observe the system's preference.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.appearance.is_none() {
            self.start_appearance(event_loop);
        }
        if self.open.is_none()
            && let Err(error) = self.open_window(event_loop)
        {
            self.fail(event_loop, error);
        }
    }

    /// Translates one window event, and asks for a frame if anything is damaged (W2).
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // AccessKit's adapter follows the window's focus and size itself (WC6).
        if let Some(open) = self.open.as_mut() {
            open.access.process_event(&open.window, &event);
        }
        let scale = self.scale();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Err(error) = self.resize(size) {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // A `Resized` with the new physical size follows; it lays out and damages.
                if let Some(open) = self.open.as_mut() {
                    open.scale = scale_factor;
                }
                self.list.damage_all();
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.paint() {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = Point::new(
                    input::to_logical(position.x, scale),
                    input::to_logical(position.y, scale),
                );
                self.deliver(&InputEvent::PointerMoved {
                    position: self.pointer,
                });
            }
            WindowEvent::CursorLeft { .. } => self.deliver(&InputEvent::PointerLeft),
            WindowEvent::MouseInput { state, button, .. } => {
                self.deliver(&InputEvent::PointerButton {
                    button: input::pointer_button(button),
                    state: input::button_state(state),
                    position: self.pointer,
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.deliver(&InputEvent::Wheel {
                    delta: input::wheel_delta(delta, scale),
                });
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = Modifiers::from_winit(modifiers.state());
                self.deliver(&InputEvent::ModifiersChanged {
                    modifiers: self.modifiers,
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.deliver(&InputEvent::Key {
                    key: input::key(&event.logical_key),
                    state: input::button_state(event.state),
                    repeat: event.repeat,
                    modifiers: self.modifiers,
                });
            }
            WindowEvent::Focused(focused) => self.deliver(&InputEvent::Focus(focused)),
            // The system's preference changed (Windows, macOS; never sent on Linux). It goes
            // through the reporter, so it reaches `user_event` like the portal's changes.
            WindowEvent::ThemeChanged(theme) => {
                if let Some(feed) = self.appearance_feed.as_ref() {
                    feed.answer(Some(appearance_from_winit(theme)));
                }
            }
            // Visible again (restored, uncovered): the system may have dropped the window's
            // pixels, so the next frame is a whole copy (`CopyPlan::choose`), even if a pointer
            // move arriving at the same time damages part of it.
            WindowEvent::Occluded(false) => {
                self.pixels_lost = true;
                if let Some(open) = self.open.as_ref() {
                    open.window.request_redraw();
                }
            }
            _ => {}
        }
        // Frames on demand (W2): only when something is damaged.
        if !self.list.damage().is_empty()
            && let Some(open) = self.open.as_ref()
        {
            open.window.request_redraw();
            // Something changed in the interface: AccessKit may need to know (WC3).
            self.push_access(false);
        }
    }

    /// Our own events, sent through the proxy (ADR-0040 W3): today the system's preference.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: ShellEvent) {
        match event {
            ShellEvent::SystemAppearance(system) => self.apply_system_appearance(system),
            ShellEvent::Access(event) => self.handle_access(event),
        }
    }
}

/// A `softbuffer` error as ours.
fn surface_error(error: &softbuffer::SoftBufferError) -> WindowError {
    WindowError::Surface {
        detail: error.to_string(),
    }
}
