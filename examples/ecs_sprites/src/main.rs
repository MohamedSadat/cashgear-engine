//! `ecs_sprites`: game code as components and systems. Entities are spawned from RON
//! prefabs, moved in fixed steps and drawn interpolated, arranged in parent/child
//! hierarchies, hidden and shown, despawned, and switched between game states.
//!
//! - Desktop: `cargo run -p ecs_sprites`. Add `-- --frames 120` to exit after 120 frames,
//!   and `--demo` to have it switch states by itself: menu, playing, paused, playing, menu,
//!   every 8 seconds at 60 fps (CI uses it to run every transition).
//! - Web: run `trunk serve --port 8082` in `examples/ecs_sprites` and open
//!   <http://127.0.0.1:8082> (`?demo` for the demo). The log is in the browser console.
//!
//! The states form a stack: `Loading`, then `Menu`, then `Playing`, with `Paused` pushed on
//! top of `Playing` so the level stays on screen, frozen, under a dim overlay.
//!
//! - **Loading**: the prefabs (`assets/prefabs`) and bunny images (`assets/images`) load
//!   through the asset server, from disk or over the network; the menu starts once they
//!   are all in. The hat, stars, ground and clouds are drawn in code and registered under
//!   `generated/...` paths, so the prefabs can name them too. On desktop, editing a prefab
//!   or image while the game runs reloads it: new spawns use the new prefab, and every
//!   sprite shows the new image. `--asset-delay 1` slows loading down to see the state.
//! - **Menu**: the logo prefab, a bunny with a ring of stars that orbit it (the ring
//!   spins; its children follow).
//! - **Playing**: bunnies in party hats fall from the sky every few tenths of a second,
//!   bounce, blink in their last second and despawn. Catch them with the player. Clicking
//!   or tapping drops a handful where you point. The level is twice as wide as the view,
//!   so off-screen bunnies are culled; the status line counts them.
//! - **Paused**: an overlay parented to the camera.
//!
//! | Action | Keyboard | Gamepad | Touch |
//! | --- | --- | --- | --- |
//! | Start (menu) | Enter, Space, click | South (A / cross) | Tap |
//! | Run | Arrows, A / D | D-pad, left stick | Left / second quarter of the screen |
//! | Jump | Space, W, Up | South | Right half of the screen |
//! | Drop bunnies | Click | | Tap |
//! | Hide / show the player (and its hat) | V | North (Y / triangle) | |
//! | Pause / resume | Escape, P | Start | |
//! | Back to the menu (paused) or quit (menu) | Q, Backspace | Select | |

use cashgear_engine::prelude::*;
use serde::{Deserialize, Serialize};

/// The level spans `-HALF_WIDTH..HALF_WIDTH`: twice the 640-unit view.
const HALF_WIDTH: f32 = 640.0;
/// The top of the ground: one 16-unit tile above the bottom of the 360-unit view.
const GROUND_Y: f32 = -164.0;
/// Where falling bunnies appear, just above the view.
const SPAWN_Y: f32 = 200.0;
/// Gravity in world units per second squared (y up).
const GRAVITY: f32 = -900.0;
/// Seconds between falling bunnies.
const SPAWN_EVERY: f32 = 0.3;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum GameState {
    Loading,
    Menu,
    Playing,
    Paused,
}

/// Velocity in world units per second. In prefabs: `"Velocity": ((120, 40))`.
#[derive(Component, Debug, Clone, Copy, Default, Serialize, Deserialize)]
struct Velocity(Vec2);

/// Seconds until the entity despawns. In prefabs: `"Lifetime": (seconds: 6)`.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Lifetime {
    seconds: f32,
}

impl Default for Lifetime {
    fn default() -> Self {
        Self { seconds: 5.0 }
    }
}

/// Turns the entity, and so its children, every frame.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Spin {
    /// Radians per second, counter-clockwise.
    speed: f32,
}

/// The player's tuning.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Player {
    /// Running speed in world units per second.
    speed: f32,
    /// Take-off speed of a jump.
    jump: f32,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            speed: 160.0,
            jump: 400.0,
        }
    }
}

/// Bunnies spawned and caught this round, and when the status line was last logged.
#[derive(Resource, Default)]
struct Score {
    spawned: u32,
    caught: u32,
    last_report: f64,
}

