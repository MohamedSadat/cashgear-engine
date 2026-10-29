//! CashGear Engine: a 2D game engine for desktop, web and Android.
//!
//! This facade crate re-exports every engine subsystem under a short module name and
//! provides [`DefaultPlugins`] and a [`prelude`] for game code:
//!
//! ```no_run
//! use cashgear_engine::prelude::*;
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn(Camera2D::default());
//!     commands.spawn((
//!         Sprite::solid(Color::hex(0xffbe0b), vec2(32.0, 32.0)),
//!         Transform2D::from_xy(0.0, 0.0),
//!     ));
//! }
//!
//! fn spin(mut sprites: Query<&mut Transform2D, With<Sprite>>, time: Res<Time>) {
//!     for mut transform in &mut sprites {
//!         transform.rotate(time.delta_secs());
//!     }
//! }
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins::default())
//!         .add_systems(Startup, setup)
//!         .add_systems(Update, spin)
//!         .run();
//! }
//! ```

pub use cg_anim as anim;
pub use cg_assets as assets;
pub use cg_audio as audio;
pub use cg_core as core;
pub use cg_physics as physics;
pub use cg_platform as platform;
pub use cg_render as render;
#[cfg(feature = "script")]
pub use cg_script as script;
pub use cg_tiled as tiled;
pub use cg_ui as ui;
pub use cg_world as world;

use cg_assets::AssetPlugin;
use cg_core::{App, FrameLimitPlugin, Plugin};
use cg_platform::input::InputPlugin;
use cg_platform::{Args, LogPlugin, WindowConfig, WindowPlugin};
use cg_render::RenderPlugin;
use cg_tiled::TiledPlugin;
use cg_world::WorldPlugin;

/// The engine version, taken from the crate manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The standard set of plugins for a game: logging, the `--frames N` flag, input, the
/// world (transforms, interpolation, prefabs), assets, the window with its runner, the
/// renderer and Tiled maps.
///
/// Launch flags work on desktop and, as a query string (`?frames=60&backend=gl`), on the
/// web: `--frames N` exits after N frames, `--no-vsync` and `--backend <list>` configure
/// the renderer, and `--hot-reload`, `--no-hot-reload`, `--asset-root <path>` and
/// `--asset-delay <seconds>` the assets.
///
/// Configure it with struct-update syntax:
///
/// ```
/// use cashgear_engine::prelude::*;
///
/// let plugins = DefaultPlugins {
///     window: WindowConfig {
///         title: "My game".into(),
///         ..default()
///     },
///     ..default()
/// };
/// ```
#[derive(Debug, Clone, Default)]
pub struct DefaultPlugins {
    /// Window settings.
    pub window: WindowConfig,
    /// Logging settings.
    pub log: LogPlugin,
    /// Renderer settings. The `--no-vsync` and `--backend` flags override them.
    pub render: RenderPlugin,
    /// Asset settings. The asset flags override them.
    pub assets: AssetPlugin,
}

impl Plugin for DefaultPlugins {
    fn build(&self, app: &mut App) {
        let args = Args::from_env();
        app.add_plugins(self.log)
            .add_plugins(FrameLimitPlugin {
                frames: args.parse("frames"),
            })
            .add_plugins(InputPlugin::default())
            .add_plugins(WorldPlugin)
            .add_plugins(self.assets.clone().with_args(&args))
            .add_plugins(WindowPlugin {
                window: self.window.clone(),
            })
            .add_plugins(self.render.clone().with_args(&args))
            .add_plugins(TiledPlugin);
    }
}

/// Common imports for game code: the app and plugins, schedules, time, the ECS, `glam`
/// vectors, logging macros, the window, input, transforms, states, prefabs, assets,
/// rendering and Tiled maps.
pub mod prelude {
    pub use crate::DefaultPlugins;
    pub use cg_assets::prelude::*;
    pub use cg_core::prelude::*;
    pub use cg_platform::prelude::*;
    pub use cg_render::prelude::*;
    pub use cg_tiled::prelude::*;
    pub use cg_world::prelude::*;
}

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_is_semver() {
        let parts: Vec<_> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()));
    }
}
