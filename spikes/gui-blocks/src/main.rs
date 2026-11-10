// SPDX-License-Identifier: GPL-3.0-or-later

//! Spike F2.01: every UI building block of ADR-0012 linked into ONE binary, so that we can measure
//! what the real `orchestra-ui` will cost before writing it: release binary size, clean build
//! time, number of crates per target, and which native libraries each platform needs.
//!
//! The code exercises each block just enough that the linker cannot drop it (dead code would make
//! the size measurement meaningless):
//! - `skrifa` + `harfrust` (fontations): shape a line of text in the bundled Inter font and a few
//!   SMuFL glyphs from Bravura, then draw the outlines;
//! - `tiny-skia`: fill those outlines into a pixmap; headless mode writes it to a PNG (ADR-0009);
//! - `winit` + `softbuffer`: window mode presents the same pixmap in a window;
//! - `accesskit` + `accesskit_winit`: window mode publishes a tiny accessibility tree;
//! - `rfd`: pressing `O` in the window opens the native "open file" dialog;
//! - `muda` (macOS only): a global menu bar with one item.
//!
//! Modes:
//! - `gui-blocks headless <out.png>` — no window, no display needed;
//! - `gui-blocks window` — interactive window (needs a display).
//!
//! Spike shortcuts (acceptable here, NOT in product code): errors panic, no glyph cache, the whole
//! window is redrawn on every frame.

mod theme;

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use harfrust::{Buffer, ShapeOptions, ShaperFont};
use skrifa::{DrawMemory, Font, GlyphId, PathSink, ScalerFont};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};

/// Bundled UI font (CFF outlines) and the SMuFL music font, embedded exactly as the product will.
const INTER: &[u8] = include_bytes!("../../../assets/fonts/inter/Inter-Regular.otf");
/// See [`INTER`].
const BRAVURA: &[u8] = include_bytes!("../../../assets/fonts/bravura/Bravura.otf");

/// Size of the rendered scene, in physical pixels.
const WIDTH: u32 = 960;
/// See [`WIDTH`].
const HEIGHT: u32 = 320;

/// Adapter from the fontations path sink to a tiny-skia path builder. Font units are y-up, the
/// pixmap is y-down: the transform applied when filling flips the axis.
struct Sink(PathBuilder);

impl PathSink for Sink {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to(x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to(x, y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.0.quad_to(cx0, cy0, x, y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.0.cubic_to(cx0, cy0, cx1, cy1, x, y);
    }
    fn close(&mut self) {
        self.0.close();
    }
}

/// Shapes `text` with harfrust and fills every glyph at `(x, baseline)`, `ppem` pixels per em.
///
/// harfrust always shapes in font units (it has no font size); we scale the advances ourselves,
/// and skrifa draws each outline already scaled to `ppem`.
fn draw_text(pixmap: &mut Pixmap, font: &Font, text: &str, ppem: f32, x: f32, baseline: f32) {
    let shaper = ShaperFont::new(font);
    let mut buffer = Buffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    harfrust::shape(&shaper, &mut buffer, ShapeOptions::new()).expect("shaping failed");

    let scaler = ScalerFont::unhinted(font.clone(), Some(ppem));
    let upem = f32::from(font.units_per_em().max(1));
    let units_to_px = ppem / upem;
    let mut pen_x = x;
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0x1f, 0x23, 0x28, 0xff));
    paint.anti_alias = true;
    for (info, pos) in buffer.glyph_infos().iter().zip(buffer.glyph_positions()) {
        let gx = pen_x + pos.x_offset as f32 * units_to_px;
        fill_glyph(
            pixmap,
            &scaler,
            GlyphId::new(info.glyph_id),
            gx,
            baseline,
            &paint,
        );
        pen_x += pos.x_advance as f32 * units_to_px;
    }
}

/// Fills one glyph outline of `scaler` with its origin at `(x, baseline)`.
fn fill_glyph(
    pixmap: &mut Pixmap,
    scaler: &ScalerFont,
    glyph: GlyphId,
    x: f32,
    baseline: f32,
    paint: &Paint,
) {
    let Some(outline) = scaler.outline(glyph) else {
        return;
    };
    let mut sink = Sink(PathBuilder::new());
    outline
        .draw(DrawMemory::Scratch, &mut sink)
        .expect("draw failed");
    // An empty outline (the space glyph) yields no path at all.
    if let Some(path) = sink.0.finish() {
        let flip = Transform::from_row(1.0, 0.0, 0.0, -1.0, x, baseline);
        pixmap.fill_path(&path, paint, FillRule::Winding, flip, None);
    }
}

