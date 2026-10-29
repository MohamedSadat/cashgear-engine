//! Drawing tilemaps: tile layers become cached chunks of queued sprites, culled and
//! queued as a whole each frame; image layers are queued, repeated to fill the view.

use crate::animation::TileAnimation;
use crate::map::{LayerKind, LayerTile, MapLayer, TiledMap};
use crate::spawn::TilemapState;
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, Assets};
use cg_core::glam::{Affine2, IVec2, Vec2, vec2};
use cg_core::{Rect, Time};
use cg_platform::Window;
use cg_render::{Color, FrameView, Image, InheritedVisibility, QueuedSprite, RenderStats, Sprites};
use cg_world::GlobalTransform2D;
use std::collections::{BTreeMap, HashMap};

/// Tiles per chunk side.
pub(crate) const CHUNK_TILES: i32 = 16;

/// Up to this many copies of a repeated image layer are drawn per frame.
const MAX_REPEATS: i64 = 4096;

/// The tiles of one layer inside one 16×16-cell square, ready to queue.
#[derive(Debug, Clone)]
pub(crate) struct TileChunk {
    pub(crate) parallax: Vec2,
    /// Where the tiles are, relative to the map entity.
    pub(crate) bounds: Rect,
    /// The tiles, with transforms relative to the map entity.
    pub(crate) sprites: Vec<QueuedSprite>,
    /// The animated tiles among them, by index into `sprites`, in order.
    pub(crate) animated: Vec<(usize, TileAnimation)>,
}

/// An image layer, ready to queue.
#[derive(Debug, Clone)]
pub(crate) struct ImageDraw {
    image: AssetId<Image>,
    layer: i32,
    /// The image's top-left corner relative to the map entity.
    top_left: Vec2,
    parallax: Vec2,
    color: Color,
    repeat_x: bool,
    repeat_y: bool,
}

/// Cuts the map's visible tile layers into chunks and lists its image layers.
pub(crate) fn build(map: &TiledMap, base: i32) -> (Vec<TileChunk>, Vec<ImageDraw>) {
    let mut chunks = Vec::new();
    let mut images = Vec::new();
    // Each animated tile's frames, shared by all its copies.
    let mut animations: HashMap<(usize, u32), Option<TileAnimation>> = HashMap::new();
    for (index, layer) in map.layers.iter().enumerate() {
        if !layer.visible {
            continue;
        }
        let draw_layer = base + index as i32;
        match &layer.kind {
            LayerKind::Tiles(tiles) => {
                let mut by_chunk: BTreeMap<(i32, i32), TileChunk> = BTreeMap::new();
                for (cell, tile) in tiles.iter() {
                    let Some(sprite) = tile_sprite(map, layer, cell, tile, draw_layer) else {
                        continue;
                    };
                    let key = cell.div_euclid(IVec2::splat(CHUNK_TILES));
                    // Rows run top to bottom; keep that order within a chunk.
                    let chunk = by_chunk.entry((key.y, key.x)).or_insert_with(|| TileChunk {
                        parallax: layer.parallax,
                        bounds: Rect::default(),
                        sprites: Vec::new(),
                        animated: Vec::new(),
                    });
                    let animation =
                        animations
                            .entry((tile.tileset, tile.id))
                            .or_insert_with(|| {
                                let tileset = map.tilesets.get(tile.tileset)?;
                                TileAnimation::of_tile(tileset, tile.id)
                            });
                    if let Some(animation) = animation {
                        chunk
                            .animated
                            .push((chunk.sprites.len(), animation.clone()));
                    }
                    chunk.sprites.push(sprite);
                }
                chunks.extend(by_chunk.into_values().map(|mut chunk| {
                    chunk.bounds = bounds(&chunk.sprites);
                    chunk
                }));
            }
            LayerKind::Image(image) => {
                if let Some(handle) = &image.image {
                    images.push(ImageDraw {
                        image: handle.id(),
                        layer: draw_layer,
                        top_left: map.to_world(layer.offset),
                        parallax: layer.parallax,
                        color: layer.color(),
                        repeat_x: image.repeat_x,
                        repeat_y: image.repeat_y,
                    });
                }
            }
            LayerKind::Objects(_) => {}
        }
    }
    (chunks, images)
}

