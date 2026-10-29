//! A reference-image test of the UI: a small screen is laid out by the UI plugin, drawn
//! offscreen through the renderer's screen-space pass and compared with
//! `tests/golden/ui.png`, plus exact checks that pixel-font text stays on the pixel grid.
//!
//! - Without a GPU or software adapter the test passes with a note, unless
//!   `CG_REQUIRE_GPU=1` is set (CI sets it).
//! - `CG_UPDATE_GOLDEN=1 cargo test -p cg_ui --test golden` rewrites the reference. Look at
//!   it before committing.
//! - On a mismatch the rendered image is written to `target/golden/ui.png`.

// Tests read and write reference files on desktop only.
#![allow(clippy::disallowed_methods)]

use bevy_ecs::prelude::*;
use cg_assets::{AssetPlugin, Assets};
use cg_core::glam::{UVec2, Vec2, uvec2, vec2};
use cg_core::{App, Render};
use cg_platform::Window;
use cg_platform::input::InputPlugin;
use cg_render::{
    Camera2D, CameraView, Color, DebugShapes, FrameView, Image, OffscreenRenderer, QueuedSprite,
    RenderSystems, ScalingMode, Sprites,
};
use cg_ui::prelude::*;
use std::path::PathBuf;

/// A channel may differ by this much before the pixel counts as different.
const CHANNEL_TOLERANCE: u8 = 4;
/// Share of pixels allowed to differ.
const PIXEL_TOLERANCE: f64 = 0.001;
const SIZE: UVec2 = UVec2::new(640, 360);
const BACKGROUND: u32 = 0x202430;
const PANEL: u32 = 0x3b5dc9;

/// The queues of the frame being drawn, copied before the renderer empties them.
#[derive(Resource, Default)]
struct Snapshot {
    world: Vec<QueuedSprite>,
    screen: Vec<QueuedSprite>,
    view: CameraView,
}

fn snapshot(sprites: Res<Sprites>, view: Res<FrameView>, mut snapshot: ResMut<Snapshot>) {
    snapshot.world = sprites.queued().to_vec();
    snapshot.screen = sprites.screen_queued().to_vec();
    snapshot.view = view.0.clone();
}

/// A 12×12 button image: a white frame, light grey inside, for nine-slicing.
fn button_image() -> Image {
    Image::from_fn(12, 12, |x, y| {
        let edge = x < 2 || y < 2 || x >= 10 || y >= 10;
        if edge {
            [250, 250, 250, 255]
        } else {
            [150, 160, 170, 255]
        }
    })
}

/// Lays the screen out: a 320×180 pixel-perfect view in a 640×360 window, so every UI
/// pixel is two by two pixels.
fn ui_app() -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        hot_reload: Some(false),
        ..Default::default()
    })
    .add_plugins(InputPlugin { gamepads: false })
    .add_plugins(UiPlugin::default())
    .insert_resource(Window::with_size(SIZE, 1.0))
    .init_resource::<Snapshot>()
    .add_systems(
        Render,
        snapshot
            .after(RenderSystems::Queue)
            .before(RenderSystems::Draw),
    );
    app.world_mut().spawn(Camera2D {
        scaling: ScalingMode::PixelPerfect(uvec2(320, 180)),
        clear_color: Color::hex(BACKGROUND),
        ..Camera2D::default()
    });
    let button = app
        .world_mut()
        .resource_mut::<Assets<Image>>()
        .add(button_image());
    let world = app.world_mut();
    let panel = world
        .spawn((
            Node {
                anchor: Anchor::Center,
                width: Val::Px(200.0),
                padding: Edges::all(8.0),
                border: 2.0,
                layout: Layout::Column,
                gap: 6.0,
                align: Align::Stretch,
                ..Node::default()
            },
            BackgroundColor(Color::hex(PANEL)),
            BorderColor(Color::hex(0xffcd75)),
        ))
        .id();
    world.spawn((
        Text::new("CashGear UI").with_align(TextAlign::Center),
        ChildOf(panel),
    ));
    let sliced = world
        .spawn((
            Node {
                height: Val::Px(20.0),
                ..Node::default()
            },
            UiImage::new(button).sliced(Edges::all(3.0)),
            ChildOf(panel),
        ))
        .id();
    world.spawn((
        Node::default().at(Anchor::Center),
        Text::new("Nine-slice").with_color(Color::hex(0x1a1c2c)),
        ChildOf(sliced),
    ));
    let bar = world
        .spawn((
            Node {
                height: Val::Px(6.0),
                ..Node::default()
            },
            ProgressBar { value: 0.5 },
            BackgroundColor(Color::hex(0x1a1c2c)),
            ChildOf(panel),
        ))
        .id();
    world.spawn((
        Node {
            height: Val::Fill,
            ..Node::default()
        },
        BarFill,
        BackgroundColor(Color::hex(0x38b764)),
        ChildOf(bar),
    ));
    // A right-to-left row: the red square comes first, so it is on the right.
    let row = world
        .spawn((
            Node {
                layout: Layout::Row,
                gap: 4.0,
                direction: LayoutDirection::RightToLeft,
                ..Node::default()
            },
            ChildOf(panel),
        ))
        .id();
    for color in [0xe04040, 0x40c060, 0x4060e0] {
        world.spawn((
            Node::sized(10.0, 10.0),
            BackgroundColor(Color::hex(color)),
            ChildOf(row),
        ));
    }
    for _ in 0..3 {
        app.update(core::time::Duration::from_millis(16));
    }
    app
}