/// The images drawn in code. The handles keep them loaded; the prefabs find the hat and
/// star by their `generated/...` paths.
#[derive(Resource)]
struct Art {
    ground: Handle<Image>,
    cloud: Handle<Image>,
    _hat: Handle<Image>,
    _star: Handle<Image>,
}

/// When the next bunny falls, and a random number generator for where.
#[derive(Resource, Default)]
struct Spawner {
    timer: f32,
    rng: Rng,
}

/// A small xorshift generator.
struct Rng(u32);

impl Default for Rng {
    fn default() -> Self {
        Self(0x2545_f491)
    }
}

impl Rng {
    /// A number in `min..max`.
    fn range(&mut self, min: f32, max: f32) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        min + (max - min) * ((self.0 >> 8) as f32 / (1 << 24) as f32)
    }
}

fn main() -> AppExit {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins {
        window: WindowConfig {
            title: "CashGear: ecs_sprites".into(),
            canvas_id: Some("cashgear".into()),
            ..default()
        },
        ..default()
    })
    .add_plugins(Game);
    if Args::from_env().flag("demo") {
        app.add_systems(Update, demo);
    }
    app.run()
}

/// The `--demo` script: frame numbers within each 480-frame cycle and the state change made
/// then.
const DEMO: [(u64, &str); 5] = [
    (60, "play"),
    (200, "drop"),
    (240, "pause"),
    (300, "resume"),
    (420, "menu"),
];

/// Switches states on a fixed frame schedule, so every transition runs without input.
fn demo(
    time: Res<Time>,
    mut commands: Commands,
    state: Res<State<GameState>>,
    mut next: ResMut<NextState<GameState>>,
    mut spawner: ResMut<Spawner>,
    mut score: ResMut<Score>,
) {
    if *state.current() == GameState::Loading {
        return;
    }
    let frame = time.frame_count() % 480;
    let Some((_, step)) = DEMO.iter().find(|(at, _)| *at == frame) else {
        return;
    };
    info!("demo: {step}");
    match *step {
        "play" => next.replace(GameState::Playing),
        "drop" => {
            for _ in 0..6 {
                let velocity = vec2(
                    spawner.rng.range(-160.0, 160.0),
                    spawner.rng.range(80.0, 320.0),
                );
                drop_bunny(&mut commands, &mut score, vec2(0.0, 0.0), velocity);
            }
        }
        "pause" => next.push(GameState::Paused),
        "resume" => next.pop(),
        _ => next.set(GameState::Menu),
    }
}

/// Everything but the engine's plugins, so the tests can build it without a window.
struct Game;

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        use GameState::{Loading, Menu, Paused, Playing};
        app.insert_resource(action_map())
            .init_resource::<Score>()
            .init_resource::<Spawner>()
            .register_component::<Velocity>("Velocity")
            .register_component::<Lifetime>("Lifetime")
            .register_component::<Spin>("Spin")
            .register_component::<Player>("Player")
            .load_prefab("logo", "prefabs/logo.prefab.ron")
            .load_prefab("player", "prefabs/player.prefab.ron")
            .load_prefab("bunny", "prefabs/bunny.prefab.ron")
            .init_state(Loading)
            .add_loading_state(Loading, Menu)
            .add_systems(Startup, setup)
            .add_systems(OnEnter(Loading), start_loading)
            .add_systems(OnEnter(Menu), enter_menu)
            .add_systems(OnEnter(Playing), enter_playing)
            .add_systems(OnExit(Playing), exit_playing)
            .add_systems(OnEnter(Paused), enter_paused)
            .add_systems(OnExit(Paused), |_: Commands| info!("resumed"))
            .add_systems(
                FixedUpdate,
                (player_control, physics, catch, age, spawn_bunnies, follow)
                    .chain()
                    .run_if(in_state(Playing)),
            )
            .add_systems(
                Update,
                (
                    spin,
                    menu_input.run_if(in_state(Menu)),
                    (playing_input, drop_at_pointer, report).run_if(in_state(Playing)),
                    paused_input.run_if(in_state(Paused)),
                ),
            );
    }
}

