//! Kinematic bodies: [`KinematicBody`], what they touch ([`KinematicContacts`]), and the
//! movement that sweeps them through tiles and colliders.

use crate::EPSILON;
use crate::layers::CollisionLayers;
use crate::shape::spans_overlap;
use crate::world::Env;
use bevy_ecs::prelude::*;
use cg_core::Rect;
use cg_core::glam::{IVec2, Vec2, vec2};
use cg_world::{Interpolated, Transform2D};
use serde::{Deserialize, Serialize};

/// The steepest slope (rise over run) bodies follow smoothly: 45°.
const MAX_GRADIENT: f32 = 1.0;

/// The acceleration applied to every [`KinematicBody`], scaled by its
/// [`gravity_scale`](KinematicBody::gravity_scale), in world units per second squared.
/// Defaults to 980 downwards; set it to zero for top-down games.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Gravity(pub Vec2);

impl Default for Gravity {
    fn default() -> Self {
        Self(Vec2::new(0.0, -980.0))
    }
}

/// A body moved by its [`velocity`](Self::velocity) in each fixed step, colliding with
/// tiles and static colliders: platformer characters, enemies, top-down movers.
///
/// Each step the physics adds [`Gravity`], moves the body along x, then along y, and
/// records what it touched in its [`KinematicContacts`]. Every move is swept over the
/// whole distance, so nothing is skipped however fast the body goes. Hitting a wall,
/// floor or ceiling stops the velocity in that direction.
///
/// On the ground the body follows slopes up and down and stays on the ground over small
/// drops (see [`snap_distance`](Self::snap_distance)). Standing on a static collider that
/// game code moves (a moving platform), the body is carried with it; a platform rising
/// into the body from below lifts it, and a solid one moving into it from the side pushes
/// it (as far as walls allow). Set the velocity from game code in
/// [`FixedUpdate`](cg_core::FixedUpdate); the physics moves the body in
/// [`FixedPostUpdate`](cg_core::FixedPostUpdate), and the contacts are ready in the next
/// step.
///
/// The body's shape is the bounding box of the [`Collider`](crate::Collider) on the same
/// entity; without one it does not move. Bodies do not block each other, but their
/// overlaps are reported (see [`CollisionStarted`](crate::CollisionStarted)). A body is
/// [`Interpolated`], so it draws smoothly between steps; after teleporting one, call
/// [`Interpolated::reset`].
///
/// ```
/// use cg_core::prelude::*;
/// use cg_physics::{KinematicBody, KinematicContacts};
/// use cg_platform::input::ActionState;
///
/// const RUN: f32 = 120.0;
/// const JUMP: f32 = 330.0;
///
/// fn control(actions: Res<ActionState>, mut player: Query<(&mut KinematicBody, &KinematicContacts)>) {
///     for (mut body, contacts) in &mut player {
///         body.velocity.x = actions.axis("left", "right") * RUN;
///         if contacts.on_ground() && actions.just_pressed("jump") {
///             if actions.pressed("down") {
///                 body.drop_through = true;
///             } else {
///                 body.velocity.y = JUMP;
///             }
///         }
///     }
/// }
/// ```
///
/// In prefabs: `"KinematicBody": (gravity_scale: 0.5)`.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[require(Transform2D, KinematicContacts, Interpolated)]
pub struct KinematicBody {
    /// World units per second.
    pub velocity: Vec2,
    /// How much of [`Gravity`] applies. 1 by default; 0 for flying or top-down bodies.
    pub gravity_scale: f32,
    /// The fastest the body falls, in world units per second. Unlimited by default.
    pub max_fall_speed: f32,
    /// How far the body may drop and still count as staying on the ground when it walks
    /// off a small step, in world units. 2 by default. Slopes up to 45° are followed
    /// whatever this is.
    pub snap_distance: f32,
    /// The height of ledges a grounded body walks up without jumping, in world units.
    /// 0 by default.
    pub step_height: f32,
    /// Set it to fall through one-way platforms during the next step, so the body drops
    /// off the platform it stands on; the physics clears it after that step. The body does
    /// not land on that platform again, since it is then below its top.
    #[serde(skip)]
    pub drop_through: bool,
}

impl Default for KinematicBody {
    fn default() -> Self {
        Self {
            velocity: Vec2::ZERO,
            gravity_scale: 1.0,
            max_fall_speed: f32::INFINITY,
            snap_distance: 2.0,
            step_height: 0.0,
            drop_through: false,
        }
    }
}

