//! `tiled_map`: a level made in Tiled with the Kenney Pixel Platformer art, loaded through
//! the asset server on desktop and web.
//!
//! - Desktop: `cargo run -p tiled_map`. Add `-- --frames 300` to exit after 300 frames,
//!   `--demo` to also reload the map once (CI uses it), and `--asset-delay 1` to watch
//!   the loading bar fill.
//! - Web: run `trunk serve --port 8083` in `examples/tiled_map` and open
//!   <http://127.0.0.1:8083>. The log is in the browser console.
//! - Live editing (desktop): open `examples/tiled_map/assets/maps/level.tmx` in Tiled
//!   while the example runs, change it and save. The level rebuilds in place. Saving a
//!   tileset, an image or a prefab reloads that too.
//!
//! What it shows:
//!
//! - **Loading**: the map (with its three tilesets and four images), the character atlas
//!   and five prefabs load in the background while a bar fills; the level starts once
//!   everything is in.
//! - **Layers**: a repeating hills image and a cloud layer scroll slower than the ground
//!   (parallax); decorations, some flipped in Tiled, sit on the ground layer.
//! - **Objects**: coins are tile objects whose `Coin` class spawns the coin prefab, which
//!   makes them bob. Enemies are tile objects of the `Slime`, `Alien`, `Pink` and `Bat`
//!   classes; their prefabs play clips of a sprite sheet built in code from the regions of
//!   `atlases/characters.atlas.ron` (packed by `cg_pack` from `art/characters`). A
//!   `Patrol` class property in Tiled tunes each walker's speed and distance.
//! - **Tile properties**: the tileset marks `solid`, `one_way` and `hazard` tiles for
//!   Phase 5's collision; the status line counts them.
//!
//! | Action | Keyboard | Gamepad | Touch |
//! | --- | --- | --- | --- |
//! | Scroll | Arrows, A / D | D-pad, left stick | Left / right quarter of the screen |
//! | Auto-scroll on / off | Space, Enter | South (A / cross) | Tap the middle |
//! | Reload the map | R | North (Y / triangle) | |
//! | Quit | Escape | Select | |
//!
//! Art: Kenney's Pixel Platformer (CC0, www.kenney.nl); see `assets/KENNEY-LICENSE.txt`.

use cashgear_engine::prelude::*;
use cashgear_engine::tiled::{LayerKind, TilemapState};
use serde::{Deserialize, Serialize};

const MAP: &str = "maps/level.tmx";
const ATLAS: &str = "atlases/characters.atlas.ron";
/// The path the character sheet is stored under once it is built from the atlas; the
/// enemy prefabs name it.
const SHEET: &str = "generated/characters.sheet";
/// The character clips: atlas regions in order, and frames per second.
const CLIPS: &[(&str, &[&str], f32)] = &[
    ("alien", &["alien_0", "alien_1"], 5.0),
    ("pink", &["pink_0", "pink_1"], 6.0),
    ("slime", &["slime_0", "slime_1"], 3.0),
    ("bat", &["bat_0", "bat_1", "bat_2", "bat_1"], 8.0),
];
/// Camera speed in world units per second.
const SCROLL_SPEED: f32 = 160.0;
/// The sky colour of the background tiles, so the view above the hills matches.
const SKY: u32 = 0xdff6f5;

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum GameState {
    Loading,
    Playing,
}

/// Bobs the entity up and down. In prefabs: `"Bob": (height: 2, speed: 3)`.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Bob {
    /// How far up and down, in world units.
    height: f32,
    /// Radians per second.
    speed: f32,
    #[serde(skip)]
    base: Option<Vec2>,
}

/// Walks back and forth around where it started. Tiled sets it per enemy with a class
/// property named `Patrol`; the prefab gives the defaults.
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Patrol {
    /// World units per second.
    speed: f32,
    /// How far either side of the start it walks.
    distance: f32,
    #[serde(skip)]
    origin: Option<f32>,
    #[serde(skip)]
    heading: f32,
}

impl Default for Patrol {
    fn default() -> Self {
        Self {
            speed: 30.0,
            distance: 48.0,
            origin: None,
            heading: 1.0,
        }
    }
}

/// Floats up and down around where it started.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Hover {
    height: f32,
    speed: f32,
    #[serde(skip)]
    base: Option<Vec2>,
}

/// Marks coins, for the status line.
#[derive(Component, Debug, Clone, Default, Serialize, Deserialize)]
struct Coin;

/// The level's handles, kept for as long as it runs.
#[derive(Resource)]
struct Level {
    map: Handle<TiledMap>,
    /// The character sheet, kept loaded once built.
    characters: Handle<SpriteSheet>,
}

