//! Entity colliders: [`Collider`], its [`ColliderShape`], [`Sensor`]s, and [`WorldShape`],
//! a collider placed in the world, with the overlap and ray tests.

use crate::EPSILON;
use bevy_ecs::prelude::*;
use cg_core::Rect;
use cg_core::glam::{Affine2, Vec2};
use cg_world::Transform2D;
use serde::{Deserialize, Serialize};

/// The shape of a [`Collider`], in its entity's local units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ColliderShape {
    /// An axis-aligned rectangle of this full size, centred on the collider.
    Rect(Vec2),
    /// A circle of this radius.
    Circle(f32),
}

/// A collision shape on an entity, placed by its [`GlobalTransform2D`](cg_world::GlobalTransform2D).
///
/// What a collider does depends on the other components of its entity:
///
/// - On its own it is **static**: it blocks [`KinematicBody`](crate::KinematicBody)s, as
///   level geometry made of entities does (crates, doors, moving blocks).
/// - With a [`Sensor`] it blocks nothing and reports overlaps (coins, triggers, hurt
///   boxes).
/// - With a `KinematicBody` it is the body's shape, moved by the body.
///
/// Rectangles stay axis-aligned: the entity's rotation moves the
/// [`offset`](Self::offset) but does not turn the box. Scale applies (circles take the
/// larger axis). Kinematic bodies treat every collider, circles included, as its bounding
/// box.
///
/// [`CollisionLayers`](crate::CollisionLayers) on the same entity decide what it interacts
/// with.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_physics::{Collider, Sensor};
///
/// fn spawn(mut commands: Commands) {
///     // A crate the player can stand on.
///     commands.spawn((Collider::rect(18.0, 18.0), Transform2D::from_xy(100.0, 9.0)));
///     // A coin that reports when something touches it.
///     commands.spawn((Collider::circle(6.0), Sensor, Transform2D::from_xy(140.0, 40.0)));
/// }
/// # use cg_world::Transform2D;
/// ```
///
/// In prefabs: `"Collider": (shape: Rect((12, 16)), offset: (0, 8))`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[require(Transform2D)]
pub struct Collider {
    /// The shape.
    pub shape: ColliderShape,
    /// Where the shape's centre sits relative to the entity's origin.
    #[serde(default)]
    pub offset: Vec2,
}

impl Collider {
    /// A rectangle `width` by `height`, centred on the entity.
    pub const fn rect(width: f32, height: f32) -> Self {
        Self {
            shape: ColliderShape::Rect(Vec2::new(width, height)),
            offset: Vec2::ZERO,
        }
    }

    /// A circle of `radius`, centred on the entity.
    pub const fn circle(radius: f32) -> Self {
        Self {
            shape: ColliderShape::Circle(radius),
            offset: Vec2::ZERO,
        }
    }

    /// The same shape with its centre at `offset` from the entity's origin.
    pub const fn with_offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }

    /// The shape placed by a world transform: the offset is transformed, the size scaled.
    pub fn world_shape(&self, transform: Affine2) -> WorldShape {
        let center = transform.transform_point2(self.offset);
        let scale = Vec2::new(
            transform.matrix2.x_axis.length(),
            transform.matrix2.y_axis.length(),
        );
        match self.shape {
            ColliderShape::Rect(size) => {
                WorldShape::Rect(Rect::from_center_size(center, size * scale))
            }
            ColliderShape::Circle(radius) => WorldShape::Circle {
                center,
                radius: radius.abs() * scale.max_element(),
            },
        }
    }
}

/// Makes the entity's [`Collider`] a sensor: it blocks nothing, and overlaps with it are
/// reported as [`CollisionStarted`](crate::CollisionStarted) and
/// [`CollisionEnded`](crate::CollisionEnded) messages and in
/// [`Overlaps`](crate::Overlaps).
///
/// In prefabs: `"Sensor": ()`.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sensor;

/// A collider placed in the world, in world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorldShape {
    /// An axis-aligned rectangle.
    Rect(Rect),
    /// A circle.
    Circle {
        /// Its centre.
        center: Vec2,
        /// Its radius.
        radius: f32,
    },
}

impl WorldShape {
    /// The smallest axis-aligned rectangle around the shape.
    pub fn aabb(&self) -> Rect {
        match *self {
            Self::Rect(rect) => rect,
            Self::Circle { center, radius } => {
                Rect::from_center_size(center, Vec2::splat(radius * 2.0))
            }
        }
    }

