//! The engine's API for scripts: entity methods and the `world`, `input`, `audio`,
//! `timer`, `events`, `state`, `time` and `log` tables. Every function reaches the world
//! through [`with_world`], so they work only inside hooks.

use super::convert::{self, color, number, vector};
use super::{Current, LuaEntity, Timer, Timers, with_world};
use crate::api::StateOp;
use crate::{ScriptApi, ScriptEvent, ScriptEvents, ScriptMessage};
use bevy_ecs::component::Mutable;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemState;
use cg_anim::{Animator, Ease, SpriteAnimation, Tween, Tweens};
use cg_assets::{AssetServer, Handle};
use cg_audio::{Audio, Bus, MusicSettings, PlaySettings, Sound, Voice};
use cg_core::glam::vec2;
use cg_physics::{
    KinematicBody, KinematicContacts, PhysicsWorld, QueryFilter, SpatialQuery, TileShape,
};
use cg_platform::input::ActionState;
use cg_render::Sprite;
use cg_tiled::TiledObject;
use cg_world::{GlobalTransform2D, Transform2D};
use mlua::prelude::*;
use serde::Deserialize;
use serde::de::IntoDeserializer;
use std::collections::HashMap;

type Res<T> = Result<T, String>;

/// Sounds scripts asked for by path, kept loaded.
#[derive(Resource, Default)]
struct ScriptSounds(HashMap<String, Handle<Sound>>);

/// The physics queries, kept between calls.
#[derive(Resource)]
struct SpatialState(SystemState<SpatialQuery<'static, 'static>>);

/// A voice from `audio.play`.
#[derive(Clone, Copy)]
struct LuaVoice(Voice);

impl LuaUserData for LuaVoice {}

/// A timer from `timer.after` and friends.
struct LuaTimer(u64);

impl LuaUserData for LuaTimer {
    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("cancel", |lua, timer, ()| {
            if let Some(mut timers) = lua.app_data_mut::<Timers>() {
                let before = timers.list.len();
                timers.list.retain(|t| t.id != timer.0);
                if timers.list.len() == before {
                    // Being called right now: not to be put back.
                    timers.cancelled.push(timer.0);
                }
            }
            Ok(())
        });
    }
}

/// Adds the entity methods to `methods` and the global tables.
pub(super) fn install(lua: &Lua, methods: &LuaTable) -> LuaResult<()> {
    entity_methods(lua, methods)?;
    let globals = lua.globals();
    globals.raw_set("world", world_table(lua)?)?;
    globals.raw_set("input", input_table(lua)?)?;
    globals.raw_set("audio", audio_table(lua)?)?;
    globals.raw_set("timer", timer_table(lua)?)?;
    globals.raw_set("events", events_table(lua)?)?;
    globals.raw_set("state", state_table(lua)?)?;
    globals.raw_set("log", log_table(lua)?)?;
    globals.raw_set("time", lua.create_table()?)?;
    globals.raw_set(
        "print",
        lua.create_function(|lua, args: LuaMultiValue| {
            log_line(lua, log::Level::Info, args);
            Ok(())
        })?,
    )?;
    Ok(())
}

/// Updates the `time` table for this run.
pub(super) fn set_time(lua: &Lua, dt: f32, step: u64, elapsed: f64, frame: u64) -> LuaResult<()> {
    let time: LuaTable = lua.globals().raw_get("time")?;
    time.raw_set("dt", dt)?;
    time.raw_set("step", step)?;
    time.raw_set("now", step as f64 * f64::from(dt))?;
    time.raw_set("elapsed", elapsed)?;
    time.raw_set("frame", frame)?;
    Ok(())
}

