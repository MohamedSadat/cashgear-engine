//! The documents the editor edits (a map's object layers, or a prefab), what is selected,
//! and the undo history. Plain data: the world only shows a document (see `scene`).

use crate::tmx::{MapObjects, ObjectData, Property, PropertyData};
use crate::value::Value;
use cashgear_engine::prelude::*;
use cashgear_engine::tiled::{MapObject, TiledMap};
use std::collections::HashMap;

/// The open document.
pub enum Document {
    /// A Tiled map, of which the editor edits the object layers.
    Map(MapDoc),
    /// A prefab file.
    Prefab(PrefabDoc),
}

impl Document {
    /// The document's asset path.
    pub fn path(&self) -> &str {
        match self {
            Self::Map(map) => &map.path,
            Self::Prefab(prefab) => &prefab.path,
        }
    }

    /// Whether it has changes that are not saved.
    pub fn dirty(&self) -> bool {
        match self {
            Self::Map(map) => map.objects != map.saved,
            Self::Prefab(prefab) => prefab.root != prefab.saved,
        }
    }

    /// The part undo restores.
    pub(crate) fn snapshot(&self) -> Snapshot {
        match self {
            Self::Map(map) => Snapshot::Map(map.objects.clone()),
            Self::Prefab(prefab) => Snapshot::Prefab(prefab.root.clone()),
        }
    }

    pub(crate) fn restore(&mut self, snapshot: Snapshot) {
        match (self, snapshot) {
            (Self::Map(map), Snapshot::Map(objects)) => map.objects = objects,
            (Self::Prefab(prefab), Snapshot::Prefab(root)) => prefab.root = root,
            _ => log::warn!("an undo step of another document was skipped"),
        }
    }
}

/// A map being edited.
pub struct MapDoc {
    /// The map's asset path.
    pub path: String,
    /// The map asset, kept loaded. The editor writes its object layers into it, so the
    /// view and Play show the edits before they are saved.
    pub handle: Handle<TiledMap>,
    /// The object layers as edited.
    pub objects: MapObjects,
    /// The object layers as last loaded or saved.
    pub saved: MapObjects,
    /// Objects made from templates, as the loader resolved them, by id: their shape and
    /// tile come from the template unless the object sets its own.
    pub templates: HashMap<u32, MapObject>,
    /// The map asset's version after the editor's own last change to it, so a change from
    /// elsewhere (the file saved in Tiled) can be told apart.
    pub synced_version: Option<u64>,
    /// The file changed on disk while this document had unsaved changes.
    pub conflict: bool,
}

impl MapDoc {
    /// The object with this id, with the index of its layer.
    pub fn find(&self, id: u32) -> Option<(usize, &ObjectData)> {
        self.objects
            .layers
            .iter()
            .enumerate()
            .find_map(|(layer, l)| l.objects.iter().find(|o| o.id == id).map(|o| (layer, o)))
    }

    /// The object with this id, mutably.
    pub fn find_mut(&mut self, id: u32) -> Option<&mut ObjectData> {
        self.objects
            .layers
            .iter_mut()
            .flat_map(|l| l.objects.iter_mut())
            .find(|o| o.id == id)
    }

    /// Removes the objects with these ids; returns them with their layer indices.
    pub fn remove(&mut self, ids: &[u32]) -> Vec<(usize, ObjectData)> {
        let mut removed = Vec::new();
        for (index, layer) in self.objects.layers.iter_mut().enumerate() {
            layer.objects.retain(|object| {
                if ids.contains(&object.id) {
                    removed.push((index, object.clone()));
                    false
                } else {
                    true
                }
            });
        }
        removed
    }

    /// Takes the next free object id.
    pub fn next_id(&mut self) -> u32 {
        let id = self.objects.next_object_id;
        self.objects.next_object_id += 1;
        id
    }

    /// Moves the objects `ids` into layer `to`, before the object `before` (or at the end),
    /// keeping their order.
    pub fn move_to_layer(&mut self, ids: &[u32], to: usize, before: Option<u32>) {
        if to >= self.objects.layers.len() {
            return;
        }
        let mut moving = self.remove(ids);
        // Keep the order they had in the map.
        moving.sort_by_key(|(layer, _)| *layer);
        let target = &mut self.objects.layers[to].objects;
        let at = before
            .and_then(|id| target.iter().position(|o| o.id == id))
            .unwrap_or(target.len());
        for (offset, (_, object)) in moving.into_iter().enumerate() {
            target.insert(at + offset, object);
        }
    }
}

/// A prefab being edited.
pub struct PrefabDoc {
    /// The prefab file's asset path.
    pub path: String,
    /// The comment lines at the top of the file, kept when saving.
    pub header: String,
    /// The prefab as edited.
    pub root: PrefabNode,
    /// The prefab as last loaded or saved.
    pub saved: PrefabNode,
}

/// One entity of a prefab: its components and children.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrefabNode {
    /// Components by registered name, in file order.
    pub components: Vec<(String, Value)>,
    /// Child entities.
    pub children: Vec<PrefabNode>,
}