/// Whether the camera scrolls by itself, and which way.
#[derive(Resource)]
struct Scroll {
    auto: bool,
    heading: f32,
}

impl Default for Scroll {
    fn default() -> Self {
        Self {
            auto: true,
            heading: 1.0,
        }
    }
}

fn main() -> AppExit {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins {
        window: WindowConfig {
            title: "CashGear: tiled_map".into(),
            canvas_id: Some("cashgear".into()),
            ..default()
        },
        ..default()
    })
    .add_plugins(Game);
    if Args::from_env().flag("demo") {
        app.add_systems(Update, demo.run_if(in_state(GameState::Playing)));
    }
    app.run()
}

/// Everything but the engine's plugins, so the tests can build it without a window.
struct Game;

impl Plugin for Game {
    fn build(&self, app: &mut App) {
        use GameState::{Loading, Playing};
        app.insert_resource(action_map())
            .init_resource::<Scroll>()
            .register_component::<Bob>("Bob")
            .register_component::<Patrol>("Patrol")
            .register_component::<Hover>("Hover")
            .register_component::<Coin>("Coin")
            // Object classes in the map name these prefabs.
            .load_prefab("Coin", "prefabs/coin.prefab.ron")
            .load_prefab("Slime", "prefabs/slime.prefab.ron")
            .load_prefab("Alien", "prefabs/alien.prefab.ron")
            .load_prefab("Pink", "prefabs/pink.prefab.ron")
            .load_prefab("Bat", "prefabs/bat.prefab.ron")
            .init_state(Loading)
            .add_loading_state(Loading, Playing)
            .add_systems(Startup, setup)
            .add_systems(PreUpdate, build_character_sheet)
            .add_systems(Update, draw_loading_bar.run_if(in_state(Loading)))
            .add_systems(OnEnter(Playing), enter_playing)
            .add_systems(
                FixedUpdate,
                (scroll_camera, bob, patrol, hover).run_if(in_state(Playing)),
            )
            .add_systems(
                Update,
                (playing_input, describe_level, report).run_if(in_state(Playing)),
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
        .bind("right", ScreenRegion::new(0.75, 0.0, 1.0, 1.0))
        .bind("auto", KeyCode::Space)
        .bind("auto", KeyCode::Enter)
        .bind("auto", GamepadButton::South)
        .bind("auto", ScreenRegion::new(0.25, 0.0, 0.75, 1.0))
        .bind("reload", KeyCode::KeyR)
        .bind("reload", GamepadButton::North)
        .bind("quit", KeyCode::Escape)
        .bind("quit", GamepadButton::Select)
}

/// Starts loading the level and the atlas (the prefabs are already loading), and spawns
/// the camera.
fn setup(mut commands: Commands, server: Res<AssetServer>, mut loading: ResMut<LoadingAssets>) {
    let map = server.load::<TiledMap>(MAP);
    loading.add(&map);
    loading.add(server.load::<TextureAtlas>(ATLAS));
    commands.insert_resource(Level {
        map,
        characters: Handle::default(),
    });
    commands.spawn((
        Name::new("camera"),
        Camera2D {
            clear_color: Color::hex(SKY),
            ..default()
        },
        Transform2D::from_xy(320.0, 180.0),
        // It moves in fixed steps, so it is drawn smoothly between them.
        Interpolated::default(),
    ));
    info!(
        "loading {} assets from `{}`",
        loading.handles().len(),
        server.io().root()
    );
}

/// A bar in the middle of the view, filling as the assets arrive.
fn draw_loading_bar(
    server: Res<AssetServer>,
    loading: Res<LoadingAssets>,
    mut sprites: ResMut<Sprites>,
) {
    let progress = loading.progress(&server);
    let centre = vec2(320.0, 180.0);
    let (width, height) = (240.0, 12.0);
    let frame = Sprite::solid(Color::hex(0x3a3f4b), vec2(width + 4.0, height + 4.0));
    sprites.draw(&frame, Transform2D::from_translation(centre));
    let fill = Sprite {
        pivot: vec2(0.0, 0.5),
        ..Sprite::solid(
            Color::hex(0x5ac54f),
            vec2(width * progress.fraction(), height),
        )
    };
    sprites.draw_on(
        1,
        &fill,
        Transform2D::from_translation(centre - vec2(width / 2.0, 0.0)),
    );
}

fn enter_playing(mut commands: Commands, level: Res<Level>) {
    commands.spawn((Name::new("level"), Tilemap::new(level.map.clone())));
    info!(
        "playing: the camera scrolls by itself; arrows or A/D take over, Space toggles \
         auto-scroll, R reloads the map, Escape quits"
    );
}

/// The `--demo` script: reload the map once, so CI runs the rebuild path.
fn demo(time: Res<Time>, server: Res<AssetServer>, mut done: Local<bool>) {
    if !*done && time.elapsed_secs() > 3.0 {
        *done = true;
        info!("demo: reloading `{MAP}`");
        server.reload(MAP);
    }
}

fn playing_input(
    actions: Res<ActionState>,
    server: Res<AssetServer>,
    mut scroll: ResMut<Scroll>,
    mut exit: MessageWriter<AppExit>,
) {
    if actions.just_pressed("auto") {
        scroll.auto = !scroll.auto;
        info!("auto-scroll {}", if scroll.auto { "on" } else { "off" });
    }
    if actions.just_pressed("reload") {
        let reloads = server.reload(MAP);
        info!("reloading `{MAP}` ({reloads} asset)");
    }
    if actions.just_pressed("quit") {
        exit.write(AppExit::Success);
    }
}

/// Moves the camera by input, or back and forth by itself, inside the map.
fn scroll_camera(
    time: Res<Time>,
    actions: Res<ActionState>,
    level: Res<Level>,
    maps: Res<Assets<TiledMap>>,
    mut scroll: ResMut<Scroll>,
    mut camera: Single<&mut Transform2D, With<Camera2D>>,
) {
    let Some(map) = maps.get(&level.map) else {
        return;
    };
    let input = actions.value("right") - actions.value("left");
    if input != 0.0 {
        scroll.auto = false;
    }
    let half_view = 320.0;
    let (min, max) = (half_view, (map.size_px().x - half_view).max(half_view));
    let x = &mut camera.translation.x;
    if scroll.auto {
        *x += scroll.heading * SCROLL_SPEED * time.fixed_delta_secs();
        if *x >= max || *x <= min {
            scroll.heading = -scroll.heading;
        }
    } else {
        *x += input * SCROLL_SPEED * 1.5 * time.fixed_delta_secs();
    }
    *x = x.clamp(min, max);
}

fn bob(time: Res<Time>, mut bobbers: Query<(&mut Transform2D, &mut Bob)>) {
    let t = time.fixed_elapsed().as_secs_f32();
    for (mut transform, mut bob) in &mut bobbers {
        let base = *bob.base.get_or_insert(transform.translation);
        // Neighbours bob out of step, by their position.
        let phase = base.x * 0.05;
        transform.translation.y = base.y + bob.height * (t * bob.speed + phase).sin();
    }
}

fn patrol(time: Res<Time>, mut walkers: Query<(&mut Transform2D, &mut Patrol, &mut Sprite)>) {
    for (mut transform, mut patrol, mut sprite) in &mut walkers {
        let origin = *patrol.origin.get_or_insert(transform.translation.x);
        transform.translation.x += patrol.heading * patrol.speed * time.fixed_delta_secs();
        let offset = transform.translation.x - origin;
        if offset.abs() >= patrol.distance {
            transform.translation.x = origin + patrol.distance * offset.signum();
            patrol.heading = -offset.signum();
        }
        sprite.flip_x = patrol.heading < 0.0;
    }
}

fn hover(time: Res<Time>, mut hoverers: Query<(&mut Transform2D, &mut Hover)>) {
    let t = time.fixed_elapsed().as_secs_f32();
    for (mut transform, mut hover) in &mut hoverers {
        let base = *hover.base.get_or_insert(transform.translation);
        transform.translation.y = base.y + hover.height * (t * hover.speed).sin();
    }
}

/// Builds the character sheet from the atlas whenever the atlas loads or changes, and
/// stores it under [`SHEET`] for the prefabs.
fn build_character_sheet(
    mut events: MessageReader<AssetEvent<TextureAtlas>>,
    atlases: Res<Assets<TextureAtlas>>,
    server: Res<AssetServer>,
    level: Option<ResMut<Level>>,
) {
    let Some(mut level) = level else {
        return;
    };
    for event in events.read() {
        let (AssetEvent::Added { id } | AssetEvent::Modified { id }) = *event else {
            continue;
        };
        if server.path(id.untyped()).as_deref() != Some(ATLAS) {
            continue;
        }
        let Some(atlas) = atlases.get(id) else {
            continue;
        };
        let mut sheet = SpriteSheet::from_atlas(atlas);
        for &(name, frames, fps) in CLIPS {
            match sheet.clip_from_frames(frames, fps) {
                Ok(clip) => {
                    sheet.add_clip(name, clip);
                }
                Err(err) => warn!("clip `{name}`: {err}"),
            }
        }
        level.characters = server.add(SHEET, sheet);
    }
}

/// Logs what the map holds each time it is built: when it first loads and after every
/// reload.
fn describe_level(tilemaps: Query<(&Tilemap, Ref<TilemapState>)>, maps: Res<Assets<TiledMap>>) {
    for (tilemap, state) in &tilemaps {
        if !state.is_changed() || !state.is_built() {
            continue;
        }
        let Some(map) = maps.get(&tilemap.map) else {
            continue;
        };
        let mut flags = [("solid", 0), ("one_way", 0), ("hazard", 0)];
        for layer in &map.layers {
            let LayerKind::Tiles(tiles) = &layer.kind else {
                continue;
            };
            for (_, tile) in tiles.iter() {
                let Some(info) = map.tile_info(tile) else {
                    continue;
                };
                for (name, count) in &mut flags {
                    if info.properties.bool(name) == Some(true) {
                        *count += 1;
                    }
                }
            }
        }
        info!(
            "level `{}`: {}x{} tiles of {}px, {} layers, {} tiles in {} chunks, {} objects; \
             {} solid, {} one-way and {} hazard tiles",
            map.properties.str("title").unwrap_or("untitled"),
            map.width,
            map.height,
            map.tile_size.x,
            map.layers.len(),
            state.tile_count(),
            state.chunk_count(),
            state.objects().len(),
            flags[0].1,
            flags[1].1,
            flags[2].1,
        );
        // The map lists its objects too; their entities spawn this frame.
        for (_, object) in map.objects() {
            match object.class.as_str() {
                "PlayerStart" | "Goal" => {
                    info!(
                        "{} `{}` at {}",
                        object.class,
                        object.name,
                        map.to_world(object.position)
                    )
                }
                "Hint" => info!(
                    "hint `{}`: {}",
                    object.name,
                    object.properties.str("text").unwrap_or_default()
                ),
                _ => {}
            }
        }
    }
}

/// A status line every two seconds.
fn report(
    time: Res<Time>,
    stats: Res<RenderStats>,
    coins: Query<(), With<Coin>>,
    enemies: Query<(), With<SpriteAnimation>>,
    camera: Single<&Transform2D, With<Camera2D>>,
    mut last: Local<f64>,
) {
    let now = time.elapsed_secs();
    if now - *last < 2.0 {
        return;
    }
    *last = now;
    info!(
        "{:.0} fps, {} sprites in {} draw calls, {} culled; {} coins, {} enemies; camera x {:.0}",
        stats.fps(),
        stats.sprites(),
        stats.draw_calls(),
        stats.culled(),
        coins.iter().count(),
        enemies.iter().count(),
        camera.translation.x,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use cashgear_engine::world::PrefabRegistry;

    #[test]
    fn the_level_loads_and_spawns() {
        let mut app = App::new();
        app.add_plugins(AssetPlugin {
            hot_reload: Some(false),
            ..default()
        })
        .add_plugins(InputPlugin { gamepads: false })
        .add_plugins(TiledPlugin)
        .add_plugins(AnimPlugin::default())
        .add_plugins(Game);
        // Desktop reads files at once: the first update loads everything and the state
        // moves on in the next; the map is built and its objects spawned after that.
        for _ in 0..4 {
            app.update(core::time::Duration::from_millis(16));
        }
        let world = app.world_mut();
        assert_eq!(
            *world.resource::<State<GameState>>().current(),
            GameState::Playing
        );
        let failures = world.resource_scope(|world, mut prefabs: Mut<Prefabs>| {
            prefabs.parse_all(world.resource::<PrefabRegistry>())
        });
        assert!(failures.is_empty(), "{failures:?}");

        let mut tilemaps = world.query::<&TilemapState>();
        let state = tilemaps.single(world).unwrap();
        assert!(state.tile_count() > 500, "{} tiles", state.tile_count());
        let objects = state.objects().len();
        let mut coins = world.query_filtered::<(), With<Coin>>();
        let mut walkers = world.query_filtered::<(), With<Patrol>>();
        let mut animated = world.query::<(&SpriteAnimation, &Sprite)>();
        assert_eq!(objects, 53);
        assert_eq!(coins.iter(world).count(), 42);
        assert_eq!(walkers.iter(world).count(), 5);
        assert_eq!(animated.iter(world).count(), 8);
        // The sheet was built from the atlas, and every enemy shows a frame of it.
        let level = world.resource::<Level>();
        let sheets = world.resource::<Assets<SpriteSheet>>();
        let sheet = sheets.get(&level.characters).expect("the character sheet");
        assert_eq!(sheet.clip("bat").unwrap().frames.len(), 4);
        for (animation, sprite) in animated.iter(world) {
            assert_eq!(animation.sheet, level.characters);
            assert!(sprite.source.is_some(), "showing {}", animation.clip());
        }
    }
}
