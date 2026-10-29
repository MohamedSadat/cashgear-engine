//! `platformer`: the CashGear Engine sample game. Kenney's green alien runs, jumps,
//! wall-jumps and stomps through three levels built in Tiled, collecting coins, with
//! checkpoints, springs and moving platforms, three hearts, menus in English and Arabic,
//! rebindable controls and saved progress.
//!
//! - Desktop: `cargo run -p platformer`. Add `-- --demo` to let a bot play the whole game
//!   from the title screen to the end (it exits with an error if it gets stuck); CI runs
//!   it. `--level N` starts in level N (1 to 3) without the menus.
//! - Web: run `trunk serve --port 8088` in `games/platformer` and open
//!   <http://127.0.0.1:8088> (add `?demo`).
//!
//! | Action | Keyboard | Gamepad | Touch |
//! | --- | --- | --- | --- |
//! | Run | Arrows, A / D | D-pad, left stick | D-pad |
//! | Jump (hold for higher), wall jump | Space, Up | South (A / cross) | Jump button |
//! | Drop through planks | Down or S, then jump | D-pad down + south | D-pad down + jump |
//! | Pause | Escape, P | Start | Pause button |
//! | Menus | Arrows or WASD, Enter, Escape | D-pad, south, east | Tap |
//!
//! The game's keys and buttons can be changed in Settings, Controls; they are saved with
//! the volume levels, the language and the progress (unlocked levels, best coins and
//! times).
//!
//! The code is split by subject: [`level`] loads levels and turns map objects into game
//! objects, [`player`] is the controller, [`enemies`] the four enemies, [`things`] the
//! coins, checkpoints, springs, moving platforms and the exit, [`progress`] the hearts,
//! the run through a level and the save file, [`controls`] the rebindable action map,
//! [`menus`] and [`hud`] the screens, and [`bot`] the demo player.
//!
//! Art: Kenney's Pixel Platformer (CC0, www.kenney.nl); the UI art and the digits are
//! original. Sounds and music: original, synthesized for the engine's examples. Fonts:
//! Pixelify Sans (built in) and Noto Kufi Arabic, SIL Open Font License 1.1.

mod bot;
mod controls;
mod enemies;
mod hud;
mod level;
mod menus;
mod player;
mod progress;
mod things;

#[cfg(test)]
mod tests;

use cashgear_engine::prelude::*;

/// The game's screens, kept as a stack: menus open over the title, the pause and end
/// screens over the level.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Screen {
    Loading,
    Title,
    Levels,
    Settings,
    Controls,
    Playing,
    Paused,
    Complete,
    GameOver,
    Victory,
}

/// Gameplay steps in `FixedUpdate`, in this order, while a level is being played.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GameSystems {
    /// The controls or the bot fill the player's [`Intent`](player::Intent).
    Input,
    /// The player, the enemies and the platforms move.
    Move,
    /// Touches from the previous physics step: coins, enemies, hazards, the exit.
    Interact,
    /// Animation parameters and footsteps.
    Animate,
}

/// Options from the command line (or the page's query string).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub(crate) struct Options {
    /// The bot plays.
    pub demo: bool,
    /// Start in this level (0-based), skipping the menus.
    pub level: Option<usize>,
}

/// The game's assets, all loaded before the title screen.
#[derive(Resource)]
pub(crate) struct Art {
    pub ui: Handle<Image>,
    pub tiles: Handle<Image>,
    pub digits: Handle<Font>,
    pub alien: Handle<SpriteSheet>,
    pub graph: Handle<AnimationGraph>,
    /// The enemies' clips, cut from the character sheet in code.
    pub enemies: Handle<SpriteSheet>,
    pub maps: Vec<Handle<TiledMap>>,
    pub title_map: Handle<TiledMap>,
    pub sounds: Sounds,
    pub title_music: Handle<Sound>,
    pub music: Vec<Handle<Sound>>,
}

/// Sound effects.
pub(crate) struct Sounds {
    pub jump: Handle<Sound>,
    pub land: Handle<Sound>,
    pub step: Handle<Sound>,
    pub coin: Handle<Sound>,
    pub hit: Handle<Sound>,
    pub stomp: Handle<Sound>,
    pub spring: Handle<Sound>,
    pub checkpoint: Handle<Sound>,
    pub fall: Handle<Sound>,
    pub click: Handle<Sound>,
    pub goal: Handle<Sound>,
    pub game_over: Handle<Sound>,
}

