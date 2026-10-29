//! Debug drawing of tiles, colliders and contacts, toggled with a key.

use crate::body::KinematicContacts;
use crate::events::Overlaps;
use crate::grid::{TileCollider, TileGrid, TileShape};
use crate::shape::{Collider, Sensor, WorldShape};
use crate::world::PhysicsWorld;
use bevy_ecs::prelude::*;
use cg_core::Rect;
use cg_core::glam::{IVec2, Vec2, vec2};
use cg_platform::Window;
use cg_platform::input::{ButtonInput, KeyCode};
use cg_render::{Color, DebugShapes, FrameView};
use cg_world::InterpolatedTransforms;
use std::collections::HashSet;

/// Whether colliders are drawn, and the key that turns the drawing on and off.
///
/// When enabled, the physics draws with [`DebugShapes`]: the exposed edges of solid tiles
/// and slope surfaces in blue, one-way platforms in cyan, hazard tiles crossed in red;
/// static colliders in green, sensors in amber (filled while something overlaps them),
/// kinematic bodies in magenta (red while touching a hazard) with the sides they touch
/// in yellow and their ground normal.
///
/// `PhysicsPlugin` sets it up with F1 as the key; `--physics-debug` (or
/// `?physics-debug` on the web) starts with drawing on.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicsDebug {
    /// Draw colliders.
    pub enabled: bool,
    /// The key that toggles [`enabled`](Self::enabled), if any.
    pub toggle_key: Option<KeyCode>,
}

const TILE: Color = Color::hex(0x1d6ff2);
const ONE_WAY: Color = Color::hex(0x00a5c8);
const HAZARD: Color = Color::hex(0xff2d2d);
const STATIC: Color = Color::hex(0x1fa84f);
const SENSOR: Color = Color::hex(0xff9d00);
const BODY: Color = Color::hex(0xe020e0);
const CONTACT: Color = Color::hex(0xffe600);
/// Grids larger than this many visible cells (a far zoom) are not drawn.
const MAX_CELLS: i64 = 40_000;

/// Flips [`PhysicsDebug::enabled`] when its key is pressed.
pub(crate) fn toggle_debug(
    mut debug: ResMut<PhysicsDebug>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
) {
    let (Some(key), Some(keys)) = (debug.toggle_key, keys) else {
        return;
    };
    if keys.just_pressed(key) {
        debug.enabled = !debug.enabled;
        log::info!(
            "physics debug drawing {}",
            if debug.enabled { "on" } else { "off" }
        );
    }
}

type ColliderItem = (
    Entity,
    &'static Collider,
    Has<Sensor>,
    Option<&'static KinematicContacts>,
);

/// Draws the visible tiles and colliders when [`PhysicsDebug`] is enabled.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_debug(
    debug: Res<PhysicsDebug>,
    shapes: Option<ResMut<DebugShapes>>,
    view: Option<Res<FrameView>>,
    window: Option<Res<Window>>,
    world: Res<PhysicsWorld>,
    overlaps: Res<Overlaps>,
    tiles: Query<&TileCollider>,
    colliders: Query<ColliderItem>,
    transforms: InterpolatedTransforms,
) {
    let (true, Some(mut shapes), Some(view), Some(window)) = (debug.enabled, shapes, view, window)
    else {
        return;
    };
    let visible = view.0.visible_rect(&window);
    let width = shapes.line_width();
    shapes.set_line_width(1.0);

    for entry in world.grids() {
        if let Ok(tile) = tiles.get(entry.entity) {
            draw_grid(&mut shapes, &tile.grid, entry.offset, visible);
        }
    }

    let overlapping: HashSet<Entity> = overlaps.iter().flat_map(|(a, b)| [a, b]).collect();
    for (entity, collider, sensor, contacts) in &colliders {
        let Some(affine) = transforms.get(entity) else {
            continue;
        };
        let shape = collider.world_shape(affine);
        if !shape.aabb().overlaps(&visible) {
            continue;
        }
        let color = match contacts {
            Some(contacts) if contacts.hazard => HAZARD,
            Some(_) => BODY,
            None if sensor => SENSOR,
            None => STATIC,
        };
        draw_shape(&mut shapes, shape, color, overlapping.contains(&entity));
        if let Some(contacts) = contacts {
            draw_contacts(&mut shapes, shape.aabb(), contacts);
        }
    }
    shapes.set_line_width(width);
}

