//! 2D transforms: the local [`Transform2D`], the world-space [`GlobalTransform2D`], and the
//! system that propagates them down the parent/child hierarchy.

use bevy_ecs::change_detection::Tick;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemChangeTick;
use cg_core::glam::{Affine2, Vec2};
use serde::{Deserialize, Serialize};

/// Where an entity sits relative to its parent, or to the world if it has none: a
/// translation, a counter-clockwise rotation in radians and a scale, applied in the order
/// scale, rotate, translate.
///
/// The world is y-up. Adding a `Transform2D` also adds a [`GlobalTransform2D`], which the
/// engine computes in [`PostUpdate`](cg_core::PostUpdate) (and after every fixed step) from
/// the transforms of the entity and its ancestors.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_world::Transform2D;
///
/// let transform = Transform2D::from_xy(100.0, 20.0).with_scale(Vec2::splat(2.0));
/// assert_eq!(transform.transform_point(vec2(1.0, 0.0)), vec2(102.0, 20.0));
/// ```
///
/// In prefabs every field is optional: `"Transform2D": (translation: (0, 14))`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[require(GlobalTransform2D)]
#[serde(default)]
pub struct Transform2D {
    /// Position relative to the parent, in world units.
    pub translation: Vec2,
    /// Counter-clockwise rotation in radians.
    pub rotation: f32,
    /// Size multiplier along the local axes. Negative values mirror.
    pub scale: Vec2,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform2D {
    /// No translation, no rotation, a scale of one.
    pub const IDENTITY: Self = Self {
        translation: Vec2::ZERO,
        rotation: 0.0,
        scale: Vec2::ONE,
    };

    /// A transform that only translates, to `(x, y)`.
    pub const fn from_xy(x: f32, y: f32) -> Self {
        Self::from_translation(Vec2::new(x, y))
    }

    /// A transform that only translates.
    pub const fn from_translation(translation: Vec2) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    /// A transform that only rotates, by `radians` counter-clockwise.
    pub const fn from_rotation(radians: f32) -> Self {
        Self {
            rotation: radians,
            ..Self::IDENTITY
        }
    }

    /// A transform that only scales.
    pub const fn from_scale(scale: Vec2) -> Self {
        Self {
            scale,
            ..Self::IDENTITY
        }
    }

    /// This transform with another translation.
    pub const fn with_translation(mut self, translation: Vec2) -> Self {
        self.translation = translation;
        self
    }

    /// This transform with another rotation, in radians.
    pub const fn with_rotation(mut self, radians: f32) -> Self {
        self.rotation = radians;
        self
    }

    /// This transform with another scale.
    pub const fn with_scale(mut self, scale: Vec2) -> Self {
        self.scale = scale;
        self
    }

    /// The transform as an affine matrix.
    pub fn affine(&self) -> Affine2 {
        Affine2::from_scale_angle_translation(self.scale, self.rotation, self.translation)
    }

    /// Maps a point from this transform's local space into its parent's space.
    pub fn transform_point(&self, point: Vec2) -> Vec2 {
        self.affine().transform_point2(point)
    }

    /// Rotates by `radians` counter-clockwise, on top of the current rotation.
    pub fn rotate(&mut self, radians: f32) {
        self.rotation += radians;
    }
}

impl From<Transform2D> for Affine2 {
    fn from(transform: Transform2D) -> Self {
        transform.affine()
    }
}

/// An entity's transform in world space, combining its [`Transform2D`] with those of all
/// its ancestors. The engine writes it; game code reads it.
///
/// It is brought up to date by [`TransformSystems::Propagate`] before and after each fixed
/// step and in [`PostUpdate`](cg_core::PostUpdate). Gameplay in
/// [`FixedUpdate`](cg_core::FixedUpdate) sees where things were at the start of the step.
/// A child whose parent has no `Transform2D` is not updated.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct GlobalTransform2D(pub Affine2);

impl GlobalTransform2D {
    /// No translation, rotation or scale.
    pub const IDENTITY: Self = Self(Affine2::IDENTITY);

    /// The affine matrix.
    pub fn affine(&self) -> Affine2 {
        self.0
    }

    /// The world position of the entity's origin.
    pub fn translation(&self) -> Vec2 {
        self.0.translation
    }

    /// The world rotation in radians, read from the direction of the local x axis.
    pub fn rotation(&self) -> f32 {
        self.0.matrix2.x_axis.to_angle()
    }

    /// The world scale along the local axes. Mirroring shows up as a negative x scale.
    pub fn scale(&self) -> Vec2 {
        self.0.to_scale_angle_translation().0
    }

    /// Maps a point from the entity's local space into world space.
    pub fn transform_point(&self, point: Vec2) -> Vec2 {
        self.0.transform_point2(point)
    }

