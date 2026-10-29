//! Renderer of CashGear Engine: the `wgpu` context and window surface, textures, the sprite
//! batcher, the 2D camera with virtual resolutions, debug shapes and render stats.
//!
//! Systems queue what to draw each frame; [`RenderPlugin`] draws it in the
//! [`Render`](cg_core::Render) schedule and empties the queues:
//!
//! | Resource | Holds |
//! | --- | --- |
//! | [`Textures`] | Images the renderer can draw, uploaded to the GPU when it is ready |
//! | [`Sprites`] | This frame's [`Sprite`]s, sorted by layer then texture, one draw call per batch |
//! | [`DebugShapes`] | This frame's lines, rectangles and circles, drawn on top |
//! | [`Camera2D`] | The view: position, zoom, rotation and [`ScalingMode`] |
//! | [`RenderStats`] | Draw calls, sprites, frame time and the GPU in use |
//!
//! Colours are sRGB ([`Color`]) and blending happens in linear light. Textures use nearest
//! sampling unless they ask for [`Filter::Linear`]. On the web the renderer uses WebGPU
//! where the browser supports it and WebGL2 otherwise; there the GPU becomes ready a few
//! frames after start-up, and nothing is drawn until then.
//!
//! ```no_run
//! use cg_core::prelude::*;
//! use cg_platform::prelude::*;
//! use cg_render::prelude::*;
//!
//! fn draw(mut sprites: ResMut<Sprites>, mut debug: ResMut<DebugShapes>) {
//!     sprites.draw(Sprite {
//!         size: Some(vec2(16.0, 16.0)),
//!         color: Color::hex(0xffbe0b),
//!         ..Sprite::new(TextureId::WHITE)
//!     });
//!     debug.circle(Vec2::ZERO, 12.0, Color::GREEN);
//! }
//!
//! App::new()
//!     .add_plugins(InputPlugin::default())
//!     .add_plugins(WindowPlugin::default())
//!     .add_plugins(RenderPlugin::default())
//!     .add_systems(Update, draw)
//!     .run();
//! ```

mod batch;
mod camera;
mod color;
mod debug;
mod gpu;
mod image;
mod sprite;
mod stats;
mod texture;

pub use camera::{Camera2D, DEFAULT_VIRTUAL_RESOLUTION, ScalingMode, Viewport};
pub use color::Color;
pub use debug::DebugShapes;
pub use gpu::RenderError;
pub use gpu::offscreen::OffscreenRenderer;
pub use image::{Image, ImageError};
pub use sprite::{Sprite, Sprites};
pub use stats::RenderStats;
pub use texture::{Filter, TextureId, Textures};
/// The `wgpu` crate the renderer is built on, for its configuration types.
pub use wgpu;

use cg_core::{App, Plugin, Render};
use cg_platform::{Args, Lifecycle, Window};

/// Adds the renderer: the [`Textures`], [`Sprites`], [`DebugShapes`], [`Camera2D`] and
/// [`RenderStats`] resources, and the system that draws them into the window in the
/// [`Render`] schedule.
///
/// Needs [`WindowPlugin`](cg_platform::WindowPlugin) for the window. The GPU is set up
/// once the window exists; if that fails the error is logged and the app exits with an
/// error.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderPlugin {
    /// Wait for the display's refresh before showing each frame, which also paces the game
    /// loop. On by default; `--no-vsync` turns it off for benchmarks.
    pub vsync: bool,
    /// Which GPU to use on machines with more than one.
    pub power_preference: wgpu::PowerPreference,
    /// Graphics APIs to try, or `None` for all of them. On the web that means WebGPU, then
    /// WebGL2. `--backend gl` (or `?backend=gl` in the page URL) forces one; on desktop the
    /// `WGPU_BACKEND` environment variable wins over both.
    pub backends: Option<wgpu::Backends>,
}

impl Default for RenderPlugin {
    fn default() -> Self {
        Self {
            vsync: true,
            power_preference: wgpu::PowerPreference::HighPerformance,
            backends: None,
        }
    }
}

impl RenderPlugin {
    /// Applies the `--no-vsync` and `--backend <list>` launch flags, when present. The
    /// list is comma-separated: `vulkan`, `dx12`, `metal`, `gl`, `webgpu`.
    pub fn with_args(mut self, args: &Args) -> Self {
        if args.flag("no-vsync") {
            self.vsync = false;
        }
        if let Some(list) = args.value("backend") {
            self.backends = Some(wgpu::Backends::from_comma_list(list));
        }
        self
    }
}

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Window>()
            .add_message::<Lifecycle>()
            .init_resource::<Textures>()
            .init_resource::<Sprites>()
            .init_resource::<DebugShapes>()
            .init_resource::<Camera2D>()
            .init_resource::<RenderStats>()
            .insert_resource(gpu::window::WindowRenderer::new(self.clone()))
            .add_systems(Render, gpu::window::render);
    }
}

/// Common imports: the plugin, sprites, textures, images, colours, the camera and debug
/// shapes.
pub mod prelude {
    pub use crate::{
        Camera2D, Color, DebugShapes, Filter, Image, RenderPlugin, RenderStats, ScalingMode,
        Sprite, Sprites, TextureId, Textures,
    };
}
