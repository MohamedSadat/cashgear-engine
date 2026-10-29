//! The painter: pipelines, buffers and GPU textures, and the passes that draw one frame
//! into a target. Both the window surface and the offscreen renderer use it.

use crate::batch::{Batcher, SpriteInstance};
use crate::debug::{ShapeVertex, tessellate};
use crate::overlay::{OverlayFrame, RenderOverlays};
use crate::{CameraView, Color, DebugShapes, Filter, Image, QueuedSprite, ScalingMode};
use bytemuck::{Pod, Zeroable};
use cg_assets::{AssetId, Assets};
use cg_core::glam::{Mat4, UVec2, Vec2, Vec3};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Counts painters, so overlays can tell a new GPU device from the old one.
static PAINTERS: AtomicU64 = AtomicU64::new(0);

/// The format of every texture the engine samples: 8-bit sRGB with alpha.
const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Everything a frame draws.
pub(crate) struct FrameInput<'a> {
    pub(crate) view: &'a CameraView,
    pub(crate) images: &'a Assets<Image>,
    pub(crate) sprites: &'a [QueuedSprite],
    /// Screen-space sprites, drawn after the world at the target's resolution.
    pub(crate) screen_sprites: &'a [QueuedSprite],
    pub(crate) shapes: &'a DebugShapes,
    /// Physical pixels per logical pixel.
    pub(crate) scale_factor: f32,
}

/// What one frame drew.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameStats {
    pub(crate) draw_calls: u32,
    pub(crate) sprites: u32,
    pub(crate) batches: u32,
    pub(crate) shapes: u32,
}

/// The view uniform shared by the sprite and shape shaders (`View` in the WGSL).
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct ViewUniform {
    view_proj: [[f32; 4]; 4],
    target_size: [f32; 2],
    snap: f32,
    _padding: f32,
}

impl ViewUniform {
    fn new(view_proj: Mat4, target_size: UVec2, snap: bool) -> Self {
        Self {
            view_proj: view_proj.to_cols_array_2d(),
            target_size: target_size.as_vec2().max(Vec2::ONE).to_array(),
            snap: if snap { 1.0 } else { 0.0 },
            _padding: 0.0,
        }
    }
}

/// The views a frame draws with, one slot each in [`Views`].
#[derive(Debug, Clone, Copy)]
enum View {
    /// The camera, with pixel snapping.
    Sprites = 0,
    /// The camera, without snapping.
    Shapes = 1,
    /// Identity: the letterbox background fills the viewport.
    Fill = 2,
    /// Screen space: UI pixels over the whole target, with pixel snapping.
    Screen = 3,
}

const VIEW_COUNT: u64 = 4;

/// One uniform buffer holding every [`View`], bound with a dynamic offset. A single
/// buffer and bind group is also the layout that every backend handles alike; separate
/// uniform bind groups read the wrong buffer on Direct3D 12's WARP software adapter.
struct Views {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Bytes between slots: the uniform offset alignment.
    stride: u64,
}

impl Views {
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout) -> Self {
        let size = size_of::<ViewUniform>() as u64;
        let stride = size.next_multiple_of(u64::from(
            device.limits().min_uniform_buffer_offset_alignment,
        ));
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cg_render views"),
            size: stride * VIEW_COUNT,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cg_render views"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size),
                }),
            }],
        });
        Self {
            buffer,
            bind_group,
            stride,
        }
    }

    fn write(&self, queue: &wgpu::Queue, view: View, value: ViewUniform) {
        queue.write_buffer(&self.buffer, self.offset(view), bytemuck::bytes_of(&value));
    }

    fn offset(&self, view: View) -> u64 {
        view as u64 * self.stride
    }

    fn bind(&self, pass: &mut wgpu::RenderPass<'_>, view: View) {
        pass.set_bind_group(0, &self.bind_group, &[self.offset(view) as u32]);
    }
}

/// A vertex buffer that grows to fit each frame's data.
struct GrowBuffer {
    label: &'static str,
    buffer: Option<wgpu::Buffer>,
}

impl GrowBuffer {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            buffer: None,
        }
    }

    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let needed = data.len() as u64;
        if self.buffer.as_ref().is_none_or(|b| b.size() < needed) {
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: needed.next_power_of_two().max(16 * 1024),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        if let Some(buffer) = &self.buffer {
            queue.write_buffer(buffer, 0, data);
        }
    }

    fn slice(&self, bytes: core::ops::Range<u64>) -> Option<wgpu::BufferSlice<'_>> {
        self.buffer.as_ref().map(|buffer| buffer.slice(bytes))
    }
}

