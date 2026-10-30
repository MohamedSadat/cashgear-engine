//! Assets of CashGear Engine: typed handles to files loaded in the background on every
//! target, load states with dependencies, loading screens, and hot reloading on desktop.
//!
//! | Item | Does |
//! | --- | --- |
//! | [`AssetServer`] | Loads files by path (`server.load("maps/level1.tmx")`) and tracks them |
//! | [`Handle<T>`] | Names an asset and keeps it loaded; the asset unloads with its last handle |
//! | [`Assets<T>`] | The loaded assets of one type |
//! | [`AssetLoader`] | Turns a file's bytes into an asset; can read more files and load dependencies |
//! | [`LoadingAssets`] | A group of assets to wait for, with [`progress`](LoadingAssets::progress) for a loading bar |
//! | [`PrefabFiles`] | Prefab files loaded into [`Prefabs`](cg_world::Prefabs) by name, reloaded when they change |
//!
//! Files live in the game's `assets` folder and are named by their path inside it, with
//! `/` separators. On desktop they are read from disk, on the web fetched from the page's
//! server, and on Android read from the APK; see
//! [`AssetIo`] for how the folder is found.
//!
//! ```
//! use cg_assets::prelude::*;
//! use cg_core::prelude::*;
//! use cg_world::prelude::*;
//!
//! #[derive(Clone, PartialEq, Eq, Hash, Debug)]
//! enum GameState { Loading, Playing }
//!
//! fn enter_playing(mut commands: Commands) {
//!     commands.spawn_prefab("bunny");
//! }
//!
//! App::new()
//!     .add_plugins(AssetPlugin::default())
//!     // Loaded in the background; the loading state waits for it.
//!     .load_prefab("bunny", "prefabs/bunny.prefab.ron")
//!     .init_state(GameState::Loading)
//!     .add_loading_state(GameState::Loading, GameState::Playing)
//!     .add_systems(OnEnter(GameState::Playing), enter_playing);
//! ```
//!
//! Engine crates register their own asset types and loaders: images (PNG) and texture
//! atlases in `cg_render`, Tiled maps in `cg_tiled`. Games add theirs with
//! [`AppAssetExt::init_asset`] and [`AppAssetExt::register_loader`].

mod assets;
mod handle;
mod loader;
mod loading;
pub mod path;
mod prefab;
mod server;

pub use assets::{AssetEvent, Assets};
pub use handle::{AssetId, Handle, RESERVED_IDS, UntypedAssetId, UntypedHandle};
pub use loader::{AssetError, AssetLoader, BoxError, BoxedFuture, LoadContext};
pub use loading::{LoadProgress, LoadingAssets};
pub use prefab::{PrefabFiles, PrefabLoader, PrefabSource};
pub use server::{AssetServer, LoadState};

use bevy_ecs::prelude::*;
use cg_core::{App, First, Plugin, Update};
use cg_platform::Args;
use cg_platform::io::{AssetIo, IoError};
use cg_world::{States, WorldPlugin, in_state};

/// A type that can be loaded and stored as an asset. Implement it for each asset type:
/// `impl Asset for Dialogue {}`.
pub trait Asset: Send + Sync + 'static {}

/// The asset systems in [`First`]. Systems that read freshly loaded assets in `First` go
/// after [`AssetSystems::Events`].
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetSystems {
    /// Picks up changed files, starts loads and stores finished ones.
    Load,
    /// Unloads assets whose last strong handle was dropped.
    Unload,
    /// Sends the [`AssetEvent`]s of this frame.
    Events,
}

/// Adds the [`AssetServer`], [`LoadingAssets`] and prefab files, and the systems that
/// load, unload and reload assets in [`First`]. Adds [`WorldPlugin`] if it is missing.
///
/// `DefaultPlugins` includes it, with the launch flags applied (see
/// [`with_args`](Self::with_args)).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AssetPlugin {
    /// The asset folder (a URL prefix on the web), or `None` for the default (see
    /// [`AssetIo`]).
    pub root: Option<String>,
    /// Reload assets when their files change. `None` means on in debug builds, where the
    /// target can watch files (desktop).
    pub hot_reload: Option<bool>,
    /// Hold every finished load back by this many seconds, to try a loading screen on a
    /// fast disk.
    pub delay: f32,
    /// An asset bundle (made by `cg_pack bundle`) to read files from first: a file on
    /// desktop, a URL relative to the page on the web (see [`AssetIo::with_bundle`]). A web
    /// page can also hand over a bundle it downloaded itself, without this. Hot reloading
    /// is off while a bundle is in use.
    pub bundle: Option<String>,
}

impl AssetPlugin {
    /// Applies the launch flags `--hot-reload`, `--no-hot-reload`, `--asset-root <path>`,
    /// `--asset-delay <seconds>` (or `?asset-delay=0.5` in the page URL) and
    /// `--bundle <path>`.
    pub fn with_args(mut self, args: &Args) -> Self {
        if args.flag("hot-reload") {
            self.hot_reload = Some(true);
        }
        if args.flag("no-hot-reload") {
            self.hot_reload = Some(false);
        }
        if let Some(root) = args.value("asset-root") {
            self.root = Some(root.to_owned());
        }
        if let Some(delay) = args.parse("asset-delay") {
            self.delay = delay;
        }
        if let Some(bundle) = args.value("bundle") {
            self.bundle = Some(bundle.to_owned());
        }
        self
    }
}