    /// The shape's centre.
    pub fn center(&self) -> Vec2 {
        match *self {
            Self::Rect(rect) => rect.center(),
            Self::Circle { center, .. } => center,
        }
    }

    /// The same shape moved by `offset`.
    pub fn translated(&self, offset: Vec2) -> Self {
        match *self {
            Self::Rect(rect) => Self::Rect(Rect {
                min: rect.min + offset,
                max: rect.max + offset,
            }),
            Self::Circle { center, radius } => Self::Circle {
                center: center + offset,
                radius,
            },
        }
    }

    /// Whether the two shapes share area. Shapes that only touch do not overlap, so a body
    /// resting on a crate does not overlap it.
    pub fn overlaps(&self, other: &WorldShape) -> bool {
        match (*self, *other) {
            (Self::Rect(a), Self::Rect(b)) => rects_overlap(&a, &b),
            (
                Self::Circle {
                    center: a,
                    radius: ra,
                },
                Self::Circle {
                    center: b,
                    radius: rb,
                },
            ) => a.distance_squared(b) < (ra + rb) * (ra + rb),
            (Self::Rect(rect), Self::Circle { center, radius })
            | (Self::Circle { center, radius }, Self::Rect(rect)) => {
                center.clamp(rect.min, rect.max).distance_squared(center) < radius * radius
            }
        }
    }

    /// Whether `point` is inside the shape or on its edge.
    pub fn contains(&self, point: Vec2) -> bool {
        match *self {
            Self::Rect(rect) => rect.contains(point),
            Self::Circle { center, radius } => point.distance_squared(center) <= radius * radius,
        }
    }

    /// Where a ray from `origin` along the unit vector `direction` first enters the shape,
    /// within `max_distance`: the distance and the surface normal there. A ray starting
    /// inside hits at distance 0 with the normal facing back along the ray.
    pub fn raycast(&self, origin: Vec2, direction: Vec2, max_distance: f32) -> Option<(f32, Vec2)> {
        match *self {
            Self::Rect(rect) => ray_rect(origin, direction, max_distance, &rect),
            Self::Circle { center, radius } => {
                ray_circle(origin, direction, max_distance, center, radius)
            }
        }
    }
}

/// Whether two rectangles share area (more than [`EPSILON`] on both axes).
pub(crate) fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    spans_overlap(a.min.x, a.max.x, b.min.x, b.max.x)
        && spans_overlap(a.min.y, a.max.y, b.min.y, b.max.y)
}

/// Whether two intervals share more than [`EPSILON`].
pub(crate) fn spans_overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> bool {
    a0 < b1 - EPSILON && b0 < a1 - EPSILON
}

