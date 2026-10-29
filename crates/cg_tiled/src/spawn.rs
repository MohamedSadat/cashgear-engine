//! The [`Tilemap`] component: builds a map's tile chunks and spawns its objects as
//! entities, again whenever the map changes.

use crate::animation::TileAnimation;
use crate::draw::{self, ImageDraw, TileChunk};
use crate::map::{
    LayerKind, LayerTile, MapLayer, MapObject, ObjectShape, Properties, PropertyValue, TiledMap,
};
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, AssetServer, Assets, Handle};
use cg_core::glam::{Vec2, vec2};
use cg_render::{Layer, Sprite, Visibility};
use cg_world::{Prefab, PrefabComponent, PrefabRegistry, Prefabs, Transform2D, insert_prefab};
use serde::{Deserialize, Serialize};

/// Draws a [`TiledMap`] at the entity's transform and spawns its objects as children.
///
/// The map's bottom-left corner sits at the entity's origin, one Tiled pixel per world
/// unit (see [`TiledMap`] for the coordinates). Map layer `i` draws on
/// [`Layer`]`(layer + i)`, so put sprites that walk in front of the map on a higher layer.
/// Tile layers are cut into 16×16-tile chunks that are built once and culled as a whole;
/// image layers can repeat; layers with parallax scroll at their own speed.
///
/// Each object becomes a child entity with a [`TiledObject`] (its id, name, class, shape
/// and properties), a [`Transform2D`] (centred on rectangles, ellipses and tile objects)
/// and a [`Layer`]. Tile objects get a [`Sprite`] of their tile. Then:
///
/// - The object's class (or a `prefab` string property) names a prefab: when
///   [`Prefabs`] has it, the object spawns as that prefab, at the object's position and
///   rotation.
/// - A property named after a registered prefab component sets that component, replacing
///   the prefab's: a class-typed property gives its members as fields
///   (`(speed: 40, distance: 64)`), a string property is read as RON, and a number or
///   boolean becomes a one-field component such as `Layer(2)`.
///
/// When the map asset changes (a hot reload, say), the chunks are rebuilt and the objects
/// despawned and spawned again.
///
/// ```
/// use cg_assets::AssetServer;
/// use cg_core::prelude::*;
/// use cg_tiled::Tilemap;
///
/// fn load_level(mut commands: Commands, server: Res<AssetServer>) {
///     commands.spawn(Tilemap::new(server.load("maps/level1.tmx")));
/// }
/// ```
///
/// In prefabs: `"Tilemap": (map: "maps/level1.tmx", layer: 0)`.
#[derive(Component, Debug, Clone, PartialEq)]
#[require(Transform2D, Visibility, TilemapState)]
pub struct Tilemap {
    /// The map.
    pub map: Handle<TiledMap>,
    /// The draw layer of the map's first layer; each later map layer is one higher.
    pub layer: i32,
    /// Spawn the objects of object layers as entities. On by default.
    pub spawn_objects: bool,
}

impl Tilemap {
    /// `map` on draw layer 0 and up, with its objects.
    pub fn new(map: Handle<TiledMap>) -> Self {
        Self {
            map,
            layer: 0,
            spawn_objects: true,
        }
    }

    /// The same, with the map's first layer on draw layer `layer`.
    pub fn with_layer(mut self, layer: i32) -> Self {
        self.layer = layer;
        self
    }
}

/// The prefab form of a [`Tilemap`]: the map by its asset path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TilemapData {
    /// The map's asset path.
    pub map: String,
    /// See [`Tilemap::layer`].
    pub layer: i32,
    /// See [`Tilemap::spawn_objects`].
    pub spawn_objects: bool,
}

impl Default for TilemapData {
    fn default() -> Self {
        Self {
            map: String::new(),
            layer: 0,
            spawn_objects: true,
        }
    }
}

impl PrefabComponent for Tilemap {
    type Data = TilemapData;

    fn from_data(data: &TilemapData, world: &World) -> Result<Self, String> {
        let server = world
            .get_resource::<AssetServer>()
            .ok_or_else(|| format!("no asset server to load `{}`", data.map))?;
        Ok(Self {
            map: server.load(data.map.as_str()),
            layer: data.layer,
            spawn_objects: data.spawn_objects,
        })
    }
}

/// What a [`Tilemap`] has built from its map: the tile chunks and image layers to draw,
/// and the object entities. Managed by the plugin.
#[derive(Component, Debug, Default)]
pub struct TilemapState {
    built: Option<(AssetId<TiledMap>, u64)>,
    pub(crate) chunks: Vec<TileChunk>,
    pub(crate) images: Vec<ImageDraw>,
    /// The parallax origin in world units relative to the map entity.
    pub(crate) parallax_origin: Vec2,
    objects: Vec<Entity>,
}

