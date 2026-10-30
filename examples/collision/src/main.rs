//! `collision`: a box runs a test course made in Tiled: slopes of 45° and 22.5°, a pit of
//! spikes, a staircase of one-way planks, clouds to drop through, a dash into a thin
//! pillar, a stack of crates and a bottomless pit, with coins to collect on the way.
//!
//! - Desktop: `cargo run -p collision`. Add `-- --physics-debug` to start with the
//!   colliders drawn, and `--demo` to let a bot run the course: it exits when the bot
//!   reaches the goal, or with an error if it dies or takes too long (CI uses it).
//! - Web: run `trunk serve --port 8084` in `examples/collision` and open
//!   <http://127.0.0.1:8084> (add `?demo` or `?physics-debug`).
//!
//! What it shows:
//!
//! - **Tile collision from Tiled**: the `ground` layer has a `collision` property, and its
//!   tileset marks tiles `solid`, `one_way` and `hazard`. The slope tiles (made from
//!   Kenney's grass tile) carry `slope_left` and `slope_right` heights; the slopes going
//!   down are the same tiles flipped in Tiled.
//! - **A kinematic body**: the box runs, jumps (higher while the button is held), drops
//!   through one-way platforms (down + jump), dashes at 2,400 units per second and slams
//!   down at 3,000 without passing through anything.
//! - **Sensors**: coins and the goal flag are tile objects whose tile has a `sensor`
//!   property; the bot's hints are sensor rectangles. Touching them sends
//!   `CollisionStarted` messages.
//! - **Static colliders**: the crates are tile objects with a `collider` property.
//! - **Queries**: the bot looks for walls and gaps with raycasts and for spikes with
//!   `tile_at`. With debug drawing on (F1), a ray follows the mouse.
//!
//! | Action | Keyboard | Gamepad | Touch |
//! | --- | --- | --- | --- |
//! | Run | Arrows, A / D | D-pad, left stick | Left fifth / second fifth of the screen |
//! | Jump (hold for higher) | Space, W, Up | South (A / cross) | Right two fifths |
//! | Drop through a platform | Down + jump | Down + South | |
//! | Slam (in the air) | S, Down | D-pad down | Middle fifth |
//! | Dash | Shift, K | West (X / square) | |
//! | Show colliders | F1 | | |
//! | Back to the start | R | North (Y / triangle) | |
//! | Quit | Escape | Select | |
//!
//! Art: Kenney's Pixel Platformer (CC0, www.kenney.nl); see `assets/KENNEY-LICENSE.txt`.

use cashgear_engine::physics::{TileShape, WorldShape};
use cashgear_engine::prelude::*;

const MAP: &str = "maps/course.tmx";
/// The sky colour of the background, so the view above the hills matches.
const SKY: u32 = 0xdff6f5;
const PLAYER_SIZE: Vec2 = Vec2::new(12.0, 16.0);
/// Draw layer of the player: above the map's four layers.
const PLAYER_LAYER: i32 = 10;
/// Units per second.
const RUN_SPEED: f32 = 120.0;
const JUMP_SPEED: f32 = 330.0;
/// Rising speed is multiplied by this in each step the jump button is up.
const JUMP_CUT: f32 = 0.5;
const FALL_SPEED: f32 = 420.0;
const DASH_SPEED: f32 = 2400.0;
const DASH_TIME: f32 = 0.12;
const SLAM_SPEED: f32 = 3000.0;
/// Falling below this height means the player fell out of the level.
const KILL_HEIGHT: f32 = -40.0;
/// The demo gives up after this many fixed steps (a minute).
const DEMO_STEPS: u64 = 60 * 60;

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
    down: bool,
    down_pressed: bool,
    dash: bool,
}

#[derive(Component, Debug)]
struct Player {
    facing: f32,
    /// Seconds of dash left.
    dash: f32,
    slamming: bool,
}

/// One of the player's eyes, which look where the player faces.
#[derive(Component)]
struct Eye {
    side: f32,
}

#[derive(Component)]
struct Coin;

#[derive(Component)]
struct Goal;