fn workspace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compares `image` with `tests/golden/<name>.png`.
fn check(name: &str, image: &Image) {
    let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"));
    if std::env::var_os("CG_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(reference.parent().unwrap()).unwrap();
        std::fs::write(&reference, image.to_png().unwrap()).unwrap();
        eprintln!("wrote {}", reference.display());
        return;
    }
    let bytes = std::fs::read(&reference).unwrap_or_else(|_| {
        panic!(
            "missing {}; run with CG_UPDATE_GOLDEN=1 to create it",
            reference.display()
        )
    });
    let expected = Image::from_png(&bytes).unwrap();
    let different = if expected.size() == image.size() {
        expected
            .data()
            .chunks(4)
            .zip(image.data().chunks(4))
            .filter(|(a, b)| {
                a.iter()
                    .zip(*b)
                    .any(|(a, b)| a.abs_diff(*b) > CHANNEL_TOLERANCE)
            })
            .count()
    } else {
        usize::MAX
    };
    let total = (image.width() * image.height()) as usize;
    if different as f64 > total as f64 * PIXEL_TOLERANCE {
        let actual = workspace_dir()
            .join("target/golden")
            .join(format!("{name}.png"));
        std::fs::create_dir_all(actual.parent().unwrap()).unwrap();
        std::fs::write(&actual, image.to_png().unwrap()).unwrap();
        panic!(
            "{name}: {different} of {total} pixels differ from {}; the render is at {}",
            reference.display(),
            actual.display()
        );
    }
}

#[test]
fn the_ui_renders_like_the_reference_with_pixel_text_on_the_grid() {
    let renderer = pollster::block_on(OffscreenRenderer::new());
    let mut renderer = match renderer {
        Ok(renderer) => renderer,
        Err(err) => {
            if std::env::var_os("CG_REQUIRE_GPU").is_some() {
                panic!("CG_REQUIRE_GPU is set but there is no GPU: {err}");
            }
            eprintln!("skipping the GPU test: {err}");
            return;
        }
    };
    let app = ui_app();
    let snapshot = app.world().resource::<Snapshot>();
    assert!(!snapshot.screen.is_empty());
    let mut sprites = Sprites::default();
    sprites.extend(snapshot.world.iter().copied());
    for sprite in &snapshot.screen {
        sprites.push_screen(*sprite);
    }
    let image = renderer
        .render(
            SIZE,
            &snapshot.view,
            app.world().resource::<Assets<Image>>(),
            &sprites,
            &DebugShapes::default(),
        )
        .expect("render");

    // The panel's colour fills its inside exactly, and the border is gold.
    let panel = Color::hex(PANEL).to_rgba8();
    let stack = app.world().resource::<cg_ui::UiStack>();
    let mut panel_rect = None;
    for &entity in stack.entities() {
        if app.world().get::<BorderColor>(entity).is_some() {
            panel_rect = app.world().get::<ComputedNode>(entity).map(|c| c.rect());
        }
    }
    let rect = panel_rect.expect("the panel was laid out");
    let to_pixel = |ui: Vec2| (ui * 2.0).as_uvec2();
    let inside = to_pixel(rect.min + vec2(4.0, 30.0));
    assert_eq!(image.pixel(inside.x, inside.y).unwrap(), panel);
    let border = to_pixel(rect.min + vec2(0.5, 20.0));
    assert_eq!(
        image.pixel(border.x, border.y).unwrap(),
        Color::hex(0xffcd75).to_rgba8()
    );

    // The pixel-font title is drawn in whole UI pixels: every 2×2 block of the title's
    // area is one colour, and only white and the panel's blue appear.
    let title_top = to_pixel(rect.min + vec2(10.0, 10.0));
    let title_bottom = to_pixel(rect.max - vec2(10.0, 0.0)).min(title_top + uvec2(360, 24));
    let white = Color::WHITE.to_rgba8();
    let mut lit = 0;
    for y in (title_top.y..title_bottom.y).step_by(2) {
        for x in (title_top.x..title_bottom.x).step_by(2) {
            let p = image.pixel(x, y).unwrap();
            assert!(p == white || p == panel, "({x}, {y}) is {p:?}");
            for (dx, dy) in [(1, 0), (0, 1), (1, 1)] {
                assert_eq!(image.pixel(x + dx, y + dy).unwrap(), p, "({x}, {y}) block");
            }
            lit += u32::from(p == white);
        }
    }
    assert!(lit > 40, "the title has {lit} lit pixels");

    check("ui", &image);
}
