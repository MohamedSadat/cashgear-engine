//! Collision from Tiled maps: tile colliders built from a map's collision layers, and
//! colliders for objects the map marks.

use crate::grid::{TileCell, TileCollider, TileGrid, TileShape};
use crate::shape::{Collider, OneWay, Sensor};
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, AssetServer, Assets};
use cg_core::Rect;
use cg_core::glam::{IVec2, UVec2, Vec2};
use cg_tiled::{LayerKind, LayerTile, ObjectShape, Properties, TiledMap, TiledObject, Tilemap};

/// What a tilemap's [`TileCollider`] was built from.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TilemapCollision {
    built: (AssetId<TiledMap>, u64),
    has_grid: bool,
}

impl TileGrid {
    /// The collision grid of a Tiled map, from its tile layers whose `collision` property
    /// is `true` (see [`TileCollider`] for the tile properties), in the map entity's
    /// units: the map's bottom-left corner at the origin, one Tiled pixel per unit.
    /// `None` if no tile layer has the property.
    ///
    /// Layer offsets and parallax are ignored for collision, with a warning.
    pub fn from_tiled_map(map: &TiledMap) -> Option<Self> {
        let layers: Vec<_> = map
            .layers
            .iter()
            .filter_map(|layer| match &layer.kind {
                LayerKind::Tiles(tiles) if layer.properties.bool("collision") == Some(true) => {
                    Some((layer, tiles))
                }
                _ => None,
            })
            .collect();
        if layers.is_empty() {
            return None;
        }
        for (layer, _) in &layers {
            if layer.offset != Vec2::ZERO || layer.parallax != Vec2::ONE {
                log::warn!(
                    "collision layer `{}` has an offset or parallax; it collides where it would be without them",
                    layer.name
                );
            }
        }

        // Tiled counts rows from the top; the grid counts them from the bottom.
        let to_grid = |cell: IVec2| IVec2::new(cell.x, map.height as i32 - 1 - cell.y);
        let mut extent: Option<(IVec2, IVec2)> = None;
        for (_, tiles) in &layers {
            if tiles.width == 0 || tiles.height == 0 {
                continue;
            }
            let a = to_grid(tiles.origin);
            let b =
                to_grid(tiles.origin + IVec2::new(tiles.width as i32 - 1, tiles.height as i32 - 1));
            let (min, max) = (a.min(b), a.max(b));
            extent = Some(match extent {
                Some((lo, hi)) => (lo.min(min), hi.max(max)),
                None => (min, max),
            });
        }
        let cell_size = map.tile_size.as_vec2();
        let Some((min, max)) = extent else {
            return Some(Self::new(cell_size, IVec2::ZERO, UVec2::ZERO));
        };
        let mut grid = Self::new(cell_size, min, (max - min + IVec2::ONE).as_uvec2());

        let mut flipped_slopes = 0;
        // Bottom to top, so higher layers' shapes win.
        for (_, tiles) in &layers {
            for (cell, tile) in tiles.iter() {
                let Some(info) = map.tile_info(tile) else {
                    continue;
                };
                let (found, flipped) = tile_cell(&info.properties, tile, cell_size.y);
                flipped_slopes += flipped as u32;
                if found.is_empty() {
                    continue;
                }
                let at = to_grid(cell);
                let existing = grid.get(at);
                let shape = if found.shape == TileShape::Empty {
                    existing.shape
                } else {
                    found.shape
                };
                grid.set(
                    at,
                    TileCell {
                        shape,
                        hazard: existing.hazard || found.hazard,
                    },
                );
            }
        }
        if flipped_slopes > 0 {
            log::warn!(
                "{flipped_slopes} slope tiles are flipped vertically or diagonally; they collide as solid blocks"
            );
        }
        Some(grid)
    }
}

/// What a tile's properties say about its collision, and whether it is a slope flipped
/// in a way that is not supported.
fn tile_cell(properties: &Properties, tile: LayerTile, tile_height: f32) -> (TileCell, bool) {
    let hazard = properties.bool("hazard") == Some(true);
    let (left, right) = (
        properties.float("slope_left"),
        properties.float("slope_right"),
    );
    let mut flipped = false;
    let shape = if left.is_some() || right.is_some() {
        let height = |value: Option<f32>| (value.unwrap_or(0.0) / tile_height).clamp(0.0, 1.0);
        let (mut left, mut right) = (height(left), height(right));
        if tile.flip_y || tile.flip_diagonal {
            flipped = true;
            TileShape::Solid
        } else {
            if tile.flip_x {
                core::mem::swap(&mut left, &mut right);
            }
            TileShape::Slope { left, right }
        }
    } else if properties.bool("solid") == Some(true) {
        TileShape::Solid
    } else if properties.bool("one_way") == Some(true) {
        TileShape::OneWay
    } else {
        TileShape::Empty
    };
    (TileCell { shape, hazard }, flipped)
}

/// Gives each [`Tilemap`] a [`TileCollider`] from its map's collision layers, again
/// whenever the map changes.
pub(crate) fn build_tile_colliders(
    mut commands: Commands,
    tilemaps: Query<(Entity, &Tilemap, Option<&TilemapCollision>)>,
    maps: Option<Res<Assets<TiledMap>>>,
    server: Option<Res<AssetServer>>,
) {
    let Some(maps) = maps else {
        return;
    };
    for (entity, tilemap, built) in &tilemaps {
        let id = tilemap.map.id();
        let (Some(version), Some(map)) = (maps.version(id), maps.get(id)) else {
            continue;
        };
        if built.is_some_and(|built| built.built == (id, version)) {
            continue;
        }
        let path = server
            .as_ref()
            .and_then(|server| server.path(&tilemap.map))
            .unwrap_or_else(|| "a map".into());
        let grid = TileGrid::from_tiled_map(map);
        let has_grid = grid.is_some();
        let mut target = commands.entity(entity);
        match grid {
            Some(grid) => {
                let summary = summarize(&grid);
                if built.is_some() {
                    log::info!("rebuilt collision for `{path}`: {summary}");
                } else {
                    log::debug!("collision for `{path}`: {summary}");
                }
                target.insert(TileCollider::new(grid));
            }
            None if built.is_some_and(|built| built.has_grid) => {
                target.remove::<TileCollider>();
            }
            None => {}
        }
        target.insert(TilemapCollision {
            built: (id, version),
            has_grid,
        });
    }
}

