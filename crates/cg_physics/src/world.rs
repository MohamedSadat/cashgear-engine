//! The colliders as of the latest physics step ([`PhysicsWorld`]), the spatial hash that
//! finds them, and [`SpatialQuery`], which game code uses for raycasts and overlap tests.

use crate::grid::{TileCell, TileCollider, TileGrid};
use crate::layers::CollisionLayers;
use crate::shape::WorldShape;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use cg_core::Rect;
use cg_core::glam::{IVec2, Vec2};
use std::collections::{BTreeSet, HashMap};

/// One entity collider as of the latest step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Entry {
    pub entity: Entity,
    pub shape: WorldShape,
    pub layers: CollisionLayers,
    pub sensor: bool,
    pub body: bool,
    /// A static collider that blocks only from above.
    pub one_way: bool,
    /// How far a static collider moved since the previous step.
    pub motion: Vec2,
    /// For a body that moved in this step, the box covering where it went.
    pub swept: Option<Rect>,
}

impl Entry {
    /// Static colliders block bodies; sensors and bodies do not.
    pub fn is_static(&self) -> bool {
        !self.sensor && !self.body
    }

    /// Static colliders that block bodies from the sides and from below too.
    pub fn is_solid(&self) -> bool {
        self.is_static() && !self.one_way
    }

    /// What overlaps with sensors and other bodies are tested against: the swept box of a
    /// body that moved, so a fast body does not skip a coin between two steps.
    fn reach(&self) -> WorldShape {
        self.swept.map_or(self.shape, WorldShape::Rect)
    }
}

/// One tile collider as of the latest step: where its grid sits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GridEntry {
    pub entity: Entity,
    pub offset: Vec2,
    pub layers: CollisionLayers,
}

/// The side of a spatial hash cell in world units.
const HASH_CELL: f32 = 64.0;
/// Colliders or queries covering more hash cells than this skip the hash.
const MAX_HASH_CELLS: i64 = 256;

/// Buckets of collider indices by the 64-unit cells their bounding boxes touch.
#[derive(Debug, Default)]
struct SpatialHash {
    cells: HashMap<IVec2, Vec<u32>>,
    /// Colliders too large to bucket, checked by every query.
    large: Vec<u32>,
}

impl SpatialHash {
    fn range(rect: Rect) -> Option<(IVec2, IVec2)> {
        let min = (rect.min / HASH_CELL).floor().as_ivec2();
        let max = (rect.max / HASH_CELL).floor().as_ivec2();
        let count = (max.x - min.x + 1) as i64 * (max.y - min.y + 1) as i64;
        (rect.min.is_finite() && rect.max.is_finite() && count <= MAX_HASH_CELLS)
            .then_some((min, max))
    }

    fn rebuild(&mut self, entries: &[Entry]) {
        self.cells.clear();
        self.large.clear();
        for (index, entry) in entries.iter().enumerate() {
            match Self::range(entry.reach().aabb()) {
                Some((min, max)) => {
                    for y in min.y..=max.y {
                        for x in min.x..=max.x {
                            self.cells
                                .entry(IVec2::new(x, y))
                                .or_default()
                                .push(index as u32);
                        }
                    }
                }
                None => self.large.push(index as u32),
            }
        }
    }

    /// The indices of every collider whose cells `rect` touches, sorted, each once.
    fn candidates(&self, rect: Rect, total: usize, out: &mut Vec<u32>) {
        out.clear();
        match Self::range(rect) {
            Some((min, max)) => {
                for y in min.y..=max.y {
                    for x in min.x..=max.x {
                        if let Some(bucket) = self.cells.get(&IVec2::new(x, y)) {
                            out.extend_from_slice(bucket);
                        }
                    }
                }
                out.extend_from_slice(&self.large);
                out.sort_unstable();
                out.dedup();
            }
            None => out.extend(0..total as u32),
        }
    }
}

