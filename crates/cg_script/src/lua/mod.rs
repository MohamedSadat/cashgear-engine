//! The Lua runtime (feature `lua`): one Lua state for the app, the compiled script files,
//! one `self` table per scripted entity, and the run that calls their hooks each fixed
//! step.
//!
//! During a run the world is lent to Lua through a scoped userdata in the registry, so the
//! engine's functions can reach it without `unsafe`; outside a run (a file's top level,
//! say) they report that they only work in hooks.

mod bindings;
mod convert;

use crate::script::{ScriptError, ScriptErrors, ScriptInfo, ScriptStatus};
use crate::{LuaScript, Script, ScriptEvent, ScriptEvents, ScriptPlugin, ScriptSystems};
use bevy_ecs::message::{MessageCursor, Messages};
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, AssetServer, Assets};
use cg_core::prelude::*;
use cg_physics::{CollisionEnded, CollisionStarted};
use mlua::prelude::*;
use mlua::{HookTriggers, VmState};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

/// The registry key of the world lent to Lua during a run.
const WORLD: &str = "cg.world";
/// The registry key of the entity methods table.
const METHODS: &str = "cg.methods";
/// The registry key of the metatable of `self` tables.
const INSTANCE: &str = "cg.instance";
/// The registry key of the metatable of each file's globals.
const ENVIRONMENT: &str = "cg.env";
/// Instructions between two checks of the budget.
const BUDGET_STEP: u32 = 1000;
/// Rounds of events delivered per run; events sent in the last round wait for the next.
const EVENT_ROUNDS: usize = 8;

pub(crate) fn build(app: &mut App, plugin: &ScriptPlugin) {
    match LuaRuntime::new(plugin) {
        Ok(runtime) => {
            app.world_mut().insert_non_send(runtime);
            app.add_systems(FixedUpdate, run.in_set(ScriptSystems::Run));
        }
        Err(err) => log::error!("cannot start Lua, scripts will not run: {err}"),
    }
}

/// An entity in Lua.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LuaEntity(pub Entity);

impl LuaUserData for LuaEntity {
    fn add_fields<F: LuaUserDataFields<Self>>(fields: &mut F) {
        // Methods come from the shared table, which `self` tables use too.
        fields.add_meta_field_with("__index", |lua| {
            lua.named_registry_value::<LuaTable>(METHODS)
        });
    }

    fn add_methods<M: LuaUserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(LuaMetaMethod::Eq, |_, a, b: LuaEntity| Ok(a.0 == b.0));
        methods.add_meta_method(LuaMetaMethod::ToString, |_, a, ()| {
            Ok(format!("entity {}", a.0))
        });
    }
}

/// An entity, or a script's `self` (its entity).
impl FromLua for LuaEntity {
    fn from_lua(value: LuaValue, _: &Lua) -> LuaResult<Self> {
        let ud = match value {
            LuaValue::UserData(ud) => ud,
            LuaValue::Table(ref t) => match t.raw_get::<LuaValue>("entity")? {
                LuaValue::UserData(ud) => ud,
                _ => return Err(LuaError::runtime("expected an entity, got a table")),
            },
            other => {
                return Err(LuaError::runtime(format!(
                    "expected an entity, got {}",
                    other.type_name()
                )));
            }
        };
        let entity = *ud
            .borrow::<LuaEntity>()
            .map_err(|_| LuaError::runtime("expected an entity, got other userdata"))?;
        Ok(entity)
    }
}

/// Runs `f` with the world lent to Lua for this run.
pub(crate) fn with_world<R>(lua: &Lua, f: impl FnOnce(&mut World) -> LuaResult<R>) -> LuaResult<R> {
    match lua.named_registry_value::<LuaValue>(WORLD)? {
        LuaValue::UserData(world) => world.borrow_mut_scoped::<World, _>(f)?,
        _ => Err(LuaError::runtime(
            "the engine's functions work only inside hooks, not at a script's top level",
        )),
    }
}

/// What is running now, for logs, `events.emit` and `send`.
#[derive(Default, Clone)]
pub(crate) struct Current {
    pub entity: Option<Entity>,
    pub script: Option<Rc<str>>,
}

