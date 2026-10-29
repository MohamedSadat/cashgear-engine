//! Enemies, placed in Tiled as tile objects of the character tileset: walkers patrol and
//! turn at walls, ledges and spikes; spikers do the same but cannot be stomped; hoppers
//! jump towards the player; bats fly back and forth. Landing on an enemy from above
//! stomps it (spikers excepted); any other touch hurts.

use crate::level::LevelBounds;
use crate::player::{Hurt, Intent, Player, SIZE};
use crate::progress::Session;
use crate::{Art, GameSystems};
use cashgear_engine::anim::SheetFrame;
use cashgear_engine::prelude::*;

/// Collision layers: the player and the enemies each on their own, so enemies can ignore
/// one another.
pub(crate) const PLAYER_LAYER: u32 = 1 << 1;
const ENEMY_LAYER: u32 = 1 << 2;
/// The tiles' and static colliders' layer.
const WORLD_LAYER: u32 = CollisionLayers::DEFAULT.memberships;
/// Draw layer: above the map, below the player.
const LAYER: i32 = 8;
/// Bounce off a stomped enemy, higher with jump held.
const STOMP_BOUNCE: f32 = 250.0;
const STOMP_BOUNCE_HELD: f32 = 360.0;
const HOP: Vec2 = Vec2::new(60.0, 290.0);
/// Seconds between hops.
const HOP_EVERY: f32 = 1.3;
/// Hoppers turn towards a player this close.
const HOP_SIGHT: f32 = 180.0;

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(FixedUpdate, think.in_set(GameSystems::Move))
        .add_systems(FixedUpdate, touch_enemies.in_set(GameSystems::Interact))
        .add_systems(Update, remove_dead);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    Walker,
    Spiker,
    Hopper,
    Bat,
}

#[derive(Component, Debug)]
pub(crate) struct Enemy {
    pub kind: Kind,
    /// -1 left, 1 right.
    dir: f32,
    speed: f32,
    timer: f32,
    /// Where it started, and how far it goes either way: a bat's flight, a hopper's patch,
    /// a walker's beat (unlimited by default: walls, ledges and spikes turn it).
    home: Vec2,
    range: f32,
    pub dead: bool,
}

/// The enemies' clips, cut from Kenney's 24-pixel character sheet.
pub(crate) fn sheet(characters: Handle<Image>) -> SpriteSheet {
    let mut sheet = SpriteSheet::new(characters);
    for id in [13u32, 14, 15, 16, 18, 19, 20, 24, 25, 26] {
        let (x, y) = ((id % 9) as f32 * 24.0, (id / 9) as f32 * 24.0);
        sheet.add_frame(
            format!("c{id}"),
            SheetFrame::new(Rect::new(x, y, x + 24.0, y + 24.0), 0.2),
        );
    }
    for (name, frames, fps) in [
        ("walker", &["c18", "c19"][..], 5.0),
        ("walker_dead", &["c20"], 1.0),
        ("spiker", &["c15", "c16"], 4.0),
        ("hopper", &["c13"], 1.0),
        ("hopper_jump", &["c14"], 1.0),
        ("bat", &["c24", "c25", "c26", "c25"], 10.0),
    ] {
        if let Ok(clip) = sheet.clip_from_frames(frames, fps) {
            sheet.add_clip(name, clip);
        }
    }
    sheet
}

