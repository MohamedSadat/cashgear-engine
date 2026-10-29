//! Renderer of CashGear Engine: the `wgpu` context and window surface, textures, the sprite
//! batcher, the 2D camera with virtual resolutions, debug shapes and render stats.
//!
//! Entities are drawn from their components:
//!
//! | Component | Does |
//! | --- | --- |
//! | [`Sprite`] | A textured quad at the entity's transform |
//! | [`Layer`] | Draw order: higher layers on top |
//! | [`Visibility`] | Hides an entity and the children that inherit from it |
//! | [`Camera2D`] | The view: zoom and [`ScalingMode`], placed by the entity's transform |
//!
//! Each frame [`RenderPlugin`] picks the active camera, adds the visible sprites inside its
//! view to the [`Sprites`] queue, draws everything in the [`Render`](cg_core::Render)
//! schedule and empties the queues. Systems can also draw directly through the queues:
//!
//! | Resource | Holds |
//! | --- | --- |
//! | [`Textures`] | Images the renderer can draw, uploaded to the GPU when it is ready |
//! | [`Sprites`] | This frame's [`QueuedSprite`]s, sorted by layer then texture, one draw call per batch |
//! | [`DebugShapes`] | This frame's lines, rectangles and circles, drawn on top |
//! | [`RenderStats`] | Draw calls, sprites, culled sprites, frame time and the GPU in use |
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
//! use cg_world::prelude::*;
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn(Camera2D::default());
//!     commands.spawn((
//!         Sprite::solid(Color::hex(0xffbe0b), vec2(16.0, 16.0)),
//!         Transform2D::from_xy(0.0, 20.0),
//!     ));
//! }
//!
//! fn draw(mut sprites: ResMut<Sprites>, mut debug: ResMut<DebugShapes>) {
//!     sprites.draw(Sprite::solid(Color::WHITE, vec2(8.0, 8.0)), Transform2D::IDENTITY);
//!     debug.circle(Vec2::ZERO, 12.0, Color::GREEN);
//! }
//!
//! App::new()
//!     .add_plugins(InputPlugin::default())
//!     .add_plugins(WindowPlugin::default())
//!     .add_plugins(RenderPlugin::default())
//!     .add_systems(Startup, setup)
//!     .add_systems(Update, draw)
//!     .run();
//! ```

mod batch;
mod camera;
mod color;
mod debug;
mod gpu;
mod image;
mod queue;
mod sprite;
mod stats;
mod texture;
mod visibility;

pub use camera::{
    Camera2D, CameraView, DEFAULT_VIRTUAL_RESOLUTION, MainCamera, ScalingMode, Viewport,
};
pub use color::Color;
pub use debug::DebugShapes;
pub use gpu::RenderError;
pub use gpu::offscreen::OffscreenRenderer;
pub use image::{Image, ImageError};
pub use queue::{FrameView, RenderSystems};
pub use sprite::{Layer, QueuedSprite, Sprite, SpriteData, Sprites};
pub use stats::RenderStats;
pub use texture::{Filter, TextureId, Textures};
pub use visibility::{InheritedVisibility, Visibility};
/// The `wgpu` crate the renderer is built on, for its configuration types.
pub use wgpu;

use bevy_ecs::schedule::IntoScheduleConfigs;
use cg_core::{App, Plugin, PostUpdate, Render};
use cg_platform::{Args, Lifecycle, Window};
use cg_world::{AppWorldExt, TransformSystems, WorldPlugin};

/// Adds the renderer: the [`Textures`], [`Sprites`], [`DebugShapes`], [`FrameView`] and
/// [`RenderStats`] resources, visibility propagation in [`PostUpdate`], and the systems
/// that queue sprite entities and draw everything into the window in the [`Render`]
/// schedule. Registers `Sprite`, `Layer`, `Visibility` and `Camera2D` for prefabs, and adds
/// [`WorldPlugin`] if it is missing.
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
        if !app.is_plugin_added::<WorldPlugin>() {
            app.add_plugins(WorldPlugin);
        }
        app.init_resource::<Window>()
            .add_message::<Lifecycle>()
            .init_resource::<Textures>()
            .init_resource::<Sprites>()
            .init_resource::<DebugShapes>()
            .init_resource::<FrameView>()
            .init_resource::<RenderStats>()
            .register_prefab_component::<Sprite>("Sprite")
            .register_component::<Layer>("Layer")
            .register_component::<Visibility>("Visibility")
            .register_component::<Camera2D>("Camera2D")
            .insert_resource(gpu::window::WindowRenderer::new(self.clone()))
            .add_systems(
                PostUpdate,
                visibility::propagate_visibility.after(TransformSystems::Propagate),
            )
            .add_systems(
                Render,
                (
                    queue::queue_world.in_set(RenderSystems::Queue),
                    gpu::window::render
                        .in_set(RenderSystems::Draw)
                        .after(RenderSystems::Queue),
                ),
            );
    }
}

/// Common imports: the plugin, sprites and layers, visibility, textures, images, colours,
/// the camera and debug shapes.
pub mod prelude {
    pub use crate::{
        Camera2D, CameraView, Color, DebugShapes, Filter, Image, Layer, MainCamera, RenderPlugin,
        RenderStats, ScalingMode, Sprite, Sprites, TextureId, Textures, Visibility,
    };
}