/// The instructions left for the hook running now, in thousands.
struct Budget {
    left: u32,
    max: u32,
}

/// A timer from `timer.after`, `timer.every`, `self:after` or `self:every`.
pub(crate) struct Timer {
    pub id: u64,
    /// The entity whose `self` it gets, and that `self`.
    pub owner: Option<(Entity, LuaTable)>,
    /// The run it is due in.
    pub due: u64,
    /// Runs between calls, for repeating timers.
    pub every: Option<u64>,
    pub callback: LuaFunction,
}

#[derive(Default)]
pub(crate) struct Timers {
    pub next_id: u64,
    pub list: Vec<Timer>,
    /// Timers cancelled while they were being called.
    pub cancelled: Vec<u64>,
    /// The current run, which new timers count from.
    pub step: u64,
    pub dt: f32,
}

/// A compiled script file.
struct Module {
    path: Rc<str>,
    /// The asset version compiled (or tried).
    version: u64,
    /// Its hooks, from the last version that compiled.
    hooks: Option<LuaTable>,
    error: Option<String>,
    loads: u32,
}

/// One scripted entity.
struct Instance {
    entity: Entity,
    script: AssetId<LuaScript>,
    this: LuaTable,
    started: bool,
    reloaded: bool,
    /// Stopped by an error until its file changes.
    failed: bool,
}

/// The Lua state and everything compiled in it. A non-send resource: Lua is not `Send`.
pub(crate) struct LuaRuntime {
    lua: Lua,
    modules: HashMap<AssetId<LuaScript>, Module>,
    instances: Vec<Instance>,
    index: HashMap<Entity, usize>,
    scripted: Option<QueryState<(Entity, &'static Script)>>,
    api_version: Option<u64>,
    started: MessageCursor<CollisionStarted>,
    ended: MessageCursor<CollisionEnded>,
    step: u64,
    calls: u32,
    changed: bool,
}

impl LuaRuntime {
    fn new(plugin: &ScriptPlugin) -> LuaResult<Self> {
        use mlua::StdLib;
        let libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE;
        let lua = Lua::new_with(libs, LuaOptions::default())?;
        lua.set_memory_limit(plugin.memory_limit)?;
        let globals = lua.globals();
        for unsafe_global in ["dofile", "loadfile", "load", "collectgarbage"] {
            globals.raw_set(unsafe_global, LuaNil)?;
        }
        let string: LuaTable = globals.get("string")?;
        string.raw_set("dump", LuaNil)?;
        // A fixed seed, so a script's run repeats.
        lua.load("math.randomseed(20261030)").exec()?;
        lua.set_app_data(Current::default());
        lua.set_app_data(Timers::default());
        let max = (plugin.max_instructions / BUDGET_STEP).max(1);
        lua.set_app_data(Budget { left: max, max });
        lua.set_hook(
            HookTriggers {
                every_nth_instruction: Some(BUDGET_STEP),
                ..Default::default()
            },
            |lua, debug| {
                let mut budget = lua
                    .app_data_mut::<Budget>()
                    .ok_or_else(|| LuaError::runtime("no budget"))?;
                if budget.left == 0 {
                    let source = debug.source();
                    let at = source.short_src.as_deref().unwrap_or("?").to_owned();
                    let line = debug
                        .current_line()
                        .map_or(String::new(), |l| format!(":{l}"));
                    let max = u64::from(budget.max) * u64::from(BUDGET_STEP);
                    return Err(LuaError::runtime(format!(
                        "{at}{line}: ran more than {max} instructions (an endless loop?)"
                    )));
                }
                budget.left -= 1;
                Ok(VmState::Continue)
            },
        )?;
        let instance = lua.create_table()?;
        let methods = lua.create_table()?;
        instance.raw_set("__index", &methods)?;
        lua.set_named_registry_value(METHODS, &methods)?;
        lua.set_named_registry_value(INSTANCE, instance)?;
        let env = lua.create_table()?;
        env.raw_set("__index", &globals)?;
        lua.set_named_registry_value(ENVIRONMENT, env)?;
        bindings::install(&lua, &methods)?;
        Ok(Self {
            lua,
            modules: HashMap::new(),
            instances: Vec::new(),
            index: HashMap::new(),
            scripted: None,
            api_version: None,
            started: MessageCursor::default(),
            ended: MessageCursor::default(),
            step: 0,
            calls: 0,
            changed: true,
        })
    }

