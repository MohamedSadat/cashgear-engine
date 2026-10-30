//! `scripting`: a small level whose gameplay is all Lua. Kenney's alien runs and jumps by
//! `scripts/player.lua`, coins bob and chime, red blocks patrol, a spawner drops crates
//! that fade away, and the flag wins the level; the Rust here only loads the level, points
//! the camera, and draws the HUD from what the scripts tell it.
//!
//! - Desktop: `cargo run -p scripting`. Add `-- --demo` to let a bot (written in Lua, in
//!   `player.lua`) run to the goal; it exits with an error if it does not get there. CI
//!   runs it. `--overlay` opens the debug overlay with its Scripts window.
//! - Lua does not run in browsers, so this example has no web page.
//!
//! What it shows:
//!
//! - **Scripts on prefabs from Tiled**: the map's objects have classes (`Player`, `Coin`,
//!   `Patrol`, `Spawner`, `Goal`) that name prefabs in `assets/prefabs`, and each prefab
//!   carries a `Script`. Their Tiled properties (a patrol's `speed`, the spawner's `every`
//!   and `drops`) reach the scripts through `self:props()`.
//! - **Hooks**: `on_start`, `on_update`, `on_collision`, `on_event`, and timers
//!   (`self:after`, `self:every`).
//! - **The API**: input actions, velocities and contacts, raycasts, sprites, the
//!   alien's animation state machine, tweens, sounds, spawning prefabs, events to the game
//!   (`events.emit`) and between scripts (`entity:send`, `events.broadcast`), and a state
//!   change (`state.set("Won")`).
//! - **Hot reload**: save any file in `assets/scripts` while the example runs; the
//!   entities keep their `self` fields and run the new code. Break one (a typo, a nil
//!   index) and the error shows at the bottom of the screen and in the debug overlay's
//!   Scripts window (F12), with its file and line; fix it and save, and it runs again.
//!
//! | Action | Keyboard | Gamepad |
//! | --- | --- | --- |
//! | Run | Arrows, A / D | D-pad, left stick |
//! | Jump (hold for higher) | Space, W, Up | South (A / cross) |
//! | Debug overlay (Scripts window) | F12 | |
//! | Quit | Escape | Select |
//!
//! Art: Kenney's Pixel Platformer (CC0, www.kenney.nl); see `assets/KENNEY-LICENSE.txt`.
//! Sounds: original, from the audio example.

use cashgear_engine::prelude::*;
use cashgear_engine::script::ScriptStatus;

const MAP: &str = "maps/playground.tmx";
/// The sky colour of the background, so the view above the hills matches.
const SKY: u32 = 0xdff6f5;
/// The prefabs the map's object classes name, and their files.
const PREFABS: [(&str, &str); 6] = [
    ("Player", "prefabs/player.prefab.ron"),
    ("Coin", "prefabs/coin.prefab.ron"),
    ("Patrol", "prefabs/patrol.prefab.ron"),
    ("Spawner", "prefabs/spawner.prefab.ron"),
    ("Crate", "prefabs/crate.prefab.ron"),
    ("Goal", "prefabs/goal.prefab.ron"),
];
/// The scripts, loaded before the level starts so the first frame runs them.
const SCRIPTS: [&str; 6] = [
    "scripts/player.lua",
    "scripts/coin.lua",
    "scripts/patrol.lua",
    "scripts/spawner.lua",
    "scripts/crate.lua",
    "scripts/goal.lua",
];
/// The demo gives up after this many fixed steps (a minute).
const DEMO_STEPS: u64 = 60 * 60;
/// Seconds on the win screen before the demo exits.
const DEMO_WIN_WAIT: f32 = 1.0;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum GameState {
    Loading,
    Playing,
    /// Pushed by `scripts/goal.lua`; the level keeps running underneath.
    Won,
}

/// The level's assets.
#[derive(Resource, Default)]
struct Level {
    map: Handle<TiledMap>,
    scripts: Vec<Handle<LuaScript>>,
}

/// Coins taken, and in the level.
#[derive(Resource, Default, Debug)]
struct Score {
    coins: u32,
    total: u32,
}

/// The HUD's texts.
#[derive(Resource, Default)]
struct Hud {
    coins: Option<Entity>,
    scripts: Option<Entity>,
    error: Option<Entity>,
}

/// `--demo`: the bot plays, and the app exits when it wins or runs out of time.
#[derive(Resource, Clone, Copy)]
struct Demo(bool);

