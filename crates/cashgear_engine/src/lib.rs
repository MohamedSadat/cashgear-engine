//! CashGear Engine: a 2D game engine for desktop, web and Android.
//!
//! This facade crate re-exports every engine subsystem under a short module name and
//! provides [`DefaultPlugins`] and a [`prelude`] for game code:
//!
//! ```no_run
//! use cashgear_engine::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins::default())
//!         .add_systems(Startup, || info!("hello"))
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

use cg_core::{App, FrameLimitPlugin, Plugin};
use cg_platform::input::InputPlugin;
use cg_platform::{LogPlugin, WindowConfig, WindowPlugin};

/// The engine version, taken from the crate manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The standard set of plugins for a game: logging, the `--frames N` flag, input, and the
/// window with its runner.
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
}

impl Plugin for DefaultPlugins {
    fn build(&self, app: &mut App) {
        app.add_plugins(self.log)
            .add_plugins(FrameLimitPlugin::from_args())
            .add_plugins(InputPlugin::default())
            .add_plugins(WindowPlugin {
                window: self.window.clone(),
            });
    }
}

/// Common imports for game code: the app and plugins, schedules, time, the ECS, `glam`
/// vectors, logging macros, the window and input.
pub mod prelude {
    pub use crate::DefaultPlugins;
    pub use cg_core::prelude::*;
    pub use cg_platform::prelude::*;
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