/// Installs the game's functions as `table.name`.
pub(super) fn install_game_functions(lua: &Lua, api: &ScriptApi) -> LuaResult<()> {
    let globals = lua.globals();
    for ((table, name), f) in &api.functions {
        let target = match globals.raw_get::<LuaValue>(table.as_str())? {
            LuaValue::Table(t) => t,
            LuaValue::Nil => {
                let t = lua.create_table()?;
                globals.raw_set(table.as_str(), &t)?;
                t
            }
            other => {
                log::error!(
                    "cannot add `{table}.{name}`: `{table}` is a {}",
                    other.type_name()
                );
                continue;
            }
        };
        let f = f.clone();
        let full = format!("{table}.{name}");
        let function = lua.create_function(move |lua, args: LuaMultiValue| {
            let args = args
                .into_iter()
                .map(convert::from_lua)
                .collect::<LuaResult<Vec<_>>>()?;
            match with_world(lua, |world| Ok(f(world, &args)))? {
                Ok(value) => convert::to_lua(lua, &value),
                Err(message) => Err(LuaError::runtime(format!("{full}: {message}"))),
            }
        })?;
        target.raw_set(name.as_str(), function)?;
    }
    Ok(())
}

// --- Helpers ----------------------------------------------------------------------------

/// Adds `name` to `table`: a function of the world and its arguments.
fn function<A, R>(
    lua: &Lua,
    table: &LuaTable,
    name: &'static str,
    f: impl Fn(&Lua, &mut World, A) -> Res<R> + 'static,
) -> LuaResult<()>
where
    A: FromLuaMulti,
    R: IntoLuaMulti,
{
    let function = lua.create_function(move |lua, args: A| {
        with_world(lua, |world| Ok(f(lua, world, args)))?
            .map_err(|message| LuaError::runtime(format!("{name}: {message}")))
    })?;
    table.raw_set(name, function)
}

/// Adds the method `name`: a function of the world, the entity (or `self`) and the rest
/// of its arguments.
fn method<A, R>(
    lua: &Lua,
    methods: &LuaTable,
    name: &'static str,
    f: impl Fn(&Lua, &mut World, Entity, A) -> Res<R> + 'static,
) -> LuaResult<()>
where
    A: FromLuaMulti,
    R: IntoLuaMulti,
{
    let function = lua.create_function(move |lua, (entity, args): (LuaEntity, A)| {
        with_world(lua, |world| Ok(f(lua, world, entity.0, args)))?
            .map_err(|message| LuaError::runtime(format!("{name}: {message}")))
    })?;
    methods.raw_set(name, function)
}

fn exists(world: &World, entity: Entity) -> Res<()> {
    world
        .get_entity(entity)
        .map(|_| ())
        .map_err(|_| format!("entity {entity} does not exist"))
}

fn get<'w, T: Component>(world: &'w World, entity: Entity, what: &str) -> Res<&'w T> {
    exists(world, entity)?;
    world
        .get::<T>(entity)
        .ok_or_else(|| format!("entity {entity} has no {what}"))
}

fn get_mut<'w, T: Component<Mutability = Mutable>>(
    world: &'w mut World,
    entity: Entity,
    what: &str,
) -> Res<Mut<'w, T>> {
    exists(world, entity)?;
    world
        .get_mut::<T>(entity)
        .ok_or_else(|| format!("entity {entity} has no {what}"))
}

fn lua_err(err: LuaError) -> String {
    err.to_string()
}

/// A curve by its name (`"QuadOut"`).
fn ease(name: &str) -> Res<Ease> {
    let names: serde::de::value::StrDeserializer<'_, serde::de::value::Error> =
        name.into_deserializer();
    Ease::deserialize(names).map_err(|_| format!("no easing curve called `{name}`"))
}

fn bus(name: &str) -> Res<Bus> {
    let names: serde::de::value::StrDeserializer<'_, serde::de::value::Error> =
        name.into_deserializer();
    Bus::deserialize(names).map_err(|_| format!("no bus called `{name}` (Sfx, Ui or Music)"))
}

/// The entity running the hook now.
fn current(lua: &Lua) -> Current {
    lua.app_data_ref::<Current>()
        .map(|c| c.clone())
        .unwrap_or_default()
}

fn opt<T: FromLua>(table: &Option<LuaTable>, key: &str) -> Res<Option<T>> {
    match table {
        Some(t) => t
            .get::<Option<T>>(key)
            .map_err(|err| format!("option `{key}`: {err}")),
        None => Ok(None),
    }
}

// --- Entity methods ---------------------------------------------------------------------

