//! The inspector: the selected map object's fields and custom properties, or the selected
//! prefab node's components, edited field by field through their serde data.

use crate::Editor;
use crate::doc::{Document, Selected, set_property};
use crate::scene::{self, EditorScene};
use crate::tmx::{Property, PropertyData, Shape};
use crate::value::{self, Value};
use cashgear_engine::prelude::*;
use cashgear_engine::tiled::TiledObject;
use cashgear_engine::ui::{Inspector, egui};
use cashgear_engine::world::PrefabRegistry;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// What the inspector remembers between frames.
#[derive(Default)]
pub(crate) struct InspectorState {
    new_property: String,
    new_type: usize,
    add_component: String,
    /// Components being edited as RON text, by a key naming the component.
    ron_text: HashMap<String, String>,
    /// Errors from the last edit of a component, by the same key.
    errors: HashMap<String, String>,
    /// Enum variants found by probing, by component and enum name.
    variants: HashMap<(String, &'static str), Vec<String>>,
}

/// The types a new property can have.
const TYPES: [&str; 7] = ["string", "int", "float", "bool", "color", "file", "object"];

pub(crate) fn show(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui) {
    ui.heading("Inspector");
    if editor.playing() {
        ui.weak("Playing. Stop to edit; the scene comes back as it was.");
        return;
    }
    match &editor.doc {
        None => {
            ui.weak("Nothing is open.");
        }
        Some(Document::Map(_)) => map_inspector(editor, world, ui),
        Some(Document::Prefab(_)) => prefab_inspector(editor, world, ui),
    }
}

fn hash_of(parts: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut hasher);
    hasher.finish()
}

// --- Map objects ----------------------------------------------------------------------

fn map_inspector(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui) {
    let ids: Vec<u32> = editor
        .selection
        .iter()
        .filter_map(|s| match s {
            Selected::Object(id) => Some(*id),
            Selected::Node(_) => None,
        })
        .collect();
    let Some(doc) = editor.map() else {
        return;
    };
    match ids.as_slice() {
        [] => {
            let maps = world.resource::<Assets<TiledMap>>();
            ui.label(&doc.path);
            if let Some(map) = maps.get(&doc.handle) {
                ui.weak(format!(
                    "{}×{} tiles of {}×{} pixels, {} layers",
                    map.width,
                    map.height,
                    map.tile_size.x,
                    map.tile_size.y,
                    map.layers.len()
                ));
            }
            let count: usize = doc.objects.layers.iter().map(|l| l.objects.len()).sum();
            ui.weak(format!(
                "{count} objects in {} object layers",
                doc.objects.layers.len()
            ));
            ui.add_space(6.0);
            ui.weak("Click an object to edit it. The tiles are Tiled's: save them there and they show here.");
        }
        [id] => object_inspector(editor, world, ui, *id),
        many => {
            ui.label(format!("{} objects selected", many.len()));
            ui.horizontal(|ui| {
                if ui.button("Duplicate").clicked() {
                    crate::view::duplicate(editor, world);
                }
                if ui.button("Delete").clicked() {
                    crate::view::delete(editor);
                }
            });
        }
    }
}