impl PrefabNode {
    /// The node at `path` (child indices from this node).
    pub fn at(&self, path: &[usize]) -> Option<&PrefabNode> {
        match path.split_first() {
            None => Some(self),
            Some((&first, rest)) => self.children.get(first)?.at(rest),
        }
    }

    /// The node at `path`, mutably.
    pub fn at_mut(&mut self, path: &[usize]) -> Option<&mut PrefabNode> {
        match path.split_first() {
            None => Some(self),
            Some((&first, rest)) => self.children.get_mut(first)?.at_mut(rest),
        }
    }

    /// The component called `name`.
    pub fn component(&self, name: &str) -> Option<&Value> {
        self.components
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
    }

    /// Moves the node at `from` to be the last child of the node at `to`. Refused (false)
    /// for the root, or when `to` is `from` or inside it.
    pub fn reparent(&mut self, from: &[usize], to: &[usize]) -> bool {
        if from.is_empty() || to.starts_with(from) {
            return false;
        }
        let (parent_path, index) = from.split_at(from.len() - 1);
        let Some(parent) = self.at_mut(parent_path) else {
            return false;
        };
        if index[0] >= parent.children.len() {
            return false;
        }
        // Removing the node shifts later siblings, which may lead to `to`.
        let mut to = to.to_vec();
        if to.len() > parent_path.len()
            && to.starts_with(parent_path)
            && to[parent_path.len()] > index[0]
        {
            to[parent_path.len()] -= 1;
        }
        let node = parent.children.remove(index[0]);
        match self.at_mut(&to) {
            Some(target) => {
                target.children.push(node);
                true
            }
            None => false,
        }
    }

    /// Writes the prefab file: the components of each node one to a line.
    pub fn to_ron(&self, header: &str) -> String {
        let mut out = String::from(header);
        self.write(&mut out, 0);
        out.push('\n');
        out
    }

    fn write(&self, out: &mut String, depth: usize) {
        let pad = "    ".repeat(depth);
        out.push_str("(\n");
        if !self.components.is_empty() {
            out.push_str(&format!("{pad}    components: {{\n"));
            for (name, value) in &self.components {
                out.push_str(&format!("{pad}        {name:?}: {},\n", value.to_ron()));
            }
            out.push_str(&format!("{pad}    }},\n"));
        }
        if !self.children.is_empty() {
            out.push_str(&format!("{pad}    children: [\n"));
            for child in &self.children {
                out.push_str(&format!("{pad}        "));
                child.write(out, depth + 2);
                out.push_str(",\n");
            }
            out.push_str(&format!("{pad}    ],\n"));
        }
        out.push_str(&pad);
        out.push(')');
    }
}

/// What undo restores.
#[derive(Clone)]
pub(crate) enum Snapshot {
    Map(MapObjects),
    Prefab(PrefabNode),
}

/// Something selected.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Selected {
    /// A map object, by id.
    Object(u32),
    /// A prefab node, by path from the root.
    Node(Vec<usize>),
}

/// The undo and redo stacks. Each step holds the document as it was before the change,
/// and what was selected then.
#[derive(Default)]
pub struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The gesture (a drag, a field being typed in) whose start is already recorded, so
    /// its later changes join the same step.
    gesture: Option<u64>,
}

struct Step {
    label: String,
    snapshot: Snapshot,
    selection: Vec<Selected>,
}

/// At most this many steps are kept.
const MAX_STEPS: usize = 500;

impl History {
    /// Records the document as it is before a change called `label`. A change with the
    /// same `gesture` as the one before (and no [`end_gesture`](Self::end_gesture) since)
    /// joins its step.
    pub(crate) fn record(
        &mut self,
        label: &str,
        gesture: Option<u64>,
        doc: &Document,
        selection: &[Selected],
    ) {
        if gesture.is_some() && gesture == self.gesture {
            return;
        }
        self.gesture = gesture;
        self.push(label, doc.snapshot(), selection);
    }

    /// Records `snapshot` as the state before a change called `label` that already
    /// happened (the file reloaded from disk).
    pub(crate) fn push(&mut self, label: &str, snapshot: Snapshot, selection: &[Selected]) {
        self.redo.clear();
        self.undo.push(Step {
            label: label.to_owned(),
            snapshot,
            selection: selection.to_vec(),
        });
        if self.undo.len() > MAX_STEPS {
            self.undo.remove(0);
        }
    }

    /// Ends the current gesture: the next change starts a step of its own.
    pub(crate) fn end_gesture(&mut self) {
        self.gesture = None;
    }

    /// Undoes the latest step; returns its label.
    pub(crate) fn undo(
        &mut self,
        doc: &mut Document,
        selection: &mut Vec<Selected>,
    ) -> Option<String> {
        let step = self.undo.pop()?;
        self.gesture = None;
        self.redo.push(Step {
            label: step.label.clone(),
            snapshot: doc.snapshot(),
            selection: selection.clone(),
        });
        doc.restore(step.snapshot);
        *selection = step.selection;
        Some(step.label)
    }

