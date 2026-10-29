//! Tiled support for CashGear Engine: `.tmx` maps with their `.tsx` tilesets load through
//! the asset server on every target, tile layers draw as cached chunks, and object layers
//! spawn entities, prefabs included.
//!
//! | Item | Does |
//! | --- | --- |
//! | [`TiledMap`] | The map asset: tilesets, layers, objects and custom properties |
//! | [`Tilemap`] | Draws a map at the entity and spawns its objects as children |
//! | [`TiledObject`] | What the map says about an object entity: id, name, class, shape, properties |
//! | [`TmxLoader`] | Reads `.tmx` files, with `.tsx` tilesets and `.tx` templates |
//! | [`TileAnimation`] | Plays Tiled's tile animations on tile objects (tile layers play them by themselves) |
//!
//! ```
//! use cg_assets::AssetServer;
//! use cg_core::prelude::*;
//! use cg_tiled::{TiledObject, Tilemap};
//!
//! fn load_level(mut commands: Commands, server: Res<AssetServer>) {
//!     commands.spawn(Tilemap::new(server.load("maps/level1.tmx")));
//! }
//!
//! // Objects spawn as entities once the map has loaded; find them by class.
//! fn find_start(objects: Query<(&TiledObject, &GlobalTransform2D), Added<TiledObject>>) {
//!     for (object, transform) in &objects {
//!         if object.class == "PlayerStart" {
//!             info!("the player starts at {}", transform.translation());
//!         }
//!     }
//! }
//! # use cg_world::GlobalTransform2D;
//! ```
//!
//! Maps are orthogonal; isometric and hexagonal maps and image-collection tilesets are not
//! supported yet. Tiles animated in Tiled's tile animation editor play in tile layers and
//! on tile objects. With hot reloading on, saving the map, a tileset or a template in
//! Tiled rebuilds the map in the running game, and saving a tileset image reloads the
//! texture.

mod animation;
mod draw;
mod loader;
mod map;
mod spawn;

pub use animation::TileAnimation;
pub use loader::TmxLoader;
pub use map::{
    ImageLayer, LayerKind, LayerTile, MapLayer, MapObject, ObjectShape, Properties, PropertyValue,
    TileFrame, TileInfo, TileLayer, TiledMap, Tileset,
};
pub use spawn::{TiledObject, Tilemap, TilemapData, TilemapState};

use bevy_ecs::schedule::IntoScheduleConfigs;
use cg_assets::AppAssetExt;
use cg_core::{App, Plugin, PostUpdate, PreUpdate, Render};
use cg_render::{RenderPlugin, RenderSystems};
use cg_world::AppWorldExt;

/// Adds the [`TiledMap`] asset with its loader, the `Tilemap` prefab component, the system
/// that builds tilemaps and spawns their objects in [`PreUpdate`], and the system that
/// queues their tiles in [`Render`]. Adds [`RenderPlugin`] if it is missing.
///
/// `DefaultPlugins` includes it.
#[derive(Debug, Clone, Copy, Default)]
pub struct TiledPlugin;

impl Plugin for TiledPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<RenderPlugin>() {
            app.add_plugins(RenderPlugin::default());
        }
        app.init_asset::<TiledMap>()
            .register_loader(TmxLoader)
            .register_prefab_component::<Tilemap>("Tilemap")
            .add_systems(PreUpdate, spawn::build_tilemaps)
            .add_systems(PostUpdate, animation::animate_tile_objects)
            .add_systems(
                Render,
                draw::queue_tilemaps
                    .in_set(RenderSystems::Queue)
                    .after(RenderSystems::View),
            );
    }
}

/// Common imports: the plugin, the map asset, the components and tile animations.
pub mod prelude {
    pub use crate::{TileAnimation, TiledMap, TiledObject, TiledPlugin, Tilemap};
}

#[cfg(test)]
mod tests;
