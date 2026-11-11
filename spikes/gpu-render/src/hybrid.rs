// SPDX-License-Identifier: GPL-3.0-or-later

//! `vello_hybrid` 0.2 on `wgpu` 29: every frame the CPU turns paths into sparse strips (no retained
//! segments), then the GPU fills and composites them with ordinary render passes. No compute
//! shaders, so it can also run where `vello` cannot (GL-only drivers). Frame times include waiting
//! for the GPU (`poll` with wait).

use std::time::{Duration, Instant};

use orchestra_ui::ColorToken;
use vello_common::color::{AlphaColor, Srgb};
use vello_common::kurbo::{
    Affine, BezPath, Rect as KRect, RoundedRect as KRoundedRect, Shape, Stroke,
};
use vello_hybrid::{RenderSize, RenderTargetConfig, Renderer, Resources, TextureBindings};
use wgpu29 as wgpu;

use crate::Backend;
use crate::scene::{Op, Scene, View, ellipse_cubics, height, width};

/// The GPU objects shared by the headless bench and the window.
pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    /// Opens the default high-performance adapter. `WGPU_BACKEND=vulkan|gl` selects a backend.
    pub fn open(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Self {
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

/// The hybrid renderer plus its scene, drawing into an offscreen texture.
pub struct Hybrid {
    scene: Scene,
    pub gpu: Gpu,
    pub renderer: Renderer,
    pub resources: Resources,
    pub hscene: vello_hybrid::Scene,
    pub target: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl Hybrid {
    /// Opens the GPU ("device") and builds the renderer's pipelines ("renderer").
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
        let gpu = Gpu::open(&instance, surface);
        let device_time = start.elapsed();
        let start = Instant::now();
        let (renderer, resources) = Renderer::new(
            &gpu.device,
            &RenderTargetConfig {
                format: wgpu::TextureFormat::Rgba8Unorm,
                width: width(),
                height: height(),
            },
        );
        let renderer_time = start.elapsed();
        let target = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("target"),
            size: wgpu::Extent3d {
                width: width(),
                height: height(),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC
                // TEXTURE_BINDING: the window mode samples this texture to blit it onto the
                // surface (found by the owner: the blitter rejects a texture without it).
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let hscene = vello_hybrid::Scene::new(width() as u16, height() as u16);
        (
            Self {
                scene,
                gpu,
                renderer,
                resources,
                hscene,
                target,
                view,
            },
            device_time,
            renderer_time,
        )
    }

    pub fn render(&mut self, view: View, lift: bool) {
        encode(&mut self.hscene, &self.scene, view, lift);
        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.renderer
            .render(
                &self.hscene,
                &mut self.resources,
                &self.gpu.device,
                &self.gpu.queue,
                &mut encoder,
                &RenderSize {
                    width: width(),
                    height: height(),
                },
                &self.view,
                &TextureBindings::new(),
            )
            .expect("hybrid render");
        self.gpu.queue.submit([encoder.finish()]);
        self.gpu
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
    }
}

fn color(scene: &Scene, token: ColorToken) -> AlphaColor<Srgb> {
    let [r, g, b, a] = scene.rgba(token);
    AlphaColor::from_rgba8(r, g, b, a)
}

fn krect(r: crate::scene::Rect) -> KRect {
    KRect::new(r.x0, r.y0, r.x1, r.y1)
}

/// Encodes the whole scene (background first: the hybrid renderer clears to transparent).
pub fn encode(h: &mut vello_hybrid::Scene, scene: &Scene, view: View, lift: bool) {
    h.reset();
    h.set_transform(Affine::IDENTITY);
    h.set_paint(color(scene, ColorToken::Surface));
    h.fill_rect(&KRect::new(0.0, 0.0, f64::from(width()), f64::from(height())));
    h.set_transform(Affine::scale(f64::from(crate::scene::scale())) * Affine::translate((view.dx, 0.0)) * Affine::scale(view.zoom));
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
                    h.set_paint(color(scene, token));
                    h.fill_rect(&krect(rect));
                }
                Op::RoundRect(rect, radius, token) => {
                    h.set_paint(color(scene, token));
                    h.fill_path(&KRoundedRect::from_rect(krect(rect), radius).to_path(0.1));
                }
                Op::StrokeRoundRect(rect, radius, width, token) => {
                    h.set_paint(color(scene, token));
                    h.set_stroke(Stroke::new(width));
                    h.stroke_path(&KRoundedRect::from_rect(krect(rect), radius).to_path(0.1));
                }
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
                    h.set_paint(color(scene, token));
                    h.fill_path(&path);
                }
                Op::Shadow(rect, radius, sigma, token) => {
                    h.set_paint(color(scene, token));
                    h.fill_blurred_rounded_rect(&krect(rect), radius as f32, sigma as f32, false);
                }
            }
        }
    }
}

impl Backend for Hybrid {
    fn name(&self) -> String {
        format!("vello_hybrid 0.2 (wgpu 29) on {}", self.gpu.describe())
    }

    fn draw_full(&mut self, view: View) {
        self.render(view, false);
    }

    fn draw_edit(&mut self, lift: bool) {
        self.render(View::IDENTITY, lift);
    }

    fn read_back(&mut self) -> Vec<u8> {
        let bytes_per_row = (width() * 4).next_multiple_of(256);
        let device = &self.gpu.device;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(bytes_per_row) * u64::from(height()),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.target,
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
                width: width(),
                height: height(),
                depth_or_array_layers: 1,
            },
        );
        self.gpu.queue.submit([encoder.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.expect("map"));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let mapped = buffer.slice(..).get_mapped_range();
        let mut out = Vec::with_capacity((width() * height() * 4) as usize);
        for row in mapped.chunks_exact(bytes_per_row as usize) {
            out.extend_from_slice(&row[..(width() * 4) as usize]);
        }
        out
    }
}
