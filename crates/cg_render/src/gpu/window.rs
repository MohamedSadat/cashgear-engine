//! Drawing to the window: GPU setup (async on the web), the surface and its lifecycle.

use super::painter::{FrameInput, FrameStats, Painter};
use super::{RenderError, describe, request_device};
use crate::overlay::RenderOverlays;
use crate::{DebugShapes, FrameView, Image, RenderPlugin, RenderStats, Sprites};
use bevy_ecs::prelude::*;
use cg_assets::Assets;
use cg_core::glam::UVec2;
use cg_core::{AppExit, Time};
use cg_platform::{Lifecycle, Window, WindowHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

type InitSlot = Arc<Mutex<Option<Result<Gpu, RenderError>>>>;

/// The renderer's GPU state for the window.
#[derive(Resource)]
pub(crate) struct WindowRenderer {
    settings: RenderPlugin,
    state: State,
    suspended: bool,
}

enum State {
    /// No GPU yet; setup starts once the window exists.
    Idle,
    /// The adapter and device are being requested (this takes a few frames on the web).
    Pending(InitSlot),
    Ready(Box<Gpu>),
    /// Setup failed; nothing is drawn.
    Failed,
}

struct Gpu {
    instance: wgpu::Instance,
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: Option<wgpu::Surface<'static>>,
    config: wgpu::SurfaceConfiguration,
    /// The sRGB view of the surface format that frames are drawn through.
    view_format: wgpu::TextureFormat,
    painter: Painter,
    /// Set by wgpu when the device is lost (driver reset, GPU removed).
    lost: Arc<AtomicBool>,
    info: wgpu::AdapterInfo,
}

impl WindowRenderer {
    pub(crate) fn new(settings: RenderPlugin) -> Self {
        Self {
            settings,
            state: State::Idle,
            suspended: false,
        }
    }

    /// Advances setup. Returns the GPU once it is ready.
    fn gpu(&mut self, handle: &WindowHandle) -> Result<Option<&mut Gpu>, RenderError> {
        if let State::Ready(gpu) = &self.state
            && gpu.lost.load(Ordering::Relaxed)
        {
            log::warn!("the GPU device was lost; setting it up again");
            self.state = State::Idle;
        }
        if matches!(self.state, State::Idle) {
            let slot = InitSlot::default();
            let sender = slot.clone();
            let handle = handle.clone();
            let settings = self.settings.clone();
            // Runs to completion here on desktop; on the web it finishes on a later frame.
            cg_platform::tasks::spawn_local(async move {
                let result = init(handle, settings).await;
                *sender.lock().unwrap_or_else(PoisonError::into_inner) = Some(result);
            });
            self.state = State::Pending(slot);
        }
        if let State::Pending(slot) = &self.state {
            let result = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
            match result {
                Some(Ok(gpu)) => {
                    let (adapter, backend) = describe(&gpu.info);
                    log::info!(
                        "renderer ready: {adapter} ({backend}), surface format {:?}",
                        gpu.view_format
                    );
                    self.state = State::Ready(Box::new(gpu));
                }
                Some(Err(err)) => {
                    self.state = State::Failed;
                    return Err(err);
                }
                None => {}
            }
        }
        Ok(match &mut self.state {
            State::Ready(gpu) => Some(gpu),
            _ => None,
        })
    }

    fn suspend(&mut self) {
        self.suspended = true;
        // The native window goes away on Android; the surface must go with it.
        if let State::Ready(gpu) = &mut self.state {
            gpu.surface = None;
        }
    }
}

impl Gpu {
    /// Draws one frame. Returns `None` when nothing could be drawn this frame.
    fn frame(
        &mut self,
        handle: &WindowHandle,
        size: UVec2,
        input: &FrameInput<'_>,
        overlays: Option<&mut RenderOverlays>,
    ) -> Result<Option<FrameStats>, RenderError> {
        self.painter
            .sync_textures(&self.device, &self.queue, input.images);
        if size.x == 0 || size.y == 0 {
            return Ok(None);
        }
        if self.surface.is_none() {
            let surface = self
                .instance
                .create_surface(handle.winit().clone())
                .map_err(|err| RenderError::Surface(err.to_string()))?;
            self.surface = Some(surface);
            self.configure(size);
        } else if (self.config.width, self.config.height) != (size.x, size.y) {
            self.configure(size);
        }
        let Some(surface) = &self.surface else {
            return Ok(None);
        };

        let mut reconfigure = false;
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                reconfigure = true;
                frame
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.configure(size);
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                log::warn!("the window surface was lost; recreating it");
                self.surface = None;
                return Ok(None);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("the window surface reported a validation error; skipping a frame");
                return Ok(None);
            }
        };
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.view_format),
            ..Default::default()
        });
        let stats = self
            .painter
            .render(&self.device, &self.queue, &view, size, input, overlays);
        handle.winit().pre_present_notify();
        self.queue.present(frame);
        if reconfigure {
            self.configure(size);
        }
        Ok(Some(stats))
    }

    fn configure(&mut self, size: UVec2) {
        self.config.width = size.x;
        self.config.height = size.y;
        if let Some(surface) = &self.surface {
            surface.configure(&self.device, &self.config);
        }
    }
}

