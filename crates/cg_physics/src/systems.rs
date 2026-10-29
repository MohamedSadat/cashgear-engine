//! The physics systems run in each fixed step, in [`FixedPostUpdate`](cg_core::FixedPostUpdate)
//! after transform propagation: snapshot the colliders, move the bodies, report overlaps.

use crate::body::{Gravity, KinematicBody, KinematicContacts, step_body};
use crate::events::{CollisionEnded, CollisionStarted, Overlaps};
use crate::grid::TileCollider;
use crate::layers::CollisionLayers;
use crate::shape::{Collider, Sensor};
use crate::world::{Entry, Env, GridEntry, PhysicsWorld};
use bevy_ecs::prelude::*;
use cg_core::Time;
use cg_core::glam::Vec2;
use cg_world::{GlobalTransform2D, Transform2D};

type ColliderItem = (
    Entity,
    &'static Collider,
    &'static GlobalTransform2D,
    Option<&'static CollisionLayers>,
    Has<Sensor>,
    Has<KinematicBody>,
);

type BodyItem = (
    &'static mut KinematicBody,
    &'static mut KinematicContacts,
    &'static Collider,
    &'static mut Transform2D,
    &'static mut GlobalTransform2D,
    Option<&'static CollisionLayers>,
    Option<&'static ChildOf>,
);

/// Records every collider and tile grid where it is now, and rebuilds the spatial hash.
pub(crate) fn prepare(
    mut world: ResMut<PhysicsWorld>,
    colliders: Query<ColliderItem>,
    grids: Query<(Entity, &GlobalTransform2D, Option<&CollisionLayers>), With<TileCollider>>,
    shapeless: Query<Entity, (Added<KinematicBody>, Without<Collider>)>,
) {
    for entity in &shapeless {
        log::warn!("kinematic body {entity} has no `Collider`, so it does not move");
    }
    world.clear();
    for (entity, collider, global, layers, sensor, body) in &colliders {
        world.push(Entry {
            entity,
            shape: collider.world_shape(global.0),
            layers: layers.copied().unwrap_or_default(),
            sensor,
            body,
            swept: None,
        });
    }
    for (entity, global, layers) in &grids {
        world.push_grid(GridEntry {
            entity,
            offset: global.translation(),
            layers: layers.copied().unwrap_or_default(),
        });
    }
    world.rebuild();
}

/// Moves every kinematic body through the level.
pub(crate) fn move_bodies(
    time: Res<Time>,
    gravity: Res<Gravity>,
    world: Res<PhysicsWorld>,
    tiles: Query<&TileCollider>,
    mut bodies: Query<BodyItem>,
    parents: Query<&GlobalTransform2D, Without<KinematicBody>>,
) {
    let env = Env::new(&world, |entity| {
        tiles.get(entity).ok().map(|tile| &tile.grid)
    });
    let dt = time.fixed_delta_secs();
    for (mut body, mut contacts, collider, mut transform, mut global, layers, parent) in &mut bodies
    {
        let rect = collider.world_shape(global.0).aabb();
        let layers = layers.copied().unwrap_or_default();
        let moved = step_body(&env, layers, rect, &mut body, &mut contacts, gravity.0, dt);
        let delta = moved.min - rect.min;
        if delta == Vec2::ZERO {
            continue;
        }
        // The global transform moves now, so the overlap check and later systems in this
        // step see the new position; the local one moves by the same amount in the
        // parent's space.
        global.0.translation += delta;
        let local = match parent.and_then(|parent| parents.get(parent.parent()).ok()) {
            Some(parent) => parent.0.matrix2.inverse() * delta,
            None => delta,
        };
        transform.translation += local;
    }
}

/// Updates the bodies' shapes after moving them and reports overlaps that started or
/// ended.
pub(crate) fn detect(
    mut world: ResMut<PhysicsWorld>,
    bodies: Query<(Entity, &Collider, &GlobalTransform2D), With<KinematicBody>>,
    mut overlaps: ResMut<Overlaps>,
    mut started: MessageWriter<CollisionStarted>,
    mut ended: MessageWriter<CollisionEnded>,
) {
    for (entity, collider, global) in &bodies {
        world.move_body(entity, collider.world_shape(global.0));
    }
    world.rebuild();
    let (began, stopped) = overlaps.replace(world.overlapping_pairs());
    for pair in stopped {
        ended.write(CollisionEnded(pair.0, pair.1));
    }
    for pair in began {
        started.write(CollisionStarted(pair.0, pair.1));
    }
}
