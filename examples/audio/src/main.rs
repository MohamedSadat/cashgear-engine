//! `audio`: Kenney's green alien runs a small Tiled level with sound: footsteps, jumps and
//! landings, coins that chime higher in a run, ambient sounds that pan as you pass, music
//! that crossfades between two areas, and a volume panel that remembers its settings.
//!
//! - Desktop: `cargo run -p audio`. Add `-- --demo` to let a bot play: it runs the level,
//!   then turns the music down, mutes and unmutes, and exits (with an error if it gets
//!   stuck). CI runs it. `--no-audio` runs without sound.
//! - Web: run `trunk serve --port 8086` in `examples/audio` and open
//!   <http://127.0.0.1:8086>. Browsers start sound only after a key press, click or touch;
//!   the speaker light at the top right blinks yellow until then.
//!
//! What it shows:
//!
//! - **Sounds as assets**: `sounds/*.ogg` are short effects, decoded when they load;
//!   `music/*.ogg` are 30- and 42-second loops, streamed while they play.
//! - **Effects with variation**: footsteps from the run clip's animation events, with
//!   random pitch; a landing as loud as the fall was long; coins collected in quick
//!   succession rise a tone each.
//! - **Positional sound**: coins sound from where they are, and the campfire and the
//!   fountain (Tiled objects of class `Ambient`, with `sound` and `volume` properties)
//!   loop as `AudioEmitter`s that pan and fade with their distance from the camera.
//! - **Music**: Tiled sensor objects of class `MusicZone` name a track; walking into one
//!   crossfades to it. The meadow has a busy tune, the planks and the tower a calm one.
//! - **Buses and settings**: master, music, effects and interface levels, and mute, drawn
//!   as bars at the top left. They are saved through `Storage` and come back next run.
//!
//! | Action | Keyboard | Gamepad | Touch |
//! | --- | --- | --- | --- |
//! | Run | Arrows, A / D | D-pad, left stick | Left fifth / second fifth of the screen |
//! | Jump (hold for higher) | Space, W, Up | South (A / cross) | Right two fifths |
//! | Pick master, music, effects or interface | 1, 2, 3, 4 | | |
//! | Volume down / up | - / = | West / North | |
//! | Mute | M | East | |
//! | Colliders | F1 | | |
//! | Back to the start | R | | |
//! | Quit | Escape | Select | |
//!
//! Art: Kenney's Pixel Platformer (CC0, www.kenney.nl); see `assets/KENNEY-LICENSE.txt`.
//! Sounds and music: original, synthesized for this example by a script, under the
//! engine's license.

use cashgear_engine::anim::SheetFrame;
use cashgear_engine::prelude::*;

const MAP: &str = "maps/level.tmx";
const SHEET: &str = "images/alien.json";
const GRAPH: &str = "anims/alien.anim.ron";
/// The sky colour of the background, so the view above the hills matches.
const SKY: u32 = 0xdff6f5;
const PLAYER_SIZE: Vec2 = Vec2::new(12.0, 18.0);
/// Draw layer of the alien: above the map's three layers and the coins.
const PLAYER_LAYER: i32 = 10;
/// Units per second, and per second squared to get there.
const RUN_SPEED: f32 = 120.0;
const RUN_ACCEL: f32 = 900.0;
const JUMP_SPEED: f32 = 330.0;
/// Rising speed is multiplied by this in each step the jump button is up.
const JUMP_CUT: f32 = 0.5;
const FALL_SPEED: f32 = 480.0;
/// Spikes throw the alien back and up at this speed.
const KNOCKBACK: Vec2 = Vec2::new(150.0, 240.0);
/// Seconds without control after a hit, and without further hits.
const STUNNED: f32 = 0.35;
const INVINCIBLE: f32 = 1.5;
/// Coins taken within this many seconds of each other count as a run.
const COIN_RUN: f32 = 0.8;
/// The demo gives up after this many fixed steps (a minute and a half).
const DEMO_STEPS: u64 = 60 * 90;
/// Volume steps of the panel.
const VOLUME_STEP: f32 = 0.1;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum GameState {
    Loading,
    Playing,
}

/// What the player wants in this fixed step, from the controls or from the bot.
#[derive(Resource, Debug, Clone, Copy, Default)]
struct Intent {
    /// -1 to 1.
    run: f32,
    jump: bool,
    jump_held: bool,
}

/// The alien's body. Its picture and dust are children, so squashing the picture leaves
/// the collider alone.
#[derive(Component, Debug)]
struct Player {
    facing: f32,
    /// The entity with the `SpriteAnimation` and `Animator`.
    sprite: Entity,
    /// The dust emitter at the feet.
    dust: Entity,
    /// The fastest the alien fell since it last stood on something.
    fall_speed: f32,
    /// Seconds left without control.
    stunned: f32,
    /// Seconds left without being hurt.
    invincible: f32,
}

#[derive(Component)]
struct Coin;

#[derive(Component)]
struct Goal;

/// A sensor area in the map telling the bot to do something.
#[derive(Component)]
struct BotHint(String);

/// A sensor area in the map with its own music.
#[derive(Component)]
struct MusicZone {
    name: String,
    track: Handle<Sound>,
}

/// The sound effects.
#[derive(Resource, Default)]
struct Sounds {
    jump: Handle<Sound>,
    land: Handle<Sound>,
    step: Handle<Sound>,
    coin: Handle<Sound>,
    hit: Handle<Sound>,
    click: Handle<Sound>,
    goal: Handle<Sound>,
}

