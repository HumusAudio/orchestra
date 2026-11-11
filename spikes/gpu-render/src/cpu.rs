// SPDX-License-Identifier: GPL-3.0-or-later

//! The two CPU backends: our own renderer (`orchestra-ui`'s `CpuRenderer` on `tiny-skia`, with
//! segments and damage exactly as the product uses them) and `vello_cpu` 0.3, single-threaded and
//! multi-threaded.

use orchestra_ui::{
    CpuRenderer, Damage, DisplayList, FillRule, Image, Point, Rect as URect, Renderer, RoundedRect,
    SegmentId, StrokeStyle, Transform,
};
use vello_cpu::kurbo::{
    Affine, BezPath, Rect as KRect, RoundedRect as KRoundedRect, Shape, Stroke,
};
use vello_cpu::{Pixmap, RenderContext, RenderSettings, Resources};

use crate::Backend;
use crate::scene::{HEIGHT, Op, Scene, View, WIDTH, ellipse_cubics};

// ------------------------------------------------------------------------------------------------
// orchestra-ui (tiny-skia)
// ------------------------------------------------------------------------------------------------

/// Our product renderer: the display list keeps one segment per measure, and an edit re-records
/// one segment and draws only the damage (ADR-0039). A full frame draws everything.
pub struct OrchestraUi {
    scene: Scene,
    list: DisplayList,
    renderer: CpuRenderer,
    image: Image,
    /// The view the list is currently recorded with: re-recording happens only when it changes.
    recorded_view: Option<(f64, f64)>,
}

impl OrchestraUi {
    pub fn new(scene: Scene) -> Self {
        Self {
            scene,
            list: DisplayList::new(),
            renderer: CpuRenderer::new(),
            image: Image::new(WIDTH, HEIGHT).expect("valid size"),
            recorded_view: None,
        }
    }

    /// Records every segment under `view` (a translation and a uniform scale, as the product's
    /// score view would push them).
    fn record_all(&mut self, view: View) {
        let transform = Transform {
            scale: view.zoom as f32,
            offset: Point::new(view.dx as f32, 0.0),
        };
        for (index, ops) in self.scene.segments.iter().enumerate() {
            record_segment(&mut self.list, index, ops, transform);
        }
        self.recorded_view = Some((view.dx, view.zoom));
    }
}

/// Records one segment: every op becomes the recorder call the product would use.
fn record_segment(list: &mut DisplayList, index: usize, ops: &[Op], transform: Transform) {
    list.record(SegmentId(index as u64), |r| {
        r.push_transform(transform);
        for op in ops {
            match *op {
                Op::Rect(rect, token) => r.fill_rect(urect(rect), token),
                Op::RoundRect(rect, radius, token) => {
                    r.fill_rounded_rect(RoundedRect::new(urect(rect), radius as f32), token);
                }
                Op::StrokeRoundRect(rect, radius, width, token) => r.stroke_rounded_rect(
                    RoundedRect::new(urect(rect), radius as f32),
                    StrokeStyle::new(width as f32),
                    token,
                ),
                Op::Ellipse {
                    cx,
                    cy,
                    a,
                    b,
                    color,
                } => {
                    let (start, cubics) = ellipse_cubics(cx, cy, a, b);
                    r.fill_path(FillRule::NonZero, color, |p| {
                        p.move_to(start.0 as f32, start.1 as f32);
                        for [c1, c2, end] in cubics {
                            p.cubic_to(
                                c1.0 as f32,
                                c1.1 as f32,
                                c2.0 as f32,
                                c2.1 as f32,
                                end.0 as f32,
                                end.1 as f32,
                            );
                        }
                        p.close();
                    });
                }
                Op::Shadow(rect, radius, sigma, token) => {
                    r.shadow(
                        RoundedRect::new(urect(rect), radius as f32),
                        sigma as f32,
                        token,
                    );
                }
            }
        }
        r.pop_transform();
    });
}

fn urect(r: crate::scene::Rect) -> URect {
    URect::new(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32)
}

impl Backend for OrchestraUi {
    fn name(&self) -> String {
        "tiny-skia (orchestra-ui CpuRenderer)".to_owned()
    }

    fn draw_full(&mut self, view: View) {
        // A scroll or zoom re-records the whole list (the product would do the same: every
        // segment's transform changes) and draws the whole frame.
        if self.recorded_view != Some((view.dx, view.zoom)) {
            self.record_all(view);
        }
        let _ = self.list.take_damage();
        self.renderer
            .render(
                &self.list,
                &self.scene.theme,
                &Damage::all(),
                1.0,
                &mut self.image,
            )
            .expect("render");
    }

    fn draw_edit(&mut self, lift: bool) {
        if self.recorded_view != Some((0.0, 1.0)) {
            self.record_all(View::IDENTITY);
            let _ = self.list.take_damage();
            self.renderer
                .render(
                    &self.list,
                    &self.scene.theme,
                    &Damage::all(),
                    1.0,
                    &mut self.image,
                )
                .expect("render");
        }
        // The steady state of editing: one segment re-recorded, only its damage drawn.
        let index = Scene::edited_index();
        let ops = if lift {
            &self.scene.edited_lifted
        } else {
            &self.scene.segments[index]
        };
        record_segment(&mut self.list, index, ops, Transform::IDENTITY);
        let damage = self.list.take_damage();
        self.renderer
            .render(&self.list, &self.scene.theme, &damage, 1.0, &mut self.image)
            .expect("render");
    }

