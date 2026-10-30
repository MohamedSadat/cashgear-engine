//! Lua scripting for CashGear Engine: gameplay behaviours written in Lua 5.4 and reloaded
//! while the game runs, without recompiling.
//!
//! A [`Script`] component runs a `.lua` file on its entity. The file returns a table of
//! hooks; each entity gets its own `self` table, which keeps its fields when the file is
//! reloaded:
//!
//! ```lua
//! -- scripts/patrol.lua: walks back and forth, turning at walls.
//! local patrol = {}
//!
//! function patrol.on_start(self)
//!   self.dir = -1
//!   self.speed = self:props().speed or 30   -- Tiled properties
//! end
//!
//! function patrol.on_update(self, dt)
//!   local c = self:contacts()
//!   if (self.dir < 0 and c.wall_left) or (self.dir > 0 and c.wall_right) then
//!     self.dir = -self.dir
//!   end
//!   self:set_velocity(self.dir * self.speed, nil)   -- nil keeps the vertical speed
//!   self:set_flip_x(self.dir > 0)
//! end
//!
//! function patrol.on_collision(self, other)
//!   if other:has("Player") then
//!     audio.play("sounds/bump.ogg", { volume = 0.5 })
//!   end
//! end
//!
//! return patrol
//! ```
//!
//! # Hooks
//!
//! All optional, all called in `FixedUpdate` ([`ScriptSystems::Run`]), so a script's run
//! depends only on the number of fixed steps, never on the frame rate. In each run:
//!
//! 1. `on_start(self)`, once, for entities new since the last run;
//! 2. `on_reload(self)` after the file changed and compiled;
//! 3. `on_event(self, name, data, from)` for events from Rust ([`ScriptEvents`]) and from
//!    other scripts (`entity:send`), the oldest first;
//! 4. `on_collision(self, other)` and `on_collision_end(self, other)` for the latest physics
//!    step's `CollisionStarted` and `CollisionEnded`;
//! 5. timers that are due;
//! 6. `on_update(self, dt)`.
//!
//! # The API
//!
//! - **The entity**: `self` (and `other`, the entities in hooks and results) have
//!   `position`, `set_position`, `world_position`, `rotation`, `set_rotation`, `scale`,
//!   `set_scale`; `velocity`, `set_velocity`, `contacts` (a `KinematicBody`'s);
//!   `flip_x`, `set_flip_x`, `color`, `set_color` (the `Sprite`); `play` and `clip` (the
//!   `SpriteAnimation`); `set_param` and `trigger` (the `Animator`); `tween`; `props` (the
//!   Tiled object's properties); `name`, `has`, `parent`, `children`, `child(name)`,
//!   `alive`, `is`, `id`, `despawn`, `insert_prefab`, `send`; and on `self`, `after` and
//!   `every` (timers owned by the entity).
//! - **`world`**: `spawn(prefab, x, y)`, `find(name)`, `raycast`, `tile_at`,
//!   `overlap_point`.
//! - **`input`**: `pressed`, `just_pressed`, `just_released`, `value`, `axis`, by action.
//! - **`audio`**: `play(path, opts)`, `stop(voice, fade)`, `music(path, opts)`,
//!   `stop_music(fade)`.
//! - **`timer`**: `after(seconds, fn)`, `every(seconds, fn)`; both return a timer with
//!   `cancel()`.
//! - **`events`**: `emit(name, data)` ([`ScriptMessage`] for the game) and
//!   `broadcast(name, data)` (every script's `on_event`).
//! - **`state`**: `current`, `stack`, `push`, `pop`, `replace`, `set`, once the game calls
//!   [`AppScriptExt::add_script_states`].
//! - **`time`**: `dt`, `now` (seconds of script time), `step`, `elapsed`, `frame`.
//! - **`log`**: `trace`, `debug`, `info`, `warn`, `error`; `print` logs at info.
//! - The game's own tables and functions ([`AppScriptExt::add_script_function`]) and
//!   component names for `has` ([`AppScriptExt::add_script_component`]).
//!
//! The details:
//!
//! - Positions, velocities and scales come and go as two numbers (`local x, y =
//!   self:position()`); `set_velocity(x, nil)` keeps the vertical speed, and
//!   `set_scale(2)` scales both axes. Positions are the local `Transform2D`'s;
//!   `world_position` is where the entity is drawn.
//! - `contacts()` is a table of booleans: `ground`, `ceiling`, `wall_left`, `wall_right`,
//!   `hazard`, `slope`, `landed` and `left_ground` (this step).
//! - Colours are `0xRRGGBB`, `"#rrggbb"` or `"#rrggbbaa"`, or `{r, g, b, a}` from 0 to 1;
//!   `color()` returns four numbers.
//! - `tween(property, to, seconds, options)` animates `"position"`, `"scale"`,
//!   `"rotation"`, `"color"` or `"flash"`; options are `from`, `ease` (a curve's name, such
//!   as `"BackOut"`), `delay`, `repeats`, `forever`, `yoyo`, `tag`, and `add` (run
//!   alongside the tweens of the same property instead of replacing them).
//! - `props()` is the Tiled object's properties as a table: numbers, booleans, strings,
//!   colours as `"#rrggbbaa"`, objects as their ids, classes as tables.
//! - `world.raycast(x, y, dx, dy, max_distance, filter)` returns `nil` or `{entity, x, y,
//!   nx, ny, distance, tile, cx, cy}`; `world.tile_at(x, y, filter)` returns `nil` or
//!   `{entity, cx, cy, solid, one_way, slope, hazard}`; `world.overlap_point(x, y,
//!   filter)` a list of entities. A filter is `{mask = layers, exclude = entity, sensors =
//!   false, tiles = false}`, every field optional. Queries see the latest physics step.
//! - `audio.play(path, options)` returns a voice for `audio.stop(voice, fade)`; options
//!   are `volume`, `speed`, `pitch_jitter`, `looped`, `fade_in`, `x` and `y` (a position),
//!   `follow` (an entity) and `bus` (`"Sfx"`, `"Ui"` or `"Music"`). `audio.music(path,
//!   options)` takes `volume`, `crossfade` and `looped`.
//! - `self:after(seconds, fn)` and `self:every(seconds, fn)` call `fn(self)`, and end with
//!   the entity; `timer.after` and `timer.every` call `fn()`. Times are counted in fixed
//!   steps, at least one.
//! - Event data and game function arguments cross as [`ScriptValue`]s: `nil`, booleans,
//!   numbers, strings, entities and tables of them.
//!
//! The standard libraries are `string`, `table`, `math`, `utf8` and `coroutine` and the
//! safe part of the base library. There is no `os`, `io`, `package`, `require`, `debug`,
//! `load`, `dofile` or `string.dump`: scripts cannot touch files or the system.
//! `math.random` starts from a fixed seed so runs repeat. Each file has its own globals.
//!
//! # Errors
//!
//! A file that does not compile is logged and recorded in [`ScriptErrors`]; the version
//! before it keeps running. An error in a hook is logged with its file, line and traceback,
//! and stops that entity's script until the file changes (no error every step). A hook
//! that runs more than [`ScriptPlugin::max_instructions`] is stopped the same way, so an
//! endless loop cannot freeze the game. With the debug overlay (feature `debug-overlay`)
//! the Scripts window lists every script and error, and the overlay opens at the first
//! error ([`ScriptPlugin::show_errors`]).
//!
//! # Targets
//!
//! Lua (through `mlua`, with its C sources compiled in) runs on desktop and Android, and
//! needs the `lua` feature, which the facade turns on for every target but wasm32. `mlua`
//! does not build for `wasm32-unknown-unknown`, so on the web this crate builds without it:
//! the plugin, the components and the API are there, scripts never run, and
//! [`Scripting::runs`] is false. Games that ship on the web keep Rust versions of their
//! scripted behaviours and pick by it.

