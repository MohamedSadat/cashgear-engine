//! Prefabs: entities described in RON files, built from components registered by name.
//!
//! A prefab lists components by their registered name, plus nested children:
//!
//! ```ron
//! (
//!     components: {
//!         "Transform2D": (scale: (2, 2)),
//!         "Sprite": (texture: "bunny_white"),
//!         "Velocity": ((120, 40)),
//!     },
//!     children: [
//!         (components: {
//!             "Transform2D": (translation: (0, 14)),
//!             "Sprite": (texture: "hat"),
//!         }),
//!     ],
//! )
//! ```
//!
//! Each component is read with serde, so fields the file leaves out take their defaults
//! when the type has `#[serde(default)]`. Components that refer to other things by name,
//! such as a sprite's texture, implement [`PrefabComponent`] to resolve the name when the
//! prefab is spawned.

use bevy_ecs::prelude::*;
use bevy_ecs::system::EntityCommands;
use bevy_ecs::world::EntityWorldMut;
use core::any::TypeId;
use core::fmt;
use serde::de::{DeserializeOwned, DeserializeSeed, Error as _, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeStruct};
use serde::{Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// A component whose prefab form differs from the component itself, typically because it
/// refers to something by name that is only known when the prefab is spawned.
///
/// Plain components that are `Clone + Serialize + Deserialize` do not need it; register
/// them with [`AppWorldExt::register_component`](crate::AppWorldExt::register_component).
///
/// ```
/// use cg_core::prelude::*;
/// use cg_world::PrefabComponent;
///
/// #[derive(Resource, Default)]
/// struct Teams(Vec<String>);
///
/// #[derive(Component)]
/// struct Team(usize);
///
/// impl PrefabComponent for Team {
///     type Data = String;
///
///     fn from_data(name: &String, world: &World) -> Result<Self, String> {
///         let teams = world.resource::<Teams>();
///         teams.0.iter().position(|t| t == name).map(Team)
///             .ok_or_else(|| format!("no team called `{name}`"))
///     }
/// }
/// ```
pub trait PrefabComponent: Component + Sized {
    /// What the prefab file holds for this component.
    type Data: Serialize + DeserializeOwned + Send + Sync + 'static;

    /// Builds the component when a prefab is spawned. An error skips the component and is
    /// reported with the prefab's name.
    fn from_data(data: &Self::Data, world: &World) -> Result<Self, String>;
}

/// One parsed component, ready to be inserted into any number of entities.
trait ComponentData: Send + Sync {
    fn insert(&self, entity: &mut EntityWorldMut) -> Result<(), String>;
    fn as_serialize(&self) -> &dyn erased_serde::Serialize;
}

/// A component that is its own prefab data.
struct Plain<T>(T);

impl<T: Component + Clone + Serialize> ComponentData for Plain<T> {
    fn insert(&self, entity: &mut EntityWorldMut) -> Result<(), String> {
        entity.insert(self.0.clone());
        Ok(())
    }

    fn as_serialize(&self) -> &dyn erased_serde::Serialize {
        &self.0
    }
}

/// A [`PrefabComponent`]'s data, turned into the component at spawn time.
struct Custom<T: PrefabComponent>(T::Data);

impl<T: PrefabComponent> ComponentData for Custom<T> {
    fn insert(&self, entity: &mut EntityWorldMut) -> Result<(), String> {
        let component = T::from_data(&self.0, entity.world())?;
        entity.insert(component);
        Ok(())
    }

    fn as_serialize(&self) -> &dyn erased_serde::Serialize {
        &self.0
    }
}

type DeserializeFn = for<'de> fn(
    &mut dyn erased_serde::Deserializer<'de>,
) -> Result<Box<dyn ComponentData>, erased_serde::Error>;

fn deserialize_plain<T: Component + Clone + Serialize + DeserializeOwned>(
    deserializer: &mut dyn erased_serde::Deserializer<'_>,
) -> Result<Box<dyn ComponentData>, erased_serde::Error> {
    Ok(Box::new(Plain(erased_serde::deserialize::<T>(
        deserializer,
    )?)))
}