fn object_inspector(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui, id: u32) {
    let Some(doc) = editor.map() else {
        return;
    };
    let Some((layer, original)) = doc.find(id).map(|(l, o)| (l, o.clone())) else {
        return;
    };
    let layer_names: Vec<String> = doc.objects.layers.iter().map(|l| l.name.clone()).collect();
    let maps = world.resource::<Assets<TiledMap>>();
    let map = maps.get(&doc.handle);
    let engine = scene::engine_object(doc, maps, id);
    let tile = original.gid != 0;
    let mut object = original.clone();
    let mut changed: Option<&'static str> = None;
    let mut to_layer = layer;

    // Classes the map's tiles give, the prefabs, and those already in the map.
    let mut classes: Vec<String> = world
        .resource::<Prefabs>()
        .names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    if let Some(map) = map {
        for tileset in &map.tilesets {
            classes.extend(tileset.tiles.values().map(|t| t.class.clone()));
        }
    }
    classes.extend(
        doc.objects
            .layers
            .iter()
            .flat_map(|l| l.objects.iter().map(|o| o.class.clone())),
    );
    classes.retain(|c| !c.is_empty());
    classes.sort();
    classes.dedup();

    let kind = match (&object.shape, tile) {
        (_, true) => "tile object",
        (Shape::Rect, false) => "rectangle",
        (Shape::Ellipse, _) => "ellipse",
        (Shape::Point, _) => "point",
        (Shape::Polygon(_), _) => "polygon",
        (Shape::Polyline(_), _) => "polyline",
        (Shape::Text(_), _) => "text",
    };
    ui.label(egui::RichText::new(format!("Object #{id}, a {kind}")).strong());
    if let Some(engine) = &engine
        && object.class.is_empty()
        && !engine.class.is_empty()
    {
        ui.weak(format!("Class `{}` from its tile", engine.class));
    }
    egui::Grid::new(("cg_editor object", id))
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            ui.label("Name");
            if ui.text_edit_singleline(&mut object.name).changed() {
                changed = Some("name");
            }
            ui.end_row();
            ui.label("Class");
            ui.horizontal(|ui| {
                if ui
                    .add(egui::TextEdit::singleline(&mut object.class).desired_width(130.0))
                    .changed()
                {
                    changed = Some("class");
                }
                ui.menu_button("Pick", |ui| {
                    for class in &classes {
                        if ui.button(class).clicked() {
                            object.class.clone_from(class);
                            changed = Some("class pick");
                            ui.close();
                        }
                    }
                });
            });
            ui.end_row();
            ui.label("Layer");
            egui::ComboBox::from_id_salt(("cg_editor layer pick", id))
                .selected_text(layer_names.get(layer).cloned().unwrap_or_default())
                .show_ui(ui, |ui| {
                    for (i, name) in layer_names.iter().enumerate() {
                        ui.selectable_value(&mut to_layer, i, name);
                    }
                });
            ui.end_row();
            ui.label("Position");
            ui.horizontal(|ui| {
                for (value, field) in [(&mut object.x, "x"), (&mut object.y, "y")] {
                    if ui.add(egui::DragValue::new(value).speed(0.5)).changed() {
                        changed = Some(field);
                    }
                }
            });
            ui.end_row();
            if !matches!(
                object.shape,
                Shape::Point | Shape::Polygon(_) | Shape::Polyline(_)
            ) {
                ui.label("Size");
                ui.horizontal(|ui| {
                    for (value, field) in
                        [(&mut object.width, "width"), (&mut object.height, "height")]
                    {
                        if ui
                            .add(egui::DragValue::new(value).speed(0.5).range(0.0..=f32::MAX))
                            .changed()
                        {
                            changed = Some(field);
                        }
                    }
                });
                ui.end_row();
            }
            ui.label("Rotation");
            if ui
                .add(
                    egui::DragValue::new(&mut object.rotation)
                        .speed(0.5)
                        .suffix("°"),
                )
                .changed()
            {
                changed = Some("rotation");
            }
            ui.end_row();
            ui.label("Visible");
            if ui.checkbox(&mut object.visible, "").changed() {
                changed = Some("visible");
            }
            ui.end_row();
            if tile {
                ui.label("Tile");
                ui.horizontal(|ui| {
                    let mut flip_x = object.gid & 0x8000_0000 != 0;
                    let mut flip_y = object.gid & 0x4000_0000 != 0;
                    if ui.checkbox(&mut flip_x, "Flip X").changed() {
                        object.gid ^= 0x8000_0000;
                        changed = Some("flip");
                    }
                    if ui.checkbox(&mut flip_y, "Flip Y").changed() {
                        object.gid ^= 0x4000_0000;
                        changed = Some("flip");
                    }
                    ui.weak(format!("gid {}", object.gid & 0x0fff_ffff));
                });
                ui.end_row();
            }
        });

    ui.add_space(6.0);
    ui.label(egui::RichText::new("Properties").strong());
    let registry_names: Vec<String> = world
        .resource::<PrefabRegistry>()
        .names()
        .map(str::to_owned)
        .collect();
    let mut remove = None;
    let mut properties = core::mem::take(&mut object.properties);
    for (index, property) in properties.iter_mut().enumerate() {
        ui.push_id(("cg_editor property", id, index), |ui| {
            ui.horizontal(|ui| {
                let component = registry_names.contains(&property.name);
                let label = if component {
                    egui::RichText::new(&property.name)
                        .color(egui::Color32::from_rgb(130, 200, 255))
                } else {
                    egui::RichText::new(&property.name)
                };
                ui.label(label).on_hover_text(if component {
                    "Named after a component: it sets that component on the object."
                } else {
                    property.value.type_name()
                });
                if ui.small_button("x").on_hover_text("Remove").clicked() {
                    remove = Some(index);
                }
            });
            if property_value_ui(
                ui,
                &mut editor.inspector,
                world,
                &property.name,
                &mut property.value,
                registry_names.contains(&property.name),
            ) {
                changed = Some("property");
            }
        });
    }
    if let Some(index) = remove {
        properties.remove(index);
        changed = Some("remove property");
    }
    object.properties = properties;

    // The tile's properties, which the object has unless it sets its own.
    if let Some(engine) = &engine {
        let inherited: Vec<(String, String)> = engine
            .properties
            .iter()
            .filter(|(name, _)| object.property(name).is_none())
            .map(|(name, value)| (name.to_owned(), value.to_ron()))
            .collect();
        if !inherited.is_empty() {
            ui.weak("From the tile:");
            for (name, value) in inherited {
                ui.horizontal(|ui| {
                    ui.weak(format!("{name} = {value}"));
                    if ui.small_button("Override").clicked() {
                        let data = engine
                            .properties
                            .get(&name)
                            .map(property_data)
                            .unwrap_or(PropertyData::String(String::new()));
                        set_property(&mut object, &name, data);
                        changed = Some("override");
                    }
                });
            }
        }
    }

    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let state = &mut editor.inspector;
        ui.add(
            egui::TextEdit::singleline(&mut state.new_property)
                .hint_text("new property")
                .desired_width(110.0),
        );
        egui::ComboBox::from_id_salt("cg_editor new type")
            .selected_text(TYPES[state.new_type])
            .width(64.0)
            .show_ui(ui, |ui| {
                for (i, name) in TYPES.iter().enumerate() {
                    ui.selectable_value(&mut state.new_type, i, *name);
                }
            });
        let name = state.new_property.trim().to_owned();
        if ui
            .add_enabled(
                !name.is_empty() && object.property(&name).is_none(),
                egui::Button::new("Add"),
            )
            .clicked()
        {
            let value = match TYPES[state.new_type] {
                "int" => PropertyData::Int(0),
                "float" => PropertyData::Float(0.0),
                "bool" => PropertyData::Bool(false),
                "color" => PropertyData::Color("#ffffffff".into()),
                "file" => PropertyData::File(String::new()),
                "object" => PropertyData::Object(0),
                _ => PropertyData::String(String::new()),
            };
            object.properties.push(Property { name, value });
            state.new_property.clear();
            changed = Some("add property");
        }
    });
    ui.horizontal(|ui| {
        ui.weak("Set a component:");
        egui::ComboBox::from_id_salt("cg_editor add override")
            .selected_text("choose...")
            .width(120.0)
            .show_ui(ui, |ui| {
                for name in &registry_names {
                    if object.property(name).is_none() && ui.button(name).clicked() {
                        let registry = world.resource::<PrefabRegistry>();
                        let ron = value::parse_component(registry, name, "()")
                            .map(|v| v.to_ron())
                            .unwrap_or_default();
                        object.properties.push(Property {
                            name: name.clone(),
                            value: PropertyData::String(ron),
                        });
                        changed = Some("add component");
                    }
                }
            });
    });

    spawned(ui, world, id);

    if let Some(field) = changed
        && object != original
    {
        editor.edit(
            &format!("edit {field}"),
            Some(hash_of((id, field))),
            |doc, _| {
                if let Document::Map(map) = doc
                    && let Some(target) = map.find_mut(id)
                {
                    *target = object;
                }
            },
        );
    }
    if to_layer != layer {
        editor.edit("move to layer", None, |doc, _| {
            if let Document::Map(map) = doc {
                map.move_to_layer(&[id], to_layer, None);
            }
        });
    }
}

