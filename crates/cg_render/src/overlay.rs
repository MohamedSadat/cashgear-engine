//! Render overlays: passes other crates add on top of each frame, such as the egui debug
//! overlay in `cg_ui`.

use bevy_ecs::resource::Resource;
use cg_core::glam::UVec2;

/// A pass drawn over each window frame, after the sprites, the screen-space sprites and the
/// debug shapes: an immediate-mode debug UI, say. Add one to [`RenderOverlays`].
///
/// The overlay records its own render passes into the frame's encoder, loading what is
/// already in the target.
pub trait RenderOverlay: Send + Sync + 'static {
    /// Draws into the frame.
    fn draw(&mut self, frame: &mut OverlayFrame<'_>);
}

/// What an overlay draws with: the GPU, the frame's encoder and target, and its size.
pub struct OverlayFrame<'a> {
    /// The GPU device.
    pub device: &'a wgpu::Device,
    /// The device's queue.
    pub queue: &'a wgpu::Queue,
    /// The frame's commands, submitted after every overlay has drawn.
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// The frame's colour target, an sRGB view.
    pub target: &'a wgpu::TextureView,
    /// The target's format.
    pub format: wgpu::TextureFormat,
    /// The target's size in physical pixels.
    pub size: UVec2,
    /// Physical pixels per logical pixel.
    pub scale_factor: f32,
    /// Changes whenever the GPU device is created again (after a device loss), so an overlay
    /// knows to rebuild its GPU resources.
    pub device_generation: u64,
}

/// The overlays drawn over each window frame, in order.
#[derive(Resource, Default)]
pub struct RenderOverlays {
    overlays: Vec<Box<dyn RenderOverlay>>,
}

impl RenderOverlays {
    /// Adds an overlay, drawn after those added before it.
    pub fn add(&mut self, overlay: impl RenderOverlay) {
        self.overlays.push(Box::new(overlay));
    }

    /// The number of overlays.
    pub fn len(&self) -> usize {
        self.overlays.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.overlays.is_empty()
    }

    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = &mut Box<dyn RenderOverlay>> {
        self.overlays.iter_mut()
    }
}