fn entity_methods(lua: &Lua, m: &LuaTable) -> LuaResult<()> {
    method(lua, m, "id", |_, _, e, ()| Ok(e.to_bits() as i64))?;
    method(lua, m, "is", |_, _, e, other: LuaEntity| Ok(e == other.0))?;
    method(lua, m, "alive", |_, w, e, ()| Ok(w.get_entity(e).is_ok()))?;
    method(lua, m, "despawn", |_, w, e, ()| {
        if w.get_entity(e).is_ok() {
            w.despawn(e);
        }
        Ok(())
    })?;
    method(lua, m, "name", |_, w, e, ()| {
        exists(w, e)?;
        Ok(w.get::<Name>(e).map(|n| n.as_str().to_owned()))
    })?;
    method(lua, m, "has", |_, w, e, name: String| {
        exists(w, e)?;
        let api = w.resource::<ScriptApi>();
        let has = *api.components.get(&name).ok_or_else(|| {
            let known: Vec<&str> = api.components().collect();
            format!(
                "no component `{name}` for scripts (they know {})",
                known.join(", ")
            )
        })?;
        Ok(has(w, e))
    })?;
    method(lua, m, "parent", |_, w, e, ()| {
        exists(w, e)?;
        Ok(w.get::<ChildOf>(e).map(|c| LuaEntity(c.parent())))
    })?;
    method(lua, m, "children", |lua, w, e, ()| {
        exists(w, e)?;
        let children: Vec<LuaEntity> = w
            .get::<Children>(e)
            .map(|c| c.iter().map(LuaEntity).collect())
            .unwrap_or_default();
        lua.create_sequence_from(children).map_err(lua_err)
    })?;
    method(lua, m, "child", |_, w, e, name: String| {
        exists(w, e)?;
        let Some(children) = w.get::<Children>(e) else {
            return Ok(None);
        };
        Ok(children
            .iter()
            .find(|c| w.get::<Name>(*c).is_some_and(|n| n.as_str() == name))
            .map(LuaEntity))
    })?;
    method(lua, m, "insert_prefab", |_, w, e, name: String| {
        exists(w, e)?;
        cg_world::insert_prefab(w, e, &name).map_err(|err| err.to_string())
    })?;
    method(
        lua,
        m,
        "send",
        |lua, w, e, (name, data): (String, LuaValue)| {
            let data = convert::from_lua(data).map_err(lua_err)?;
            w.resource_mut::<ScriptEvents>().push(ScriptEvent {
                target: Some(e),
                name,
                data,
                from: current(lua).entity,
            });
            Ok(())
        },
    )?;

    // The transform.
    method(lua, m, "position", |_, w, e, ()| {
        let t = get::<Transform2D>(w, e, "Transform2D")?;
        Ok((t.translation.x, t.translation.y))
    })?;
    method(lua, m, "set_position", |_, w, e, (x, y): (f32, f32)| {
        let mut t = get_mut::<Transform2D>(w, e, "Transform2D")?;
        if t.translation != vec2(x, y) {
            t.translation = vec2(x, y);
        }
        Ok(())
    })?;
    method(lua, m, "world_position", |_, w, e, ()| {
        let at = get::<GlobalTransform2D>(w, e, "GlobalTransform2D")?.translation();
        Ok((at.x, at.y))
    })?;
    method(lua, m, "rotation", |_, w, e, ()| {
        Ok(get::<Transform2D>(w, e, "Transform2D")?.rotation)
    })?;
    method(lua, m, "set_rotation", |_, w, e, r: f32| {
        let mut t = get_mut::<Transform2D>(w, e, "Transform2D")?;
        if t.rotation != r {
            t.rotation = r;
        }
        Ok(())
    })?;
    method(lua, m, "scale", |_, w, e, ()| {
        let t = get::<Transform2D>(w, e, "Transform2D")?;
        Ok((t.scale.x, t.scale.y))
    })?;
    method(
        lua,
        m,
        "set_scale",
        |_, w, e, (x, y): (f32, Option<f32>)| {
            let scale = vec2(x, y.unwrap_or(x));
            let mut t = get_mut::<Transform2D>(w, e, "Transform2D")?;
            if t.scale != scale {
                t.scale = scale;
            }
            Ok(())
        },
    )?;

    // The kinematic body.
    method(lua, m, "velocity", |_, w, e, ()| {
        let v = get::<KinematicBody>(w, e, "KinematicBody")?.velocity;
        Ok((v.x, v.y))
    })?;
    method(
        lua,
        m,
        "set_velocity",
        |_, w, e, (x, y): (Option<f32>, Option<f32>)| {
            let mut body = get_mut::<KinematicBody>(w, e, "KinematicBody")?;
            let v = vec2(x.unwrap_or(body.velocity.x), y.unwrap_or(body.velocity.y));
            if body.velocity != v {
                body.velocity = v;
            }
            Ok(())
        },
    )?;
    method(lua, m, "contacts", |lua, w, e, ()| {
        let c = get::<KinematicContacts>(w, e, "KinematicBody")?;
        let t = lua.create_table_with_capacity(0, 8).map_err(lua_err)?;
        let set = |key: &str, value: bool| t.raw_set(key, value).map_err(lua_err);
        set("ground", c.on_ground())?;
        set("ceiling", c.on_ceiling())?;
        set("wall_left", c.wall_left.is_some())?;
        set("wall_right", c.wall_right.is_some())?;
        set("hazard", c.hazard)?;
        set("slope", c.on_slope())?;
        set("landed", c.just_landed())?;
        set("left_ground", c.just_left_ground())?;
        Ok(t)
    })?;

    // The sprite and its animation.
    method(lua, m, "flip_x", |_, w, e, ()| {
        Ok(get::<Sprite>(w, e, "Sprite")?.flip_x)
    })?;
    method(lua, m, "set_flip_x", |_, w, e, flip: bool| {
        let mut sprite = get_mut::<Sprite>(w, e, "Sprite")?;
        if sprite.flip_x != flip {
            sprite.flip_x = flip;
        }
        Ok(())
    })?;
    method(lua, m, "color", |_, w, e, ()| {
        let c = get::<Sprite>(w, e, "Sprite")?.color;
        Ok((c.r, c.g, c.b, c.a))
    })?;
    method(lua, m, "set_color", |_, w, e, value: LuaValue| {
        let c = color(&value)?;
        let mut sprite = get_mut::<Sprite>(w, e, "Sprite")?;
        if sprite.color != c {
            sprite.color = c;
        }
        Ok(())
    })?;
    method(lua, m, "play", |_, w, e, clip: String| {
        let mut animation = get_mut::<SpriteAnimation>(w, e, "SpriteAnimation")?;
        if animation.clip() != clip {
            animation.play(&clip);
        }
        Ok(())
    })?;
    method(lua, m, "clip", |_, w, e, ()| {
        Ok(get::<SpriteAnimation>(w, e, "SpriteAnimation")?
            .clip()
            .to_owned())
    })?;
    method(
        lua,
        m,
        "set_param",
        |_, w, e, (name, value): (String, LuaValue)| {
            let value = match value {
                LuaValue::Boolean(b) => f32::from(u8::from(b)),
                other => number(&other)?,
            };
            let mut animator = get_mut::<Animator>(w, e, "Animator")?;
            if animator.get(&name) != value {
                animator.set(&name, value);
            }
            Ok(())
        },
    )?;
    method(lua, m, "trigger", |_, w, e, name: String| {
        get_mut::<Animator>(w, e, "Animator")?.trigger(&name);
        Ok(())
    })?;
    method(
        lua,
        m,
        "tween",
        |_, w, e, args: (String, LuaValue, f32, Option<LuaTable>)| {
            let (tween, add) = tween(args)?;
            exists(w, e)?;
            match w.get_mut::<Tweens>(e) {
                Some(mut tweens) if add => tweens.add(tween),
                Some(mut tweens) => tweens.start(tween),
                None => {
                    let mut tweens = Tweens::default();
                    tweens.start(tween);
                    w.entity_mut(e).insert(tweens);
                }
            }
            Ok(())
        },
    )?;
    method(lua, m, "props", |lua, w, e, ()| {
        exists(w, e)?;
        let props = match w.get::<TiledObject>(e) {
            Some(object) => crate::ScriptValue::Map(
                object
                    .properties
                    .iter()
                    .map(|(k, v)| (k.to_owned(), convert::property(v)))
                    .collect(),
            ),
            None => crate::ScriptValue::Map(Default::default()),
        };
        convert::to_lua(lua, &props).map_err(lua_err)
    })?;

    // Timers owned by a script's `self`.
    for (name, repeat) in [("after", false), ("every", true)] {
        let f = lua.create_function(
            move |lua, (this, seconds, callback): (LuaTable, f32, LuaFunction)| {
                let entity = match this.raw_get::<LuaValue>("entity")? {
                    LuaValue::UserData(ud) => ud.borrow::<LuaEntity>().map(|e| e.0).ok(),
                    _ => None,
                }
                .ok_or_else(|| {
                    LuaError::runtime(format!("{name}: call it on a script's `self`"))
                })?;
                add_timer(lua, Some((entity, this)), seconds, repeat, callback)
            },
        )?;
        m.raw_set(name, f)?;
    }
    Ok(())
}

