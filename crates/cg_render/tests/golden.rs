//! Reference-image tests: scenes are rendered offscreen and compared with the PNGs in
//! `tests/golden/`, so visual regressions show up without anyone looking at a screen.
//!
//! - Without a GPU or software adapter the tests pass with a note, unless `CG_REQUIRE_GPU=1`
//!   is set (CI sets it).
//! - `CG_UPDATE_GOLDEN=1 cargo test -p cg_render --test golden` rewrites the references.
//!   Look at them before committing.
//! - On a mismatch the rendered image is written to `target/golden/<name>.png`.

// Tests read and write reference files on desktop only.
#![allow(clippy::disallowed_methods)]

use cg_core::Rect;
use cg_core::glam::{UVec2, Vec2, uvec2, vec2};
use cg_render::{
    Camera2D, Color, DebugShapes, Filter, Image, OffscreenRenderer, ScalingMode, Sprite, TextureId,
    Textures,
};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// A channel may differ by this much before the pixel counts as different.
const CHANNEL_TOLERANCE: u8 = 4;
/// Share of pixels allowed to differ: GPUs may disagree on a few edge pixels of rotated
/// shapes, but a sprite shifted by one pixel changes far more.
const PIXEL_TOLERANCE: f64 = 0.001;

static RENDERER: OnceLock<Option<Mutex<OffscreenRenderer>>> = OnceLock::new();

/// The shared renderer, or `None` (after a note) when there is no adapter.
fn renderer() -> Option<MutexGuard<'static, OffscreenRenderer>> {
    let renderer = RENDERER.get_or_init(|| match pollster::block_on(OffscreenRenderer::new()) {
        Ok(renderer) => {
            eprintln!(
                "golden tests on {} ({})",
                renderer.adapter(),
                renderer.backend()
            );
            Some(Mutex::new(renderer))
        }
        Err(err) => {
            if std::env::var_os("CG_REQUIRE_GPU").is_some() {
                panic!("CG_REQUIRE_GPU is set but there is no GPU: {err}");
            }
            eprintln!("skipping GPU tests: {err}");
            None
        }
    });
    renderer
        .as_ref()
        .map(|r| r.lock().unwrap_or_else(|e| e.into_inner()))
}

fn render(
    size: UVec2,
    camera: &Camera2D,
    textures: &Textures,
    sprites: &[Sprite],
    shapes: &DebugShapes,
) -> Option<Image> {
    let mut renderer = renderer()?;
    Some(
        renderer
            .render(size, camera, textures, sprites, shapes)
            .expect("render"),
    )
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
            "{name}: {} of {total} pixels differ from {} (size {} vs {}); the render is at {}",
            if different == usize::MAX {
                "all".to_owned()
            } else {
                different.to_string()
            },
            reference.display(),
            image.size(),
            expected.size(),
            actual.display()
        );
    }
}

/// An 8×8 texture with a different colour in each quarter, so flips and source
/// rectangles are visible: red top-left, green top-right, blue bottom-left, yellow
/// bottom-right (as seen in an image editor).
fn quadrants() -> Image {
    Image::from_fn(8, 8, |x, y| match (x < 4, y < 4) {
        (true, true) => [230, 40, 40, 255],
        (false, true) => [40, 200, 70, 255],
        (true, false) => [50, 90, 230, 255],
        (false, false) => [240, 210, 40, 255],
    })
}

fn camera(scaling: ScalingMode) -> Camera2D {
    Camera2D {
        scaling,
        clear_color: Color::hex(0x202430),
        bar_color: Color::hex(0x000000),
        ..Camera2D::default()
    }
}

