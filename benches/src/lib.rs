//! Scenes for the engine's benchmarks and its allocation check. Each builds an app without
//! a window, as the engine's own tests do, so frames run the same schedules as a game
//! (input, fixed steps, physics, animation, transforms, visibility, the UI layout and the
//! render queues) without drawing.
//!
//! `benches/frames.rs` times one frame of each with criterion (`cargo bench -p cg_bench`),
//! and `tests/allocations.rs` counts the heap allocations a frame makes once a scene has
//! warmed up.

use cashgear_engine::audio::AudioOutput;
use cashgear_engine::platform::input::InputPlugin;
use cashgear_engine::prelude::*;
use core::time::Duration;

/// A frame at 60 frames a second: one fixed step each.
pub const FRAME: Duration = Duration::from_nanos(16_666_667);

/// Runs one frame.
pub fn frame(app: &mut App) {
    app.update(FRAME);
}

/// Runs `count` frames.
pub fn frames(app: &mut App, count: usize) {
    for _ in 0..count {
        frame(app);
    }
}

/// The folder of this crate's own assets.
const OWN_ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets");
/// The collision example's assets: Kenney tiles and a test course.
const COURSE_ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/collision/assets");
/// The platformer's assets.
const PLATFORMER_ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../games/platformer/assets");

/// The engine without a window, reading assets from `root`.
fn engine(root: &str) -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        root: Some(root.to_owned()),
        hot_reload: Some(false),
        ..default()
    })
    .add_plugins(InputPlugin { gamepads: false })
    .add_plugins(StoragePlugin {
        memory: true,
        ..default()
    })
    .add_plugins(TiledPlugin)
    .add_plugins(PhysicsPlugin::default())
    .add_plugins(AnimPlugin::default())
    .add_plugins(UiPlugin::default())
    .insert_resource(Window::with_size(UVec2::new(1280, 720), 1.0));
    app
}

/// A small repeatable random stream.
#[derive(Resource)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.next()
    }
}

/// A sprite's velocity.
#[derive(Component)]
struct Velocity(Vec2);

/// The area the sprites bounce in: twice the default 640x360 view each way, so about
/// three quarters of them are culled.
const ARENA: Vec2 = Vec2::new(1280.0, 720.0);

/// `count` interpolated sprite entities bouncing in fixed steps, a quarter of them in view.
pub fn sprites(count: usize) -> App {
    let mut app = engine(OWN_ASSETS);
    app.insert_resource(Rng(7))
        .add_systems(
            Startup,
            move |mut commands: Commands, mut rng: ResMut<Rng>| {
                commands.spawn(Camera2D::default());
                for i in 0..count {
                    let at = vec2(
                        rng.range(-0.5, 0.5) * ARENA.x,
                        rng.range(-0.5, 0.5) * ARENA.y,
                    );
                    let velocity = vec2(rng.range(-80.0, 80.0), rng.range(-80.0, 80.0));
                    let shade = (i % 5) as f32 / 5.0;
                    commands.spawn((
                        Sprite::solid(Color::rgb(1.0, shade, 0.4), vec2(8.0, 8.0)),
                        Transform2D::from_translation(at),
                        Interpolated::default(),
                        Velocity(velocity),
                    ));
                }
            },
        )
        .add_systems(FixedUpdate, bounce);
    app
}

fn bounce(time: Res<Time>, mut query: Query<(&mut Transform2D, &mut Velocity)>) {
    let dt = time.fixed_delta_secs();
    let half = ARENA * 0.5;
    for (mut transform, mut velocity) in &mut query {
        let mut at = transform.translation + velocity.0 * dt;
        if at.x.abs() > half.x {
            velocity.0.x = -velocity.0.x;
            at.x = at.x.clamp(-half.x, half.x);
        }
        if at.y.abs() > half.y {
            velocity.0.y = -velocity.0.y;
            at.y = at.y.clamp(-half.y, half.y);
        }
        transform.translation = at;
    }
}

/// A walker in the level scene.
#[derive(Component)]
struct Walker {
    dir: f32,
    start: Vec2,
}

/// The collision example's Tiled course with `bodies` kinematic boxes walking its ground
/// (turning at walls) and three particle emitters. Run [`warm_up`] first: the map loads in
/// the first frames.
pub fn level(bodies: usize) -> App {
    let mut app = engine(COURSE_ASSETS);
    app.add_systems(
        Startup,
        move |mut commands: Commands, server: Res<AssetServer>| {
            commands.spawn((Camera2D::default(), Transform2D::from_xy(320.0, 180.0)));
            commands.spawn(Tilemap::new(server.load("maps/course.tmx")));
            for i in 0..bodies {
                let x = 40.0 + (i % 50) as f32 * 12.0;
                let y = 120.0 + (i / 50) as f32 * 24.0;
                commands.spawn((
                    Walker {
                        dir: if i % 2 == 0 { 1.0 } else { -1.0 },
                        start: vec2(x, y),
                    },
                    KinematicBody::default(),
                    Collider::rect(10.0, 14.0),
                    Sprite::solid(Color::hex(0xf28c28), vec2(10.0, 14.0)),
                    Transform2D::from_xy(x, y),
                ));
            }
            for (i, x) in [120.0, 320.0, 520.0].into_iter().enumerate() {
                commands.spawn((
                    ParticleEmitter::new(ParticleSettings {
                        rate: 120.0,
                        lifetime: (0.8, 1.2),
                        speed: (30.0, 60.0),
                        direction: 90.0,
                        spread: 40.0,
                        gravity: vec2(0.0, -40.0),
                        size: vec![3.0, 1.0],
                        color: vec![Color::WHITE, Color::hex(0x7ec8e3).with_alpha(0.0)],
                        ..default()
                    })
                    .with_seed(i as u64),
                    Transform2D::from_xy(x, 200.0),
                ));
            }
        },
    )
    .add_systems(FixedUpdate, walk);
    app
}