/// Draws the whole scene: a line in each of the six interface languages and some Bravura glyphs.
fn render_scene() -> Pixmap {
    let inter = Font::new(INTER, 0).expect("Inter did not parse");
    let bravura = Font::new(BRAVURA, 0).expect("Bravura did not parse");
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT).expect("pixmap");
    pixmap.fill(Color::from_rgba8(0xfa, 0xfa, 0xf7, 0xff));
    let lines = [
        "Quarter note · Crotchet · Semiminima · Noire · Viertelnote · Negra",
        "Ünïcödé shaping: fi ffl “quotes” — Größe, déjà, Ñandú",
    ];
    for (i, line) in lines.iter().enumerate() {
        draw_text(
            &mut pixmap,
            &inter,
            line,
            22.0,
            24.0,
            48.0 + i as f32 * 36.0,
        );
    }
    // SMuFL code points in the Private Use Area: G clef, quarter note up, half note up, flat.
    let music: String = [
        '\u{E050}', ' ', '\u{E1D5}', '\u{E1D3}', '\u{E260}', '\u{E1D5}',
    ]
    .iter()
    .collect();
    draw_text(&mut pixmap, &bravura, &music, 64.0, 24.0, 230.0);
    pixmap
}

/// Headless mode: renders the scene and writes a PNG, without touching any windowing system.
fn headless(out: &str) {
    let start = Instant::now();
    let pixmap = render_scene();
    println!("rendered in {:.2} ms", start.elapsed().as_secs_f64() * 1e3);
    pixmap.save_png(out).expect("PNG write failed");
    println!("wrote {out}");
}

mod window {
    //! Window mode: winit window, softbuffer presentation, AccessKit tree, rfd dialog on `O`.