    /// The transform of `child` (local to this entity) in world space.
    pub fn mul_transform(&self, child: &Transform2D) -> Self {
        Self(self.0 * child.affine())
    }
}

impl From<Transform2D> for GlobalTransform2D {
    fn from(transform: Transform2D) -> Self {
        Self(transform.affine())
    }
}

impl From<GlobalTransform2D> for Affine2 {
    fn from(transform: GlobalTransform2D) -> Self {
        transform.0
    }
}

/// System sets for transform work. [`WorldPlugin`](crate::WorldPlugin) runs
/// [`Propagate`](Self::Propagate) in [`FixedPreUpdate`](cg_core::FixedPreUpdate),
/// [`FixedPostUpdate`](cg_core::FixedPostUpdate) and [`PostUpdate`](cg_core::PostUpdate);
/// order systems that read [`GlobalTransform2D`] after it.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransformSystems {
    /// Computes [`GlobalTransform2D`] from [`Transform2D`] down the hierarchy.
    Propagate,
}

/// A hierarchy root: an entity with a transform and no parent.
type Root = (
    Entity,
    Ref<'static, Transform2D>,
    &'static mut GlobalTransform2D,
    Option<&'static Children>,
);

/// An entity with a parent.
type Node = (
    Ref<'static, Transform2D>,
    &'static mut GlobalTransform2D,
    Option<&'static Children>,
    Ref<'static, ChildOf>,
);

/// When transforms were last propagated. [`propagate_transforms`] runs in several
/// schedules; sharing this tick means each change is handled by the first run after it
/// rather than by every run. [`WorldPlugin`](crate::WorldPlugin) adds it.
#[derive(Resource, Debug, Default)]
pub struct TransformPropagation {
    last: Option<Tick>,
}