    fn read_back(&mut self) -> Vec<u8> {
        self.image.data().to_vec()
    }

    fn cpu_pixels(&self) -> Option<&[u8]> {
        Some(self.image.data())
    }
}

// ------------------------------------------------------------------------------------------------
// vello_cpu
// ------------------------------------------------------------------------------------------------

/// `vello_cpu` 0.3: re-encodes the whole scene every frame (it has no retained segments), then
/// rasterises, on one thread or on several.
pub struct VelloCpu {
    scene: Scene,
    ctx: RenderContext,
    resources: Resources,
    pixmap: Pixmap,
    threads: u16,
}

impl VelloCpu {
    /// `threads == 0` is single-threaded; otherwise that many worker threads.
    pub fn new(scene: Scene, threads: u16) -> Self {
        let settings = RenderSettings {
            num_threads: threads,
            ..RenderSettings::default()
        };
        Self {
            scene,
            ctx: RenderContext::new_with(WIDTH as u16, HEIGHT as u16, settings),
            resources: Resources::new(),
            pixmap: Pixmap::new(WIDTH as u16, HEIGHT as u16),
            threads,
        }
    }

    /// The pixels of the last frame, premultiplied RGBA (for presenting in a window).
    pub fn pixels(&self) -> &[u8] {
        self.pixmap.data_as_u8_slice()
    }

    fn encode_and_render(&mut self, view: View, lift: bool) {
        self.ctx.reset();
        let root = Affine::translate((view.dx, 0.0)) * Affine::scale(view.zoom);
        // Background: the theme's surface, as our renderer clears to.
        self.ctx.set_transform(Affine::IDENTITY);
        self.ctx
            .set_paint(color(&self.scene, orchestra_ui::ColorToken::Surface));
        self.ctx
            .fill_rect(&KRect::new(0.0, 0.0, f64::from(WIDTH), f64::from(HEIGHT)));
        self.ctx.set_transform(root);
        let edited = Scene::edited_index();
        for (index, ops) in self.scene.segments.iter().enumerate() {
            let ops = if lift && index == edited {
                &self.scene.edited_lifted
            } else {
                ops
            };
            for op in ops {
                draw_op_cpu(&mut self.ctx, &self.scene, op);
            }
        }
        self.ctx.flush();
        self.ctx.render(&mut self.pixmap, &mut self.resources);
    }
}

fn color(
    scene: &Scene,
    token: orchestra_ui::ColorToken,
) -> vello_cpu::color::AlphaColor<vello_cpu::color::Srgb> {
    let [r, g, b, a] = scene.rgba(token);
    vello_cpu::color::AlphaColor::from_rgba8(r, g, b, a)
}

fn krect(r: crate::scene::Rect) -> KRect {
    KRect::new(r.x0, r.y0, r.x1, r.y1)
}

fn draw_op_cpu(ctx: &mut RenderContext, scene: &Scene, op: &Op) {
    match *op {
        Op::Rect(rect, token) => {
            ctx.set_paint(color(scene, token));
            ctx.fill_rect(&krect(rect));
        }
        Op::RoundRect(rect, radius, token) => {
            ctx.set_paint(color(scene, token));
            ctx.fill_path(&KRoundedRect::from_rect(krect(rect), radius).to_path(0.1));
        }
        Op::StrokeRoundRect(rect, radius, width, token) => {
            ctx.set_paint(color(scene, token));
            ctx.set_stroke(Stroke::new(width));
            ctx.stroke_path(&KRoundedRect::from_rect(krect(rect), radius).to_path(0.1));
        }
        Op::Ellipse {
            cx,
            cy,
            a,
            b,
            color: token,
        } => {
            ctx.set_paint(color(scene, token));
            ctx.fill_path(&ellipse_path(cx, cy, a, b));
        }
        Op::Shadow(rect, radius, sigma, token) => {
            ctx.set_paint(color(scene, token));
            ctx.fill_blurred_rounded_rect(&krect(rect), radius as f32, sigma as f32, false);
        }
    }
}

/// The ellipse of [`ellipse_cubics`] as a `kurbo` path of `vello_cpu`'s kurbo version.
fn ellipse_path(cx: f64, cy: f64, a: f64, b: f64) -> BezPath {
    let (start, cubics) = ellipse_cubics(cx, cy, a, b);
    let mut path = BezPath::new();
    path.move_to(start);
    for [c1, c2, end] in cubics {
        path.curve_to(c1, c2, end);
    }
    path.close_path();
    path
}

impl Backend for VelloCpu {
    fn name(&self) -> String {
        if self.threads == 0 {
            "vello_cpu (1 thread)".to_owned()
        } else {
            format!("vello_cpu ({} worker threads)", self.threads)
        }
    }

    fn draw_full(&mut self, view: View) {
        self.encode_and_render(view, false);
    }

    fn draw_edit(&mut self, lift: bool) {
        // No retained segments and no damage: an edit costs a full frame.
        self.encode_and_render(View::IDENTITY, lift);
    }

    fn read_back(&mut self) -> Vec<u8> {
        self.pixels().to_vec()
    }

    fn cpu_pixels(&self) -> Option<&[u8]> {
        Some(self.pixels())
    }
}