fn ray_rect(origin: Vec2, direction: Vec2, max_distance: f32, rect: &Rect) -> Option<(f32, Vec2)> {
    let inside = origin.x > rect.min.x
        && origin.x < rect.max.x
        && origin.y > rect.min.y
        && origin.y < rect.max.y;
    if inside {
        return Some((0.0, -direction));
    }
    let mut enter = f32::NEG_INFINITY;
    let mut exit = f32::INFINITY;
    let mut normal = Vec2::ZERO;
    for axis in 0..2 {
        let (o, d, lo, hi) = (
            origin[axis],
            direction[axis],
            rect.min[axis],
            rect.max[axis],
        );
        if d == 0.0 {
            // Parallel: the ray must run strictly between the two edges.
            if o <= lo || o >= hi {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((lo - o) / d, (hi - o) / d);
        let (near, far) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
        if near > enter {
            enter = near;
            normal = Vec2::ZERO;
            normal[axis] = -d.signum();
        }
        exit = exit.min(far);
    }
    (enter < exit && exit > 0.0 && enter >= 0.0 && enter <= max_distance).then_some((enter, normal))
}

fn ray_circle(
    origin: Vec2,
    direction: Vec2,
    max_distance: f32,
    center: Vec2,
    radius: f32,
) -> Option<(f32, Vec2)> {
    let m = origin - center;
    let c = m.length_squared() - radius * radius;
    if c < 0.0 {
        return Some((0.0, -direction));
    }
    let b = m.dot(direction);
    if b > 0.0 {
        return None;
    }
    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return None;
    }
    let t = -b - discriminant.sqrt();
    (t <= max_distance).then(|| (t, (origin + direction * t - center).normalize_or_zero()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::glam::vec2;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> WorldShape {
        WorldShape::Rect(Rect::new(x0, y0, x1, y1))
    }

    fn circle(x: f32, y: f32, radius: f32) -> WorldShape {
        WorldShape::Circle {
            center: vec2(x, y),
            radius,
        }
    }

    #[test]
    fn overlaps_need_shared_area() {
        let a = rect(0.0, 0.0, 10.0, 10.0);
        assert!(a.overlaps(&rect(5.0, 5.0, 15.0, 15.0)));
        // Touching edges do not count: a body standing on a crate is not inside it.
        assert!(!a.overlaps(&rect(10.0, 0.0, 20.0, 10.0)));
        assert!(!a.overlaps(&rect(0.0, 10.0, 10.0, 20.0)));
        assert!(circle(0.0, 0.0, 5.0).overlaps(&circle(9.0, 0.0, 5.0)));
        assert!(!circle(0.0, 0.0, 5.0).overlaps(&circle(10.0, 0.0, 5.0)));
        // A circle near a corner: inside the bounding boxes but not touching the corner.
        assert!(!a.overlaps(&circle(13.0, 13.0, 4.0)));
        assert!(a.overlaps(&circle(12.0, 5.0, 3.0)));
        assert!(circle(12.0, 5.0, 3.0).overlaps(&a));
        assert!(a.contains(vec2(10.0, 10.0)));
        assert!(!circle(0.0, 0.0, 1.0).contains(vec2(1.0, 0.1)));
    }

    #[test]
    fn colliders_follow_transforms() {
        let collider = Collider::rect(10.0, 4.0).with_offset(vec2(5.0, 0.0));
        let at = Affine2::from_scale_angle_translation(vec2(2.0, 2.0), 0.0, vec2(100.0, 50.0));
        assert_eq!(
            collider.world_shape(at),
            rect(100.0, 46.0, 120.0, 54.0),
            "the offset and size are scaled"
        );
        let turned = Affine2::from_angle_translation(core::f32::consts::FRAC_PI_2, Vec2::ZERO);
        let WorldShape::Rect(r) = collider.world_shape(turned) else {
            unreachable!()
        };
        // The offset turns with the entity; the box stays axis-aligned and keeps its size.
        assert!((r.center() - vec2(0.0, 5.0)).length() < 1e-5);
        assert!((r.size() - vec2(10.0, 4.0)).length() < 1e-5);
        let circle = Collider::circle(3.0).world_shape(Affine2::from_scale(vec2(1.0, 3.0)));
        assert_eq!(
            circle,
            super::WorldShape::Circle {
                center: Vec2::ZERO,
                radius: 9.0
            }
        );
    }

    #[test]
    fn rays_hit_rects_and_circles() {
        let block = rect(10.0, -5.0, 20.0, 5.0);
        assert_eq!(
            block.raycast(Vec2::ZERO, Vec2::X, 100.0),
            Some((10.0, -Vec2::X))
        );
        assert_eq!(block.raycast(Vec2::ZERO, Vec2::X, 9.0), None, "too short");
        assert_eq!(block.raycast(Vec2::ZERO, -Vec2::X, 100.0), None, "behind");
        assert_eq!(
            block.raycast(vec2(15.0, 20.0), -Vec2::Y, 100.0),
            Some((15.0, Vec2::Y))
        );
        // Grazing an edge is not a hit; starting inside is, at distance 0.
        assert_eq!(block.raycast(vec2(0.0, 5.0), Vec2::X, 100.0), None);
        assert_eq!(
            block.raycast(vec2(15.0, 0.0), Vec2::X, 100.0),
            Some((0.0, -Vec2::X))
        );
        let ball = circle(10.0, 0.0, 2.0);
        let (t, n) = ball.raycast(Vec2::ZERO, Vec2::X, 100.0).unwrap();
        assert!((t - 8.0).abs() < 1e-5 && (n + Vec2::X).length() < 1e-5);
        assert_eq!(ball.raycast(Vec2::ZERO, Vec2::Y, 100.0), None);
        assert_eq!(
            ball.raycast(vec2(10.0, 1.0), Vec2::Y, 100.0),
            Some((0.0, -Vec2::Y))
        );
    }
}
