//! Camera effects: following a target with a dead zone and level bounds, and shake.

use crate::rng::noise;
use bevy_ecs::prelude::*;
use cg_core::glam::{Affine2, Vec2, uvec2, vec2};
use cg_core::{Rect, Time};
use cg_platform::Window;
use cg_render::{Camera2D, CameraView, FrameView};
use cg_world::{GlobalTransform2D, Interpolated, Transform2D};
use serde::{Deserialize, Serialize};

/// Moves the camera entity after a target: the target may move freely inside a dead zone
/// around the middle of the view; beyond it the camera follows, smoothly, and never shows
/// anything outside the bounds (a level's rectangle).
///
/// It runs in fixed steps after the physics ([`AnimSystems::Camera`](crate::AnimSystems::Camera)),
/// so give the camera [`Interpolated`] for smooth motion. It moves the camera's
/// [`Transform2D`], so the camera should have no parent. Each frame the drawn view is kept
/// inside the bounds as well, so a zoom tween or [`CameraShake`] never shows past them.
///
/// ```
/// use cg_anim::CameraFollow;
/// use cg_core::prelude::*;
/// use cg_core::Rect;
/// use cg_render::Camera2D;
/// use cg_world::{Interpolated, Transform2D};
///
/// fn spawn_camera(mut commands: Commands, player: Single<Entity, With<Name>>) {
///     commands.spawn((
///         Camera2D::default(),
///         Transform2D::default(),
///         Interpolated::default(),
///         CameraFollow::new(*player)
///             .with_dead_zone(vec2(48.0, 64.0))
///             .with_bounds(Rect::new(0.0, 0.0, 1920.0, 540.0)),
///     ));
/// }
/// ```
#[derive(Component, Debug, Clone, PartialEq)]
pub struct CameraFollow {
    /// The entity to follow; the camera stays put without one.
    pub target: Option<Entity>,
    /// Added to the target's position: `(0, 24)` keeps more of the view above it.
    pub offset: Vec2,
    /// The size of the box around the middle of the view that the target may move in
    /// without the camera following. Zero follows every move.
    pub dead_zone: Vec2,
    /// Seconds for the camera to cover most (63%) of the way to where it should be; zero
    /// snaps.
    pub smoothing: f32,
    /// The world rectangle the view must stay inside. A view larger than the bounds is
    /// centred on them.
    pub bounds: Option<Rect>,
    snap: bool,
}

impl Default for CameraFollow {
    fn default() -> Self {
        Self {
            target: None,
            offset: Vec2::ZERO,
            dead_zone: vec2(32.0, 48.0),
            smoothing: 0.12,
            bounds: None,
            snap: true,
        }
    }
}

impl CameraFollow {
    /// Follows `target`, starting with the camera on it.
    pub fn new(target: Entity) -> Self {
        Self {
            target: Some(target),
            ..Self::default()
        }
    }

    /// Uses `size` for the dead zone.
    pub fn with_dead_zone(mut self, size: Vec2) -> Self {
        self.dead_zone = size;
        self
    }

    /// Uses `seconds` of smoothing.
    pub fn with_smoothing(mut self, seconds: f32) -> Self {
        self.smoothing = seconds;
        self
    }

    /// Keeps the view inside `bounds`.
    pub fn with_bounds(mut self, bounds: Rect) -> Self {
        self.bounds = Some(bounds);
        self
    }

    /// Follows the target's position plus `offset`.
    pub fn with_offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }

    /// Jumps straight to the target at the next step instead of gliding there, after a
    /// respawn, say. Also skips the camera's interpolation for that step.
    pub fn snap(&mut self) {
        self.snap = true;
    }

    /// Where the camera goes from `camera` this step, before the bounds: the target
    /// pushes the dead zone, and smoothing eases towards that.
    fn next_position(&self, camera: Vec2, target: Vec2, dt: f32) -> Vec2 {
        let target = target + self.offset;
        if self.snap {
            return target;
        }
        let half = self.dead_zone.max(Vec2::ZERO) / 2.0;
        let delta = target - camera;
        let push = delta - delta.clamp(-half, half);
        let desired = camera + push;
        if self.smoothing <= 0.0 {
            desired
        } else {
            camera + (desired - camera) * (1.0 - (-dt / self.smoothing).exp())
        }
    }
}