/// A property as the file would store it, from the engine's value.
fn property_data(value: &cashgear_engine::tiled::PropertyValue) -> PropertyData {
    use cashgear_engine::tiled::PropertyValue as V;
    match value {
        V::Bool(v) => PropertyData::Bool(*v),
        V::Int(v) => PropertyData::Int(*v),
        V::Float(v) => PropertyData::Float(*v),
        V::String(v) => PropertyData::String(v.clone()),
        V::Color(c) => PropertyData::Color(scene::color_text(*c)),
        V::File(v) => PropertyData::File(v.clone()),
        V::Object(id) => PropertyData::Object(*id),
        V::Class { class, properties } => PropertyData::Class {
            class: class.clone(),
            members: properties
                .iter()
                .map(|(name, v)| Property {
                    name: name.to_owned(),
                    value: property_data(v),
                })
                .collect(),
        },
        V::List(_) => PropertyData::String(value.to_ron()),
    }
}

/// Edits one property value; returns whether it changed.
fn property_value_ui(
    ui: &mut egui::Ui,
    state: &mut InspectorState,
    world: &World,
    name: &str,
    value: &mut PropertyData,
    component: bool,
) -> bool {
    match value {
        PropertyData::String(text) if component => {
            // RON for the component: edit its fields when it reads.
            let registry = world.resource::<PrefabRegistry>();
            match value::parse_component(
                registry,
                name,
                if text.trim().is_empty() { "()" } else { text },
            ) {
                Ok(mut parsed) => {
                    let before = parsed.clone();
                    let mut changed = false;
                    ui.indent(name, |ui| {
                        changed = value_ui(ui, state, registry, name, &mut parsed, &mut Vec::new());
                    });
                    if changed && parsed != before {
                        *text = parsed.to_ron();
                        return true;
                    }
                    false
                }
                Err(err) => {
                    let changed = ui
                        .add(
                            egui::TextEdit::multiline(text)
                                .desired_rows(1)
                                .code_editor(),
                        )
                        .changed();
                    ui.colored_label(egui::Color32::from_rgb(240, 120, 100), err);
                    changed
                }
            }
        }
        PropertyData::String(text) | PropertyData::File(text) => {
            if text.contains('\n') {
                ui.add(egui::TextEdit::multiline(text).desired_rows(2))
                    .changed()
            } else {
                ui.text_edit_singleline(text).changed()
            }
        }
        PropertyData::Int(v) => ui.add(egui::DragValue::new(v)).changed(),
        PropertyData::Float(v) => ui.add(egui::DragValue::new(v).speed(0.1)).changed(),
        PropertyData::Bool(v) => ui.checkbox(v, "").changed(),
        PropertyData::Object(v) => ui.add(egui::DragValue::new(v).prefix("#")).changed(),
        PropertyData::Color(text) => {
            let mut color = scene::parse_color(text).unwrap_or(Color::WHITE);
            let [r, g, b, a] = color.to_rgba8();
            let mut rgba = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
            if ui.color_edit_button_srgba(&mut rgba).changed() {
                let [r, g, b, a] = rgba.to_srgba_unmultiplied();
                color = Color::rgba8(r, g, b, a);
                *text = scene::color_text(color);
                return true;
            }
            false
        }
        PropertyData::Class { class, members } => {
            let mut changed = false;
            ui.indent(("class", name.to_owned()), |ui| {
                ui.weak(format!("class {class}"));
                for member in members.iter_mut() {
                    ui.horizontal(|ui| {
                        ui.label(&member.name);
                    });
                    changed |=
                        property_value_ui(ui, state, world, &member.name, &mut member.value, false);
                }
            });
            changed
        }
        PropertyData::Raw(raw) => {
            ui.weak("Kept as written (edit it in Tiled):");
            ui.add(
                egui::Label::new(egui::RichText::new(raw.as_str()).monospace().small()).truncate(),
            );
            false
        }
    }
}

