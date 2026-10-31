//! Smooth drawing of entities that move in fixed steps: the opt-in [`Interpolated`]
//! component and [`InterpolatedTransforms`], which tells the renderer where to draw.

use crate::GlobalTransform2D;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use cg_core::Time;
use cg_core::glam::Affine2;

/// Draws an entity smoothly between fixed steps.
///
/// Gameplay runs at a fixed rate (60 Hz by default) while the display may refresh faster
/// or slower. An entity moved in [`FixedUpdate`](cg_core::FixedUpdate) would visibly jump
/// once per step; with `Interpolated` it is drawn between its last two fixed-step
/// positions, blended by [`Time::alpha`]. That costs one fixed step of display latency.
///
/// Add it to things that move in `FixedUpdate`: the player, enemies, moving platforms and
/// a camera that follows them. Their children are drawn with them. Things moved in
/// [`Update`](cg_core::Update) should not have it.
///
/// After a teleport, call [`reset`](Self::reset) so the entity is not drawn sliding across
/// the level:
///
/// ```
/// use cg_core::prelude::*;
/// use cg_world::{Interpolated, Transform2D};
///
/// fn respawn(mut player: Query<(&mut Transform2D, &mut Interpolated)>) {
///     for (mut transform, mut interpolated) in &mut player {
///         transform.translation = vec2(32.0, 64.0);
///         interpolated.reset();
///     }
/// }
/// ```
///
/// In prefabs: `"Interpolated": ()`.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct Interpolated {
    previous: Option<Affine2>,
    reset: bool,
}

impl Interpolated {
    /// Draws the entity at its current transform until the next fixed step, instead of
    /// blending from where it was.
    pub fn reset(&mut self) {
        self.reset = true;
    }

    /// The world transform at the start of the latest fixed step, if a step has run since
    /// the entity was spawned or reset.
    pub fn previous(&self) -> Option<Affine2> {
        self.previous
    }

    /// The transform to draw: `current` blended from the previous step by `alpha`.
    pub fn blend(&self, current: Affine2, alpha: f32) -> Affine2 {
        match self.previous {
            Some(previous) => lerp_affine(previous, current, alpha),
            None => current,
        }
    }
}

/// Turns smaller than this (about 3 degrees) are blended as plain matrices: the result is
/// at most 0.03% smaller than a true rotation, and the blend is several times cheaper.
const SMALL_TURN: f32 = 0.05;

/// Blends two affine transforms. Translation, scale and rotation (the short way round) are
/// interpolated separately. When the rotation changes little, the matrices are blended
/// directly, which is cheaper and also keeps any skew from non-uniformly scaled parents.
pub fn lerp_affine(from: Affine2, to: Affine2, t: f32) -> Affine2 {
    if from == to {
        return to;
    }
    let (a, b) = (from.matrix2.x_axis, to.matrix2.x_axis);
    let turn = a.perp_dot(b).atan2(a.dot(b));
    if turn.abs() < SMALL_TURN {
        return Affine2 {
            matrix2: from.matrix2 + (to.matrix2 - from.matrix2) * t,
            translation: from.translation.lerp(to.translation, t),
        };
    }
    let (from_scale, from_angle, from_translation) = from.to_scale_angle_translation();
    let (to_scale, _, to_translation) = to.to_scale_angle_translation();
    Affine2::from_scale_angle_translation(
        from_scale.lerp(to_scale, t),
        from_angle + turn * t,
        from_translation.lerp(to_translation, t),
    )
}

/// Records where each [`Interpolated`] entity is before a fixed step moves it. Runs in
/// [`FixedPreUpdate`](cg_core::FixedPreUpdate), after a transform propagation.
#[profiling::function]
pub fn snapshot_interpolated(mut query: Query<(&GlobalTransform2D, &mut Interpolated)>) {
    for (global, mut interpolated) in &mut query {
        interpolated.previous = Some(global.0);
        interpolated.reset = false;
    }
}

/// Applies [`Interpolated::reset`] once the new transform has propagated.
#[profiling::function]
pub fn apply_interpolation_resets(mut query: Query<(&GlobalTransform2D, &mut Interpolated)>) {
    for (global, mut interpolated) in &mut query {
        if interpolated.reset {
            interpolated.previous = Some(global.0);
            interpolated.reset = false;
        }
    }
}

/// Where entities should be drawn this frame: their [`GlobalTransform2D`], blended between
/// fixed steps for [`Interpolated`] entities and their descendants.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_world::InterpolatedTransforms;
///
/// fn draw(things: Query<Entity>, transforms: InterpolatedTransforms) {
///     for entity in &things {
///         if let Some(affine) = transforms.get(entity) {
///             // draw at `affine`
///         }
///     }
/// }
/// ```
#[derive(SystemParam)]
pub struct InterpolatedTransforms<'w, 's> {
    time: Res<'w, Time>,
    nodes: Query<
        'w,
        's,
        (
            &'static GlobalTransform2D,
            Option<&'static Interpolated>,
            Option<&'static ChildOf>,
        ),
    >,
}