/// The assets, and what the map says about the level.
#[derive(Resource, Default)]
struct Level {
    map: Handle<TiledMap>,
    sheet: Handle<SpriteSheet>,
    graph: Handle<AnimationGraph>,
    /// Two coin frames from the tileset, built in code once the map has loaded.
    coins: Handle<SpriteSheet>,
    start: Option<Vec2>,
    bounds: Rect,
}

/// What was heard, for the log, the demo and the tests.
#[derive(Resource, Debug, Default)]
struct Stats {
    coins: u32,
    coins_total: u32,
    footsteps: u32,
    jumps: u32,
    landings: u32,
    hits: u32,
    /// The longest run of coins, each a tone higher.
    best_coin_run: u32,
    /// Music zones entered, in order.
    zones: Vec<String>,
    ambient: u32,
    clicks: u32,
    finished: Option<f32>,
}

/// The run of coins being collected: how many, and when the last one was.
#[derive(Resource, Default)]
struct CoinRun {
    count: u32,
    last: f32,
}

/// The demo's bot: what it was told by the last hint, and how long it keeps holding jump.
#[derive(Resource, Default)]
struct Bot {
    enabled: bool,
    pending: Option<(String, u64)>,
    hold: u32,
}

/// The volume panel: the bus the keys change (0 is the master), and its UI nodes.
#[derive(Resource, Default)]
struct Panel {
    selected: usize,
    /// Per row: the marker, the icon and ten segments.
    rows: Vec<(Entity, Entity, Vec<Entity>)>,
    /// The speaker light: green while sound runs, blinking yellow while it waits.
    light: Option<Entity>,
}

/// The panel's rows: master, then the buses, each with its colour and label.
const ROWS: [(Option<Bus>, u32, &str); 4] = [
    (None, 0xf1faee, "Master"),
    (Some(Bus::Music), 0xc77dff, "Music"),
    (Some(Bus::Sfx), 0xffb703, "Effects"),
    (Some(Bus::Ui), 0x4cc9f0, "Menus"),
];

fn main() {
    let demo = Args::from_env().flag("demo");
    let mut app = App::new();
    app.add_plugins(DefaultPlugins {
        window: WindowConfig {
            title: "CashGear: audio".into(),
            canvas_id: Some("cashgear".into()),
            ..default()
        },
        storage: StoragePlugin {
            namespace: Some("cashgear-audio-example".into()),
            ..default()
        },
        // The demo changes the levels; it must not overwrite the player's settings.
        audio: AudioPlugin {
            settings_key: (!demo).then(|| "audio-settings.ron".into()),
            ..default()
        },
        ..default()
    })
    .add_plugins(Game { demo });
    app.run();
}

/// Everything but the engine's plugins, so the tests can build it without a window.
struct Game {
    /// Let the bot play, and exit when the demo is over.
    demo: bool,
}

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        use GameState::{Loading, Playing};
        app.insert_resource(action_map())
            .init_resource::<Intent>()
            .init_resource::<Level>()
            .init_resource::<Sounds>()
            .init_resource::<Stats>()
            .init_resource::<CoinRun>()
            .init_resource::<Panel>()
            .insert_resource(Bot {
                enabled: self.demo,
                ..default()
            })
            .load_prefab("Dust", "prefabs/dust.prefab.ron")
            .init_state(Loading)
            .add_loading_state(Loading, Playing)
            .add_state_fade::<GameState>()
            .add_systems(Startup, (setup, spawn_panel).chain())
            .add_systems(OnEnter(Playing), enter_playing)
            .add_systems(PreUpdate, tag_objects)
            .add_systems(
                FixedUpdate,
                (
                    (read_controls.run_if(bot_off), drive_bot.run_if(bot_on)),
                    control_player,
                    (collect, hurt, change_music),
                    (feed_animator, footsteps),
                )
                    .chain()
                    .run_if(in_state(Playing)),
            )
            .add_systems(FixedUpdate, direct_demo.run_if(bot_on).run_if(not_loading))
            .add_systems(Update, (mixer_keys, update_panel, keys, pop_coins, report));
    }
}

fn bot_on(bot: Res<Bot>) -> bool {
    bot.enabled
}

fn bot_off(bot: Res<Bot>) -> bool {
    !bot.enabled
}

fn not_loading(state: Res<State<GameState>>) -> bool {
    *state.current() != GameState::Loading
}