/// The components the object's entity has in the editor's scene.
fn spawned(ui: &mut egui::Ui, world: &mut World, id: u32) {
    let mut objects = world.query::<(Entity, &TiledObject, &ChildOf)>();
    let scenes: Vec<Entity> = world
        .query_filtered::<Entity, With<EditorScene>>()
        .iter(world)
        .collect();
    let Some(entity) = objects
        .iter(world)
        .find(|(_, object, parent)| object.id == id && scenes.contains(&parent.parent()))
        .map(|(e, ..)| e)
    else {
        return;
    };
    ui.add_space(6.0);
    egui::CollapsingHeader::new("Spawned as")
        .id_salt(("cg_editor spawned", id))
        .show(ui, |ui| {
            ui.weak("What the engine made of it here (prefab and property components included). Game code adds more when it plays.");
            let Ok(components) = world.inspect_entity(entity) else {
                return;
            };
            let components: Vec<(String, Option<core::any::TypeId>)> = components
                .map(|(_, info)| (info.name().shortname().to_string(), info.type_id()))
                .collect();
            for (name, type_id) in components {
                let text = type_id.and_then(|t| world.get_resource::<Inspector>()?.describe(world, entity, t));
                match text {
                    Some(text) => {
                        egui::CollapsingHeader::new(&name).show(ui, |ui| {
                            ui.label(egui::RichText::new(text).monospace().small());
                        });
                    }
                    None => {
                        ui.label(&name);
                    }
                }
            }
        });
}

