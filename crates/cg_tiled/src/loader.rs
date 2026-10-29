//! Loading `.tmx` maps with the `tiled` crate, reading tilesets and templates through the
//! asset IO so it works on every target.

use crate::map::{
    ImageLayer, LayerKind, LayerTile, MapLayer, MapObject, ObjectShape, Properties, PropertyValue,
    TileFrame, TileInfo, TileLayer, TiledMap, Tileset,
};
use cg_assets::{AssetLoader, BoxError, BoxedFuture, LoadContext, path};
use cg_core::glam::{IVec2, UVec2, Vec2, vec2};
use cg_render::{Color, Image};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

/// Loads Tiled maps (`.tmx`) as [`TiledMap`]s. External tilesets (`.tsx`) and object
/// templates (`.tx`) are read as part of the map, so editing them reloads it; tileset and
/// image-layer images load as dependencies. `TiledPlugin` registers it.
#[derive(Debug, Default)]
pub struct TmxLoader;

/// How many extra files (tilesets, templates) one map may pull in.
const MAX_FILES: usize = 64;

impl AssetLoader for TmxLoader {
    type Asset = TiledMap;

    fn extensions(&self) -> &[&str] {
        &["tmx"]
    }

    fn load<'a>(&'a self, ctx: &'a mut LoadContext) -> BoxedFuture<'a, Result<TiledMap, BoxError>> {
        Box::pin(async move {
            let map_path = ctx.path().to_owned();
            let files = Rc::new(RefCell::new(HashMap::from([(
                map_path.clone(),
                ctx.take_bytes(),
            )])));
            // The `tiled` crate reads files synchronously, so serve it from memory: parse,
            // and whenever it asks for a file not read yet, read that file and try again.
            for _ in 0..MAX_FILES {
                let missing = Rc::new(RefCell::new(None));
                let reader = MemoryReader {
                    files: files.clone(),
                    missing: missing.clone(),
                };
                let result = tiled::Loader::with_reader(reader).load_tmx_map(&map_path);
                let wanted = missing.borrow_mut().take();
                match (result, wanted) {
                    (Ok(map), _) => return convert(&map, &map_path, ctx),
                    (Err(_), Some(wanted)) => {
                        let bytes = ctx.read(&wanted).await?;
                        files.borrow_mut().insert(wanted, bytes);
                    }
                    (Err(err), None) => return Err(err.into()),
                }
            }
            Err(format!("the map needs more than {MAX_FILES} other files").into())
        })
    }
}

/// Serves files read so far, and notes the first one asked for that is not there.
struct MemoryReader {
    files: Rc<RefCell<HashMap<String, Vec<u8>>>>,
    missing: Rc<RefCell<Option<String>>>,
}

impl tiled::ResourceReader for MemoryReader {
    type Resource = Cursor<Vec<u8>>;
    type Error = std::io::Error;

    fn read_from(&mut self, file: &Path) -> Result<Self::Resource, Self::Error> {
        let key = path::normalize(&file.to_string_lossy());
        if let Some(bytes) = self.files.borrow().get(&key) {
            return Ok(Cursor::new(bytes.clone()));
        }
        self.missing.borrow_mut().get_or_insert(key.clone());
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("`{key}` is being read"),
        ))
    }
}

/// Turns a parsed map into the engine's types, starting loads of its images.
fn convert(map: &tiled::Map, map_path: &str, ctx: &mut LoadContext) -> Result<TiledMap, BoxError> {
    if map.orientation != tiled::Orientation::Orthogonal {
        return Err(format!(
            "only orthogonal maps are supported, not {:?}",
            map.orientation
        )
        .into());
    }
    let mut converter = Converter {
        ctx,
        map_path,
        tilesets: Vec::new(),
        sources: HashMap::new(),
    };
    for (index, tileset) in map.tilesets().iter().enumerate() {
        let first_gid = map_first_gid(map, index);
        converter.add_tileset(tileset, first_gid);
    }
    let mut layers = Vec::new();
    for layer in map.layers() {
        converter.layer(&layer, &Inherited::default(), &mut layers);
    }
    let properties = converter.properties(&map.properties, map_path);
    Ok(TiledMap {
        width: map.width,
        height: map.height,
        tile_size: UVec2::new(map.tile_width, map.tile_height),
        infinite: map.infinite(),
        tilesets: converter.tilesets,
        layers,
        properties,
        class: map.user_type.clone(),
        background: map.background_color.map(color),
        parallax_origin: vec2(map.parallax_origin_x, map.parallax_origin_y),
    })
}