fn action_map() -> ActionMap {
    use AxisDirection::{Negative, Positive};
    ActionMap::new()
        .bind("left", KeyCode::ArrowLeft)
        .bind("left", KeyCode::KeyA)
        .bind("left", GamepadButton::DPadLeft)
        .bind("left", Binding::Axis(GamepadAxis::LeftStickX, Negative))
        .bind("left", ScreenRegion::new(0.0, 0.0, 0.2, 1.0))
        .bind("right", KeyCode::ArrowRight)
        .bind("right", KeyCode::KeyD)
        .bind("right", GamepadButton::DPadRight)
        .bind("right", Binding::Axis(GamepadAxis::LeftStickX, Positive))
        .bind("right", ScreenRegion::new(0.2, 0.0, 0.4, 1.0))
        .bind("jump", KeyCode::Space)
        .bind("jump", KeyCode::KeyW)
        .bind("jump", KeyCode::ArrowUp)
        .bind("jump", GamepadButton::South)
        .bind("jump", ScreenRegion::new(0.6, 0.0, 1.0, 1.0))
        .bind("master", KeyCode::Digit1)
        .bind("music", KeyCode::Digit2)
        .bind("sfx", KeyCode::Digit3)
        .bind("ui", KeyCode::Digit4)
        .bind("quieter", KeyCode::Minus)
        .bind("quieter", KeyCode::NumpadSubtract)
        .bind("quieter", GamepadButton::West)
        .bind("louder", KeyCode::Equal)
        .bind("louder", KeyCode::NumpadAdd)
        .bind("louder", GamepadButton::North)
        .bind("mute", KeyCode::KeyM)
        .bind("mute", GamepadButton::East)
        .bind("reset", KeyCode::KeyR)
        .bind("quit", KeyCode::Escape)
        .bind("quit", GamepadButton::Select)
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut loading: ResMut<LoadingAssets>,
    mut level: ResMut<Level>,
    mut sounds: ResMut<Sounds>,
    mut fade: ResMut<ScreenFade>,
) {
    level.map = server.load(MAP);
    level.sheet = server.load(SHEET);
    level.graph = server.load(GRAPH);
    loading.add(&level.map);
    loading.add(&level.sheet);
    loading.add(&level.graph);
    *sounds = Sounds {
        jump: server.load("sounds/jump.ogg"),
        land: server.load("sounds/land.ogg"),
        step: server.load("sounds/step.ogg"),
        coin: server.load("sounds/coin.ogg"),
        hit: server.load("sounds/hit.ogg"),
        click: server.load("sounds/click.ogg"),
        goal: server.load("sounds/goal.ogg"),
    };
    for sound in [
        &sounds.jump,
        &sounds.land,
        &sounds.step,
        &sounds.coin,
        &sounds.hit,
        &sounds.click,
        &sounds.goal,
    ] {
        loading.add(sound);
    }
    commands.spawn((
        Name::new("camera"),
        Camera2D {
            clear_color: Color::hex(SKY),
            ..default()
        },
        Transform2D::from_xy(320.0, 180.0),
        Interpolated::default(),
        CameraShake::default(),
        // Sounds are heard from the camera anyway; this says so.
        AudioListener,
    ));
    // Start covered; the level fades in once it has loaded.
    fade.set(1.0);
}

/// The volume panel, in the UI's screen space at the top left (so camera shake does not
/// move it): a row per bus with a marker, an icon, a label and ten segments, and the
/// speaker light at the top right.
fn spawn_panel(mut commands: Commands, mut panel: ResMut<Panel>) {
    let root = commands
        .spawn((
            Name::new("volume panel"),
            Node {
                offset: vec2(6.0, 6.0),
                padding: Edges::all(3.0),
                layout: Layout::Column,
                gap: 2.0,
                ..default()
            },
            BackgroundColor(Color::hex(0x1d1f24).with_alpha(0.55)),
        ))
        .id();
    let square = |commands: &mut Commands, parent: Entity, width: f32| {
        commands
            .spawn((
                Node::sized(width, 6.0),
                BackgroundColor(Color::WHITE),
                ChildOf(parent),
            ))
            .id()
    };
    for (_, hex, label) in ROWS {
        let row = commands
            .spawn((
                Node {
                    layout: Layout::Row,
                    gap: 3.0,
                    align: Align::Center,
                    ..default()
                },
                ChildOf(root),
            ))
            .id();
        let marker = square(&mut commands, row, 2.0);
        let icon = square(&mut commands, row, 6.0);
        commands.spawn((
            Node::sized(38.0, 12.0),
            Text::new(label).with_color(Color::hex(hex)),
            ChildOf(row),
        ));
        let bar = commands
            .spawn((
                Node {
                    layout: Layout::Row,
                    gap: 1.0,
                    ..default()
                },
                ChildOf(row),
            ))
            .id();
        let segments = (0..10).map(|_| square(&mut commands, bar, 4.0)).collect();
        panel.rows.push((marker, icon, segments));
    }
    panel.light = Some(
        commands
            .spawn((
                Name::new("speaker light"),
                Node::sized(6.0, 6.0)
                    .at(Anchor::TopRight)
                    .with_offset(vec2(-10.0, 10.0)),
                BackgroundColor(Color::WHITE),
            ))
            .id(),
    );
}

fn enter_playing(
    mut commands: Commands,
    mut level: ResMut<Level>,
    maps: Res<Assets<TiledMap>>,
    mut sheets: ResMut<Assets<SpriteSheet>>,
    mut fade: ResMut<ScreenFade>,
    bot: Res<Bot>,
) {
    let Some(map) = maps.get(&level.map) else {
        return;
    };
    level.bounds = Rect::from_min_size(Vec2::ZERO, map.size_px());
    if let Some(tiles) = map.tilesets.iter().find(|t| t.name == "tiles")
        && let Some(image) = &tiles.image
    {
        let mut sheet = SpriteSheet::new(image.clone());
        sheet.add_frame("front", SheetFrame::new(tiles.tile_rect(151), 0.18));
        sheet.add_frame("side", SheetFrame::new(tiles.tile_rect(152), 0.12));
        if let Ok(spin) = sheet.clip_from_frames(&["front", "side"], 6.0) {
            sheet.add_clip("spin", spin);
        }
        level.coins = sheets.add(sheet);
    }
    commands.spawn((Name::new("level"), Tilemap::new(level.map.clone())));
    fade.fade_in(0.6);
    if bot.enabled {
        info!("demo: the bot plays the level");
    } else {
        info!(
            "run with A/D or the arrows and jump with Space; 1-4 pick master, music, effects \
             or interface, -/= change it, M mutes; R restarts"
        );
    }
}