/// Sprites exercising every `Sprite` field, on a 160×90 pixel grid.
#[test]
fn sprites() {
    let mut textures = Textures::default();
    let quad = textures.add(quadrants());
    let gradient = textures.add_with_filter(
        Image::from_fn(2, 2, |x, y| match (x, y) {
            (0, 0) => [255, 0, 0, 255],
            (1, 0) => [0, 255, 0, 255],
            (0, 1) => [0, 0, 255, 255],
            _ => [255, 255, 255, 255],
        }),
        Filter::Linear,
    );
    let at = |x: f32, y: f32| Sprite {
        position: vec2(x, y),
        ..Sprite::new(quad)
    };
    let sprites = [
        // Top row: plain, flipped x, flipped y, both, a source quarter scaled up.
        at(-64.0, 28.0),
        Sprite {
            flip_x: true,
            ..at(-48.0, 28.0)
        },
        Sprite {
            flip_y: true,
            ..at(-32.0, 28.0)
        },
        Sprite {
            flip_x: true,
            flip_y: true,
            ..at(-16.0, 28.0)
        },
        Sprite {
            source: Some(Rect::new(4.0, 0.0, 8.0, 4.0)),
            scale: Vec2::splat(3.0),
            ..at(8.0, 28.0)
        },
        // A linear-filtered gradient, stretched.
        Sprite {
            size: Some(vec2(24.0, 16.0)),
            ..at(44.0, 26.0)
        }
        .with_texture(gradient),
        // Middle row: rotation, a bottom-left pivot, a tint, half transparency.
        Sprite {
            rotation: core::f32::consts::FRAC_PI_4,
            scale: Vec2::splat(2.0),
            ..at(-56.0, -4.0)
        },
        Sprite {
            pivot: Vec2::ZERO,
            scale: Vec2::splat(2.0),
            ..at(-36.0, -12.0)
        },
        Sprite {
            color: Color::hex(0x60ffff),
            scale: Vec2::splat(2.0),
            ..at(-4.0, -4.0)
        },
        Sprite {
            size: Some(vec2(28.0, 10.0)),
            color: Color::WHITE.with_alpha(0.5),
            ..Sprite::new(TextureId::WHITE)
        }
        .at(-4.0, -4.0),
        // Bottom row: layers beat queue order, then texture grouping within a layer.
        Sprite {
            layer: 2,
            size: Some(vec2(12.0, 12.0)),
            color: Color::hex(0xff00ff),
            ..Sprite::new(TextureId::WHITE)
        }
        .at(30.0, -30.0),
        Sprite {
            layer: 1,
            scale: Vec2::splat(2.0),
            ..at(24.0, -26.0)
        },
        Sprite {
            scale: vec2(-2.0, 2.0),
            ..at(52.0, -26.0)
        },
    ];
    let Some(image) = render(
        uvec2(160, 90),
        &camera(ScalingMode::PixelPerfect(uvec2(160, 90))),
        &textures,
        &sprites,
        &DebugShapes::default(),
    ) else {
        return;
    };
    check("sprites", &image);
}

/// A 64×36 pixel-perfect view scaled 3× into 200×120, with bars, an off-grid camera and
/// sprite, and a thin debug line drawn at full resolution on top.
#[test]
fn pixel_perfect() {
    let mut textures = Textures::default();
    let quad = textures.add(quadrants());
    let mut cam = camera(ScalingMode::PixelPerfect(uvec2(64, 36)));
    cam.position = vec2(0.4, 0.3);
    let sprites = [
        Sprite::new(quad).at(-10.3, 4.6),
        Sprite {
            rotation: 0.4,
            ..Sprite::new(quad)
        }
        .at(12.0, -6.0),
    ];
    let mut shapes = DebugShapes::default();
    shapes.line(vec2(-30.0, -15.0), vec2(30.0, 15.0), Color::GREEN);
    let Some(image) = render(uvec2(200, 120), &cam, &textures, &sprites, &shapes) else {
        return;
    };
    check("pixel_perfect", &image);
}

/// Letterbox and stretch fitting the same 64×36 view into a square target.
#[test]
fn letterbox_and_stretch() {
    let mut textures = Textures::default();
    let quad = textures.add(quadrants());
    let sprites = [
        Sprite {
            rotation: 0.3,
            scale: Vec2::splat(2.0),
            ..Sprite::new(quad)
        },
        Sprite::new(quad).at(-28.0, 14.0),
        Sprite::new(quad).at(28.0, -14.0),
    ];
    for (name, scaling) in [
        ("letterbox", ScalingMode::Letterbox(uvec2(64, 36))),
        ("stretch", ScalingMode::Stretch(uvec2(64, 36))),
    ] {
        let Some(image) = render(
            uvec2(96, 96),
            &camera(scaling),
            &textures,
            &sprites,
            &DebugShapes::default(),
        ) else {
            return;
        };
        check(name, &image);
    }
}

