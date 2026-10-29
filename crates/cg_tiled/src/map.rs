//! The [`TiledMap`] asset: tilesets, layers, objects and custom properties, in the
//! engine's own types.

use cg_assets::{Asset, Handle};
use cg_core::Rect;
use cg_core::glam::{IVec2, UVec2, Vec2, vec2};
use cg_render::{Color, Image};
use std::collections::{BTreeMap, HashMap};

/// A Tiled map loaded from a `.tmx` file, with its tilesets (from `.tsx` files or
/// embedded) and images. Spawn it with a [`Tilemap`](crate::Tilemap).
///
/// Group layers are flattened: their offsets, parallax, opacity, tint and visibility are
/// folded into the layers inside them, so `layers` lists every drawable layer from the
/// bottom up.
///
/// # Coordinates
///
/// Tiled measures in pixels from the map's top-left corner with y pointing down. The
/// engine puts the map's bottom-left corner at the origin of the map entity, with y up and
/// one pixel per world unit: Tiled's `(x, y)` is the world's `(x, height_px - y)`. See
/// [`to_world`](Self::to_world) and [`tile_to_world`](Self::tile_to_world).
#[derive(Debug, Clone)]
pub struct TiledMap {
    /// Width in tiles (for infinite maps, as the file gives it).
    pub width: u32,
    /// Height in tiles (for infinite maps, as the file gives it).
    pub height: u32,
    /// The size of one grid cell in pixels.
    pub tile_size: UVec2,
    /// Whether the map is infinite (stored in chunks; tiles may sit at negative cells).
    pub infinite: bool,
    /// The tilesets, in file order.
    pub tilesets: Vec<Tileset>,
    /// Every drawable layer, bottom to top, groups flattened.
    pub layers: Vec<MapLayer>,
    /// The map's custom properties.
    pub properties: Properties,
    /// The map's class.
    pub class: String,
    /// The background colour, if the map sets one.
    pub background: Option<Color>,
    /// The point, in Tiled pixels, where parallax layers line up with the rest.
    pub parallax_origin: Vec2,
}

impl Asset for TiledMap {}

impl TiledMap {
    /// The map's height in pixels: [`height`](Self::height) times the tile height.
    pub fn height_px(&self) -> f32 {
        (self.height * self.tile_size.y) as f32
    }

    /// The map's size in pixels (world units).
    pub fn size_px(&self) -> Vec2 {
        (UVec2::new(self.width, self.height) * self.tile_size).as_vec2()
    }

    /// The map's area in world units, relative to the map entity: from the origin to
    /// [`size_px`](Self::size_px).
    pub fn bounds(&self) -> Rect {
        Rect::from_min_size(Vec2::ZERO, self.size_px())
    }

    /// A point in Tiled's pixels (y down from the top) in world units relative to the map
    /// entity (y up from the bottom).
    pub fn to_world(&self, tiled: Vec2) -> Vec2 {
        vec2(tiled.x, self.height_px() - tiled.y)
    }

    /// A point in world units relative to the map entity in Tiled's pixels.
    pub fn to_tiled(&self, world: Vec2) -> Vec2 {
        vec2(world.x, self.height_px() - world.y)
    }

    /// The bottom-left corner of grid cell `(column, row)` (rows counted from the top, as
    /// in Tiled) in world units relative to the map entity.
    pub fn tile_to_world(&self, cell: IVec2) -> Vec2 {
        let size = self.tile_size.as_vec2();
        self.to_world(vec2(cell.x as f32 * size.x, (cell.y + 1) as f32 * size.y))
    }

    /// The grid cell `(column, row)` containing a point in world units relative to the map
    /// entity.
    pub fn world_to_tile(&self, world: Vec2) -> IVec2 {
        (self.to_tiled(world) / self.tile_size.as_vec2())
            .floor()
            .as_ivec2()
    }