/// Gives map objects their game components by class, and spawns the alien at the start.
/// When the map is reloaded its objects spawn again, and the run starts over.
#[allow(clippy::too_many_arguments)]
fn tag_objects(
    mut commands: Commands,
    server: Res<AssetServer>,
    objects: Query<(Entity, &TiledObject, &Transform2D), Added<TiledObject>>,
    players: Query<Entity, With<Player>>,
    camera: Single<Entity, With<Camera2D>>,
    mut level: ResMut<Level>,
    mut stats: ResMut<Stats>,
) {
    if objects
        .iter()
        .any(|(_, object, _)| object.class == "PlayerStart")
    {
        stats.coins = 0;
        stats.coins_total = 0;
        stats.finished = None;
        stats.ambient = 0;
    }
    for (entity, object, transform) in &objects {
        match object.class.as_str() {
            "Coin" => {
                stats.coins_total += 1;
                let base = transform.translation;
                let speed = 0.8 + (base.x / 18.0) % 5.0 * 0.1;
                commands
                    .entity(entity)
                    .insert((
                        Coin,
                        SpriteAnimation::new(level.coins.clone(), "spin").with_speed(speed),
                    ))
                    .tween(
                        Tween::translation(base + vec2(0.0, 3.0), 0.7)
                            .from(base)
                            .ease(Ease::SineInOut)
                            .delay((base.x / 18.0) % 4.0 * 0.15)
                            .yoyo()
                            .forever(),
                    );
            }
            "Goal" => {
                commands.entity(entity).insert(Goal);
            }
            "BotHint" => {
                let action = object.properties.str("action").unwrap_or_default();
                commands.entity(entity).insert(BotHint(action.to_owned()));
            }
            "Ambient" => {
                let Some(path) = object.properties.str("sound") else {
                    warn!("ambient object `{}` has no `sound`", object.name);
                    continue;
                };
                let volume = object.properties.float("volume").unwrap_or(1.0);
                stats.ambient += 1;
                commands.entity(entity).insert((
                    AudioEmitter::new(server.load(path)).with_volume(volume),
                    ParticleEmitter::new(ambient_particles(&object.name)),
                    Layer(PLAYER_LAYER - 1),
                ));
            }
            "MusicZone" => {
                let Some(path) = object.properties.str("music") else {
                    warn!("music zone `{}` has no `music`", object.name);
                    continue;
                };
                commands.entity(entity).insert(MusicZone {
                    name: object.name.clone(),
                    track: server.load(path),
                });
            }
            "PlayerStart" => {
                // The point is on the ground; the body stands on it.
                let start = transform.translation + vec2(0.0, PLAYER_SIZE.y / 2.0);
                level.start = Some(start);
                for player in &players {
                    commands.entity(player).despawn();
                }
                let body = spawn_player(&mut commands, &level, start);
                commands.entity(*camera).insert(
                    CameraFollow::new(body)
                        .with_dead_zone(vec2(48.0, 72.0))
                        .with_smoothing(0.15)
                        .with_offset(vec2(0.0, 24.0))
                        .with_bounds(level.bounds),
                );
            }
            _ => {}
        }
    }
}

/// Something to see where an ambient sound comes from.
fn ambient_particles(name: &str) -> ParticleSettings {
    if name == "fountain" {
        ParticleSettings {
            rate: 140.0,
            lifetime: (0.9, 1.2),
            speed: (120.0, 150.0),
            spread: 16.0,
            gravity: vec2(0.0, -300.0),
            size: vec![2.0, 2.0, 1.0],
            color: vec![Color::hex(0xbde8ff), Color::hex(0x3a86ff).with_alpha(0.0)],
            ..default()
        }
    } else {
        ParticleSettings {
            rate: 90.0,
            lifetime: (0.4, 0.8),
            speed: (14.0, 40.0),
            spread: 40.0,
            gravity: vec2(0.0, 60.0),
            size: vec![4.0, 2.0, 1.0],
            color: vec![
                Color::hex(0xfff3a0),
                Color::hex(0xff9f1c),
                Color::hex(0xe63946).with_alpha(0.6),
                Color::hex(0x6d6875).with_alpha(0.0),
            ],
            shape: EmitShape::Rect(vec2(12.0, 2.0)),
            ..default()
        }
    }
}

fn spawn_player(commands: &mut Commands, level: &Level, at: Vec2) -> Entity {
    let feet = vec2(0.0, -PLAYER_SIZE.y / 2.0);
    let body = commands
        .spawn((
            Name::new("alien"),
            KinematicBody {
                max_fall_speed: FALL_SPEED,
                ..default()
            },
            Collider::rect(PLAYER_SIZE.x, PLAYER_SIZE.y),
            Transform2D::from_translation(at),
        ))
        .id();
    let sprite = commands
        .spawn((
            Name::new("alien picture"),
            SpriteAnimation::new(level.sheet.clone(), "idle").with_pivot(FEET),
            Animator::new(level.graph.clone()),
            Layer(PLAYER_LAYER),
            Transform2D::from_translation(feet),
            ChildOf(body),
        ))
        .id();
    let dust = commands
        .spawn_prefab("Dust")
        .insert((Transform2D::from_translation(feet), ChildOf(body)))
        .id();
    commands.entity(body).insert(Player {
        facing: 1.0,
        sprite,
        dust,
        fall_speed: 0.0,
        stunned: 0.0,
        invincible: 0.0,
    });
    body
}

fn read_controls(actions: Res<ActionState>, mut intent: ResMut<Intent>) {
    *intent = Intent {
        run: actions.axis("left", "right"),
        jump: actions.just_pressed("jump"),
        jump_held: actions.pressed("jump"),
    };
}