fn deserialize_custom<T: PrefabComponent>(
    deserializer: &mut dyn erased_serde::Deserializer<'_>,
) -> Result<Box<dyn ComponentData>, erased_serde::Error> {
    Ok(Box::new(Custom::<T>(erased_serde::deserialize(
        deserializer,
    )?)))
}

struct ComponentType {
    type_id: TypeId,
    type_name: &'static str,
    deserialize: DeserializeFn,
}

/// The components prefabs can use, by name. Engine plugins register their components;
/// games register theirs with [`AppWorldExt`](crate::AppWorldExt).
#[derive(Resource, Default)]
pub struct PrefabRegistry {
    types: BTreeMap<String, ComponentType>,
}

impl PrefabRegistry {
    /// Registers a component that is read from prefabs as it is.
    ///
    /// # Panics
    ///
    /// Panics if `name` is already registered for a different type.
    pub fn register<T: Component + Clone + Serialize + DeserializeOwned>(&mut self, name: &str) {
        self.insert::<T>(name, deserialize_plain::<T>);
    }

    /// Registers a [`PrefabComponent`].
    ///
    /// # Panics
    ///
    /// Panics if `name` is already registered for a different type.
    pub fn register_with<T: PrefabComponent>(&mut self, name: &str) {
        self.insert::<T>(name, deserialize_custom::<T>);
    }

    fn insert<T: 'static>(&mut self, name: &str, deserialize: DeserializeFn) {
        let type_name = core::any::type_name::<T>();
        if let Some(existing) = self.types.get(name) {
            assert!(
                existing.type_id == TypeId::of::<T>(),
                "prefab component `{name}` is already registered for `{}`, not `{type_name}`",
                existing.type_name
            );
        }
        self.types.insert(
            name.to_owned(),
            ComponentType {
                type_id: TypeId::of::<T>(),
                type_name,
                deserialize,
            },
        );
    }

    /// Whether a component is registered under `name`.
    pub fn contains(&self, name: &str) -> bool {
        self.types.contains_key(name)
    }

    /// The registered names, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.types.keys().map(String::as_str)
    }
}

/// An error from reading or spawning a prefab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefabError {
    /// The RON text is malformed, or names an unknown component, or a component's fields
    /// are wrong.
    Parse {
        /// The 1-based line of the problem.
        line: usize,
        /// The 1-based column of the problem.
        column: usize,
        /// What went wrong.
        message: String,
    },
    /// No prefab has this name.
    NotFound(String),
    /// Some components could not be built while spawning; the rest were.
    Spawn(Vec<String>),
}

impl fmt::Display for PrefabError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse {
                line,
                column,
                message,
            } => write!(f, "line {line}, column {column}: {message}"),
            Self::NotFound(name) => write!(f, "no prefab called `{name}`"),
            Self::Spawn(problems) => write!(f, "{}", problems.join("; ")),
        }
    }
}

impl std::error::Error for PrefabError {}

/// An entity template: components and child prefabs. Parse one with
/// [`from_ron`](Self::from_ron), or let [`Prefabs`] do it on first use.
#[derive(Default)]
pub struct Prefab {
    components: Vec<(String, Box<dyn ComponentData>)>,
    children: Vec<Prefab>,
}

impl fmt::Debug for Prefab {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prefab")
            .field("components", &self.component_names().collect::<Vec<_>>())
            .field("children", &self.children)
            .finish()
    }
}

impl Prefab {
    /// Parses a prefab from RON text. Every component must be registered in `registry`.
    pub fn from_ron(source: &str, registry: &PrefabRegistry) -> Result<Self, PrefabError> {
        ron::Options::default()
            .from_str_seed(source, NodeSeed { registry })
            .map_err(|err| PrefabError::Parse {
                line: err.span.start.line,
                column: err.span.start.col,
                message: err.code.to_string(),
            })
    }

    /// Writes the prefab as pretty-printed RON that [`from_ron`](Self::from_ron) reads
    /// back.
    pub fn to_ron(&self) -> String {
        let config = ron::ser::PrettyConfig::new().struct_names(false);
        ron::ser::to_string_pretty(self, config).expect("prefab components serialize to RON")
    }

    /// The names of the prefab's own components, in file order.
    pub fn component_names(&self) -> impl Iterator<Item = &str> {
        self.components.iter().map(|(name, _)| name.as_str())
    }