/// A tween from `(property, to, seconds, options)`, and whether it runs alongside the
/// tweens of the same property.
fn tween(
    (property, to, seconds, opts): (String, LuaValue, f32, Option<LuaTable>),
) -> Res<(Tween, bool)> {
    let from: Option<LuaValue> = opt(&opts, "from")?;
    let mut tween = match property.as_str() {
        "position" | "translation" => {
            let t = Tween::translation(vector(&to)?, seconds);
            match &from {
                Some(v) => t.from(vector(v)?),
                None => t,
            }
        }
        "scale" => {
            let t = Tween::scale(vector(&to)?, seconds);
            match &from {
                Some(v) => t.from(vector(v)?),
                None => t,
            }
        }
        "rotation" => {
            let t = Tween::rotation(number(&to)?, seconds);
            match &from {
                Some(v) => t.from(number(v)?),
                None => t,
            }
        }
        "color" => {
            let t = Tween::color(color(&to)?, seconds);
            match &from {
                Some(v) => t.from(color(v)?),
                None => t,
            }
        }
        "flash" => Tween::flash(color(&to)?, seconds),
        other => {
            return Err(format!(
                "no tween property `{other}` (position, scale, rotation, color or flash)"
            ));
        }
    };
    if let Some(name) = opt::<String>(&opts, "ease")? {
        tween = tween.ease(ease(&name)?);
    }
    if let Some(delay) = opt::<f32>(&opts, "delay")? {
        tween = tween.delay(delay);
    }
    if let Some(times) = opt::<u32>(&opts, "repeats")? {
        tween = tween.repeat(times);
    }
    if opt::<bool>(&opts, "forever")?.unwrap_or(false) {
        tween = tween.forever();
    }
    if opt::<bool>(&opts, "yoyo")?.unwrap_or(false) {
        tween = tween.yoyo();
    }
    if let Some(tag) = opt::<String>(&opts, "tag")? {
        tween = tween.tag(tag);
    }
    Ok((tween, opt::<bool>(&opts, "add")?.unwrap_or(false)))
}