/// A window-sized view through a moved, zoomed and rotated camera.
#[test]
fn camera_transform() {
    let mut textures = Textures::default();
    let quad = textures.add(quadrants());
    let mut cam = camera(ScalingMode::WindowSize);
    cam.position = vec2(8.0, 4.0);
    cam.zoom = 2.0;
    cam.rotation = core::f32::consts::FRAC_PI_6;
    let sprites = [Sprite::new(quad), Sprite::new(quad).at(16.0, 8.0)];
    let mut shapes = DebugShapes::default();
    shapes.circle(Vec2::ZERO, 2.0, Color::WHITE);
    let Some(image) = render(uvec2(96, 64), &cam, &textures, &sprites, &shapes) else {
        return;
    };
    check("camera_transform", &image);
}

/// Every debug shape, outlined and filled, in two line widths.
#[test]
fn debug_shapes() {
    let mut shapes = DebugShapes::default();
    shapes.rect_filled(
        Rect::new(-40.0, -20.0, -10.0, 10.0),
        Color::BLUE.with_alpha(0.5),
    );
    shapes.rect(Rect::new(-40.0, -20.0, -10.0, 10.0), Color::WHITE);
    shapes.circle_filled(vec2(14.0, 0.0), 10.0, Color::hex(0xff8800));
    shapes.set_line_width(3.0);
    shapes.circle(vec2(14.0, 0.0), 14.0, Color::RED);
    shapes.line(
        vec2(-44.0, -26.0),
        vec2(44.0, 26.0),
        Color::GREEN.with_alpha(0.75),
    );
    let Some(image) = render(
        uvec2(96, 64),
        &camera(ScalingMode::WindowSize),
        &Textures::default(),
        &[],
        &shapes,
    ) else {
        return;
    };
    check("debug_shapes", &image);
}

/// Pixel snapping keeps whole-texel sprites on whole pixels wherever they are placed.
#[test]
fn snapped_sprites_cover_whole_pixels() {
    let textures = Textures::default();
    let cam = camera(ScalingMode::PixelPerfect(uvec2(32, 18)));
    for offset in [0.0, 0.25, 0.5, 0.75, 0.999] {
        let sprites = [Sprite {
            size: Some(vec2(4.0, 3.0)),
            ..Sprite::new(TextureId::WHITE)
        }
        .at(offset, -offset)];
        let Some(image) = render(
            uvec2(32, 18),
            &cam,
            &textures,
            &sprites,
            &DebugShapes::default(),
        ) else {
            return;
        };
        let white = image
            .data()
            .chunks(4)
            .filter(|p| p == &[255, 255, 255, 255])
            .count();
        let background = Color::hex(0x202430).to_rgba8();
        let other = image
            .data()
            .chunks(4)
            .filter(|p| *p != [255, 255, 255, 255] && *p != background)
            .count();
        assert_eq!((white, other), (12, 0), "offset {offset}");
    }
}

/// Colours come out exactly as given, and alpha blends in linear light.
#[test]
fn srgb_colours_and_linear_blending() {
    let textures = Textures::default();
    let mut cam = camera(ScalingMode::WindowSize);
    cam.clear_color = Color::BLACK;
    let solid = |color: Color, x: f32| {
        Sprite {
            size: Some(vec2(8.0, 8.0)),
            color,
            ..Sprite::new(TextureId::WHITE)
        }
        .at(x, 0.0)
    };
    let sprites = [
        solid(Color::rgb8(128, 64, 200), -8.0),
        solid(Color::WHITE.with_alpha(0.5), 8.0),
    ];
    let Some(image) = render(
        uvec2(32, 16),
        &cam,
        &textures,
        &sprites,
        &DebugShapes::default(),
    ) else {
        return;
    };
    let close = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 1);
    let left = image.pixel(8, 8).unwrap();
    let right = image.pixel(24, 8).unwrap();
    assert!(close(left, [128, 64, 200, 255]), "{left:?}");
    // Half of linear white is sRGB 188, not 128 as gamma-space blending would give.
    assert!(close(right, [188, 188, 188, 255]), "{right:?}");
}

trait SpriteExt {
    fn at(self, x: f32, y: f32) -> Self;
    fn with_texture(self, texture: TextureId) -> Self;
}

impl SpriteExt for Sprite {
    fn at(self, x: f32, y: f32) -> Self {
        Self {
            position: vec2(x, y),
            ..self
        }
    }

    fn with_texture(self, texture: TextureId) -> Self {
        Self { texture, ..self }
    }
}