fn main() {
    let args = Args::from_env();
    let options = Options {
        demo: args.flag("demo"),
        level: args
            .value("level")
            .and_then(|n| n.parse::<usize>().ok())
            .map(|n| n.clamp(1, level::LEVELS.len()) - 1),
    };
    App::new()
        .add_plugins(DefaultPlugins {
            window: WindowConfig {
                title: "CashGear Platformer".into(),
                canvas_id: Some("cashgear".into()),
                ..default()
            },
            // The demo must not change the player's saves, so it keeps them in memory.
            storage: StoragePlugin {
                namespace: Some("cashgear-platformer".into()),
                memory: options.demo,
            },
            ui: UiPlugin {
                locale: Some(LocaleSettings::new(&["en", "ar"])),
                ..default()
            },
            ..default()
        })
        .add_plugins(Game { options })
        .run();
}

/// Everything but the engine's plugins, so the tests can build the game without a window.
pub(crate) struct Game {
    pub options: Options,
}

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        use Screen::Loading;
        app.insert_resource(self.options)
            .load_prefab("Dust", "prefabs/dust.prefab.ron")
            .init_state(Loading)
            .add_loading_state(Loading, Screen::Title)
            .add_state_fade::<Screen>()
            .configure_sets(
                FixedUpdate,
                (
                    GameSystems::Input,
                    GameSystems::Move,
                    GameSystems::Interact,
                    GameSystems::Animate,
                )
                    .chain()
                    .run_if(in_state(Screen::Playing)),
            )
            .add_systems(Startup, setup)
            .add_systems(OnEnter(Loading), menus::spawn_loading)
            .add_systems(Update, menus::show_loading.run_if(in_state(Loading)));
        controls::plugin(app);
        progress::plugin(app);
        level::plugin(app);
        player::plugin(app);
        enemies::plugin(app);
        things::plugin(app);
        hud::plugin(app);
        menus::plugin(app);
        bot::plugin(app);
    }
}

/// The camera and the assets.
fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut loading: ResMut<LoadingAssets>,
    mut sheets: ResMut<Assets<SpriteSheet>>,
) {
    commands.spawn((
        Name::new("camera"),
        Camera2D {
            clear_color: Color::hex(level::LEVELS[0].sky),
            ..default()
        },
        Transform2D::from_xy(320.0, 180.0),
        Interpolated::default(),
        CameraShake::default(),
    ));
    let characters: Handle<Image> = server.load("images/characters.png");
    let sound = |name: &str| server.load::<Sound>(&format!("sounds/{name}.ogg"));
    let art = Art {
        ui: server.load("images/ui.png"),
        tiles: server.load("images/tiles.png"),
        digits: server.load("fonts/digits.fnt"),
        alien: server.load("images/alien.json"),
        graph: server.load("anims/alien.anim.ron"),
        enemies: sheets.add(enemies::sheet(characters.clone())),
        maps: level::LEVELS.iter().map(|l| server.load(l.map)).collect(),
        title_map: server.load("maps/title.tmx"),
        sounds: Sounds {
            jump: sound("jump"),
            land: sound("land"),
            step: sound("step"),
            coin: sound("coin"),
            hit: sound("hit"),
            stomp: sound("stomp"),
            spring: sound("spring"),
            checkpoint: sound("checkpoint"),
            fall: sound("fall"),
            click: sound("click"),
            goal: sound("goal"),
            game_over: sound("game_over"),
        },
        title_music: server.load("music/heights.ogg"),
        music: level::LEVELS.iter().map(|l| server.load(l.music)).collect(),
    };
    loading.add(&characters);
    loading.add(&art.ui);
    loading.add(&art.tiles);
    loading.add(&art.digits);
    loading.add(&art.alien);
    loading.add(&art.graph);
    loading.add(&art.title_map);
    for map in &art.maps {
        loading.add(map);
    }
    let s = &art.sounds;
    for sound in [
        &s.jump,
        &s.land,
        &s.step,
        &s.coin,
        &s.hit,
        &s.stomp,
        &s.spring,
        &s.checkpoint,
        &s.fall,
        &s.click,
        &s.goal,
        &s.game_over,
    ] {
        loading.add(sound);
    }
    // Music streams, so it is not waited for.
    commands.insert_resource(art);
}

/// Moves `from` towards `to` by at most `step`.
pub(crate) fn move_towards(from: f32, to: f32, step: f32) -> f32 {
    if (to - from).abs() <= step {
        to
    } else {
        from + step * (to - from).signum()
    }
}

/// The source rectangle of a tile of Kenney's 18-pixel tile sheet (20 columns).
pub(crate) fn tile_rect(id: u32) -> Rect {
    let (x, y) = ((id % 20) as f32 * 18.0, (id / 20) as f32 * 18.0);
    Rect::new(x, y, x + 18.0, y + 18.0)
}