impl TilemapState {
    /// Whether the map has been built.
    pub fn is_built(&self) -> bool {
        self.built.is_some()
    }

    /// The entities spawned from the map's objects.
    pub fn objects(&self) -> &[Entity] {
        &self.objects
    }

    /// The number of tiles in the built chunks.
    pub fn tile_count(&self) -> usize {
        self.chunks.iter().map(|chunk| chunk.sprites.len()).sum()
    }

    /// The number of built chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

/// An entity spawned from a Tiled object: what the map says about it.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct TiledObject {
    /// The object's id, unique in its map.
    pub id: u32,
    /// The object's name.
    pub name: String,
    /// The object's class (or its tile's class).
    pub class: String,
    /// The name of the object layer it came from.
    pub layer: String,
    /// The shape in world units relative to the entity, y up and unrotated: rectangles
    /// and ellipses are centred on the entity, polygon points are relative to it.
    pub shape: ObjectShape,
    /// The tile, for tile objects.
    pub tile: Option<LayerTile>,
    /// The object's custom properties.
    pub properties: Properties,
}

/// Builds tilemaps whose map has loaded or changed since they were last built.
pub(crate) fn build_tilemaps(
    mut commands: Commands,
    mut tilemaps: Query<(Entity, &Tilemap, &mut TilemapState)>,
    maps: Res<Assets<TiledMap>>,
    server: Option<Res<AssetServer>>,
) {
    for (entity, tilemap, mut state) in &mut tilemaps {
        let id = tilemap.map.id();
        let (Some(version), Some(map)) = (maps.version(id), maps.get(id)) else {
            continue;
        };
        if state.built == Some((id, version)) {
            continue;
        }
        let rebuilt = state.built.is_some();
        for object in state.objects.drain(..) {
            commands.entity(object).try_despawn();
        }
        let (chunks, images) = draw::build(map, tilemap.layer);
        state.chunks = chunks;
        state.images = images;
        state.parallax_origin = map.to_world(map.parallax_origin);
        if tilemap.spawn_objects {
            state.objects = spawn_objects(&mut commands, entity, map, tilemap.layer);
        }
        state.built = Some((id, version));
        let path = server
            .as_ref()
            .and_then(|server| server.path(&tilemap.map))
            .unwrap_or_else(|| "a map".into());
        let summary = format!(
            "{} tiles in {} chunks, {} objects",
            state.tile_count(),
            state.chunks.len(),
            state.objects.len()
        );
        if rebuilt {
            log::info!("rebuilt `{path}`: {summary}");
        } else {
            log::debug!("built `{path}`: {summary}");
        }
    }
}

fn spawn_objects(
    commands: &mut Commands,
    map_entity: Entity,
    map: &TiledMap,
    base: i32,
) -> Vec<Entity> {
    let mut spawned = Vec::new();
    for (index, layer) in map.layers.iter().enumerate() {
        let LayerKind::Objects(objects) = &layer.kind else {
            continue;
        };
        for object in objects {
            spawned.push(spawn_object(
                commands,
                map_entity,
                map,
                layer,
                base + index as i32,
                object,
            ));
        }
    }
    spawned
}

fn spawn_object(
    commands: &mut Commands,
    map_entity: Entity,
    map: &TiledMap,
    layer: &MapLayer,
    draw_layer: i32,
    object: &MapObject,
) -> Entity {
    let transform = object_transform(map, object);
    let label = if object.name.is_empty() {
        format!("object {}", object.id)
    } else {
        format!("object {} `{}`", object.id, object.name)
    };
    let name = [&object.name, &object.class]
        .into_iter()
        .find(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("object {}", object.id));
    let mut entity = commands.spawn((
        TiledObject {
            id: object.id,
            name: object.name.clone(),
            class: object.class.clone(),
            layer: layer.name.clone(),
            shape: world_shape(&object.shape),
            tile: object.tile,
            properties: object.properties.clone(),
        },
        transform,
        Name::new(name),
        Layer(draw_layer),
        ChildOf(map_entity),
    ));
    if !(layer.visible && object.visible) {
        entity.insert(Visibility::Hidden);
    }
    if let (Some(tile), ObjectShape::Rect(size)) = (object.tile, &object.shape)
        && let Some(sprite) = tile_object_sprite(map, layer, tile, *size)
    {
        entity.insert(sprite);
        if let Some(animation) = map
            .tilesets
            .get(tile.tileset)
            .and_then(|tileset| TileAnimation::of_tile(tileset, tile.id))
        {
            entity.insert(animation);
        }
    }
    let id = entity.id();
    let prefab = object
        .properties
        .str("prefab")
        .unwrap_or(&object.class)
        .to_owned();
    let properties = object.properties.clone();
    if !prefab.is_empty() || !properties.is_empty() {
        commands.queue(move |world: &mut World| {
            apply_prefab_and_properties(world, id, &prefab, &properties, transform, &label);
        });
    }
    id
}