/// The demo's player: runs right, jumps at walls it finds with a raycast, and follows the
/// hints placed in the map onto the planks. It stands still once it reaches the goal.
#[allow(clippy::too_many_arguments)]
fn drive_bot(
    time: Res<Time>,
    stats: Res<Stats>,
    mut bot: ResMut<Bot>,
    mut intent: ResMut<Intent>,
    mut started: MessageReader<CollisionStarted>,
    hints: Query<&BotHint>,
    player: Single<(Entity, &Collider, &GlobalTransform2D, &KinematicContacts), With<Player>>,
    spatial: SpatialQuery,
) {
    let (entity, collider, transform, contacts) = *player;
    let step = time.fixed_step_count();
    for collision in started.read() {
        if let Some(hint) = collision
            .other(entity)
            .and_then(|other| hints.get(other).ok())
        {
            bot.pending = Some((hint.0.clone(), step));
        }
    }
    // Hints left unused for half a second are forgotten.
    if bot.pending.as_ref().is_some_and(|(_, at)| step > at + 30) {
        bot.pending = None;
    }
    if stats.finished.is_some() {
        *intent = Intent::default();
        return;
    }

    *intent = Intent {
        run: 1.0,
        jump_held: bot.hold > 0,
        ..default()
    };
    bot.hold = bot.hold.saturating_sub(1);
    let grounded = contacts.on_ground();
    let mut jump = grounded && bot.pending.as_ref().is_some_and(|(a, _)| a == "jump");
    if grounded && !jump {
        let rect = collider.world_shape(transform.affine()).aabb();
        let solid = QueryFilter::default().exclude(entity).without_sensors();
        // A wall right ahead: a hit facing back at us. Slopes face up, so they do not count.
        jump = spatial
            .raycast(
                vec2(rect.max.x - 1.0, rect.min.y + 4.0),
                Vec2::X,
                6.0,
                solid,
            )
            .is_some_and(|hit| hit.normal.x < -0.9);
    }
    if jump {
        bot.pending = None;
        intent.jump = true;
        intent.jump_held = true;
        bot.hold = 20;
    }
}

/// Turns the intent into velocity, with take-off and landing effects and sounds.
#[allow(clippy::too_many_arguments)]
fn control_player(
    mut commands: Commands,
    time: Res<Time>,
    intent: Res<Intent>,
    sounds: Res<Sounds>,
    mut audio: ResMut<Audio>,
    mut stats: ResMut<Stats>,
    player: Single<(&mut Player, &mut KinematicBody, &KinematicContacts)>,
    mut sprites: Query<&mut Sprite>,
    mut emitters: Query<&mut ParticleEmitter>,
    mut shake: Single<&mut CameraShake>,
) {
    let (mut player, mut body, contacts) = player.into_inner();
    let dt = time.fixed_delta_secs();
    player.stunned = (player.stunned - dt).max(0.0);
    player.invincible = (player.invincible - dt).max(0.0);

    if contacts.just_landed() {
        // Squash, dust and a thud in proportion to the fall; shake only for long ones.
        let speed = player.fall_speed;
        let strength = (speed / FALL_SPEED).clamp(0.3, 1.0);
        commands.entity(player.sprite).tween(
            Tween::scale(Vec2::ONE, 0.3)
                .from(vec2(1.0 + 0.35 * strength, 1.0 - 0.3 * strength))
                .ease(Ease::BackOut),
        );
        if let Ok(mut dust) = emitters.get_mut(player.dust) {
            dust.emit(4 + (speed / 40.0) as u32);
        }
        if speed > 300.0 {
            shake.add_trauma(((speed - 300.0) / 300.0).min(0.7));
        }
        audio.play_with(
            &sounds.land,
            PlaySettings::new()
                .with_volume(0.35 + 0.65 * strength)
                .with_pitch_jitter(1.0),
        );
        stats.landings += 1;
    }
    if contacts.on_ground() {
        player.fall_speed = 0.0;
    } else {
        player.fall_speed = player.fall_speed.max(-body.velocity.y);
    }

    if player.stunned > 0.0 {
        return;
    }
    let target = intent.run * RUN_SPEED;
    let accel = if contacts.on_ground() {
        RUN_ACCEL
    } else {
        RUN_ACCEL * 0.6
    };
    body.velocity.x = move_towards(body.velocity.x, target, accel * dt);
    if intent.run != 0.0 && intent.run.signum() != player.facing {
        player.facing = intent.run.signum();
        if let Ok(mut sprite) = sprites.get_mut(player.sprite) {
            sprite.flip_x = player.facing < 0.0;
        }
    }
    if contacts.on_ground() && intent.jump {
        body.velocity.y = JUMP_SPEED;
        stats.jumps += 1;
        commands.entity(player.sprite).tween(
            Tween::scale(Vec2::ONE, 0.25)
                .from(vec2(0.8, 1.25))
                .ease(Ease::QuadOut),
        );
        if let Ok(mut dust) = emitters.get_mut(player.dust) {
            dust.emit(5);
        }
        audio.play_with(
            &sounds.jump,
            PlaySettings::new().with_volume(0.45).with_pitch_jitter(0.5),
        );
    }
    if body.velocity.y > 0.0 && !intent.jump_held {
        body.velocity.y *= JUMP_CUT;
    }
}

fn move_towards(from: f32, to: f32, step: f32) -> f32 {
    if (to - from).abs() <= step {
        to
    } else {
        from + step * (to - from).signum()
    }
}

