//! The level: a Tiled map (`assets/maps/level.tmx`) whose object layer places the start,
//! the coins and the door. Edit it in Tiled or in the scene editor; while the game runs on
//! desktop, saving it rebuilds the level.
//!
//! Tiles collide by their properties in the tileset (`solid`, `one_way`, `hazard`), on
//! layers with the `collision` property. Objects are tile objects whose tile has a class:
//! `Coin` and `Exit` here, both `sensor`s; and a `PlayerStart` point.

use crate::player::{self, Player};
use crate::{GameSystems, Screen};
use cashgear_engine::prelude::*;

/// The level's map.
pub const MAP: &str = "maps/level.tmx";

/// What the level needs, loaded before the title shows.
#[derive(Resource)]
pub struct LevelAssets {
    /// The map.
    pub map: Handle<TiledMap>,
    /// The player's sprite sheet (an Aseprite export).
    pub sheet: Handle<SpriteSheet>,
    /// The player's animation state machine.
    pub graph: Handle<AnimationGraph>,
    /// Sounds.
    pub coin: Handle<Sound>,
    /// The jump sound.
    pub jump: Handle<Sound>,
    /// The sound at the door.
    pub goal: Handle<Sound>,
}

/// This try at the level.
#[derive(Resource, Debug, Default)]
pub struct Run {
    /// Coins taken, and how many the level has.
    pub coins: u32,
    /// Coins in the level.
    pub coins_total: u32,
    /// Where the player starts, and comes back to after a fall.
    pub start: Vec2,
    /// Falls and spikes so far.
    pub deaths: u32,
}

/// A coin.
#[derive(Component)]
pub struct Coin;

/// The door at the end.
#[derive(Component)]
pub struct Door;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Run>()
        // Coins get this prefab by their class: a Lua script that makes them bob.
        .load_prefab("Coin", "prefabs/coin.prefab.ron")
        .add_systems(Startup, load)
        .add_systems(OnEnter(Screen::Playing), spawn_level)
        .add_systems(Update, tag_objects.run_if(in_state(Screen::Playing)))
        .add_systems(
            FixedUpdate,
            (collect, fall_out).chain().in_set(GameSystems::Interact),
        )
        .add_systems(PreUpdate, play_in_editor.run_if(in_editor));
}

// ANCHOR: load
fn load(mut commands: Commands, server: Res<AssetServer>, mut loading: ResMut<LoadingAssets>) {
    let assets = LevelAssets {
        map: server.load(MAP),
        sheet: server.load("images/alien.json"),
        graph: server.load("anims/alien.anim.ron"),
        coin: server.load("sounds/coin.ogg"),
        jump: server.load("sounds/jump.ogg"),
        goal: server.load("sounds/goal.ogg"),
    };
    loading.add(&assets.map);
    loading.add(&assets.sheet);
    loading.add(&assets.graph);
    loading.add(&assets.coin);
    loading.add(&assets.jump);
    loading.add(&assets.goal);
    commands.insert_resource(assets);
}
// ANCHOR_END: load

/// A fresh level: the map spawns its objects, which [`tag_objects`] turns into the game's.
fn spawn_level(mut commands: Commands, assets: Res<LevelAssets>, mut run: ResMut<Run>) {
    *run = Run::default();
    commands.spawn((
        Name::new("level"),
        Tilemap::new(assets.map.clone()),
        StateScoped(Screen::Playing),
    ));
}

/// Gives the map's objects their game components by class, and spawns the player at the
/// start. The map spawns them again whenever it changes (hot reload, the editor).
// ANCHOR: objects
fn tag_objects(
    mut commands: Commands,
    objects: Query<(Entity, &TiledObject, &Transform2D), Added<TiledObject>>,
    players: Query<Entity, With<Player>>,
    assets: Res<LevelAssets>,
    mut run: ResMut<Run>,
) {
    for (entity, object, transform) in &objects {
        match object.class.as_str() {
            "Coin" => {
                commands.entity(entity).insert(Coin);
                run.coins_total += 1;
            }
            "Exit" => {
                commands.entity(entity).insert(Door);
            }
            "PlayerStart" => {
                // The point is on the ground; the player stands on it.
                run.start = transform.translation + vec2(0.0, player::SIZE.y / 2.0);
                for player in &players {
                    commands.entity(player).despawn();
                }
                player::spawn(&mut commands, &assets, run.start);
            }
            _ => {}
        }
    }
}
// ANCHOR_END: objects

/// Takes the coins the player touches, and ends the level at the door.
#[allow(clippy::too_many_arguments)]
// ANCHOR: collect
fn collect(
    mut commands: Commands,
    mut started: MessageReader<CollisionStarted>,
    player: Single<Entity, With<Player>>,
    coins: Query<(), With<Coin>>,
    doors: Query<(), With<Door>>,
    assets: Res<LevelAssets>,
    mut audio: ResMut<Audio>,
    mut run: ResMut<Run>,
    mut next: ResMut<NextState<Screen>>,
) {
    for collision in started.read() {
        let Some(other) = collision.other(*player) else {
            continue;
        };
        if coins.contains(other) {
            commands.entity(other).despawn();
            run.coins += 1;
            audio.play(&assets.coin);
        } else if doors.contains(other) {
            audio.play(&assets.goal);
            next.push(Screen::Won);
        }
    }
}
// ANCHOR_END: collect

/// Spikes or a fall below the level send the player back to the start.
fn fall_out(
    player: Single<(
        &mut Transform2D,
        &mut KinematicBody,
        &KinematicContacts,
        &mut Interpolated,
    ), With<Player>>,
    mut run: ResMut<Run>,
) {
    let (mut transform, mut body, contacts, mut interpolated) = player.into_inner();
    if contacts.hazard || transform.translation.y < -40.0 {
        transform.translation = run.start;
        body.velocity = Vec2::ZERO;
        // Jump there instead of sliding across the screen.
        interpolated.reset();
        run.deaths += 1;
    }
}

/// With the scene editor: its Play starts the level being edited, Stop comes back.
fn play_in_editor(
    mut started: MessageReader<PlayStarted>,
    mut stopped: MessageReader<PlayStopped>,
    mut next: ResMut<NextState<Screen>>,
) {
    if started.read().last().is_some() {
        next.set(Screen::Playing);
    }
    if stopped.read().last().is_some() {
        next.set(Screen::Editor);
    }
}
