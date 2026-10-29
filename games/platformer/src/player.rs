//! The player: Kenney's green alien on a kinematic body. Runs with acceleration, jumps
//! higher while the button is held, forgives late jumps off ledges (coyote time) and
//! early presses before landing (jump buffer), slides down walls and jumps off them.
//! Takes hits (a heart each, with a knock back), falls into pits (a heart, back to the
//! checkpoint), and ends the level when the hearts run out.

use crate::level::LevelBounds;
use crate::progress::Session;
use crate::{Art, GameSystems, Screen, move_towards};
use cashgear_engine::prelude::*;

/// The body's size.
pub(crate) const SIZE: Vec2 = Vec2::new(12.0, 18.0);
/// Draw layer: above the map and the enemies.
const LAYER: i32 = 10;
/// Units per second, and per second squared to get there on the ground and in the air.
pub(crate) const RUN_SPEED: f32 = 125.0;
const GROUND_ACCEL: f32 = 1100.0;
const AIR_ACCEL: f32 = 760.0;
/// Take-off speed of a jump: about three tiles high.
pub(crate) const JUMP_SPEED: f32 = 340.0;
/// Rising speed is multiplied by this when the button is let go during a jump.
const JUMP_CUT: f32 = 0.5;
const MAX_FALL: f32 = 480.0;
/// Seconds after leaving a ledge that a jump still works...
const COYOTE: f32 = 0.1;
/// ...and seconds a press before landing is remembered.
const BUFFER: f32 = 0.12;
/// The fastest slide down a wall.
const WALL_SLIDE: f32 = 70.0;
/// A wall jump's kick away from the wall and up.
pub(crate) const WALL_JUMP: Vec2 = Vec2::new(165.0, 320.0);
/// Seconds after a wall jump before the controls steer again.
const WALL_LOCK: f32 = 0.16;
/// Seconds after touching a wall that a wall jump still works.
const WALL_COYOTE: f32 = 0.08;
/// Thrown back by a hit, and for how long without control and without further hits.
const KNOCKBACK: Vec2 = Vec2::new(150.0, 230.0);
const STUNNED: f32 = 0.3;
const INVINCIBLE: f32 = 1.4;
/// Seconds between falling out of the level and coming back at the checkpoint.
const RESPAWN: f32 = 0.7;
/// Seconds between losing the last heart and the game over screen.
const GAME_OVER: f32 = 1.2;

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Intent>()
        .add_message::<Hurt>()
        .add_systems(
            FixedUpdate,
            read_controls
                .in_set(GameSystems::Input)
                .run_if(|options: Res<crate::Options>| !options.demo),
        )
        .add_systems(FixedUpdate, control.in_set(GameSystems::Move))
        .add_systems(
            FixedUpdate,
            (hazards, take_hits, fall_and_respawn)
                .chain()
                .in_set(GameSystems::Interact)
                .after(crate::enemies::touch_enemies),
        )
        .add_systems(
            FixedUpdate,
            (feed_animator, footsteps).in_set(GameSystems::Animate),
        );
}

/// What the player wants in this fixed step, from the controls or from the bot.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Intent {
    /// -1 to 1.
    pub run: f32,
    /// Jump was pressed in this step.
    pub jump: bool,
    pub jump_held: bool,
    pub down: bool,
}

/// Something hurt the player; `from` is its x, to knock the player away from it.
#[derive(Message, Debug, Clone, Copy)]
pub(crate) struct Hurt {
    pub from: f32,
}

/// The player's state. The picture and the dust are children, so squashing the picture
/// leaves the collider alone.
#[derive(Component, Debug)]
pub(crate) struct Player {
    pub facing: f32,
    /// The entity with the `SpriteAnimation` and `Animator`.
    pub sprite: Entity,
    /// The dust emitter at the feet.
    pub dust: Entity,
    coyote: f32,
    buffer: f32,
    /// The side of the wall touched last (-1 left, 1 right), and how long ago.
    wall: f32,
    wall_time: f32,
    /// Seconds left before the controls steer again after a wall jump.
    lock: f32,
    /// In a jump the button can still cut short.
    jumping: bool,
    /// The fastest fall since standing on something.
    fall_speed: f32,
    pub sliding: bool,
    pub stunned: f32,
    pub invincible: f32,
    /// Seconds until coming back after a fall, or until the game over screen.
    pub dying: f32,
    /// Out of hearts: the level is over when `dying` runs out.
    pub dead: bool,
}