/// A texture on the GPU. `bind_group` is `None` when it could not be uploaded.
struct GpuTexture {
    version: u64,
    bind_group: Option<wgpu::BindGroup>,
}

/// The image a pixel-perfect camera draws into before it is scaled up.
struct PixelTarget {
    size: UVec2,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

pub(crate) struct Painter {
    format: wgpu::TextureFormat,
    max_texture_size: u32,
    texture_layout: wgpu::BindGroupLayout,
    sprite_pipeline: wgpu::RenderPipeline,
    shape_pipeline: wgpu::RenderPipeline,
    blit_pipeline: wgpu::RenderPipeline,
    nearest: wgpu::Sampler,
    linear: wgpu::Sampler,
    views: Views,
    instances: GrowBuffer,
    vertices: GrowBuffer,
    textures: HashMap<AssetId<Image>, GpuTexture>,
    texture_revision: Option<u64>,
    pixel_target: Option<PixelTarget>,
    batcher: Batcher,
    shape_vertices: Vec<ShapeVertex>,
    generation: u64,
}

impl Painter {
    /// Creates the pipelines for render targets viewed as `format`, which should be an sRGB
    /// format so blending happens in linear light.
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cg_render view"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(size_of::<ViewUniform>() as u64),
                },
                count: None,
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cg_render texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let sprite_shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/sprite.wgsl"));
        let shape_shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/shape.wgsl"));
        let blit_shader = device.create_shader_module(wgpu::include_wgsl!("../shaders/blit.wgsl"));

