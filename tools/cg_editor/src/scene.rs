//! Showing the open document in the world: a map's edited objects go into the map asset
//! (so the engine draws and spawns them as the game would), and a prefab is spawned as a
//! preview.

use crate::doc::{MapDoc, PrefabNode};
use crate::tmx::{ObjectData, PropertyData, Shape};
use crate::value::Value;
use cashgear_engine::assets::path;
use cashgear_engine::prelude::*;
use cashgear_engine::tiled::{LayerKind, MapObject, ObjectShape, Properties, PropertyValue};
use cashgear_engine::world::PrefabRegistry;

/// The editor's own entity showing the document: the map's `Tilemap`, or the prefab's
/// root.
#[derive(Component, Debug, Clone, Copy)]
pub struct EditorScene;

/// The object as the engine sees it: what the map loader would make of it, tile and
/// template defaults included.
pub fn to_map_object(
    map: &TiledMap,
    layer_offset: Vec2,
    object: &ObjectData,
    template: Option<&MapObject>,
    map_path: &str,
) -> MapObject {
    let tile = if object.gid != 0 {
        map.tile_for_gid(object.gid)
    } else {
        template.and_then(|t| t.tile)
    };
    let size = vec2(object.width, object.height);
    let shape = match &object.shape {
        // A template object without its own size takes the template's shape.
        Shape::Rect if size == Vec2::ZERO && template.is_some() => {
            template.map_or(ObjectShape::Rect(size), |t| t.shape.clone())
        }
        Shape::Rect | Shape::Text(_) => ObjectShape::Rect(size),
        Shape::Ellipse => ObjectShape::Ellipse(size),
        Shape::Point => ObjectShape::Point,
        Shape::Polygon(_) => ObjectShape::Polygon(points(&object.shape)),
        Shape::Polyline(_) => ObjectShape::Polyline(points(&object.shape)),
    };
    let tile_info = tile.and_then(|tile| map.tile_info(tile));
    let mut properties = match template {
        Some(template) => template.properties.clone(),
        None => tile_info
            .map(|info| info.properties.clone())
            .unwrap_or_default(),
    };
    for property in &object.properties {
        if let Some(value) = property_value(&property.value, map_path) {
            properties.0.insert(property.name.clone(), value);
        }
    }
    let mut class = object.class.clone();
    if class.is_empty() {
        class = template
            .map(|t| t.class.clone())
            .filter(|c| !c.is_empty())
            .or_else(|| tile_info.map(|info| info.class.clone()))
            .unwrap_or_default();
    }
    MapObject {
        id: object.id,
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

fn points(shape: &Shape) -> Vec<Vec2> {
    shape
        .points()
        .into_iter()
        .map(|(x, y)| vec2(x, y))
        .collect()
}

/// A property as the engine reads it; `None` for kinds the editor keeps as raw text.
pub fn property_value(data: &PropertyData, map_path: &str) -> Option<PropertyValue> {
    Some(match data {
        PropertyData::String(text) => PropertyValue::String(text.clone()),
        PropertyData::Int(value) => PropertyValue::Int(*value),
        PropertyData::Float(value) => PropertyValue::Float(*value),
        PropertyData::Bool(value) => PropertyValue::Bool(*value),
        PropertyData::Color(text) => PropertyValue::Color(parse_color(text)?),
        PropertyData::File(text) if text.is_empty() => PropertyValue::File(String::new()),
        PropertyData::File(text) => PropertyValue::File(path::resolve(map_path, text)),
        PropertyData::Object(id) => PropertyValue::Object(*id),
        PropertyData::Class { class, members } => PropertyValue::Class {
            class: class.clone(),
            properties: Properties(
                members
                    .iter()
                    .filter_map(|m| Some((m.name.clone(), property_value(&m.value, map_path)?)))
                    .collect(),
            ),
        },
        PropertyData::Raw(_) => return None,
    })
}

/// Tiled's `#aarrggbb` or `#rrggbb`.
pub fn parse_color(text: &str) -> Option<Color> {
    let hex = text.strip_prefix('#')?;
    let value = u32::from_str_radix(hex, 16).ok()?;
    let byte = |shift: u32| ((value >> shift) & 0xff) as u8;
    match hex.len() {
        6 => Some(Color::rgba8(byte(16), byte(8), byte(0), 255)),
        8 => Some(Color::rgba8(byte(16), byte(8), byte(0), byte(24))),
        _ => None,
    }
}

/// Tiled's `#aarrggbb` for a colour.
pub fn color_text(color: Color) -> String {
    let [r, g, b, a] = color.to_rgba8();
    format!("#{a:02x}{r:02x}{g:02x}{b:02x}")
}

/// The offset of each object layer, by layer id, in Tiled pixels.
pub fn layer_offset(map: &TiledMap, layer_id: u32) -> Vec2 {
    map.layers
        .iter()
        .find(|layer| layer.id == layer_id)
        .map_or(Vec2::ZERO, |layer| layer.offset)
}

/// The engine's view of one object of the document, or `None` before the map loads.
pub fn engine_object(doc: &MapDoc, maps: &Assets<TiledMap>, id: u32) -> Option<MapObject> {
    let map = maps.get(&doc.handle)?;
    let (layer, object) = doc.find(id)?;
    let layer_id = doc.objects.layers[layer].id;
    Some(to_map_object(
        map,
        layer_offset(map, layer_id),
        object,
        doc.templates.get(&id),
        &doc.path,
    ))
}

/// Writes the document's object layers into the map asset, so the tilemap spawns the
/// objects again as edited. Returns whether the map was there to write into.
pub fn apply_map(doc: &mut MapDoc, maps: &mut Assets<TiledMap>) -> bool {
    let Some(map) = maps.get(&doc.handle) else {
        return false;
    };
    let converted: Vec<(u32, Vec<MapObject>)> = doc
        .objects
        .layers
        .iter()
        .map(|layer| {
            let offset = layer_offset(map, layer.id);
            let objects = layer
                .objects
                .iter()
                .map(|o| to_map_object(map, offset, o, doc.templates.get(&o.id), &doc.path))
                .collect();
            (layer.id, objects)
        })
        .collect();
    let Some(map) = maps.get_mut(&doc.handle) else {
        return false;
    };
    for (id, objects) in converted {
        if let Some(layer) = map.layers.iter_mut().find(|l| l.id == id)
            && let LayerKind::Objects(old) = &mut layer.kind
        {
            *old = objects;
        }
    }
    doc.synced_version = maps.version(&doc.handle);
    true
}

/// The objects of the loaded map that come from templates, as the loader resolved them.
pub fn capture_templates(doc: &mut MapDoc, maps: &Assets<TiledMap>) {
    let Some(map) = maps.get(&doc.handle) else {
        return;
    };
    let with_templates: Vec<u32> = doc
        .objects
        .layers
        .iter()
        .flat_map(|l| l.objects.iter())
        .filter(|o| o.template.is_some())
        .map(|o| o.id)
        .collect();
    doc.templates = map
        .objects()
        .filter(|(_, o)| with_templates.contains(&o.id))
        .map(|(_, o)| (o.id, o.clone()))
        .collect();
}

/// Reads a prefab file into nodes, each component's data captured as a [`Value`].
pub fn read_prefab(source: &str, registry: &PrefabRegistry) -> Result<PrefabNode, String> {
    let prefab = Prefab::from_ron(source, registry).map_err(|err| err.to_string())?;
    node_of(&prefab)
}

fn node_of(prefab: &Prefab) -> Result<PrefabNode, String> {
    Ok(PrefabNode {
        components: prefab
            .components()
            .map(|(name, data)| Ok((name.to_owned(), Value::from_serialize(data)?)))
            .collect::<Result<_, String>>()?,
        children: prefab
            .children()
            .iter()
            .map(node_of)
            .collect::<Result<_, _>>()?,
    })
}

/// The comment lines at the top of a prefab file.
pub fn prefab_header(source: &str) -> String {
    let mut header = String::new();
    for line in source.lines() {
        if line.trim_start().starts_with("//") {
            header.push_str(line.trim_end());
            header.push('\n');
        } else {
            break;
        }
    }
    header
}

/// Spawns the prefab document as the editor's preview; returns the root entity.
pub fn spawn_prefab(world: &mut World, root: &PrefabNode) -> Result<Entity, String> {
    let prefab = {
        let registry = world.resource::<PrefabRegistry>();
        Prefab::from_ron(&root.to_ron(""), registry).map_err(|err| err.to_string())?
    };
    let (entity, result) = prefab.spawn(world);
    world
        .entity_mut(entity)
        .insert((EditorScene, Name::new("editor: prefab")));
    if world.get::<Transform2D>(entity).is_none() {
        world.entity_mut(entity).insert(Transform2D::default());
    }
    if let Err(err) = result {
        log::warn!("prefab preview: {err}");
    }
    Ok(entity)
}

/// The preview entity of the node at `path` (child indices), found through `Children`.
pub fn node_entity(world: &World, root: Entity, path: &[usize]) -> Option<Entity> {
    let mut entity = root;
    for &index in path {
        entity = *world.get::<Children>(entity)?.get(index)?;
    }
    Some(entity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_read_as_tiled_writes_them() {
        assert_eq!(
            parse_color("#ff112233"),
            Some(Color::rgba8(0x11, 0x22, 0x33, 0xff))
        );
        assert_eq!(
            parse_color("#80112233"),
            Some(Color::rgba8(0x11, 0x22, 0x33, 0x80))
        );
        assert_eq!(
            parse_color("#112233"),
            Some(Color::rgba8(0x11, 0x22, 0x33, 255))
        );
        assert_eq!(parse_color(""), None);
        assert_eq!(
            color_text(Color::rgba8(0x11, 0x22, 0x33, 0x80)),
            "#80112233"
        );
    }
}
