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
//! fn main() -> AppExit {
//!     App::new()
//!         .add_plugins(DefaultPlugins::default())
//!         .add_systems(Startup, setup)
//!         .add_systems(Update, spin)
//!         .run()
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

use cg_anim::AnimPlugin;
use cg_assets::AssetPlugin;
use cg_audio::AudioPlugin;
use cg_core::{App, FrameLimitPlugin, Plugin};
use cg_physics::PhysicsPlugin;
use cg_platform::input::InputPlugin;
use cg_platform::storage::StoragePlugin;
use cg_platform::{Args, LogPlugin, WindowConfig, WindowPlugin};
use cg_render::RenderPlugin;
use cg_tiled::TiledPlugin;
use cg_ui::UiPlugin;
use cg_world::WorldPlugin;

/// The engine version, taken from the crate manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Declares a game's Android entry point: the `android_main` function that the APK's
/// activity calls on its own thread. It hands the app to the engine, then calls `$run`, the
/// function that builds and runs the game (the same one the desktop `main` calls). Put it
/// in the crate that builds the game's Android library (a `cdylib`); on other targets it
/// expands to nothing.
///
/// ```no_run
/// use cashgear_engine::prelude::*;
///
/// pub fn run() -> AppExit {
///     App::new().add_plugins(DefaultPlugins::default()).run()
/// }
///
/// cashgear_engine::android_main!(run);
/// # fn main() {}
/// ```
///
/// Launch flags come from the intent's `args` extra there:
/// `adb shell am start -n <app id>/com.cashgear.engine.CashGearActivity --es args "--demo"`.
#[macro_export]
macro_rules! android_main {
    ($run:path) => {
        /// The entry point `android-activity` calls when the activity starts.
        #[cfg(target_os = "android")]
        #[unsafe(no_mangle)]
        fn android_main(app: $crate::platform::android::AndroidApp) {
            $crate::platform::android::run(app, || {
                let _ = $run();
            });
        }
    };
}

/// The standard set of plugins for a game: logging, the `--frames N` flag, input, save
/// storage, the world (transforms, interpolation, prefabs), assets, the window with its
/// runner, the renderer, Tiled maps, physics, animation and effects, audio, and the UI
/// (text, widgets, touch controls, localization and, with the `debug-overlay` feature, the
/// F12 debug overlay).
///
/// Launch flags work on desktop and, as a query string (`?frames=60&backend=gl`), on the
/// web: `--frames N` exits after N frames, `--no-vsync` and `--backend <list>` configure
/// the renderer, `--hot-reload`, `--no-hot-reload`, `--asset-root <path>`,
/// `--asset-delay <seconds>` and `--bundle <path>` the assets, `--physics-debug` draws
/// colliders from the start, `--no-audio` turns the sound off, `--touch` and `--no-touch`
/// show or hide the on-screen touch controls whatever the device, and `--overlay` opens
/// the debug overlay.
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
    /// Physics settings. `--physics-debug` overrides them.
    pub physics: PhysicsPlugin,
    /// Animation settings: the particle budget.
    pub anim: AnimPlugin,
    /// Save storage settings. Set `namespace` to your game's id.
    pub storage: StoragePlugin,
    /// Audio settings: voice caps, positional defaults, streaming. `--no-audio` overrides
    /// them.
    pub audio: AudioPlugin,
    /// UI settings: the game's languages, touch controls, the debug overlay. `--touch`,
    /// `--no-touch` and `--overlay` override them.
    pub ui: UiPlugin,
}

impl Plugin for DefaultPlugins {
    fn build(&self, app: &mut App) {
        let args = Args::from_env();
        app.add_plugins(self.log)
            .add_plugins(FrameLimitPlugin {
                frames: args.parse("frames"),
            })
            .add_plugins(InputPlugin::default())
            .add_plugins(self.storage.clone())
            .add_plugins(WorldPlugin)
            .add_plugins(self.assets.clone().with_args(&args))
            .add_plugins(WindowPlugin {
                window: self.window.clone(),
            })
            .add_plugins(self.render.clone().with_args(&args))
            .add_plugins(TiledPlugin)
            .add_plugins(self.physics.clone().with_args(&args))
            .add_plugins(self.anim.clone())
            .add_plugins(self.audio.clone().with_args(&args))
            .add_plugins(self.ui.clone().with_args(&args));
        #[cfg(feature = "debug-overlay")]
        register_inspect(app);
    }
}

/// Shows the values of the animation, audio and Tiled components in the debug overlay's
/// inspector (the UI registers the rest).
#[cfg(feature = "debug-overlay")]
fn register_inspect(app: &mut App) {
    use cg_ui::InspectExt;
    app.register_inspect::<cg_anim::SpriteAnimation>()
        .register_inspect::<cg_anim::Animator>()
        .register_inspect::<cg_anim::Tweens>()
        .register_inspect::<cg_anim::ParticleEmitter>()
        .register_inspect::<cg_anim::CameraFollow>()
        .register_inspect::<cg_anim::CameraShake>()
        .register_inspect::<cg_audio::AudioEmitter>()
        .register_inspect::<cg_audio::AudioListener>()
        .register_inspect::<cg_tiled::Tilemap>()
        .register_inspect::<cg_tiled::TiledObject>()
        .register_inspect::<cg_tiled::TileAnimation>();
}

/// Common imports for game code: the app and plugins, schedules, time, the ECS, `glam`
/// vectors, logging macros, the window, input, save storage, transforms, states, prefabs,
/// assets, rendering, Tiled maps, physics, animation and effects, audio, and the UI.
pub mod prelude {
    pub use crate::DefaultPlugins;
    pub use cg_anim::prelude::*;
    pub use cg_assets::prelude::*;
    pub use cg_audio::prelude::*;
    pub use cg_core::prelude::*;
    pub use cg_physics::prelude::*;
    pub use cg_platform::prelude::*;
    pub use cg_render::prelude::*;
    pub use cg_tiled::prelude::*;
    pub use cg_ui::prelude::*;
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