    /// Before the hooks: the game's functions, the files and the instances.
    fn prepare(&mut self, world: &mut World) {
        let api = world.resource::<crate::ScriptApi>();
        if self.api_version != Some(api.version()) {
            self.api_version = Some(api.version());
            let api = api.clone();
            if let Err(err) = bindings::install_game_functions(&self.lua, &api) {
                log::error!("cannot add the game's script functions: {err}");
            }
        }
        // Which entities run which file.
        let query = self.scripted.get_or_insert_with(|| world.query());
        let scripted: Vec<(Entity, AssetId<LuaScript>)> =
            query.iter(world).map(|(e, s)| (e, s.script.id())).collect();
        // Compile new and changed files.
        let mut ids: Vec<AssetId<LuaScript>> = scripted.iter().map(|(_, id)| *id).collect();
        ids.sort_unstable();
        ids.dedup();
        for id in ids {
            self.sync_module(world, id);
        }
        // Drop instances whose entity is gone or runs another file now.
        let current: HashMap<Entity, AssetId<LuaScript>> = scripted.iter().copied().collect();
        let before = self.instances.len();
        let mut gone = Vec::new();
        self.instances.retain(|instance| {
            let keep = current.get(&instance.entity) == Some(&instance.script);
            if !keep {
                gone.push(instance.entity);
            }
            keep
        });
        if !gone.is_empty()
            && let Some(mut timers) = self.lua.app_data_mut::<Timers>()
        {
            timers
                .list
                .retain(|t| t.owner.as_ref().is_none_or(|(e, _)| !gone.contains(e)));
        }
        // Start instances for new entities whose file has compiled, in entity order.
        let known: HashSet<Entity> = self.instances.iter().map(|i| i.entity).collect();
        let mut fresh: Vec<(Entity, AssetId<LuaScript>)> = scripted
            .into_iter()
            .filter(|(e, id)| {
                !known.contains(e) && self.modules.get(id).is_some_and(|m| m.hooks.is_some())
            })
            .collect();
        fresh.sort_by_key(|(e, _)| e.index_u32());
        for (entity, script) in fresh {
            match self.new_instance(entity) {
                Ok(this) => self.instances.push(Instance {
                    entity,
                    script,
                    this,
                    started: false,
                    reloaded: false,
                    failed: false,
                }),
                Err(err) => log::error!("cannot make the script table of {entity}: {err}"),
            }
        }
        if self.instances.len() != before || !gone.is_empty() {
            self.changed = true;
        }
        self.index = self
            .instances
            .iter()
            .enumerate()
            .map(|(i, instance)| (instance.entity, i))
            .collect();
    }

    fn new_instance(&self, entity: Entity) -> LuaResult<LuaTable> {
        let this = self.lua.create_table()?;
        this.raw_set("entity", LuaEntity(entity))?;
        this.set_metatable(Some(self.lua.named_registry_value::<LuaTable>(INSTANCE)?))?;
        Ok(this)
    }

    /// Compiles the file if it is new or changed since it was last compiled.
    fn sync_module(&mut self, world: &mut World, id: AssetId<LuaScript>) {
        let scripts = world.resource::<Assets<LuaScript>>();
        let (Some(script), Some(version)) = (scripts.get(id), scripts.version(id)) else {
            return; // Still loading (or failed to load, which the asset server reports).
        };
        if self.modules.get(&id).is_some_and(|m| m.version == version) {
            return;
        }
        let path: Rc<str> = world
            .resource::<AssetServer>()
            .path(id)
            .map_or_else(|| format!("script {id:?}").into(), Into::into);
        let source = script.source().to_owned();
        self.changed = true;
        let result = self.compile(&path, &source);
        let module = self.modules.entry(id).or_insert_with(|| Module {
            path: path.clone(),
            version,
            hooks: None,
            error: None,
            loads: 0,
        });
        module.version = version;
        match result {
            Ok(hooks) => {
                let reload = module.hooks.is_some();
                module.hooks = Some(hooks);
                module.error = None;
                module.loads += 1;
                if reload {
                    log::info!("reloaded {path}");
                    for instance in self.instances.iter_mut().filter(|i| i.script == id) {
                        instance.reloaded = instance.started;
                        instance.failed = false;
                    }
                }
            }
            Err(err) => {
                let message = describe(&err);
                if module.hooks.is_some() {
                    log::error!("{message}\n(keeping the version of {path} loaded before)");
                } else {
                    log::error!("{message}");
                }
                module.error = Some(message.clone());
                record(world, &path, None, "load", message);
            }
        }
    }