impl KinematicBody {
    /// A body at rest, with the default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// The same body with another velocity.
    pub fn with_velocity(mut self, velocity: Vec2) -> Self {
        self.velocity = velocity;
        self
    }

    /// The same body with another gravity scale.
    pub fn with_gravity_scale(mut self, scale: f32) -> Self {
        self.gravity_scale = scale;
        self
    }
}

/// Something a [`KinematicBody`] touched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    /// The collider's entity: for tiles, the entity with the
    /// [`TileCollider`](crate::TileCollider).
    pub entity: Entity,
    /// The tile, for tile colliders.
    pub cell: Option<IVec2>,
    /// The surface normal, pointing out of what was touched.
    pub normal: Vec2,
}

/// What a [`KinematicBody`] touched in the latest physics step. Added with the body.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct KinematicContacts {
    /// What it stands on.
    pub ground: Option<Contact>,
    /// What its head hit while moving up.
    pub ceiling: Option<Contact>,
    /// The wall that stopped it moving left.
    pub wall_left: Option<Contact>,
    /// The wall that stopped it moving right.
    pub wall_right: Option<Contact>,
    /// Whether it overlaps a hazard tile, or touches a solid one.
    pub hazard: bool,
    /// Whether it stood on something in the step before.
    pub was_on_ground: bool,
}

impl KinematicContacts {
    /// Whether it stands on something.
    pub fn on_ground(&self) -> bool {
        self.ground.is_some()
    }

    /// Whether its head hit something.
    pub fn on_ceiling(&self) -> bool {
        self.ceiling.is_some()
    }

    /// Whether a wall stopped it, on either side.
    pub fn on_wall(&self) -> bool {
        self.wall_left.is_some() || self.wall_right.is_some()
    }

    /// Whether it landed in this step.
    pub fn just_landed(&self) -> bool {
        self.on_ground() && !self.was_on_ground
    }

    /// Whether it left the ground in this step, by jumping, falling or walking off.
    pub fn just_left_ground(&self) -> bool {
        !self.on_ground() && self.was_on_ground
    }

    /// The normal of the ground it stands on; straight up when in the air.
    pub fn ground_normal(&self) -> Vec2 {
        self.ground.map_or(Vec2::Y, |ground| ground.normal)
    }

    /// Whether it stands on a slope.
    pub fn on_slope(&self) -> bool {
        self.ground_normal().x.abs() > 1e-4
    }
}

/// How far the floor search reaches from the body's bottom.
struct Reach {
    /// Below the bottom.
    down: f32,
    /// Above the bottom, for flat tops.
    flat: f32,
    /// Above the bottom, for slope surfaces under the foot.
    slope: f32,
}

struct Floor {
    height: f32,
    contact: Contact,
}

fn shifted(rect: Rect, by: Vec2) -> Rect {
    Rect {
        min: rect.min + by,
        max: rect.max + by,
    }
}

/// The longest move made at once. Faster bodies move in several sub-steps, so ground
/// following on slopes sees each part of the way; sweeps keep even the longest sub-step
/// from passing through anything.
const MAX_SUBSTEP: f32 = 8.0;
/// The most sub-steps per step.
const MAX_SUBSTEPS: usize = 32;

/// Runs one physics step for a body whose world bounding box is `rect`, and returns its
/// new bounding box.
pub(crate) fn step_body(
    env: &Env,
    layers: CollisionLayers,
    rect: Rect,
    body: &mut KinematicBody,
    contacts: &mut KinematicContacts,
    gravity: Vec2,
    dt: f32,
) -> Rect {
    let rect = follow_platforms(env, layers, rect, contacts);
    let mut mover = Mover {
        env,
        layers,
        step_height: body.step_height,
        snap_distance: body.snap_distance,
        grounded: contacts.on_ground(),
        one_way: !core::mem::take(&mut body.drop_through),
        contacts: KinematicContacts {
            was_on_ground: contacts.on_ground(),
            ..Default::default()
        },
    };
    body.velocity += gravity * body.gravity_scale * dt;
    body.velocity.y = body.velocity.y.max(-body.max_fall_speed);
    let delta = body.velocity * dt;
    let substeps = ((delta.length() / MAX_SUBSTEP).ceil() as usize).clamp(1, MAX_SUBSTEPS);
    let mut part = delta / substeps as f32;
    let mut rect = rect;
    for _ in 0..substeps {
        rect = mover.substep(rect, part);
        let found = &mover.contacts;
        if found.on_wall() {
            part.x = 0.0;
            body.velocity.x = 0.0;
        }
        if found.on_ceiling() {
            part.y = part.y.min(0.0);
            body.velocity.y = body.velocity.y.min(0.0);
        }
        if found.on_ground() {
            part.y = part.y.max(0.0);
            body.velocity.y = body.velocity.y.max(0.0);
        }
    }
    mover.contacts.hazard = touches_hazard(env, layers, rect, &mover.contacts);
    *contacts = mover.contacts;
    rect
}