fn action_map() -> ActionMap {
    use AxisDirection::{Negative, Positive};
    ActionMap::new()
        .bind("left", KeyCode::ArrowLeft)
        .bind("left", KeyCode::KeyA)
        .bind("left", GamepadButton::DPadLeft)
        .bind("left", Binding::Axis(GamepadAxis::LeftStickX, Negative))
        .bind("left", ScreenRegion::new(0.0, 0.0, 0.25, 1.0))
        .bind("right", KeyCode::ArrowRight)
        .bind("right", KeyCode::KeyD)
        .bind("right", GamepadButton::DPadRight)
        .bind("right", Binding::Axis(GamepadAxis::LeftStickX, Positive))
        .bind("right", ScreenRegion::new(0.25, 0.0, 0.5, 1.0))
        .bind("jump", KeyCode::Space)
        .bind("jump", KeyCode::KeyW)
        .bind("jump", KeyCode::ArrowUp)
        .bind("jump", GamepadButton::South)
        .bind("jump", ScreenRegion::new(0.5, 0.0, 1.0, 1.0))
        .bind("start", KeyCode::Enter)
        .bind("start", KeyCode::Space)
        .bind("start", GamepadButton::South)
        .bind("start", ScreenRegion::new(0.0, 0.0, 1.0, 1.0))
        .bind("hide", KeyCode::KeyV)
        .bind("hide", GamepadButton::North)
        .bind("pause", KeyCode::Escape)
        .bind("pause", KeyCode::KeyP)
        .bind("pause", GamepadButton::Start)
        .bind("back", KeyCode::KeyQ)
        .bind("back", KeyCode::Backspace)
        .bind("back", GamepadButton::Select)
}

/// Draws the art made in code, registering it under paths the prefabs can name, and spawns
/// the camera that lives through every state.
fn setup(mut commands: Commands, server: Res<AssetServer>) {
    commands.insert_resource(Art {
        ground: server.add("generated/ground", ground()),
        cloud: server.add("generated/cloud", cloud()),
        _hat: server.add("generated/hat", hat()),
        _star: server.add("generated/star", star()),
    });

    commands.spawn((
        Name::new("camera"),
        Camera2D {
            clear_color: Color::hex(0x5fa8d3),
            ..default()
        },
        // It follows the player in fixed steps, so it is drawn smoothly too.
        Interpolated::default(),
    ));
}

/// Starts loading the bunny images; the prefabs are already loading. The menu starts once
/// everything in `LoadingAssets` is in.
fn start_loading(server: Res<AssetServer>, mut loading: ResMut<LoadingAssets>) {
    for path in ["images/bunny_white.png", "images/bunny_brown.png"] {
        loading.add(server.load::<Image>(path));
    }
    info!("loading {} assets", loading.handles().len());
}

fn enter_menu(
    mut commands: Commands,
    camera: Single<(&mut Transform2D, &mut Interpolated), With<Camera2D>>,
) {
    let (mut transform, mut interpolated) = camera.into_inner();
    *transform = Transform2D::IDENTITY;
    interpolated.reset();
    commands
        .spawn_prefab("logo")
        .insert(StateScoped(GameState::Menu));
    info!("menu: press Enter, Space or click to play; Q quits");
}

fn menu_input(
    actions: Res<ActionState>,
    pointers: Res<Pointers>,
    mut next: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
) {
    if actions.just_pressed("start") || pointers.iter().any(|p| p.just_pressed() && !p.is_touch()) {
        next.replace(GameState::Playing);
    }
    if actions.just_pressed("back") {
        exit.write(AppExit::Success);
    }
}

fn spin(time: Res<Time>, mut spinners: Query<(&mut Transform2D, &Spin)>) {
    for (mut transform, spin) in &mut spinners {
        transform.rotate(spin.speed * time.delta_secs());
    }
}

