//! The 2D camera, its virtual-resolution [`ScalingMode`]s and world/screen conversion.

use crate::Color;
use bevy_ecs::resource::Resource;
use cg_core::Rect;
use cg_core::glam::{Mat4, Quat, UVec2, Vec2, Vec3};
use cg_platform::Window;

/// The default virtual resolution: 640×360, which scales by a whole number to 720p (2×),
/// 1080p (3×), 1440p (4×) and 4K (6×).
pub const DEFAULT_VIRTUAL_RESOLUTION: UVec2 = UVec2::new(640, 360);

/// The smallest zoom the camera uses; smaller values are raised to it.
const MIN_ZOOM: f32 = 1e-4;

/// How the camera's view is fitted to the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalingMode {
    /// One world unit is one logical pixel, so the view grows with the window.
    WindowSize,
    /// The virtual resolution is stretched over the whole window, distorting when the
    /// aspect ratios differ.
    Stretch(UVec2),
    /// The virtual resolution is scaled up as far as it fits, keeping its aspect ratio, with
    /// bars filling the rest. Sprites are drawn at the window's resolution.
    Letterbox(UVec2),
    /// The scene is drawn into an image of exactly the virtual resolution, which is then
    /// scaled up by the largest whole number that fits, with bars filling the rest. Every
    /// sprite, rotation and camera move lands on the virtual pixel grid. When the window is
    /// smaller than the virtual resolution, the image is scaled down to fit instead. The
    /// default, at [`DEFAULT_VIRTUAL_RESOLUTION`].
    PixelPerfect(UVec2),
}

impl Default for ScalingMode {
    fn default() -> Self {
        Self::PixelPerfect(DEFAULT_VIRTUAL_RESOLUTION)
    }
}

/// The 2D camera: what part of the world is drawn and how it fits the window.
///
/// The world is y-up, and the camera's `position` is the centre of the view. Screen
/// positions (as in [`Pointers`](cg_platform::input::Pointers)) are logical pixels from the
/// window's top-left corner, y down.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::{Camera2D, ScalingMode};
///
/// fn follow(mut camera: ResMut<Camera2D>) {
///     camera.position = vec2(120.0, 40.0);
///     camera.zoom = 2.0;
///     camera.scaling = ScalingMode::PixelPerfect(uvec2(320, 180));
/// }
/// ```
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Camera2D {
    /// The world point at the centre of the view.
    pub position: Vec2,
    /// Magnification: 2 shows half as much of the world, twice as large.
    pub zoom: f32,
    /// Counter-clockwise rotation of the camera, in radians. The world appears to turn the
    /// other way.
    pub rotation: f32,
    /// How the view is fitted to the window.
    pub scaling: ScalingMode,
    /// The background colour inside the view.
    pub clear_color: Color,
    /// The colour of the bars around a letterboxed or pixel-perfect view.
    pub bar_color: Color,
    /// Moves each sprite so its corner lands exactly on a pixel of the image it is drawn
    /// into (the virtual resolution in [`ScalingMode::PixelPerfect`], the window otherwise).
    /// This keeps pixel art from shimmering or doubling texels. On by default.
    pub pixel_snap: bool,
}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            position: Vec2::ZERO,
            zoom: 1.0,
            rotation: 0.0,
            scaling: ScalingMode::default(),
            clear_color: Color::hex(0x1d1f24),
            bar_color: Color::BLACK,
            pixel_snap: true,
        }
    }
}

/// Where and how the camera draws into a render target, as computed by
/// [`Camera2D::viewport`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// The part of the target the view covers, in physical pixels from the top-left. The
    /// rest is bars.
    pub rect: Rect,
    /// The pixel grid sprites are drawn on: the virtual resolution in
    /// [`ScalingMode::PixelPerfect`], otherwise the size of `rect`.
    pub render_size: UVec2,
    /// How many world units the view spans, after zoom.
    pub world_size: Vec2,
}