/// The first gid of the map's `index`th tileset: one past the previous tileset's range.
/// `tiled` resolves gids itself and does not keep them, so this is for reference only.
fn map_first_gid(map: &tiled::Map, index: usize) -> u32 {
    map.tilesets()[..index]
        .iter()
        .map(|tileset| tileset.tilecount)
        .sum::<u32>()
        + 1
}

/// What a group layer passes on to the layers inside it.
struct Inherited {
    offset: Vec2,
    parallax: Vec2,
    opacity: f32,
    tint: Color,
    visible: bool,
}

impl Default for Inherited {
    fn default() -> Self {
        Self {
            offset: Vec2::ZERO,
            parallax: Vec2::ONE,
            opacity: 1.0,
            tint: Color::WHITE,
            visible: true,
        }
    }
}

struct Converter<'a> {
    ctx: &'a mut LoadContext,
    map_path: &'a str,
    tilesets: Vec<Tileset>,
    /// Tileset index by the tileset's `tiled` identity (its `Arc` address), so tiles
    /// from object templates find tilesets the map already has.
    sources: HashMap<*const tiled::Tileset, usize>,
}

impl Converter<'_> {
    fn add_tileset(&mut self, tileset: &Arc<tiled::Tileset>, first_gid: u32) -> usize {
        if let Some(&index) = self.sources.get(&Arc::as_ptr(tileset)) {
            return index;
        }
        // Templates may carry their own copy of a tileset the map also uses.
        if let Some(index) = self
            .tilesets
            .iter()
            .position(|t| t.name == tileset.name && t.tile_count == tileset.tilecount)
        {
            self.sources.insert(Arc::as_ptr(tileset), index);
            return index;
        }
        let tileset_path = path::normalize(&tileset.source.to_string_lossy());
        let (image, image_size) = match &tileset.image {
            Some(image) => {
                let image_path = path::normalize(&image.source.to_string_lossy());
                (
                    Some(self.ctx.load::<Image>(image_path)),
                    UVec2::new(image.width.max(0) as u32, image.height.max(0) as u32),
                )
            }
            None => {
                log::warn!(
                    "`{}`: tileset `{}` is an image collection, which is not supported yet; its tiles draw nothing",
                    self.map_path,
                    tileset.name
                );
                (None, UVec2::ZERO)
            }
        };
        let tiles = tileset
            .tiles()
            .filter(|(_, tile)| {
                !tile.user_type.is_empty()
                    || !tile.properties.is_empty()
                    || tile.animation.is_some()
            })
            .map(|(id, tile)| {
                let info = TileInfo {
                    class: tile.user_type.clone(),
                    properties: self.properties(&tile.properties, &tileset_path),
                    animation: tile
                        .animation
                        .iter()
                        .flatten()
                        .map(|frame| TileFrame {
                            tile: frame.tile_id,
                            duration: frame.duration as f32 / 1000.0,
                        })
                        .collect(),
                };
                (id, info)
            })
            .collect();
        let properties = self.properties(&tileset.properties, &tileset_path);
        self.tilesets.push(Tileset {
            name: tileset.name.clone(),
            first_gid,
            image,
            image_size,
            tile_size: UVec2::new(tileset.tile_width, tileset.tile_height),
            columns: tileset.columns,
            spacing: tileset.spacing,
            margin: tileset.margin,
            tile_count: tileset.tilecount,
            offset: IVec2::new(tileset.offset_x, tileset.offset_y),
            properties,
            tiles,
        });
        let index = self.tilesets.len() - 1;
        self.sources.insert(Arc::as_ptr(tileset), index);
        index
    }

    fn layer(&mut self, layer: &tiled::Layer, parent: &Inherited, out: &mut Vec<MapLayer>) {
        let here = Inherited {
            offset: parent.offset + vec2(layer.offset_x, layer.offset_y),
            parallax: parent.parallax * vec2(layer.parallax_x, layer.parallax_y),
            opacity: parent.opacity * layer.opacity,
            tint: multiply(parent.tint, layer.tint_color.map_or(Color::WHITE, color)),
            visible: parent.visible && layer.visible,
        };
        let kind = match layer.layer_type() {
            tiled::LayerType::Group(group) => {
                for child in group.layers() {
                    self.layer(&child, &here, out);
                }
                return;
            }
            tiled::LayerType::Tiles(tiles) => LayerKind::Tiles(tile_layer(&tiles)),
            tiled::LayerType::Image(image) => LayerKind::Image(self.image_layer(&image)),
            tiled::LayerType::Objects(objects) => {
                let objects = objects
                    .objects()
                    .map(|object| self.object(&object, here.offset))
                    .collect();
                LayerKind::Objects(objects)
            }
        };
        let properties = self.properties(&layer.properties, self.map_path);
        out.push(MapLayer {
            id: layer.id(),
            name: layer.name.clone(),
            class: layer.user_type.clone(),
            visible: here.visible,
            opacity: here.opacity,
            tint: here.tint,
            offset: here.offset,
            parallax: here.parallax,
            properties,
            kind,
        });
    }

    fn image_layer(&mut self, layer: &tiled::ImageLayer) -> ImageLayer {
        let (image, image_size) = match &layer.image {
            Some(image) => (
                Some(
                    self.ctx
                        .load::<Image>(path::normalize(&image.source.to_string_lossy())),
                ),
                UVec2::new(image.width.max(0) as u32, image.height.max(0) as u32),
            ),
            None => (None, UVec2::ZERO),
        };
        ImageLayer {
            image,
            image_size,
            repeat_x: layer.repeat_x,
            repeat_y: layer.repeat_y,
        }
    }

    fn object(&mut self, object: &tiled::Object, layer_offset: Vec2) -> MapObject {
        let tile = object.tile_data().map(|data| {
            let tileset = match data.tileset_location() {
                tiled::TilesetLocation::Map(index) => *index,
                tiled::TilesetLocation::Template(tileset) => self.add_tileset(tileset, 0),
            };
            LayerTile {
                tileset,
                id: data.id(),
                flip_x: data.flip_h,
                flip_y: data.flip_v,
                flip_diagonal: data.flip_d,
            }
        });
        let points = |points: &[(f32, f32)]| points.iter().map(|&(x, y)| vec2(x, y)).collect();
        let shape = match &object.shape {
            tiled::ObjectShape::Rect { width, height }
            | tiled::ObjectShape::Capsule { width, height }
            | tiled::ObjectShape::Text { width, height, .. } => {
                ObjectShape::Rect(vec2(*width, *height))
            }
            tiled::ObjectShape::Ellipse { width, height } => {
                ObjectShape::Ellipse(vec2(*width, *height))
            }
            tiled::ObjectShape::Polygon { points: p } => ObjectShape::Polygon(points(p)),
            tiled::ObjectShape::Polyline { points: p } => ObjectShape::Polyline(points(p)),
            tiled::ObjectShape::Point(..) => ObjectShape::Point,
        };
        // A tile object without a class of its own takes its tile's class and properties
        // as defaults.
        let tile_info = tile.and_then(|tile| self.tilesets.get(tile.tileset)?.tiles.get(&tile.id));
        let mut class = object.user_type.clone();
        let mut properties = tile_info
            .map(|info| info.properties.clone())
            .unwrap_or_default();
        if class.is_empty() {
            class = tile_info.map(|info| info.class.clone()).unwrap_or_default();
        }
        let own = self.properties(&object.properties, self.map_path);
        properties.0.extend(own.0);
        MapObject {
            id: object.id(),
            name: object.name.clone(),
            class,
            position: layer_offset + vec2(object.x, object.y),
            rotation: object.rotation,
            visible: object.visible,
            shape,
            tile,
            properties,
        }
    }

    fn properties(&self, properties: &tiled::Properties, owner: &str) -> Properties {
        Properties(
            properties
                .iter()
                .map(|(name, value)| (name.clone(), property(value, owner)))
                .collect(),
        )
    }
}