/// Recomputes [`GlobalTransform2D`] for every entity whose transform, parent or ancestors
/// changed since transforms were last propagated.
#[profiling::function]
pub fn propagate_transforms(
    mut roots: Query<Root, Without<ChildOf>>,
    mut nodes: Query<Node>,
    mut orphaned: RemovedComponents<ChildOf>,
    mut propagation: ResMut<TransformPropagation>,
    ticks: SystemChangeTick,
    mut stack: Local<Vec<(Entity, Affine2, bool)>>,
) {
    let since = propagation.last.unwrap_or(ticks.last_run());
    let this_run = ticks.this_run();
    propagation.last = Some(this_run);
    let newer = |tick: Tick| tick.is_newer_than(since, this_run);

    // Entities that lost their parent are roots now, with an unchanged local transform.
    let orphaned: Vec<Entity> = orphaned.read().collect();
    for (entity, transform, mut global, children) in &mut roots {
        let changed = newer(transform.last_changed()) || orphaned.contains(&entity);
        if changed {
            *global = GlobalTransform2D(transform.affine());
        }
        if let Some(children) = children {
            stack.extend(children.iter().map(|child| (child, global.0, changed)));
        }
    }
    while let Some((entity, parent, parent_changed)) = stack.pop() {
        let Ok((transform, mut global, children, child_of)) = nodes.get_mut(entity) else {
            continue;
        };
        let changed =
            parent_changed || newer(transform.last_changed()) || newer(child_of.last_changed());
        if changed {
            *global = GlobalTransform2D(parent * transform.affine());
        }
        if let Some(children) = children {
            stack.extend(children.iter().map(|child| (child, global.0, changed)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::schedule::Schedule;
    use cg_core::glam::vec2;
    use core::f32::consts::FRAC_PI_2;

    fn close(a: Vec2, b: Vec2) -> bool {
        a.distance(b) < 1e-4
    }

    fn world_and_schedule() -> (World, Schedule) {
        let mut schedule = Schedule::default();
        schedule.add_systems(propagate_transforms);
        let mut world = World::new();
        world.init_resource::<TransformPropagation>();
        (world, schedule)
    }

    #[test]
    fn separate_runs_share_the_work_but_miss_nothing() {
        // Two copies of the system, as in the fixed and frame schedules.
        let (mut world, mut first) = world_and_schedule();
        let (_, mut second) = world_and_schedule();
        let entity = world.spawn(Transform2D::from_xy(1.0, 0.0)).id();
        first.run(&mut world);
        // A global written by hand stays: the second run sees no new change...
        world.get_mut::<GlobalTransform2D>(entity).unwrap().0 = Affine2::IDENTITY;
        second.run(&mut world);
        assert_eq!(
            world
                .get::<GlobalTransform2D>(entity)
                .unwrap()
                .translation(),
            Vec2::ZERO
        );
        // ...but a change made after the first run is picked up by the second.
        world.get_mut::<Transform2D>(entity).unwrap().translation.x = 5.0;
        second.run(&mut world);
        assert_eq!(
            world
                .get::<GlobalTransform2D>(entity)
                .unwrap()
                .translation(),
            vec2(5.0, 0.0)
        );
    }

    #[test]
    fn transform_applies_scale_then_rotation_then_translation() {
        let transform = Transform2D {
            translation: vec2(10.0, 0.0),
            rotation: FRAC_PI_2,
            scale: vec2(2.0, 1.0),
        };
        assert!(close(
            transform.transform_point(vec2(1.0, 0.0)),
            vec2(10.0, 2.0)
        ));
    }

    #[test]
    fn children_follow_their_parents() {
        let (mut world, mut schedule) = world_and_schedule();
        let parent = world
            .spawn(Transform2D::from_xy(100.0, 0.0).with_rotation(FRAC_PI_2))
            .id();
        let child = world
            .spawn((Transform2D::from_xy(10.0, 0.0), ChildOf(parent)))
            .id();
        let grandchild = world
            .spawn((Transform2D::from_xy(0.0, 5.0), ChildOf(child)))
            .id();
        schedule.run(&mut world);

        let global = |world: &World, e| *world.get::<GlobalTransform2D>(e).unwrap();
        assert!(close(
            global(&world, child).translation(),
            vec2(100.0, 10.0)
        ));
        assert!(close(
            global(&world, grandchild).translation(),
            vec2(95.0, 10.0)
        ));
        assert!((global(&world, grandchild).rotation() - FRAC_PI_2).abs() < 1e-5);

        // Moving the parent moves the whole subtree on the next run.
        world.get_mut::<Transform2D>(parent).unwrap().translation = vec2(0.0, 0.0);
        schedule.run(&mut world);
        assert!(close(
            global(&world, grandchild).translation(),
            vec2(-5.0, 10.0)
        ));
    }

    #[test]
    fn reparenting_and_orphaning_update_globals() {
        let (mut world, mut schedule) = world_and_schedule();
        let a = world.spawn(Transform2D::from_xy(100.0, 0.0)).id();
        let b = world.spawn(Transform2D::from_xy(0.0, 100.0)).id();
        let child = world
            .spawn((Transform2D::from_xy(1.0, 1.0), ChildOf(a)))
            .id();
        schedule.run(&mut world);
        let translation =
            |world: &World| world.get::<GlobalTransform2D>(child).unwrap().translation();
        assert_eq!(translation(&world), vec2(101.0, 1.0));

        world.entity_mut(child).insert(ChildOf(b));
        schedule.run(&mut world);
        assert_eq!(translation(&world), vec2(1.0, 101.0));

        world.entity_mut(child).remove::<ChildOf>();
        schedule.run(&mut world);
        assert_eq!(translation(&world), vec2(1.0, 1.0));
    }

    #[test]
    fn unchanged_entities_are_left_alone() {
        let (mut world, mut schedule) = world_and_schedule();
        let entity = world.spawn(Transform2D::from_xy(1.0, 2.0)).id();
        schedule.run(&mut world);
        // A global written by hand survives while nothing changes...
        world.get_mut::<GlobalTransform2D>(entity).unwrap().0 = Affine2::IDENTITY;
        schedule.run(&mut world);
        assert_eq!(
            world
                .get::<GlobalTransform2D>(entity)
                .unwrap()
                .translation(),
            Vec2::ZERO
        );
        // ...and is recomputed once the transform changes.
        world.get_mut::<Transform2D>(entity).unwrap().rotate(0.0);
        schedule.run(&mut world);
        assert_eq!(
            world
                .get::<GlobalTransform2D>(entity)
                .unwrap()
                .translation(),
            vec2(1.0, 2.0)
        );
    }

    #[test]
    fn global_decomposes_scale_and_rotation() {
        let global = GlobalTransform2D::from(
            Transform2D::from_xy(3.0, 4.0)
                .with_rotation(0.5)
                .with_scale(vec2(2.0, 3.0)),
        );
        assert!(close(global.scale(), vec2(2.0, 3.0)));
        assert!((global.rotation() - 0.5).abs() < 1e-5);
        assert_eq!(global.translation(), vec2(3.0, 4.0));
    }

    #[test]
    fn transform_round_trips_through_ron_with_defaults() {
        let transform: Transform2D = ron::from_str("(translation: (0, 14))").unwrap();
        assert_eq!(transform, Transform2D::from_xy(0.0, 14.0));
        let text = ron::to_string(&transform).unwrap();
        assert_eq!(ron::from_str::<Transform2D>(&text).unwrap(), transform);
    }
}