/// Builds the level: ground tiles, clouds and the player, all scoped to `Playing`.
fn enter_playing(
    mut commands: Commands,
    art: Res<Art>,
    mut score: ResMut<Score>,
    mut spawner: ResMut<Spawner>,
    state: Res<State<GameState>>,
) {
    let scope = StateScoped(GameState::Playing);
    // Ground: one row of 16×16 tiles under one parent, so the whole floor is one entity
    // tree that despawns together.
    commands
        .spawn((
            Name::new("ground"),
            Transform2D::from_xy(0.0, GROUND_Y),
            scope.clone(),
        ))
        .with_children(|ground| {
            let mut x = -HALF_WIDTH;
            while x < HALF_WIDTH {
                ground.spawn((
                    Sprite {
                        pivot: vec2(0.0, 1.0),
                        ..Sprite::new(art.ground.clone())
                    },
                    Transform2D::from_xy(x, 0.0),
                ));
                x += 16.0;
            }
        });
    for (i, x) in [-520.0f32, -260.0, 40.0, 300.0, 540.0]
        .into_iter()
        .enumerate()
    {
        commands.spawn((
            Sprite::new(art.cloud.clone()),
            Transform2D::from_xy(x, 110.0 + 25.0 * (i % 2) as f32).with_scale(Vec2::splat(2.0)),
            Layer(-1),
            Velocity(vec2(8.0 + 3.0 * i as f32, 0.0)),
            scope.clone(),
        ));
    }
    commands
        .spawn_prefab("player")
        .insert((Transform2D::from_xy(0.0, GROUND_Y), scope));
    *score = Score::default();
    spawner.timer = 0.0;
    info!(
        "playing ({:?}): run with A/D or the arrows, jump with Space, click to drop bunnies, \
         V hides the player, Escape pauses",
        state.stack()
    );
}

fn exit_playing(score: Res<Score>) {
    info!(
        "round over: caught {} of {} bunnies",
        score.caught, score.spawned
    );
}

fn playing_input(
    actions: Res<ActionState>,
    mut player: Single<&mut Visibility, With<Player>>,
    mut next: ResMut<NextState<GameState>>,
) {
    if actions.just_pressed("pause") {
        next.push(GameState::Paused);
    }
    if actions.just_pressed("hide") {
        player.toggle();
        info!("player {:?}", **player);
    }
}

/// Clicks and taps drop a handful of bunnies where they land in the world.
fn drop_at_pointer(
    mut commands: Commands,
    pointers: Res<Pointers>,
    camera: MainCamera,
    mut spawner: ResMut<Spawner>,
    mut score: ResMut<Score>,
) {
    for pointer in pointers.iter().filter(|p| p.just_pressed()) {
        let at = camera.screen_to_world(pointer.position());
        for _ in 0..6 {
            let velocity = vec2(
                spawner.rng.range(-160.0, 160.0),
                spawner.rng.range(80.0, 320.0),
            );
            drop_bunny(&mut commands, &mut score, at, velocity);
        }
    }
}

fn paused_input(actions: Res<ActionState>, mut next: ResMut<NextState<GameState>>) {
    if actions.just_pressed("pause") {
        next.pop();
    }
    if actions.just_pressed("back") {
        next.set(GameState::Menu);
    }
}

/// Dims the view with an overlay parented to the camera, so it stays centred wherever the
/// camera is, with a pause sign on top.
fn enter_paused(mut commands: Commands, camera: Single<Entity, With<Camera2D>>) {
    commands.spawn((
        Name::new("pause overlay"),
        Sprite::solid(Color::hex(0x1d1f24).with_alpha(0.6), vec2(640.0, 360.0)),
        Layer(100),
        ChildOf(*camera),
        StateScoped(GameState::Paused),
        children![
            (
                Sprite::solid(Color::WHITE, vec2(10.0, 36.0)),
                Transform2D::from_xy(-9.0, 0.0),
                Layer(101),
            ),
            (
                Sprite::solid(Color::WHITE, vec2(10.0, 36.0)),
                Transform2D::from_xy(9.0, 0.0),
                Layer(101),
            ),
        ],
    ));
    info!("paused: Escape resumes, Q goes back to the menu");
}

fn drop_bunny(commands: &mut Commands, score: &mut Score, at: Vec2, velocity: Vec2) {
    score.spawned += 1;
    commands.spawn_prefab("bunny").insert((
        Transform2D::from_translation(at),
        Velocity(velocity),
        StateScoped(GameState::Playing),
    ));
}

fn player_control(
    actions: Res<ActionState>,
    player: Single<(&Player, &mut Velocity, &Transform2D)>,
) {
    let (player, mut velocity, transform) = player.into_inner();
    velocity.0.x = actions.axis("left", "right") * player.speed;
    let grounded = transform.translation.y <= GROUND_Y;
    if grounded && actions.just_pressed("jump") {
        velocity.0.y = player.jump;
    }
}