fn main() -> AppExit {
    let demo = Args::from_env().flag("demo");
    App::new()
        .add_plugins(DefaultPlugins {
            window: WindowConfig {
                title: "CashGear: scripting".into(),
                ..default()
            },
            ..default()
        })
        .add_plugins(Game { demo })
        .run()
}

/// Everything but the engine's plugins, so the test can build it without a window.
struct Game {
    demo: bool,
}

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        use GameState::{Loading, Playing, Won};
        let demo = self.demo;
        app.insert_resource(action_map())
            .insert_resource(Demo(demo))
            .init_resource::<Level>()
            .init_resource::<Score>()
            .init_resource::<Hud>()
            .init_state(Loading)
            .add_loading_state(Loading, Playing)
            // Scripts may enter `Won`, nothing else.
            .add_script_states(&[Won])
            .add_script_function("game", "demo", move |_, ()| Ok(demo))
            .add_systems(Startup, setup)
            .add_systems(OnEnter(Playing), (spawn_level, spawn_hud))
            .add_systems(OnEnter(Won), show_win)
            .add_systems(PreUpdate, count_coins)
            .add_systems(Update, (read_messages, update_hud, quit).chain())
            .add_systems(FixedUpdate, direct_demo.run_if(|demo: Res<Demo>| demo.0));
        for (name, path) in PREFABS {
            app.load_prefab(name, path);
        }
    }
}

fn action_map() -> ActionMap {
    use AxisDirection::{Negative, Positive};
    ActionMap::new()
        .bind("left", KeyCode::ArrowLeft)
        .bind("left", KeyCode::KeyA)
        .bind("left", GamepadButton::DPadLeft)
        .bind("left", Binding::Axis(GamepadAxis::LeftStickX, Negative))
        .bind("right", KeyCode::ArrowRight)
        .bind("right", KeyCode::KeyD)
        .bind("right", GamepadButton::DPadRight)
        .bind("right", Binding::Axis(GamepadAxis::LeftStickX, Positive))
        .bind("jump", KeyCode::Space)
        .bind("jump", KeyCode::KeyW)
        .bind("jump", KeyCode::ArrowUp)
        .bind("jump", GamepadButton::South)
        .bind("quit", KeyCode::Escape)
        .bind("quit", GamepadButton::Select)
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut loading: ResMut<LoadingAssets>,
    mut level: ResMut<Level>,
) {
    level.map = server.load(MAP);
    loading.add(&level.map);
    for path in SCRIPTS {
        let script: Handle<LuaScript> = server.load(path);
        loading.add(&script);
        level.scripts.push(script);
    }
    commands.spawn((
        Name::new("camera"),
        Camera2D {
            clear_color: Color::hex(SKY),
            ..default()
        },
        Transform2D::from_xy(320.0, 180.0),
        Interpolated::default(),
    ));
}

fn spawn_level(mut commands: Commands, level: Res<Level>) {
    commands.spawn((Name::new("level"), Tilemap::new(level.map.clone())));
}

/// Counts the level's coins as its objects spawn.
fn count_coins(mut score: ResMut<Score>, objects: Query<&TiledObject, Added<TiledObject>>) {
    let new = objects.iter().filter(|o| o.class == "Coin").count();
    if new > 0 {
        score.total += new as u32;
    }
}

/// What the scripts tell the game: point the camera at me, a coin was taken.
fn read_messages(
    mut commands: Commands,
    mut messages: MessageReader<ScriptMessage>,
    mut score: ResMut<Score>,
    level: Res<Level>,
    maps: Res<Assets<TiledMap>>,
    camera: Single<Entity, With<Camera2D>>,
) {
    for message in messages.read() {
        match (message.name.as_str(), message.entity) {
            ("follow", Some(entity)) => {
                let size = maps
                    .get(&level.map)
                    .map_or(vec2(640.0, 360.0), TiledMap::size_px);
                commands.entity(*camera).insert(
                    CameraFollow::new(entity)
                        .with_dead_zone(vec2(40.0, 48.0))
                        .with_smoothing(0.12)
                        .with_offset(vec2(0.0, 30.0))
                        .with_bounds(Rect::from_min_size(Vec2::ZERO, size)),
                );
            }
            ("coin", _) => score.coins += 1,
            (other, _) => warn!("a script sent `{other}`, which the game does not know"),
        }
    }
}