// --- Prefab nodes ---------------------------------------------------------------------

fn prefab_inspector(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui) {
    let path = match editor.selection.as_slice() {
        [Selected::Node(path)] => path.clone(),
        [] => Vec::new(),
        _ => {
            ui.label(format!("{} entities selected", editor.selection.len()));
            return;
        }
    };
    let Some(doc) = editor.prefab() else {
        return;
    };
    let Some(original) = doc.root.at(&path).cloned() else {
        return;
    };
    ui.label(
        egui::RichText::new(if path.is_empty() {
            "The prefab's root".to_owned()
        } else {
            format!(
                "Child {}",
                path.iter()
                    .map(usize::to_string)
                    .collect::<Vec<_>>()
                    .join(".")
            )
        })
        .strong(),
    );
    let mut node = original.clone();
    let mut changed: Option<String> = None;
    let mut remove = None;
    let registry_names: Vec<String> = world
        .resource::<PrefabRegistry>()
        .names()
        .map(str::to_owned)
        .collect();
    for (index, (name, value)) in node.components.iter_mut().enumerate() {
        let key = format!("{path:?}/{name}");
        egui::CollapsingHeader::new(egui::RichText::new(name.as_str()).strong())
            .id_salt(("cg_editor component", &key))
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut as_ron = editor.inspector.ron_text.contains_key(&key);
                    if ui
                        .toggle_value(&mut as_ron, "RON")
                        .on_hover_text("Edit as text")
                        .changed()
                    {
                        if as_ron {
                            editor
                                .inspector
                                .ron_text
                                .insert(key.clone(), value.to_ron());
                        } else {
                            editor.inspector.ron_text.remove(&key);
                        }
                    }
                    if ui.small_button("Remove").clicked() {
                        remove = Some(index);
                    }
                });
                let registry = world.resource::<PrefabRegistry>();
                if let Some(text) = editor.inspector.ron_text.get_mut(&key) {
                    if ui
                        .add(
                            egui::TextEdit::multiline(text)
                                .code_editor()
                                .desired_rows(2),
                        )
                        .changed()
                    {
                        match value::parse_component(registry, name, text) {
                            Ok(parsed) => {
                                *value = parsed;
                                editor.inspector.errors.remove(&key);
                                changed = Some(key.clone());
                            }
                            Err(err) => {
                                editor.inspector.errors.insert(key.clone(), err);
                            }
                        }
                    }
                } else if value_ui(
                    ui,
                    &mut editor.inspector,
                    registry,
                    name,
                    value,
                    &mut Vec::new(),
                ) {
                    // Only what the component reads back counts.
                    match value::component_prefab(registry, name, &value.to_ron()) {
                        Ok(_) => {
                            editor.inspector.errors.remove(&key);
                            changed = Some(key.clone());
                        }
                        Err(err) => {
                            editor.inspector.errors.insert(key.clone(), err);
                        }
                    }
                }
                if let Some(err) = editor.inspector.errors.get(&key) {
                    ui.colored_label(egui::Color32::from_rgb(240, 120, 100), err);
                }
            });
    }
    if let Some(index) = remove {
        node.components.remove(index);
        changed = Some("remove".into());
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label("Add component");
        egui::ComboBox::from_id_salt("cg_editor add component")
            .selected_text(if editor.inspector.add_component.is_empty() {
                "choose..."
            } else {
                &editor.inspector.add_component
            })
            .width(140.0)
            .show_ui(ui, |ui| {
                for name in &registry_names {
                    if node.component(name).is_none() && ui.button(name).clicked() {
                        let registry = world.resource::<PrefabRegistry>();
                        match value::parse_component(registry, name, "()") {
                            Ok(value) => {
                                node.components.push((name.clone(), value));
                                changed = Some("add".into());
                            }
                            Err(_) => {
                                // No defaults: start from text the user completes.
                                editor.inspector.add_component.clone_from(name);
                            }
                        }
                    }
                }
            });
    });
    if !editor.inspector.add_component.is_empty() {
        let name = editor.inspector.add_component.clone();
        let key = format!("{path:?}/new {name}");
        let text = editor
            .inspector
            .ron_text
            .entry(key.clone())
            .or_insert_with(|| "()".into());
        ui.weak(format!("`{name}` needs its fields; write them as RON:"));
        ui.add(
            egui::TextEdit::multiline(text)
                .code_editor()
                .desired_rows(2),
        );
        let registry = world.resource::<PrefabRegistry>();
        let parsed = value::parse_component(registry, &name, text);
        ui.horizontal(|ui| {
            match &parsed {
                Ok(_) => {
                    if ui.button("Add").clicked() {
                        node.components
                            .push((name.clone(), parsed.clone().unwrap_or(Value::Unit)));
                        changed = Some("add".into());
                        editor.inspector.add_component.clear();
                    }
                }
                Err(err) => {
                    ui.colored_label(egui::Color32::from_rgb(240, 120, 100), err);
                }
            }
            if ui.button("Cancel").clicked() {
                editor.inspector.add_component.clear();
            }
        });
    }
    if let Some(field) = changed
        && node != original
    {
        let label = format!("edit {}", field.rsplit('/').next().unwrap_or("component"));
        editor.edit(&label, Some(hash_of((&path, &field))), |doc, _| {
            if let Document::Prefab(prefab) = doc
                && let Some(target) = prefab.root.at_mut(&path)
            {
                *target = node;
            }
        });
    }
}

