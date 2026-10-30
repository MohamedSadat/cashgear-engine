//! Levels: the table of levels, spawning the one being played (and the title screen's
//! backdrop), and turning the map's objects into game objects by their class.

use crate::progress::{MAX_HEARTS, Session};
use crate::scripts::Scripts;
use crate::{Art, Screen, enemies, player, things};
use cashgear_engine::prelude::*;
use cashgear_engine::tiled::ObjectShape;

/// One level.
pub(crate) struct LevelInfo {
    /// Its name's key in the string tables.
    pub name: &'static str,
    pub map: &'static str,
    pub music: &'static str,
    /// The sky colour, matching the top of its parallax hills.
    pub sky: u32,
}

pub(crate) const LEVELS: [LevelInfo; 3] = [
    LevelInfo {
        name: "level.1",
        map: "maps/level1.tmx",
        music: "music/meadow.ogg",
        sky: 0x7ff0a5,
    },
    LevelInfo {
        name: "level.2",
        map: "maps/level2.tmx",
        music: "music/dunes.ogg",
        sky: 0xfbc97b,
    },
    LevelInfo {
        name: "level.3",
        map: "maps/level3.tmx",
        music: "music/peaks.ogg",
        sky: 0xdff6f5,
    },
];

/// The title backdrop's sky.
const TITLE_SKY: u32 = 0x7ff0a5;
/// How far below the map's bottom a falling player is lost.
const KILL_DEPTH: f32 = 48.0;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<LevelBounds>()
        .add_systems(
            OnEnter(Screen::Title),
            (spawn_backdrop, title_music, skip_to_level),
        )
        .add_systems(Update, pan_backdrop.run_if(state_in_stack(Screen::Title)))
        .add_systems(OnEnter(Screen::Playing), spawn_level)
        .add_systems(
            PreUpdate,
            tag_objects.run_if(state_in_stack(Screen::Playing)),
        );
}

/// The map of the level being played.
#[derive(Component)]
pub(crate) struct LevelMap;

/// The level's extent in world units, and the depth at which a fall is lost.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub(crate) struct LevelBounds {
    pub rect: Rect,
}

impl LevelBounds {
    /// Whether something at `y` has fallen out of the level.
    pub fn fell(&self, y: f32) -> bool {
        y < self.rect.min.y - KILL_DEPTH
    }
}

/// A rectangle in the map where the demo's bot does something (`action`).
#[derive(Component, Debug, Clone)]
pub(crate) struct BotHint {
    pub action: String,
    pub rect: Rect,
}

// --- The title screen's backdrop ----------------------------------------------------

#[derive(Component)]
struct Backdrop;

fn spawn_backdrop(
    mut commands: Commands,
    art: Res<Art>,
    maps: Res<Assets<TiledMap>>,
    camera: Single<(Entity, &mut Camera2D, &mut Transform2D, &mut Interpolated)>,
) {
    commands.spawn((
        Name::new("title backdrop"),
        Tilemap::new(art.title_map.clone()),
        Backdrop,
        StateScoped(Screen::Title),
    ));
    let (entity, mut camera, mut transform, mut interpolated) = camera.into_inner();
    camera.clear_color = Color::hex(TITLE_SKY);
    commands.entity(entity).remove::<CameraFollow>();
    let height = maps
        .get(&art.title_map)
        .map_or(360.0, |map| map.size_px().y);
    transform.translation = vec2(320.0, height - 180.0);
    interpolated.reset();
}

/// Drifts the camera along the backdrop and back.
fn pan_backdrop(
    time: Res<Time>,
    art: Res<Art>,
    maps: Res<Assets<TiledMap>>,
    camera: Single<&mut Transform2D, With<Camera2D>>,
) {
    let Some(map) = maps.get(&art.title_map) else {
        return;
    };
    let span = (map.size_px().x - 640.0).max(0.0);
    let t = time.raw_elapsed().as_secs_f32() * 0.04;
    let x = 320.0 + span * (0.5 - 0.5 * (t * core::f32::consts::PI).cos());
    let mut transform = camera.into_inner();
    transform.translation.x = x;
}

/// `--level N`: straight into a level the first time the title opens.
fn skip_to_level(
    options: Res<crate::Options>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<Screen>>,
    mut done: Local<bool>,
) {
    if let Some(level) = options.level
        && !core::mem::replace(&mut *done, true)
    {
        session.level = level;
        next.set(Screen::Playing);
    }
}