/// Every collider as of the latest physics step, with a spatial hash to find them.
///
/// The physics systems rebuild it at the start of each step and update the bodies after
/// moving them, so game code reading it (through [`SpatialQuery`]) sees where things were
/// at the end of the latest step, not changes made since.
#[derive(Resource, Debug, Default)]
pub struct PhysicsWorld {
    entries: Vec<Entry>,
    index: HashMap<Entity, usize>,
    grids: Vec<GridEntry>,
    hash: SpatialHash,
    /// The centre of each static collider in the previous step, to find how far it moved.
    previous: HashMap<Entity, Vec2>,
}

impl PhysicsWorld {
    /// The number of entity colliders.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no entity colliders.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The number of tile colliders.
    pub fn grid_count(&self) -> usize {
        self.grids.len()
    }

    /// Where an entity's collider was at the end of the latest step.
    pub fn shape(&self, entity: Entity) -> Option<WorldShape> {
        self.index.get(&entity).map(|&i| self.entries[i].shape)
    }

    /// How far a static collider moved in the latest step (zero in the step it appeared).
    /// Bodies standing on it were carried by as much. `None` for sensors, bodies and
    /// unknown entities.
    pub fn motion(&self, entity: Entity) -> Option<Vec2> {
        let entry = &self.entries[*self.index.get(&entity)?];
        entry.is_static().then_some(entry.motion)
    }

    pub(crate) fn entry(&self, entity: Entity) -> Option<&Entry> {
        self.index.get(&entity).map(|&i| &self.entries[i])
    }

    /// Forgets the colliders, remembering where the static ones were.
    pub(crate) fn clear(&mut self) {
        self.previous.clear();
        for entry in &self.entries {
            if entry.is_static() {
                self.previous.insert(entry.entity, entry.shape.center());
            }
        }
        self.entries.clear();
        self.index.clear();
        self.grids.clear();
    }

    /// Adds a collider. A static one that was there in the previous step gets its motion.
    pub(crate) fn push(&mut self, mut entry: Entry) {
        if entry.is_static()
            && let Some(previous) = self.previous.get(&entry.entity)
        {
            entry.motion = entry.shape.center() - *previous;
        }
        self.index.insert(entry.entity, self.entries.len());
        self.entries.push(entry);
    }

    pub(crate) fn push_grid(&mut self, grid: GridEntry) {
        self.grids.push(grid);
    }

    /// Moves a body's collider to `shape`, remembering the box it swept from where it was.
    pub(crate) fn move_body(&mut self, entity: Entity, shape: WorldShape) {
        if let Some(&i) = self.index.get(&entity) {
            let entry = &mut self.entries[i];
            if entry.shape != shape {
                let (a, b) = (entry.shape.aabb(), shape.aabb());
                entry.swept = Some(Rect {
                    min: a.min.min(b.min),
                    max: a.max.max(b.max),
                });
                entry.shape = shape;
            }
        }
    }

    /// Rebuilds the spatial hash after colliders were added or moved. Swept boxes are
    /// hashed, so overlap tests find what a body passed.
    pub(crate) fn rebuild(&mut self) {
        self.hash.rebuild(&self.entries);
    }

    pub(crate) fn grids(&self) -> &[GridEntry] {
        &self.grids
    }

    /// Every collider whose bounding box may touch `rect`.
    pub(crate) fn near<'a>(
        &'a self,
        rect: Rect,
        buffer: &'a mut Vec<u32>,
    ) -> impl Iterator<Item = &'a Entry> {
        self.hash.candidates(rect, self.entries.len(), buffer);
        buffer.iter().map(|&i| &self.entries[i as usize])
    }

    /// Every pair of interacting colliders that overlap, except pairs of two static ones.
    /// Bodies are tested by their swept box against sensors and other bodies, and by their
    /// final shape against static colliders.
    pub(crate) fn overlapping_pairs(&self) -> BTreeSet<Pair> {
        let mut pairs = BTreeSet::new();
        let mut buffer = Vec::new();
        for (i, a) in self.entries.iter().enumerate() {
            if a.is_static() {
                continue;
            }
            self.hash
                .candidates(a.reach().aabb(), self.entries.len(), &mut buffer);
            for &j in &buffer {
                let j = j as usize;
                let b = &self.entries[j];
                // Pairs of two moving colliders are found from the lower index only.
                if j == i || (!b.is_static() && j < i) {
                    continue;
                }
                let overlap = if b.is_static() {
                    a.shape.overlaps(&b.shape)
                } else {
                    a.reach().overlaps(&b.reach())
                };
                if overlap && a.layers.interacts(b.layers) {
                    pairs.insert(Pair::new(a.entity, b.entity));
                }
            }
        }
        pairs
    }
}

