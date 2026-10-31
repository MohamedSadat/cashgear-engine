//! {{title}} with the scene editor built in: `cargo run -p {{crate_name}}_editor`. It opens
//! the level; place and tune its objects, save (Ctrl+S) into `assets/maps/level.tmx`, and
//! Play (F5) runs the level with the game's own code. Stop brings the edited scene back.

use cashgear_engine::prelude::*;
use cg_editor::{EditorPlugin, PlayMode};
use {{crate_name}}::{Game, Options};

/// The game's assets, unless `CG_ASSET_ROOT` or `--asset-root` says otherwise.
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../assets");

fn main() -> AppExit {
    let args = Args::from_env();
    let options = Options::default();
    let mut engine = {{crate_name}}::engine_plugins(&options);
    engine.window = WindowConfig {
        title: "{{title}}: editor".into(),
        width: 1600,
        height: 900,
        ..default()
    };
    // Plays from the editor keep saves in memory.
    engine.storage.memory = true;
    engine.assets.hot_reload = Some(true);
    if std::env::var_os("CG_ASSET_ROOT").is_none() {
        engine.assets.root = Some(ASSETS.into());
    }
    let mut editor = EditorPlugin::new(PlayMode::Game).with_args(&args);
    if editor.open.is_none() {
        editor.open = Some({{crate_name}}::level::MAP.into());
    }
    App::new()
        .add_plugins(engine)
        .add_plugins(editor)
        .add_plugins(Game { options })
        .run()
}