/// Gravity, the ground and the level's edges. Clouds have no `Lifetime` and drift without
/// gravity; bunnies bounce; the player stops.
fn physics(
    time: Res<Time>,
    mut bodies: Query<(&mut Transform2D, &mut Velocity, Has<Player>, Has<Lifetime>)>,
) {
    let dt = time.fixed_delta_secs();
    for (mut transform, mut velocity, is_player, is_bunny) in &mut bodies {
        if !is_player && !is_bunny {
            // A cloud: drift and wrap around the level.
            transform.translation.x += velocity.0.x * dt;
            if transform.translation.x > HALF_WIDTH + 48.0 {
                transform.translation.x -= 2.0 * HALF_WIDTH + 96.0;
            }
            continue;
        }
        velocity.0.y += GRAVITY * dt;
        transform.translation += velocity.0 * dt;
        if transform.translation.y < GROUND_Y {
            transform.translation.y = GROUND_Y;
            velocity.0.y = if is_player { 0.0 } else { -velocity.0.y * 0.6 };
        }
        let edge = HALF_WIDTH - 9.0;
        if transform.translation.x.abs() > edge {
            transform.translation.x = transform.translation.x.clamp(-edge, edge);
            if !is_player {
                velocity.0.x = -velocity.0.x;
            }
        }
    }
}

/// Bunnies the player touches are caught and despawn (their hats with them).
fn catch(
    mut commands: Commands,
    player: Single<(&Transform2D, &Visibility), With<Player>>,
    bunnies: Query<(Entity, &Transform2D), With<Lifetime>>,
    mut score: ResMut<Score>,
) {
    let (player, visibility) = player.into_inner();
    if *visibility == Visibility::Hidden {
        return;
    }
    for (bunny, transform) in &bunnies {
        if transform.translation.distance(player.translation) < 16.0 {
            commands.entity(bunny).try_despawn();
            score.caught += 1;
        }
    }
}