mod api;
#[cfg(feature = "lua")]
mod lua;
#[cfg(feature = "debug-overlay")]
mod overlay;
mod script;
mod value;

#[cfg(all(test, feature = "lua"))]
mod tests;

pub use api::{AppScriptExt, ScriptApi};
pub use script::{
    LuaScript, LuaScriptLoader, Script, ScriptError, ScriptErrors, ScriptEvent, ScriptEvents,
    ScriptInfo, ScriptMessage, ScriptStatus, Scripting,
};
pub use value::{FromScriptArgs, FromScriptValue, IntoScriptValue, ScriptValue};

use bevy_ecs::prelude::*;
use cg_assets::AppAssetExt;
use cg_core::prelude::*;
use cg_platform::Args;
use cg_world::AppWorldExt;

/// Sets for the script systems.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptSystems {
    /// The scripts' run, in `FixedUpdate`. Games order it among their own sets; the
    /// platformer runs it with its enemies' movement.
    Run,
}

/// Lua scripting: the `.lua` loader, the [`Script`] component (also a prefab component),
/// the runtime and the API.
#[derive(Debug, Clone)]
pub struct ScriptPlugin {
    /// Whether scripts run. `--no-scripts` turns it off; see [`Scripting::runs`].
    pub enabled: bool,
    /// Lua instructions one hook call may run (counted in thousands) before it is stopped
    /// as stuck. The default, 20 million, is far more than a step of gameplay needs.
    pub max_instructions: u32,
    /// The memory Lua may use, in bytes (64 MiB by default).
    pub memory_limit: usize,
    /// Open the debug overlay (with the `debug-overlay` feature) when a script fails. On in
    /// debug builds.
    pub show_errors: bool,
}

