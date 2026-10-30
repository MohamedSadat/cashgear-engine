//! Script files, the [`Script`] component, events between Rust and scripts, and what the
//! runtime reports ([`ScriptErrors`], [`ScriptStatus`]).

use crate::value::{IntoScriptValue, ScriptValue};
use bevy_ecs::prelude::*;
use cg_assets::{Asset, AssetLoader, AssetServer, BoxError, BoxedFuture, Handle, LoadContext};
use cg_world::PrefabComponent;
use std::collections::VecDeque;

/// A Lua source file (`.lua`), loaded through the asset server. The runtime compiles it
/// when it loads and again whenever it changes (hot reload on desktop).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LuaScript {
    source: String,
}

impl LuaScript {
    /// A script from its source text, for scripts made in code (add it with
    /// `Assets<LuaScript>::add` or `AssetServer::add`).
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    /// The source text.
    pub fn source(&self) -> &str {
        &self.source
    }
}

impl Asset for LuaScript {}

/// Loads `.lua` files as [`LuaScript`]s. `ScriptPlugin` registers it.
#[derive(Debug, Default)]
pub struct LuaScriptLoader;

impl AssetLoader for LuaScriptLoader {
    type Asset = LuaScript;

    fn extensions(&self) -> &[&str] {
        &["lua"]
    }

    fn load<'a>(
        &'a self,
        ctx: &'a mut LoadContext,
    ) -> BoxedFuture<'a, Result<LuaScript, BoxError>> {
        Box::pin(async move {
            let source = core::str::from_utf8(ctx.bytes())?;
            // A byte-order mark from an editor is not Lua.
            Ok(LuaScript::new(source.trim_start_matches('\u{feff}')))
        })
    }
}

/// Runs a Lua script on its entity. The script file returns a table of hooks, which the
/// runtime calls with the entity's own `self` table:
///
/// ```lua
/// local spinner = {}
///
/// function spinner.on_start(self)
///   self.speed = self:props().speed or 2
/// end
///
/// function spinner.on_update(self, dt)
///   self:set_rotation(self:rotation() + self.speed * dt)
/// end
///
/// return spinner
/// ```
///
/// In prefabs and Tiled properties a script is its asset path:
/// `"Script": "scripts/spinner.lua"`.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// The script file.
    pub script: Handle<LuaScript>,
}

impl Script {
    /// Runs `script` on the entity.
    pub fn new(script: Handle<LuaScript>) -> Self {
        Self { script }
    }
}

impl PrefabComponent for Script {
    type Data = String;

    fn from_data(path: &String, world: &World) -> Result<Self, String> {
        let server = world
            .get_resource::<AssetServer>()
            .ok_or_else(|| format!("no asset server to load `{path}`"))?;
        Ok(Self::new(server.load(path.as_str())))
    }
}

/// Events for scripts, sent from Rust: each reaches the `on_event(self, name, data, from)`
/// hook of the scripts it is for, at the start of the next script run (before any
/// `on_update`).
///
/// ```
/// use cg_script::ScriptEvents;
/// # use bevy_ecs::prelude::*;
///
/// fn stomp(mut events: ResMut<ScriptEvents>, enemy: Entity) {
///     events.send(enemy, "stomped", ());
/// }
/// ```
#[derive(Resource, Debug, Default)]
pub struct ScriptEvents {
    queue: Vec<ScriptEvent>,
}

/// One event for scripts.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptEvent {
    /// The entity whose script gets it; `None` for every script.
    pub target: Option<Entity>,
    /// The event's name.
    pub name: String,
    /// Its data.
    pub data: ScriptValue,
    /// The entity whose script sent it, if a script did.
    pub from: Option<Entity>,
}

impl ScriptEvents {
    /// Sends `name` with `data` to the script on `entity`.
    pub fn send(&mut self, entity: Entity, name: impl Into<String>, data: impl IntoScriptValue) {
        self.push(ScriptEvent {
            target: Some(entity),
            name: name.into(),
            data: data.into_script_value(),
            from: None,
        });
    }