fn spawn_hud(mut commands: Commands, mut hud: ResMut<Hud>) {
    let text = |commands: &mut Commands, at: Anchor, offset: Vec2, color: u32| {
        commands
            .spawn((
                Node::sized(600.0, 12.0).at(at).with_offset(offset),
                Text::new("").with_color(Color::hex(color)),
            ))
            .id()
    };
    hud.coins = Some(text(
        &mut commands,
        Anchor::TopLeft,
        vec2(8.0, 6.0),
        0x1d1f24,
    ));
    hud.scripts = Some(text(
        &mut commands,
        Anchor::TopLeft,
        vec2(8.0, 20.0),
        0x3e4450,
    ));
    hud.error = Some(text(
        &mut commands,
        Anchor::BottomLeft,
        vec2(8.0, -8.0),
        0x3e4450,
    ));
}

fn update_hud(
    score: Res<Score>,
    status: Res<ScriptStatus>,
    errors: Res<ScriptErrors>,
    hud: Res<Hud>,
    mut texts: Query<&mut Text>,
) {
    let mut set = |entity: Option<Entity>, value: String, color: Option<Color>| {
        if let Some(mut text) = entity.and_then(|e| texts.get_mut(e).ok()) {
            if text.value != value {
                text.value = value;
            }
            if let Some(color) = color
                && text.style.color != color
            {
                text.style.color = color;
            }
        }
    };
    set(
        hud.coins,
        format!("Coins {} / {}", score.coins, score.total),
        None,
    );
    let entities: usize = status.scripts.iter().map(|s| s.instances).sum();
    set(
        hud.scripts,
        format!(
            "Lua: {} files, {entities} entities, {} calls a step",
            status.scripts.len(),
            status.calls
        ),
        None,
    );
    let (line, color) = match errors.last() {
        Some(error) => (error.summary().to_owned(), Color::hex(0xc0392b)),
        None => (
            "Edit the files in assets/scripts while this runs. F12: the Scripts window.".to_owned(),
            Color::hex(0x3e4450),
        ),
    };
    set(hud.error, line, Some(color));
}

fn show_win(mut commands: Commands, score: Res<Score>) {
    info!("won with {} of {} coins", score.coins, score.total);
    commands.spawn((
        Node::sized(640.0, 24.0)
            .at(Anchor::Center)
            .with_offset(vec2(0.0, -40.0)),
        Text::new(format!("You win! {} of {} coins", score.coins, score.total))
            .with_size(22.0)
            .with_align(TextAlign::Center)
            .with_color(Color::hex(0x1d1f24)),
        StateScoped(GameState::Won),
    ));
}

fn quit(actions: Res<ActionState>, mut exit: MessageWriter<AppExit>) {
    if actions.just_pressed("quit") {
        exit.write(AppExit::Success);
    }
}

/// Ends the demo: with success a moment after the win, with an error if it takes too long
/// or a script failed.
fn direct_demo(
    time: Res<Time>,
    state: Res<State<GameState>>,
    errors: Res<ScriptErrors>,
    mut won_for: Local<f32>,
    mut exit: MessageWriter<AppExit>,
) {
    if let Some(error) = errors.last() {
        error!("the demo failed: {}", error.summary());
        exit.write(AppExit::error());
        return;
    }
    if *state.current() == GameState::Won {
        *won_for += time.fixed_delta_secs();
        if *won_for >= DEMO_WIN_WAIT {
            exit.write(AppExit::Success);
        }
    } else if time.fixed_step_count() > DEMO_STEPS {
        error!("the demo did not reach the goal in time");
        exit.write(AppExit::error());
    }
}

#[cfg(test)]
mod tests {
    // The capture test writes files on desktop only.
    #![allow(clippy::disallowed_methods)]

    use super::*;
    use cashgear_engine::audio::AudioOutput;
    use cashgear_engine::platform::input::InputPlugin;
    use cashgear_engine::render::{CameraView, FrameView, OffscreenRenderer, RenderSystems};