/// Two entities, the lower (by index, then generation) first. Sets of pairs iterate in
/// that order too; `Entity`'s own ordering does not follow the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Pair(pub Entity, pub Entity);

/// An entity's position in index order.
fn key(entity: Entity) -> (u32, u32) {
    (entity.index_u32(), entity.generation().to_bits())
}

impl Pair {
    pub fn new(a: Entity, b: Entity) -> Self {
        if key(a) <= key(b) {
            Self(a, b)
        } else {
            Self(b, a)
        }
    }
}

impl Ord for Pair {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (key(self.0), key(self.1)).cmp(&(key(other.0), key(other.1)))
    }
}

impl PartialOrd for Pair {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// What a query considers.
///
/// By default everything: every layer, sensors and tile colliders. Narrow it with the
/// builder methods:
///
/// ```
/// use cg_physics::QueryFilter;
/// # let player = cg_core::prelude::Entity::from_raw_u32(1).unwrap();
/// let filter = QueryFilter::default().exclude(player).without_sensors();
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryFilter {
    /// Only colliders on one of these layers
    /// ([`CollisionLayers::memberships`](CollisionLayers::memberships)).
    pub mask: u32,
    /// An entity to skip, typically the one asking.
    pub exclude: Option<Entity>,
    /// Include sensors.
    pub sensors: bool,
    /// Include tile colliders.
    pub tiles: bool,
}

impl Default for QueryFilter {
    fn default() -> Self {
        Self {
            mask: CollisionLayers::ALL_BITS,
            exclude: None,
            sensors: true,
            tiles: true,
        }
    }
}

impl QueryFilter {
    /// Skips `entity`.
    pub fn exclude(mut self, entity: Entity) -> Self {
        self.exclude = Some(entity);
        self
    }

    /// Only colliders on one of the layers in `mask`.
    pub fn with_mask(mut self, mask: u32) -> Self {
        self.mask = mask;
        self
    }

    /// Leaves sensors out.
    pub fn without_sensors(mut self) -> Self {
        self.sensors = false;
        self
    }

    /// Leaves tile colliders out.
    pub fn without_tiles(mut self) -> Self {
        self.tiles = false;
        self
    }

    fn accepts(&self, entity: Entity, layers: CollisionLayers) -> bool {
        layers.memberships & self.mask != 0 && self.exclude != Some(entity)
    }

    fn accepts_entry(&self, entry: &Entry) -> bool {
        (self.sensors || !entry.sensor) && self.accepts(entry.entity, entry.layers)
    }
}

/// Where a ray hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    /// The collider's entity: the entity with the [`TileCollider`] for tiles.
    pub entity: Entity,
    /// The tile hit, for tile colliders.
    pub cell: Option<IVec2>,
    /// The point hit, in world units.
    pub point: Vec2,
    /// The surface normal there.
    pub normal: Vec2,
    /// How far along the ray.
    pub distance: f32,
}

/// A tile found by [`SpatialQuery::tile_at`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileHit {
    /// The entity with the [`TileCollider`].
    pub entity: Entity,
    /// The cell, in the grid's coordinates.
    pub cell: IVec2,
    /// What is there.
    pub tile: TileCell,
}

/// A tile grid placed in the world.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Grid<'a> {
    pub entity: Entity,
    pub grid: &'a TileGrid,
    pub offset: Vec2,
    pub layers: CollisionLayers,
}

/// What movement and queries collide with: the colliders and the tile grids.
pub(crate) struct Env<'a> {
    pub world: &'a PhysicsWorld,
    pub grids: Vec<Grid<'a>>,
}