/// A sensor area in the map telling the bot to do something.
#[derive(Component)]
struct BotHint(String);

/// The level and how the run is going.
#[derive(Resource, Default)]
struct Course {
    map: Handle<TiledMap>,
    start: Option<Vec2>,
    coins: u32,
    coins_total: u32,
    deaths: u32,
    finished: Option<f32>,
    /// Dashes that ended against a wall, and where the player stopped.
    dash_stops: Vec<f32>,
    /// Slams that ended on a one-way platform.
    slams_onto_platforms: u32,
}

/// The demo's bot: what it was told by the last hint, and how long it keeps holding jump.
#[derive(Resource, Default)]
struct Bot {
    enabled: bool,
    pending: Option<(String, u64)>,
    hold: u32,
}

fn main() -> AppExit {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins {
        window: WindowConfig {
            title: "CashGear: collision".into(),
            canvas_id: Some("cashgear".into()),
            ..default()
        },
        ..default()
    })
    .add_plugins(Game {
        demo: Args::from_env().flag("demo"),
    });
    app.run()
}

/// Everything but the engine's plugins, so the test can build it without a window.
struct Game {
    /// Let the bot play, and exit when it finishes.
    demo: bool,
}

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        use GameState::{Loading, Playing};
        app.insert_resource(action_map())
            .init_resource::<Intent>()
            .init_resource::<Course>()
            .insert_resource(Bot {
                enabled: self.demo,
                ..default()
            })
            .init_state(Loading)
            .add_loading_state(Loading, Playing)
            .add_systems(Startup, setup)
            .add_systems(OnEnter(Playing), enter_playing)
            .add_systems(PreUpdate, tag_objects)
            .add_systems(
                FixedUpdate,
                (
                    (read_controls.run_if(bot_off), drive_bot.run_if(bot_on)),
                    control_player,
                    (collect, fall_and_hurt, follow_camera),
                    finish_demo.run_if(bot_on),
                )
                    .chain()
                    .run_if(in_state(Playing)),
            )
            .add_systems(
                Update,
                (keys, look, aim_ray, report).run_if(in_state(Playing)),
            );
    }
}

fn bot_on(bot: Res<Bot>) -> bool {
    bot.enabled
}

fn bot_off(bot: Res<Bot>) -> bool {
    !bot.enabled
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
        .bind("down", KeyCode::KeyS)
        .bind("down", KeyCode::ArrowDown)
        .bind("down", GamepadButton::DPadDown)
        .bind("down", Binding::Axis(GamepadAxis::LeftStickY, Negative))
        .bind("down", ScreenRegion::new(0.4, 0.0, 0.6, 1.0))
        .bind("dash", KeyCode::ShiftLeft)
        .bind("dash", KeyCode::ShiftRight)
        .bind("dash", KeyCode::KeyK)
        .bind("dash", GamepadButton::West)
        .bind("reset", KeyCode::KeyR)
        .bind("reset", GamepadButton::North)
        .bind("quit", KeyCode::Escape)
        .bind("quit", GamepadButton::Select)
}

fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut loading: ResMut<LoadingAssets>,
    mut course: ResMut<Course>,
) {
    course.map = server.load(MAP);
    loading.add(&course.map);
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

fn enter_playing(mut commands: Commands, course: Res<Course>, bot: Res<Bot>) {
    commands.spawn((Name::new("course"), Tilemap::new(course.map.clone())));
    if bot.enabled {
        info!("demo: the bot runs the course");
    } else {
        info!(
            "run with A/D or the arrows, jump with Space, down + jump drops through platforms, \
             down in the air slams, Shift dashes; F1 shows the colliders, R restarts"
        );
    }
}

/// Gives map objects their game components by class, and spawns the player at the start.
/// When the map is reloaded its objects spawn again, and the run starts over.
fn tag_objects(
    mut commands: Commands,
    objects: Query<(Entity, &TiledObject, &Transform2D), Added<TiledObject>>,
    players: Query<Entity, With<Player>>,
    mut course: ResMut<Course>,
) {
    if objects
        .iter()
        .any(|(_, object, _)| object.class == "PlayerStart")
    {
        course.coins = 0;
        course.coins_total = 0;
        course.finished = None;
    }
    for (entity, object, transform) in &objects {
        match object.class.as_str() {
            "Coin" => {
                commands.entity(entity).insert(Coin);
                course.coins_total += 1;
            }
            "Goal" => {
                commands.entity(entity).insert(Goal);
            }
            "BotHint" => {
                let action = object.properties.str("action").unwrap_or_default();
                commands.entity(entity).insert(BotHint(action.to_owned()));
            }
            "PlayerStart" => {
                // The point is on the ground; the box stands on it.
                let start = transform.translation + vec2(0.0, PLAYER_SIZE.y / 2.0);
                course.start = Some(start);
                for player in &players {
                    commands.entity(player).despawn();
                }
                spawn_player(&mut commands, start);
            }
            _ => {}
        }
    }
}

fn spawn_player(commands: &mut Commands, at: Vec2) {
    commands
        .spawn((
            Name::new("player"),
            Player {
                facing: 1.0,
                dash: 0.0,
                slamming: false,
            },
            KinematicBody {
                max_fall_speed: FALL_SPEED,
                ..default()
            },
            Collider::rect(PLAYER_SIZE.x, PLAYER_SIZE.y),
            Sprite::solid(Color::hex(0xf28c28), PLAYER_SIZE),
            Layer(PLAYER_LAYER),
            Transform2D::from_translation(at),
        ))
        .with_children(|parent| {
            for side in [-1.0, 1.0] {
                parent
                    .spawn((
                        Eye { side },
                        Sprite::solid(Color::WHITE, vec2(3.0, 4.0)),
                        Layer(PLAYER_LAYER + 1),
                        Transform2D::from_xy(side * 3.0, 3.0),
                    ))
                    .with_child((
                        Sprite::solid(Color::hex(0x2b2d42), vec2(2.0, 2.0)),
                        Layer(PLAYER_LAYER + 2),
                        Transform2D::from_xy(0.5, -0.5),
                    ));
            }
        });
}

fn read_controls(actions: Res<ActionState>, mut intent: ResMut<Intent>) {
    *intent = Intent {
        run: actions.axis("left", "right"),
        jump: actions.just_pressed("jump"),
        jump_held: actions.pressed("jump"),
        down: actions.pressed("down"),
        down_pressed: actions.just_pressed("down"),
        dash: actions.just_pressed("dash"),
    };
}

/// The demo's player: runs right, jumps at walls, gaps and spikes it finds with queries,
/// and follows the hints placed in the map for the rest.
fn drive_bot(
    time: Res<Time>,
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

    *intent = Intent {
        run: 1.0,
        jump_held: bot.hold > 0,
        ..default()
    };
    bot.hold = bot.hold.saturating_sub(1);
    let grounded = contacts.on_ground();
    let mut jump = false;
    match bot.pending.as_ref().map(|(action, _)| action.as_str()) {
        Some("jump") if grounded => jump = true,
        Some("drop") if grounded => {
            intent.down = true;
            intent.jump = true;
        }
        Some("slam") if !grounded => intent.down_pressed = true,
        Some("dash") => intent.dash = true,
        _ => {}
    }
    if intent.jump || intent.down_pressed || intent.dash || jump {
        bot.pending = None;
    }

    let rect = collider.world_shape(transform.affine()).aabb();
    if grounded && !intent.jump {
        let solid = QueryFilter::default().exclude(entity).without_sensors();
        // A wall right ahead: a hit facing back at us. Slopes face up, so they do not count.
        let wall = spatial
            .raycast(
                vec2(rect.max.x - 1.0, rect.min.y + 4.0),
                Vec2::X,
                6.0,
                solid,
            )
            .is_some_and(|hit| hit.normal.x < -0.9);
        // No ground a little ahead, or spikes there.
        let ahead = vec2(rect.max.x + 6.0, rect.min.y + 2.0);
        let gap = spatial.raycast(ahead, Vec2::NEG_Y, 24.0, solid).is_none();
        let spikes = spatial
            .tile_at(ahead - vec2(0.0, 11.0), solid)
            .is_some_and(|tile| tile.tile.hazard);
        jump |= wall || gap || spikes;
    }
    if jump {
        intent.jump = true;
        intent.jump_held = true;
        bot.hold = 20;
    }
}

/// Turns the intent into velocity: run, jump, drop through, dash, slam.
fn control_player(
    time: Res<Time>,
    intent: Res<Intent>,
    spatial: SpatialQuery,
    mut course: ResMut<Course>,
    player: Single<(
        Entity,
        &mut Player,
        &mut KinematicBody,
        &KinematicContacts,
        &Collider,
        &GlobalTransform2D,
    )>,
) {
    let (entity, mut player, mut body, contacts, collider, transform) = player.into_inner();
    let dt = time.fixed_delta_secs();
    if contacts.on_ground() {
        if player.slamming
            && contacts.just_landed()
            && standing_on_platform(&spatial, entity, collider, transform)
        {
            course.slams_onto_platforms += 1;
            info!("slammed onto a one-way platform at {SLAM_SPEED} units per second");
        }
        player.slamming = false;
        body.max_fall_speed = FALL_SPEED;
    }

    if player.dash > 0.0 {
        player.dash -= dt;
        if contacts.on_wall() {
            let x = transform.translation().x;
            course.dash_stops.push(x);
            info!("the dash stopped at a wall, x {x:.1}");
            player.dash = 0.0;
        }
        if player.dash <= 0.0 {
            body.gravity_scale = 1.0;
            body.velocity.x = player.facing * RUN_SPEED;
        } else {
            body.velocity = vec2(player.facing * DASH_SPEED, 0.0);
        }
        return;
    }

    body.velocity.x = intent.run * RUN_SPEED;
    if intent.run != 0.0 {
        player.facing = intent.run.signum();
    }
    if intent.dash {
        player.dash = DASH_TIME;
        body.gravity_scale = 0.0;
        body.velocity = vec2(player.facing * DASH_SPEED, 0.0);
        return;
    }
    if contacts.on_ground() && intent.jump {
        if intent.down && standing_on_platform(&spatial, entity, collider, transform) {
            body.drop_through = true;
        } else {
            body.velocity.y = JUMP_SPEED;
        }
    }
    if !contacts.on_ground() && intent.down_pressed && !player.slamming {
        player.slamming = true;
        body.max_fall_speed = SLAM_SPEED;
        body.velocity = vec2(0.0, -SLAM_SPEED);
    }
    if body.velocity.y > 0.0 && !intent.jump_held {
        body.velocity.y *= JUMP_CUT;
    }
}

/// Whether the tile under the middle of the player's feet is a one-way platform.
fn standing_on_platform(
    spatial: &SpatialQuery,
    entity: Entity,
    collider: &Collider,
    transform: &GlobalTransform2D,
) -> bool {
    let rect = collider.world_shape(transform.affine()).aabb();
    let foot = vec2(rect.center().x, rect.min.y - 1.0);
    let filter = QueryFilter::default().exclude(entity);
    spatial
        .tile_at(foot, filter)
        .is_some_and(|tile| tile.tile.shape == TileShape::OneWay)
}

/// Picks up coins and notices the goal.
fn collect(
    mut commands: Commands,
    time: Res<Time>,
    mut started: MessageReader<CollisionStarted>,
    mut course: ResMut<Course>,
    player: Single<Entity, With<Player>>,
    coins: Query<(), With<Coin>>,
    goals: Query<(), With<Goal>>,
) {
    for collision in started.read() {
        let Some(other) = collision.other(*player) else {
            continue;
        };
        if coins.contains(other) {
            commands.entity(other).despawn();
            course.coins += 1;
        } else if goals.contains(other) && course.finished.is_none() {
            let seconds = time.fixed_elapsed().as_secs_f32();
            course.finished = Some(seconds);
            info!(
                "course complete in {seconds:.1} s with {} of {} coins and {} restarts",
                course.coins, course.coins_total, course.deaths
            );
        }
    }
}

/// Sends the player back to the start after touching spikes or falling out of the level.
fn fall_and_hurt(
    mut course: ResMut<Course>,
    player: Single<(
        &mut Transform2D,
        &mut KinematicBody,
        &KinematicContacts,
        &mut Interpolated,
        &mut Player,
    )>,
) {
    let (mut transform, mut body, contacts, mut interpolated, mut player) = player.into_inner();
    let fell = transform.translation.y < KILL_HEIGHT;
    if !(contacts.hazard || fell) {
        return;
    }
    let Some(start) = course.start else {
        return;
    };
    course.deaths += 1;
    info!(
        "{} at x {:.0}; back to the start",
        if fell { "fell out" } else { "ouch, spikes" },
        transform.translation.x
    );
    transform.translation = start;
    body.velocity = Vec2::ZERO;
    body.gravity_scale = 1.0;
    body.max_fall_speed = FALL_SPEED;
    player.dash = 0.0;
    player.slamming = false;
    interpolated.reset();
}

/// Keeps the camera on the player, inside the map.
fn follow_camera(
    course: Res<Course>,
    maps: Res<Assets<TiledMap>>,
    player: Single<&Transform2D, (With<Player>, Without<Camera2D>)>,
    mut camera: Single<&mut Transform2D, With<Camera2D>>,
) {
    let Some(map) = maps.get(&course.map) else {
        return;
    };
    let half_view = 320.0;
    let max = (map.size_px().x - half_view).max(half_view);
    camera.translation = vec2(player.translation.x.clamp(half_view, max), 180.0);
}

/// The demo's end: success at the goal, an error after a restart or a minute.
fn finish_demo(
    time: Res<Time>,
    course: Res<Course>,
    mut exit: MessageWriter<AppExit>,
    mut done: Local<bool>,
) {
    if *done {
        return;
    }
    let failed = course.deaths > 0 || time.fixed_step_count() > DEMO_STEPS;
    if course.finished.is_some() {
        *done = true;
        exit.write(AppExit::Success);
    } else if failed {
        *done = true;
        error!(
            "demo: the bot failed ({} restarts, {} steps)",
            course.deaths,
            time.fixed_step_count()
        );
        exit.write(AppExit::error());
    }
}

fn keys(
    actions: Res<ActionState>,
    mut course: ResMut<Course>,
    player: Single<(&mut Transform2D, &mut KinematicBody, &mut Interpolated), With<Player>>,
    mut exit: MessageWriter<AppExit>,
) {
    if actions.just_pressed("reset")
        && let Some(start) = course.start
    {
        let (mut transform, mut body, mut interpolated) = player.into_inner();
        transform.translation = start;
        body.velocity = Vec2::ZERO;
        interpolated.reset();
        course.finished = None;
        info!("back to the start");
    }
    if actions.just_pressed("quit") {
        exit.write(AppExit::Success);
    }
}

/// Points the eyes where the player faces.
fn look(player: Single<&Player>, mut eyes: Query<(&Eye, &mut Transform2D)>) {
    for (eye, mut transform) in &mut eyes {
        let x = eye.side * 3.0 + player.facing * 1.5;
        if transform.translation.x != x {
            transform.translation.x = x;
        }
    }
}

/// With debug drawing on, casts a ray from the player towards the mouse and draws where
/// it hits.
fn aim_ray(
    debug: Res<PhysicsDebug>,
    pointers: Res<Pointers>,
    camera: MainCamera,
    spatial: SpatialQuery,
    player: Single<(Entity, &GlobalTransform2D), With<Player>>,
    mut shapes: ResMut<DebugShapes>,
) {
    let Some(pointer) = pointers.primary().filter(|p| !p.is_touch()) else {
        return;
    };
    if !debug.enabled {
        return;
    }
    let (entity, transform) = *player;
    let origin = transform.translation();
    let target = camera.screen_to_world(pointer.position());
    let direction = (target - origin).normalize_or_zero();
    let reach = 240.0;
    let filter = QueryFilter::default().exclude(entity).without_sensors();
    match spatial.raycast(origin, direction, reach, filter) {
        Some(hit) => {
            shapes.line(origin, hit.point, Color::WHITE);
            shapes.circle(hit.point, 2.5, Color::WHITE);
            shapes.line(
                hit.point,
                hit.point + hit.normal * 10.0,
                Color::hex(0xffe600),
            );
        }
        None => shapes.line(origin, origin + direction * reach, Color::GRAY),
    }
    // Everything under the pointer, sensors included, gets a ring.
    for found in spatial.overlap_point(target, QueryFilter::default()) {
        if let Some(WorldShape::Rect(rect)) = spatial.world().shape(found) {
            shapes.rect(rect, Color::WHITE);
        }
    }
}

/// A status line every two seconds.
fn report(
    time: Res<Time>,
    stats: Res<RenderStats>,
    course: Res<Course>,
    player: Single<(&Transform2D, &KinematicContacts), With<Player>>,
    mut last: Local<f64>,
) {
    let now = time.elapsed_secs();
    if now - *last < 2.0 {
        return;
    }
    *last = now;
    let (transform, contacts) = *player;
    let state = if !contacts.on_ground() {
        "in the air"
    } else if contacts.on_slope() {
        "on a slope"
    } else {
        "on the ground"
    };
    info!(
        "{:.0} fps; player at ({:.0}, {:.0}) {state}{}; {} of {} coins, {} restarts",
        stats.fps(),
        transform.translation.x,
        transform.translation.y,
        if contacts.on_wall() {
            ", against a wall"
        } else {
            ""
        },
        course.coins,
        course.coins_total,
        course.deaths,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The course without a window, with the bot playing.
    fn course_app() -> App {
        let mut app = App::new();
        app.add_plugins(AssetPlugin {
            hot_reload: Some(false),
            ..default()
        })
        .add_plugins(InputPlugin { gamepads: false })
        .add_plugins(TiledPlugin)
        .add_plugins(PhysicsPlugin::default())
        .add_plugins(Game { demo: true });
        app
    }

    #[test]
    fn the_map_builds_its_collision() {
        let mut app = course_app();
        for _ in 0..4 {
            app.update(core::time::Duration::from_millis(16));
        }
        let world = app.world_mut();
        let mut tilemaps = world.query::<&TileCollider>();
        let grid = &tilemaps.single(world).unwrap().grid;
        let count = |f: fn(&TileCell) -> bool| grid.iter().filter(|(_, cell)| f(cell)).count();
        assert_eq!(count(|c| c.shape == TileShape::Solid), 779);
        assert_eq!(count(|c| c.shape == TileShape::OneWay), 29);
        assert_eq!(count(|c| matches!(c.shape, TileShape::Slope { .. })), 12);
        assert_eq!(count(|c| c.hazard), 3);
        let mut crates = world.query_filtered::<&Collider, (With<TiledObject>, Without<Sensor>)>();
        assert_eq!(crates.iter(world).count(), 6);
        assert_eq!(world.resource::<Course>().coins_total, 39);
    }

    /// The bot runs the whole course in fixed steps, so the result does not depend on the
    /// frame rate: slopes both ways, spikes, one-way planks, dropping through clouds, a
    /// slam onto a cloud and a dash into a pillar, neither passing through, the crates and
    /// the pit.
    #[test]
    fn the_bot_finishes_the_course() {
        let mut app = course_app();
        let step = app.world().resource::<Time>().fixed_delta();
        for _ in 0..DEMO_STEPS {
            app.update(step);
            let course = app.world().resource::<Course>();
            if course.finished.is_some() || course.deaths > 0 {
                break;
            }
        }
        let course = app.world().resource::<Course>();
        assert_eq!(course.deaths, 0, "the bot died");
        let seconds = course.finished.expect("the bot did not reach the goal");
        assert!(seconds < 40.0, "{seconds} s");
        // The pillar's left face is at x 92 × 18 = 1656; the box is 12 wide.
        assert_eq!(
            course.dash_stops,
            [1650.0],
            "the dash stopped at the pillar"
        );
        assert_eq!(course.slams_onto_platforms, 1);
        assert!(course.coins >= 25, "{} coins", course.coins);
    }
}