/// Spikes throw the alien back with a flash, a shake and a crunch.
#[allow(clippy::too_many_arguments)]
fn hurt(
    mut commands: Commands,
    sounds: Res<Sounds>,
    mut audio: ResMut<Audio>,
    mut stats: ResMut<Stats>,
    player: Single<(&mut Player, &mut KinematicBody, &KinematicContacts)>,
    mut animators: Query<&mut Animator>,
    mut shake: Single<&mut CameraShake>,
) {
    let (mut player, mut body, contacts) = player.into_inner();
    if !contacts.hazard || player.invincible > 0.0 {
        return;
    }
    player.stunned = STUNNED;
    player.invincible = INVINCIBLE;
    body.velocity = vec2(-player.facing * KNOCKBACK.x, KNOCKBACK.y);
    stats.hits += 1;
    shake.add_trauma(0.6);
    if let Ok(mut animator) = animators.get_mut(player.sprite) {
        animator.trigger("hurt");
    }
    commands
        .entity(player.sprite)
        .tween(Tween::flash(Color::WHITE, 0.25))
        .tween(
            Tween::color(Color::WHITE.with_alpha(0.3), 0.075)
                .from(Color::WHITE)
                .yoyo()
                .repeat(2 * (INVINCIBLE / 0.15) as u32),
        );
    audio.play_with(&sounds.hit, PlaySettings::new().with_volume(0.7));
    info!("ouch, spikes");
}

/// Picks up coins with a pop and a chime from where the coin is (a tone higher for each
/// coin in a run), and notices the goal.
#[allow(clippy::too_many_arguments)]
fn collect(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Res<Sounds>,
    mut audio: ResMut<Audio>,
    mut started: MessageReader<CollisionStarted>,
    mut stats: ResMut<Stats>,
    mut run: ResMut<CoinRun>,
    player: Single<Entity, With<Player>>,
    coins: Query<&GlobalTransform2D, With<Coin>>,
    goals: Query<&GlobalTransform2D, With<Goal>>,
) {
    let now = time.fixed_elapsed().as_secs_f32();
    for collision in started.read() {
        let Some(other) = collision.other(*player) else {
            continue;
        };
        if let Ok(transform) = coins.get(other) {
            stats.coins += 1;
            run.count = if now - run.last < COIN_RUN {
                (run.count + 1).min(12)
            } else {
                0
            };
            run.last = now;
            stats.best_coin_run = stats.best_coin_run.max(run.count + 1);
            audio.play_with(
                &sounds.coin,
                PlaySettings::new()
                    .at(transform.translation())
                    .with_volume(0.5)
                    .with_speed(2f32.powf(run.count as f32 * 2.0 / 12.0)),
            );
            commands
                .entity(other)
                .remove::<(Coin, Collider)>()
                .tween(Tween::scale(vec2(1.8, 1.8), 0.25).ease(Ease::QuadOut))
                .tween(
                    Tween::color(Color::WHITE.with_alpha(0.0), 0.25)
                        .ease(Ease::QuadIn)
                        .tag("picked"),
                );
        } else if let Ok(goal) = goals.get(other)
            && stats.finished.is_none()
        {
            stats.finished = Some(now);
            audio.play_with(&sounds.goal, PlaySettings::new().at(goal.translation()));
            info!(
                "at the goal after {now:.1} s with {} of {} coins (best run {}), {} jumps and \
                 {} hits",
                stats.coins, stats.coins_total, stats.best_coin_run, stats.jumps, stats.hits
            );
        }
    }
}

/// Removes coins once their pop has played.
fn pop_coins(mut commands: Commands, mut finished: MessageReader<TweenFinished>) {
    for tween in finished.read() {
        if tween.tag.as_deref() == Some("picked") {
            commands.entity(tween.entity).despawn();
        }
    }
}

/// Crossfades to a zone's music when the alien walks into it.
fn change_music(
    mut started: MessageReader<CollisionStarted>,
    mut audio: ResMut<Audio>,
    mut stats: ResMut<Stats>,
    player: Single<Entity, With<Player>>,
    zones: Query<&MusicZone>,
) {
    for collision in started.read() {
        let Some(zone) = collision
            .other(*player)
            .and_then(|other| zones.get(other).ok())
        else {
            continue;
        };
        if audio.music() != Some(&zone.track) {
            info!("music: {}", zone.name);
            stats.zones.push(zone.name.clone());
        }
        audio.play_music(
            &zone.track,
            MusicSettings {
                volume: 0.6,
                crossfade: 1.5,
                ..default()
            },
        );
    }
}

/// Tells the state machine how the body moves.
fn feed_animator(
    player: Single<(&Player, &KinematicBody, &KinematicContacts)>,
    mut animators: Query<&mut Animator>,
) {
    let (player, body, contacts) = *player;
    if let Ok(mut animator) = animators.get_mut(player.sprite) {
        animator.set("speed", body.velocity.x.abs());
        animator.set("vy", body.velocity.y);
        animator.set_bool("grounded", contacts.on_ground());
    }
}

/// A step sound and a puff of dust at each footstep of the run clip.
fn footsteps(
    mut events: MessageReader<AnimationEvent>,
    sounds: Res<Sounds>,
    mut audio: ResMut<Audio>,
    mut stats: ResMut<Stats>,
    player: Single<&Player>,
    mut emitters: Query<&mut ParticleEmitter>,
) {
    for event in events.read() {
        if event.entity == player.sprite && event.name == "footstep" {
            stats.footsteps += 1;
            audio.play_with(
                &sounds.step,
                PlaySettings::new().with_volume(0.5).with_pitch_jitter(2.0),
            );
            if let Ok(mut dust) = emitters.get_mut(player.dust) {
                dust.emit(2);
            }
        }
    }
}

