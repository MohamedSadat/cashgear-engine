//! `animation`: Kenney's green alien runs a small Tiled level, animated by an Aseprite
//! sheet and a state machine, with dust, squash and stretch, hit flashes, spinning coins,
//! a camera that follows, shakes and zooms, and fades into a particle gallery.
//!
//! - Desktop: `cargo run -p animation`. Add `-- --demo` to let a bot play: it runs the
//!   level, zooms in at the goal, visits the particle gallery and exits (with an error if
//!   it gets stuck). CI runs it.
//! - Web: run `trunk serve --port 8085` in `examples/animation` and open
//!   <http://127.0.0.1:8085> (add `?demo`).
//!
//! What it shows:
//!
//! - **Aseprite sheets**: `images/alien.json` is a trimmed sheet in Aseprite's export
//!   format, with tags `idle` (with a blink), `run`, `jump`, `fall` and `land`, and
//!   `footstep` events in the run tag's user data.
//! - **A state machine**: `anims/alien.anim.ron` picks the clip from the `speed`, `vy` and
//!   `grounded` parameters and a `hurt` trigger; the run plays faster the faster the alien
//!   runs. Saving either file while the example runs reloads it.
//! - **Animation events**: each footstep kicks up dust from an emitter at the feet (the
//!   `Dust` prefab).
//! - **Tweens**: a stretch on take-off, a squash on landing (harder after longer falls), a
//!   slow breath while idle, a white flash and a blink when spikes hurt, bobbing and
//!   popping coins, and the camera zoom.
//! - **Particles**: dust, coin sparkles and pickup bursts; the gallery (P) shows a
//!   fountain, fire, snow, sparks and a stream of 10,000 particles.
//! - **The camera**: follows with a dead zone, never leaves the level, shakes on hard
//!   landings and hits, and zooms (Z). F1 draws the dead zone with the colliders.
//! - **Fades**: into the level at the start, and into and out of the gallery.
//! - **Tile animations**: the goal flag waves, animated in Tiled's tile animation editor
//!   (`tilesets/tiles.tsx`); coins spin through a clip built in code from two tiles.
//!
//! | Action | Keyboard | Gamepad | Touch |
//! | --- | --- | --- | --- |
//! | Run | Arrows, A / D | D-pad, left stick | Left fifth / second fifth of the screen |
//! | Jump (hold for higher) | Space, W, Up | South (A / cross) | Right two fifths |
//! | Zoom in and out | Z | East (B / circle) | |
//! | Particle gallery | P | North (Y / triangle) | Middle fifth |
//! | Colliders and dead zone | F1 | | |
//! | Back to the start | R | | |
//! | Quit | Escape | Select | |
//!
//! Art: Kenney's Pixel Platformer (CC0, www.kenney.nl); see `assets/KENNEY-LICENSE.txt`.
//! The alien's blink and landing frames are edits of Kenney's alien.

use cashgear_engine::anim::SheetFrame;
use cashgear_engine::prelude::*;
use cashgear_engine::render::FrameView;
use std::collections::BTreeSet;

const MAP: &str = "maps/playground.tmx";
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
/// Particles in the gallery's big stream.
const STREAM: u32 = 10_000;
/// The demo gives up after this many fixed steps (a minute and a half).
const DEMO_STEPS: u64 = 60 * 90;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum GameState {
    Loading,
    Playing,
    /// Pushed over `Playing`, which waits underneath.
    Gallery,
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

/// What happened, for the log, the demo and the tests.
#[derive(Resource, Debug, Default)]
struct Stats {
    coins: u32,
    coins_total: u32,
    footsteps: u32,
    jumps: u32,
    landings: u32,
    hardest_landing: f32,
    hits: u32,
    states: BTreeSet<String>,
    finished: Option<f32>,
    peak_trauma: f32,
    peak_particles: u32,
    gallery_visits: u32,
    /// Times the camera's view was seen outside the level.
    camera_outside: u32,
}

/// The demo's bot: what it was told by the last hint, and how long it keeps holding jump.
#[derive(Resource, Default)]
struct Bot {
    enabled: bool,
    pending: Option<(String, u64)>,
    hold: u32,
}

/// Whether the camera is zoomed in.
#[derive(Resource, Default)]
struct Zoom(bool);

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins {
        window: WindowConfig {
            title: "CashGear: animation".into(),
            canvas_id: Some("cashgear".into()),
            ..default()
        },
        ..default()
    })
    .add_plugins(Game {
        demo: Args::from_env().flag("demo"),
    });
    app.run();
}