fn add_timer(
    lua: &Lua,
    owner: Option<(Entity, LuaTable)>,
    seconds: f32,
    repeat: bool,
    callback: LuaFunction,
) -> LuaResult<LuaTimer> {
    let mut timers = lua
        .app_data_mut::<Timers>()
        .ok_or_else(|| LuaError::runtime("no timers"))?;
    if !(seconds.is_finite() && seconds >= 0.0) {
        return Err(LuaError::runtime(format!(
            "a timer needs seconds of 0 or more, not {seconds}"
        )));
    }
    let dt = if timers.dt > 0.0 {
        timers.dt
    } else {
        1.0 / 60.0
    };
    // Whole steps, at least one: a timer never fires in the run that made it.
    let steps = ((seconds / dt) - 1e-4).ceil().max(1.0) as u64;
    timers.next_id += 1;
    let id = timers.next_id;
    let due = timers.step + steps;
    timers.list.push(Timer {
        id,
        owner,
        due,
        every: repeat.then_some(steps),
        callback,
    });
    Ok(LuaTimer(id))
}

// --- Global tables ----------------------------------------------------------------------

fn filter(table: Option<LuaTable>) -> Res<QueryFilter> {
    let mut filter = QueryFilter::default();
    let Some(t) = table else {
        return Ok(filter);
    };
    if let Some(mask) = t.get::<Option<u32>>("mask").map_err(lua_err)? {
        filter = filter.with_mask(mask);
    }
    if let Some(entity) = t.get::<Option<LuaEntity>>("exclude").map_err(lua_err)? {
        filter = filter.exclude(entity.0);
    }
    if t.get::<Option<bool>>("sensors").map_err(lua_err)? == Some(false) {
        filter = filter.without_sensors();
    }
    if t.get::<Option<bool>>("tiles").map_err(lua_err)? == Some(false) {
        filter = filter.without_tiles();
    }
    Ok(filter)
}