/// Edits a value of component `component` in place; `path` leads from the component's
/// value to this one (for finding enum variants). Returns whether it changed.
fn value_ui(
    ui: &mut egui::Ui,
    state: &mut InspectorState,
    registry: &PrefabRegistry,
    component: &str,
    value: &mut Value,
    path: &mut Vec<usize>,
) -> bool {
    value_ui_at(ui, state, registry, component, value, path, None)
}

#[allow(clippy::too_many_arguments)]
fn value_ui_at(
    ui: &mut egui::Ui,
    state: &mut InspectorState,
    registry: &PrefabRegistry,
    component: &str,
    value: &mut Value,
    path: &mut Vec<usize>,
    root: Option<&Value>,
) -> bool {
    // Variants are probed with the whole component, so keep the outermost value.
    let root_copy;
    let root = match root {
        Some(root) => root,
        None => {
            root_copy = value.clone();
            &root_copy
        }
    };
    let id = ui.scope_id().with(("cg_editor value", path.clone()));
    // Two numbers (glam's vectors) on one line.
    if value.is_pair()
        && let Some((a, b)) = value.as_pair_mut()
    {
        let mut changed = false;
        ui.horizontal(|ui| {
            changed |= number_ui(ui, a);
            changed |= number_ui(ui, b);
        });
        return changed;
    }
    let list = matches!(value, Value::Seq(_));
    match value {
        Value::Bool(v) => ui.checkbox(v, "").changed(),
        Value::Int(v) => ui.add(egui::DragValue::new(v)).changed(),
        Value::UInt(v) => ui.add(egui::DragValue::new(v)).changed(),
        Value::F32(v) => {
            let speed = drag_speed(f64::from(*v));
            ui.add(egui::DragValue::new(v).speed(speed)).changed()
        }
        Value::F64(v) => {
            let speed = drag_speed(*v);
            ui.add(egui::DragValue::new(v).speed(speed)).changed()
        }
        Value::Char(c) => {
            let mut text = c.to_string();
            let changed = ui
                .add(egui::TextEdit::singleline(&mut text).desired_width(24.0))
                .changed();
            if changed && let Some(first) = text.chars().next() {
                *c = first;
                return true;
            }
            false
        }
        Value::Str(text) => ui.text_edit_singleline(text).changed(),
        Value::Bytes(bytes) => {
            ui.weak(format!("{} bytes", bytes.len()));
            false
        }
        Value::None => {
            ui.weak("None (edit as RON to set it)");
            false
        }
        Value::Some(inner) => {
            let mut changed = false;
            let mut clear = false;
            ui.horizontal(|ui| {
                path.push(0);
                changed = value_ui_at(ui, state, registry, component, inner, path, Some(root));
                path.pop();
                clear = ui.small_button("None").on_hover_text("Clear").clicked();
            });
            if clear {
                *value = Value::None;
                return true;
            }
            changed
        }
        Value::Unit | Value::UnitStruct(_) => {
            ui.weak("(no fields)");
            false
        }
        Value::NewtypeStruct(_, inner) => {
            path.push(0);
            let changed = value_ui_at(ui, state, registry, component, inner, path, Some(root));
            path.pop();
            changed
        }
        Value::UnitVariant { .. }
        | Value::NewtypeVariant { .. }
        | Value::TupleVariant { .. }
        | Value::StructVariant { .. } => enum_ui(ui, state, registry, component, value, path, root),
        Value::Seq(items) | Value::Tuple(items) | Value::TupleStruct(_, items) => {
            let mut changed = false;
            let mut remove = None;
            ui.vertical(|ui| {
                for (i, item) in items.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.horizontal(|ui| {
                            ui.weak(format!("{i}"));
                            path.push(i);
                            changed |=
                                value_ui_at(ui, state, registry, component, item, path, Some(root));
                            path.pop();
                            if list && ui.small_button("x").clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                }
                if list
                    && let Some(last) = items.last().cloned()
                    && ui.small_button("+ copy the last").clicked()
                {
                    items.push(last);
                    changed = true;
                }
            });
            if let Some(i) = remove {
                items.remove(i);
                changed = true;
            }
            changed
        }
        Value::Map(entries) => {
            let mut changed = false;
            egui::Grid::new(id).num_columns(2).show(ui, |ui| {
                for (i, (key, item)) in entries.iter_mut().enumerate() {
                    ui.label(key.to_ron());
                    path.push(i);
                    changed |= value_ui_at(ui, state, registry, component, item, path, Some(root));
                    path.pop();
                    ui.end_row();
                }
            });
            changed
        }
        Value::Struct(_, fields) => {
            fields_ui(ui, state, registry, component, fields, path, root, id)
        }
    }
}

fn number_ui(ui: &mut egui::Ui, value: &mut Value) -> bool {
    match value {
        Value::Int(v) => ui.add(egui::DragValue::new(v)).changed(),
        Value::UInt(v) => ui.add(egui::DragValue::new(v)).changed(),
        Value::F32(v) => {
            let speed = drag_speed(f64::from(*v));
            ui.add(egui::DragValue::new(v).speed(speed)).changed()
        }
        Value::F64(v) => {
            let speed = drag_speed(*v);
            ui.add(egui::DragValue::new(v).speed(speed)).changed()
        }
        _ => false,
    }
}

/// Dragging moves a value by about a hundredth of its size.
fn drag_speed(v: f64) -> f64 {
    (v.abs() / 100.0).clamp(0.01, 10.0)
}

#[allow(clippy::too_many_arguments)]
fn fields_ui(
    ui: &mut egui::Ui,
    state: &mut InspectorState,
    registry: &PrefabRegistry,
    component: &str,
    fields: &mut [(&'static str, Value)],
    path: &mut Vec<usize>,
    root: &Value,
    id: egui::Id,
) -> bool {
    let mut changed = false;
    egui::Grid::new(id).num_columns(2).show(ui, |ui| {
        for (i, (name, item)) in fields.iter_mut().enumerate() {
            ui.label(*name);
            path.push(i);
            if item.is_scalar() || item.is_pair() {
                changed |= value_ui_at(ui, state, registry, component, item, path, Some(root));
            } else {
                ui.vertical(|ui| {
                    changed |= value_ui_at(ui, state, registry, component, item, path, Some(root));
                });
            }
            path.pop();
            ui.end_row();
        }
    });
    changed
}

/// An enum: a choice of variant, then the variant's data.
fn enum_ui(
    ui: &mut egui::Ui,
    state: &mut InspectorState,
    registry: &PrefabRegistry,
    component: &str,
    value: &mut Value,
    path: &mut Vec<usize>,
    root: &Value,
) -> bool {
    let (name, variant) = match value {
        Value::UnitVariant { name, variant }
        | Value::NewtypeVariant { name, variant, .. }
        | Value::TupleVariant { name, variant, .. }
        | Value::StructVariant { name, variant, .. } => (*name, *variant),
        _ => return false,
    };
    let key = (component.to_owned(), name);
    let variants = state
        .variants
        .entry(key)
        .or_insert_with(|| value::variants(registry, component, root, path))
        .clone();
    let mut changed = false;
    ui.vertical(|ui| {
        let mut chosen = variant.to_owned();
        egui::ComboBox::from_id_salt(ui.scope_id().with(("cg_editor variant", path.clone())))
            .selected_text(variant)
            .show_ui(ui, |ui| {
                for v in &variants {
                    ui.selectable_value(&mut chosen, v.clone(), v);
                }
            });
        if chosen != variant
            && let Some(switched) = switch_variant(registry, component, root, path, value, &chosen)
        {
            *value = switched;
            changed = true;
            return;
        }
        let id = ui.scope_id().with(("cg_editor variant data", path.clone()));
        match value {
            Value::NewtypeVariant { value: inner, .. } => {
                path.push(0);
                changed |= value_ui_at(ui, state, registry, component, inner, path, Some(root));
                path.pop();
            }
            Value::TupleVariant { fields, .. } => {
                for (i, item) in fields.iter_mut().enumerate() {
                    path.push(i);
                    changed |= value_ui_at(ui, state, registry, component, item, path, Some(root));
                    path.pop();
                }
            }
            Value::StructVariant { fields, .. } => {
                changed |= fields_ui(ui, state, registry, component, fields, path, root, id);
            }
            _ => {}
        }
    });
    changed
}

/// The value with its enum at `path` switched to `variant`: tried without data, then
/// with the data the current variant has. `None` if the component reads neither.
fn switch_variant(
    registry: &PrefabRegistry,
    component: &str,
    root: &Value,
    path: &[usize],
    current: &Value,
    variant: &str,
) -> Option<Value> {
    let leaked = value::intern(variant);
    let name = match current {
        Value::UnitVariant { name, .. }
        | Value::NewtypeVariant { name, .. }
        | Value::TupleVariant { name, .. }
        | Value::StructVariant { name, .. } => *name,
        _ => return None,
    };
    let mut candidates = vec![Value::UnitVariant {
        name,
        variant: leaked,
    }];
    let mut same = current.clone();
    match &mut same {
        Value::NewtypeVariant { variant, .. }
        | Value::TupleVariant { variant, .. }
        | Value::StructVariant { variant, .. } => {
            *variant = leaked;
            candidates.push(same);
        }
        _ => {}
    }
    for candidate in candidates {
        let mut whole = root.clone();
        if let Some(slot) = whole.at_mut(path) {
            *slot = candidate.clone();
        }
        if let Ok(read) = value::parse_component(registry, component, &whole.to_ron()) {
            let mut read = read;
            return read.at_mut(path).cloned();
        }
    }
    log::warn!(
        "{component}: `{variant}` needs data the editor cannot guess; edit the component as RON"
    );
    None
}