impl<'a> Env<'a> {
    /// The world's colliders and the grids `lookup` finds for its tile colliders.
    pub fn new(world: &'a PhysicsWorld, lookup: impl Fn(Entity) -> Option<&'a TileGrid>) -> Self {
        let grids = world
            .grids()
            .iter()
            .filter_map(|entry| {
                Some(Grid {
                    entity: entry.entity,
                    grid: lookup(entry.entity)?,
                    offset: entry.offset,
                    layers: entry.layers,
                })
            })
            .collect();
        Self { world, grids }
    }

    pub fn grid(&self, entity: Entity) -> Option<&Grid<'a>> {
        self.grids.iter().find(|grid| grid.entity == entity)
    }

    pub fn raycast(
        &self,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
        filter: QueryFilter,
    ) -> Option<RayHit> {
        let direction = direction.try_normalize()?;
        let mut best: Option<RayHit> = None;
        let mut reach = max_distance;
        if filter.tiles {
            for grid in &self.grids {
                if !filter.accepts(grid.entity, grid.layers) {
                    continue;
                }
                if let Some((distance, normal, cell)) =
                    grid.grid.raycast(origin - grid.offset, direction, reach)
                {
                    reach = distance;
                    best = Some(RayHit {
                        entity: grid.entity,
                        cell: Some(cell),
                        point: origin + direction * distance,
                        normal,
                        distance,
                    });
                }
            }
        }
        let span = Rect::from_corners(origin, origin + direction * reach);
        let mut buffer = Vec::new();
        for entry in self.world.near(span, &mut buffer) {
            if !filter.accepts_entry(entry) {
                continue;
            }
            if let Some((distance, normal)) = entry.shape.raycast(origin, direction, reach)
                && best.is_none_or(|b| distance < b.distance)
                && (!entry.one_way || (normal.y > 0.5 && distance > 0.0))
            {
                reach = distance;
                best = Some(RayHit {
                    entity: entry.entity,
                    cell: None,
                    point: origin + direction * distance,
                    normal,
                    distance,
                });
            }
        }
        best
    }

    pub fn overlap_shape(&self, shape: WorldShape, filter: QueryFilter) -> Vec<Entity> {
        let mut buffer = Vec::new();
        self.world
            .near(shape.aabb(), &mut buffer)
            .filter(|entry| filter.accepts_entry(entry) && shape.overlaps(&entry.shape))
            .map(|entry| entry.entity)
            .collect()
    }

    pub fn overlap_point(&self, point: Vec2, filter: QueryFilter) -> Vec<Entity> {
        let mut buffer = Vec::new();
        self.world
            .near(Rect::from_center_size(point, Vec2::ZERO), &mut buffer)
            .filter(|entry| filter.accepts_entry(entry) && entry.shape.contains(point))
            .map(|entry| entry.entity)
            .collect()
    }

    pub fn tile_at(&self, point: Vec2, filter: QueryFilter) -> Option<TileHit> {
        if !filter.tiles {
            return None;
        }
        self.grids.iter().find_map(|grid| {
            if !filter.accepts(grid.entity, grid.layers) {
                return None;
            }
            let cell = grid.grid.cell_at(point - grid.offset);
            let tile = grid.grid.get(cell);
            (!tile.is_empty()).then_some(TileHit {
                entity: grid.entity,
                cell,
                tile,
            })
        })
    }
}

/// Raycasts and overlap tests against the colliders and tile grids, for game code.
///
/// Queries see the colliders where they were at the end of the latest physics step
/// (see [`PhysicsWorld`]). It reads only [`TileCollider`]s and the physics resources, so
/// it can sit next to queries that move entities.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_physics::{KinematicBody, QueryFilter, SpatialQuery};
///
/// // Is there ground a little ahead of each walker?
/// fn look_ahead(walkers: Query<(Entity, &GlobalTransform2D), With<KinematicBody>>, spatial: SpatialQuery) {
///     for (entity, transform) in &walkers {
///         let ahead = transform.translation() + vec2(12.0, 0.0);
///         let filter = QueryFilter::default().exclude(entity).without_sensors();
///         if spatial.raycast(ahead, Vec2::NEG_Y, 24.0, filter).is_none() {
///             info!("{entity} is about to walk off a ledge");
///         }
///     }
/// }
/// # use cg_world::GlobalTransform2D;
/// ```
#[derive(SystemParam)]
pub struct SpatialQuery<'w, 's> {
    world: Res<'w, PhysicsWorld>,
    tiles: Query<'w, 's, &'static TileCollider>,
}

