//! `cg_editor`: the scene editor on its own, for any asset folder.
//!
//! ```text
//! cargo run -p cg_editor -- --asset-root examples/scripting/assets maps/level.tmx
//! ```
//!
//! Without a game built in, Play runs the engine's own systems on the scene: physics,
//! animation and Lua scripts. Every `.prefab.ron` in the asset folder is loaded, named
//! after its file in PascalCase (`coin.prefab.ron` is the prefab `Coin`), so map objects
//! of that class spawn as it. A game's own editor runs its Rust code too (see the
//! platformer's `platformer_editor`).

use cashgear_engine::prelude::*;
use cg_editor::{EditorPlugin, PlayMode};

fn main() -> AppExit {
    let args = Args::from_env();
    App::new()
        .add_plugins(DefaultPlugins {
            window: WindowConfig {
                title: "CashGear Editor".into(),
                width: 1600,
                height: 900,
                ..default()
            },
            assets: AssetPlugin {
                hot_reload: Some(true),
                ..default()
            },
            ..default()
        })
        .add_plugins(EditorPlugin {
            load_prefabs: true,
            ..EditorPlugin::new(PlayMode::Scene).with_args(&args)
        })
        .run()
}