    /// The first layer called `name`.
    pub fn layer(&self, name: &str) -> Option<&MapLayer> {
        self.layers.iter().find(|layer| layer.name == name)
    }

    /// The tile at grid cell `(column, row)` of the tile layer called `layer`.
    pub fn tile(&self, layer: &str, cell: IVec2) -> Option<LayerTile> {
        match &self.layer(layer)?.kind {
            LayerKind::Tiles(tiles) => tiles.get(cell),
            _ => None,
        }
    }

    /// The class and properties a tileset gives this tile, if any.
    pub fn tile_info(&self, tile: LayerTile) -> Option<&TileInfo> {
        self.tilesets.get(tile.tileset)?.tiles.get(&tile.id)
    }

    /// Every object in every object layer, with its layer.
    pub fn objects(&self) -> impl Iterator<Item = (&MapLayer, &MapObject)> {
        self.layers.iter().flat_map(|layer| {
            let objects: &[MapObject] = match &layer.kind {
                LayerKind::Objects(objects) => objects,
                _ => &[],
            };
            objects.iter().map(move |object| (layer, object))
        })
    }
}

/// A tileset: a grid of tiles cut from one image.
#[derive(Debug, Clone)]
pub struct Tileset {
    /// The tileset's name.
    pub name: String,
    /// The first global tile id the map gives this tileset.
    pub first_gid: u32,
    /// The image the tiles are cut from. `None` for image-collection tilesets, which are
    /// not supported yet; their tiles draw nothing.
    pub image: Option<Handle<Image>>,
    /// The image's size in pixels, as the tileset gives it.
    pub image_size: UVec2,
    /// The size of one tile in pixels.
    pub tile_size: UVec2,
    /// Tiles per row of the image.
    pub columns: u32,
    /// Pixels between tiles.
    pub spacing: u32,
    /// Pixels around the tiles at the image's edge.
    pub margin: u32,
    /// The number of tiles.
    pub tile_count: u32,
    /// Where tiles are drawn relative to their cell, in Tiled pixels (x right, y down).
    pub offset: IVec2,
    /// The tileset's custom properties.
    pub properties: Properties,
    /// The tiles that have a class or properties, by local tile id.
    pub tiles: HashMap<u32, TileInfo>,
}

impl Tileset {
    /// The texels of tile `id` in the tileset image, from its top-left corner.
    pub fn tile_rect(&self, id: u32) -> Rect {
        let columns = self.columns.max(1);
        let (column, row) = (id % columns, id / columns);
        let min = UVec2::new(
            self.margin + column * (self.tile_size.x + self.spacing),
            self.margin + row * (self.tile_size.y + self.spacing),
        );
        Rect::from_min_size(min.as_vec2(), self.tile_size.as_vec2())
    }
}

/// What a tileset says about one of its tiles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TileInfo {
    /// The tile's class (its "type" before Tiled 1.9).
    pub class: String,
    /// The tile's custom properties, such as `solid` or `one_way` for collision.
    pub properties: Properties,
    /// The tile's animation from Tiled's tile animation editor: tiles of the same tileset
    /// shown in turn. Empty for a still tile.
    pub animation: Vec<TileFrame>,
}

/// One frame of a tile animation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileFrame {
    /// The local id of the tile shown.
    pub tile: u32,
    /// Seconds it shows.
    pub duration: f32,
}

/// One drawable layer of a [`TiledMap`].
#[derive(Debug, Clone)]
pub struct MapLayer {
    /// The layer's id in the file.
    pub id: u32,
    /// The layer's name.
    pub name: String,
    /// The layer's class.
    pub class: String,
    /// Whether it is drawn (hidden layers' objects still spawn, hidden).
    pub visible: bool,
    /// Opacity from 0 to 1.
    pub opacity: f32,
    /// Multiplies the layer's colours; white leaves them unchanged.
    pub tint: Color,
    /// Offset in Tiled pixels (x right, y down).
    pub offset: Vec2,
    /// How fast the layer scrolls with the camera: 1 like the rest of the map, 0 not at
    /// all, values in between for distant backgrounds.
    pub parallax: Vec2,
    /// The layer's custom properties.
    pub properties: Properties,
    /// What the layer holds.
    pub kind: LayerKind,
}