/// Makes a map object an enemy of its class.
pub(crate) fn spawn(
    commands: &mut Commands,
    art: &Art,
    entity: Entity,
    object: &TiledObject,
    at: Vec2,
) {
    let props = &object.properties;
    let (kind, speed, clip) = match object.class.as_str() {
        "Spiker" => (Kind::Spiker, 22.0, "spiker"),
        "Hopper" => (Kind::Hopper, HOP.x, "hopper"),
        "Bat" => (Kind::Bat, 40.0, "bat"),
        _ => (Kind::Walker, 28.0, "walker"),
    };
    let enemy = Enemy {
        kind,
        dir: props.float("dir").unwrap_or(-1.0).signum(),
        speed: props.float("speed").unwrap_or(speed),
        timer: HOP_EVERY * 0.5,
        home: at,
        range: props
            .float("range")
            .unwrap_or(match kind {
                Kind::Walker | Kind::Spiker => f32::INFINITY,
                _ => 48.0,
            })
            .max(1.0),
        dead: false,
    };
    let (body, collider) = if kind == Kind::Bat {
        (
            KinematicBody::default().with_gravity_scale(0.0),
            Collider::rect(16.0, 10.0),
        )
    } else {
        // The feet sit at the bottom of the 24-pixel cell, the object's bottom.
        (
            KinematicBody {
                max_fall_speed: 480.0,
                ..default()
            },
            Collider::rect(14.0, 12.0).with_offset(vec2(0.0, -6.0)),
        )
    };
    commands.entity(entity).insert((
        enemy,
        body,
        collider,
        CollisionLayers::new(ENEMY_LAYER, WORLD_LAYER | PLAYER_LAYER),
        SpriteAnimation::new(art.enemies.clone(), clip),
        Layer(LAYER),
    ));
}

type Brain = (
    Entity,
    &'static mut Enemy,
    &'static mut KinematicBody,
    &'static KinematicContacts,
    &'static GlobalTransform2D,
    &'static mut Sprite,
    &'static mut SpriteAnimation,
);

/// Each enemy's behaviour for this step.
fn think(
    time: Res<Time>,
    session: Res<Session>,
    bounds: Res<LevelBounds>,
    spatial: SpatialQuery,
    player: Option<Single<&GlobalTransform2D, With<Player>>>,
    mut enemies: Query<Brain>,
    mut commands: Commands,
) {
    let dt = time.fixed_delta_secs();
    let player_at = player.map(|p| p.translation());
    for (entity, mut enemy, mut body, contacts, transform, mut sprite, mut animation) in
        &mut enemies
    {
        if enemy.dead {
            continue;
        }
        let at = transform.translation();
        if bounds.fell(at.y) {
            commands.entity(entity).despawn();
            continue;
        }
        let grounded = contacts.on_ground();
        let solid = QueryFilter::default()
            .exclude(entity)
            .without_sensors()
            .with_mask(WORLD_LAYER);
        // Is there floor (and no spikes) a little ahead of the feet?
        let floor_ahead = |dir: f32| {
            let foot = vec2(at.x + dir * 9.0, at.y - 12.0);
            let floor = spatial
                .raycast(foot + vec2(0.0, 2.0), Vec2::NEG_Y, 10.0, solid)
                .is_some();
            let spikes = spatial
                .tile_at(foot + vec2(0.0, 6.0), solid)
                .is_some_and(|hit| hit.tile.hazard);
            floor && !spikes
        };
        match enemy.kind {
            Kind::Walker | Kind::Spiker => {
                let blocked = (enemy.dir < 0.0 && contacts.wall_left.is_some())
                    || (enemy.dir > 0.0 && contacts.wall_right.is_some());
                let far = (at.x - enemy.home.x) * enemy.dir > enemy.range;
                if blocked || far || (grounded && !floor_ahead(enemy.dir)) {
                    enemy.dir = -enemy.dir;
                }
                body.velocity.x = enemy.dir * enemy.speed;
            }
            Kind::Hopper => {
                if grounded {
                    body.velocity.x = 0.0;
                    enemy.timer -= dt;
                    if enemy.timer <= 0.0 {
                        enemy.timer = HOP_EVERY;
                        if let Some(target) = player_at
                            && (target - at).length() < HOP_SIGHT
                        {
                            enemy.dir = (target.x - at.x).signum();
                        }
                        // Hoppers keep near home, and off ledges.
                        if (at.x - enemy.home.x) * enemy.dir > enemy.range
                            || !floor_ahead(enemy.dir * 3.0)
                        {
                            enemy.dir = -enemy.dir;
                        }
                        body.velocity = vec2(enemy.dir * enemy.speed, HOP.y);
                    }
                } else if contacts.on_wall() {
                    enemy.dir = -enemy.dir;
                    body.velocity.x = enemy.dir * enemy.speed;
                }
                animation.play(if grounded { "hopper" } else { "hopper_jump" });
            }
            Kind::Bat => {
                // Back and forth around home, bobbing, on the level's clock.
                let t = session.steps as f32 * dt;
                let w = enemy.speed / enemy.range;
                let target =
                    |t: f32| enemy.home + vec2(enemy.range * (w * t).sin(), 6.0 * (t * 3.0).sin());
                body.velocity = (target(t + dt) - at) / dt;
                enemy.dir = (w * t).cos().signum();
            }
        }
        // Kenney's characters face left.
        let flip = enemy.dir > 0.0;
        if sprite.flip_x != flip {
            sprite.flip_x = flip;
        }
    }
}