    fn compile(&self, path: &str, source: &str) -> LuaResult<LuaTable> {
        let env = self.lua.create_table()?;
        env.set_metatable(Some(
            self.lua.named_registry_value::<LuaTable>(ENVIRONMENT)?,
        ))?;
        self.reset_budget();
        let value: LuaValue = self
            .lua
            .load(source)
            .set_name(format!("@{path}"))
            .set_environment(env)
            .eval()?;
        match value {
            LuaValue::Table(hooks) => Ok(hooks),
            other => Err(LuaError::runtime(format!(
                "{path}: the file must return a table of hooks, not {}",
                other.type_name()
            ))),
        }
    }

    fn reset_budget(&self) {
        if let Some(mut budget) = self.lua.app_data_mut::<Budget>() {
            budget.left = budget.max;
        }
    }

    /// The hooks, with the world lent to Lua.
    fn run_hooks(&mut self, world: &LuaAnyUserData) -> LuaResult<()> {
        let (dt, elapsed, frame) = world.borrow_mut_scoped::<World, _>(|world| {
            let time = world.resource::<Time>();
            (
                time.fixed_delta_secs(),
                time.elapsed_secs(),
                time.frame_count(),
            )
        })?;
        self.step += 1;
        self.calls = 0;
        bindings::set_time(&self.lua, dt, self.step, elapsed, frame)?;
        if let Some(mut timers) = self.lua.app_data_mut::<Timers>() {
            timers.step = self.step;
            timers.dt = dt;
        }
        for i in 0..self.instances.len() {
            if !self.instances[i].started {
                self.instances[i].started = true;
                self.call(world, i, "on_start", ())?;
            }
            if self.instances[i].reloaded {
                self.instances[i].reloaded = false;
                self.call(world, i, "on_reload", ())?;
            }
        }
        for _ in 0..EVENT_ROUNDS {
            let events = world.borrow_mut_scoped::<World, _>(|world| {
                world.resource_mut::<ScriptEvents>().take()
            })?;
            if events.is_empty() {
                break;
            }
            for event in events {
                self.deliver(world, event)?;
            }
        }
        let (started, ended) = world.borrow_mut_scoped::<World, _>(|world| {
            let started: Vec<(Entity, Entity)> = world
                .get_resource::<Messages<CollisionStarted>>()
                .map(|m| self.started.read(m).map(|c| (c.0, c.1)).collect())
                .unwrap_or_default();
            let ended: Vec<(Entity, Entity)> = world
                .get_resource::<Messages<CollisionEnded>>()
                .map(|m| self.ended.read(m).map(|c| (c.0, c.1)).collect())
                .unwrap_or_default();
            (started, ended)
        })?;
        for (hook, pairs) in [("on_collision", started), ("on_collision_end", ended)] {
            for (a, b) in pairs {
                for (me, other) in [(a, b), (b, a)] {
                    if let Some(&i) = self.index.get(&me) {
                        self.call(world, i, hook, LuaEntity(other))?;
                    }
                }
            }
        }
        self.fire_timers(world)?;
        for i in 0..self.instances.len() {
            self.call(world, i, "on_update", dt)?;
        }
        Ok(())
    }

    fn deliver(&mut self, world: &LuaAnyUserData, event: ScriptEvent) -> LuaResult<()> {
        let targets: Vec<usize> = match event.target {
            Some(entity) => self.index.get(&entity).copied().into_iter().collect(),
            None => (0..self.instances.len()).collect(),
        };
        for i in targets {
            let data = convert::to_lua(&self.lua, &event.data)?;
            let from = event.from.map(LuaEntity);
            self.call(world, i, "on_event", (event.name.as_str(), data, from))?;
        }
        Ok(())
    }

