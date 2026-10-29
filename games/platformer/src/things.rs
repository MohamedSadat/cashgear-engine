//! The level's things: coins, checkpoint flags, springs, moving platforms and the exit
//! door. All but the platforms are sensor tile objects; the platforms are rectangle
//! objects with `dx`, `dy` (travel in pixels), `speed` and `phase` properties.

use crate::player::{Player, SIZE};
use crate::progress::{Progress, Session};
use crate::{Art, GameSystems, Screen, tile_rect};
use cashgear_engine::prelude::*;
use cashgear_engine::tiled::ObjectShape;

/// A spring's launch speed: about nine tiles up.
pub(crate) const SPRING_SPEED: f32 = 560.0;
/// The spring tile at rest and stretched.
const SPRING_REST: u32 = 107;
const SPRING_UP: u32 = 108;
/// The wooden beam a platform is drawn with: left end, middle, right end.
const BEAM: [u32; 3] = [48, 49, 50];
/// Seconds from reaching the door to the level-complete screen.
const EXIT_DELAY: f32 = 1.0;
/// A checkpoint flag not reached yet.
const CHECKPOINT_GREY: u32 = 0x8b9bb4;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, move_platforms.in_set(GameSystems::Move))
        .add_systems(
            FixedUpdate,
            (touch_things, leave_level)
                .chain()
                .in_set(GameSystems::Interact),
        )
        .add_systems(Update, (pop_coins, relax_springs, tint_checkpoints));
}

#[derive(Component)]
pub(crate) struct Coin;

#[derive(Component)]
pub(crate) struct Checkpoint {
    pub active: bool,
}

#[derive(Component)]
pub(crate) struct Spring {
    /// Seconds left showing the stretched spring.
    stretched: f32,
}

#[derive(Component)]
pub(crate) struct Exit;

/// A platform moving back and forth between where it starts and `travel` away, easing
/// at the ends.
#[derive(Component, Debug)]
pub(crate) struct Platform {
    pub origin: Vec2,
    pub travel: Vec2,
    /// Seconds for the way there and back.
    pub period: f32,
    /// Where in the period it starts, 0 to 1 (0.5 starts at the far end).
    pub phase: f32,
}

impl Platform {
    /// Where it is `seconds` into the level.
    pub fn at(&self, seconds: f32) -> Vec2 {
        let cycle = (seconds / self.period + self.phase).rem_euclid(1.0);
        let there = 1.0 - (2.0 * cycle - 1.0).abs();
        let eased = 0.5 - 0.5 * (there * core::f32::consts::PI).cos();
        self.origin + self.travel * eased
    }
}

pub(crate) fn coin(commands: &mut Commands, entity: Entity, at: Vec2) {
    commands.entity(entity).insert(Coin).tween(
        Tween::translation(at + vec2(0.0, 2.0), 0.7)
            .from(at)
            .ease(Ease::SineInOut)
            .delay((at.x / 18.0) % 4.0 * 0.15)
            .yoyo()
            .forever(),
    );
}

pub(crate) fn checkpoint(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).insert(Checkpoint { active: false });
}

/// Flags are grey until reached. The flag tile sits on its pole, a tile above the
/// ground, so its sensor reaches down over the pole to where the player stands.
fn tint_checkpoints(
    mut commands: Commands,
    mut flags: Query<(Entity, &Checkpoint, &mut Sprite), Added<Checkpoint>>,
) {
    for (entity, checkpoint, mut sprite) in &mut flags {
        if !checkpoint.active {
            sprite.color = Color::hex(CHECKPOINT_GREY);
        }
        commands.entity(entity).insert((
            Collider::rect(12.0, 36.0).with_offset(vec2(0.0, -9.0)),
            Sensor,
        ));
    }
}

pub(crate) fn spring(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).insert(Spring { stretched: 0.0 });
}

pub(crate) fn exit(commands: &mut Commands, entity: Entity) {
    commands.entity(entity).insert(Exit);
}