/// The volume panel's keys. Each change clicks on the interface bus, so it is heard at
/// that bus's level.
fn mixer_keys(
    actions: Res<ActionState>,
    sounds: Res<Sounds>,
    mut audio: ResMut<Audio>,
    mut settings: ResMut<AudioSettings>,
    mut panel: ResMut<Panel>,
    mut stats: ResMut<Stats>,
) {
    for (i, action) in ["master", "music", "sfx", "ui"].into_iter().enumerate() {
        if actions.just_pressed(action) {
            panel.selected = i;
        }
    }
    let step = if actions.just_pressed("louder") {
        VOLUME_STEP
    } else if actions.just_pressed("quieter") {
        -VOLUME_STEP
    } else {
        0.0
    };
    let mut changed = false;
    if step != 0.0 {
        match ROWS[panel.selected].0 {
            Some(bus) => settings.step_level(bus, step),
            None => settings.step_master(step),
        }
        changed = true;
    }
    if actions.just_pressed("mute") {
        settings.muted = !settings.muted;
        changed = true;
    }
    if changed {
        stats.clicks += 1;
        audio.play_with(&sounds.click, PlaySettings::new().with_bus(Bus::Ui));
        info!(
            "volume: master {:.1}, music {:.1}, effects {:.1}, interface {:.1}{}",
            settings.master,
            settings.music,
            settings.sfx,
            settings.ui,
            if settings.muted { ", muted" } else { "" }
        );
    }
}

/// Draws the levels, the selected row and the speaker light.
fn update_panel(
    time: Res<Time>,
    audio: Res<Audio>,
    settings: Res<AudioSettings>,
    panel: Res<Panel>,
    mut squares: Query<&mut BackgroundColor>,
) {
    for (row, (marker, icon, segments)) in panel.rows.iter().enumerate() {
        let (bus, hex, _) = ROWS[row];
        let level = bus.map_or(settings.master, |bus| settings.level(bus));
        let lit = (level * 10.0).round() as usize;
        let color = if settings.muted {
            Color::hex(hex).with_alpha(0.35)
        } else {
            Color::hex(hex)
        };
        if let Ok(mut square) = squares.get_mut(*marker) {
            square.0 = if row == panel.selected {
                Color::WHITE
            } else {
                Color::WHITE.with_alpha(0.0)
            };
        }
        if let Ok(mut square) = squares.get_mut(*icon) {
            square.0 = if settings.muted && bus.is_none() {
                Color::hex(0xe63946)
            } else {
                color
            };
        }
        for (i, segment) in segments.iter().enumerate() {
            if let Ok(mut square) = squares.get_mut(*segment) {
                square.0 = if i < lit {
                    color
                } else {
                    Color::hex(0x1d1f24).with_alpha(0.45)
                };
            }
        }
    }
    if let Some(light) = panel.light
        && let Ok(mut square) = squares.get_mut(light)
    {
        let blink = (time.raw_elapsed().as_secs_f32() * 3.0).fract() < 0.5;
        square.0 = match audio.status() {
            AudioStatus::Running if settings.muted => Color::hex(0xe63946),
            AudioStatus::Running => Color::hex(0x06d6a0),
            AudioStatus::Waiting if blink => Color::hex(0xffd166),
            AudioStatus::Waiting => Color::hex(0xffd166).with_alpha(0.2),
            AudioStatus::Paused | AudioStatus::Off => Color::hex(0x6d6875),
        };
    }
}

fn keys(
    actions: Res<ActionState>,
    level: Res<Level>,
    state: Res<State<GameState>>,
    camera: Query<&mut CameraFollow>,
    player: Query<(
        &mut Transform2D,
        &mut KinematicBody,
        &mut Interpolated,
        &Player,
    )>,
    mut animators: Query<&mut Animator>,
    mut exit: MessageWriter<AppExit>,
) {
    if actions.just_pressed("reset")
        && *state.current() == GameState::Playing
        && let Some(start) = level.start
    {
        for (mut transform, mut body, mut interpolated, player) in player {
            transform.translation = start;
            body.velocity = Vec2::ZERO;
            interpolated.reset();
            if let Ok(mut animator) = animators.get_mut(player.sprite) {
                animator.force("idle");
            }
        }
        for mut follow in camera {
            follow.snap();
        }
        info!("back to the start");
    }
    if actions.just_pressed("quit") {
        exit.write(AppExit::Success);
    }
}

/// The demo after the goal: turn the music down, mute, unmute and exit. Exits with an
/// error if the bot never reaches the goal.
#[allow(clippy::too_many_arguments)]
fn direct_demo(
    time: Res<Time>,
    stats: Res<Stats>,
    sounds: Res<Sounds>,
    mut audio: ResMut<Audio>,
    mut settings: ResMut<AudioSettings>,
    mut exit: MessageWriter<AppExit>,
    mut stage: Local<(u32, u64)>,
) {
    let step = time.fixed_step_count();
    let (current, since) = *stage;
    let next = |stage: &mut (u32, u64)| *stage = (current + 1, step);
    let click = |audio: &mut Audio| {
        audio.play_with(&sounds.click, PlaySettings::new().with_bus(Bus::Ui));
    };
    match current {
        0 if stats.finished.is_some() => next(&mut stage),
        0 if step > DEMO_STEPS => {
            error!("demo: the bot did not reach the goal ({:?})", *stats);
            exit.write(AppExit::error());
            *stage = (99, step);
        }
        1 if step > since + 120 => {
            info!("demo: music down to 0.3");
            settings.set_level(Bus::Music, 0.3);
            click(&mut audio);
            next(&mut stage);
        }
        2 if step > since + 90 => {
            info!("demo: muted");
            settings.muted = true;
            next(&mut stage);
        }
        3 if step > since + 60 => {
            info!("demo: unmuted");
            settings.muted = false;
            click(&mut audio);
            next(&mut stage);
        }
        4 if step > since + 60 => {
            info!("demo: done ({:?}; {:?})", *stats, audio.stats());
            exit.write(AppExit::Success);
            next(&mut stage);
        }
        _ => {}
    }
}

