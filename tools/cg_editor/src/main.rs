//! `cg_editor`: the scene editor on its own, for any asset folder.
//!
//! ```text
//! cargo run -p cg_editor -- --asset-root examples/scripting/assets maps/playground.tmx
//! ```
//!
//! Without a game built in, Play runs the engine's own systems on the scene: physics,
//! animation and Lua scripts. Every `.prefab.ron` in the asset folder is loaded, named
//! after its file in PascalCase (`coin.prefab.ron` is the prefab `Coin`), so map objects
//! of that class spawn as it. A game's own editor runs its Rust code too (see the
//! platformer's `platformer_editor`).
//!
//! Given no file, it opens the first map in the asset folder's `maps/`. Play's scripts
//! read the actions of [`cg_editor::play_controls`]. The release's Windows download is
//! this binary with the scripting example's assets beside it.

// No console window behind the editor in Windows release builds; its console panel shows
// the log.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cashgear_engine::prelude::*;
use cg_editor::{EditorPlugin, PlayMode};
use std::path::Path;

fn main() -> AppExit {
    let args = Args::from_env();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins {
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
    });
    app.insert_resource(cg_editor::play_controls());
    let mut editor = EditorPlugin {
        load_prefabs: true,
        ..EditorPlugin::new(PlayMode::Scene).with_args(&args)
    };
    if editor.open.is_none() {
        editor.open = first_map(Path::new(app.world().resource::<AssetServer>().io().root()));
    }
    app.add_plugins(editor).run()
}

/// The first `.tmx` in the asset folder's `maps/`, else in the folder itself, so a
/// double-clicked editor opens on a level.
#[allow(clippy::disallowed_methods)] // A desktop tool: it lists the asset folder.
fn first_map(root: &Path) -> Option<String> {
    ["maps", ""].into_iter().find_map(|dir| {
        let mut maps: Vec<String> = std::fs::read_dir(root.join(dir))
            .ok()?
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .filter(|name| name.ends_with(".tmx"))
            .collect();
        maps.sort();
        let name = maps.into_iter().next()?;
        Some(if dir.is_empty() {
            name
        } else {
            format!("{dir}/{name}")
        })
    })
}