/// `position` moved so a view of `size` around it stays inside `bounds`.
fn clamp_view(position: Vec2, size: Vec2, bounds: Rect) -> Vec2 {
    let half = size / 2.0;
    let axis = |p: f32, half: f32, min: f32, max: f32| {
        if max - min <= 2.0 * half {
            (min + max) / 2.0
        } else {
            p.clamp(min + half, max - half)
        }
    };
    vec2(
        axis(position.x, half.x, bounds.min.x, bounds.max.x),
        axis(position.y, half.y, bounds.min.y, bounds.max.y),
    )
}

/// Moves each [`CameraFollow`] camera for one fixed step.
pub(crate) fn follow_targets(
    time: Res<Time>,
    window: Option<Res<Window>>,
    targets: Query<&GlobalTransform2D>,
    mut cameras: Query<(
        &mut CameraFollow,
        &mut Transform2D,
        &Camera2D,
        Option<&mut Interpolated>,
    )>,
) {
    let dt = time.fixed_delta_secs();
    for (mut follow, mut transform, camera, interpolated) in &mut cameras {
        let Some(target) = follow.target.and_then(|t| targets.get(t).ok()) else {
            continue;
        };
        let mut position = follow.next_position(transform.translation, target.translation(), dt);
        if let Some(bounds) = follow.bounds {
            let view = CameraView::from_transform(camera.clone(), Affine2::IDENTITY);
            let visible = match &window {
                Some(window) => view.visible_rect(window),
                None => view.visible_rect_in(uvec2(1280, 720), 1.0),
            };
            position = clamp_view(position, visible.size(), bounds);
        }
        if follow.snap {
            follow.snap = false;
            if let Some(mut interpolated) = interpolated {
                interpolated.reset();
            }
        }
        if transform.translation != position {
            transform.translation = position;
        }
    }
}

/// Shakes the view of the camera it is on: add trauma on hits and landings, and the view
/// jitters by up to `max_offset` and `max_rotation` times trauma squared, so small knocks
/// barely show and big ones shake hard. Trauma wears off at `decay` per second.
///
/// The shake moves only what is drawn, not the camera's transform, so following,
/// [`MainCamera`](cg_render::MainCamera) and pointer picking ignore it. It follows smooth
/// noise in time, so it looks the same at any frame rate.
///
/// ```
/// use cg_anim::CameraShake;
/// use cg_core::prelude::*;
///
/// fn on_landing(mut shake: Single<&mut CameraShake>) {
///     shake.add_trauma(0.4);
/// }
/// ```
///
/// In prefabs: `"CameraShake": (max_offset: (8, 6), decay: 1.5)`.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraShake {
    /// How hard it shakes now, from 0 to 1.
    pub trauma: f32,
    /// Trauma lost per second.
    pub decay: f32,
    /// The largest offset in world units, at full trauma.
    pub max_offset: Vec2,
    /// The largest turn in radians, at full trauma.
    pub max_rotation: f32,
    /// How fast it jitters, in noise cells per second.
    pub frequency: f32,
    /// Picks the noise, so two cameras shake differently.
    pub seed: u32,
}

impl Default for CameraShake {
    fn default() -> Self {
        Self {
            trauma: 0.0,
            decay: 1.5,
            max_offset: vec2(10.0, 8.0),
            max_rotation: 0.03,
            frequency: 20.0,
            seed: 0,
        }
    }
}

impl CameraShake {
    /// Adds trauma, up to 1.
    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).clamp(0.0, 1.0);
    }

    /// The offset and turn at `seconds` with the current trauma.
    pub fn offset(&self, seconds: f32) -> (Vec2, f32) {
        let strength = self.trauma * self.trauma;
        if strength <= 0.0 {
            return (Vec2::ZERO, 0.0);
        }
        let x = seconds * self.frequency;
        let seed = self.seed.wrapping_mul(3);
        let offset = vec2(noise(seed, x), noise(seed + 1, x)) * self.max_offset * strength;
        (offset, noise(seed + 2, x) * self.max_rotation * strength)
    }
}

/// Wears trauma off every frame.
pub(crate) fn decay_shake(time: Res<Time>, mut shakes: Query<&mut CameraShake>) {
    let dt = time.delta_secs();
    for mut shake in &mut shakes {
        if shake.trauma > 0.0 {
            shake.trauma = (shake.trauma - shake.decay * dt).max(0.0);
        }
    }
}

/// A camera's view effects.
type ViewEffects<'a> = (
    &'a Camera2D,
    Option<&'a CameraShake>,
    Option<&'a CameraFollow>,
);