fn walk(
    mut walkers: Query<(
        &mut Walker,
        &mut KinematicBody,
        &KinematicContacts,
        &mut Transform2D,
    )>,
) {
    for (mut walker, mut body, contacts, mut transform) in &mut walkers {
        // A walker that fell into a pit starts again.
        if transform.translation.y < -100.0 {
            transform.translation = walker.start;
            body.velocity = Vec2::ZERO;
        }
        if (walker.dir > 0.0 && contacts.wall_right.is_some())
            || (walker.dir < 0.0 && contacts.wall_left.is_some())
        {
            walker.dir = -walker.dir;
        }
        body.velocity.x = walker.dir * 60.0;
    }
}

/// A menu of `buttons` labelled buttons, a slider and a progress bar that fills over time:
/// the UI's input, layout and drawing.
pub fn menu(buttons: usize) -> App {
    let mut app = engine(OWN_ASSETS);
    app.add_systems(Startup, move |mut commands: Commands| {
        commands.spawn(Camera2D::default());
        commands
            .spawn(Node {
                anchor: Anchor::Center,
                layout: Layout::Column,
                gap: 2.0,
                padding: Edges::all(4.0),
                ..default()
            })
            .insert(BackgroundColor(Color::hex(0x1d2b53)))
            .with_children(|menu| {
                for i in 0..buttons {
                    menu.spawn((
                        Node::sized(120.0, 12.0),
                        Button::default(),
                        Focusable::default(),
                        BackgroundColor(Color::hex(0x3b5dc9)),
                        Text::new(format!("Button {i}")),
                    ));
                }
                menu.spawn((Node::sized(120.0, 8.0), Slider::new(0.5, 0.1)));
                menu.spawn((
                    Node::sized(120.0, 6.0),
                    ProgressBar::default(),
                    BackgroundColor(Color::BLACK),
                ));
            });
    })
    .add_systems(
        Update,
        |time: Res<Time>, mut bars: Query<&mut ProgressBar>| {
            for mut bar in &mut bars {
                bar.value = (time.elapsed_secs() * 0.1).fract() as f32;
            }
        },
    );
    app
}

/// `count` scripted kinematic bodies on a floor, each running `scripts/pacer.lua`, which
/// reads and writes its transform and velocity every step.
pub fn scripts(count: usize) -> App {
    let mut app = engine(OWN_ASSETS);
    app.add_plugins(ScriptPlugin {
        show_errors: false,
        ..default()
    })
    .add_systems(
        Startup,
        move |mut commands: Commands, server: Res<AssetServer>| {
            commands.spawn(Camera2D::default());
            // The floor.
            commands.spawn((
                Collider::rect(4000.0, 20.0),
                Transform2D::from_xy(0.0, -200.0),
            ));
            let script: Handle<LuaScript> = server.load("scripts/pacer.lua");
            for i in 0..count {
                let x = -300.0 + (i % 100) as f32 * 6.0;
                let y = -180.0 + (i / 100) as f32 * 30.0;
                commands.spawn((
                    Script::new(script.clone()),
                    KinematicBody::default(),
                    Collider::rect(4.0, 8.0),
                    Transform2D::from_xy(x, y),
                ));
            }
        },
    );
    app
}

/// The platformer's first level played by its bot, with Lua walkers and checkpoints where
/// Lua runs: the sample game's whole frame.
pub fn platformer() -> App {
    use platformer::{Game, Options};
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        root: Some(PLATFORMER_ASSETS.to_owned()),
        hot_reload: Some(false),
        ..default()
    })
    .add_plugins(InputPlugin { gamepads: false })
    .add_plugins(StoragePlugin {
        memory: true,
        ..default()
    })
    .add_plugins(TiledPlugin)
    .add_plugins(PhysicsPlugin::default())
    .add_plugins(AnimPlugin::default())
    .add_plugins(AudioPlugin {
        output: AudioOutput::Mock,
        settings_key: None,
        ..default()
    })
    .add_plugins(UiPlugin {
        locale: Some(LocaleSettings::new(&["en", "ar"])),
        ..default()
    })
    .add_plugins(ScriptPlugin {
        show_errors: false,
        ..default()
    })
    .add_plugins(Game {
        options: Options {
            demo: true,
            level: Some(0),
        },
    })
    .insert_resource(Window::with_size(UVec2::new(1280, 720), 1.0));
    app
}

/// Runs frames until the scene's assets are in and its caches are full: two seconds of
/// frames.
pub fn warm_up(app: &mut App) {
    frames(app, 120);
}
