//! `hello_window`: opens a window, runs the fixed-timestep game loop and logs input, on
//! desktop and in the browser.
//!
//! - Desktop: `cargo run -p hello_window`. Add `-- --frames 120` to exit after 120 frames.
//! - Web: run `trunk serve` in `examples/hello_window` and open <http://127.0.0.1:8080>. The
//!   log is in the browser console.
//!
//! A pretend player is simulated at 60 Hz in `FixedUpdate` and drawn as a box, interpolated
//! between fixed steps, with the camera following it along a striped ground.
//!
//! | Action | Keyboard | Gamepad | Touch |
//! | --- | --- | --- | --- |
//! | `left` / `right` | Arrows, A / D | D-pad, left stick | Left / second quarter of the screen |
//! | `jump` | Space, W, Up | South (A / cross) | Right half of the screen |
//! | `quit` | Escape | | |

use cashgear_engine::prelude::*;

/// Horizontal speed in pixels per second.
const RUN_SPEED: f32 = 160.0;
/// Jump take-off speed in pixels per second (up is positive).
const JUMP_SPEED: f32 = 420.0;
/// Gravity in pixels per second squared.
const GRAVITY: f32 = -1200.0;

/// A pretend player, simulated at the fixed rate.
#[derive(Resource, Default)]
struct Player {
    position: Vec2,
    previous: Vec2,
    velocity: Vec2,
}

/// Counters for the once-a-second status line.
#[derive(Resource, Default)]
struct Status {
    frames: u32,
    fixed_steps: u32,
    last_report: f64,
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins {
            window: WindowConfig {
                title: "CashGear: hello_window".into(),
                canvas_id: Some("cashgear".into()),
                ..default()
            },
            ..default()
        })
        .insert_resource(action_map())
        .init_resource::<Player>()
        .init_resource::<Status>()
        .add_systems(Startup, hello)
        .add_systems(FixedUpdate, move_player)
        .add_systems(
            Update,
            (
                log_lifecycle,
                log_input,
                log_actions,
                log_status,
                draw,
                quit,
            )
                .chain(),
        )
        .run();
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
        .bind("quit", KeyCode::Escape)
}

fn hello(mut commands: Commands, window: Res<Window>, gamepads: Res<Gamepads>, time: Res<Time>) {
    commands.spawn(Camera2D::default());
    info!(
        "hello_window running on CashGear {}: {:.0}x{:.0} logical pixels, \
         fixed step {:.2} ms, {} gamepad(s)",
        cashgear_engine::VERSION,
        window.logical_size().x,
        window.logical_size().y,
        time.fixed_delta().as_secs_f64() * 1000.0,
        gamepads.len(),
    );
    info!("move with arrows/A/D, jump with Space, quit with Escape");
}

/// Gameplay: runs at exactly 60 Hz however fast frames are.
fn move_player(actions: Res<ActionState>, time: Res<Time>, mut player: ResMut<Player>) {
    let dt = time.fixed_delta_secs();
    let grounded = player.position.y <= 0.0;
    player.previous = player.position;

    player.velocity.x = actions.axis("left", "right") * RUN_SPEED;
    if grounded && actions.just_pressed("jump") {
        player.velocity.y = JUMP_SPEED;
        info!("jump (fixed step {})", time.fixed_step_count());
    } else if !grounded {
        player.velocity.y += GRAVITY * dt;
    }

    let velocity = player.velocity;
    player.position += velocity * dt;
    if player.position.y < 0.0 {
        player.position.y = 0.0;
        player.velocity.y = 0.0;
    }
}

fn log_lifecycle(mut lifecycle: MessageReader<Lifecycle>) {
    for message in lifecycle.read() {
        info!("lifecycle: {message:?}");
    }
}

fn log_input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pointers: Res<Pointers>,
    gamepads: Res<Gamepads>,
) {
    for key in keys.get_just_pressed() {
        info!("key pressed: {key:?}");
    }
    for key in keys.get_just_released() {
        info!("key released: {key:?}");
    }
    for button in mouse.get_just_pressed() {
        info!("mouse pressed: {button:?}");
    }
    for pointer in pointers.iter() {
        let p = pointer.position();
        if pointer.just_pressed() {
            info!(
                "pointer {:?} down at ({:.0}, {:.0})",
                pointer.id(),
                p.x,
                p.y
            );
        }
        if pointer.just_released() {
            info!("pointer {:?} up at ({:.0}, {:.0})", pointer.id(), p.x, p.y);
        }
    }
    if pointers.scroll() != Vec2::ZERO {
        info!("scroll: {}", pointers.scroll());
    }
    for (id, pad) in gamepads.iter() {
        for button in pad.buttons().get_just_pressed() {
            info!("gamepad {} ({}) pressed: {button:?}", id.0, pad.name());
        }
    }
}

fn log_actions(actions: Res<ActionState>) {
    for action in actions.get_just_pressed() {
        info!("action started: {action}");
    }
    for action in actions.get_just_released() {
        info!("action ended: {action}");
    }
}

/// Once a second: frame rate, fixed steps, and the player's interpolated position.
fn log_status(time: Res<Time>, player: Res<Player>, mut status: ResMut<Status>) {
    status.frames += 1;
    status.fixed_steps += time.fixed_steps_this_frame();
    let elapsed = time.raw_elapsed().as_secs_f64();
    if elapsed - status.last_report < 1.0 {
        return;
    }
    let seconds = elapsed - status.last_report;
    let drawn = player.previous.lerp(player.position, time.alpha());
    info!(
        "{:.0} fps, {:.0} fixed steps/s, player at ({:.0}, {:.0})",
        f64::from(status.frames) / seconds,
        f64::from(status.fixed_steps) / seconds,
        drawn.x,
        drawn.y,
    );
    *status = Status {
        last_report: elapsed,
        ..default()
    };
}

/// Draws the player where it is between the last two fixed steps, and a striped ground the
/// camera scrolls over.
fn draw(
    time: Res<Time>,
    player: Res<Player>,
    window: Res<Window>,
    camera: Single<(&Camera2D, &mut Transform2D)>,
    mut sprites: ResMut<Sprites>,
) {
    let drawn = player.previous.lerp(player.position, time.alpha());
    // Keep the ground near the bottom of the view and the player in the middle.
    let (settings, mut transform) = camera.into_inner();
    transform.translation = vec2(drawn.x, 120.0);

    let view =
        CameraView::from_transform(settings.clone(), transform.affine()).visible_rect(&window);
    let stripe = 32.0;
    let first = (view.min.x / stripe).floor() as i32;
    let last = (view.max.x / stripe).ceil() as i32;
    for i in first..last {
        let color = if i % 2 == 0 { 0x3a3f4b } else { 0x323641 };
        let tile = Sprite {
            pivot: vec2(0.0, 1.0),
            ..Sprite::solid(Color::hex(color), vec2(stripe, 48.0))
        };
        sprites.draw(&tile, Transform2D::from_xy(i as f32 * stripe, 0.0));
    }
    let player = Sprite {
        pivot: vec2(0.5, 0.0),
        ..Sprite::solid(Color::hex(0xffbe0b), vec2(16.0, 24.0))
    };
    sprites.draw_on(1, &player, Transform2D::from_translation(drawn));
}

fn quit(actions: Res<ActionState>, mut exit: MessageWriter<AppExit>) {
    if actions.just_pressed("quit") {
        exit.write(AppExit::Success);
    }
}