/// Everything but the engine's plugins, so the tests can build it without a window.
struct Game {
    /// Let the bot play, and exit when the demo is over.
    demo: bool,
}

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        use GameState::{Gallery, Loading, Playing};
        app.insert_resource(action_map())
            .init_resource::<Intent>()
            .init_resource::<Level>()
            .init_resource::<Stats>()
            .init_resource::<Zoom>()
            .insert_resource(Bot {
                enabled: self.demo,
                ..default()
            })
            .load_prefab("Dust", "prefabs/dust.prefab.ron")
            .init_state(Loading)
            .add_loading_state(Loading, Playing)
            .add_state_fade::<GameState>()
            .add_systems(Startup, setup)
            .add_systems(OnEnter(Playing), enter_playing)
            .add_systems(OnEnter(Gallery), enter_gallery)
            .add_systems(PreUpdate, tag_objects)
            .add_systems(
                FixedUpdate,
                (
                    (read_controls.run_if(bot_off), drive_bot.run_if(bot_on)),
                    control_player,
                    (collect, hurt),
                    (feed_animator, footsteps, breathe),
                )
                    .chain()
                    .run_if(in_state(Playing)),
            )
            .add_systems(FixedUpdate, sparks.run_if(in_state(Gallery)))
            .add_systems(FixedUpdate, direct_demo.run_if(bot_on).run_if(not_loading))
            .add_systems(
                Update,
                (keys, pop_coins, draw_debug, watch, report).run_if(not_loading),
            );
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
        .bind("zoom", KeyCode::KeyZ)
        .bind("zoom", GamepadButton::East)
        .bind("gallery", KeyCode::KeyP)
        .bind("gallery", GamepadButton::North)
        .bind("gallery", ScreenRegion::new(0.4, 0.0, 0.6, 1.0))
        .bind("reset", KeyCode::KeyR)
        .bind("quit", KeyCode::Escape)
        .bind("quit", GamepadButton::Select)
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut loading: ResMut<LoadingAssets>,
    mut level: ResMut<Level>,
    mut fade: ResMut<ScreenFade>,
) {
    level.map = server.load(MAP);
    level.sheet = server.load(SHEET);
    level.graph = server.load(GRAPH);
    loading.add(&level.map);
    loading.add(&level.sheet);
    loading.add(&level.graph);
    commands.spawn((
        Name::new("camera"),
        Camera2D {
            clear_color: Color::hex(SKY),
            ..default()
        },
        Transform2D::from_xy(320.0, 180.0),
        Interpolated::default(),
        CameraShake::default(),
    ));
    // Start covered; the level fades in once it has loaded.
    fade.set(1.0);
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
    // A clip built in code: the coin's two tiles, front and side, as a spin.
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
            "run with A/D or the arrows and jump with Space; Z zooms, P opens the particle \
             gallery, F1 shows the colliders and the camera's dead zone, R restarts"
        );
    }
}