    /// The prefab's own components with their data, in file order. The data serializes
    /// as the prefab file holds it, so tools can show or convert it: the editor's
    /// inspector turns it into fields.
    pub fn components(&self) -> impl Iterator<Item = (&str, &dyn erased_serde::Serialize)> {
        self.components
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_serialize()))
    }

    /// The child prefabs, in file order.
    pub fn children(&self) -> &[Prefab] {
        &self.children
    }

    /// Spawns the prefab and its children into `world`.
    ///
    /// Components that fail to build are skipped, and reported in the error; the entity
    /// exists either way.
    pub fn spawn(&self, world: &mut World) -> (Entity, Result<(), PrefabError>) {
        let entity = world.spawn_empty().id();
        (entity, self.insert_into(world, entity))
    }

    /// Adds the prefab's components to an existing entity and spawns its children under it.
    pub fn insert_into(&self, world: &mut World, entity: Entity) -> Result<(), PrefabError> {
        let mut problems = Vec::new();
        self.build(world, entity, &mut problems);
        if problems.is_empty() {
            Ok(())
        } else {
            Err(PrefabError::Spawn(problems))
        }
    }

    fn build(&self, world: &mut World, entity: Entity, problems: &mut Vec<String>) {
        let Ok(mut target) = world.get_entity_mut(entity) else {
            problems.push(format!("entity {entity} no longer exists"));
            return;
        };
        for (name, data) in &self.components {
            if let Err(err) = data.insert(&mut target) {
                problems.push(format!("`{name}`: {err}"));
            }
        }
        for child in &self.children {
            let id = world.spawn(ChildOf(entity)).id();
            child.build(world, id, problems);
        }
    }
}

impl Serialize for Prefab {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct Components<'a>(&'a [(String, Box<dyn ComponentData>)]);

        impl Serialize for Components<'_> {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut map = serializer.serialize_map(Some(self.0.len()))?;
                for (name, data) in self.0 {
                    map.serialize_entry(name, data.as_serialize())?;
                }
                map.end()
            }
        }

        let fields =
            usize::from(!self.components.is_empty()) + usize::from(!self.children.is_empty());
        let mut node = serializer.serialize_struct("Prefab", fields)?;
        if self.components.is_empty() {
            node.skip_field("components")?;
        } else {
            node.serialize_field("components", &Components(&self.components))?;
        }
        if self.children.is_empty() {
            node.skip_field("children")?;
        } else {
            node.serialize_field("children", &self.children)?;
        }
        node.end()
    }
}

const FIELDS: &[&str] = &["components", "children"];

/// Reads one prefab node, looking components up in the registry as it goes.
#[derive(Clone, Copy)]
struct NodeSeed<'a> {
    registry: &'a PrefabRegistry,
}

impl<'de> DeserializeSeed<'de> for NodeSeed<'_> {
    type Value = Prefab;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Prefab, D::Error> {
        deserializer.deserialize_struct("Prefab", FIELDS, self)
    }
}

impl<'de> Visitor<'de> for NodeSeed<'_> {
    type Value = Prefab;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a prefab: (components: { ... }, children: [ ... ])")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Prefab, A::Error> {
        let mut prefab = Prefab::default();
        let (mut components, mut children) = (false, false);
        while let Some(field) = map.next_key::<String>()? {
            match field.as_str() {
                "components" if !components => {
                    components = true;
                    prefab.components = map.next_value_seed(ComponentsSeed(self))?;
                }
                "children" if !children => {
                    children = true;
                    prefab.children = map.next_value_seed(ChildrenSeed(self))?;
                }
                "components" | "children" => {
                    return Err(A::Error::custom(format!("`{field}` appears twice")));
                }
                other => return Err(A::Error::unknown_field(other, FIELDS)),
            }
        }
        Ok(prefab)
    }
}

struct ComponentsSeed<'a>(NodeSeed<'a>);