    /// Sends `name` with `data` to every script.
    pub fn broadcast(&mut self, name: impl Into<String>, data: impl IntoScriptValue) {
        self.push(ScriptEvent {
            target: None,
            name: name.into(),
            data: data.into_script_value(),
            from: None,
        });
    }

    /// Queues an event.
    pub fn push(&mut self, event: ScriptEvent) {
        self.queue.push(event);
    }

    /// The events waiting.
    pub fn pending(&self) -> &[ScriptEvent] {
        &self.queue
    }

    /// Takes the events waiting.
    pub fn take(&mut self) -> Vec<ScriptEvent> {
        core::mem::take(&mut self.queue)
    }
}

/// A message from a script to the game: `events.emit(name, data)` in Lua. Read it with a
/// `MessageReader<ScriptMessage>`; the game decides what it means (a score, a cutscene, a
/// screen to open).
#[derive(Message, Debug, Clone, PartialEq)]
pub struct ScriptMessage {
    /// The entity whose script sent it (`None` from a global timer).
    pub entity: Option<Entity>,
    /// The message's name.
    pub name: String,
    /// Its data.
    pub data: ScriptValue,
}

/// An error a script raised or ran into: a syntax error when its file loaded, or an error
/// in one of its hooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    /// The script's asset path.
    pub script: String,
    /// The entity it ran on, if it was in a hook.
    pub entity: Option<Entity>,
    /// Where it happened: `load`, or the hook's name (`on_update`, a timer).
    pub during: String,
    /// Lua's message, starting with the file and line, and the traceback.
    pub message: String,
    /// The frame it happened in.
    pub frame: u64,
}

impl ScriptError {
    /// The first line of the message: the file, the line and what went wrong.
    pub fn summary(&self) -> &str {
        self.message.lines().next().unwrap_or_default()
    }
}

/// The latest script errors (at most 32, oldest first). The debug overlay's Scripts
/// window shows them; a game can too.
#[derive(Resource, Debug, Default)]
pub struct ScriptErrors {
    errors: VecDeque<ScriptError>,
    count: u64,
}

impl ScriptErrors {
    const KEEP: usize = 32;

    /// Records an error.
    pub fn push(&mut self, error: ScriptError) {
        if self.errors.len() == Self::KEEP {
            self.errors.pop_front();
        }
        self.errors.push_back(error);
        self.count += 1;
    }

    /// The errors kept, oldest first.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &ScriptError> + ExactSizeIterator {
        self.errors.iter()
    }

    /// The latest error.
    pub fn last(&self) -> Option<&ScriptError> {
        self.errors.back()
    }

    /// How many errors there have been since the start (also those no longer kept).
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Whether none are kept.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    /// Forgets the errors kept (the count stays).
    pub fn clear(&mut self) {
        self.errors.clear();
    }
}

/// How each script file is doing, updated by the runtime after every run that changed
/// something.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
pub struct ScriptStatus {
    /// One entry per script file in use, by path.
    pub scripts: Vec<ScriptInfo>,
    /// Hook calls in the latest run.
    pub calls: u32,
}

/// One script file in [`ScriptStatus`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScriptInfo {
    /// Its asset path.
    pub path: String,
    /// The entities running it.
    pub instances: usize,
    /// How many times it was compiled (1 at first, one more per reload).
    pub loads: u32,
    /// Its latest compile error, while its file does not compile (the previous version
    /// keeps running).
    pub error: Option<String>,
    /// Instances stopped by an error in a hook, until the file changes.
    pub failed: usize,
}

/// Whether scripts run here.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scripting {
    pub(crate) enabled: bool,
}

impl Scripting {
    /// Whether this build has Lua: everywhere but the web.
    pub const LUA: bool = cfg!(feature = "lua");

    /// Whether scripts run: Lua is built in and `ScriptPlugin::enabled` (not `--no-scripts`).
    /// A game with Rust versions of its scripted behaviours uses them when this is false.
    pub fn runs(&self) -> bool {
        Self::LUA && self.enabled
    }
}