/// One tile as a queued sprite relative to the map entity. The sprite is a unit square
/// stretched to the tile's size, with Tiled's flips folded into its transform.
fn tile_sprite(
    map: &TiledMap,
    layer: &MapLayer,
    cell: IVec2,
    tile: LayerTile,
    draw_layer: i32,
) -> Option<QueuedSprite> {
    let tileset = map.tilesets.get(tile.tileset)?;
    let texture = tileset.image.as_ref()?.id();
    let grid = map.tile_size.as_vec2();
    // Tiles hang from the bottom-left corner of their cell, so tall tiles reach upwards.
    let corner = vec2(cell.x as f32 * grid.x, (cell.y + 1) as f32 * grid.y)
        + layer.offset
        + tileset.offset.as_vec2();
    let size = tileset.tile_size.as_vec2();
    let transform =
        Affine2::from_translation(map.to_world(corner)) * Affine2::from_scale(size) * flips(tile);
    Some(QueuedSprite {
        texture,
        size: Some(Vec2::ONE),
        source: Some(tileset.tile_rect(tile.id)),
        pivot: Vec2::ZERO,
        color: layer.color(),
        flip_x: false,
        flip_y: false,
        flash: Color::TRANSPARENT,
        transform,
        layer: draw_layer,
    })
}

/// Tiled's tile flips as a map of the unit square onto itself (y up): the diagonal flip
/// first, then the horizontal and vertical ones, as Tiled applies them.
pub(crate) fn flips(tile: LayerTile) -> Affine2 {
    let mut m = Affine2::IDENTITY;
    if tile.flip_diagonal {
        // Swapping x and y in Tiled's y-down space is the anti-diagonal mirror with y up.
        m = Affine2::from_cols(vec2(0.0, -1.0), vec2(-1.0, 0.0), vec2(1.0, 1.0));
    }
    if tile.flip_x {
        m = Affine2::from_cols(vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(1.0, 0.0)) * m;
    }
    if tile.flip_y {
        m = Affine2::from_cols(vec2(1.0, 0.0), vec2(0.0, -1.0), vec2(0.0, 1.0)) * m;
    }
    m
}

/// The bounding box of sprites made by [`tile_sprite`] (unit squares under their
/// transforms).
fn bounds(sprites: &[QueuedSprite]) -> Rect {
    let corners = sprites.iter().flat_map(|s| {
        [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE].map(|c| s.transform.transform_point2(c))
    });
    Rect::from_points(corners).unwrap_or_default()
}

/// The bounding box of `rect` under `transform`.
fn transform_rect(transform: Affine2, rect: Rect) -> Rect {
    let corners = [
        rect.min,
        rect.max,
        vec2(rect.min.x, rect.max.y),
        vec2(rect.max.x, rect.min.y),
    ];
    Rect::from_points(corners.map(|c| transform.transform_point2(c))).unwrap_or(rect)
}

/// Queues the chunks and image layers of every visible tilemap that overlap the view,
/// with animated tiles on their current frame.
pub(crate) fn queue_tilemaps(
    time: Res<Time>,
    maps: Query<(&TilemapState, &GlobalTransform2D, &InheritedVisibility)>,
    images: Res<Assets<Image>>,
    view: Res<FrameView>,
    window: Res<Window>,
    mut sprites: ResMut<Sprites>,
    mut stats: ResMut<RenderStats>,
) {
    let visible = view.0.visible_rect(&window);
    let camera = view.0.position;
    let seconds = time.elapsed_secs();
    for (state, global, shown) in &maps {
        if !shown.get() || (state.chunks.is_empty() && state.images.is_empty()) {
            continue;
        }
        let to_world = global.0;
        let camera_local = to_world.inverse().transform_point2(camera);
        // A layer with parallax p moves (1 - p) times as far as the camera, measured from
        // the map's parallax origin, so it seems to scroll p times as fast.
        let placed = |parallax: Vec2| {
            to_world
                * Affine2::from_translation(
                    (Vec2::ONE - parallax) * (camera_local - state.parallax_origin),
                )
        };
        let mut culled = 0;
        for chunk in &state.chunks {
            let transform = placed(chunk.parallax);
            if !transform_rect(transform, chunk.bounds).overlaps(&visible) {
                culled += chunk.sprites.len() as u32;
                continue;
            }
            let mut animated = chunk.animated.iter().peekable();
            sprites.extend(chunk.sprites.iter().enumerate().map(|(index, sprite)| {
                let mut queued = QueuedSprite {
                    transform: transform * sprite.transform,
                    ..*sprite
                };
                if let Some((_, animation)) = animated.next_if(|(i, _)| *i == index) {
                    queued.source = Some(animation.source_at(seconds));
                }
                queued
            }));
        }
        for image in &state.images {
            let Some(size) = images.get(image.image).map(|i| i.size().as_vec2()) else {
                continue;
            };
            queue_image(image, size, placed(image.parallax), visible, &mut sprites);
        }
        stats.add_culled(culled);
    }
}