/// Makes a rectangle object a moving one-way platform drawn as a wooden beam.
pub(crate) fn platform(
    commands: &mut Commands,
    art: &Art,
    entity: Entity,
    object: &TiledObject,
    at: Vec2,
) {
    let size = match object.shape {
        ObjectShape::Rect(size) => size,
        _ => vec2(54.0, 18.0),
    };
    let props = &object.properties;
    let travel = vec2(
        props.float("dx").unwrap_or(0.0),
        // Tiled's y points down.
        -props.float("dy").unwrap_or(0.0),
    );
    let speed = props.float("speed").unwrap_or(40.0).max(1.0);
    let platform = Platform {
        origin: at,
        travel,
        period: (2.0 * travel.length() / speed).max(0.1),
        phase: props.float("phase").unwrap_or(0.0),
    };
    let start = platform.at(0.0);
    commands.entity(entity).insert((
        platform,
        Transform2D::from_translation(start),
        Interpolated::default(),
        // The beam's top eight pixels carry the player.
        Collider::rect(size.x, 8.0).with_offset(vec2(0.0, size.y / 2.0 - 4.0)),
        OneWay,
    ));
    let tiles = (size.x / 18.0).round().max(1.0) as usize;
    for i in 0..tiles {
        let tile = match i {
            0 if tiles > 1 => BEAM[0],
            i if i + 1 == tiles && tiles > 1 => BEAM[2],
            _ => BEAM[1],
        };
        commands.spawn((
            Sprite {
                source: Some(tile_rect(tile)),
                ..Sprite::new(art.tiles.clone())
            },
            Transform2D::from_xy((i as f32 + 0.5) * 18.0 - size.x / 2.0, 0.0),
            Layer(4),
            ChildOf(entity),
        ));
    }
}

/// Moves the platforms along their paths on the level's clock, so every run of a level
/// sees them in the same places at the same moments.
fn move_platforms(
    time: Res<Time>,
    session: Res<Session>,
    mut platforms: Query<(&Platform, &mut Transform2D)>,
) {
    let seconds = session.steps as f32 * time.fixed_delta_secs();
    for (platform, mut transform) in &mut platforms {
        let at = platform.at(seconds);
        if transform.translation != at {
            transform.translation = at;
        }
    }
}