/// Runs `f` with the physics queries.
fn spatial<R>(world: &mut World, f: impl FnOnce(&SpatialQuery) -> R) -> Res<R> {
    if !world.contains_resource::<PhysicsWorld>() {
        return Err("there is no physics in this app (PhysicsPlugin)".into());
    }
    if !world.contains_resource::<SpatialState>() {
        let state = SystemState::new(world);
        world.insert_resource(SpatialState(state));
    }
    world.resource_scope(|world, mut state: Mut<SpatialState>| {
        let query = state.0.get(world).map_err(|err| err.to_string())?;
        Ok(f(&query))
    })
}

fn world_table(lua: &Lua) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    function(
        lua,
        &t,
        "spawn",
        |_, w, (prefab, x, y): (String, Option<f32>, Option<f32>)| {
            let entity = w.spawn_empty().id();
            if let Err(err) = cg_world::insert_prefab(w, entity, &prefab) {
                w.despawn(entity);
                return Err(err.to_string());
            }
            if let (Some(x), Some(y)) = (x, y) {
                match w.get_mut::<Transform2D>(entity) {
                    Some(mut t) => t.translation = vec2(x, y),
                    None => {
                        w.entity_mut(entity).insert(Transform2D::from_xy(x, y));
                    }
                }
            }
            Ok(LuaEntity(entity))
        },
    )?;
    function(lua, &t, "find", |_, w, name: String| {
        let mut query = w.query::<(Entity, &Name)>();
        Ok(query
            .iter(w)
            .filter(|(_, n)| n.as_str() == name)
            .map(|(e, _)| e)
            .min_by_key(|e| e.index_u32())
            .map(LuaEntity))
    })?;
    function(
        lua,
        &t,
        "raycast",
        |lua, w, (x, y, dx, dy, max, f): (f32, f32, f32, f32, f32, Option<LuaTable>)| {
            let filter = filter(f)?;
            let hit = spatial(w, |q| q.raycast(vec2(x, y), vec2(dx, dy), max, filter))?;
            let Some(hit) = hit else {
                return Ok(None);
            };
            let t = lua.create_table().map_err(lua_err)?;
            let fill = || -> LuaResult<()> {
                t.raw_set("entity", LuaEntity(hit.entity))?;
                t.raw_set("x", hit.point.x)?;
                t.raw_set("y", hit.point.y)?;
                t.raw_set("nx", hit.normal.x)?;
                t.raw_set("ny", hit.normal.y)?;
                t.raw_set("distance", hit.distance)?;
                t.raw_set("tile", hit.cell.is_some())?;
                if let Some(cell) = hit.cell {
                    t.raw_set("cx", cell.x)?;
                    t.raw_set("cy", cell.y)?;
                }
                Ok(())
            };
            fill().map_err(lua_err)?;
            Ok(Some(t))
        },
    )?;
    function(
        lua,
        &t,
        "tile_at",
        |lua, w, (x, y, f): (f32, f32, Option<LuaTable>)| {
            let filter = filter(f)?;
            let Some(hit) = spatial(w, |q| q.tile_at(vec2(x, y), filter))? else {
                return Ok(None);
            };
            let t = lua.create_table().map_err(lua_err)?;
            let fill = || -> LuaResult<()> {
                t.raw_set("entity", LuaEntity(hit.entity))?;
                t.raw_set("cx", hit.cell.x)?;
                t.raw_set("cy", hit.cell.y)?;
                t.raw_set("solid", hit.tile.shape == TileShape::Solid)?;
                t.raw_set("one_way", hit.tile.shape == TileShape::OneWay)?;
                t.raw_set("slope", matches!(hit.tile.shape, TileShape::Slope { .. }))?;
                t.raw_set("hazard", hit.tile.hazard)?;
                Ok(())
            };
            fill().map_err(lua_err)?;
            Ok(Some(t))
        },
    )?;
    function(
        lua,
        &t,
        "overlap_point",
        |lua, w, (x, y, f): (f32, f32, Option<LuaTable>)| {
            let filter = filter(f)?;
            let mut hits = spatial(w, |q| q.overlap_point(vec2(x, y), filter))?;
            hits.sort_by_key(|e| e.index_u32());
            lua.create_sequence_from(hits.into_iter().map(LuaEntity))
                .map_err(lua_err)
        },
    )?;
    Ok(t)
}

