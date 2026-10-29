//! `bunnymark`: thousands of bouncing bunnies, to measure the sprite batcher.
//!
//! - Desktop: `cargo run -p bunnymark --release`. Flags: `--bunnies N` (default 10000),
//!   `--no-vsync` to see the uncapped frame rate, `--backend gl` (or `vulkan`, `dx12`,
//!   `metal`) to pick the graphics API, and `--frames N` to exit after N frames.
//! - Web: run `trunk serve` in `examples/bunnymark` and open <http://127.0.0.1:8081>. The
//!   same flags go in the URL: `?bunnies=20000&backend=gl` forces WebGL2.
//!
//! The bunnies move at 60 Hz in `FixedUpdate` and are drawn interpolated, four textures
//! in four batches. Stats are logged once a second (the browser console on the web).
//!
//! | Input | Does |
//! | --- | --- |
//! | Hold the mouse button or touch | Add bunnies at the pointer |
//! | Up / Down | Add / remove 1000 bunnies |
//! | M | Cycle scaling: pixel-perfect, letterbox, stretch, window size |
//! | Mouse wheel / Q, E / R | Zoom / rotate / reset the camera |
//! | Space | Toggle debug shapes |
//! | Escape | Quit |

use cashgear_engine::prelude::*;

/// The area the bunnies bounce in: the default 640×360 virtual resolution.
const ARENA: Vec2 = Vec2::new(640.0, 360.0);
/// Gravity in world units per second squared (y up).
const GRAVITY: f32 = -1500.0;
/// Bunnies added per frame while the pointer is held.
const SPAWN_PER_FRAME: usize = 100;
const STEP: usize = 1000;

const BUNNY_PNGS: [&[u8]; 4] = [
    include_bytes!("../assets/bunny_white.png"),
    include_bytes!("../assets/bunny_tan.png"),
    include_bytes!("../assets/bunny_gray.png"),
    include_bytes!("../assets/bunny_brown.png"),
];

struct Bunny {
    position: Vec2,
    previous: Vec2,
    velocity: Vec2,
    texture: TextureId,
}

#[derive(Resource, Default)]
struct Bunnies {
    list: Vec<Bunny>,
    textures: Vec<TextureId>,
    /// Bunnies to create once the textures exist.
    pending: usize,
    rng: Rng,
}

impl Bunnies {
    fn spawn(&mut self, at: Vec2, count: usize) {
        for _ in 0..count {
            let texture = self.textures[self.list.len() % self.textures.len()];
            let direction = if self.rng.next() < 0.5 { -1.0 } else { 1.0 };
            let velocity = vec2(
                direction * self.rng.range(60.0, 300.0),
                self.rng.range(-100.0, 300.0),
            );
            self.list.push(Bunny {
                position: at,
                previous: at,
                velocity,
                texture,
            });
        }
    }
}

/// A small xorshift generator; plenty for scattering bunnies.
struct Rng(u32);

impl Default for Rng {
    fn default() -> Self {
        Self(0x9e37_79b9)
    }
}

impl Rng {
    /// A number in `0.0..1.0`.
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1 << 24) as f32
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next()
    }
}

/// Counters for the once-a-second status line.
#[derive(Resource, Default)]
struct Status {
    last_report: f64,
    show_debug: bool,
}

fn main() {
    let args = Args::from_env();
    App::new()
        .add_plugins(DefaultPlugins {
            window: WindowConfig {
                title: "CashGear: bunnymark".into(),
                canvas_id: Some("cashgear".into()),
                ..default()
            },
            ..default()
        })
        .insert_resource(Bunnies {
            pending: args.parse("bunnies").unwrap_or(10_000),
            ..default()
        })
        .init_resource::<Status>()
        .add_systems(Startup, setup)
        .add_systems(FixedUpdate, bounce)
        .add_systems(
            Update,
            (add_and_remove, camera_and_debug, draw, report).chain(),
        )
        .run();
}

fn setup(mut textures: ResMut<Textures>, mut bunnies: ResMut<Bunnies>) {
    for png in BUNNY_PNGS {
        let image = Image::from_png(png).expect("bunny PNGs are valid");
        bunnies.textures.push(textures.add(image));
    }
    // Scatter the starting bunnies over the arena.
    let count = core::mem::take(&mut bunnies.pending);
    for _ in 0..count {
        let at = vec2(
            bunnies.rng.range(-0.5, 0.5) * ARENA.x,
            bunnies.rng.range(-0.5, 0.5) * ARENA.y,
        );
        bunnies.spawn(at, 1);
    }
    info!(
        "bunnymark: {count} bunnies. Hold the mouse to add more, Up/Down for ±{STEP}, M for \
         scaling modes, wheel/Q/E/R for the camera, Space for debug shapes"
    );
}

