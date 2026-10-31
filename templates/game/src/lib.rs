//! {{title}}: a game made with CashGear Engine.
//!
//! The game is a library, so the desktop binary (`src/main.rs`), the web page, the Android
//! library (`android/`), the scene editor (`editor/`) and the tests all build the same
//! [`Game`]. Start reading at [`Game::build`]; each module is one part of the game:
//!
//! - [`screens`]: the title, the HUD and the win screen, as a state stack.
//! - [`level`]: the Tiled map, its objects (the start, coins, the door) and collecting.
//! - [`player`]: the alien: input, running and jumping, its animation and sounds.
//! - [`bot`]: `--demo` lets a bot play, which the test and CI use.

#[cfg(test)]
mod tests;

pub mod bot;
pub mod level;
pub mod player;
pub mod screens;

use cashgear_engine::prelude::*;

/// The game's screens. `Loading` waits for the assets; the HUD belongs to `Playing`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Screen {
    /// Loading the assets.
    Loading,
    /// Idle while the scene editor edits the level; its Play button starts `Playing`.
    Editor,
    /// The title screen.
    Title,
    /// The level.
    Playing,
    /// The door was reached.
    Won,
}

/// What the command line (or the page's URL on the web) asked for.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct Options {
    /// A bot plays the level and the game exits when it reaches the door (`--demo`).
    pub demo: bool,
}

impl Options {
    /// Reads `--demo`.
    pub fn from_args() -> Self {
        Self {
            demo: Args::from_env().flag("demo"),
        }
    }
}

/// The engine's plugins as the game sets them up: its window title and its save folder.
pub fn engine_plugins(options: &Options) -> DefaultPlugins {
    DefaultPlugins {
        window: WindowConfig {
            title: "{{title}}".into(),
            canvas_id: Some("cashgear".into()),
            ..default()
        },
        storage: StoragePlugin {
            namespace: Some("{{project-name}}".into()),
            // The bot never touches the player's saves.
            memory: options.demo,
        },
        ..default()
    }
}

/// Builds and runs the game: what `main` (desktop and web) and the Android library call.
// ANCHOR: run
pub fn run() -> AppExit {
    let options = Options::from_args();
    App::new()
        .add_plugins(engine_plugins(&options))
        .add_plugins(Game { options })
        .run()
}
// ANCHOR_END: run

/// The game: everything but the engine's plugins, so tests and the editor can build it
/// around their own.
pub struct Game {
    /// The launch options.
    pub options: Options,
}

/// The game's sets of systems in `FixedUpdate`, run in this order while a level is played.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSystems {
    /// Reading the controls (or the bot).
    Input,
    /// Moving the player.
    Move,
    /// Coins, the door, falling out of the level.
    Interact,
}

// ANCHOR: game
impl Plugin for Game {
    fn build(&self, app: &mut App) {
        // In the scene editor the game waits for Play instead of showing its title.
        let first = if app.world().contains_resource::<EditMode>() {
            Screen::Editor
        } else {
            Screen::Title
        };
        app.insert_resource(self.options)
            .insert_resource(player::action_map())
            .init_state(Screen::Loading)
            .add_loading_state(Screen::Loading, first)
            .configure_sets(
                FixedUpdate,
                (GameSystems::Input, GameSystems::Move, GameSystems::Interact)
                    .chain()
                    .run_if(in_state(Screen::Playing)),
            );
        level::plugin(app);
        player::plugin(app);
        screens::plugin(app);
        bot::plugin(app);
    }
}
// ANCHOR_END: game