impl SpatialQuery<'_, '_> {
    fn env(&self) -> Env<'_> {
        Env::new(&self.world, |entity| {
            self.tiles.get(entity).ok().map(|t| &t.grid)
        })
    }

    /// The first collider or tile a ray from `origin` along `direction` hits within
    /// `max_distance`. Solid tiles and colliders are hit from any side, one-way platforms
    /// and slopes only from above. A ray starting inside a collider hits it at distance 0.
    /// Returns `None` for a zero direction.
    pub fn raycast(
        &self,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
        filter: QueryFilter,
    ) -> Option<RayHit> {
        self.env().raycast(origin, direction, max_distance, filter)
    }

    /// Every collider containing `point` (edges included).
    pub fn overlap_point(&self, point: Vec2, filter: QueryFilter) -> Vec<Entity> {
        self.env().overlap_point(point, filter)
    }

    /// Every collider sharing area with `shape`.
    pub fn overlap_shape(&self, shape: WorldShape, filter: QueryFilter) -> Vec<Entity> {
        self.env().overlap_shape(shape, filter)
    }

    /// The first tile collider with a non-empty cell at `point`, and the cell.
    pub fn tile_at(&self, point: Vec2, filter: QueryFilter) -> Option<TileHit> {
        self.env().tile_at(point, filter)
    }

    /// The colliders as of the latest step.
    pub fn world(&self) -> &PhysicsWorld {
        &self.world
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::glam::vec2;

    fn entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    fn entry(index: u32, shape: WorldShape) -> Entry {
        Entry {
            entity: entity(index),
            shape,
            layers: CollisionLayers::DEFAULT,
            sensor: false,
            body: false,
            one_way: false,
            motion: Vec2::ZERO,
            swept: None,
        }
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> WorldShape {
        WorldShape::Rect(Rect::new(x0, y0, x1, y1))
    }

    #[test]
    fn the_hash_finds_neighbours_and_large_colliders() {
        let mut world = PhysicsWorld::default();
        world.push(entry(1, rect(0.0, 0.0, 10.0, 10.0)));
        world.push(entry(2, rect(500.0, 500.0, 510.0, 510.0)));
        world.push(entry(3, rect(-1.0e5, -10.0, 1.0e5, 0.0)));
        world.rebuild();
        let mut buffer = Vec::new();
        let near: Vec<_> = world
            .near(Rect::new(5.0, 5.0, 20.0, 20.0), &mut buffer)
            .map(|e| e.entity)
            .collect();
        assert_eq!(
            near,
            [entity(1), entity(3)],
            "the huge floor is always checked"
        );
        assert_eq!(
            world.shape(entity(2)),
            Some(rect(500.0, 500.0, 510.0, 510.0))
        );
        // A query too large for the hash checks everything.
        let all = world
            .near(Rect::new(-1.0e6, -1.0e6, 1.0e6, 1.0e6), &mut buffer)
            .count();
        assert_eq!(all, 3);
    }

    #[test]
    fn pairs_skip_static_pairs_and_layers_that_ignore_each_other() {
        let mut world = PhysicsWorld::default();
        world.push(entry(1, rect(0.0, 0.0, 10.0, 10.0)));
        world.push(entry(2, rect(5.0, 5.0, 15.0, 15.0)));
        world.push(Entry {
            sensor: true,
            ..entry(3, rect(8.0, 8.0, 12.0, 12.0))
        });
        world.push(Entry {
            body: true,
            ..entry(4, rect(9.0, 9.0, 20.0, 20.0))
        });
        world.push(Entry {
            body: true,
            layers: CollisionLayers::new(2, 2),
            ..entry(5, rect(0.0, 0.0, 20.0, 20.0))
        });
        world.rebuild();
        let pairs: Vec<_> = world
            .overlapping_pairs()
            .into_iter()
            .map(|p| (p.0, p.1))
            .collect();
        // 1-2 are both static; 5 is on a layer nothing else filters for.
        assert_eq!(
            pairs,
            [
                (entity(1), entity(3)),
                (entity(1), entity(4)),
                (entity(2), entity(3)),
                (entity(2), entity(4)),
                (entity(3), entity(4)),
            ]
        );
    }

    #[test]
    fn fast_bodies_touch_the_sensors_they_pass() {
        let mut world = PhysicsWorld::default();
        world.push(Entry {
            sensor: true,
            ..entry(1, rect(50.0, 0.0, 60.0, 10.0))
        });
        world.push(entry(2, rect(50.0, 20.0, 60.0, 30.0)));
        world.push(Entry {
            body: true,
            ..entry(3, rect(0.0, 0.0, 10.0, 10.0))
        });
        world.rebuild();
        // The body jumps from x 0 to x 100 in one step, past the coin; the crate above its
        // path is static, so only its final shape counts.
        world.move_body(entity(3), rect(100.0, 0.0, 110.0, 10.0));
        world.rebuild();
        let pairs: Vec<_> = world
            .overlapping_pairs()
            .into_iter()
            .map(|p| (p.0, p.1))
            .collect();
        assert_eq!(pairs, [(entity(1), entity(3))]);
        assert_eq!(world.shape(entity(3)), Some(rect(100.0, 0.0, 110.0, 10.0)));
    }

    #[test]
    fn rays_find_the_nearest_of_tiles_and_colliders() {
        let mut world = PhysicsWorld::default();
        world.push(entry(1, rect(30.0, 0.0, 40.0, 10.0)));
        world.push(Entry {
            sensor: true,
            ..entry(2, rect(10.0, 0.0, 12.0, 10.0))
        });
        world.push_grid(GridEntry {
            entity: entity(9),
            offset: vec2(100.0, 0.0),
            layers: CollisionLayers::DEFAULT,
        });
        world.rebuild();
        let grid = TileGrid::parse(Vec2::splat(10.0), "#");
        let env = Env::new(&world, |e| (e == entity(9)).then_some(&grid));
        let origin = vec2(0.0, 5.0);
        let hit = env
            .raycast(origin, Vec2::X, 500.0, QueryFilter::default())
            .unwrap();
        assert_eq!(
            (hit.entity, hit.distance),
            (entity(2), 10.0),
            "the sensor first"
        );
        let solid = QueryFilter::default().without_sensors();
        let hit = env.raycast(origin, Vec2::X, 500.0, solid).unwrap();
        assert_eq!((hit.entity, hit.point), (entity(1), vec2(30.0, 5.0)));
        let hit = env
            .raycast(origin, Vec2::X, 500.0, solid.exclude(entity(1)))
            .unwrap();
        assert_eq!(
            (hit.entity, hit.cell, hit.distance),
            (entity(9), Some(IVec2::ZERO), 100.0)
        );
        assert_eq!(
            env.raycast(origin, Vec2::X, 500.0, solid.with_mask(2)),
            None
        );
        assert_eq!(env.raycast(origin, Vec2::ZERO, 500.0, solid), None);

        assert_eq!(
            env.overlap_point(vec2(11.0, 5.0), QueryFilter::default()),
            [entity(2)]
        );
        assert_eq!(
            env.overlap_shape(
                WorldShape::Circle {
                    center: vec2(20.0, 5.0),
                    radius: 11.0
                },
                solid
            ),
            [entity(1)]
        );
        let tile = env
            .tile_at(vec2(105.0, 5.0), QueryFilter::default())
            .unwrap();
        assert_eq!(
            (tile.entity, tile.cell, tile.tile),
            (entity(9), IVec2::ZERO, TileCell::SOLID)
        );
        assert_eq!(env.tile_at(vec2(115.0, 5.0), QueryFilter::default()), None);
    }
}