fn tile_layer(layer: &tiled::TileLayer) -> TileLayer {
    let tile = |data: &tiled::LayerTileData| LayerTile {
        tileset: data.tileset_index(),
        id: data.id(),
        flip_x: data.flip_h,
        flip_y: data.flip_v,
        flip_diagonal: data.flip_d,
    };
    match layer {
        tiled::TileLayer::Finite(finite) => {
            let (width, height) = (finite.width(), finite.height());
            let tiles = (0..height as i32)
                .flat_map(|y| (0..width as i32).map(move |x| (x, y)))
                .map(|(x, y)| finite.get_tile_data(x, y).map(tile))
                .collect();
            TileLayer {
                origin: IVec2::ZERO,
                width,
                height,
                tiles,
            }
        }
        tiled::TileLayer::Infinite(infinite) => {
            let (cw, ch) = (
                tiled::ChunkData::WIDTH as i32,
                tiled::ChunkData::HEIGHT as i32,
            );
            let chunks: Vec<_> = infinite.chunk_data().collect();
            let Some(min) = chunks
                .iter()
                .map(|&((x, y), _)| IVec2::new(x * cw, y * ch))
                .reduce(IVec2::min)
            else {
                return TileLayer::default();
            };
            let max = chunks
                .iter()
                .map(|&((x, y), _)| IVec2::new((x + 1) * cw, (y + 1) * ch))
                .fold(min, IVec2::max);
            let size = (max - min).as_uvec2();
            let mut tiles = vec![None; (size.x * size.y) as usize];
            for ((cx, cy), chunk) in chunks {
                for y in 0..ch {
                    for x in 0..cw {
                        let Some(data) = chunk.get_tile_data(x, y) else {
                            continue;
                        };
                        let cell = IVec2::new(cx * cw + x, cy * ch + y) - min;
                        tiles[(cell.y as u32 * size.x + cell.x as u32) as usize] = Some(tile(data));
                    }
                }
            }
            TileLayer {
                origin: min,
                width: size.x,
                height: size.y,
                tiles,
            }
        }
    }
}