impl Default for ScriptPlugin {
    fn default() -> Self {
        Self {
            enabled: true,
            max_instructions: 20_000_000,
            memory_limit: 64 << 20,
            show_errors: cfg!(debug_assertions),
        }
    }
}

impl ScriptPlugin {
    /// Applies the launch flags: `--no-scripts`.
    pub fn with_args(mut self, args: &Args) -> Self {
        if args.flag("no-scripts") {
            self.enabled = false;
        }
        self
    }
}

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<LuaScript>()
            .register_loader(LuaScriptLoader)
            .register_prefab_component::<Script>("Script")
            .insert_resource(Scripting {
                enabled: self.enabled,
            })
            .init_resource::<ScriptEvents>()
            .init_resource::<ScriptErrors>()
            .init_resource::<ScriptStatus>()
            .add_message::<ScriptMessage>();
        app.world_mut().get_resource_or_init::<ScriptApi>();
        add_engine_components(app);
        #[cfg(feature = "lua")]
        if self.enabled {
            lua::build(app, self);
        }
        #[cfg(not(feature = "lua"))]
        app.add_systems(FixedUpdate, warn_without_lua.in_set(ScriptSystems::Run));
        #[cfg(feature = "debug-overlay")]
        overlay::build(app, self.show_errors);
    }
}

/// The engine's components that `entity:has(name)` knows by default.
fn add_engine_components(app: &mut App) {
    use cg_anim::{Animator, ParticleEmitter, SpriteAnimation, Tweens};
    use cg_physics::{Collider, KinematicBody, Sensor};
    use cg_render::{Camera2D, Sprite};
    use cg_tiled::TiledObject;
    use cg_world::Transform2D;
    app.add_script_component::<Transform2D>("Transform2D")
        .add_script_component::<Sprite>("Sprite")
        .add_script_component::<Camera2D>("Camera2D")
        .add_script_component::<KinematicBody>("KinematicBody")
        .add_script_component::<Collider>("Collider")
        .add_script_component::<Sensor>("Sensor")
        .add_script_component::<SpriteAnimation>("SpriteAnimation")
        .add_script_component::<Animator>("Animator")
        .add_script_component::<Tweens>("Tweens")
        .add_script_component::<ParticleEmitter>("ParticleEmitter")
        .add_script_component::<TiledObject>("TiledObject")
        .add_script_component::<Script>("Script");
}

/// Without Lua, says once that scripts do not run here.
#[cfg(not(feature = "lua"))]
fn warn_without_lua(scripts: Query<(), Added<Script>>, mut warned: Local<bool>) {
    if !*warned && !scripts.is_empty() {
        *warned = true;
        log::warn!("Lua scripts do not run on this platform; their entities do nothing");
    }
}

/// Common imports for game code using scripts.
pub mod prelude {
    pub use crate::{
        AppScriptExt, LuaScript, Script, ScriptErrors, ScriptEvents, ScriptMessage, ScriptPlugin,
        ScriptSystems, ScriptValue, Scripting,
    };
}