        let instance_attributes = wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4, 4 => Float32x4,
            5 => Unorm8x4
        ];
        let sprite_pipeline = pipeline(
            device,
            "cg_render sprites",
            &[Some(&view_layout), Some(&texture_layout)],
            &sprite_shader,
            &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<SpriteInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &instance_attributes,
            })],
            format,
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        let shape_attributes = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
        let shape_pipeline = pipeline(
            device,
            "cg_render shapes",
            &[Some(&view_layout)],
            &shape_shader,
            &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<ShapeVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &shape_attributes,
            })],
            format,
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        let blit_pipeline = pipeline(
            device,
            "cg_render pixel-perfect upscale",
            &[Some(&texture_layout)],
            &blit_shader,
            &[],
            format,
            None,
        );

        let sampler = |filter: wgpu::FilterMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("cg_render sampler"),
                mag_filter: filter,
                min_filter: filter,
                ..Default::default()
            })
        };

        Self {
            format,
            max_texture_size: device.limits().max_texture_dimension_2d,
            sprite_pipeline,
            shape_pipeline,
            blit_pipeline,
            nearest: sampler(wgpu::FilterMode::Nearest),
            linear: sampler(wgpu::FilterMode::Linear),
            views: Views::new(device, &view_layout),
            texture_layout,
            instances: GrowBuffer::new("cg_render sprite instances"),
            vertices: GrowBuffer::new("cg_render shape vertices"),
            textures: HashMap::new(),
            texture_revision: None,
            pixel_target: None,
            batcher: Batcher::default(),
            shape_vertices: Vec::new(),
            generation: PAINTERS.fetch_add(1, Ordering::Relaxed) + 1,
        }
    }

    /// Uploads new and changed images and frees removed ones. [`AssetId::DEFAULT`] is a
    /// built-in white texture unless an image was stored there.
    pub(crate) fn sync_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        images: &Assets<Image>,
    ) {
        if self.texture_revision == Some(images.revision()) {
            return;
        }
        self.textures.retain(|id, _| images.contains(*id));
        for (id, image, version) in images.iter_versions() {
            if self
                .textures
                .get(&id)
                .is_some_and(|gpu| gpu.version == version)
            {
                continue;
            }
            let texture = self.upload(device, queue, id, image, version);
            self.textures.insert(id, texture);
        }
        if !self.textures.contains_key(&AssetId::DEFAULT) {
            let white = Image::filled(1, 1, Color::WHITE);
            let texture = self.upload(device, queue, AssetId::DEFAULT, &white, 0);
            self.textures.insert(AssetId::DEFAULT, texture);
        }
        self.texture_revision = Some(images.revision());
    }

    fn upload(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: AssetId<Image>,
        image: &Image,
        version: u64,
    ) -> GpuTexture {
        let size = image.size();
        let bind_group = if size.max_element() > self.max_texture_size {
            log::error!(
                "texture {} is {}x{}, larger than this GPU's limit of {}; it will not be drawn",
                id.index(),
                size.x,
                size.y,
                self.max_texture_size
            );
            None
        } else {
            let view = upload_texture(device, queue, image);
            let sampler = match image.filter() {
                Filter::Nearest => &self.nearest,
                Filter::Linear => &self.linear,
            };
            Some(texture_bind_group(
                device,
                &self.texture_layout,
                &view,
                sampler,
            ))
        };
        GpuTexture {
            version,
            bind_group,
        }
    }

    /// Draws a frame into `target` (viewed as this painter's format), then the overlays,
    /// and submits it.
    pub(crate) fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &wgpu::TextureView,
        target_size: UVec2,
        input: &FrameInput<'_>,
        overlays: Option<&mut RenderOverlays>,
    ) -> FrameStats {
        let camera = &input.view.camera;
        let viewport = input.view.viewport(target_size, input.scale_factor);
        let rect = viewport.rect;
        let has_view = !rect.is_empty();
        let pixel_perfect = matches!(camera.scaling, ScalingMode::PixelPerfect(_));
        let letterbox = matches!(camera.scaling, ScalingMode::Letterbox(_));
        let view_proj = input.view.view_projection(viewport.world_size);

        // Build this frame's geometry.
        self.batcher
            .build(input.sprites, input.screen_sprites, input.images);
        self.shape_vertices.clear();
        if letterbox {
            // The letterbox background: a quad covering the viewport, under everything.
            let fill = [
                (-1.0, -1.0),
                (1.0, -1.0),
                (1.0, 1.0),
                (-1.0, -1.0),
                (1.0, 1.0),
                (-1.0, 1.0),
            ];
            let color = camera.clear_color.to_array();
            self.shape_vertices.extend(fill.map(|(x, y)| ShapeVertex {
                position: [x, y],
                color,
            }));
        }
        let fill_vertices = self.shape_vertices.len() as u32;
        let world_per_pixel = viewport.world_size.x / rect.width().max(1.0);
        tessellate(
            input.shapes.shapes(),
            world_per_pixel,
            input.scale_factor,
            &mut self.shape_vertices,
        );
        let shape_vertices = self.shape_vertices.len() as u32;

        // Upload it.
        let size = viewport.render_size;
        let views = [
            (
                View::Sprites,
                ViewUniform::new(view_proj, size, camera.pixel_snap),
            ),
            (View::Shapes, ViewUniform::new(view_proj, size, false)),
            (View::Fill, ViewUniform::new(Mat4::IDENTITY, size, false)),
            (
                View::Screen,
                ViewUniform::new(screen_projection(input, target_size), target_size, true),
            ),
        ];
        for (view, value) in views {
            self.views.write(queue, view, value);
        }
        self.instances
            .write(device, queue, bytemuck::cast_slice(&self.batcher.instances));
        self.vertices
            .write(device, queue, bytemuck::cast_slice(&self.shape_vertices));
        if pixel_perfect && has_view {
            self.ensure_pixel_target(device, viewport.render_size);
        }

        let mut stats = FrameStats {
            sprites: self.batcher.instances.len() as u32,
            batches: self.batcher.batches.len() as u32,
            shapes: input.shapes.len() as u32,
            draw_calls: 0,
        };
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("cg_render frame"),
        });

        if pixel_perfect
            && has_view
            && let Some(pixel_target) = &self.pixel_target
        {
            let mut pass = begin_pass(&mut encoder, &pixel_target.view, camera.clear_color);
            stats.draw_calls += self.draw_sprites(&mut pass, View::Sprites);
        }

        {
            let clear = if pixel_perfect || letterbox {
                camera.bar_color
            } else {
                camera.clear_color
            };
            let mut pass = begin_pass(&mut encoder, target, clear);
            if has_view {
                pass.set_viewport(
                    rect.min.x,
                    rect.min.y,
                    rect.width(),
                    rect.height(),
                    0.0,
                    1.0,
                );
                if fill_vertices > 0 {
                    stats.draw_calls += self.draw_shapes(&mut pass, View::Fill, 0..fill_vertices);
                }
                if pixel_perfect {
                    if let Some(pixel_target) = &self.pixel_target {
                        pass.set_pipeline(&self.blit_pipeline);
                        pass.set_bind_group(0, &pixel_target.bind_group, &[]);
                        pass.draw(0..3, 0..1);
                        stats.draw_calls += 1;
                    }
                } else {
                    stats.draw_calls += self.draw_sprites(&mut pass, View::Sprites);
                }
                if self.batcher.screen_start < self.batcher.batches.len() {
                    let full = target_size.as_vec2();
                    pass.set_viewport(0.0, 0.0, full.x, full.y, 0.0, 1.0);
                    stats.draw_calls += self.draw_sprites(&mut pass, View::Screen);
                    pass.set_viewport(
                        rect.min.x,
                        rect.min.y,
                        rect.width(),
                        rect.height(),
                        0.0,
                        1.0,
                    );
                }
                if shape_vertices > fill_vertices {
                    stats.draw_calls +=
                        self.draw_shapes(&mut pass, View::Shapes, fill_vertices..shape_vertices);
                }
            }
        }

        if let Some(overlays) = overlays {
            for overlay in overlays.iter_mut() {
                overlay.draw(&mut OverlayFrame {
                    device,
                    queue,
                    encoder: &mut encoder,
                    target,
                    format: self.format,
                    size: target_size,
                    scale_factor: input.scale_factor,
                    device_generation: self.generation,
                });
            }
        }

        queue.submit([encoder.finish()]);
        stats
    }

    /// Draws the world sprite batches (`View::Sprites`) or the screen-space ones
    /// (`View::Screen`); returns the number of draw calls.
    fn draw_sprites(&self, pass: &mut wgpu::RenderPass<'_>, view: View) -> u32 {
        let batches = match view {
            View::Screen => &self.batcher.batches[self.batcher.screen_start..],
            _ => &self.batcher.batches[..self.batcher.screen_start],
        };
        if batches.is_empty() {
            return 0;
        }
        pass.set_pipeline(&self.sprite_pipeline);
        self.views.bind(pass, view);
        let stride = size_of::<SpriteInstance>() as u64;
        let mut draws = 0;
        for batch in batches {
            let Some(bind_group) = self
                .textures
                .get(&batch.texture)
                .and_then(|gpu| gpu.bind_group.as_ref())
            else {
                continue;
            };
            let range = batch.instances.clone();
            let Some(slice) = self
                .instances
                .slice(u64::from(range.start) * stride..u64::from(range.end) * stride)
            else {
                continue;
            };
            pass.set_bind_group(1, bind_group, &[]);
            // Each batch binds its own slice of the instance buffer instead of using a first
            // instance, which WebGL2 does not support.
            pass.set_vertex_buffer(0, slice);
            pass.draw(0..6, 0..range.len() as u32);
            draws += 1;
        }
        draws
    }

    /// Draws a range of shape vertices; returns the number of draw calls.
    fn draw_shapes(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        view: View,
        vertices: core::ops::Range<u32>,
    ) -> u32 {
        let stride = size_of::<ShapeVertex>() as u64;
        let Some(slice) = self
            .vertices
            .slice(u64::from(vertices.start) * stride..u64::from(vertices.end) * stride)
        else {
            return 0;
        };
        pass.set_pipeline(&self.shape_pipeline);
        self.views.bind(pass, view);
        pass.set_vertex_buffer(0, slice);
        pass.draw(0..vertices.len() as u32, 0..1);
        1
    }

    /// Makes sure the pixel-perfect image exists at `size`.
    fn ensure_pixel_target(&mut self, device: &wgpu::Device, size: UVec2) {
        let size = size.min(UVec2::splat(self.max_texture_size));
        if self.pixel_target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cg_render pixel-perfect image"),
            size: extent(size),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = texture_bind_group(device, &self.texture_layout, &view, &self.nearest);
        self.pixel_target = Some(PixelTarget {
            size,
            view,
            bind_group,
        });
    }
}