/// Stomps (falling onto an enemy from above) and hits (any other touch), from the
/// overlaps of the latest physics step.
#[allow(clippy::too_many_arguments)]
pub(crate) fn touch_enemies(
    mut commands: Commands,
    overlaps: Res<Overlaps>,
    intent: Res<Intent>,
    art: Res<Art>,
    mut audio: ResMut<Audio>,
    mut session: ResMut<Session>,
    player: Single<(Entity, &mut Player, &mut KinematicBody, &GlobalTransform2D)>,
    mut enemies: Query<
        (
            &mut Enemy,
            &GlobalTransform2D,
            &Collider,
            &mut SpriteAnimation,
        ),
        Without<Player>,
    >,
    mut hurt: MessageWriter<Hurt>,
    mut shake: Single<&mut CameraShake>,
) {
    let (entity, mut player, mut body, transform) = player.into_inner();
    if player.dying > 0.0 || player.dead {
        return;
    }
    let bottom = transform.translation().y - SIZE.y / 2.0;
    let touching: Vec<Entity> = overlaps.with(entity).collect();
    for other in touching {
        let Ok((mut enemy, enemy_transform, collider, mut animation)) = enemies.get_mut(other)
        else {
            continue;
        };
        if enemy.dead {
            continue;
        }
        let top = collider.world_shape(enemy_transform.0).aabb().max.y;
        let stomp = enemy.kind != Kind::Spiker && body.velocity.y < 0.0 && bottom > top - 8.0;
        if stomp {
            enemy.dead = true;
            session.stomps += 1;
            body.velocity.y = if intent.jump_held {
                STOMP_BOUNCE_HELD
            } else {
                STOMP_BOUNCE
            };
            player.set_jumping(intent.jump_held);
            if enemy.kind == Kind::Walker {
                animation.play("walker_dead");
            }
            commands
                .entity(other)
                .remove::<(KinematicBody, Collider)>()
                .tween(Tween::scale(vec2(1.2, 0.35), 0.15).ease(Ease::QuadOut))
                .tween(
                    Tween::color(Color::WHITE.with_alpha(0.0), 0.4)
                        .delay(0.25)
                        .tag("enemy dead"),
                );
            shake.add_trauma(0.2);
            audio.play_with(
                &art.sounds.stomp,
                PlaySettings::new()
                    .at(enemy_transform.translation())
                    .with_pitch_jitter(1.0),
            );
        } else if player.is_vulnerable() {
            hurt.write(Hurt {
                from: enemy_transform.translation().x,
            });
        }
    }
}

/// Removes stomped enemies once they have faded.
fn remove_dead(mut commands: Commands, mut finished: MessageReader<TweenFinished>) {
    for tween in finished.read() {
        if tween.tag.as_deref() == Some("enemy dead") {
            commands.entity(tween.entity).despawn();
        }
    }
}