/// Gameplay at 60 Hz: gravity and bounces off the arena's edges.
fn bounce(time: Res<Time>, mut bunnies: ResMut<Bunnies>) {
    let dt = time.fixed_delta_secs();
    let half = ARENA * 0.5;
    let Bunnies { list, rng, .. } = &mut *bunnies;
    for bunny in list.iter_mut() {
        bunny.previous = bunny.position;
        bunny.velocity.y += GRAVITY * dt;
        bunny.position += bunny.velocity * dt;

        if bunny.position.x.abs() > half.x {
            bunny.velocity.x = -bunny.velocity.x;
            bunny.position.x = bunny.position.x.clamp(-half.x, half.x);
        }
        if bunny.position.y < -half.y {
            bunny.position.y = -half.y;
            bunny.velocity.y = -bunny.velocity.y * 0.85;
            if rng.next() < 0.5 {
                bunny.velocity.y += rng.range(0.0, 360.0);
            }
        } else if bunny.position.y > half.y {
            bunny.position.y = half.y;
            bunny.velocity.y = 0.0;
        }
    }
}

/// The pointer adds bunnies where it is held; Up and Down add and remove a thousand.
fn add_and_remove(
    keys: Res<ButtonInput<KeyCode>>,
    pointers: Res<Pointers>,
    window: Res<Window>,
    camera: Res<Camera2D>,
    mut bunnies: ResMut<Bunnies>,
) {
    for pointer in pointers.iter().filter(|p| p.pressed()) {
        let at = camera.screen_to_world(pointer.position(), &window);
        bunnies.spawn(at.clamp(-ARENA * 0.5, ARENA * 0.5), SPAWN_PER_FRAME);
    }
    if keys.just_pressed(KeyCode::ArrowUp) {
        bunnies.spawn(vec2(-ARENA.x * 0.5, ARENA.y * 0.5), STEP);
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        let keep = bunnies.list.len().saturating_sub(STEP);
        bunnies.list.truncate(keep);
    }
}

/// Scaling modes, zoom, rotation, debug shapes and quitting.
fn camera_and_debug(
    keys: Res<ButtonInput<KeyCode>>,
    pointers: Res<Pointers>,
    time: Res<Time>,
    mut camera: ResMut<Camera2D>,
    mut debug: ResMut<DebugShapes>,
    mut status: ResMut<Status>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        exit.write(AppExit::Success);
    }
    if keys.just_pressed(KeyCode::KeyM) {
        camera.scaling = match camera.scaling {
            ScalingMode::PixelPerfect(size) => ScalingMode::Letterbox(size),
            ScalingMode::Letterbox(size) => ScalingMode::Stretch(size),
            ScalingMode::Stretch(_) => ScalingMode::WindowSize,
            ScalingMode::WindowSize => ScalingMode::default(),
        };
        info!("scaling: {:?}", camera.scaling);
    }
    let scroll = pointers.scroll().y;
    if scroll != 0.0 {
        camera.zoom = (camera.zoom * 1.1f32.powf(scroll)).clamp(0.25, 8.0);
    }
    let turn =
        f32::from(i8::from(keys.pressed(KeyCode::KeyQ)) - i8::from(keys.pressed(KeyCode::KeyE)));
    camera.rotation += turn * time.raw_delta_secs();
    if keys.just_pressed(KeyCode::KeyR) {
        *camera = Camera2D {
            scaling: camera.scaling,
            ..default()
        };
    }
    if keys.just_pressed(KeyCode::Space) {
        status.show_debug = !status.show_debug;
    }
    debug.set_enabled(status.show_debug);
}

/// Queues every bunny, interpolated between its last two fixed steps.
fn draw(
    time: Res<Time>,
    bunnies: Res<Bunnies>,
    pointers: Res<Pointers>,
    window: Res<Window>,
    camera: Res<Camera2D>,
    mut sprites: ResMut<Sprites>,
    mut debug: ResMut<DebugShapes>,
) {
    let alpha = time.alpha();
    sprites.extend(bunnies.list.iter().map(|bunny| Sprite {
        texture: bunny.texture,
        position: bunny.previous.lerp(bunny.position, alpha),
        pivot: vec2(0.5, 0.0),
        // Lean into the jump, and face the way the bunny runs.
        rotation: (bunny.velocity.y * 0.0006 * bunny.velocity.x.signum()).clamp(-0.3, 0.3),
        flip_x: bunny.velocity.x < 0.0,
        ..default()
    }));

    if debug.enabled() {
        debug.rect(Rect::from_center_size(Vec2::ZERO, ARENA), Color::YELLOW);
        for bunny in bunnies.list.iter().take(50) {
            let at = bunny.previous.lerp(bunny.position, alpha);
            debug.rect(
                Rect::from_min_size(at - vec2(9.0, 0.0), vec2(18.0, 24.0)),
                Color::GREEN,
            );
        }
        if let Some(pointer) = pointers.primary() {
            let at = camera.screen_to_world(pointer.position(), &window);
            debug.circle(at, 8.0, Color::RED);
        }
    }
}

fn report(
    time: Res<Time>,
    bunnies: Res<Bunnies>,
    stats: Res<RenderStats>,
    mut status: ResMut<Status>,
) {
    let elapsed = time.raw_elapsed().as_secs_f64();
    if elapsed - status.last_report < 1.0 {
        return;
    }
    status.last_report = elapsed;
    info!(
        "{} bunnies: {:.0} fps ({:.2} ms), {} sprites in {} batches, {} draw calls, {} on {}",
        bunnies.list.len(),
        stats.fps(),
        stats.frame_time().as_secs_f64() * 1000.0,
        stats.sprites(),
        stats.batches(),
        stats.draw_calls(),
        stats.backend(),
        stats.adapter(),
    );
}
