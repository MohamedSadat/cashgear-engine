//! The game without a window: the bot plays the level to the door.

use super::*;
use crate::bot::{Bot, TIMEOUT};
use crate::level::Run;
use cashgear_engine::audio::AudioOutput;
use cashgear_engine::platform::input::InputPlugin;

/// The game with the engine's plugins but no window: a mock sound device and saves in
/// memory, as CI's machines have neither a screen nor a sound card.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        hot_reload: Some(false),
        ..default()
    })
    .add_plugins(InputPlugin { gamepads: false })
    .add_plugins(StoragePlugin {
        memory: true,
        ..default()
    })
    .add_plugins(TiledPlugin)
    .add_plugins(PhysicsPlugin::default())
    .add_plugins(AnimPlugin::default())
    .add_plugins(AudioPlugin {
        output: AudioOutput::Mock,
        settings_key: None,
        ..default()
    })
    .add_plugins(UiPlugin::default())
    .add_plugins(ScriptPlugin::default())
    .add_plugins(Game {
        options: Options { demo: true },
    })
    .insert_resource(Window::with_size(UVec2::new(1280, 720), 1.0));
    app
}

#[test]
fn the_bot_reaches_the_door() {
    let mut app = app();
    let mut exit = None;
    for _ in 0..TIMEOUT + 300 {
        app.update(core::time::Duration::from_millis(16));
        if let Some(code) = app.should_exit() {
            exit = Some(code);
            break;
        }
    }
    let bot = app.world().resource::<Bot>();
    let run = app.world().resource::<Run>();
    assert_eq!(exit, Some(AppExit::Success), "{bot:?} {run:?}");
    assert!(bot.won);
    assert!(run.coins > 0, "{run:?}");
}