/// Moves `rect` with the static collider it stands on, if that moved in this step, and out
/// of solid colliders that moved into it from the side.
fn follow_platforms(
    env: &Env,
    layers: CollisionLayers,
    rect: Rect,
    contacts: &KinematicContacts,
) -> Rect {
    let mut rect = rect;
    let platform = contacts
        .ground
        .filter(|ground| ground.cell.is_none())
        .and_then(|ground| env.world.entry(ground.entity))
        .filter(|entry| entry.is_static() && entry.motion != Vec2::ZERO);
    if let Some(platform) = platform {
        let motion = platform.motion;
        let dy = if motion.y > 0.0 {
            sweep_up(env, layers, rect, motion.y).0
        } else {
            motion.y
        };
        rect = shifted(rect, vec2(0.0, dy));
        if motion.x != 0.0 {
            rect = shifted(rect, vec2(sweep_x(env, layers, rect, motion.x, 0.0).0, 0.0));
        }
    }
    // The furthest push from solid colliders that came in from the side in this step.
    let mut push = 0.0f32;
    let mut buffer = Vec::new();
    for entry in env.world.near(rect, &mut buffer) {
        if !entry.is_solid() || entry.motion.x == 0.0 || !layers.interacts(entry.layers) {
            continue;
        }
        let b = entry.shape.aabb();
        if !spans_overlap(b.min.x, b.max.x, rect.min.x, rect.max.x)
            || !spans_overlap(b.min.y, b.max.y, rect.min.y, rect.max.y)
        {
            continue;
        }
        let before = shifted(b, -entry.motion);
        let d = if entry.motion.x > 0.0 {
            if before.max.x > rect.min.x + EPSILON {
                continue;
            }
            b.max.x - rect.min.x
        } else {
            if before.min.x < rect.max.x - EPSILON {
                continue;
            }
            b.min.x - rect.max.x
        };
        if d.abs() > push.abs() {
            push = d;
        }
    }
    if push != 0.0 {
        rect = shifted(rect, vec2(sweep_x(env, layers, rect, push, 0.0).0, 0.0));
    }
    rect
}

/// Moves one body through its sub-steps, gathering contacts.
struct Mover<'e, 'a> {
    env: &'e Env<'a>,
    layers: CollisionLayers,
    step_height: f32,
    snap_distance: f32,
    /// Whether the body stood on something after the previous sub-step.
    grounded: bool,
    one_way: bool,
    contacts: KinematicContacts,
}

impl Mover<'_, '_> {
    /// Moves `rect` by `delta`: along x, then along y (or onto the floor).
    fn substep(&mut self, rect: Rect, delta: Vec2) -> Rect {
        let (env, layers) = (self.env, self.layers);
        // While grounded, the bottom of the box may run into terrain the foot will climb:
        // a step, or, near slopes, the plateau at the top of the slope under the box's
        // leading corner, which can be as high as the slope rises under half the box and
        // the move.
        let lift = if self.grounded {
            let span = Rect {
                min: vec2(rect.min.x + delta.x.min(0.0), rect.min.y),
                max: vec2(rect.max.x + delta.x.max(0.0), rect.max.y),
            };
            let steepest = steepest_slope_near(env, layers, span);
            let reach = rect.width() * 0.5 + delta.x.abs();
            self.step_height.max(reach * steepest)
        } else {
            0.0
        };

        let mut rect = rect;
        let mut moved_x = 0.0;
        if delta.x != 0.0 {
            let (dx, wall) = sweep_x(env, layers, rect, delta.x, lift);
            moved_x = dx;
            rect = shifted(rect, vec2(dx, 0.0));
            if let Some(wall) = wall {
                self.hit_wall(delta.x, wall);
            }
        }

        if delta.y > 0.0 {
            let (dy, ceiling) = sweep_up(env, layers, rect, delta.y);
            rect = shifted(rect, vec2(0.0, dy));
            if ceiling.is_some() {
                self.contacts.ceiling = ceiling;
            }
            self.contacts.ground = None;
            self.grounded = false;
            return rect;
        }

        let snap = if self.grounded {
            self.snap_distance
        } else {
            0.0
        };
        let grounded = self.grounded;
        let reach = |dx: f32| {
            let follow = dx.abs() * MAX_GRADIENT;
            Reach {
                down: -delta.y + snap + if grounded { follow } else { 0.0 },
                flat: lift + EPSILON,
                slope: follow + lift + EPSILON,
            }
        };
        let mut floor = find_floor(env, layers, rect, reach(moved_x), self.one_way);
        // A floor above the bottom lifts the body (up a slope or a step). If that would
        // push its head into a ceiling, the gap is too low: undo the horizontal move.
        if let Some(f) = &floor
            && moved_x != 0.0
            && f.height > rect.min.y
            && sweep_up(env, layers, rect, f.height - rect.min.y)
                .1
                .is_some()
        {
            let wall = Contact {
                normal: vec2(-moved_x.signum(), 0.0),
                ..f.contact
            };
            rect = shifted(rect, vec2(-moved_x, 0.0));
            self.hit_wall(moved_x, wall);
            floor = find_floor(env, layers, rect, reach(0.0), self.one_way);
        }
        match floor {
            Some(floor) => {
                rect = shifted(rect, vec2(0.0, floor.height - rect.min.y));
                self.contacts.ground = Some(floor.contact);
            }
            None => {
                rect = shifted(rect, vec2(0.0, delta.y));
                self.contacts.ground = None;
            }
        }
        self.grounded = self.contacts.ground.is_some();
        rect
    }