impl Camera2D {
    /// The viewport for a render target of `target_size` physical pixels with the given
    /// scale factor (physical pixels per logical pixel).
    pub fn viewport(&self, target_size: UVec2, scale_factor: f32) -> Viewport {
        let target = target_size.as_vec2();
        let zoom = self.zoom.max(MIN_ZOOM);
        let full = Rect::from_min_size(Vec2::ZERO, target);
        let fitted = |virtual_size: UVec2, scale: f32| {
            let size = (virtual_size.as_vec2() * scale).floor();
            let min = ((target - size) * 0.5).floor();
            Rect::from_min_size(min, size)
        };
        match self.scaling {
            ScalingMode::WindowSize => Viewport {
                rect: full,
                render_size: target_size,
                world_size: target / scale_factor.max(f32::EPSILON) / zoom,
            },
            ScalingMode::Stretch(size) => Viewport {
                rect: full,
                render_size: target_size,
                world_size: size.max(UVec2::ONE).as_vec2() / zoom,
            },
            ScalingMode::Letterbox(size) => {
                let size = size.max(UVec2::ONE);
                let rect = fitted(size, (target / size.as_vec2()).min_element());
                Viewport {
                    rect,
                    render_size: rect.size().as_uvec2(),
                    world_size: size.as_vec2() / zoom,
                }
            }
            ScalingMode::PixelPerfect(size) => {
                let size = size.max(UVec2::ONE);
                let fit = (target / size.as_vec2()).min_element();
                let scale = if fit >= 1.0 { fit.floor() } else { fit };
                Viewport {
                    rect: fitted(size, scale),
                    render_size: size,
                    world_size: size.as_vec2() / zoom,
                }
            }
        }
    }

    /// The matrix from world coordinates to clip space for a view spanning `world_size`
    /// world units (see [`Viewport::world_size`]).
    pub fn view_projection(&self, world_size: Vec2) -> Mat4 {
        // Orthographic: the view's half-size maps to the clip-space edges, and z stays 0.
        let half = (world_size * 0.5).max(Vec2::splat(f32::EPSILON));
        let projection = Mat4::from_scale(Vec3::new(1.0 / half.x, 1.0 / half.y, 1.0));
        let camera = Mat4::from_rotation_translation(
            Quat::from_rotation_z(self.rotation),
            self.position.extend(0.0),
        );
        projection * camera.inverse()
    }

    /// The screen position (logical pixels from the window's top-left) of a world point.
    pub fn world_to_screen(&self, world: Vec2, window: &Window) -> Vec2 {
        let scale_factor = window.scale_factor() as f32;
        self.world_to_target(world, window.physical_size(), scale_factor) / scale_factor
    }

    /// The world point under a screen position (logical pixels from the window's top-left).
    /// Positions over the bars map to points outside the view.
    pub fn screen_to_world(&self, screen: Vec2, window: &Window) -> Vec2 {
        let scale_factor = window.scale_factor() as f32;
        self.target_to_world(screen * scale_factor, window.physical_size(), scale_factor)
    }

    /// The world-space bounding box of what the camera shows in `window`. With rotation it
    /// is the box around the turned view.
    pub fn visible_rect(&self, window: &Window) -> Rect {
        let viewport = self.viewport(window.physical_size(), window.scale_factor() as f32);
        let half = viewport.world_size * 0.5;
        let (sin, cos) = self.rotation.sin_cos();
        let extent = Vec2::new(
            half.x * cos.abs() + half.y * sin.abs(),
            half.x * sin.abs() + half.y * cos.abs(),
        );
        Rect::from_center_size(self.position, extent * 2.0)
    }

    /// A world point in physical pixels of a target of `target_size`.
    pub(crate) fn world_to_target(&self, world: Vec2, target_size: UVec2, scale: f32) -> Vec2 {
        let viewport = self.viewport(target_size, scale);
        let ndc = self
            .view_projection(viewport.world_size)
            .project_point3(world.extend(0.0))
            .truncate();
        viewport.rect.min + (ndc * Vec2::new(0.5, -0.5) + 0.5) * viewport.rect.size()
    }