impl<'de> DeserializeSeed<'de> for ComponentsSeed<'_> {
    type Value = Vec<(String, Box<dyn ComponentData>)>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for ComponentsSeed<'_> {
    type Value = Vec<(String, Box<dyn ComponentData>)>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a map of component names to components")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let registry = self.0.registry;
        let mut components: Vec<(String, Box<dyn ComponentData>)> = Vec::new();
        while let Some(name) = map.next_key::<String>()? {
            let Some(ty) = registry.types.get(&name) else {
                let known: Vec<_> = registry.names().collect();
                return Err(A::Error::custom(format!(
                    "unknown component `{name}`; registered: {}",
                    known.join(", ")
                )));
            };
            if components.iter().any(|(existing, _)| *existing == name) {
                return Err(A::Error::custom(format!(
                    "component `{name}` appears twice"
                )));
            }
            let data = map.next_value_seed(ComponentSeed(ty))?;
            components.push((name, data));
        }
        Ok(components)
    }
}

struct ComponentSeed<'a>(&'a ComponentType);

impl<'de> DeserializeSeed<'de> for ComponentSeed<'_> {
    type Value = Box<dyn ComponentData>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        let mut erased = <dyn erased_serde::Deserializer>::erase(deserializer);
        (self.0.deserialize)(&mut erased).map_err(D::Error::custom)
    }
}

struct ChildrenSeed<'a>(NodeSeed<'a>);

impl<'de> DeserializeSeed<'de> for ChildrenSeed<'_> {
    type Value = Vec<Prefab>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for ChildrenSeed<'_> {
    type Value = Vec<Prefab>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of child prefabs")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut children = Vec::new();
        while let Some(child) = seq.next_element_seed(self.0)? {
            children.push(child);
        }
        Ok(children)
    }
}

enum Entry {
    Source(String),
    Parsed(Arc<Prefab>),
    Failed(PrefabError),
}

/// Named prefabs, ready to spawn with
/// [`PrefabCommandsExt::spawn_prefab`]. Prefabs added as RON text are parsed the first time
/// they are spawned, so components registered later in the app's setup still count.
///
/// Phase 4's asset server will load them from files; until then, embed them:
///
/// ```
/// use cg_core::prelude::*;
/// use cg_world::AppWorldExt;
///
/// App::new()
///     .add_prefab("dot", r#"(components: { "Transform2D": (translation: (4, 4)) })"#);
///     // .add_prefab("bunny", include_str!("../assets/bunny.prefab.ron"));
/// ```
#[derive(Resource, Default)]
pub struct Prefabs {
    entries: HashMap<String, Entry>,
}

impl Prefabs {
    /// Adds a prefab as RON text, replacing any prefab with the same name. It is parsed on
    /// first use.
    pub fn add_ron(&mut self, name: impl Into<String>, source: impl Into<String>) {
        self.entries
            .insert(name.into(), Entry::Source(source.into()));
    }

    /// Adds a parsed prefab, replacing any prefab with the same name.
    pub fn insert(&mut self, name: impl Into<String>, prefab: Prefab) {
        self.entries
            .insert(name.into(), Entry::Parsed(Arc::new(prefab)));
    }

    /// Whether a prefab has this name.
    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// The prefabs' names, sorted.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.entries.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    /// The prefab called `name`, parsing it first if needed.
    pub fn get(
        &mut self,
        name: &str,
        registry: &PrefabRegistry,
    ) -> Result<Arc<Prefab>, PrefabError> {
        let entry = self
            .entries
            .get_mut(name)
            .ok_or_else(|| PrefabError::NotFound(name.to_owned()))?;
        if let Entry::Source(source) = entry {
            *entry = match Prefab::from_ron(source, registry) {
                Ok(prefab) => Entry::Parsed(Arc::new(prefab)),
                Err(err) => Entry::Failed(err),
            };
        }
        match entry {
            Entry::Parsed(prefab) => Ok(prefab.clone()),
            Entry::Failed(err) => Err(err.clone()),
            Entry::Source(_) => unreachable!("parsed above"),
        }
    }

    /// Parses every prefab not parsed yet and returns the ones that fail, by name. Handy in
    /// a test that checks a game's prefabs.
    pub fn parse_all(&mut self, registry: &PrefabRegistry) -> Vec<(String, PrefabError)> {
        let mut names: Vec<String> = self.entries.keys().cloned().collect();
        names.sort();
        names
            .into_iter()
            .filter_map(|name| self.get(&name, registry).err().map(|err| (name, err)))
            .collect()
    }
}