    fn fire_timers(&mut self, world: &LuaAnyUserData) -> LuaResult<()> {
        let due: Vec<Timer> = {
            let Some(mut timers) = self.lua.app_data_mut::<Timers>() else {
                return Ok(());
            };
            let step = self.step;
            let (mut due, keep): (Vec<Timer>, Vec<Timer>) =
                timers.list.drain(..).partition(|t| t.due <= step);
            timers.list = keep;
            due.sort_by_key(|t| (t.due, t.id));
            due
        };
        for mut timer in due {
            let owner = match &timer.owner {
                Some((entity, _)) => match self.index.get(entity) {
                    Some(&i) if !self.instances[i].failed => Some(i),
                    _ => continue,
                },
                None => None,
            };
            let this = timer.owner.as_ref().map(|(_, this)| this.clone());
            let script = owner.map(|i| self.module_path(i));
            self.begin(owner.map(|i| self.instances[i].entity), script.clone());
            let result = match this {
                Some(this) => timer.callback.call::<()>(this),
                None => timer.callback.call::<()>(()),
            };
            self.end();
            self.calls += 1;
            if let Err(err) = result {
                let message = describe(&err);
                let path = script.unwrap_or_else(|| "timer".into());
                if let Some(i) = owner {
                    self.instances[i].failed = true;
                    self.changed = true;
                }
                log::error!("{message}");
                let entity = owner.map(|i| self.instances[i].entity);
                world.borrow_mut_scoped::<World, _>(|world| {
                    record(world, &path, entity, "timer", message)
                })?;
                continue;
            }
            if let Some(every) = timer.every
                && let Some(mut timers) = self.lua.app_data_mut::<Timers>()
            {
                if let Some(at) = timers.cancelled.iter().position(|id| *id == timer.id) {
                    timers.cancelled.swap_remove(at);
                } else {
                    timer.due += every;
                    timers.list.push(timer);
                }
            }
        }
        if let Some(mut timers) = self.lua.app_data_mut::<Timers>() {
            timers.cancelled.clear();
        }
        Ok(())
    }

    fn module_path(&self, i: usize) -> Rc<str> {
        self.modules
            .get(&self.instances[i].script)
            .map_or_else(|| "?".into(), |m| m.path.clone())
    }

    fn begin(&self, entity: Option<Entity>, script: Option<Rc<str>>) {
        if let Some(mut current) = self.lua.app_data_mut::<Current>() {
            *current = Current { entity, script };
        }
        self.reset_budget();
    }

    fn end(&self) {
        if let Some(mut current) = self.lua.app_data_mut::<Current>() {
            *current = Current::default();
        }
    }

    /// Calls the hook `name` of instance `i`, if its file has one, unless the instance was
    /// stopped by an error or its entity is gone. An error stops the instance and is
    /// recorded; only failures of the runtime itself are returned.
    fn call(
        &mut self,
        world: &LuaAnyUserData,
        i: usize,
        name: &str,
        args: impl IntoLuaMulti,
    ) -> LuaResult<()> {
        let instance = &self.instances[i];
        if instance.failed {
            return Ok(());
        }
        let entity = instance.entity;
        let alive =
            world.borrow_mut_scoped::<World, _>(|world| world.get_entity(entity).is_ok())?;
        if !alive {
            return Ok(());
        }
        let Some(module) = self.modules.get(&instance.script) else {
            return Ok(());
        };
        let Some(hooks) = &module.hooks else {
            return Ok(());
        };
        let hook = match hooks.raw_get::<LuaValue>(name)? {
            LuaValue::Function(f) => f,
            LuaValue::Nil => return Ok(()),
            other => {
                let message = format!(
                    "{}: `{name}` must be a function, not {}",
                    module.path,
                    other.type_name()
                );
                return self.fail(world, i, name, message);
            }
        };
        let path = module.path.clone();
        let this = instance.this.clone();
        self.begin(Some(entity), Some(path));
        let result = hook.call::<()>((this, args));
        self.end();
        self.calls += 1;
        match result {
            Ok(()) => Ok(()),
            Err(err) => self.fail(world, i, name, describe(&err)),
        }
    }

