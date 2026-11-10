// SPDX-License-Identifier: GPL-3.0-or-later

//! The event loop's handler (ADR-0040): owns the window, the surface, the frame and the display
//! list, translates `winit`'s events into ours for the [`WindowApp`], and draws frames on demand.

use std::num::NonZeroU32;
use std::rc::Rc;

use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

use super::input::{self, InputEvent, Modifiers};
use super::order::open_in_order;
use super::present::{CopyPlan, copy_region, copy_whole, softbuffer_rect};
use super::{WindowApp, WindowError, WindowOptions};
use crate::draw::device::DeviceRegions;
use crate::{CpuRenderer, DisplayList, Image, Point, Rect, Renderer};

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
}

/// The handler of the event loop.
pub(crate) struct Shell<A> {
    /// What the window shows.
    app: A,
    /// The options it was started with.
    options: WindowOptions,
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
    /// The damage rectangles for `present_with_damage`, reused from frame to frame.
    present_rects: Vec<softbuffer::Rect>,
    /// The error that stopped the loop, returned by [`super::run`].
    pub(crate) error: Option<WindowError>,
}

impl<A: WindowApp> Shell<A> {
    /// A handler for `app`, with no window yet.
    pub(crate) fn new(app: A, options: WindowOptions) -> Self {
        Self {
            app,
            options,
            list: DisplayList::new(),
            renderer: CpuRenderer::new(),
            open: None,
            modifiers: Modifiers::default(),
            pointer: Point::default(),
            present_rects: Vec::new(),
            error: None,
        }
    }

    /// Stops the loop with `error`.
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: WindowError) {
        self.error = Some(error);
        event_loop.exit();
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
            // The setup hook (ADR-0040 W7): the surface now; AccessKit's adapter in F2.08–F2.09.
            |window| -> Result<_, WindowError> {
                let context =
                    Context::new(Rc::clone(window)).map_err(|error| surface_error(&error))?;
                let surface = Surface::new(&context, Rc::clone(window))
                    .map_err(|error| surface_error(&error))?;
                Ok((context, surface))
            },
        )?;
        let (context, surface) = surface?;
        let scale = window.scale_factor();
        let size = window.inner_size();
        self.open = Some(Open {
            window,
            _context: context,
            surface,
            frame: None,
            scale,
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
            .render(&self.list, &self.options.theme, &damage, scale, frame)
            .map_err(WindowError::Draw)?;
        let mut buffer = open
            .surface
            .buffer_mut()
            .map_err(|error| surface_error(&error))?;
        match CopyPlan::choose(buffer.age(), &regions, width, height) {
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

    /// The scale in effect, 1 before the window exists.
    fn scale(&self) -> f64 {
        self.open.as_ref().map_or(1.0, |open| open.scale)
    }
}

impl<A: WindowApp> ApplicationHandler for Shell<A> {
    /// Creates the window the first time the application is resumed (`winit` 0.30 allows window
    /// creation only from here on).
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.open.is_none()
            && let Err(error) = self.open_window(event_loop)
        {
            self.fail(event_loop, error);
        }
    }

    /// Translates one window event, and asks for a frame if anything is damaged (W2).
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
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
            // Visible again (restored, uncovered): the system may have dropped the window's
            // pixels, so present the frame again (a whole copy, see `CopyPlan::choose`).
            WindowEvent::Occluded(false) => {
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
        }
    }
}

/// A `softbuffer` error as ours.
fn surface_error(error: &softbuffer::SoftBufferError) -> WindowError {
    WindowError::Surface {
        detail: error.to_string(),
    }
}