fn summarize(grid: &TileGrid) -> String {
    let (mut solid, mut one_way, mut slopes, mut hazards) = (0, 0, 0, 0);
    for (_, cell) in grid.iter() {
        match cell.shape {
            TileShape::Solid => solid += 1,
            TileShape::OneWay => one_way += 1,
            TileShape::Slope { .. } => slopes += 1,
            TileShape::Empty => {}
        }
        hazards += cell.hazard as u32;
    }
    format!("{solid} solid, {one_way} one-way, {slopes} slope and {hazards} hazard cells")
}

/// Map objects spawned since the last run that have no collider yet.
type NewObjectsWithoutCollider = (Added<TiledObject>, Without<Collider>);

/// Gives map objects whose `collider` or `sensor` property is `true` a [`Collider`] of
/// their shape (and a [`Sensor`] for `sensor`, a [`OneWay`] for `one_way`), unless a prefab
/// already gave them one.
pub(crate) fn add_object_colliders(
    mut commands: Commands,
    objects: Query<(Entity, &TiledObject), NewObjectsWithoutCollider>,
) {
    for (entity, object) in &objects {
        let sensor = object.properties.bool("sensor") == Some(true);
        if !sensor && object.properties.bool("collider") != Some(true) {
            continue;
        }
        match object_collider(&object.shape) {
            Some(collider) => {
                let mut target = commands.entity(entity);
                target.try_insert(collider);
                if sensor {
                    target.try_insert(Sensor);
                } else if object.properties.bool("one_way") == Some(true) {
                    target.try_insert(OneWay);
                }
            }
            None => log::warn!(
                "object {} `{}` has no area to collide with",
                object.id,
                object.name
            ),
        }
    }
}

/// A collider for an object's shape: rectangles as they are, round ellipses as circles,
/// other ellipses and polygons by their bounding box.
fn object_collider(shape: &ObjectShape) -> Option<Collider> {
    match shape {
        ObjectShape::Rect(size) => Some(Collider::rect(size.x, size.y)),
        ObjectShape::Ellipse(size) if (size.x - size.y).abs() < 1e-3 => {
            Some(Collider::circle(size.x * 0.5))
        }
        ObjectShape::Ellipse(size) => Some(Collider::rect(size.x, size.y)),
        ObjectShape::Polygon(points) | ObjectShape::Polyline(points) => {
            let bounds = Rect::from_points(points.iter().copied())?;
            (!bounds.is_empty()).then(|| {
                Collider::rect(bounds.width(), bounds.height()).with_offset(bounds.center())
            })
        }
        ObjectShape::Point => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::glam::vec2;
    use cg_tiled::PropertyValue;
    use std::collections::BTreeMap;

    fn props(entries: &[(&str, PropertyValue)]) -> Properties {
        Properties(
            entries
                .iter()
                .map(|(name, value)| (name.to_string(), value.clone()))
                .collect::<BTreeMap<_, _>>(),
        )
    }

    fn tile(flip_x: bool, flip_y: bool) -> LayerTile {
        LayerTile {
            tileset: 0,
            id: 0,
            flip_x,
            flip_y,
            flip_diagonal: false,
        }
    }

    #[test]
    fn tile_properties_become_cells() {
        use PropertyValue::{Bool, Float, Int};
        let cell = |p: &[(&str, PropertyValue)], t| tile_cell(&props(p), t, 18.0);
        assert_eq!(
            cell(&[("solid", Bool(true))], tile(false, false)).0,
            TileCell::SOLID
        );
        assert_eq!(
            cell(&[("one_way", Bool(true))], tile(false, false)).0,
            TileCell::ONE_WAY
        );
        assert_eq!(
            cell(&[("hazard", Bool(true))], tile(false, false)).0,
            TileCell::EMPTY.with_hazard()
        );
        let slope = [("slope_left", Int(0)), ("slope_right", Float(9.0))];
        assert_eq!(
            cell(&slope, tile(false, false)),
            (TileCell::slope(0.0, 0.5), false)
        );
        assert_eq!(
            cell(&slope, tile(true, false)),
            (TileCell::slope(0.5, 0.0), false)
        );
        assert_eq!(cell(&slope, tile(false, true)), (TileCell::SOLID, true));
        assert!(
            cell(&[("solid", Bool(false))], tile(false, false))
                .0
                .is_empty()
        );
    }

    #[test]
    fn objects_collide_by_their_shape() {
        assert_eq!(
            object_collider(&ObjectShape::Rect(vec2(8.0, 4.0))),
            Some(Collider::rect(8.0, 4.0))
        );
        assert_eq!(
            object_collider(&ObjectShape::Ellipse(vec2(6.0, 6.0))),
            Some(Collider::circle(3.0))
        );
        assert_eq!(
            object_collider(&ObjectShape::Polygon(vec![
                vec2(0.0, 0.0),
                vec2(10.0, 0.0),
                vec2(10.0, -4.0)
            ])),
            Some(Collider::rect(10.0, 4.0).with_offset(vec2(5.0, -2.0)))
        );
        assert_eq!(object_collider(&ObjectShape::Point), None);
    }
}