fn input_table(lua: &Lua) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    fn actions<R: Default>(world: &World, f: impl FnOnce(&ActionState) -> R) -> R {
        world
            .get_resource::<ActionState>()
            .map(f)
            .unwrap_or_default()
    }
    function(lua, &t, "pressed", |_, w, a: String| {
        Ok(actions(w, |s| s.pressed(&a)))
    })?;
    function(lua, &t, "just_pressed", |_, w, a: String| {
        Ok(actions(w, |s| s.just_pressed(&a)))
    })?;
    function(lua, &t, "just_released", |_, w, a: String| {
        Ok(actions(w, |s| s.just_released(&a)))
    })?;
    function(lua, &t, "value", |_, w, a: String| {
        Ok(actions(w, |s| s.value(&a)))
    })?;
    function(lua, &t, "axis", |_, w, (neg, pos): (String, String)| {
        Ok(actions(w, |s| s.axis(&neg, &pos)))
    })?;
    Ok(t)
}

/// The sound at `path`, loaded once and kept.
fn sound(world: &mut World, path: &str) -> Handle<Sound> {
    if let Some(handle) = world
        .get_resource::<ScriptSounds>()
        .and_then(|s| s.0.get(path))
    {
        return handle.clone();
    }
    let handle: Handle<Sound> = world.resource::<AssetServer>().load(path);
    world
        .get_resource_or_init::<ScriptSounds>()
        .0
        .insert(path.to_owned(), handle.clone());
    handle
}

fn audio_table(lua: &Lua) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    function(
        lua,
        &t,
        "play",
        |_, w, (path, opts): (String, Option<LuaTable>)| {
            let mut settings = PlaySettings::new();
            if let Some(v) = opt::<f32>(&opts, "volume")? {
                settings = settings.with_volume(v);
            }
            if let Some(v) = opt::<f32>(&opts, "speed")? {
                settings = settings.with_speed(v);
            }
            if let Some(v) = opt::<f32>(&opts, "pitch_jitter")? {
                settings = settings.with_pitch_jitter(v);
            }
            if let Some(v) = opt::<bool>(&opts, "looped")? {
                settings.looped = v;
            }
            if let Some(v) = opt::<f32>(&opts, "fade_in")? {
                settings.fade_in = v;
            }
            if let (Some(x), Some(y)) = (opt::<f32>(&opts, "x")?, opt::<f32>(&opts, "y")?) {
                settings = settings.at(vec2(x, y));
            }
            if let Some(e) = opt::<LuaEntity>(&opts, "follow")? {
                settings.follow = Some(e.0);
            }
            if let Some(name) = opt::<String>(&opts, "bus")? {
                settings.bus = bus(&name)?;
            }
            let handle = sound(w, &path);
            let mut audio = w
                .get_resource_mut::<Audio>()
                .ok_or("there is no audio in this app (AudioPlugin)")?;
            Ok(LuaVoice(audio.play_with(&handle, settings)))
        },
    )?;
    function(
        lua,
        &t,
        "stop",
        |_, w, (voice, fade): (LuaAnyUserData, Option<f32>)| {
            let voice = *voice
                .borrow::<LuaVoice>()
                .map_err(|_| "expected a voice from audio.play".to_owned())?;
            if let Some(mut audio) = w.get_resource_mut::<Audio>() {
                audio.stop(voice.0, fade.unwrap_or(0.02));
            }
            Ok(())
        },
    )?;
    function(
        lua,
        &t,
        "music",
        |_, w, (path, opts): (String, Option<LuaTable>)| {
            let mut settings = MusicSettings::default();
            if let Some(v) = opt::<f32>(&opts, "volume")? {
                settings.volume = v;
            }
            if let Some(v) = opt::<f32>(&opts, "crossfade")? {
                settings.crossfade = v;
            }
            if let Some(v) = opt::<bool>(&opts, "looped")? {
                settings.looped = v;
            }
            let handle = sound(w, &path);
            if let Some(mut audio) = w.get_resource_mut::<Audio>() {
                audio.play_music(&handle, settings);
            }
            Ok(())
        },
    )?;
    function(lua, &t, "stop_music", |_, w, fade: Option<f32>| {
        if let Some(mut audio) = w.get_resource_mut::<Audio>() {
            audio.stop_music(fade.unwrap_or(0.5));
        }
        Ok(())
    })?;
    Ok(t)
}