impl Plugin for AssetPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<WorldPlugin>() {
            app.add_plugins(WorldPlugin);
        }
        let mut io = AssetIo::new(self.root.as_deref());
        if let Some(bundle) = &self.bundle {
            io = io.with_bundle(bundle);
        }
        let server = AssetServer::new(io);
        // Changed files in the folder would be read from the bundle again, unchanged.
        if server.io().bundle().is_some() {
            log::debug!("no hot reloading: reading from a bundle");
        } else if self.hot_reload.unwrap_or(cfg!(debug_assertions)) {
            match server.watch_for_changes() {
                Ok(()) => log::info!("hot reloading assets in `{}`", server.io().root()),
                Err(IoError::Unsupported(why)) => log::debug!("no hot reloading: {why}"),
                // A game without an asset folder has nothing to watch.
                Err(IoError::NotFound(root)) => {
                    log::debug!("no hot reloading: no asset folder at `{root}`")
                }
                Err(err) => log::warn!("no hot reloading: {err}"),
            }
        }
        if self.delay > 0.0 {
            server.set_simulated_delay(f64::from(self.delay));
        }
        app.insert_resource(server)
            .init_resource::<LoadingAssets>()
            .init_resource::<PrefabFiles>()
            .add_systems(First, update_assets.in_set(AssetSystems::Load))
            .init_asset::<PrefabSource>()
            .register_loader(PrefabLoader)
            .add_systems(First, prefab::sync_prefab_files.after(AssetSystems::Events));
    }
}

fn update_assets(world: &mut World) {
    if let Some(server) = world.get_resource::<AssetServer>().cloned() {
        server.update(world);
    }
}

fn unload_unused<T: Asset>(mut assets: ResMut<Assets<T>>, server: Option<Res<AssetServer>>) {
    let freed = assets.bypass_change_detection().unload_dropped();
    if freed.is_empty() {
        return;
    }
    assets.set_changed();
    if let Some(server) = server {
        server.forget(freed.into_iter().map(AssetId::untyped));
    }
}

fn send_asset_events<T: Asset>(
    mut assets: ResMut<Assets<T>>,
    mut writer: MessageWriter<AssetEvent<T>>,
) {
    let events = assets.bypass_change_detection().take_events();
    if !events.is_empty() {
        writer.write_batch(events);
    }
}

/// Asset additions to [`App`].
pub trait AppAssetExt {
    /// Makes `T` an asset type: adds [`Assets<T>`], the [`AssetEvent<T>`] message and the
    /// systems that unload `T`s and send their events. Adds [`AssetPlugin`] if it is
    /// missing. Calling it again does nothing.
    fn init_asset<T: Asset>(&mut self) -> &mut Self;

    /// Adds an [`AssetLoader`]. Adds [`AssetPlugin`] if it is missing.
    fn register_loader(&mut self, loader: impl AssetLoader) -> &mut Self;

    /// Loads a prefab file and names it, so `commands.spawn_prefab(name)` finds it once it
    /// has loaded. The file joins [`LoadingAssets`], so a loading state waits for it. When
    /// the file changes (with hot reloading), new spawns use the new version.
    fn load_prefab(&mut self, name: &str, path: &str) -> &mut Self;

    /// Moves from `loading` to `next` (with [`NextState::replace`](cg_world::NextState))
    /// once everything in [`LoadingAssets`] has loaded or failed. Add the assets to wait
    /// for in `OnEnter(loading)`, or with [`load_prefab`](Self::load_prefab).
    fn add_loading_state<S: States>(&mut self, loading: S, next: S) -> &mut Self;
}

impl AppAssetExt for App {
    fn init_asset<T: Asset>(&mut self) -> &mut Self {
        if !self.world().contains_resource::<AssetServer>() {
            self.add_plugins(AssetPlugin::default());
        }
        let server = self.world().resource::<AssetServer>().clone();
        if server.is_registered::<T>() {
            return self;
        }
        let existing = self
            .world()
            .get_resource::<Assets<T>>()
            .map(|assets| assets.shared().clone());
        let shared = server.register_type::<T>(existing.as_ref());
        if existing.is_none() {
            self.insert_resource(Assets::<T>::with_shared(shared));
        }
        self.add_message::<AssetEvent<T>>().add_systems(
            First,
            (
                unload_unused::<T>
                    .in_set(AssetSystems::Unload)
                    .after(AssetSystems::Load),
                send_asset_events::<T>
                    .in_set(AssetSystems::Events)
                    .after(AssetSystems::Unload),
            ),
        )
    }

    fn register_loader(&mut self, loader: impl AssetLoader) -> &mut Self {
        if !self.world().contains_resource::<AssetServer>() {
            self.add_plugins(AssetPlugin::default());
        }
        self.world()
            .resource::<AssetServer>()
            .register_loader(loader);
        self
    }

    fn load_prefab(&mut self, name: &str, path: &str) -> &mut Self {
        if !self.world().contains_resource::<AssetServer>() {
            self.add_plugins(AssetPlugin::default());
        }
        let handle = self
            .world()
            .resource::<AssetServer>()
            .load::<PrefabSource>(path);
        let world = self.world_mut();
        world.resource_mut::<LoadingAssets>().add(&handle);
        world.resource_mut::<PrefabFiles>().add(name, handle);
        self
    }

    fn add_loading_state<S: States>(&mut self, loading: S, next: S) -> &mut Self {
        self.add_systems(
            Update,
            loading::finish_loading(next).run_if(in_state(loading)),
        )
    }
}

/// Common imports: the plugin, the server, handles, asset collections and loading groups.
pub mod prelude {
    pub use crate::{
        AppAssetExt, Asset, AssetEvent, AssetId, AssetPlugin, AssetServer, Assets, Handle,
        LoadProgress, LoadState, LoadingAssets,
    };
}

#[cfg(test)]
mod tests;