/// Counts lifetimes down: bunnies blink in their last second, then despawn.
fn age(
    mut commands: Commands,
    time: Res<Time>,
    mut bunnies: Query<(Entity, &mut Lifetime, &mut Visibility)>,
) {
    let dt = time.fixed_delta_secs();
    for (bunny, mut lifetime, mut visibility) in &mut bunnies {
        lifetime.seconds -= dt;
        if lifetime.seconds <= 0.0 {
            commands.entity(bunny).try_despawn();
        } else if lifetime.seconds < 1.0 {
            // Hiding the bunny hides its hat too: the hat inherits visibility.
            let shown = (lifetime.seconds * 8.0) as i32 % 2 == 0;
            let wanted = if shown {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *visibility != wanted {
                *visibility = wanted;
            }
        }
    }
}

fn spawn_bunnies(
    mut commands: Commands,
    time: Res<Time>,
    mut spawner: ResMut<Spawner>,
    mut score: ResMut<Score>,
) {
    spawner.timer -= time.fixed_delta_secs();
    while spawner.timer <= 0.0 {
        spawner.timer += SPAWN_EVERY;
        let at = vec2(spawner.rng.range(-HALF_WIDTH, HALF_WIDTH), SPAWN_Y);
        let velocity = vec2(
            spawner.rng.range(-90.0, 90.0),
            spawner.rng.range(-60.0, 60.0),
        );
        drop_bunny(&mut commands, &mut score, at, velocity);
    }
}

/// The camera eases towards the player, staying inside the level.
fn follow(
    time: Res<Time>,
    player: Single<&Transform2D, (With<Player>, Without<Camera2D>)>,
    mut camera: Single<&mut Transform2D, With<Camera2D>>,
) {
    let target = player
        .translation
        .x
        .clamp(-HALF_WIDTH + 320.0, HALF_WIDTH - 320.0);
    let ease = 1.0 - (-6.0 * time.fixed_delta_secs()).exp();
    camera.translation.x += (target - camera.translation.x) * ease;
}

fn report(
    time: Res<Time>,
    stats: Res<RenderStats>,
    bunnies: Query<(), With<Lifetime>>,
    entities: Query<()>,
    mut score: ResMut<Score>,
) {
    let elapsed = time.raw_elapsed().as_secs_f64();
    if elapsed - score.last_report < 2.0 {
        return;
    }
    score.last_report = elapsed;
    info!(
        "{} bunnies falling, {} caught of {}; {} entities, {} sprites drawn, {} culled, \
         {} draw calls, {:.0} fps",
        bunnies.iter().count(),
        score.caught,
        score.spawned,
        entities.iter().count(),
        stats.sprites(),
        stats.culled(),
        stats.draw_calls(),
        stats.fps(),
    );
}

/// Builds an image from rows of characters, one per pixel, looked up in `palette`; any
/// other character is transparent.
fn pixel_art(rows: &[&str], palette: &[(char, u32)]) -> Image {
    let width = rows[0].len() as u32;
    Image::from_fn(width, rows.len() as u32, |x, y| {
        let c = rows[y as usize].as_bytes()[x as usize] as char;
        palette
            .iter()
            .find(|(key, _)| *key == c)
            .map(|&(_, rgb)| Color::hex(rgb).to_rgba8())
            .unwrap_or([0; 4])
    })
}

/// A 10×8 top hat, light so that sprite tints colour it.
fn hat() -> Image {
    pixel_art(
        &[
            "..######..",
            "..#####=..",
            "..#####=..",
            "..#####=..",
            "..------..",
            "..#####=..",
            "##########",
            ".========.",
        ],
        &[('#', 0xf2f2f2), ('=', 0xc8c8c8), ('-', 0x8a8a8a)],
    )
}

/// A 7×7 star, white so that sprite tints colour it.
fn star() -> Image {
    pixel_art(
        &[
            "...#...", "..###..", "#######", ".#####.", "..###..", ".##.##.", ".#...#.",
        ],
        &[('#', 0xffffff)],
    )
}

/// A 16×16 ground tile: grass over speckled earth.
fn ground() -> Image {
    Image::from_fn(16, 16, |x, y| {
        let rgb = match y {
            0..=2 => 0x5ac54f,
            3 => 0x3e9e3c,
            _ if (x * 7 + y * 13) % 11 == 0 => 0x6e452d,
            _ => 0x8a5a3c,
        };
        Color::hex(rgb).to_rgba8()
    })
}

/// A 24×10 cloud.
fn cloud() -> Image {
    Image::from_fn(24, 10, |x, y| {
        let (dx, dy) = ((x as f32 - 11.5) / 12.0, (y as f32 - 5.5) / 4.5);
        let bump = ((x as f32 - 8.0) / 5.0).powi(2) + ((y as f32 - 2.5) / 3.0).powi(2);
        if dx * dx + dy * dy <= 1.0 || bump <= 1.0 {
            [255, 255, 255, 235]
        } else {
            [0; 4]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cashgear_engine::world::PrefabRegistry;

    #[test]
    fn prefabs_load_and_parse() {
        let mut app = App::new();
        app.add_plugins(AssetPlugin {
            hot_reload: Some(false),
            ..default()
        })
        .add_plugins(InputPlugin { gamepads: false })
        .add_plugins(RenderPlugin::default())
        .add_plugins(Game);
        // Desktop reads files at once. The prefabs load in the first update; the images
        // are asked for in `OnEnter(Loading)`, after that update's asset systems, so they
        // arrive in the second. The menu starts in the third.
        for _ in 0..3 {
            app.update(core::time::Duration::ZERO);
        }
        assert_eq!(
            *app.world().resource::<State<GameState>>().current(),
            GameState::Menu
        );
        let world = app.world_mut();
        let progress = world
            .resource::<LoadingAssets>()
            .progress(world.resource::<AssetServer>());
        assert_eq!((progress.loaded, progress.total), (5, 5), "{progress:?}");
        let failures = world.resource_scope(|world, mut prefabs: Mut<Prefabs>| {
            prefabs.parse_all(world.resource::<PrefabRegistry>())
        });
        assert!(failures.is_empty(), "{failures:?}");
        for name in ["logo", "player", "bunny"] {
            assert!(world.resource::<Prefabs>().contains(name), "{name}");
        }
    }
}