/// A status line every two seconds.
fn report(
    time: Res<Time>,
    render_stats: Res<RenderStats>,
    audio: Res<Audio>,
    stats: Res<Stats>,
    sounds: Res<Assets<Sound>>,
    mut last: Local<f64>,
) {
    let now = time.elapsed_secs();
    if now - *last < 2.0 {
        return;
    }
    *last = now;
    let audio_stats = audio.stats();
    let music = audio
        .music()
        .and_then(|track| sounds.get(track.id()))
        .and_then(Sound::duration)
        .map_or_else(|| "none".to_owned(), |secs| format!("a {secs:.0} s loop"));
    info!(
        "{:.0} fps; audio {:?}: {} voices and {} fading ({} streamed, {} underruns, {} cut \
         by the caps), music {music}; {} of {} coins, {} footsteps, {} landings",
        render_stats.fps(),
        audio.status(),
        audio_stats.voices,
        audio_stats.fading,
        audio_stats.streams,
        audio_stats.underruns,
        audio_stats.stolen,
        stats.coins,
        stats.coins_total,
        stats.footsteps,
        stats.landings,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The level without a window, with the bot playing and a mock audio device.
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
        .insert_resource(RenderStats::default())
        .add_plugins(Game { demo: true });
        // A window that is up and that the user has touched, so sound may start.
        app.insert_resource(Window::with_size(UVec2::new(1280, 720), 2.0));
        app
    }

    #[test]
    fn the_sounds_and_music_load() {
        let mut app = demo_app();
        for _ in 0..4 {
            app.update(core::time::Duration::from_millis(16));
        }
        let world = app.world();
        let sounds = world.resource::<Sounds>();
        let assets = world.resource::<Assets<Sound>>();
        for handle in [
            &sounds.jump,
            &sounds.land,
            &sounds.step,
            &sounds.coin,
            &sounds.hit,
        ] {
            let sound = assets.get(handle.id()).expect("an effect loaded");
            assert!(!sound.is_streamed());
            assert!(sound.duration().unwrap() < 0.5);
        }
        let mut zones = app.world_mut().query::<&MusicZone>();
        let tracks: Vec<_> = zones.iter(app.world()).map(|z| z.track.clone()).collect();
        assert_eq!(tracks.len(), 2);
        for _ in 0..4 {
            app.update(core::time::Duration::from_millis(16));
        }
        let assets = app.world().resource::<Assets<Sound>>();
        for track in &tracks {
            let music = assets.get(track.id()).expect("a track loaded");
            assert!(music.is_streamed(), "music is streamed");
            assert!(music.duration().unwrap() > 25.0);
            assert_eq!(music.channels(), 2);
        }
        assert_eq!(app.world().resource::<Stats>().ambient, 2);
        let panel = app.world().resource::<Panel>();
        assert_eq!(panel.rows.len(), ROWS.len(), "the volume panel was spawned");
        assert!(panel.light.is_some());
    }

    /// The bot plays the level in fixed steps, so the result does not depend on the frame
    /// rate: footsteps, jumps, landings, a hit, coins in runs, both music zones, the goal,
    /// and the mixer changes after it.
    #[test]
    fn the_bot_plays_the_demo_with_sound() {
        let mut app = demo_app();
        let step = app.world().resource::<Time>().fixed_delta();
        let mut exit = None;
        let mut most_voices = 0;
        for _ in 0..DEMO_STEPS + 600 {
            app.update(step);
            most_voices = most_voices.max(app.world().resource::<Audio>().stats().voices);
            if let Some(code) = app.should_exit() {
                exit = Some(code);
                break;
            }
        }
        let stats = app.world().resource::<Stats>();
        assert_eq!(exit, Some(AppExit::Success), "{stats:?}");
        assert!(stats.finished.is_some(), "{stats:?}");
        assert!(stats.footsteps >= 20, "{stats:?}");
        assert!(stats.landings >= 5, "{stats:?}");
        assert!(stats.hits >= 1, "{stats:?}");
        assert!(stats.coins >= 10, "{stats:?}");
        assert!(stats.best_coin_run >= 3, "{stats:?}");
        assert_eq!(stats.zones, ["meadow", "heights"], "{stats:?}");
        let audio = app.world().resource::<Audio>();
        assert_eq!(audio.status(), AudioStatus::Running);
        // The caps keep the count in check: 32 effects at most, and the music.
        assert!(most_voices <= 33, "{most_voices}");
        assert!(audio.stats().stolen > 0);
        assert_eq!(audio.stats().streams, 2, "both tracks, one fading out");
        let settings = app.world().resource::<AudioSettings>();
        assert_eq!((settings.music, settings.muted), (0.3, false));
    }
}