/// Adds the prefab called `name` to `entity`, as [`Prefab::insert_into`] does.
pub fn insert_prefab(world: &mut World, entity: Entity, name: &str) -> Result<(), PrefabError> {
    let prefab = world.resource_scope(|world, mut prefabs: Mut<Prefabs>| {
        prefabs.get(name, world.resource::<PrefabRegistry>())
    })?;
    prefab.insert_into(world, entity)
}

/// Spawning prefabs from [`Commands`].
pub trait PrefabCommandsExt {
    /// Spawns the prefab called `name` (from [`Prefabs`]) and returns its root entity's
    /// commands. Components inserted through them afterwards replace the prefab's, so
    /// `.insert(Transform2D::from_xy(10.0, 20.0))` moves the whole prefab. Problems are
    /// logged as errors and leave the entity with the components that did work.
    fn spawn_prefab(&mut self, name: impl Into<String>) -> EntityCommands<'_>;
}

impl PrefabCommandsExt for Commands<'_, '_> {
    fn spawn_prefab(&mut self, name: impl Into<String>) -> EntityCommands<'_> {
        let name = name.into();
        let mut entity = self.spawn_empty();
        entity.queue(move |entity: EntityWorldMut| {
            let id = entity.id();
            if let Err(err) = insert_prefab(entity.into_world_mut(), id, &name) {
                log::error!("prefab `{name}`: {err}");
            }
        });
        entity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppWorldExt, GlobalTransform2D, Transform2D, WorldPlugin};
    use cg_core::App;
    use cg_core::glam::{Vec2, vec2};
    use core::time::Duration;
    use serde::Deserialize;

    #[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    struct Velocity(Vec2);

    #[derive(Component, Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
    #[serde(default)]
    struct Health {
        max: u32,
        regen: bool,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    enum Kind {
        Walker,
        Jumper,
    }

    #[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
    struct Enemy(Kind);

    #[derive(Resource)]
    struct Palette(Vec<&'static str>);

    /// Stored as an index, written in prefabs as a colour name.
    #[derive(Component, Debug, PartialEq)]
    struct Tint(usize);

    impl PrefabComponent for Tint {
        type Data = String;

        fn from_data(name: &String, world: &World) -> Result<Self, String> {
            world
                .resource::<Palette>()
                .0
                .iter()
                .position(|c| c == name)
                .map(Tint)
                .ok_or_else(|| format!("no colour `{name}`"))
        }
    }

    const BUNNY: &str = r#"
        // A bunny with a hat.
        (
            components: {
                "Transform2D": (scale: (2, 2)),
                "Velocity": ((120, 40)),
                "Health": (max: 3),
                "Enemy": (Jumper),
                "Tint": "green",
            },
            children: [
                (components: { "Transform2D": (translation: (0, 14)), "Tint": "red" }),
                (),
            ],
        )
    "#;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(WorldPlugin)
            .insert_resource(Palette(vec!["red", "green"]))
            .register_component::<Velocity>("Velocity")
            .register_component::<Health>("Health")
            .register_component::<Enemy>("Enemy")
            .register_prefab_component::<Tint>("Tint")
            .add_prefab("bunny", BUNNY);
        app
    }

    fn registry(app: &App) -> &PrefabRegistry {
        app.world().resource::<PrefabRegistry>()
    }

    #[test]
    fn spawns_components_and_children_with_defaults() {
        let mut app = app();
        let root = app.world_mut().spawn_empty().id();
        insert_prefab(app.world_mut(), root, "bunny").unwrap();
        let world = app.world();
        assert_eq!(
            *world.get::<Transform2D>(root).unwrap(),
            Transform2D::from_scale(vec2(2.0, 2.0))
        );
        assert_eq!(
            world.get::<Velocity>(root),
            Some(&Velocity(vec2(120.0, 40.0)))
        );
        assert_eq!(
            world.get::<Health>(root),
            Some(&Health {
                max: 3,
                regen: false
            })
        );
        assert_eq!(world.get::<Enemy>(root), Some(&Enemy(Kind::Jumper)));
        assert_eq!(world.get::<Tint>(root), Some(&Tint(1)));

        let children = world.get::<Children>(root).unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(world.get::<Tint>(children[0]), Some(&Tint(0)));
        assert_eq!(
            world.get::<Transform2D>(children[0]).unwrap().translation,
            vec2(0.0, 14.0)
        );
        // An empty child prefab is still an entity under the root.
        assert_eq!(world.get::<ChildOf>(children[1]), Some(&ChildOf(root)));
    }

    #[test]
    fn commands_spawn_prefabs_and_overrides_win() {
        let mut app = app();
        app.add_systems(cg_core::Startup, |mut commands: Commands| {
            commands
                .spawn_prefab("bunny")
                .insert(Transform2D::from_xy(5.0, 6.0));
        });
        app.update(Duration::ZERO);
        let mut query = app
            .world_mut()
            .query_filtered::<&GlobalTransform2D, With<Velocity>>();
        let globals: Vec<_> = query.iter(app.world()).map(|g| g.translation()).collect();
        assert_eq!(globals, [vec2(5.0, 6.0)]);
    }

    #[test]
    fn round_trips_through_ron() {
        let app = app();
        let prefab = Prefab::from_ron(BUNNY, registry(&app)).unwrap();
        let text = prefab.to_ron();
        let again = Prefab::from_ron(&text, registry(&app)).unwrap();
        assert_eq!(again.to_ron(), text);
        assert_eq!(
            again.component_names().collect::<Vec<_>>(),
            ["Transform2D", "Velocity", "Health", "Enemy", "Tint"]
        );
        assert_eq!(again.children().len(), 2);
        let tint = again
            .components()
            .find(|(name, _)| *name == "Tint")
            .map(|(_, data)| ron::to_string(data).unwrap());
        assert_eq!(tint.as_deref(), Some("\"green\""));
    }

    #[test]
    fn unknown_components_are_parse_errors_with_positions() {
        let app = app();
        let err = Prefab::from_ron(
            "(\n  components: {\n    \"Velocty\": ((1, 2)),\n  },\n)",
            registry(&app),
        )
        .unwrap_err();
        let PrefabError::Parse { line, message, .. } = &err else {
            panic!("expected a parse error, got {err:?}");
        };
        assert_eq!(*line, 3);
        assert!(message.contains("unknown component `Velocty`"), "{message}");
        assert!(message.contains("Velocity"), "{message}");
    }

    #[test]
    fn bad_fields_and_duplicates_are_errors() {
        let app = app();
        for source in [
            r#"(components: { "Health": (max: "three") })"#,
            r#"(components: { "Velocity": ((1, 2)), "Velocity": ((3, 4)) })"#,
            r#"(colour: "red")"#,
        ] {
            assert!(
                Prefab::from_ron(source, registry(&app)).is_err(),
                "{source} should not parse"
            );
        }
    }

    #[test]
    fn spawn_problems_skip_the_component_only() {
        let mut app = app();
        app.world_mut().resource_mut::<Prefabs>().add_ron(
            "odd",
            r#"(components: { "Tint": "purple", "Velocity": ((1, 1)) })"#,
        );
        let entity = app.world_mut().spawn_empty().id();
        let err = insert_prefab(app.world_mut(), entity, "odd").unwrap_err();
        assert_eq!(
            err,
            PrefabError::Spawn(vec!["`Tint`: no colour `purple`".into()])
        );
        assert!(app.world().get::<Velocity>(entity).is_some());
        assert!(app.world().get::<Tint>(entity).is_none());
    }

    #[test]
    fn parse_all_reports_broken_prefabs_by_name() {
        let mut app = app();
        app.add_prefab("broken", "(components: {");
        let world = app.world_mut();
        let failures = world.resource_scope(|world, mut prefabs: Mut<Prefabs>| {
            prefabs.parse_all(world.resource::<PrefabRegistry>())
        });
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, "broken");
        assert_eq!(
            insert_prefab(world, Entity::PLACEHOLDER, "missing"),
            Err(PrefabError::NotFound("missing".into()))
        );
    }

    #[test]
    #[should_panic(expected = "already registered")]
    fn one_name_cannot_mean_two_types() {
        let mut registry = PrefabRegistry::default();
        registry.register::<Velocity>("Velocity");
        registry.register::<Health>("Velocity");
    }
}