impl Player {
    /// Lets the next jump be cut short (stomp bounces with the button held).
    pub fn set_jumping(&mut self, jumping: bool) {
        self.jumping = jumping;
        self.coyote = 0.0;
    }

    /// Whether the player can be touched: not respawning, dead or invincible.
    pub fn is_vulnerable(&self) -> bool {
        self.dying <= 0.0 && !self.dead && self.invincible <= 0.0
    }
}

/// Spawns the player at `at` (the centre of its body).
pub(crate) fn spawn(commands: &mut Commands, art: &Art, at: Vec2) -> Entity {
    let feet = vec2(0.0, -SIZE.y / 2.0);
    let body = commands
        .spawn((
            Name::new("player"),
            KinematicBody {
                max_fall_speed: MAX_FALL,
                // Walks up the lip of a platform that is not quite level with the ground.
                step_height: 4.0,
                ..default()
            },
            Collider::rect(SIZE.x, SIZE.y),
            CollisionLayers::new(crate::enemies::PLAYER_LAYER, CollisionLayers::ALL_BITS),
            Transform2D::from_translation(at),
            StateScoped(Screen::Playing),
        ))
        .id();
    let sprite = commands
        .spawn((
            Name::new("player picture"),
            SpriteAnimation::new(art.alien.clone(), "idle").with_pivot(FEET),
            Animator::new(art.graph.clone()),
            Layer(LAYER),
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
        coyote: 0.0,
        buffer: 0.0,
        wall: 0.0,
        wall_time: 0.0,
        lock: 0.0,
        jumping: false,
        fall_speed: 0.0,
        sliding: false,
        stunned: 0.0,
        invincible: 0.0,
        dying: 0.0,
        dead: false,
    });
    body
}

fn read_controls(actions: Res<ActionState>, mut intent: ResMut<Intent>) {
    *intent = Intent {
        run: actions.axis("left", "right"),
        jump: actions.just_pressed("jump"),
        jump_held: actions.pressed("jump"),
        down: actions.pressed("down"),
    };
}