fn property(value: &tiled::PropertyValue, owner: &str) -> PropertyValue {
    match value {
        tiled::PropertyValue::BoolValue(v) => PropertyValue::Bool(*v),
        tiled::PropertyValue::IntValue(v) => PropertyValue::Int(*v),
        tiled::PropertyValue::FloatValue(v) => PropertyValue::Float(*v),
        tiled::PropertyValue::StringValue(v) => PropertyValue::String(v.clone()),
        tiled::PropertyValue::ColorValue(v) => PropertyValue::Color(color(*v)),
        tiled::PropertyValue::FileValue(v) if v.is_empty() => PropertyValue::File(String::new()),
        tiled::PropertyValue::FileValue(v) => PropertyValue::File(path::resolve(owner, v)),
        tiled::PropertyValue::ObjectValue(v) => PropertyValue::Object(*v),
        tiled::PropertyValue::ListValue(values) => {
            PropertyValue::List(values.iter().map(|v| property(v, owner)).collect())
        }
        tiled::PropertyValue::ClassValue {
            property_type,
            properties,
        } => PropertyValue::Class {
            class: property_type.clone(),
            properties: Properties(
                properties
                    .iter()
                    .map(|(name, value)| (name.clone(), property(value, owner)))
                    .collect(),
            ),
        },
    }
}

fn color(c: tiled::Color) -> Color {
    Color::rgba8(c.red, c.green, c.blue, c.alpha)
}

fn multiply(a: Color, b: Color) -> Color {
    Color::rgba(a.r * b.r, a.g * b.g, a.b * b.b, a.a * b.a)
}