    fn hit_wall(&mut self, dx: f32, wall: Contact) {
        if dx > 0.0 {
            self.contacts.wall_right = Some(wall);
        } else {
            self.contacts.wall_left = Some(wall);
        }
    }
}

/// The steepest slope tile near the bottom of `span`, over every grid it interacts with.
fn steepest_slope_near(env: &Env, layers: CollisionLayers, span: Rect) -> f32 {
    env.grids
        .iter()
        .filter(|grid| layers.interacts(grid.layers))
        .map(|grid| grid.grid.steepest_slope_near(shifted(span, -grid.offset)))
        .fold(0.0, f32::max)
}

/// How far `rect` moves along x by `dx`, and what stopped it. With `lift`, obstacles
/// whose tops are no more than that above the bottom are ignored.
fn sweep_x(
    env: &Env,
    layers: CollisionLayers,
    rect: Rect,
    dx: f32,
    lift: f32,
) -> (f32, Option<Contact>) {
    let bottom = rect.min.y + lift;
    let probe = Rect {
        min: vec2(rect.min.x, bottom),
        max: vec2(rect.max.x, rect.max.y.max(bottom + 2.0 * EPSILON)),
    };
    let closer = |d: f32, best: f32| if dx > 0.0 { d < best } else { d > best };
    let normal = vec2(-dx.signum(), 0.0);
    let mut allowed = dx;
    let mut hit = None;
    for grid in &env.grids {
        if !layers.interacts(grid.layers) {
            continue;
        }
        if let Some((d, cell)) = grid.grid.sweep_x(shifted(probe, -grid.offset), dx)
            && closer(d, allowed)
        {
            allowed = d;
            hit = Some(Contact {
                entity: grid.entity,
                cell: Some(cell),
                normal,
            });
        }
    }
    let span = Rect {
        min: vec2(probe.min.x.min(probe.min.x + dx), probe.min.y),
        max: vec2(probe.max.x.max(probe.max.x + dx), probe.max.y),
    };
    let mut buffer = Vec::new();
    for entry in env.world.near(span, &mut buffer) {
        if !entry.is_solid() || !layers.interacts(entry.layers) {
            continue;
        }
        let b = entry.shape.aabb();
        if !spans_overlap(b.min.y, b.max.y, probe.min.y, probe.max.y) {
            continue;
        }
        let d = if dx > 0.0 {
            if b.min.x < rect.max.x - EPSILON {
                continue;
            }
            b.min.x - rect.max.x
        } else {
            if b.max.x > rect.min.x + EPSILON {
                continue;
            }
            b.max.x - rect.min.x
        };
        if closer(d, allowed) {
            allowed = d;
            hit = Some(Contact {
                entity: entry.entity,
                cell: None,
                normal,
            });
        }
    }
    (allowed, hit)
}