impl InterpolatedTransforms<'_, '_> {
    /// The fraction of a fixed step that has passed since the latest one.
    pub fn alpha(&self) -> f32 {
        self.time.alpha()
    }

    /// Where to draw `entity`, or `None` if it has no [`GlobalTransform2D`].
    pub fn get(&self, entity: Entity) -> Option<Affine2> {
        let (global, interpolated, parent) = self.nodes.get(entity).ok()?;
        Some(self.blend(global.0, interpolated, parent))
    }

    /// Where to draw an entity whose components the caller already has. `parent` is used
    /// to find an [`Interpolated`] ancestor.
    pub fn blend(
        &self,
        global: Affine2,
        interpolated: Option<&Interpolated>,
        parent: Option<&ChildOf>,
    ) -> Affine2 {
        if let Some(interpolated) = interpolated {
            return interpolated.blend(global, self.alpha());
        }
        // A child is drawn where it sits relative to its nearest interpolated ancestor.
        let mut next = parent.map(ChildOf::parent);
        while let Some(ancestor) = next {
            let Ok((ancestor_global, interpolated, parent)) = self.nodes.get(ancestor) else {
                break;
            };
            if let Some(interpolated) = interpolated {
                if interpolated.previous.is_none() {
                    break;
                }
                let smooth = interpolated.blend(ancestor_global.0, self.alpha());
                return smooth * ancestor_global.0.inverse() * global;
            }
            next = parent.map(ChildOf::parent);
        }
        global
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Transform2D, WorldPlugin};
    use cg_core::App;
    use cg_core::glam::{Vec2, vec2};
    use core::f32::consts::PI;
    use core::time::Duration;

    #[test]
    fn lerp_blends_translation_rotation_and_scale() {
        let from = Transform2D::from_xy(0.0, 0.0).affine();
        let to = Transform2D::from_xy(10.0, 20.0)
            .with_rotation(1.0)
            .with_scale(vec2(3.0, 3.0))
            .affine();
        let (scale, angle, translation) = lerp_affine(from, to, 0.5).to_scale_angle_translation();
        assert!(translation.distance(vec2(5.0, 10.0)) < 1e-5);
        assert!((angle - 0.5).abs() < 1e-5);
        assert!(scale.distance(vec2(2.0, 2.0)) < 1e-5);
    }

    #[test]
    fn small_turns_blend_matrices_closely() {
        let from = Transform2D::from_rotation(0.0).affine();
        let to = Transform2D::from_rotation(0.04).affine();
        let middle = lerp_affine(from, to, 0.5);
        assert!((middle.matrix2.x_axis.to_angle() - 0.02).abs() < 1e-5);
        assert!((middle.matrix2.x_axis.length() - 1.0).abs() < 3e-4);
    }

    #[test]
    fn lerp_turns_the_short_way() {
        let from = Transform2D::from_rotation(PI - 0.1).affine();
        let to = Transform2D::from_rotation(-PI + 0.1).affine();
        let middle = lerp_affine(from, to, 0.5);
        assert!((middle.matrix2.x_axis.to_angle().abs() - PI).abs() < 1e-4);
    }

    /// An app with one interpolated entity moving 10 units per fixed step, plus a child.
    fn app() -> (App, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins(WorldPlugin);
        app.add_systems(
            cg_core::FixedUpdate,
            |mut q: Query<&mut Transform2D, With<Interpolated>>| {
                for mut transform in &mut q {
                    transform.translation.x += 10.0;
                }
            },
        );
        let mover = app
            .world_mut()
            .spawn((Transform2D::default(), Interpolated::default()))
            .id();
        let child = app
            .world_mut()
            .spawn((Transform2D::from_xy(0.0, 5.0), ChildOf(mover)))
            .id();
        (app, mover, child)
    }

    fn drawn(app: &mut App, entity: Entity) -> Vec2 {
        let mut state =
            bevy_ecs::system::SystemState::<InterpolatedTransforms>::new(app.world_mut());
        state
            .get(app.world())
            .unwrap()
            .get(entity)
            .unwrap()
            .translation
    }

    #[test]
    fn interpolated_entities_and_children_are_drawn_between_steps() {
        let (mut app, mover, child) = app();
        let step = app.world().resource::<Time>().fixed_delta();
        app.update(Duration::ZERO);
        // One and a half steps: one step runs, half a step is left over.
        app.update(step + step / 2);
        assert_eq!(
            app.world()
                .get::<GlobalTransform2D>(mover)
                .unwrap()
                .translation(),
            vec2(10.0, 0.0)
        );
        assert!(drawn(&mut app, mover).distance(vec2(5.0, 0.0)) < 1e-3);
        assert!(drawn(&mut app, child).distance(vec2(5.0, 5.0)) < 1e-3);
    }

    #[test]
    fn reset_skips_the_blend() {
        let (mut app, mover, _) = app();
        let step = app.world().resource::<Time>().fixed_delta();
        app.update(Duration::ZERO);
        app.update(step + step / 2);
        app.world_mut()
            .get_mut::<Interpolated>(mover)
            .unwrap()
            .reset();
        app.update(Duration::ZERO);
        assert_eq!(drawn(&mut app, mover), vec2(10.0, 0.0));
    }
}