/// Queues an image layer, repeated over the part of `visible` it should fill.
fn queue_image(
    image: &ImageDraw,
    size: Vec2,
    transform: Affine2,
    visible: Rect,
    sprites: &mut Sprites,
) {
    let origin = image.top_left - vec2(0.0, size.y);
    // The visible rectangle in the image layer's own space.
    let local = transform_rect(transform.inverse(), visible);
    let range = |repeat: bool, min: f32, max: f32, start: f32, step: f32| -> (i64, i64) {
        if !repeat {
            return (0, 0);
        }
        let first = ((min - start) / step).floor() as i64;
        let last = ((max - start) / step).floor() as i64;
        (first, last.min(first + MAX_REPEATS))
    };
    let (x0, x1) = range(image.repeat_x, local.min.x, local.max.x, origin.x, size.x);
    let (y0, y1) = range(image.repeat_y, local.min.y, local.max.y, origin.y, size.y);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let corner = origin + vec2(x as f32 * size.x, y as f32 * size.y);
            let copy = transform * Affine2::from_translation(corner);
            if !transform_rect(copy, Rect::from_min_size(Vec2::ZERO, size)).overlaps(&visible) {
                continue;
            }
            sprites.push(QueuedSprite {
                texture: image.image,
                size: Some(size),
                source: None,
                pivot: Vec2::ZERO,
                color: image.color,
                flip_x: false,
                flip_y: false,
                flash: Color::TRANSPARENT,
                transform: copy,
                layer: image.layer,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(flip_x: bool, flip_y: bool, flip_diagonal: bool) -> LayerTile {
        LayerTile {
            tileset: 0,
            id: 0,
            flip_x,
            flip_y,
            flip_diagonal,
        }
    }

    /// Where the image's top-left corner (the unit square's (0, 1) with y up) is drawn.
    fn top_left(t: LayerTile) -> Vec2 {
        flips(t).transform_point2(vec2(0.0, 1.0))
    }

    #[test]
    fn flips_follow_tileds_rules() {
        assert_eq!(top_left(tile(false, false, false)), vec2(0.0, 1.0));
        assert_eq!(top_left(tile(true, false, false)), vec2(1.0, 1.0));
        assert_eq!(top_left(tile(false, true, false)), vec2(0.0, 0.0));
        // Diagonal alone mirrors across the top-left to bottom-right diagonal.
        assert_eq!(top_left(tile(false, false, true)), vec2(0.0, 1.0));
        assert_eq!(
            flips(tile(false, false, true)).transform_point2(vec2(1.0, 1.0)),
            vec2(0.0, 0.0),
            "the top-right corner lands bottom-left"
        );
        // Tiled's "rotate clockwise" is diagonal + horizontal: top-left goes top-right,
        // top-right goes bottom-right.
        let cw = tile(true, false, true);
        assert_eq!(top_left(cw), vec2(1.0, 1.0));
        assert_eq!(flips(cw).transform_point2(vec2(1.0, 1.0)), vec2(1.0, 0.0));
        // Every combination keeps the unit square in place.
        for bits in 0..8 {
            let t = tile(bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            let moved =
                Rect::from_points([Vec2::ZERO, Vec2::ONE].map(|p| flips(t).transform_point2(p)));
            assert_eq!(moved, Some(Rect::new(0.0, 0.0, 1.0, 1.0)), "{t:?}");
        }
    }
}
