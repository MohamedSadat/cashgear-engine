//! Lua scripts, where they run (desktop and Android): the walkers run
//! `scripts/walker.lua`, and the checkpoint flags `scripts/checkpoint.lua` (through
//! `prefabs/checkpoint.prefab.ron`, which the map's `Checkpoint` objects get by their
//! class). The game tells the scripts what they need to know about it in the `game` table.
//!
//! Where scripts do not run (the web, or `--no-scripts`), the Rust versions in `enemies`
//! and `things` do the same; a test plays the whole game both ways and compares.

use crate::GameSystems;
use crate::level::LevelBounds;
use crate::player::Player;
use crate::progress::Session;
use cashgear_engine::prelude::*;

const WALKER: &str = "scripts/walker.lua";

/// Whether the walkers and checkpoints run as scripts, and the walker's script.
#[derive(Resource, Debug, Clone, Default)]
pub(crate) struct Scripts {
    pub walker: Option<Handle<LuaScript>>,
}

impl Scripts {
    /// Whether the scripted versions are used.
    pub fn on(&self) -> bool {
        self.walker.is_some()
    }
}

pub(crate) fn plugin(app: &mut App) {
    // Scripts move things, so they run with the enemies and the platforms, and only while
    // a level is being played.
    app.configure_sets(FixedUpdate, ScriptSystems::Run.in_set(GameSystems::Move));
    let runs = app
        .world()
        .get_resource::<Scripting>()
        .is_some_and(Scripting::runs);
    if !runs {
        app.init_resource::<Scripts>();
        return;
    }
    let walker: Handle<LuaScript> = app.world().resource::<AssetServer>().load(WALKER);
    app.world_mut().resource_mut::<LoadingAssets>().add(&walker);
    app.insert_resource(Scripts {
        walker: Some(walker),
    })
    .load_prefab("Checkpoint", "prefabs/checkpoint.prefab.ron")
    .load_prefab("CheckpointSparks", "prefabs/checkpoint_sparks.prefab.ron")
    .add_script_component::<Player>("Player")
    .add_script_function("game", "fell", |world, (y,): (f32,)| {
        Ok(world.resource::<LevelBounds>().fell(y))
    })
    .add_script_function("game", "set_checkpoint", |world, (x, y): (f32, f32)| {
        world.resource_mut::<Session>().checkpoint = Some(vec2(x, y));
        Ok(())
    })
    // Whether the player can take things: not while coming back from a fall or losing.
    .add_script_function("game", "player_active", |world, ()| {
        let mut players = world.query::<&Player>();
        Ok(players
            .iter(world)
            .next()
            .is_some_and(|player| player.dying <= 0.0 && !player.dead))
    });
}