/// Turns the intent into velocity: running, jumps, wall slides and wall jumps, with
/// take-off and landing effects.
#[allow(clippy::too_many_arguments)]
fn control(
    mut commands: Commands,
    time: Res<Time>,
    intent: Res<Intent>,
    session: Res<Session>,
    art: Res<Art>,
    mut audio: ResMut<Audio>,
    player: Single<(&mut Player, &mut KinematicBody, &KinematicContacts)>,
    mut sprites: Query<&mut Sprite>,
    mut emitters: Query<&mut ParticleEmitter>,
    mut shake: Single<&mut CameraShake>,
) {
    let (mut player, mut body, contacts) = player.into_inner();
    // At the door the player stops listening.
    let intent = if session.finished.is_some() {
        Intent::default()
    } else {
        *intent
    };
    let dt = time.fixed_delta_secs();
    player.stunned = (player.stunned - dt).max(0.0);
    player.invincible = (player.invincible - dt).max(0.0);
    if player.dying > 0.0 || player.dead {
        return;
    }
    let grounded = contacts.on_ground();

    if contacts.just_landed() {
        // Squash, dust and a thud in proportion to the fall; shake only for long ones.
        let speed = player.fall_speed;
        let strength = (speed / MAX_FALL).clamp(0.3, 1.0);
        commands.entity(player.sprite).tween(
            Tween::scale(Vec2::ONE, 0.3)
                .from(vec2(1.0 + 0.35 * strength, 1.0 - 0.3 * strength))
                .ease(Ease::BackOut),
        );
        if let Ok(mut dust) = emitters.get_mut(player.dust) {
            dust.emit(4 + (speed / 40.0) as u32);
        }
        if speed > 380.0 {
            shake.add_trauma(((speed - 380.0) / 300.0).min(0.5));
        }
        audio.play_with(
            &art.sounds.land,
            PlaySettings::new()
                .with_volume(0.3 + 0.6 * strength)
                .with_pitch_jitter(1.0),
        );
        player.jumping = false;
    }
    if grounded {
        player.fall_speed = 0.0;
        player.coyote = COYOTE;
    } else {
        player.fall_speed = player.fall_speed.max(-body.velocity.y);
        player.coyote -= dt;
    }
    if intent.jump {
        player.buffer = BUFFER;
    } else {
        player.buffer -= dt;
    }
    let wall = if contacts.wall_left.is_some() {
        -1.0
    } else if contacts.wall_right.is_some() {
        1.0
    } else {
        0.0
    };
    if wall != 0.0 && !grounded {
        player.wall = wall;
        player.wall_time = WALL_COYOTE;
    } else {
        player.wall_time -= dt;
    }
    player.lock -= dt;
    if player.stunned > 0.0 {
        player.sliding = false;
        return;
    }

    // Running.
    if player.lock <= 0.0 {
        let target = intent.run * RUN_SPEED;
        let accel = if grounded { GROUND_ACCEL } else { AIR_ACCEL };
        body.velocity.x = move_towards(body.velocity.x, target, accel * dt);
        if intent.run.abs() > 0.1 {
            player.facing = intent.run.signum();
        }
    }

    // Sliding down a wall while pushing into it.
    player.sliding = !grounded && wall != 0.0 && intent.run * wall > 0.5 && body.velocity.y < 0.0;
    if player.sliding {
        body.velocity.y = body.velocity.y.max(-WALL_SLIDE);
        player.facing = wall;
        if time.fixed_step_count().is_multiple_of(8)
            && let Ok(mut dust) = emitters.get_mut(player.dust)
        {
            dust.emit(1);
        }
    }

    // Jumps: from the ground (or just after leaving it), down through a plank, or off a
    // wall.
    if player.buffer > 0.0 {
        let jumped = if player.coyote > 0.0 {
            if intent.down && grounded {
                body.drop_through = true;
                false
            } else {
                body.velocity.y = JUMP_SPEED;
                true
            }
        } else if player.wall_time > 0.0 && !grounded {
            body.velocity = vec2(-player.wall * WALL_JUMP.x, WALL_JUMP.y);
            player.facing = -player.wall;
            player.lock = WALL_LOCK;
            player.wall_time = 0.0;
            true
        } else {
            false
        };
        if jumped {
            player.buffer = 0.0;
            player.coyote = 0.0;
            player.jumping = true;
            commands.entity(player.sprite).tween(
                Tween::scale(Vec2::ONE, 0.25)
                    .from(vec2(0.8, 1.25))
                    .ease(Ease::QuadOut),
            );
            if let Ok(mut dust) = emitters.get_mut(player.dust) {
                dust.emit(5);
            }
            audio.play_with(
                &art.sounds.jump,
                PlaySettings::new().with_volume(0.45).with_pitch_jitter(0.5),
            );
        }
    }
    if player.jumping && body.velocity.y > 0.0 && !intent.jump_held {
        body.velocity.y *= JUMP_CUT;
        player.jumping = false;
    }
    if let Ok(mut sprite) = sprites.get_mut(player.sprite) {
        let flip = player.facing < 0.0;
        if sprite.flip_x != flip {
            sprite.flip_x = flip;
        }
    }
}

/// Spikes and other hazard tiles hurt.
fn hazards(
    player: Single<(&Player, &KinematicContacts, &GlobalTransform2D)>,
    mut hurt: MessageWriter<Hurt>,
) {
    let (player, contacts, transform) = *player;
    if contacts.hazard && player.is_vulnerable() {
        hurt.write(Hurt {
            from: transform.translation().x - player.facing,
        });
    }
}