/// Gives map objects their game components by class, and spawns the alien at the start.
/// When the map is reloaded its objects spawn again, and the run starts over.
#[allow(clippy::too_many_arguments)]
fn tag_objects(
    mut commands: Commands,
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
    }
    for (entity, object, transform) in &objects {
        match object.class.as_str() {
            "Coin" => {
                stats.coins_total += 1;
                let base = transform.translation;
                // Spinning at slightly different speeds, so the coins drift out of step.
                let speed = 0.8 + (base.x / 18.0) % 5.0 * 0.1;
                commands
                    .entity(entity)
                    .insert((
                        Coin,
                        SpriteAnimation::new(level.coins.clone(), "spin").with_speed(speed),
                    ))
                    // Bob for ever, each coin a little out of step with its neighbours.
                    .tween(
                        Tween::translation(base + vec2(0.0, 3.0), 0.7)
                            .from(base)
                            .ease(Ease::SineInOut)
                            .delay((base.x / 18.0) % 4.0 * 0.15)
                            .yoyo()
                            .forever(),
                    )
                    .with_child((
                        ParticleEmitter::new(sparkle()),
                        Layer(PLAYER_LAYER - 1),
                    ));
            }
            "Goal" => {
                commands.entity(entity).insert(Goal);
            }
            "BotHint" => {
                let action = object.properties.str("action").unwrap_or_default();
                commands.entity(entity).insert(BotHint(action.to_owned()));
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

/// A few faint specks around a coin.
fn sparkle() -> ParticleSettings {
    ParticleSettings {
        rate: 1.5,
        lifetime: (0.5, 0.8),
        speed: (3.0, 8.0),
        spread: 360.0,
        size: vec![1.0],
        color: vec![Color::hex(0xfff6b0), Color::hex(0xffd166).with_alpha(0.0)],
        shape: EmitShape::Circle(7.0),
        max: 8,
        ..default()
    }
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

/// Turns the intent into velocity, with take-off and landing effects.
#[allow(clippy::too_many_arguments)]
fn control_player(
    mut commands: Commands,
    time: Res<Time>,
    intent: Res<Intent>,
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
        // Squash and dust in proportion to the fall; shake only for long ones.
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
        stats.landings += 1;
        stats.hardest_landing = stats.hardest_landing.max(speed);
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

/// Spikes throw the alien back with a flash, a blink and a shake.
fn hurt(
    mut commands: Commands,
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
    // The flash and the blink are different properties, so they run together.
    commands
        .entity(player.sprite)
        .tween(Tween::flash(Color::WHITE, 0.25))
        .tween(
            Tween::color(Color::WHITE.with_alpha(0.3), 0.075)
                .from(Color::WHITE)
                .yoyo()
                .repeat(2 * (INVINCIBLE / 0.15) as u32),
        );
    info!("ouch, spikes");
}

/// Picks up coins with a pop and a burst, and notices the goal.
fn collect(
    mut commands: Commands,
    time: Res<Time>,
    mut started: MessageReader<CollisionStarted>,
    mut stats: ResMut<Stats>,
    player: Single<Entity, With<Player>>,
    coins: Query<&GlobalTransform2D, With<Coin>>,
    goals: Query<(), With<Goal>>,
) {
    for collision in started.read() {
        let Some(other) = collision.other(*player) else {
            continue;
        };
        if let Ok(transform) = coins.get(other) {
            stats.coins += 1;
            commands
                .entity(other)
                .remove::<(Coin, Collider)>()
                .tween(Tween::scale(vec2(1.8, 1.8), 0.25).ease(Ease::QuadOut))
                .tween(
                    Tween::color(Color::WHITE.with_alpha(0.0), 0.25)
                        .ease(Ease::QuadIn)
                        .tag("picked"),
                );
            commands.spawn((
                ParticleEmitter::new(ParticleSettings {
                    rate: 0.0,
                    burst: 14,
                    lifetime: (0.3, 0.6),
                    speed: (40.0, 90.0),
                    spread: 360.0,
                    gravity: vec2(0.0, -160.0),
                    size: vec![2.0, 1.0],
                    color: vec![Color::hex(0xfff3a0), Color::hex(0xffb02e).with_alpha(0.0)],
                    ..default()
                })
                .once(),
                Transform2D::from_translation(transform.translation()),
                Layer(PLAYER_LAYER + 1),
            ));
        } else if goals.contains(other) && stats.finished.is_none() {
            let seconds = time.fixed_elapsed().as_secs_f32();
            stats.finished = Some(seconds);
            info!(
                "at the goal after {seconds:.1} s with {} of {} coins, {} jumps and {} hits",
                stats.coins, stats.coins_total, stats.jumps, stats.hits
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

/// Kicks up dust at each footstep of the run clip.
fn footsteps(
    mut events: MessageReader<AnimationEvent>,
    mut stats: ResMut<Stats>,
    player: Single<&Player>,
    mut emitters: Query<&mut ParticleEmitter>,
) {
    for event in events.read() {
        if event.entity == player.sprite && event.name == "footstep" {
            stats.footsteps += 1;
            if let Ok(mut dust) = emitters.get_mut(player.dust) {
                dust.emit(2);
            }
        }
    }
}

/// Breathes slowly while idle; stops when the alien moves.
fn breathe(
    mut commands: Commands,
    mut changes: MessageReader<AnimationStateChanged>,
    mut stats: ResMut<Stats>,
    player: Single<&Player>,
    tweens: Query<&Tweens>,
) {
    for change in changes.read() {
        if change.entity != player.sprite {
            continue;
        }
        if stats.states.insert(change.to.clone()) {
            info!("the alien's first `{}`", change.to);
        }
        let breathing = tweens
            .get(player.sprite)
            .is_ok_and(|t| t.iter().any(|t| t.tag.as_deref() == Some("breathe")));
        if change.to == "idle" {
            commands.entity(player.sprite).tween(
                Tween::scale(vec2(1.0, 1.04), 0.9)
                    .from(Vec2::ONE)
                    .ease(Ease::SineInOut)
                    .yoyo()
                    .forever()
                    .delay(0.4)
                    .tag("breathe"),
            );
        } else if breathing {
            commands
                .entity(player.sprite)
                .tween(Tween::scale(Vec2::ONE, 0.1));
        }
    }
}

/// The particle gallery over the paused level: a fountain, fire, snow, sparks and a
/// stream of 10,000 particles, all gone when the gallery closes.
fn enter_gallery(
    mut commands: Commands,
    camera: Single<&Transform2D, With<Camera2D>>,
    player: Query<&GlobalTransform2D, With<Player>>,
    mut stats: ResMut<Stats>,
) {
    use GameState::Gallery;
    stats.gallery_visits += 1;
    let center = camera.translation;
    // The fountain and the fire stand where the alien stands.
    let floor = player.iter().next().map_or(-90.0, |p| {
        p.translation().y - PLAYER_SIZE.y / 2.0 - center.y
    });
    let at = |x: f32, y: f32| Transform2D::from_translation(center + vec2(x, y));
    commands.spawn((
        Name::new("gallery backdrop"),
        Sprite::solid(Color::hex(0x1d1f24).with_alpha(0.88), vec2(720.0, 420.0)),
        at(0.0, 0.0),
        Layer(20),
        StateScoped(Gallery),
    ));
    let gallery = [
        (
            "fountain",
            at(-220.0, floor),
            ParticleSettings {
                rate: 320.0,
                lifetime: (1.1, 1.5),
                speed: (170.0, 210.0),
                spread: 18.0,
                gravity: vec2(0.0, -300.0),
                size: vec![2.0, 3.0, 2.0],
                color: vec![Color::hex(0xbde8ff), Color::hex(0x3a86ff).with_alpha(0.0)],
                ..default()
            },
        ),
        (
            "fire",
            at(-80.0, floor),
            ParticleSettings {
                rate: 260.0,
                lifetime: (0.5, 0.9),
                speed: (20.0, 60.0),
                spread: 40.0,
                gravity: vec2(0.0, 70.0),
                size: vec![5.0, 3.0, 1.0],
                color: vec![
                    Color::hex(0xfff3a0),
                    Color::hex(0xff9f1c),
                    Color::hex(0xe63946).with_alpha(0.6),
                    Color::hex(0x6d6875).with_alpha(0.0),
                ],
                shape: EmitShape::Rect(vec2(18.0, 2.0)),
                ..default()
            },
        ),
        (
            "snow",
            at(0.0, 200.0),
            ParticleSettings {
                rate: 70.0,
                lifetime: (6.0, 7.0),
                speed: (14.0, 30.0),
                direction: -90.0,
                spread: 40.0,
                size: vec![2.0],
                scale: (0.5, 1.0),
                color: vec![Color::WHITE, Color::WHITE, Color::WHITE.with_alpha(0.0)],
                shape: EmitShape::Rect(vec2(680.0, 4.0)),
                ..default()
            },
        ),
        (
            "stream",
            at(170.0, -40.0),
            ParticleSettings {
                rate: 5_600.0,
                lifetime: (1.6, 2.0),
                speed: (20.0, 150.0),
                spread: 360.0,
                gravity: vec2(0.0, -40.0),
                drag: 0.6,
                size: vec![1.0],
                color: vec![
                    Color::hex(0xff5d8f),
                    Color::hex(0xffd166),
                    Color::hex(0x06d6a0),
                    Color::hex(0x118ab2).with_alpha(0.0),
                ],
                shape: EmitShape::Circle(4.0),
                max: STREAM,
                ..default()
            },
        ),
    ];
    for (name, transform, settings) in gallery {
        commands.spawn((
            Name::new(name),
            ParticleEmitter::new(settings),
            transform,
            Layer(21),
            StateScoped(Gallery),
        ));
    }
    commands.spawn((
        Name::new("sparks"),
        ParticleEmitter::new(ParticleSettings {
            rate: 0.0,
            burst: 0,
            lifetime: (0.5, 0.9),
            speed: (80.0, 170.0),
            spread: 360.0,
            gravity: vec2(0.0, -220.0),
            drag: 1.0,
            size: vec![2.0, 1.0],
            color: vec![
                Color::WHITE,
                Color::hex(0xffd166),
                Color::hex(0xff9f1c).with_alpha(0.0),
            ],
            ..default()
        }),
        at(60.0, 60.0),
        Layer(21),
        StateScoped(Gallery),
    ));
    info!("the particle gallery: a fountain, fire, snow, sparks and a stream of {STREAM}");
}

/// Bursts of sparks every half second in the gallery.
fn sparks(time: Res<Time>, mut emitters: Query<(&Name, &mut ParticleEmitter)>) {
    if !time.fixed_step_count().is_multiple_of(30) {
        return;
    }
    for (name, mut emitter) in &mut emitters {
        if name.as_str() == "sparks" {
            emitter.emit(60);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn keys(
    actions: Res<ActionState>,
    level: Res<Level>,
    state: Res<State<GameState>>,
    mut zoom: ResMut<Zoom>,
    mut state_fade: ResMut<StateFade<GameState>>,
    mut commands: Commands,
    camera: Single<(Entity, Option<&mut CameraFollow>), With<Camera2D>>,
    player: Query<(
        &mut Transform2D,
        &mut KinematicBody,
        &mut Interpolated,
        &Player,
    )>,
    mut animators: Query<&mut Animator>,
    mut exit: MessageWriter<AppExit>,
) {
    let (camera, follow) = camera.into_inner();
    if actions.just_pressed("zoom") {
        zoom_camera(&mut commands, camera, &mut zoom);
    }
    if actions.just_pressed("gallery") && !state_fade.is_busy() {
        if *state.current() == GameState::Gallery {
            state_fade.pop();
        } else {
            state_fade.push(GameState::Gallery);
        }
    }
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
        if let Some(mut follow) = follow {
            follow.snap();
        }
        info!("back to the start");
    }
    if actions.just_pressed("quit") {
        exit.write(AppExit::Success);
    }
}

fn zoom_camera(commands: &mut Commands, camera: Entity, zoom: &mut Zoom) {
    zoom.0 = !zoom.0;
    let to = if zoom.0 { 2.0 } else { 1.0 };
    commands
        .entity(camera)
        .tween(Tween::zoom(to, 0.8).ease(Ease::CubicInOut));
}

/// The demo after the goal: zoom in and out, open the gallery for four seconds, close it
/// and exit. Exits with an error if the bot never reaches the goal.
#[allow(clippy::too_many_arguments)]
fn direct_demo(
    time: Res<Time>,
    stats: Res<Stats>,
    state: Res<State<GameState>>,
    budget: Res<ParticleBudget>,
    render_stats: Res<RenderStats>,
    mut zoom: ResMut<Zoom>,
    mut state_fade: ResMut<StateFade<GameState>>,
    mut commands: Commands,
    camera: Single<Entity, With<Camera2D>>,
    mut exit: MessageWriter<AppExit>,
    mut stage: Local<(u32, u64)>,
) {
    let step = time.fixed_step_count();
    let (current, since) = *stage;
    let next = |stage: &mut (u32, u64)| *stage = (current + 1, step);
    match current {
        0 if stats.finished.is_some() => {
            info!("demo: zooming in at the goal");
            zoom_camera(&mut commands, *camera, &mut zoom);
            next(&mut stage);
        }
        0 if step > DEMO_STEPS => {
            error!("demo: the bot did not reach the goal ({:?})", *stats);
            exit.write(AppExit::error());
            *stage = (99, step);
        }
        1 if step > since + 90 => {
            zoom_camera(&mut commands, *camera, &mut zoom);
            next(&mut stage);
        }
        2 if step > since + 90 => {
            state_fade.push(GameState::Gallery);
            next(&mut stage);
        }
        3 if *state.current() == GameState::Gallery && !state_fade.is_busy() => next(&mut stage),
        4 if step > since + 240 => {
            info!(
                "demo: the gallery holds {} particles ({} at most) at {:.0} fps, {} draw calls",
                budget.live(),
                stats.peak_particles,
                render_stats.fps(),
                render_stats.draw_calls()
            );
            state_fade.pop();
            next(&mut stage);
        }
        5 if *state.current() == GameState::Playing && !state_fade.is_busy() => {
            info!("demo: done ({:?})", *stats);
            exit.write(AppExit::Success);
            next(&mut stage);
        }
        _ => {}
    }
}

/// Keeps the statistics: the most particles and trauma, and whether the latest frame
/// showed anything outside the level.
fn watch(
    mut stats: ResMut<Stats>,
    level: Res<Level>,
    budget: Res<ParticleBudget>,
    frame: Res<FrameView>,
    window: Res<Window>,
    shake: Single<&CameraShake>,
) {
    stats.peak_particles = stats.peak_particles.max(budget.live());
    stats.peak_trauma = stats.peak_trauma.max(shake.trauma);
    if level.bounds.size() != Vec2::ZERO {
        let view = frame.0.visible_rect(&window);
        let inside = view.min.x >= level.bounds.min.x - 0.01
            && view.min.y >= level.bounds.min.y - 0.01
            && view.max.x <= level.bounds.max.x + 0.01
            && view.max.y <= level.bounds.max.y + 0.01;
        if !inside {
            stats.camera_outside += 1;
        }
    }
}

/// With debug drawing on (F1), outlines the camera's dead zone and the level's bounds.
fn draw_debug(
    debug: Res<PhysicsDebug>,
    camera: Single<(&Transform2D, &CameraFollow)>,
    mut shapes: ResMut<DebugShapes>,
) {
    if !debug.enabled {
        return;
    }
    let (transform, follow) = *camera;
    let zone = Rect::from_center_size(transform.translation - follow.offset, follow.dead_zone);
    shapes.rect(zone, Color::hex(0xff5d8f));
    if let Some(bounds) = follow.bounds {
        shapes.rect(bounds, Color::hex(0x06d6a0));
    }
}

/// A status line every two seconds.
fn report(
    time: Res<Time>,
    render_stats: Res<RenderStats>,
    stats: Res<Stats>,
    budget: Res<ParticleBudget>,
    player: Query<&Player>,
    animators: Query<&Animator>,
    mut last: Local<f64>,
) {
    let now = time.elapsed_secs();
    if now - *last < 2.0 {
        return;
    }
    *last = now;
    let state = player
        .iter()
        .next()
        .and_then(|p| animators.get(p.sprite).ok())
        .and_then(Animator::state)
        .unwrap_or("-");
    info!(
        "{:.0} fps, {} draw calls; the alien is {state}; {} of {} coins, {} footsteps, {} \
         landings, {} hits; {} particles",
        render_stats.fps(),
        render_stats.draw_calls(),
        stats.coins,
        stats.coins_total,
        stats.footsteps,
        stats.landings,
        stats.hits,
        budget.live(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use cashgear_engine::anim::Direction;

    /// The level without a window, with the bot playing.
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
        .add_plugins(Game { demo: true });
        app
    }

    #[test]
    fn the_sheet_graph_and_level_load() {
        let mut app = demo_app();
        for _ in 0..4 {
            app.update(core::time::Duration::from_millis(16));
        }
        let world = app.world();
        let level = world.resource::<Level>();
        let sheets = world.resource::<Assets<SpriteSheet>>();
        let sheet = sheets.get(&level.sheet).expect("the alien sheet loaded");
        assert_eq!(sheet.frames.len(), 7);
        let names: Vec<_> = sheet.clips.keys().map(String::as_str).collect();
        assert_eq!(names, ["fall", "idle", "jump", "land", "run"]);
        let run = sheet.clip("run").unwrap();
        assert_eq!(run.events.len(), 2, "footsteps from the tag's user data");
        assert_eq!(sheet.clip("land").unwrap().repeat, Some(1));
        assert_eq!(sheet.clip("idle").unwrap().direction, Direction::Forward);
        // The trimmed landing frame is shorter and sits lower in its 24×24 canvas.
        let land = sheet.frames[sheet.clip("land").unwrap().frames[0].frame];
        assert_eq!((land.rect.height(), land.offset.y), (21.0, 3.0));
        let coins = sheets.get(&level.coins).expect("the coin sheet was built");
        assert_eq!(coins.clip("spin").unwrap().frames.len(), 2);

        let graphs = world.resource::<Assets<AnimationGraph>>();
        let graph = graphs.get(&level.graph).expect("the graph loaded");
        assert_eq!(graph.states.len(), 6);
        graph.validate().unwrap();
        assert_eq!(world.resource::<Stats>().coins_total, 19);
        let world = app.world_mut();
        let mut flags = world.query_filtered::<(), (With<Goal>, With<TileAnimation>)>();
        assert_eq!(flags.iter(world).count(), 1, "the flag waves");
        let world = app.world();
        let level = world.resource::<Level>();
        assert_eq!(level.bounds, Rect::new(0.0, 0.0, 72.0 * 18.0, 26.0 * 18.0));
    }

    /// The bot plays the whole demo in fixed steps, so the result does not depend on the
    /// frame rate: every animation state, footsteps, landings with dust and shake, a hit,
    /// coins, the zoom, and the gallery with its 10,000-particle stream.
    #[test]
    fn the_bot_plays_the_demo() {
        let mut app = demo_app();
        let step = app.world().resource::<Time>().fixed_delta();
        let mut exit = None;
        for _ in 0..DEMO_STEPS + 1200 {
            app.update(step);
            if let Some(code) = app.should_exit() {
                exit = Some(code);
                break;
            }
        }
        let stats = app.world().resource::<Stats>();
        assert_eq!(exit, Some(AppExit::Success), "{stats:?}");
        assert!(stats.finished.is_some(), "{stats:?}");
        let states: Vec<_> = stats.states.iter().map(String::as_str).collect();
        assert_eq!(states, ["fall", "hurt", "idle", "jump", "land", "run"]);
        assert!(stats.footsteps >= 20, "{stats:?}");
        assert!(stats.landings >= 5, "{stats:?}");
        assert!(
            stats.hardest_landing >= FALL_SPEED - 1.0,
            "the cliff: {stats:?}"
        );
        assert!(stats.hits >= 1, "{stats:?}");
        assert!(stats.peak_trauma > 0.4, "{stats:?}");
        assert!(stats.coins >= 10, "{stats:?}");
        assert!(stats.peak_particles >= STREAM, "{stats:?}");
        assert_eq!(stats.gallery_visits, 1);
        assert_eq!(stats.camera_outside, 0, "the camera left the level");
        let budget = app.world().resource::<ParticleBudget>();
        assert_eq!(budget.dropped(), 0, "the budget was never full");
    }
}