    use super::*;
    #[cfg(feature = "a11y")]
    use accesskit::{Node, NodeId, Role, TreeId, TreeInfo, TreeUpdate};
    use winit::application::ApplicationHandler;
    use winit::event::{ElementState, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
    use winit::keyboard::{Key, NamedKey};
    use winit::window::{Window, WindowId};

    /// Event type of the loop: AccessKit's when accessibility is linked in, nothing otherwise.
    #[cfg(feature = "a11y")]
    type UserEvent = accesskit_winit::Event;
    /// See the other definition.
    #[cfg(not(feature = "a11y"))]
    type UserEvent = ();

    /// AccessKit node ids of the tiny tree: a window with one label.
    #[cfg(feature = "a11y")]
    const ROOT: NodeId = NodeId(0);
    /// See [`ROOT`].
    #[cfg(feature = "a11y")]
    const LABEL: NodeId = NodeId(1);

    /// Builds the full accessibility tree (sent when a screen reader asks for it).
    #[cfg(feature = "a11y")]
    fn tree() -> TreeUpdate {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![LABEL]);
        root.set_label("Orchestra spike");
        let mut label = Node::new(Role::Label);
        label.set_value("Press O to open a file");
        TreeUpdate {
            nodes: vec![(ROOT, root), (LABEL, label)],
            tree: Some(TreeInfo::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus: ROOT,
        }
    }

    /// Everything the window needs once it exists.
    struct State {
        /// The window, shared with the softbuffer context.
        window: Rc<Window>,
        /// The softbuffer surface we copy the pixmap into.
        surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
        /// The AccessKit adapter; it must be created before the window is first shown.
        #[cfg(feature = "a11y")]
        adapter: accesskit_winit::Adapter,
        /// The scene, rendered once (the spike does not measure redraw cost; ADR-0012 did).
        scene: Pixmap,
        /// The macOS global menu, kept alive for the lifetime of the window.
        #[cfg(target_os = "macos")]
        _menu: muda::Menu,
    }

    /// The winit application.
    struct App {
        /// Proxy used by AccessKit to deliver its events into our loop.
        proxy: EventLoopProxy<UserEvent>,
        /// `None` until `resumed` creates the window.
        state: Option<State>,
    }

    impl ApplicationHandler<UserEvent> for App {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            let attrs = Window::default_attributes()
                .with_title("gui-blocks")
                .with_visible(false)
                .with_inner_size(winit::dpi::PhysicalSize::new(WIDTH, HEIGHT));
            let window = Rc::new(event_loop.create_window(attrs).expect("window"));
            #[cfg(feature = "a11y")]
            let adapter = accesskit_winit::Adapter::with_event_loop_proxy(
                event_loop,
                &window,
                self.proxy.clone(),
            );
            window.set_visible(true);
            let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
            let surface = softbuffer::Surface::new(&context, window.clone()).expect("surface");
            #[cfg(target_os = "macos")]
            let _menu = {
                let menu = muda::Menu::new();
                let file = muda::Submenu::new("File", true);
                file.append(&muda::MenuItem::new("Open…", true, None))
                    .expect("menu item");
                menu.append(&file).expect("submenu");
                menu.init_for_nsapp();
                menu
            };
            self.state = Some(State {
                window,
                surface,
                #[cfg(feature = "a11y")]
                adapter,
                scene: render_scene(),
                #[cfg(target_os = "macos")]
                _menu,
            });
        }

        fn user_event(&mut self, _: &ActiveEventLoop, event: UserEvent) {
            #[cfg(feature = "a11y")]
            {
                let Some(state) = &mut self.state else { return };
                if let accesskit_winit::WindowEvent::InitialTreeRequested = event.window_event {
                    state.adapter.update_if_active(tree);
                }
            }
            #[cfg(not(feature = "a11y"))]
            let () = event;
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
            let Some(state) = &mut self.state else { return };
            #[cfg(feature = "a11y")]
            state.adapter.process_event(&state.window, &event);
            match event {
                WindowEvent::CloseRequested => event_loop.exit(),
                WindowEvent::KeyboardInput { event, .. }
                    if event.state == ElementState::Pressed =>
                {
                    match event.logical_key {
                        Key::Character(c) if c.eq_ignore_ascii_case("o") => {
                            #[cfg(all(feature = "dialogs-ashpd", not(feature = "dialogs")))]
                            use rfd016 as rfd;
                            #[cfg(any(feature = "dialogs", feature = "dialogs-ashpd"))]
                            let picked = rfd::FileDialog::new()
                                .add_filter("Orchestra", &["orchestra"])
                                .set_parent(&*state.window)
                                .pick_file();
                            #[cfg(not(any(feature = "dialogs", feature = "dialogs-ashpd")))]
                            let picked: Option<std::path::PathBuf> = None;
                            println!("picked: {picked:?}");
                        }
                        Key::Named(NamedKey::Escape) => event_loop.exit(),
                        _ => {}
                    }
                }
                WindowEvent::RedrawRequested => {
                    let size = state.window.inner_size();
                    let (Some(w), Some(h)) =
                        (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                    else {
                        return;
                    };
                    state.surface.resize(w, h).expect("resize");
                    let mut buffer = state.surface.buffer_mut().expect("buffer");
                    buffer.fill(0x00fa_faf7);
                    // Copy the premultiplied RGBA pixmap into softbuffer's 0RGB format.
                    let sw = state.scene.width().min(size.width) as usize;
                    let sh = state.scene.height().min(size.height) as usize;
                    for y in 0..sh {
                        for x in 0..sw {
                            let p = state.scene.pixels()[y * state.scene.width() as usize + x];
                            buffer[y * size.width as usize + x] = (u32::from(p.red()) << 16)
                                | (u32::from(p.green()) << 8)
                                | u32::from(p.blue());
                        }
                    }
                    buffer.present().expect("present");
                }
                _ => {}
            }
        }
    }

    /// Opens the window and runs the event loop until it is closed.
    pub fn run() {
        let event_loop = EventLoop::<UserEvent>::with_user_event()
            .build()
            .expect("event loop");
        let mut app = App {
            proxy: event_loop.create_proxy(),
            state: None,
        };
        event_loop.run_app(&mut app).expect("event loop failed");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("headless") => headless(args.get(2).map_or("gui-blocks.png", String::as_str)),
        Some("window") => window::run(),
        Some("theme") => theme::run(),
        _ => eprintln!("usage: gui-blocks headless <out.png> | gui-blocks window"),
    }
}
