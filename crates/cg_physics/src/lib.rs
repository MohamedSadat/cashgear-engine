//! Collision and physics for CashGear Engine: platformer-grade kinematic bodies that
//! collide with tiles (solid, one-way, slopes, hazards) and entity colliders, sensors that
//! report overlaps, collision layers, and raycasts and shape queries.
//!
//! | Item | Does |
//! | --- | --- |
//! | [`Collider`] | A rectangle or circle on an entity: static by default, a [`Sensor`] or a body's shape |
//! | [`KinematicBody`] | Moves by its velocity each fixed step, swept against tiles and static colliders |
//! | [`KinematicContacts`] | What a body touched: ground, walls, ceiling, hazards |
//! | [`TileCollider`], [`TileGrid`] | Tile collision; built automatically from Tiled maps |
//! | [`CollisionLayers`] | Which colliders interact |
//! | [`CollisionStarted`], [`CollisionEnded`], [`Overlaps`] | Overlaps of sensors, bodies and colliders |
//! | [`SpatialQuery`] | Raycasts, point and shape tests, tile lookups |
//! | [`PhysicsDebug`] | Draws colliders and contacts (F1) |
//!
//! Units are world units: Tiled pixels, so speeds are pixels per second. The physics runs
//! in [`FixedPostUpdate`], so set velocities in
//! [`FixedUpdate`](cg_core::FixedUpdate) and read contacts and messages there in the next
//! step:
//!
//! ```
//! use cg_core::prelude::*;
//! use cg_physics::prelude::*;
//! use cg_world::prelude::*;
//!
//! fn spawn(mut commands: Commands) {
//!     // A floor: one row of solid cells, 16 units square.
//!     let floor = TileGrid::parse(vec2(16.0, 16.0), "##########");
//!     commands.spawn(TileCollider::new(floor));
//!     // A box that falls onto it.
//!     commands.spawn((
//!         KinematicBody::default(),
//!         Collider::rect(12.0, 14.0),
//!         Transform2D::from_xy(40.0, 100.0),
//!     ));
//! }
//!
//! fn report(bodies: Query<&KinematicContacts, Changed<KinematicContacts>>) {
//!     for contacts in &bodies {
//!         if contacts.just_landed() {
//!             info!("landed");
//!         }
//!     }
//! }
//!
//! App::new()
//!     .add_plugins(WorldPlugin)
//!     .add_plugins(PhysicsPlugin::default())
//!     .add_systems(Startup, spawn)
//!     .add_systems(FixedUpdate, report)
//!     .run();
//! ```
//!
//! Tiled maps collide through their tile properties (`solid`, `one_way`, `slope_left` and
//! `slope_right`, `hazard`) on layers marked `collision`, and objects become colliders or
//! sensors with a `collider` or `sensor` property; see [`TileCollider`]. Dynamic rigid
//! bodies (`rapier2d`) are not part of the engine yet.

mod body;
mod debug;
mod events;
mod grid;
mod layers;
mod shape;
mod systems;
mod tiled;
mod world;

pub use body::{Contact, Gravity, KinematicBody, KinematicContacts};
pub use debug::PhysicsDebug;
pub use events::{CollisionEnded, CollisionStarted, Overlaps};
pub use grid::{TileCell, TileCollider, TileGrid, TileShape};
pub use layers::CollisionLayers;
pub use shape::{Collider, ColliderShape, Sensor, WorldShape};
pub use world::{PhysicsWorld, QueryFilter, RayHit, SpatialQuery, TileHit};

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use cg_core::glam::Vec2;
use cg_core::{App, FixedPostUpdate, Plugin, PreUpdate, Render, Update};
use cg_platform::Args;
use cg_platform::input::KeyCode;
use cg_render::RenderSystems;
use cg_world::{AppWorldExt, TransformSystems};

/// Distances up to this (a hundredth of a world unit) count as touching.
pub(crate) const EPSILON: f32 = 0.01;

/// System sets in [`FixedPostUpdate`], in this order, after
/// transform propagation. Systems that need this step's contacts or overlaps can run
/// `.after(PhysicsSystems::Detect)`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhysicsSystems {
    /// Records every collider where it is now.
    Prepare,
    /// Moves the kinematic bodies.
    Move,
    /// Reports overlaps that started or ended.
    Detect,
}

/// Adds the physics: [`Gravity`], the [`PhysicsWorld`] and [`Overlaps`] resources, the
/// [`CollisionStarted`] and [`CollisionEnded`] messages, the systems that build tile
/// colliders from Tiled maps (in [`PreUpdate`]), the physics step (in
/// [`FixedPostUpdate`]), and debug drawing (in [`Render`], when the renderer is there).
/// Registers `Collider`, `Sensor`, `KinematicBody` and `CollisionLayers` for prefabs and
/// Tiled properties.
///
/// `DefaultPlugins` includes it.
#[derive(Debug, Clone, PartialEq)]
pub struct PhysicsPlugin {
    /// The starting [`Gravity`], in world units per second squared.
    pub gravity: Vec2,
    /// Draw colliders from the start. `--physics-debug` turns it on.
    pub debug: bool,
    /// The key that toggles debug drawing. F1 by default.
    pub debug_key: Option<KeyCode>,
}

impl Default for PhysicsPlugin {
    fn default() -> Self {
        Self {
            gravity: Gravity::default().0,
            debug: false,
            debug_key: Some(KeyCode::F1),
        }
    }
}

impl PhysicsPlugin {
    /// Applies the `--physics-debug` launch flag (`?physics-debug` in the page URL).
    pub fn with_args(mut self, args: &Args) -> Self {
        if args.flag("physics-debug") {
            self.debug = true;
        }
        self
    }
}

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Gravity(self.gravity))
            .insert_resource(PhysicsDebug {
                enabled: self.debug,
                toggle_key: self.debug_key,
            })
            .init_resource::<PhysicsWorld>()
            .init_resource::<Overlaps>()
            .add_message::<CollisionStarted>()
            .add_message::<CollisionEnded>()
            .register_component::<Collider>("Collider")
            .register_component::<Sensor>("Sensor")
            .register_component::<KinematicBody>("KinematicBody")
            .register_component::<CollisionLayers>("CollisionLayers")
            .add_systems(
                PreUpdate,
                (tiled::build_tile_colliders, tiled::add_object_colliders),
            )
            .add_systems(
                FixedPostUpdate,
                (
                    systems::prepare.in_set(PhysicsSystems::Prepare),
                    systems::move_bodies.in_set(PhysicsSystems::Move),
                    systems::detect.in_set(PhysicsSystems::Detect),
                )
                    .chain()
                    .after(TransformSystems::Propagate),
            )
            .add_systems(Update, debug::toggle_debug)
            .add_systems(
                Render,
                debug::draw_debug
                    .in_set(RenderSystems::Queue)
                    .after(RenderSystems::View),
            );
    }
}

/// Common imports: the plugin, colliders, bodies and their contacts, tiles, layers,
/// overlap messages and queries.
pub mod prelude {
    pub use crate::{
        Collider, ColliderShape, CollisionEnded, CollisionLayers, CollisionStarted, Gravity,
        KinematicBody, KinematicContacts, Overlaps, PhysicsDebug, PhysicsPlugin, PhysicsSystems,
        QueryFilter, RayHit, Sensor, SpatialQuery, TileCell, TileCollider, TileGrid, TileShape,
    };
}

#[cfg(test)]
mod tests;