/// How far `rect` moves up by `dy`, and what stopped it.
fn sweep_up(env: &Env, layers: CollisionLayers, rect: Rect, dy: f32) -> (f32, Option<Contact>) {
    let mut allowed = dy;
    let mut hit = None;
    for grid in &env.grids {
        if !layers.interacts(grid.layers) {
            continue;
        }
        if let Some((d, cell)) = grid.grid.sweep_up(shifted(rect, -grid.offset), dy)
            && d < allowed
        {
            allowed = d;
            hit = Some(Contact {
                entity: grid.entity,
                cell: Some(cell),
                normal: Vec2::NEG_Y,
            });
        }
    }
    let span = Rect {
        min: rect.min,
        max: rect.max + vec2(0.0, dy),
    };
    let mut buffer = Vec::new();
    for entry in env.world.near(span, &mut buffer) {
        if !entry.is_solid() || !layers.interacts(entry.layers) {
            continue;
        }
        let b = entry.shape.aabb();
        if b.min.y < rect.max.y - EPSILON
            || !spans_overlap(b.min.x, b.max.x, rect.min.x, rect.max.x)
        {
            continue;
        }
        let d = b.min.y - rect.max.y;
        if d < allowed {
            allowed = d;
            hit = Some(Contact {
                entity: entry.entity,
                cell: None,
                normal: Vec2::NEG_Y,
            });
        }
    }
    (allowed, hit)
}

/// The highest floor within reach of `rect`'s bottom.
fn find_floor(
    env: &Env,
    layers: CollisionLayers,
    rect: Rect,
    reach: Reach,
    one_way: bool,
) -> Option<Floor> {
    let bottom = rect.min.y;
    let (lo, hi_flat, hi_slope) = (
        bottom - reach.down,
        bottom + reach.flat,
        bottom + reach.slope,
    );
    let mut best: Option<Floor> = None;
    let mut consider = |height: f32, contact: Contact| {
        if best.as_ref().is_none_or(|b| height > b.height) {
            best = Some(Floor { height, contact });
        }
    };
    for grid in &env.grids {
        if !layers.interacts(grid.layers) {
            continue;
        }
        let dy = grid.offset.y;
        if let Some(floor) = grid.grid.floor(
            shifted(rect, -grid.offset),
            lo - dy,
            hi_flat - dy,
            hi_slope - dy,
            one_way,
        ) {
            consider(
                floor.height + dy,
                Contact {
                    entity: grid.entity,
                    cell: Some(floor.cell),
                    normal: floor.normal,
                },
            );
        }
    }
    // Up to the body's top, for colliders that rose into it in this step.
    let span = Rect {
        min: vec2(rect.min.x, lo),
        max: vec2(rect.max.x, hi_flat.max(rect.max.y)),
    };
    let mut buffer = Vec::new();
    for entry in env.world.near(span, &mut buffer) {
        if !entry.is_static() || (entry.one_way && !one_way) || !layers.interacts(entry.layers) {
            continue;
        }
        let b = entry.shape.aabb();
        let top = b.max.y;
        // A collider that rose counts if its top was within reach before it moved: it
        // came up under the body and lifts it.
        let rise = entry.motion.y.max(0.0);
        if lo <= top
            && top - rise <= hi_flat
            && spans_overlap(b.min.x, b.max.x, rect.min.x, rect.max.x)
        {
            consider(
                top,
                Contact {
                    entity: entry.entity,
                    cell: None,
                    normal: Vec2::Y,
                },
            );
        }
    }
    best
}