/// A hit costs a heart and knocks the player back with a flash; the last heart ends the
/// level.
#[allow(clippy::too_many_arguments)]
fn take_hits(
    mut commands: Commands,
    mut hits: MessageReader<Hurt>,
    art: Res<Art>,
    mut audio: ResMut<Audio>,
    mut session: ResMut<Session>,
    player: Single<(&mut Player, &mut KinematicBody, &GlobalTransform2D)>,
    mut animators: Query<&mut Animator>,
    mut shake: Single<&mut CameraShake>,
) {
    let (mut player, mut body, transform) = player.into_inner();
    let Some(hit) = hits.read().last().copied() else {
        return;
    };
    if !player.is_vulnerable() || session.finished.is_some() {
        return;
    }
    session.hearts = session.hearts.saturating_sub(1);
    session.hits += 1;
    let away = match transform.translation().x - hit.from {
        dx if dx.abs() < 0.5 => -player.facing,
        dx => dx.signum(),
    };
    body.velocity = vec2(away * KNOCKBACK.x, KNOCKBACK.y);
    player.stunned = STUNNED;
    player.invincible = INVINCIBLE;
    player.jumping = false;
    shake.add_trauma(0.55);
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
    audio.play_with(&art.sounds.hit, PlaySettings::new().with_volume(0.7));
    info!("hit: {} hearts left", session.hearts);
    if session.hearts == 0 {
        player.dead = true;
        player.dying = GAME_OVER;
        audio.play_with(&art.sounds.game_over, PlaySettings::new().with_volume(0.55));
        audio.stop_music(0.8);
    }
}

/// Falling out of the level costs a heart and brings the player back at the checkpoint
/// behind a quick fade; when the hearts run out (by falls or hits) the game over screen
/// opens.
#[allow(clippy::too_many_arguments)]
fn fall_and_respawn(
    time: Res<Time>,
    art: Res<Art>,
    bounds: Res<LevelBounds>,
    mut audio: ResMut<Audio>,
    mut session: ResMut<Session>,
    mut fade: ResMut<ScreenFade>,
    mut next: ResMut<NextState<Screen>>,
    player: Single<(
        &mut Player,
        &mut KinematicBody,
        &mut Transform2D,
        &mut Interpolated,
    )>,
    mut animators: Query<&mut Animator>,
    mut follow: Single<&mut CameraFollow>,
) {
    let (mut player, mut body, mut transform, mut interpolated) = player.into_inner();
    let dt = time.fixed_delta_secs();
    if player.dying <= 0.0 && !player.dead && bounds.fell(transform.translation.y) {
        session.hearts = session.hearts.saturating_sub(1);
        session.falls += 1;
        audio.play_with(&art.sounds.fall, PlaySettings::new().with_volume(0.35));
        info!("fell: {} hearts left", session.hearts);
        body.velocity = Vec2::ZERO;
        body.gravity_scale = 0.0;
        if session.hearts == 0 {
            player.dead = true;
            player.dying = GAME_OVER;
            audio.play_with(&art.sounds.game_over, PlaySettings::new().with_volume(0.55));
            audio.stop_music(0.8);
        } else {
            player.dying = RESPAWN;
            fade.fade_out(RESPAWN * 0.5);
        }
        return;
    }
    if player.dying <= 0.0 {
        return;
    }
    player.dying -= dt;
    if player.dying > 0.0 {
        return;
    }
    if player.dead {
        info!("game over in level {}", session.level + 1);
        next.push(Screen::GameOver);
        return;
    }
    transform.translation = session.respawn_point();
    interpolated.reset();
    body.velocity = Vec2::ZERO;
    body.gravity_scale = 1.0;
    player.invincible = INVINCIBLE;
    player.fall_speed = 0.0;
    follow.snap();
    fade.fade_in(0.35);
    if let Ok(mut animator) = animators.get_mut(player.sprite) {
        animator.force("idle");
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
        animator.set_bool("wall", player.sliding);
    }
}

/// A step sound and a puff of dust at each footstep of the run clip.
fn footsteps(
    mut events: MessageReader<AnimationEvent>,
    art: Res<Art>,
    mut audio: ResMut<Audio>,
    player: Single<&Player>,
    mut emitters: Query<&mut ParticleEmitter>,
) {
    for event in events.read() {
        if event.entity == player.sprite && event.name == "footstep" {
            audio.play_with(
                &art.sounds.step,
                PlaySettings::new().with_volume(0.4).with_pitch_jitter(2.0),
            );
            if let Ok(mut dust) = emitters.get_mut(player.dust) {
                dust.emit(2);
            }
        }
    }
}