    fn fail(
        &mut self,
        world: &LuaAnyUserData,
        i: usize,
        hook: &str,
        message: String,
    ) -> LuaResult<()> {
        let path = self.module_path(i);
        let instance = &mut self.instances[i];
        instance.failed = true;
        self.changed = true;
        let entity = instance.entity;
        log::error!("{message}\n({hook} of {entity}; its script stops until {path} changes)");
        world.borrow_mut_scoped::<World, _>(|world| {
            record(world, &path, Some(entity), hook, message)
        })
    }

    /// Updates [`ScriptStatus`] when something changed.
    fn publish(&mut self, world: &mut World) {
        let mut status = world.resource_mut::<ScriptStatus>();
        if !core::mem::take(&mut self.changed) && status.calls == self.calls {
            return;
        }
        let mut scripts: BTreeMap<&str, ScriptInfo> = BTreeMap::new();
        for module in self.modules.values() {
            scripts.insert(
                &module.path,
                ScriptInfo {
                    path: module.path.to_string(),
                    loads: module.loads,
                    error: module.error.clone(),
                    ..default()
                },
            );
        }
        for instance in &self.instances {
            if let Some(info) = self
                .modules
                .get(&instance.script)
                .and_then(|m| scripts.get_mut(&*m.path))
            {
                info.instances += 1;
                info.failed += usize::from(instance.failed);
            }
        }
        status.scripts = scripts.into_values().collect();
        status.calls = self.calls;
    }
}

/// Records an error in [`ScriptErrors`].
fn record(world: &mut World, script: &str, entity: Option<Entity>, during: &str, message: String) {
    let frame = world.get_resource::<Time>().map_or(0, Time::frame_count);
    world.resource_mut::<ScriptErrors>().push(ScriptError {
        script: script.to_owned(),
        entity,
        during: during.to_owned(),
        message,
        frame,
    });
}

/// A Lua error as a message that starts with the file and line, followed by the
/// traceback. Errors raised by the engine's functions (a missing component, a bad
/// argument) get the line of the script that called them.
fn describe(err: &LuaError) -> String {
    match err {
        LuaError::CallbackError { traceback, cause } => {
            let inner = describe(cause);
            // The first frame in a script file is the caller.
            let at = traceback
                .lines()
                .map(str::trim)
                .filter(|line| !line.starts_with("[C]"))
                .find_map(|line| {
                    let (place, _) = line.split_once(": in ")?;
                    Some(place.to_owned())
                });
            let first = inner.lines().next().unwrap_or_default();
            match at {
                Some(at) if !first.starts_with(&at) => format!("{at}: {inner}\n{traceback}"),
                _ => format!("{inner}\n{traceback}"),
            }
        }
        LuaError::RuntimeError(message) => message.clone(),
        LuaError::SyntaxError { message, .. } => message.clone(),
        LuaError::MemoryError(message) => format!("out of memory: {message}"),
        LuaError::BadArgument {
            to,
            pos,
            name,
            cause,
        } => {
            let what = match (to, name) {
                (Some(to), _) => format!("`{to}`"),
                (None, Some(name)) => format!("`{name}`"),
                _ => "a function".to_owned(),
            };
            format!("bad argument #{pos} to {what}: {}", describe(cause))
        }
        LuaError::FromLuaConversionError { from, to, message } => match message {
            Some(message) => message.clone(),
            None => format!("expected {to}, got {from}"),
        },
        other => other.to_string(),
    }
}

/// The scripts' run in `FixedUpdate`.
fn run(world: &mut World) {
    let Some(mut runtime) = world.remove_non_send::<LuaRuntime>() else {
        return;
    };
    runtime.prepare(world);
    let lua = runtime.lua.clone();
    let result = lua.scope(|scope| {
        let lent = scope.create_any_userdata_ref_mut(world)?;
        lua.set_named_registry_value(WORLD, &lent)?;
        let result = runtime.run_hooks(&lent);
        lua.set_named_registry_value(WORLD, LuaNil)?;
        result
    });
    if let Err(err) = result {
        log::error!("the script run failed: {}", describe(&err));
    }
    runtime.publish(world);
    world.insert_non_send(runtime);
}