/// The matrix from screen space (UI pixels, y down) to clip space over the whole target.
fn screen_projection(input: &FrameInput<'_>, target_size: UVec2) -> Mat4 {
    let space = input
        .view
        .camera
        .screen_space(target_size, input.scale_factor);
    let target = target_size.as_vec2().max(Vec2::ONE);
    let scale = 2.0 * space.scale / target;
    let offset = 2.0 * space.origin / target - 1.0;
    Mat4::from_translation(Vec3::new(offset.x, -offset.y, 0.0))
        * Mat4::from_scale(Vec3::new(scale.x, -scale.y, 1.0))
}

fn pipeline(
    device: &wgpu::Device,
    label: &str,
    bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
    shader: &wgpu::ShaderModule,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    format: wgpu::TextureFormat,
    blend: Option<wgpu::BlendState>,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers,
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn begin_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    clear: Color,
) -> wgpu::RenderPass<'e> {
    // Clear values for sRGB targets are given in linear light, like shader output.
    let [r, g, b, a] = clear.to_linear().map(f64::from);
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("cg_render pass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    })
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    image: &crate::Image,
) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("cg_render texture"),
        size: extent(image.size()),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        image.data(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(image.width() * 4),
            rows_per_image: Some(image.height()),
        },
        extent(image.size()),
    );
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

fn texture_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("cg_render texture"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

pub(crate) fn extent(size: UVec2) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: size.x.max(1),
        height: size.y.max(1),
        depth_or_array_layers: 1,
    }
}
