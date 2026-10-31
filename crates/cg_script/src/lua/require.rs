//! `require`: shared code in other script files. `require("enemies.common")` runs
//! `enemies/common.lua` from the asset folder once and returns what it returned (`true` for
//! nothing), the same value to every file that asks. A name with a `/` or ending in `.lua`
//! is a path as it is: `require("scripts/lib.lua")`.
//!
//! Required files are assets like any other: they load through the asset server, and a
//! file that requires one is compiled again when it changes (hot reload), with `on_reload`
//! for its entities. A file whose requires are still loading compiles once they are in.

use super::with_world;
use crate::LuaScript;
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, AssetServer, Assets, Handle, LoadState};
use mlua::prelude::*;
use std::collections::HashMap;

use super::ENVIRONMENT;

/// A required file at one version, and what it returned.
struct Required {
    version: u64,
    value: LuaValue,
    /// The files it required in turn, with their versions.
    deps: Vec<(AssetId<LuaScript>, u64)>,
}

/// What `require` keeps between calls: the files' results, handles that keep them loaded,
/// and the bookkeeping of the compile running now.
#[derive(Default)]
pub(crate) struct RequireState {
    results: HashMap<AssetId<LuaScript>, Required>,
    handles: HashMap<String, Handle<LuaScript>>,
    /// The files being run now, innermost last, to catch circular requires.
    running: Vec<AssetId<LuaScript>>,
    /// For each compile under way, innermost last: the files it required.
    collecting: Vec<Vec<(AssetId<LuaScript>, u64)>>,
    /// A required file was not loaded yet during the latest compile.
    pub pending: bool,
}

/// The world lent to `require` while a file's top level runs (the engine's other
/// functions work only in hooks).
pub(crate) struct CompileWorld(pub Option<LuaAnyUserData>);

/// The asset path of a module name.
pub(crate) fn module_path(name: &str) -> String {
    if name.contains('/') || name.ends_with(".lua") {
        if name.ends_with(".lua") {
            name.to_owned()
        } else {
            format!("{name}.lua")
        }
    } else {
        format!("{}.lua", name.replace('.', "/"))
    }
}

/// Starts collecting the requires of a compile.
pub(crate) fn begin_compile(lua: &Lua) {
    if let Some(mut state) = lua.app_data_mut::<RequireState>() {
        state.collecting.push(Vec::new());
        state.pending = false;
    }
}

/// Ends a compile, returning the files it required (directly or not).
pub(crate) fn end_compile(lua: &Lua) -> Vec<(AssetId<LuaScript>, u64)> {
    lua.app_data_mut::<RequireState>()
        .and_then(|mut state| state.collecting.pop())
        .unwrap_or_default()
}

/// Whether a file a compile needed is still loading.
pub(crate) fn pending(lua: &Lua) -> bool {
    lua.app_data_ref::<RequireState>()
        .is_some_and(|state| state.pending)
}

fn note_dependency(lua: &Lua, deps: &[(AssetId<LuaScript>, u64)]) {
    if let Some(mut state) = lua.app_data_mut::<RequireState>()
        && let Some(top) = state.collecting.last_mut()
    {
        for dep in deps {
            if !top.contains(dep) {
                top.push(*dep);
            }
        }
    }
}

/// Runs `f` with the world: the compile's, or the run's inside hooks.
fn with_any_world<R>(lua: &Lua, f: impl FnOnce(&mut World) -> LuaResult<R>) -> LuaResult<R> {
    let compiling = lua.app_data_ref::<CompileWorld>().and_then(|w| w.0.clone());
    match compiling {
        Some(world) => world.borrow_mut_scoped::<World, _>(f)?,
        None => with_world(lua, f),
    }
}

/// `require(name)`.
pub(crate) fn require(lua: &Lua, name: String) -> LuaResult<LuaValue> {
    let path = module_path(&name);
    let in_compile = lua
        .app_data_ref::<CompileWorld>()
        .is_some_and(|w| w.0.is_some());
    // Find the file, starting its load if this is the first ask.
    let found = with_any_world(lua, |world| {
        let server = world.resource::<AssetServer>();
        let handle = server
            .get_handle::<LuaScript>(&path)
            .unwrap_or_else(|| server.load(path.clone()));
        let id = handle.id();
        let state = server.load_state(id);
        let error = server.load_error(id).map(|e| e.to_string());
        let scripts = world.resource::<Assets<LuaScript>>();
        let source = scripts
            .get(id)
            .zip(scripts.version(id))
            .map(|(s, v)| (s.source().to_owned(), v));
        Ok((handle, state, error, source))
    })?;
    let (handle, state, error, source) = found;
    let id = handle.id();
    if let Some(mut require) = lua.app_data_mut::<RequireState>() {
        require.handles.entry(path.clone()).or_insert(handle);
    }
    let Some((source, version)) = source else {
        if state == LoadState::Failed {
            let why = error.unwrap_or_else(|| "it failed to load".to_owned());
            return Err(LuaError::runtime(format!(
                "require: cannot load {path}: {why}"
            )));
        }
        if in_compile {
            if let Some(mut require) = lua.app_data_mut::<RequireState>() {
                require.pending = true;
            }
            return Err(LuaError::runtime(format!(
                "require: {path} is still loading"
            )));
        }
        return Err(LuaError::runtime(format!(
            "require: {path} is not loaded yet; require files at the top of a script"
        )));
    };

    // Already run at this version: the same value.
    let cached = lua.app_data_ref::<RequireState>().and_then(|state| {
        state
            .results
            .get(&id)
            .filter(|r| r.version == version)
            .map(|r| (r.value.clone(), r.deps.clone()))
    });
    if let Some((value, mut deps)) = cached {
        deps.push((id, version));
        note_dependency(lua, &deps);
        return Ok(value);
    }

    let circular = lua
        .app_data_ref::<RequireState>()
        .is_some_and(|state| state.running.contains(&id));
    if circular {
        return Err(LuaError::runtime(format!(
            "require: {path} requires itself (directly or through other files)"
        )));
    }
    if let Some(mut state) = lua.app_data_mut::<RequireState>() {
        state.running.push(id);
        state.collecting.push(Vec::new());
    }
    let env = lua.create_table()?;
    env.set_metatable(Some(lua.named_registry_value::<LuaTable>(ENVIRONMENT)?))?;
    let result = lua
        .load(source)
        .set_name(format!("@{path}"))
        .set_environment(env)
        .eval::<LuaValue>();
    let deps = lua
        .app_data_mut::<RequireState>()
        .map(|mut state| {
            state.running.pop();
            state.collecting.pop().unwrap_or_default()
        })
        .unwrap_or_default();
    let value = match result? {
        LuaValue::Nil => LuaValue::Boolean(true),
        value => value,
    };
    if let Some(mut state) = lua.app_data_mut::<RequireState>() {
        state.results.insert(
            id,
            Required {
                version,
                value: value.clone(),
                deps: deps.clone(),
            },
        );
    }
    let mut all = deps;
    all.push((id, version));
    note_dependency(lua, &all);
    Ok(value)
}