    /// Redoes the latest undone step; returns its label.
    pub(crate) fn redo(
        &mut self,
        doc: &mut Document,
        selection: &mut Vec<Selected>,
    ) -> Option<String> {
        let step = self.redo.pop()?;
        self.gesture = None;
        self.undo.push(Step {
            label: step.label.clone(),
            snapshot: doc.snapshot(),
            selection: selection.clone(),
        });
        doc.restore(step.snapshot);
        *selection = step.selection;
        Some(step.label)
    }

    /// The label of the step undo would undo.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    /// The label of the step redo would redo.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    /// Forgets every step (a new document is open).
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

/// Where a map object is in the world: its anchor and outline, from the file's data and
/// the map. World units, y up.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// Tiled's anchor point (see [`ObjectData::x`]).
    pub anchor: Vec2,
    /// Counter-clockwise rotation in radians.
    pub rotation: f32,
    /// The centre, which gizmos turn around.
    pub centre: Vec2,
    /// The outline's corners (four for boxes, the points of polygons, none for points).
    pub outline: Vec<Vec2>,
    /// Whether the outline is closed.
    pub closed: bool,
    /// The size in world units, for boxes.
    pub size: Vec2,
}

/// The world placement of a map object (`object` converted for the map, template data
/// included), its layer offset added (in Tiled pixels).
pub fn placed(map: &TiledMap, object: &MapObject) -> Placed {
    use cashgear_engine::tiled::ObjectShape;
    let anchor = map.to_world(object.position);
    let rotation = -object.rotation.to_radians();
    let turn = |p: Vec2| anchor + Vec2::from_angle(rotation).rotate(p);
    let (outline, closed, size, to_centre) = match (&object.shape, object.tile) {
        // Tile objects stand on their bottom-left corner.
        (ObjectShape::Rect(size), Some(_)) => (
            vec![Vec2::ZERO, vec2(size.x, 0.0), *size, vec2(0.0, size.y)],
            true,
            *size,
            *size / 2.0,
        ),
        // Rectangles and ellipses hang from their top-left corner.
        (ObjectShape::Rect(size) | ObjectShape::Ellipse(size), None) => (
            vec![
                Vec2::ZERO,
                vec2(size.x, 0.0),
                vec2(size.x, -size.y),
                vec2(0.0, -size.y),
            ],
            true,
            *size,
            vec2(size.x, -size.y) / 2.0,
        ),
        (ObjectShape::Ellipse(size), Some(_)) => (Vec::new(), false, *size, Vec2::ZERO),
        (ObjectShape::Polygon(points), _) | (ObjectShape::Polyline(points), _) => {
            let points: Vec<Vec2> = points.iter().map(|p| vec2(p.x, -p.y)).collect();
            let centre = points.iter().copied().sum::<Vec2>() / points.len().max(1) as f32;
            (
                points,
                matches!(object.shape, ObjectShape::Polygon(_)),
                Vec2::ZERO,
                centre,
            )
        }
        (ObjectShape::Point, _) => (Vec::new(), false, Vec2::ZERO, Vec2::ZERO),
    };
    Placed {
        anchor,
        rotation,
        centre: turn(to_centre),
        outline: outline.into_iter().map(turn).collect(),
        closed,
        size,
    }
}

/// A map object's own properties and the tile's, as the loader merges them: what the
/// game sees.
pub fn property_names(object: &ObjectData) -> impl Iterator<Item = &str> {
    object.properties.iter().map(|p| p.name.as_str())
}

/// Sets the own property `name` of `object` to `value`, adding it if needed.
pub fn set_property(object: &mut ObjectData, name: &str, value: PropertyData) {
    match object.properties.iter_mut().find(|p| p.name == name) {
        Some(property) => property.value = value,
        None => object.properties.push(Property {
            name: name.to_owned(),
            value,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str) -> PrefabNode {
        PrefabNode {
            components: vec![(name.to_owned(), Value::Unit)],
            children: Vec::new(),
        }
    }

    #[test]
    fn nodes_reparent_without_losing_their_place() {
        let mut root = node("root");
        root.children = vec![node("a"), node("b"), node("c")];
        root.children[2].children.push(node("c1"));
        // `a` into `c`: `c` moves up one when `a` leaves.
        assert!(root.reparent(&[0], &[2]));
        let names = |n: &PrefabNode| {
            n.children
                .iter()
                .map(|c| c.components[0].0.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&root), ["b", "c"]);
        assert_eq!(names(&root.children[1]), ["c1", "a"]);
        // Not into itself or its own children, and not the root.
        assert!(!root.reparent(&[1], &[1, 0]));
        assert!(!root.reparent(&[], &[0]));
        assert_eq!(names(&root), ["b", "c"]);
    }

    #[test]
    fn prefab_text_reads_back() {
        let mut root = node("Sensor");
        root.components.push((
            "Layer".into(),
            Value::NewtypeStruct("Layer", Box::new(Value::Int(3))),
        ));
        root.children.push(node("Interpolated"));
        let text = root.to_ron("// A test.\n");
        assert_eq!(
            text,
            "// A test.\n(\n    components: {\n        \"Sensor\": (),\n        \"Layer\": (3),\n    },\n    children: [\n        (\n            components: {\n                \"Interpolated\": (),\n            },\n        ),\n    ],\n)\n"
        );
    }
}