/// Creates the instance, surface, adapter and device for the window.
async fn init(handle: WindowHandle, settings: RenderPlugin) -> Result<Gpu, RenderError> {
    let window = handle.winit().clone();
    let mut descriptor =
        wgpu::InstanceDescriptor::new_with_display_handle(Box::new(window.clone()));
    if let Some(backends) = settings.backends {
        descriptor.backends = backends;
    }
    // WGPU_BACKEND and friends override the settings on desktop.
    let descriptor = descriptor.with_env();
    // On the web this picks WebGPU when the browser really supports it, WebGL2 otherwise.
    let instance = wgpu::util::new_instance_with_webgpu_detection(descriptor).await;
    let surface = instance
        .create_surface(window)
        .map_err(|err| RenderError::Surface(err.to_string()))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: settings.power_preference,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            ..Default::default()
        })
        .await
        .map_err(|err| RenderError::NoAdapter(err.to_string()))?;
    let (device, queue) = request_device(&adapter).await?;

    // Draw through an sRGB view so blending happens in linear light. Browsers offer only
    // non-sRGB canvas formats, so there the sRGB variant is added as a view format.
    let capabilities = surface.get_capabilities(&adapter);
    let format = capabilities
        .formats
        .iter()
        .copied()
        .find(wgpu::TextureFormat::is_srgb)
        .or_else(|| capabilities.formats.first().copied())
        .ok_or_else(|| RenderError::Surface("the surface supports no formats".into()))?;
    let view_format = format.add_srgb_suffix();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: 0,
        height: 0,
        present_mode: if settings.vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        },
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: if view_format == format {
            vec![]
        } else {
            vec![view_format]
        },
    };

    let lost = Arc::new(AtomicBool::new(false));
    device.set_device_lost_callback({
        let lost = lost.clone();
        move |reason, message| {
            // `Destroyed` is our own doing: the device was dropped.
            if reason != wgpu::DeviceLostReason::Destroyed {
                log::error!("GPU device lost: {message}");
                lost.store(true, Ordering::Relaxed);
            }
        }
    });

    Ok(Gpu {
        painter: Painter::new(&device, view_format),
        info: adapter.get_info(),
        instance,
        device,
        queue,
        // Configured on the first frame, once the size is known.
        surface: Some(surface),
        config,
        view_format,
        lost,
    })
}

/// Draws the queued sprites and debug shapes into the window, then empties the queues.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render(
    mut renderer: ResMut<WindowRenderer>,
    window: Res<Window>,
    handle: Option<Res<WindowHandle>>,
    mut lifecycle: MessageReader<Lifecycle>,
    view: Res<FrameView>,
    images: Res<Assets<Image>>,
    mut sprites: ResMut<Sprites>,
    mut shapes: ResMut<DebugShapes>,
    mut stats: ResMut<RenderStats>,
    overlays: Option<ResMut<RenderOverlays>>,
    time: Res<Time>,
    mut exit: MessageWriter<AppExit>,
) {
    for event in lifecycle.read() {
        match event {
            Lifecycle::Suspended => renderer.suspend(),
            Lifecycle::Resumed => renderer.suspended = false,
            _ => {}
        }
    }
    stats.record_frame(time.raw_delta());

    let drawn = match handle {
        Some(handle) if !renderer.suspended => {
            let input = FrameInput {
                view: &view.0,
                images: &images,
                sprites: sprites.queued(),
                screen_sprites: sprites.screen_queued(),
                shapes: &shapes,
                scale_factor: window.scale_factor() as f32,
            };
            match renderer.gpu(&handle) {
                Ok(Some(gpu)) => {
                    if stats.adapter.is_empty() {
                        (stats.adapter, stats.backend) = describe(&gpu.info);
                    }
                    gpu.frame(
                        &handle,
                        window.physical_size(),
                        &input,
                        overlays.map(ResMut::into_inner),
                    )
                }
                Ok(None) => Ok(None),
                Err(err) => Err(err),
            }
        }
        _ => Ok(None),
    };
    let frame = drawn.unwrap_or_else(|err| {
        log::error!("{err}");
        exit.write(AppExit::error());
        None
    });
    let frame = frame.unwrap_or_default();
    stats.draw_calls = frame.draw_calls;
    stats.sprites = frame.sprites;
    stats.batches = frame.batches;
    stats.shapes = frame.shapes;

    sprites.clear();
    shapes.clear();
}