fn title_music(art: Res<Art>, mut audio: ResMut<Audio>) {
    audio.play_music(
        &art.title_music,
        MusicSettings {
            volume: 0.55,
            crossfade: 1.0,
            ..default()
        },
    );
}

// --- Levels ---------------------------------------------------------------------------

/// Spawns the session's level, fresh: full hearts, no coins, no checkpoint.
#[allow(clippy::too_many_arguments)]
fn spawn_level(
    mut commands: Commands,
    art: Res<Art>,
    maps: Res<Assets<TiledMap>>,
    mut session: ResMut<Session>,
    mut bounds: ResMut<LevelBounds>,
    mut audio: ResMut<Audio>,
    mut time: ResMut<Time>,
    camera: Single<&mut Camera2D>,
) {
    let level = session.level.min(LEVELS.len() - 1);
    *session = Session::new(level);
    session.hearts = MAX_HEARTS;
    let info = &LEVELS[level];
    let size = maps
        .get(&art.maps[level])
        .map_or(vec2(640.0, 360.0), TiledMap::size_px);
    bounds.rect = Rect::from_min_size(Vec2::ZERO, size);
    commands.spawn((
        Name::new(format!("level {}", level + 1)),
        Tilemap::new(art.maps[level].clone()),
        LevelMap,
        StateScoped(Screen::Playing),
    ));
    camera.into_inner().clear_color = Color::hex(info.sky);
    audio.play_music(
        &art.music[level],
        MusicSettings {
            volume: 0.55,
            crossfade: 1.0,
            ..default()
        },
    );
    time.set_time_scale(1.0);
    info!("level {}: {}", level + 1, info.map);
}

/// Gives the level's objects their game components by class. When a map is reloaded
/// (hot reload) its objects spawn again and the level starts over.
#[allow(clippy::too_many_arguments)]
fn tag_objects(
    mut commands: Commands,
    art: Res<Art>,
    objects: Query<(Entity, &TiledObject, &Transform2D, &ChildOf), Added<TiledObject>>,
    levels: Query<(), With<LevelMap>>,
    players: Query<Entity, With<player::Player>>,
    camera: Single<Entity, With<Camera2D>>,
    bounds: Res<LevelBounds>,
    scripts: Res<Scripts>,
    mut session: ResMut<Session>,
) {
    // A map that spawns (or respawns after a hot reload) starts the count again.
    let restarted = objects
        .iter()
        .any(|(_, object, ..)| object.class == "PlayerStart");
    if restarted {
        session.coins = 0;
        session.coins_total = 0;
    }
    for (entity, object, transform, parent) in &objects {
        if !levels.contains(parent.parent()) {
            continue;
        }
        let at = transform.translation;
        match object.class.as_str() {
            "PlayerStart" => {
                // The point is on the ground; the body stands on it.
                let start = at + vec2(0.0, player::SIZE.y / 2.0);
                session.start = start;
                session.checkpoint = None;
                for player in &players {
                    commands.entity(player).despawn();
                }
                let body = player::spawn(&mut commands, &art, start);
                commands.entity(*camera).insert(
                    CameraFollow::new(body)
                        .with_dead_zone(vec2(40.0, 56.0))
                        .with_smoothing(0.12)
                        .with_offset(vec2(0.0, 28.0))
                        .with_bounds(bounds.rect),
                );
            }
            "Coin" => {
                session.coins_total += 1;
                things::coin(&mut commands, entity, at);
            }
            // Scripted checkpoints are made by their prefab (see `scripts`).
            "Checkpoint" if !scripts.on() => things::checkpoint(&mut commands, entity),
            "Spring" => things::spring(&mut commands, entity),
            "Exit" => things::exit(&mut commands, entity),
            "Platform" => things::platform(&mut commands, &art, entity, object, at),
            "Walker" | "Spiker" | "Hopper" | "Bat" => {
                enemies::spawn(&mut commands, &art, &scripts, entity, object, at);
            }
            "BotHint" => {
                let size = match &object.shape {
                    ObjectShape::Rect(size) => *size,
                    _ => Vec2::splat(18.0),
                };
                commands.entity(entity).insert(BotHint {
                    action: object
                        .properties
                        .str("action")
                        .unwrap_or_default()
                        .to_owned(),
                    rect: Rect::from_center_size(at, size),
                });
            }
            _ => {}
        }
    }
    if restarted {
        info!(
            "{} coins in level {}",
            session.coins_total,
            session.level + 1
        );
    }
}
