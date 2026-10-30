//! What games add to the scripts' API: their own functions, component names for
//! `entity:has(name)`, and their game states for `state.*`.

use crate::value::{FromScriptArgs, IntoScriptValue, ScriptValue};
use bevy_ecs::prelude::*;
use cg_core::App;
use cg_world::{NextState, State, States};
use std::collections::BTreeMap;
use std::sync::Arc;

/// A game function as the runtime calls it.
pub(crate) type ScriptFn =
    Arc<dyn Fn(&mut World, &[ScriptValue]) -> Result<ScriptValue, String> + Send + Sync>;

/// How `entity:has(name)` checks one component.
pub(crate) type HasFn = fn(&World, Entity) -> bool;

/// The names of the game state stack, bottom first.
type StackFn = Arc<dyn Fn(&World) -> Vec<String> + Send + Sync>;

/// Changes the game state.
type ChangeFn = Arc<dyn Fn(&mut World, StateOp, &str) -> Result<(), String> + Send + Sync>;

/// A state change asked for by a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "lua"), allow(dead_code))]
pub(crate) enum StateOp {
    Push,
    Pop,
    Replace,
    Set,
}

/// The game state type scripts can see, with its type erased.
#[derive(Clone)]
#[cfg_attr(not(feature = "lua"), allow(dead_code))]
pub(crate) struct StateApi {
    pub stack: StackFn,
    pub change: ChangeFn,
}

/// The functions, component names and states games added for scripts. The runtime
/// installs them before the next script run; [`version`](Self::version) changes with each
/// addition.
#[derive(Resource, Default, Clone)]
pub struct ScriptApi {
    pub(crate) functions: BTreeMap<(String, String), ScriptFn>,
    pub(crate) components: BTreeMap<String, HasFn>,
    pub(crate) states: Option<StateApi>,
    version: u64,
}

/// Tables of the engine's own API, which game functions cannot be added to.
pub(crate) const RESERVED_TABLES: &[&str] = &[
    "audio",
    "events",
    "input",
    "log",
    "state",
    "time",
    "timer",
    "world",
    "string",
    "table",
    "math",
    "utf8",
    "coroutine",
    "_G",
];

impl ScriptApi {
    /// Bumped by every change.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The game functions, as `table.name`.
    pub fn functions(&self) -> impl Iterator<Item = String> + '_ {
        self.functions.keys().map(|(t, n)| format!("{t}.{n}"))
    }

    /// The component names `entity:has` knows.
    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.components.keys().map(String::as_str)
    }
}

/// Adding to the scripts' API from plugins.
pub trait AppScriptExt {
    /// Adds the function `table.name` for scripts. `f` gets the world and the arguments
    /// (a tuple of [`FromScriptValue`](crate::FromScriptValue) types, or `Vec<ScriptValue>`
    /// for all of them as they are), and returns a value for the script or an error, which
    /// stops the hook that called it like a Lua error does.
    ///
    /// ```
    /// use cg_core::prelude::*;
    /// use cg_script::AppScriptExt;
    ///
    /// #[derive(Resource, Default)]
    /// struct Score(u32);
    ///
    /// let mut app = App::new();
    /// app.init_resource::<Score>()
    ///     .add_script_function("game", "add_score", |world, (points,): (u32,)| {
    ///         world.resource_mut::<Score>().0 += points;
    ///         Ok(world.resource::<Score>().0)
    ///     });
    /// // In Lua: `local total = game.add_score(10)`
    /// ```
    ///
    /// `table` must not be one of the engine's (`world`, `audio`, `input`...); such a
    /// function is skipped with an error in the log.
    fn add_script_function<A, R>(
        &mut self,
        table: &str,
        name: &str,
        f: impl Fn(&mut World, A) -> Result<R, String> + Send + Sync + 'static,
    ) -> &mut Self
    where
        A: FromScriptArgs + 'static,
        R: IntoScriptValue + 'static;

    /// Lets scripts check for `T` with `entity:has(name)`.
    fn add_script_component<T: Component>(&mut self, name: &str) -> &mut Self;

    /// Lets scripts read and change the game states `S` through `state.current()`,
    /// `state.stack()`, `state.push(name)`, `state.pop()`, `state.replace(name)` and
    /// `state.set(name)`. Names are the states' `Debug` names, and only the states listed
    /// in `allowed` can be entered from scripts. One state type at a time: a second call
    /// replaces the first.
    fn add_script_states<S: States>(&mut self, allowed: &[S]) -> &mut Self;
}

impl AppScriptExt for App {
    fn add_script_function<A, R>(
        &mut self,
        table: &str,
        name: &str,
        f: impl Fn(&mut World, A) -> Result<R, String> + Send + Sync + 'static,
    ) -> &mut Self
    where
        A: FromScriptArgs + 'static,
        R: IntoScriptValue + 'static,
    {
        if RESERVED_TABLES.contains(&table) || table.is_empty() || name.is_empty() {
            log::error!(
                "cannot add the script function `{table}.{name}`: `{table}` is the engine's"
            );
            return self;
        }
        let f: ScriptFn = Arc::new(move |world, args| {
            let args = A::from_script_args(args)?;
            f(world, args).map(R::into_script_value)
        });
        let mut api = self.world_mut().get_resource_or_init::<ScriptApi>();
        api.functions.insert((table.to_owned(), name.to_owned()), f);
        api.version += 1;
        self
    }

    fn add_script_component<T: Component>(&mut self, name: &str) -> &mut Self {
        let mut api = self.world_mut().get_resource_or_init::<ScriptApi>();
        api.components.insert(name.to_owned(), |world, entity| {
            world.get::<T>(entity).is_some()
        });
        api.version += 1;
        self
    }

    fn add_script_states<S: States>(&mut self, allowed: &[S]) -> &mut Self {
        let allowed: Vec<(String, S)> = allowed
            .iter()
            .map(|s| (format!("{s:?}"), s.clone()))
            .collect();
        let stack = Arc::new(|world: &World| {
            world
                .get_resource::<State<S>>()
                .map(|state| state.stack().iter().map(|s| format!("{s:?}")).collect())
                .unwrap_or_default()
        });
        let change = Arc::new(move |world: &mut World, op: StateOp, name: &str| {
            let state = match op {
                StateOp::Pop => None,
                _ => Some(
                    allowed
                        .iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, s)| s.clone())
                        .ok_or_else(|| {
                            let names: Vec<&str> =
                                allowed.iter().map(|(n, _)| n.as_str()).collect();
                            format!(
                                "no state `{name}` for scripts (they can enter {})",
                                names.join(", ")
                            )
                        })?,
                ),
            };
            let mut next = world
                .get_resource_mut::<NextState<S>>()
                .ok_or("the game has no states")?;
            match (op, state) {
                (StateOp::Pop, _) => next.pop(),
                (StateOp::Push, Some(s)) => next.push(s),
                (StateOp::Replace, Some(s)) => next.replace(s),
                (StateOp::Set, Some(s)) => next.set(s),
                _ => unreachable!("every op but pop has a state"),
            }
            Ok(())
        });
        let mut api = self.world_mut().get_resource_or_init::<ScriptApi>();
        if api.states.is_some() {
            log::warn!(
                "scripts see the states `{}` now, not the earlier ones",
                core::any::type_name::<S>()
            );
        }
        api.states = Some(StateApi { stack, change });
        api.version += 1;
        self
    }
}
