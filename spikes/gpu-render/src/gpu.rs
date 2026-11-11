// SPDX-License-Identifier: GPL-3.0-or-later

//! `vello` 0.11 on `wgpu` 30: the whole scene is encoded on the CPU every frame (vello has no
//! retained segments), then rasterised by compute shaders on the GPU into an `Rgba8Unorm` storage
//! texture. Frame times include waiting for the GPU to finish (`poll` with wait), so they are the
//! latency of a frame, not just the cost of submitting it.

use std::time::{Duration, Instant};

use orchestra_ui::ColorToken;
use vello::kurbo::{Affine, BezPath, Rect as KRect, RoundedRect as KRoundedRect, Stroke};
use vello::peniko::{Color, Fill};
use vello::wgpu;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions};

use crate::Backend;
use crate::scene::{HEIGHT, Op, Scene, View, WIDTH, ellipse_cubics};

/// The GPU objects shared by the headless bench and the window.
pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    /// Opens the default high-performance adapter. `WGPU_BACKEND=vulkan|gl` selects a backend.
    pub fn open(surface: Option<&wgpu::Surface<'_>>, instance: &wgpu::Instance) -> Self {
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: surface,
            ..Default::default()
        }))
        .expect("no GPU adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gpu-render"),
            required_features: wgpu::Features::empty(),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .expect("no GPU device");
        Self {
            adapter,
            device,
            queue,
        }
    }

    /// One line describing the adapter, for the report.
    pub fn describe(&self) -> String {
        let info = self.adapter.get_info();
        format!(
            "{} ({:?}, driver {} {})",
            info.name, info.backend, info.driver, info.driver_info
        )
    }
}

/// The vello renderer plus its scene, drawing into an offscreen texture.
pub struct Vello {
    scene: Scene,
    pub gpu: Gpu,
    pub renderer: Renderer,
    pub vscene: vello::Scene,
    pub target: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl Vello {
    /// Opens the GPU (timed as "device") and builds the renderer, i.e. compiles vello's shader
    /// pipelines (timed as "renderer").
    pub fn headless(scene: Scene) -> (Self, Duration, Duration) {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        Self::build(scene, instance, None)
    }

    /// Builds on `instance`, with an adapter able to present to `surface` when there is one.
    pub fn build(
        scene: Scene,
        instance: wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> (Self, Duration, Duration) {
        let start = Instant::now();
        let gpu = Gpu::open(surface, &instance);
        let device_time = start.elapsed();
        eprintln!("  adapter: {}", gpu.describe());
        let start = Instant::now();
        let renderer = new_renderer(&gpu.device);
        let renderer_time = start.elapsed();
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        (
            Self {
                scene,
                gpu,
                renderer,
                vscene: vello::Scene::new(),
                target,
                view,
            },
            device_time,
            renderer_time,
        )
    }

    pub fn render(&mut self, view: View, lift: bool) {
        encode(&mut self.vscene, &self.scene, view, lift);
        self.renderer
            .render_to_texture(
                &self.gpu.device,
                &self.gpu.queue,
                &self.vscene,
                &self.view,
                &params(&self.scene, WIDTH, HEIGHT),
            )
            .expect("vello render");
        self.gpu
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
    }
}

/// The renderer with only the area antialiasing pipelines (the fastest, and the one we use):
/// compiling all three AA modes would inflate the start-up time we measure.
pub fn new_renderer(device: &wgpu::Device) -> Renderer {
    Renderer::new(
        device,
        RendererOptions {
            antialiasing_support: AaSupport::area_only(),
            ..RendererOptions::default()
        },
    )
    .expect("vello renderer")
}

/// Render parameters: background = the theme's surface, area antialiasing.
pub fn params(scene: &Scene, width: u32, height: u32) -> RenderParams {
    RenderParams {
        base_color: color(scene, ColorToken::Surface),
        width,
        height,
        antialiasing_method: AaConfig::Area,
    }
}

fn color(scene: &Scene, token: ColorToken) -> Color {
    let [r, g, b, a] = scene.rgba(token);
    Color::from_rgba8(r, g, b, a)
}

fn krect(r: crate::scene::Rect) -> KRect {
    KRect::new(r.x0, r.y0, r.x1, r.y1)
}

/// Encodes the whole scene into `vscene` (reset first) under `view`.
pub fn encode(vscene: &mut vello::Scene, scene: &Scene, view: View, lift: bool) {
    vscene.reset();
    let t = Affine::translate((view.dx, 0.0)) * Affine::scale(view.zoom);
    let edited = Scene::edited_index();
    for (index, ops) in scene.segments.iter().enumerate() {
        let ops = if lift && index == edited {
            &scene.edited_lifted
        } else {
            ops
        };
        for op in ops {
            match *op {
                Op::Rect(rect, token) => {
                    vscene.fill(Fill::NonZero, t, color(scene, token), None, &krect(rect));
                }
                Op::RoundRect(rect, radius, token) => vscene.fill(
                    Fill::NonZero,
                    t,
                    color(scene, token),
                    None,
                    &KRoundedRect::from_rect(krect(rect), radius),
                ),
                Op::StrokeRoundRect(rect, radius, width, token) => vscene.stroke(
                    &Stroke::new(width),
                    t,
                    color(scene, token),
                    None,
                    &KRoundedRect::from_rect(krect(rect), radius),
                ),
                Op::Ellipse {
                    cx,
                    cy,
                    a,
                    b,
                    color: token,
                } => {
                    let (start, cubics) = ellipse_cubics(cx, cy, a, b);
                    let mut path = BezPath::new();
                    path.move_to(start);
                    for [c1, c2, end] in cubics {
                        path.curve_to(c1, c2, end);
                    }
                    path.close_path();
                    vscene.fill(Fill::NonZero, t, color(scene, token), None, &path);
                }
                Op::Shadow(rect, radius, sigma, token) => {
                    vscene.draw_blurred_rounded_rect(
                        t,
                        krect(rect),
                        color(scene, token),
                        radius,
                        sigma,
                    );
                }
            }
        }
    }
}

impl Backend for Vello {
    fn name(&self) -> String {
        format!("vello 0.11 (wgpu 30, GPU compute) on {}", self.gpu.describe())
    }

    fn draw_full(&mut self, view: View) {
        self.render(view, false);
    }

    fn draw_edit(&mut self, lift: bool) {
        // No damage on the GPU path: an edit is a full frame.
        self.render(View::IDENTITY, lift);
    }

    fn read_back(&mut self) -> Vec<u8> {
        read_texture(&self.gpu.device, &self.gpu.queue, &self.target)
    }
}

/// Copies an `Rgba8Unorm` texture of the frame size back to memory, tightly packed.
fn read_texture(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let bytes_per_row = (WIDTH * 4).next_multiple_of(256);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(bytes_per_row) * u64::from(HEIGHT),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
    let mapped = buffer.slice(..).get_mapped_range().expect("mapped range");
    let mut out = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for row in mapped.chunks_exact(bytes_per_row as usize) {
        out.extend_from_slice(&row[..(WIDTH * 4) as usize]);
    }
    out
}
