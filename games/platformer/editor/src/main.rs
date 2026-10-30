//! `platformer_editor`: the platformer with the scene editor built in. It opens level 1
//! (or the map given), and Play runs the level being edited with the game's own code.
//!
//! ```text
//! cargo run -p platformer_editor
//! cargo run -p platformer_editor -- maps/level2.tmx
//! cargo run -p platformer_editor -- --demo
//! ```
//!
//! `--demo` runs the editor's scripted demo (it saves into a scratch folder, never into the
//! game's files); CI runs it. Plays from the editor keep the game's saves in memory, so
//! they never unlock levels or set best times.

use cashgear_engine::prelude::*;
use cg_editor::{EditorPlugin, PlayMode};
use platformer::{Game, Options};

#[cfg(test)]
mod tests;

/// The game's assets, unless `CG_ASSET_ROOT` or `--asset-root` says otherwise.
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets");

fn main() -> AppExit {
    let args = Args::from_env();
    // The game's bot and `--level` belong to the game; the editor's `--demo` is its own.
    let options = Options::default();
    let mut engine = platformer::engine_plugins(&options);
    engine.window = WindowConfig {
        title: "CashGear Editor: platformer".into(),
        width: 1600,
        height: 900,
        ..default()
    };
    engine.storage.memory = true;
    engine.assets.hot_reload = Some(true);
    if std::env::var_os("CG_ASSET_ROOT").is_none() {
        engine.assets.root = Some(ASSETS.into());
    }
    let mut editor = EditorPlugin::new(PlayMode::Game).with_args(&args);
    if editor.open.is_none() {
        editor.open = Some("maps/level1.tmx".into());
    }
    App::new()
        .add_plugins(engine)
        .add_plugins(editor)
        .add_plugins(Game { options })
        .run()
}