impl MapLayer {
    /// The colour tiles and images on this layer are tinted with: the tint with the
    /// opacity folded into its alpha.
    pub fn color(&self) -> Color {
        let tint = self.tint;
        tint.with_alpha(tint.a * self.opacity)
    }
}

/// What a [`MapLayer`] holds.
#[derive(Debug, Clone)]
pub enum LayerKind {
    /// A grid of tiles.
    Tiles(TileLayer),
    /// One image, optionally repeated.
    Image(ImageLayer),
    /// Objects, which spawn as entities.
    Objects(Vec<MapObject>),
}

/// A grid of tiles. Cells are `(column, row)` with rows from the top, as in Tiled.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TileLayer {
    /// The cell of the grid's top-left corner: `(0, 0)` except on infinite maps.
    pub origin: IVec2,
    /// Width of the stored grid in cells.
    pub width: u32,
    /// Height of the stored grid in cells.
    pub height: u32,
    /// The cells, row by row from the top; `None` is empty.
    pub tiles: Vec<Option<LayerTile>>,
}

impl TileLayer {
    /// The tile at a cell, if any.
    pub fn get(&self, cell: IVec2) -> Option<LayerTile> {
        let local = cell - self.origin;
        if local.x < 0
            || local.y < 0
            || local.x >= self.width as i32
            || local.y >= self.height as i32
        {
            return None;
        }
        self.tiles[(local.y as u32 * self.width + local.x as u32) as usize]
    }

    /// Every tile with its cell.
    pub fn iter(&self) -> impl Iterator<Item = (IVec2, LayerTile)> + '_ {
        self.tiles.iter().enumerate().filter_map(|(i, tile)| {
            let i = i as u32;
            Some((
                self.origin + IVec2::new((i % self.width) as i32, (i / self.width) as i32),
                (*tile)?,
            ))
        })
    }
}

/// A tile placed in a layer or object: which tile, and how it is flipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayerTile {
    /// Index into [`TiledMap::tilesets`].
    pub tileset: usize,
    /// The tile's id within its tileset.
    pub id: u32,
    /// Mirrored left to right.
    pub flip_x: bool,
    /// Mirrored top to bottom.
    pub flip_y: bool,
    /// Mirrored across the diagonal (x and y swapped), applied before the other flips.
    /// Together with them it gives quarter turns.
    pub flip_diagonal: bool,
}

/// An image layer.
#[derive(Debug, Clone)]
pub struct ImageLayer {
    /// The image. `None` if the layer has none set.
    pub image: Option<Handle<Image>>,
    /// The image's size in pixels, as the map gives it.
    pub image_size: UVec2,
    /// Repeat the image left and right to fill the view.
    pub repeat_x: bool,
    /// Repeat the image up and down to fill the view.
    pub repeat_y: bool,
}

/// An object from an object layer.
#[derive(Debug, Clone, PartialEq)]
pub struct MapObject {
    /// The object's id, unique in the map.
    pub id: u32,
    /// The object's name.
    pub name: String,
    /// The object's class (its "type" before Tiled 1.9), or its tile's class for a tile
    /// object without one. With a [`Tilemap`](crate::Tilemap), a class names the prefab
    /// the object spawns as.
    pub class: String,
    /// Its position in Tiled pixels, layer offset included: the top-left corner of
    /// rectangles and ellipses, the bottom-left of tile objects, the point itself, and
    /// the origin of polygons.
    pub position: Vec2,
    /// Clockwise rotation in degrees around [`position`](Self::position), as in Tiled.
    pub rotation: f32,
    /// Whether it is shown.
    pub visible: bool,
    /// Its shape, in Tiled pixels relative to `position`.
    pub shape: ObjectShape,
    /// The tile it shows, for tile objects.
    pub tile: Option<LayerTile>,
    /// Its custom properties, template and tile properties included.
    pub properties: Properties,
}