fn timer_table(lua: &Lua) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    for (name, repeat) in [("after", false), ("every", true)] {
        let f = lua.create_function(move |lua, (seconds, callback): (f32, LuaFunction)| {
            add_timer(lua, None, seconds, repeat, callback)
        })?;
        t.raw_set(name, f)?;
    }
    Ok(t)
}

fn events_table(lua: &Lua) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    function(
        lua,
        &t,
        "emit",
        |lua, w, (name, data): (String, LuaValue)| {
            let data = convert::from_lua(data).map_err(lua_err)?;
            w.write_message(ScriptMessage {
                entity: current(lua).entity,
                name,
                data,
            });
            Ok(())
        },
    )?;
    function(
        lua,
        &t,
        "broadcast",
        |lua, w, (name, data): (String, LuaValue)| {
            let data = convert::from_lua(data).map_err(lua_err)?;
            w.resource_mut::<ScriptEvents>().push(ScriptEvent {
                target: None,
                name,
                data,
                from: current(lua).entity,
            });
            Ok(())
        },
    )?;
    Ok(t)
}

fn state_table(lua: &Lua) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    fn states(world: &World) -> Res<crate::api::StateApi> {
        world
            .resource::<ScriptApi>()
            .states
            .clone()
            .ok_or_else(|| "the game has not shown scripts its states (add_script_states)".into())
    }
    function(lua, &t, "current", |_, w, ()| {
        Ok((states(w)?.stack)(w).pop())
    })?;
    function(lua, &t, "stack", |_, w, ()| Ok((states(w)?.stack)(w)))?;
    for (name, op) in [
        ("push", StateOp::Push),
        ("replace", StateOp::Replace),
        ("set", StateOp::Set),
    ] {
        function(lua, &t, name, move |_, w, state: String| {
            (states(w)?.change)(w, op, &state)
        })?;
    }
    function(lua, &t, "pop", |_, w, ()| {
        (states(w)?.change)(w, StateOp::Pop, "")
    })?;
    Ok(t)
}

fn log_line(lua: &Lua, level: log::Level, args: LuaMultiValue) {
    let text: Vec<String> = args
        .iter()
        .map(|v| v.to_string().unwrap_or_else(|_| "?".into()))
        .collect();
    let script = current(lua).script;
    let script = script.as_deref().unwrap_or("script");
    log::log!(target: "cg_script", level, "{script}: {}", text.join("\t"));
}

fn log_table(lua: &Lua) -> LuaResult<LuaTable> {
    let t = lua.create_table()?;
    for (name, level) in [
        ("trace", log::Level::Trace),
        ("debug", log::Level::Debug),
        ("info", log::Level::Info),
        ("warn", log::Level::Warn),
        ("error", log::Level::Error),
    ] {
        let f = lua.create_function(move |lua, args: LuaMultiValue| {
            log_line(lua, level, args);
            Ok(())
        })?;
        t.raw_set(name, f)?;
    }
    Ok(t)
}