    /// The world point at physical pixel `pixel` of a target of `target_size`.
    pub(crate) fn target_to_world(&self, pixel: Vec2, target_size: UVec2, scale: f32) -> Vec2 {
        let viewport = self.viewport(target_size, scale);
        let size = viewport.rect.size().max(Vec2::ONE);
        let ndc = ((pixel - viewport.rect.min) / size - 0.5) * Vec2::new(2.0, -2.0);
        self.view_projection(viewport.world_size)
            .inverse()
            .project_point3(ndc.extend(0.0))
            .truncate()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::glam::{uvec2, vec2};

    fn camera(scaling: ScalingMode) -> Camera2D {
        Camera2D {
            scaling,
            ..Camera2D::default()
        }
    }

    #[test]
    fn pixel_perfect_uses_whole_scales_and_centres() {
        let viewport = camera(ScalingMode::default()).viewport(uvec2(1920, 1080), 1.0);
        assert_eq!(viewport.rect, Rect::new(0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(viewport.render_size, uvec2(640, 360));

        // 2.5× fits; 2× is used and the rest is bars.
        let viewport = camera(ScalingMode::default()).viewport(uvec2(1600, 1000), 1.0);
        assert_eq!(viewport.rect, Rect::new(160.0, 140.0, 1440.0, 860.0));
        assert_eq!(viewport.world_size, vec2(640.0, 360.0));
    }

    #[test]
    fn pixel_perfect_shrinks_when_the_window_is_small() {
        let viewport = camera(ScalingMode::default()).viewport(uvec2(320, 360), 1.0);
        assert_eq!(viewport.rect.size(), vec2(320.0, 180.0));
        assert_eq!(viewport.render_size, uvec2(640, 360));
    }

    #[test]
    fn letterbox_keeps_the_aspect_ratio() {
        let mode = ScalingMode::Letterbox(uvec2(640, 360));
        let viewport = camera(mode).viewport(uvec2(1000, 1000), 1.0);
        assert_eq!(viewport.rect, Rect::new(0.0, 219.0, 1000.0, 781.0));
        assert_eq!(viewport.render_size, uvec2(1000, 562));
    }

    #[test]
    fn window_size_and_stretch_fill_the_target() {
        let viewport = camera(ScalingMode::WindowSize).viewport(uvec2(800, 600), 2.0);
        assert_eq!(viewport.world_size, vec2(400.0, 300.0));
        let viewport = camera(ScalingMode::Stretch(uvec2(100, 100))).viewport(uvec2(800, 600), 2.0);
        assert_eq!(viewport.world_size, vec2(100.0, 100.0));
        assert_eq!(viewport.rect.size(), vec2(800.0, 600.0));
    }

    #[test]
    fn zoom_shrinks_the_world_view() {
        let mut cam = camera(ScalingMode::default());
        cam.zoom = 2.0;
        assert_eq!(
            cam.viewport(uvec2(1280, 720), 1.0).world_size,
            vec2(320.0, 180.0)
        );
    }

    #[test]
    fn world_and_screen_round_trip() {
        let mut cam = camera(ScalingMode::Letterbox(uvec2(640, 360)));
        cam.position = vec2(50.0, -20.0);
        cam.zoom = 1.5;
        cam.rotation = 0.3;
        let target = uvec2(1000, 700);
        for point in [vec2(0.0, 0.0), vec2(55.0, -10.0), vec2(-100.0, 80.0)] {
            let pixel = cam.world_to_target(point, target, 1.0);
            let back = cam.target_to_world(pixel, target, 1.0);
            assert!(back.distance(point) < 1e-3, "{point} -> {pixel} -> {back}");
        }
    }

    #[test]
    fn screen_is_y_down_from_the_top_left() {
        let cam = camera(ScalingMode::PixelPerfect(uvec2(640, 360)));
        let target = uvec2(1280, 720);
        // The camera centre is the screen centre; up in the world is up on screen.
        assert_eq!(
            cam.world_to_target(Vec2::ZERO, target, 1.0),
            vec2(640.0, 360.0)
        );
        assert_eq!(
            cam.world_to_target(vec2(-320.0, 180.0), target, 1.0),
            vec2(0.0, 0.0)
        );
        assert_eq!(
            cam.target_to_world(vec2(1280.0, 720.0), target, 1.0),
            vec2(320.0, -180.0)
        );
    }
}