/// Coins, checkpoints, springs and the exit, touched in the latest physics step.
#[allow(clippy::too_many_arguments)]
fn touch_things(
    mut commands: Commands,
    mut started: MessageReader<CollisionStarted>,
    art: Res<Art>,
    mut audio: ResMut<Audio>,
    mut session: ResMut<Session>,
    player: Single<(Entity, &mut Player, &mut KinematicBody)>,
    coins: Query<&GlobalTransform2D, (With<Coin>, Without<Player>)>,
    mut checkpoints: Query<(&mut Checkpoint, &GlobalTransform2D), Without<Player>>,
    mut springs: Query<(&mut Spring, &mut Sprite), Without<Player>>,
    exits: Query<&GlobalTransform2D, (With<Exit>, Without<Player>)>,
) {
    let (entity, mut player, mut body) = player.into_inner();
    if player.dying > 0.0 || player.dead {
        return;
    }
    for collision in started.read() {
        let Some(other) = collision.other(entity) else {
            continue;
        };
        if let Ok(coin) = coins.get(other) {
            session.coins += 1;
            audio.play_with(
                &art.sounds.coin,
                PlaySettings::new()
                    .at(coin.translation())
                    .with_volume(0.5)
                    .with_pitch_jitter(0.3),
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
        } else if let Ok((mut checkpoint, flag)) = checkpoints.get_mut(other) {
            if checkpoint.active {
                continue;
            }
            checkpoint.active = true;
            // The flag is one tile above the ground; come back standing under it.
            let at = flag.translation() - vec2(0.0, 18.0 - SIZE.y / 2.0 + 9.0);
            session.checkpoint = Some(at);
            commands
                .entity(other)
                .tween(Tween::color(Color::WHITE, 0.3))
                .tween(
                    Tween::scale(Vec2::ONE, 0.4)
                        .from(vec2(1.4, 1.4))
                        .ease(Ease::BackOut),
                )
                .insert(ParticleEmitter::new(ParticleSettings {
                    rate: 0.0,
                    burst: 24,
                    lifetime: (0.4, 0.8),
                    speed: (40.0, 110.0),
                    spread: 360.0,
                    gravity: vec2(0.0, -200.0),
                    size: vec![3.0, 2.0],
                    color: vec![Color::hex(0xffcd75), Color::hex(0xef7d57).with_alpha(0.0)],
                    ..default()
                }));
            audio.play_with(&art.sounds.checkpoint, PlaySettings::new().with_volume(0.7));
            info!("checkpoint at {at}");
        } else if let Ok((mut spring, mut sprite)) = springs.get_mut(other) {
            body.velocity.y = SPRING_SPEED;
            player.set_jumping(false);
            spring.stretched = 0.25;
            sprite.source = Some(tile_rect(SPRING_UP));
            audio.play_with(&art.sounds.spring, PlaySettings::new().with_volume(0.7));
        } else if exits.contains(other) && session.finished.is_none() {
            session.finished = Some(session.time);
            body.velocity.x = 0.0;
            // In through the door.
            commands
                .entity(player.sprite)
                .tween(Tween::color(Color::WHITE.with_alpha(0.0), 0.5).delay(0.2));
            audio.stop_music(0.5);
            audio.play_with(&art.sounds.goal, PlaySettings::new());
            info!(
                "level {} done in {:.1} s with {} of {} coins",
                session.level + 1,
                session.time,
                session.coins,
                session.coins_total
            );
        }
    }
}

/// After the door: a moment to walk in, then the progress is saved and the level-complete
/// screen opens.
fn leave_level(
    time: Res<Time>,
    storage: Res<Storage>,
    options: Res<crate::Options>,
    mut progress: ResMut<Progress>,
    mut session: ResMut<Session>,
    mut next: ResMut<NextState<Screen>>,
    mut waited: Local<f32>,
) {
    if session.finished.is_none() {
        *waited = 0.0;
        return;
    }
    *waited += time.fixed_delta_secs();
    if *waited < EXIT_DELAY || *waited - time.fixed_delta_secs() >= EXIT_DELAY {
        return;
    }
    let time_taken = session.time;
    session.new_best = progress.finish(
        session.level,
        session.coins,
        session.coins_total,
        time_taken,
    );
    if !options.demo {
        progress.save(&storage);
    }
    next.push(Screen::Complete);
}

/// Removes coins once their pop has played.
fn pop_coins(mut commands: Commands, mut finished: MessageReader<TweenFinished>) {
    for tween in finished.read() {
        if tween.tag.as_deref() == Some("picked") {
            commands.entity(tween.entity).despawn();
        }
    }
}

/// Springs go back to rest a moment after a bounce.
fn relax_springs(time: Res<Time>, mut springs: Query<(&mut Spring, &mut Sprite)>) {
    for (mut spring, mut sprite) in &mut springs {
        if spring.stretched > 0.0 {
            spring.stretched -= time.delta_secs();
            if spring.stretched <= 0.0 {
                sprite.source = Some(tile_rect(SPRING_REST));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platforms_ease_there_and_back() {
        let platform = Platform {
            origin: vec2(100.0, 50.0),
            travel: vec2(90.0, 0.0),
            period: 4.0,
            phase: 0.0,
        };
        assert_eq!(platform.at(0.0), vec2(100.0, 50.0));
        assert!((platform.at(2.0) - vec2(190.0, 50.0)).length() < 1e-3);
        assert!((platform.at(1.0) - vec2(145.0, 50.0)).length() < 1e-3);
        assert!((platform.at(4.0) - vec2(100.0, 50.0)).length() < 1e-3);
        let late = Platform {
            phase: 0.5,
            ..platform
        };
        assert!((late.at(0.0) - vec2(190.0, 50.0)).length() < 1e-3);
    }
}