/// Cameras with at least one view effect.
type WithViewEffects = Or<(With<CameraShake>, With<CameraFollow>)>;

/// Adjusts this frame's view, before anything is culled or queued: moves it by the active
/// camera's shake, then keeps it inside its [`CameraFollow::bounds`]. The follow keeps
/// the camera itself inside in fixed steps; this also covers what changes between them,
/// such as a zoom tween widening the view, and the shake.
pub(crate) fn adjust_view(
    time: Res<Time>,
    window: Res<Window>,
    cameras: Query<ViewEffects, WithViewEffects>,
    mut view: ResMut<FrameView>,
) {
    // Wrapped so the noise input keeps its precision in long sessions.
    let seconds = (time.elapsed_secs() % 10_000.0) as f32;
    let Some((_, shake, follow)) = cameras.iter().find(|(camera, ..)| camera.active) else {
        return;
    };
    let mut adjusted = view.0.clone();
    if let Some(shake) = shake {
        let (offset, turn) = shake.offset(seconds);
        adjusted.position += offset;
        adjusted.rotation += turn;
    }
    if let Some(bounds) = follow.and_then(|f| f.bounds) {
        let size = adjusted.visible_rect(&window).size();
        adjusted.position = clamp_view(adjusted.position, size, bounds);
    }
    if adjusted != view.0 {
        view.0 = adjusted;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dead_zone_lets_the_target_move_before_the_camera_follows() {
        let follow = CameraFollow {
            snap: false,
            smoothing: 0.0,
            dead_zone: vec2(40.0, 20.0),
            ..CameraFollow::default()
        };
        let camera = vec2(100.0, 50.0);
        assert_eq!(
            follow.next_position(camera, vec2(115.0, 55.0), 0.1),
            camera,
            "inside"
        );
        assert_eq!(
            follow.next_position(camera, vec2(130.0, 30.0), 0.1),
            vec2(110.0, 40.0),
            "pushed by the edge of the zone"
        );
        let snapping = CameraFollow::default().with_offset(vec2(0.0, 10.0));
        assert_eq!(
            snapping.next_position(camera, vec2(0.0, 0.0), 0.1),
            vec2(0.0, 10.0)
        );
    }

    #[test]
    fn smoothing_eases_towards_the_target() {
        let follow = CameraFollow {
            snap: false,
            smoothing: 0.1,
            dead_zone: Vec2::ZERO,
            ..CameraFollow::default()
        };
        let next = follow.next_position(Vec2::ZERO, vec2(100.0, 0.0), 0.1);
        assert!((next.x - 63.212).abs() < 0.01, "{next}");
        // The same distance covered in two half steps.
        let half = follow.next_position(Vec2::ZERO, vec2(100.0, 0.0), 0.05);
        let two = follow.next_position(half, vec2(100.0, 0.0), 0.05);
        assert!((two.x - next.x).abs() < 0.01);
    }

    #[test]
    fn bounds_keep_the_view_inside() {
        let bounds = Rect::new(0.0, 0.0, 1000.0, 300.0);
        let view = vec2(640.0, 360.0);
        assert_eq!(
            clamp_view(vec2(100.0, 100.0), view, bounds),
            vec2(320.0, 150.0)
        );
        assert_eq!(
            clamp_view(vec2(990.0, 100.0), view, bounds),
            vec2(680.0, 150.0)
        );
        assert_eq!(clamp_view(vec2(500.0, 100.0), view, bounds).x, 500.0);
    }

    #[test]
    fn shake_grows_with_trauma_squared_and_stays_in_range() {
        let mut shake = CameraShake::default();
        assert_eq!(shake.offset(1.0), (Vec2::ZERO, 0.0));
        shake.add_trauma(0.5);
        let mut largest = 0.0f32;
        for i in 0..1000 {
            let (offset, turn) = shake.offset(i as f32 * 0.01);
            assert!(offset.x.abs() <= 10.0 * 0.25 + 1e-4 && offset.y.abs() <= 8.0 * 0.25 + 1e-4);
            assert!(turn.abs() <= 0.03 * 0.25 + 1e-6);
            largest = largest.max(offset.x.abs());
        }
        assert!(largest > 1.0, "it does shake: {largest}");
        shake.add_trauma(5.0);
        assert_eq!(shake.trauma, 1.0);
    }
}