/// The shape of a [`MapObject`] in Tiled pixels (y down).
#[derive(Debug, Clone, PartialEq)]
pub enum ObjectShape {
    /// A rectangle (or a tile object) of this size.
    Rect(Vec2),
    /// An ellipse in a rectangle of this size.
    Ellipse(Vec2),
    /// A point.
    Point,
    /// A closed polygon.
    Polygon(Vec<Vec2>),
    /// An open line.
    Polyline(Vec<Vec2>),
}

/// Custom properties, by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Properties(pub BTreeMap<String, PropertyValue>);

impl Properties {
    /// The property called `name`.
    pub fn get(&self, name: &str) -> Option<&PropertyValue> {
        self.0.get(name)
    }

    /// A `bool` property.
    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            PropertyValue::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// An `int` property.
    pub fn int(&self, name: &str) -> Option<i32> {
        match self.get(name)? {
            PropertyValue::Int(value) => Some(*value),
            _ => None,
        }
    }

    /// A `float` property, or an `int` one as a float.
    pub fn float(&self, name: &str) -> Option<f32> {
        match self.get(name)? {
            PropertyValue::Float(value) => Some(*value),
            PropertyValue::Int(value) => Some(*value as f32),
            _ => None,
        }
    }

    /// A `string` or `file` property.
    pub fn str(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            PropertyValue::String(value) | PropertyValue::File(value) => Some(value),
            _ => None,
        }
    }

    /// A `color` property.
    pub fn color(&self, name: &str) -> Option<Color> {
        match self.get(name)? {
            PropertyValue::Color(value) => Some(*value),
            _ => None,
        }
    }

    /// The names and values, sorted by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &PropertyValue)> {
        self.0.iter().map(|(name, value)| (name.as_str(), value))
    }

    /// The number of properties.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// One custom property value.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// `bool`.
    Bool(bool),
    /// `int`.
    Int(i32),
    /// `float`.
    Float(f32),
    /// `string`.
    String(String),
    /// `color`.
    Color(Color),
    /// `file`, as an asset path (resolved against the map or tileset that set it).
    File(String),
    /// `object`: the id of another object in the map, 0 if unset.
    Object(u32),
    /// `list`.
    List(Vec<PropertyValue>),
    /// A custom class with its members.
    Class {
        /// The class's name.
        class: String,
        /// Its members.
        properties: Properties,
    },
}