fn draw_shape(shapes: &mut DebugShapes, shape: WorldShape, color: Color, filled: bool) {
    let fill = color.with_alpha(0.35);
    match shape {
        WorldShape::Rect(rect) => {
            if filled {
                shapes.rect_filled(rect, fill);
            }
            shapes.rect(rect, color);
        }
        WorldShape::Circle { center, radius } => {
            if filled {
                shapes.circle_filled(center, radius, fill);
            }
            shapes.circle(center, radius, color);
        }
    }
}

fn draw_contacts(shapes: &mut DebugShapes, rect: Rect, contacts: &KinematicContacts) {
    shapes.set_line_width(2.0);
    let (min, max) = (rect.min, rect.max);
    if contacts.wall_left.is_some() {
        shapes.line(min, vec2(min.x, max.y), CONTACT);
    }
    if contacts.wall_right.is_some() {
        shapes.line(vec2(max.x, min.y), max, CONTACT);
    }
    if contacts.ceiling.is_some() {
        shapes.line(vec2(min.x, max.y), max, CONTACT);
    }
    if let Some(ground) = contacts.ground {
        let foot = vec2(rect.center().x, min.y);
        shapes.line(min, vec2(max.x, min.y), CONTACT);
        shapes.line(foot, foot + ground.normal * 8.0, CONTACT);
    }
    shapes.set_line_width(1.0);
}

/// Whether the cell at `cell` closes off the side of a solid neighbour facing it
/// (`side` points from the neighbour to the cell).
fn covers(grid: &TileGrid, cell: IVec2, side: IVec2) -> bool {
    match grid.get(cell).shape {
        TileShape::Solid => true,
        // A slope is solid along its bottom, and up to its height along its sides.
        TileShape::Slope { left, right } => match (side.x, side.y) {
            (1, _) => left >= 1.0,
            (-1, _) => right >= 1.0,
            (_, 1) => true,
            _ => false,
        },
        _ => false,
    }
}

fn draw_grid(shapes: &mut DebugShapes, grid: &TileGrid, offset: Vec2, visible: Rect) {
    let size = grid.cell_size();
    let local_min = grid.cell_at(visible.min - offset).max(grid.origin());
    let local_max = grid
        .cell_at(visible.max - offset)
        .min(grid.origin() + grid.size().as_ivec2() - IVec2::ONE);
    if local_min.x > local_max.x || local_min.y > local_max.y {
        return;
    }
    let count = (local_max.x - local_min.x + 1) as i64 * (local_max.y - local_min.y + 1) as i64;
    if count > MAX_CELLS {
        return;
    }
    for y in local_min.y..=local_max.y {
        for x in local_min.x..=local_max.x {
            let cell = IVec2::new(x, y);
            let tile = grid.get(cell);
            let rect = grid.cell_rect(cell);
            let (min, max) = (rect.min + offset, rect.max + offset);
            match tile.shape {
                TileShape::Solid => {
                    let edges = [
                        (IVec2::NEG_X, min, vec2(min.x, max.y)),
                        (IVec2::X, vec2(max.x, min.y), max),
                        (IVec2::NEG_Y, min, vec2(max.x, min.y)),
                        (IVec2::Y, vec2(min.x, max.y), max),
                    ];
                    for (side, a, b) in edges {
                        if !covers(grid, cell + side, side) {
                            shapes.line(a, b, TILE);
                        }
                    }
                }
                TileShape::OneWay => {
                    shapes.line(vec2(min.x, max.y), max, ONE_WAY);
                    // Little posts under the surface show which way is solid.
                    let post = size.y.min(size.x) * 0.25;
                    for t in [0.25, 0.75] {
                        let x = min.x + size.x * t;
                        shapes.line(vec2(x, max.y), vec2(x, max.y - post), ONE_WAY);
                    }
                }
                TileShape::Slope { left, right } => {
                    shapes.line(
                        vec2(min.x, min.y + left * size.y),
                        vec2(max.x, min.y + right * size.y),
                        TILE,
                    );
                }
                TileShape::Empty => {}
            }
            if tile.hazard {
                let inset = size * 0.15;
                shapes.line(min + inset, max - inset, HAZARD);
                shapes.line(
                    vec2(min.x + inset.x, max.y - inset.y),
                    vec2(max.x - inset.x, min.y + inset.y),
                    HAZARD,
                );
            }
        }
    }
}