    /// The example without a window.
    fn demo_app() -> App {
        let mut app = App::new();
        app.add_plugins(AssetPlugin {
            hot_reload: Some(false),
            ..default()
        })
        .add_plugins(InputPlugin { gamepads: false })
        .add_plugins(TiledPlugin)
        .add_plugins(PhysicsPlugin::default())
        .add_plugins(AnimPlugin::default())
        .add_plugins(AudioPlugin {
            output: AudioOutput::Mock,
            settings_key: None,
            ..default()
        })
        .add_plugins(UiPlugin::default())
        .add_plugins(ScriptPlugin {
            show_errors: false,
            ..default()
        })
        .add_plugins(Game { demo: true });
        app.insert_resource(Window::with_size(UVec2::new(1280, 720), 1.0));
        app
    }

    #[test]
    fn the_lua_bot_reaches_the_goal() {
        let mut app = demo_app();
        let mut exit = None;
        for _ in 0..DEMO_STEPS + 120 {
            app.update(core::time::Duration::from_secs_f64(1.0 / 60.0));
            exit = app.should_exit();
            if exit.is_some() {
                break;
            }
        }
        let errors = app.world().resource::<ScriptErrors>();
        assert!(errors.is_empty(), "{:?}", errors.last());
        assert_eq!(exit, Some(AppExit::Success));
        assert_eq!(
            *app.world().resource::<State<GameState>>().current(),
            GameState::Won
        );
        let score = app.world().resource::<Score>();
        assert!(score.total >= 19 && score.coins > 0, "{score:?}");
        // Every script ran, and the spawner dropped crates.
        let status = app.world().resource::<ScriptStatus>();
        for path in SCRIPTS {
            assert!(
                status
                    .scripts
                    .iter()
                    .any(|s| s.path == path && s.loads == 1),
                "{path}: {status:?}"
            );
        }
        let world = app.world_mut();
        let crates = world
            .query::<&Name>()
            .iter(world)
            .filter(|n| n.as_str() == "crate")
            .count();
        assert!(crates > 0);
    }

    /// The queues of the frame being drawn, copied before the renderer empties them.
    #[derive(Resource, Default)]
    struct Snapshot {
        world: Vec<QueuedSprite>,
        screen: Vec<QueuedSprite>,
        view: CameraView,
    }

    fn snapshot(sprites: Res<Sprites>, view: Res<FrameView>, mut snapshot: ResMut<Snapshot>) {
        snapshot.world = sprites.queued().to_vec();
        snapshot.screen = sprites.screen_queued().to_vec();
        snapshot.view = view.0.clone();
    }

    /// Renders moments of the bot's run offscreen at 1280x720 and saves PNGs, for looking
    /// at the example without a window: `cargo test -p scripting -- --ignored
    /// capture_screens` writes them to `CG_CAPTURE_DIR` (default
    /// `target/scripting-captures`).
    #[test]
    #[ignore = "needs a GPU; writes PNGs for people to look at"]
    fn capture_screens() {
        let dir = std::env::var("CG_CAPTURE_DIR").unwrap_or_else(|_| {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../target/scripting-captures"
            )
            .into()
        });
        std::fs::create_dir_all(&dir).unwrap();
        let mut renderer = pollster::block_on(OffscreenRenderer::new()).expect("a GPU");
        let mut app = demo_app();
        app.init_resource::<Snapshot>().add_systems(
            Render,
            snapshot
                .after(RenderSystems::Queue)
                .before(RenderSystems::Draw),
        );
        let shots = [("start", 40), ("crates", 330), ("planks", 480)];
        let mut won = None;
        for frame in 0..DEMO_STEPS {
            app.update(core::time::Duration::from_secs_f64(1.0 / 60.0));
            let name = match shots.iter().find(|(_, at)| *at == frame) {
                Some((name, _)) => *name,
                None => {
                    let state = app.world().resource::<State<GameState>>().current().clone();
                    if state == GameState::Won && won.is_none() {
                        won = Some(frame);
                    }
                    if won.map(|at| frame - at) != Some(20) {
                        continue;
                    }
                    "won"
                }
            };
            let snapshot = app.world().resource::<Snapshot>();
            let mut sprites = Sprites::default();
            sprites.extend(snapshot.world.iter().copied());
            for sprite in &snapshot.screen {
                sprites.push_screen(*sprite);
            }
            let image = renderer
                .render(
                    UVec2::new(1280, 720),
                    &snapshot.view,
                    app.world().resource::<Assets<Image>>(),
                    &sprites,
                    &DebugShapes::default(),
                )
                .unwrap();
            std::fs::write(format!("{dir}/{name}.png"), image.to_png().unwrap()).unwrap();
            if name == "won" {
                return;
            }
        }
        panic!("the bot did not win");
    }
}
