//! Rendering into an image instead of a window.

use super::painter::{FrameInput, Painter, extent};
use super::{RenderError, describe, request_device};
use crate::{CameraView, DebugShapes, Image, Sprites};
use cg_assets::Assets;
use cg_core::glam::UVec2;
use std::sync::{Arc, Mutex, PoisonError};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Renders frames into an [`Image`] with no window, through the same code path as the
/// window. The engine's reference-image tests use it; it also suits screenshots and tools.
///
/// Reading the image back waits for the GPU, so [`render`](Self::render) works on desktop
/// only. Without a GPU, [`new`](Self::new) picks a software adapter if the system has one
/// (WARP on Windows, lavapipe or llvmpipe on Linux).
///
/// ```no_run
/// use cg_assets::Assets;
/// use cg_core::prelude::*;
/// use cg_render::{CameraView, Color, DebugShapes, OffscreenRenderer, Sprite, Sprites};
///
/// let mut renderer = pollster::block_on(OffscreenRenderer::new())?;
/// let mut sprites = Sprites::default();
/// sprites.draw(&Sprite::solid(Color::RED, vec2(8.0, 8.0)), Affine2::IDENTITY);
/// let image = renderer.render(
///     uvec2(64, 36),
///     &CameraView::default(),
///     &Assets::default(),
///     &sprites,
///     &DebugShapes::default(),
/// )?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    painter: Painter,
    adapter: String,
    backend: String,
}

impl OffscreenRenderer {
    /// Sets up a GPU device. `WGPU_ADAPTER_NAME`, `WGPU_BACKEND` and `WGPU_POWER_PREF`
    /// choose the adapter.
    pub async fn new() -> Result<Self, RenderError> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = wgpu::util::initialize_adapter_from_env_or_default(&instance, None)
            .await
            .map_err(|err| RenderError::NoAdapter(err.to_string()))?;
        let (device, queue) = request_device(&adapter).await?;
        let (adapter, backend) = describe(&adapter.get_info());
        Ok(Self {
            painter: Painter::new(&device, FORMAT),
            device,
            queue,
            adapter,
            backend,
        })
    }

    /// The GPU's name.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// The graphics API in use.
    pub fn backend(&self) -> &str {
        &self.backend
    }

    /// Draws one frame into an image of `size` pixels (scale factor 1) and reads it back.
    pub fn render(
        &mut self,
        size: UVec2,
        view: &CameraView,
        images: &Assets<Image>,
        sprites: &Sprites,
        shapes: &DebugShapes,
    ) -> Result<Image, RenderError> {
        let size = size.max(UVec2::ONE);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cg_render offscreen target"),
            size: extent(size),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let target = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.painter
            .sync_textures(&self.device, &self.queue, images);
        let input = FrameInput {
            view,
            images,
            sprites: sprites.queued(),
            screen_sprites: sprites.screen_queued(),
            shapes,
            scale_factor: 1.0,
        };
        self.painter
            .render(&self.device, &self.queue, &target, size, &input, None);
        self.read_back(&texture, size)
    }

    fn read_back(&self, texture: &wgpu::Texture, size: UVec2) -> Result<Image, RenderError> {
        let row_bytes = size.x * 4;
        let padded_row_bytes = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cg_render readback"),
            size: u64::from(padded_row_bytes) * u64::from(size.y),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cg_render readback"),
            });
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
                    bytes_per_row: Some(padded_row_bytes),
                    rows_per_image: Some(size.y),
                },
            },
            extent(size),
        );
        self.queue.submit([encoder.finish()]);

        let mapped = Arc::new(Mutex::new(None));
        let sender = mapped.clone();
        buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            *sender.lock().unwrap_or_else(PoisonError::into_inner) = Some(result);
        });
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .map_err(|err| RenderError::Readback(err.to_string()))?;
        match mapped.lock().unwrap_or_else(PoisonError::into_inner).take() {
            Some(Ok(())) => {}
            Some(Err(err)) => return Err(RenderError::Readback(err.to_string())),
            None => {
                return Err(RenderError::Readback(
                    "the GPU did not finish; offscreen rendering needs a desktop GPU".into(),
                ));
            }
        }

        let mut data = Vec::with_capacity((row_bytes * size.y) as usize);
        {
            let view = buffer
                .get_mapped_range(..)
                .map_err(|err| RenderError::Readback(err.to_string()))?;
            for row in view.chunks(padded_row_bytes as usize) {
                data.extend_from_slice(&row[..row_bytes as usize]);
            }
        }
        buffer.unmap();
        Image::new(size.x, size.y, data).map_err(|err| RenderError::Readback(err.to_string()))
    }
}
