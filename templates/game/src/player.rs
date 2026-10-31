//! The player: a kinematic body that runs and jumps, with Kenney's alien drawn by its
//! sprite sheet and animation state machine (`assets/anims/alien.anim.ron`).
//!
//! Gameplay runs in fixed steps (`FixedUpdate`), 60 a second on every machine; the
//! renderer blends between steps, so movement stays smooth on fast screens.

use crate::level::LevelAssets;
use crate::{GameSystems, Options};
use cashgear_engine::prelude::*;

/// The player's collision box.
pub const SIZE: Vec2 = Vec2::new(12.0, 18.0);
/// Top running speed, in pixels a second, and how fast it is reached on the ground and in
/// the air.
const RUN_SPEED: f32 = 125.0;
const GROUND_ACCEL: f32 = 1100.0;
const AIR_ACCEL: f32 = 760.0;
/// The jump's starting speed; letting go of jump early cuts it, for low hops.
const JUMP_SPEED: f32 = 340.0;
/// A jump still works this long after running off a ledge (coyote time)...
const COYOTE: f32 = 0.1;
/// ...and a press this long before landing jumps on landing (a jump buffer).
const BUFFER: f32 = 0.12;
const MAX_FALL: f32 = 480.0;

/// The player, and its picture: a child entity, so it can flip and squash on its own.
#[derive(Component, Debug)]
pub struct Player {
    /// The picture's entity.
    pub sprite: Entity,
    /// 1 facing right, -1 left.
    pub facing: f32,
    coyote: f32,
    buffer: f32,
}

/// What the player wants this step: from the controls, or from the bot in `--demo`.
#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct Intent {
    /// -1 to run left, 1 right, 0 to stop.
    pub run: f32,
    /// Jump was pressed since the last step.
    pub jump: bool,
    /// Jump is held (a released jump falls short).
    pub jump_held: bool,
}

/// The controls: keys, a gamepad and the on-screen buttons (see `screens.rs`).
// ANCHOR: controls
pub fn action_map() -> ActionMap {
    use AxisDirection::{Negative, Positive};
    ActionMap::new()
        .bind("left", KeyCode::ArrowLeft)
        .bind("left", KeyCode::KeyA)
        .bind("left", GamepadButton::DPadLeft)
        .bind("left", Binding::Axis(GamepadAxis::LeftStickX, Negative))
        .bind("right", KeyCode::ArrowRight)
        .bind("right", KeyCode::KeyD)
        .bind("right", GamepadButton::DPadRight)
        .bind("right", Binding::Axis(GamepadAxis::LeftStickX, Positive))
        .bind("jump", KeyCode::Space)
        .bind("jump", KeyCode::ArrowUp)
        .bind("jump", KeyCode::KeyW)
        .bind("jump", GamepadButton::South)
}
// ANCHOR_END: controls

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Intent>()
        .add_systems(
            FixedUpdate,
            read_controls
                .in_set(GameSystems::Input)
                .run_if(|options: Res<Options>| !options.demo),
        )
        .add_systems(FixedUpdate, run_and_jump.in_set(GameSystems::Move));
}

/// Spawns the player at `at` (the body's centre). The camera follows it (`screens.rs`).
// ANCHOR: spawn
pub fn spawn(commands: &mut Commands, assets: &LevelAssets, at: Vec2) -> Entity {
    let body = commands
        .spawn((
            Name::new("player"),
            KinematicBody {
                max_fall_speed: MAX_FALL,
                ..default()
            },
            Collider::rect(SIZE.x, SIZE.y),
            Transform2D::from_translation(at),
            StateScoped(crate::Screen::Playing),
        ))
        .id();
    let sprite = commands
        .spawn((
            Name::new("player picture"),
            SpriteAnimation::new(assets.sheet.clone(), "idle").with_pivot(FEET),
            Animator::new(assets.graph.clone()),
            Layer(10),
            // The sheet's pivot is at the feet; the body's origin is its centre.
            Transform2D::from_xy(0.0, -SIZE.y / 2.0),
            ChildOf(body),
        ))
        .id();
    commands.entity(body).insert(Player {
        sprite,
        facing: 1.0,
        coyote: 0.0,
        buffer: 0.0,
    });
    body
}
// ANCHOR_END: spawn

fn read_controls(actions: Res<ActionState>, mut intent: ResMut<Intent>) {
    *intent = Intent {
        // From -1 to 1: keys give whole steps, a stick anything between.
        run: actions.axis("left", "right"),
        jump: actions.just_pressed("jump"),
        jump_held: actions.pressed("jump"),
    };
}

fn move_towards(from: f32, to: f32, step: f32) -> f32 {
    if (to - from).abs() <= step {
        to
    } else {
        from + step * (to - from).signum()
    }
}

// ANCHOR: movement
fn run_and_jump(
    time: Res<Time>,
    intent: Res<Intent>,
    player: Single<(&mut Player, &mut KinematicBody, &KinematicContacts)>,
    mut pictures: Query<(&mut Sprite, &mut Animator)>,
    assets: Res<LevelAssets>,
    mut audio: ResMut<Audio>,
) {
    let (mut player, mut body, contacts) = player.into_inner();
    let dt = time.fixed_delta_secs();
    let grounded = contacts.on_ground();

    let accel = if grounded { GROUND_ACCEL } else { AIR_ACCEL };
    body.velocity.x = move_towards(body.velocity.x, intent.run * RUN_SPEED, accel * dt);
    if intent.run != 0.0 {
        player.facing = intent.run.signum();
    }

    player.coyote = if grounded { COYOTE } else { (player.coyote - dt).max(0.0) };
    player.buffer = if intent.jump { BUFFER } else { (player.buffer - dt).max(0.0) };
    if player.buffer > 0.0 && player.coyote > 0.0 {
        body.velocity.y = JUMP_SPEED;
        player.buffer = 0.0;
        player.coyote = 0.0;
        audio.play(&assets.jump);
    } else if !intent.jump_held && body.velocity.y > JUMP_SPEED * 0.5 {
        body.velocity.y = JUMP_SPEED * 0.5;
    }

    if let Ok((mut sprite, mut animator)) = pictures.get_mut(player.sprite) {
        let flip = player.facing < 0.0;
        if sprite.flip_x != flip {
            sprite.flip_x = flip;
        }
        animator.set("speed", body.velocity.x.abs());
        animator.set("vy", body.velocity.y);
        animator.set_bool("grounded", grounded);
        animator.set_bool("wall", false);
    }
}
// ANCHOR_END: movement