/// Where an object's entity goes, relative to the map entity: the centre of rectangles,
/// ellipses and tile objects, and Tiled's anchor point otherwise.
fn object_transform(map: &TiledMap, object: &MapObject) -> Transform2D {
    // Tiled turns clockwise with y down; the engine turns counter-clockwise with y up.
    let rotation = -object.rotation.to_radians();
    let anchor = map.to_world(object.position);
    let to_centre = match (&object.shape, object.tile) {
        // Tile objects hang from their bottom-left corner.
        (ObjectShape::Rect(size), Some(_)) => *size / 2.0,
        // Rectangles and ellipses hang from their top-left corner.
        (ObjectShape::Rect(size) | ObjectShape::Ellipse(size), None) => vec2(size.x, -size.y) / 2.0,
        _ => Vec2::ZERO,
    };
    let centre = anchor + Vec2::from_angle(rotation).rotate(to_centre);
    Transform2D::from_xy(centre.x, centre.y).with_rotation(rotation)
}

/// An object's shape in world units relative to its entity.
fn world_shape(shape: &ObjectShape) -> ObjectShape {
    let flip = |points: &[Vec2]| points.iter().map(|p| vec2(p.x, -p.y)).collect();
    match shape {
        ObjectShape::Polygon(points) => ObjectShape::Polygon(flip(points)),
        ObjectShape::Polyline(points) => ObjectShape::Polyline(flip(points)),
        other => other.clone(),
    }
}

fn tile_object_sprite(
    map: &TiledMap,
    layer: &MapLayer,
    tile: LayerTile,
    size: Vec2,
) -> Option<Sprite> {
    let tileset = map.tilesets.get(tile.tileset)?;
    let texture = tileset.image.clone()?;
    if size.x == 0.0 || size.y == 0.0 {
        return None;
    }
    // The tileset's offset (x right, y down) moves the picture, not the object.
    let offset = vec2(tileset.offset.x as f32, -(tileset.offset.y as f32));
    Some(Sprite {
        size: Some(size),
        source: Some(tileset.tile_rect(tile.id)),
        pivot: Vec2::splat(0.5) - offset / size,
        color: layer.color(),
        flip_x: tile.flip_x,
        flip_y: tile.flip_y,
        ..Sprite::new(texture)
    })
}

/// Spawns the object's prefab into its entity, then applies properties that name
/// components.
fn apply_prefab_and_properties(
    world: &mut World,
    entity: Entity,
    prefab: &str,
    properties: &Properties,
    placed: Transform2D,
    label: &str,
) {
    if world.get_entity(entity).is_err() {
        return;
    }
    if !prefab.is_empty() && world.resource::<Prefabs>().contains(prefab) {
        if let Err(err) = insert_prefab(world, entity, prefab) {
            log::error!("{label}: prefab `{prefab}`: {err}");
        }
        // The object decides where the entity is; the prefab may still scale it.
        if let Some(mut transform) = world.get_mut::<Transform2D>(entity) {
            transform.translation = placed.translation;
            transform.rotation = placed.rotation;
        }
    } else if !prefab.is_empty() {
        log::debug!("{label}: no prefab called `{prefab}`");
    }

    let Some(registry) = world.get_resource::<PrefabRegistry>() else {
        return;
    };
    let components: Vec<_> = properties
        .iter()
        .filter(|(name, _)| registry.contains(name))
        .map(|(name, value)| (name.to_owned(), value.clone()))
        .collect();
    for (name, value) in components {
        match component_from_property(world.resource::<PrefabRegistry>(), &name, &value) {
            Ok(prefab) => {
                if let Err(err) = prefab.insert_into(world, entity) {
                    log::error!("{label}: property `{name}`: {err}");
                }
            }
            Err(err) => log::error!("{label}: property `{name}`: {err}"),
        }
    }
}

/// A one-component prefab from a property named after the component.
fn component_from_property(
    registry: &PrefabRegistry,
    name: &str,
    value: &PropertyValue,
) -> Result<Prefab, String> {
    let wrap = |ron: &str| format!("(components: {{ {name:?}: {ron} }})");
    let parse = |ron: &str| Prefab::from_ron(&wrap(ron), registry).map_err(|err| err.to_string());
    match value {
        PropertyValue::Class { .. } => parse(&value.to_ron()),
        // A string is RON text; failing that, the string itself (for `Name`, say).
        PropertyValue::String(text) => parse(text).or_else(|_| parse(&value.to_ron())),
        // Numbers, booleans and the rest fill a one-field component.
        _ => parse(&format!("({})", value.to_ron())),
    }
}