/// Whether `rect` overlaps a hazard tile or one of the contacts is a hazard tile.
fn touches_hazard(
    env: &Env,
    layers: CollisionLayers,
    rect: Rect,
    contacts: &KinematicContacts,
) -> bool {
    let overlapped = env.grids.iter().any(|grid| {
        layers.interacts(grid.layers) && grid.grid.hazard_in(shifted(rect, -grid.offset)).is_some()
    });
    let touched = [
        contacts.ground,
        contacts.ceiling,
        contacts.wall_left,
        contacts.wall_right,
    ]
    .into_iter()
    .flatten()
    .any(|contact| {
        contact.cell.is_some_and(|cell| {
            env.grid(contact.entity)
                .is_some_and(|grid| grid.grid.get(cell).hazard)
        })
    });
    overlapped || touched
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::TileGrid;
    use crate::shape::WorldShape;
    use crate::world::{Entry, GridEntry, PhysicsWorld};

    const DT: f32 = 1.0 / 60.0;
    const GRAVITY: Vec2 = Vec2::new(0.0, -980.0);
    /// A 16-unit grid; bodies are 10 wide and 14 tall.
    const CELL: f32 = 16.0;

    fn grid_entity() -> Entity {
        Entity::from_raw_u32(100).unwrap()
    }

    struct Level {
        world: PhysicsWorld,
        grid: TileGrid,
    }

    impl Level {
        fn new(text: &str) -> Self {
            Self::with_colliders(text, &[])
        }

        fn with_colliders(text: &str, crates: &[Rect]) -> Self {
            let mut world = PhysicsWorld::default();
            world.push_grid(GridEntry {
                entity: grid_entity(),
                offset: Vec2::ZERO,
                layers: CollisionLayers::DEFAULT,
            });
            for (i, rect) in crates.iter().enumerate() {
                world.push(Entry {
                    entity: Entity::from_raw_u32(i as u32 + 1).unwrap(),
                    shape: WorldShape::Rect(*rect),
                    layers: CollisionLayers::DEFAULT,
                    sensor: false,
                    body: false,
                    one_way: false,
                    motion: Vec2::ZERO,
                    swept: None,
                });
            }
            world.rebuild();
            Self {
                world,
                grid: TileGrid::parse(Vec2::splat(CELL), text),
            }
        }

        fn env(&self) -> Env<'_> {
            Env::new(&self.world, |_| Some(&self.grid))
        }
    }

    /// A body standing with its bottom-left corner at `(x, y)`.
    struct Runner {
        rect: Rect,
        body: KinematicBody,
        contacts: KinematicContacts,
    }

    impl Runner {
        fn at(x: f32, y: f32) -> Self {
            Self {
                rect: Rect::from_min_size(vec2(x, y), vec2(10.0, 14.0)),
                body: KinematicBody::default(),
                contacts: KinematicContacts::default(),
            }
        }

        fn step(&mut self, level: &Level) {
            self.rect = step_body(
                &level.env(),
                CollisionLayers::DEFAULT,
                self.rect,
                &mut self.body,
                &mut self.contacts,
                GRAVITY,
                DT,
            );
        }

        fn run(&mut self, level: &Level, steps: usize, vx: f32) {
            for _ in 0..steps {
                self.body.velocity.x = vx;
                self.step(level);
            }
        }

        fn foot(&self) -> Vec2 {
            vec2(self.rect.center().x, self.rect.min.y)
        }
    }

    #[test]
    fn falling_bodies_land_and_stop() {
        let level = Level::new(
            "
            ....
            ....
            ####
            ",
        );
        let mut runner = Runner::at(20.0, 40.0);
        for _ in 0..60 {
            runner.step(&level);
        }
        assert_eq!(runner.rect.min.y, 16.0);
        assert!(runner.contacts.on_ground() && !runner.contacts.just_landed());
        assert_eq!(runner.body.velocity.y, 0.0);
        let ground = runner.contacts.ground.unwrap();
        assert_eq!((ground.entity, ground.normal), (grid_entity(), Vec2::Y));
    }

    #[test]
    fn nothing_tunnels_at_any_speed() {
        // A floor and a wall one cell thick.
        let level = Level::new(
            "
            ..........#.....
            ..........#.....
            ################
            ",
        );
        let mut runner = Runner::at(20.0, 16.0);
        runner.contacts.ground = Some(Contact {
            entity: grid_entity(),
            cell: None,
            normal: Vec2::Y,
        });
        // 100,000 units per second is about 1,700 units, or 100 cells, per step.
        runner.run(&level, 1, 100_000.0);
        assert_eq!(runner.rect.max.x, 160.0, "stopped by the wall");
        assert!(runner.contacts.wall_right.is_some() && runner.body.velocity.x == 0.0);
        // Straight down from high above at the same speed.
        let mut faller = Runner::at(40.0, 1000.0);
        faller.body.velocity.y = -100_000.0;
        faller.step(&level);
        assert_eq!(faller.rect.min.y, 16.0);
        assert!(faller.contacts.just_landed());
        // And up into the ceiling of a closed box.
        let boxed = Level::new(
            "
            #####
            #...#
            #...#
            #####
            ",
        );
        let mut jumper = Runner::at(20.0, 16.0);
        jumper.body.velocity.y = 100_000.0;
        jumper.step(&boxed);
        assert_eq!(jumper.rect.max.y, 48.0);
        assert!(jumper.contacts.on_ceiling() && jumper.body.velocity.y <= 0.0);
    }

    #[test]
    fn one_way_platforms_only_stop_falls() {
        let level = Level::new(
            "
            ......
            ......
            ..--..
            ......
            ######
            ",
        );
        // Jumping up from below goes through the platform (top at 48) and lands on it.
        let mut runner = Runner::at(36.0, 16.0);
        runner.body.velocity.y = 400.0;
        let mut above = false;
        for _ in 0..90 {
            runner.step(&level);
            above |= runner.rect.min.y > 48.0;
        }
        assert!(above);
        assert_eq!(runner.rect.min.y, 48.0, "landed on the platform");
        // Walking sideways through the platform's level is not blocked.
        let mut walker = Runner::at(2.0, 40.0);
        walker.body.gravity_scale = 0.0;
        walker.run(&level, 30, 120.0);
        assert!(walker.rect.min.x > 60.0 && !walker.contacts.on_wall());
        // Dropping through: the body falls to the ground and does not land on the way.
        runner.body.drop_through = true;
        for _ in 0..60 {
            runner.step(&level);
        }
        assert_eq!(runner.rect.min.y, 16.0);
    }

    #[test]
    fn walls_and_ledges() {
        let level = Level::new(
            "
            .......#
            .......#
            ###....#
            ########
            ",
        );
        // Walk off the ledge (top 32) and fall to the floor (top 16), then into the wall.
        let mut runner = Runner::at(10.0, 32.0);
        runner.step(&level);
        assert!(runner.contacts.on_ground());
        runner.run(&level, 4, 60.0);
        assert!(runner.contacts.on_ground(), "still on the ledge");
        runner.run(&level, 120, 60.0);
        assert_eq!(runner.rect.min.y, 16.0);
        assert_eq!(runner.rect.max.x, 112.0);
        assert!(runner.contacts.wall_right.is_some() && runner.contacts.on_ground());
        assert_eq!(runner.contacts.wall_right.unwrap().normal, -Vec2::X);
        // Back to the left: stopped by the ledge's side, since it is too high to step up.
        runner.run(&level, 90, -60.0);
        assert_eq!(runner.rect.min.x, 48.0);
        assert!(runner.contacts.wall_left.is_some());
        // With a step height that covers the ledge, the body walks up it.
        runner.body.step_height = 16.0;
        runner.run(&level, 10, -60.0);
        assert_eq!(runner.rect.min.y, 32.0);
    }

    #[test]
    fn small_drops_keep_the_body_grounded() {
        let level = Level::new(
            "
            ..........
            ##........
            ##########
            ",
        );
        // Ledge top 32, floor top 16: a full cell down is too far to snap.
        let mut runner = Runner::at(10.0, 32.0);
        runner.step(&level);
        let mut airborne = false;
        for _ in 0..45 {
            runner.run(&level, 1, 90.0);
            airborne |= !runner.contacts.on_ground();
        }
        assert!(airborne && runner.contacts.on_ground());
        // A 2-unit step down (static crates) keeps it on the ground.
        let level = Level::with_colliders(
            "
            ..........
            ##########
            ",
            &[Rect::new(0.0, 16.0, 40.0, 18.0)],
        );
        let mut runner = Runner::at(10.0, 18.0);
        runner.step(&level);
        for _ in 0..30 {
            runner.run(&level, 1, 90.0);
            assert!(runner.contacts.on_ground(), "at x {}", runner.rect.min.x);
        }
        assert_eq!(runner.rect.min.y, 16.0);
    }

    /// Walks a body across `level` from `x` and checks that it stays on the ground with
    /// its foot on `surface`.
    fn walk_surface(
        level: &Level,
        x: f32,
        y: f32,
        vx: f32,
        steps: usize,
        surface: impl Fn(f32) -> f32,
    ) {
        let mut runner = Runner::at(x, y);
        runner.step(level);
        for step in 0..steps {
            runner.run(level, 1, vx);
            let foot = runner.foot();
            assert!(
                runner.contacts.on_ground(),
                "step {step}: airborne at {foot}"
            );
            assert!(!runner.contacts.on_wall(), "step {step}: stuck at {foot}");
            let expected = surface(foot.x);
            assert!(
                (foot.y - expected).abs() < 0.05,
                "step {step}: foot at {foot}, surface at {expected}"
            );
        }
    }

    #[test]
    fn bodies_walk_over_45_degree_hills() {
        // Floor top 16; the hill rises from x 48 to 64, plateau to 96, down to 112.
        let level = Level::new(
            "
            ................
            .../##\\.........
            ################
            ",
        );
        let hill = |x: f32| 16.0 + (x - 48.0).clamp(0.0, 16.0) - (x - 96.0).clamp(0.0, 16.0);
        walk_surface(&level, 2.0, 16.0, 120.0, 70, hill);
        walk_surface(&level, 200.0, 16.0, -120.0, 70, hill);
        // Fast enough to cross a whole cell in one step.
        walk_surface(&level, 2.0, 16.0, 900.0, 12, hill);
    }

    #[test]
    fn bodies_walk_over_22_degree_hills() {
        let level = Level::new(
            "
            ....................
            ...ab##BA...........
            ####################
            ",
        );
        let hill =
            |x: f32| 16.0 + (x - 48.0).clamp(0.0, 32.0) * 0.5 - (x - 112.0).clamp(0.0, 32.0) * 0.5;
        walk_surface(&level, 2.0, 16.0, 120.0, 90, hill);
        walk_surface(&level, 250.0, 16.0, -120.0, 110, hill);
    }

    #[test]
    fn bodies_land_on_slopes_and_jump_off_them() {
        let level = Level::new(
            "
            ........
            ........
            .../....
            ########
            ",
        );
        // Dropped above the middle of the slope: lands with the foot on the surface.
        let mut runner = Runner::at(51.0, 60.0);
        for _ in 0..60 {
            runner.step(&level);
        }
        assert!(runner.contacts.on_ground() && runner.contacts.on_slope());
        assert!((runner.foot().y - 24.0).abs() < 0.05, "{}", runner.foot());
        // A jump leaves the slope.
        runner.body.velocity.y = 200.0;
        runner.step(&level);
        assert!(!runner.contacts.on_ground() && runner.contacts.just_left_ground());
        // Standing still on a slope does not slide.
        for _ in 0..120 {
            runner.step(&level);
        }
        assert!(
            (runner.foot() - vec2(56.0, 24.0)).length() < 0.05,
            "{}",
            runner.foot()
        );
    }

    #[test]
    fn low_ceilings_stop_climbs() {
        // The ceiling's underside is at 32 from x 64, the slope starts at x 96: a body 14
        // tall walks under the ceiling on the floor (16 to 30) and must stop once the
        // slope would lift its head into the ceiling.
        let level = Level::new(
            "
            ....######
            ....../###
            ##########
            ",
        );
        let mut runner = Runner::at(10.0, 16.0);
        runner.step(&level);
        runner.run(&level, 120, 60.0);
        assert!(runner.rect.max.y <= 32.0 + 1e-3, "{:?}", runner.rect);
        assert!(
            runner.contacts.wall_right.is_some(),
            "{:?}",
            runner.contacts
        );
        assert!(runner.contacts.on_ground());
    }

    #[test]
    fn crates_block_like_tiles() {
        let level = Level::with_colliders(
            "
            ..........
            ##########
            ",
            &[Rect::new(60.0, 16.0, 76.0, 32.0)],
        );
        let mut runner = Runner::at(10.0, 16.0);
        runner.step(&level);
        runner.run(&level, 60, 120.0);
        assert_eq!(runner.rect.max.x, 60.0);
        let wall = runner.contacts.wall_right.unwrap();
        assert_eq!(
            (wall.entity, wall.cell),
            (Entity::from_raw_u32(1).unwrap(), None)
        );
        // Jump onto it.
        runner.body.velocity.y = 300.0;
        for _ in 0..60 {
            runner.body.velocity.x = 30.0;
            runner.step(&level);
            if runner.rect.min.x > 62.0 && runner.contacts.on_ground() {
                break;
            }
        }
        assert_eq!(runner.rect.min.y, 32.0, "standing on the crate");
    }

    #[test]
    fn hazards_are_reported() {
        let level = Level::new(
            "
            ......
            ...^..
            ######
            ",
        );
        let mut runner = Runner::at(10.0, 16.0);
        runner.step(&level);
        assert!(!runner.contacts.hazard);
        runner.run(&level, 40, 60.0);
        assert!(runner.contacts.hazard, "walked into the spikes");
        let lava = Level::new(
            "
            ......
            !!!!!!
            ",
        );
        let mut runner = Runner::at(10.0, 40.0);
        for _ in 0..60 {
            runner.step(&lava);
        }
        assert!(
            runner.contacts.on_ground() && runner.contacts.hazard,
            "standing on lava"
        );
    }

    #[test]
    fn layers_decide_what_blocks() {
        let level = Level::new(
            "
            ....
            ####
            ",
        );
        let mut ghost = Runner::at(10.0, 30.0);
        for _ in 0..10 {
            ghost.rect = step_body(
                &level.env(),
                CollisionLayers::new(2, 2),
                ghost.rect,
                &mut ghost.body,
                &mut ghost.contacts,
                GRAVITY,
                DT,
            );
        }
        assert!(
            ghost.rect.min.y < 16.0,
            "fell through the grid on another layer"
        );
    }
}