impl PropertyValue {
    /// The value written as RON: numbers and booleans as they are, strings, files and
    /// colours (`"#rrggbbaa"`) quoted, lists in brackets, and classes as structs
    /// `(member: value, ...)`. This is how property values become component fields.
    pub fn to_ron(&self) -> String {
        match self {
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => {
                let text = value.to_string();
                if text.contains(['.', 'e', 'i', 'N']) {
                    text
                } else {
                    format!("{text}.0")
                }
            }
            Self::String(value) | Self::File(value) => format!("{value:?}"),
            Self::Color(color) => format!("{:?}", color.to_hex_string()),
            Self::Object(id) => id.to_string(),
            Self::List(values) => {
                let items: Vec<_> = values.iter().map(Self::to_ron).collect();
                format!("[{}]", items.join(", "))
            }
            Self::Class { properties, .. } => {
                let fields: Vec<_> = properties
                    .iter()
                    .map(|(name, value)| format!("{name}: {}", value.to_ron()))
                    .collect();
                format!("({})", fields.join(", "))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> TiledMap {
        TiledMap {
            width: 10,
            height: 5,
            tile_size: UVec2::new(16, 16),
            infinite: false,
            tilesets: Vec::new(),
            layers: Vec::new(),
            properties: Properties::default(),
            class: String::new(),
            background: None,
            parallax_origin: Vec2::ZERO,
        }
    }

    #[test]
    fn tiled_pixels_flip_into_world_units() {
        let map = map();
        assert_eq!(map.height_px(), 80.0);
        assert_eq!(map.to_world(vec2(8.0, 0.0)), vec2(8.0, 80.0));
        assert_eq!(map.to_tiled(vec2(8.0, 80.0)), vec2(8.0, 0.0));
        // Row 0 is the top row: its bottom edge is 16 units below the top.
        assert_eq!(map.tile_to_world(IVec2::new(2, 0)), vec2(32.0, 64.0));
        assert_eq!(map.world_to_tile(vec2(33.0, 70.0)), IVec2::new(2, 0));
        assert_eq!(map.world_to_tile(vec2(-1.0, 1.0)), IVec2::new(-1, 4));
        assert_eq!(map.bounds(), Rect::new(0.0, 0.0, 160.0, 80.0));
    }

    #[test]
    fn tile_rects_skip_margin_and_spacing() {
        let tileset = Tileset {
            name: "t".into(),
            first_gid: 1,
            image: None,
            image_size: UVec2::new(100, 100),
            tile_size: UVec2::new(18, 18),
            columns: 4,
            spacing: 1,
            margin: 2,
            tile_count: 16,
            offset: IVec2::ZERO,
            properties: Properties::default(),
            tiles: HashMap::new(),
        };
        assert_eq!(tileset.tile_rect(0), Rect::new(2.0, 2.0, 20.0, 20.0));
        assert_eq!(tileset.tile_rect(5), Rect::new(21.0, 21.0, 39.0, 39.0));
    }

    #[test]
    fn tile_layers_index_by_cell() {
        let tile = |id| {
            Some(LayerTile {
                tileset: 0,
                id,
                flip_x: false,
                flip_y: false,
                flip_diagonal: false,
            })
        };
        let layer = TileLayer {
            origin: IVec2::new(-1, 0),
            width: 2,
            height: 2,
            tiles: vec![tile(1), None, None, tile(4)],
        };
        assert_eq!(layer.get(IVec2::new(-1, 0)).map(|t| t.id), Some(1));
        assert_eq!(layer.get(IVec2::new(0, 1)).map(|t| t.id), Some(4));
        assert_eq!(layer.get(IVec2::new(1, 0)), None);
        let cells: Vec<_> = layer.iter().map(|(cell, t)| (cell, t.id)).collect();
        assert_eq!(cells, [(IVec2::new(-1, 0), 1), (IVec2::new(0, 1), 4)]);
    }

    #[test]
    fn properties_read_and_write_ron() {
        let class = PropertyValue::Class {
            class: "Patrol".into(),
            properties: Properties(BTreeMap::from([
                ("speed".into(), PropertyValue::Float(40.0)),
                ("loop".into(), PropertyValue::Bool(true)),
                ("name".into(), PropertyValue::String("a \"b\"".into())),
            ])),
        };
        assert_eq!(
            class.to_ron(),
            r#"(loop: true, name: "a \"b\"", speed: 40.0)"#
        );
        assert_eq!(PropertyValue::Float(0.5).to_ron(), "0.5");
        assert_eq!(
            PropertyValue::Color(Color::rgba8(255, 0, 0, 128)).to_ron(),
            r##""#ff000080""##
        );
        let props = Properties(BTreeMap::from([
            ("n".into(), PropertyValue::Int(3)),
            ("f".into(), PropertyValue::File("images/a.png".into())),
        ]));
        assert_eq!(props.float("n"), Some(3.0));
        assert_eq!(props.int("n"), Some(3));
        assert_eq!(props.str("f"), Some("images/a.png"));
        assert_eq!(props.bool("n"), None);
    }
}
