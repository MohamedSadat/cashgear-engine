//! The 2D camera: the [`Camera2D`] component, its virtual-resolution [`ScalingMode`]s, the
//! resolved [`CameraView`] with world/screen conversion, and the [`MainCamera`] system
//! parameter.

use crate::Color;
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use cg_core::Rect;
use cg_core::glam::{Affine2, Mat4, Quat, UVec2, Vec2, Vec3};
use cg_platform::Window;
use cg_world::{GlobalTransform2D, Transform2D};
use serde::{Deserialize, Serialize};

/// The default virtual resolution: 640×360, which scales by a whole number to 720p (2×),
/// 1080p (3×), 1440p (4×) and 4K (6×).
pub const DEFAULT_VIRTUAL_RESOLUTION: UVec2 = UVec2::new(640, 360);

/// The smallest zoom the camera uses; smaller values are raised to it.
const MIN_ZOOM: f32 = 1e-4;

/// How the camera's view is fitted to the window.
///
/// In RON: `PixelPerfect((320, 180))`, `Letterbox((640, 360))`, `WindowSize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// A 2D camera. Spawn an entity with it; the entity's transform places the camera:
/// translation is the world point at the centre of the view, rotation turns the view
/// (the world appears to turn the other way), and scale is ignored (use `zoom`).
///
/// The renderer draws through the one active camera. With several active cameras it picks
/// the one with the lowest entity index and warns once; with none, it uses a default
/// camera at the origin.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::{Camera2D, ScalingMode};
/// use cg_world::{Interpolated, Transform2D};
///
/// fn spawn_camera(mut commands: Commands) {
///     commands.spawn((
///         Camera2D {
///             zoom: 2.0,
///             scaling: ScalingMode::PixelPerfect(uvec2(320, 180)),
///             ..default()
///         },
///         Transform2D::from_xy(120.0, 40.0),
///         // Follows something that moves in `FixedUpdate`, so draw it smoothly too.
///         Interpolated::default(),
///     ));
/// }
/// ```
///
/// The world is y-up. Screen positions (as in
/// [`Pointers`](cg_platform::input::Pointers)) are logical pixels from the window's
/// top-left corner, y down; [`MainCamera`] converts between them.
///
/// In prefabs every field is optional: `"Camera2D": (zoom: 2)`.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[require(Transform2D)]
#[serde(default)]
pub struct Camera2D {
    /// Magnification: 2 shows half as much of the world, twice as large.
    pub zoom: f32,
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
    /// Whether the renderer may draw through this camera. On by default; turn it off to
    /// keep a camera around unused.
    pub active: bool,
}

impl Default for Camera2D {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            scaling: ScalingMode::default(),
            clear_color: Color::hex(0x1d1f24),
            bar_color: Color::BLACK,
            pixel_snap: true,
            active: true,
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
}

/// A camera's settings together with where it is: everything needed to draw a frame or
/// convert between world and screen positions.
///
/// The renderer builds one from the active [`Camera2D`] each frame; [`MainCamera`] builds
/// one for game code; the [`OffscreenRenderer`](crate::OffscreenRenderer) takes one
/// directly.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::{Camera2D, CameraView};
///
/// let view = CameraView {
///     position: vec2(100.0, 50.0),
///     ..CameraView::new(Camera2D::default())
/// };
/// assert_eq!(view.camera.zoom, 1.0);
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CameraView {
    /// The camera's settings.
    pub camera: Camera2D,
    /// The world point at the centre of the view.
    pub position: Vec2,
    /// Counter-clockwise rotation of the camera, in radians.
    pub rotation: f32,
}

impl CameraView {
    /// A view through `camera` from the origin, unrotated.
    pub fn new(camera: Camera2D) -> Self {
        Self {
            camera,
            position: Vec2::ZERO,
            rotation: 0.0,
        }
    }

    /// A view through `camera` placed by a world transform (its scale is ignored).
    pub fn from_transform(camera: Camera2D, transform: Affine2) -> Self {
        Self {
            camera,
            position: transform.translation,
            rotation: transform.matrix2.x_axis.to_angle(),
        }
    }

    /// The viewport for a render target; see [`Camera2D::viewport`].
    pub fn viewport(&self, target_size: UVec2, scale_factor: f32) -> Viewport {
        self.camera.viewport(target_size, scale_factor)
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
        self.visible_rect_in(window.physical_size(), window.scale_factor() as f32)
    }

    /// The world-space bounding box of what the camera shows in a render target.
    pub fn visible_rect_in(&self, target_size: UVec2, scale_factor: f32) -> Rect {
        let viewport = self.viewport(target_size, scale_factor);
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

/// Picks the camera the renderer uses: the active one with the lowest entity index (entity
/// ordering itself is not by index). Returns it and whether other active cameras were
/// passed over.
pub(crate) fn pick_active<'a, T>(
    cameras: impl Iterator<Item = (Entity, &'a Camera2D, T)>,
) -> (Option<(Entity, &'a Camera2D, T)>, bool) {
    let mut picked: Option<(Entity, &Camera2D, T)> = None;
    let mut several = false;
    for camera in cameras.filter(|(_, camera, _)| camera.active) {
        match &picked {
            Some((entity, ..)) => {
                several = true;
                if camera.0.index_u32() < entity.index_u32() {
                    picked = Some(camera);
                }
            }
            None => picked = Some(camera),
        }
    }
    (picked, several)
}

/// The active camera, for game code: where it is and how to convert between world and
/// screen positions. It uses the camera's [`GlobalTransform2D`], so moves made this frame
/// count after [`PostUpdate`](cg_core::PostUpdate).
///
/// ```
/// use cg_core::prelude::*;
/// use cg_platform::input::Pointers;
/// use cg_render::MainCamera;
///
/// fn aim(camera: MainCamera, pointers: Res<Pointers>) {
///     if let Some(pointer) = pointers.primary() {
///         let target = camera.screen_to_world(pointer.position());
///         info!("aiming at {target}");
///     }
/// }
/// ```
#[derive(SystemParam)]
pub struct MainCamera<'w, 's> {
    cameras: Query<'w, 's, (Entity, &'static Camera2D, &'static GlobalTransform2D)>,
    window: Res<'w, Window>,
}

impl MainCamera<'_, '_> {
    /// The active camera's entity, if there is one.
    pub fn entity(&self) -> Option<Entity> {
        pick_active(self.cameras.iter())
            .0
            .map(|(entity, ..)| entity)
    }

    /// The active camera's view, or a default camera at the origin if there is none.
    pub fn view(&self) -> CameraView {
        match pick_active(self.cameras.iter()).0 {
            Some((_, camera, transform)) => CameraView::from_transform(camera.clone(), transform.0),
            None => CameraView::default(),
        }
    }

    /// The screen position (logical pixels from the window's top-left) of a world point.
    pub fn world_to_screen(&self, world: Vec2) -> Vec2 {
        self.view().world_to_screen(world, &self.window)
    }

    /// The world point under a screen position (logical pixels from the window's top-left).
    pub fn screen_to_world(&self, screen: Vec2) -> Vec2 {
        self.view().screen_to_world(screen, &self.window)
    }

    /// The world-space bounding box of what the camera shows.
    pub fn visible_rect(&self) -> Rect {
        self.view().visible_rect(&self.window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::glam::{uvec2, vec2};

    fn view(scaling: ScalingMode) -> CameraView {
        CameraView::new(Camera2D {
            scaling,
            ..Camera2D::default()
        })
    }

    #[test]
    fn pixel_perfect_uses_whole_scales_and_centres() {
        let viewport = view(ScalingMode::default()).viewport(uvec2(1920, 1080), 1.0);
        assert_eq!(viewport.rect, Rect::new(0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(viewport.render_size, uvec2(640, 360));

        // 2.5× fits; 2× is used and the rest is bars.
        let viewport = view(ScalingMode::default()).viewport(uvec2(1600, 1000), 1.0);
        assert_eq!(viewport.rect, Rect::new(160.0, 140.0, 1440.0, 860.0));
        assert_eq!(viewport.world_size, vec2(640.0, 360.0));
    }

    #[test]
    fn pixel_perfect_shrinks_when_the_window_is_small() {
        let viewport = view(ScalingMode::default()).viewport(uvec2(320, 360), 1.0);
        assert_eq!(viewport.rect.size(), vec2(320.0, 180.0));
        assert_eq!(viewport.render_size, uvec2(640, 360));
    }

    #[test]
    fn letterbox_keeps_the_aspect_ratio() {
        let mode = ScalingMode::Letterbox(uvec2(640, 360));
        let viewport = view(mode).viewport(uvec2(1000, 1000), 1.0);
        assert_eq!(viewport.rect, Rect::new(0.0, 219.0, 1000.0, 781.0));
        assert_eq!(viewport.render_size, uvec2(1000, 562));
    }

    #[test]
    fn window_size_and_stretch_fill_the_target() {
        let viewport = view(ScalingMode::WindowSize).viewport(uvec2(800, 600), 2.0);
        assert_eq!(viewport.world_size, vec2(400.0, 300.0));
        let viewport = view(ScalingMode::Stretch(uvec2(100, 100))).viewport(uvec2(800, 600), 2.0);
        assert_eq!(viewport.world_size, vec2(100.0, 100.0));
        assert_eq!(viewport.rect.size(), vec2(800.0, 600.0));
    }

    #[test]
    fn zoom_shrinks_the_world_view() {
        let mut view = view(ScalingMode::default());
        view.camera.zoom = 2.0;
        assert_eq!(
            view.viewport(uvec2(1280, 720), 1.0).world_size,
            vec2(320.0, 180.0)
        );
    }

    #[test]
    fn world_and_screen_round_trip() {
        let mut view = view(ScalingMode::Letterbox(uvec2(640, 360)));
        view.position = vec2(50.0, -20.0);
        view.camera.zoom = 1.5;
        view.rotation = 0.3;
        let target = uvec2(1000, 700);
        for point in [vec2(0.0, 0.0), vec2(55.0, -10.0), vec2(-100.0, 80.0)] {
            let pixel = view.world_to_target(point, target, 1.0);
            let back = view.target_to_world(pixel, target, 1.0);
            assert!(back.distance(point) < 1e-3, "{point} -> {pixel} -> {back}");
        }
    }

    #[test]
    fn screen_is_y_down_from_the_top_left() {
        let view = view(ScalingMode::PixelPerfect(uvec2(640, 360)));
        let target = uvec2(1280, 720);
        // The camera centre is the screen centre; up in the world is up on screen.
        assert_eq!(
            view.world_to_target(Vec2::ZERO, target, 1.0),
            vec2(640.0, 360.0)
        );
        assert_eq!(
            view.world_to_target(vec2(-320.0, 180.0), target, 1.0),
            vec2(0.0, 0.0)
        );
        assert_eq!(
            view.target_to_world(vec2(1280.0, 720.0), target, 1.0),
            vec2(320.0, -180.0)
        );
    }

    #[test]
    fn views_come_from_transforms() {
        let transform = Transform2D::from_xy(10.0, 20.0)
            .with_rotation(0.5)
            .with_scale(vec2(3.0, 3.0));
        let view = CameraView::from_transform(Camera2D::default(), transform.affine());
        assert_eq!(view.position, vec2(10.0, 20.0));
        assert!((view.rotation - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_active_camera_with_the_lowest_index_wins() {
        let mut world = World::new();
        let off = Camera2D {
            active: false,
            ..Camera2D::default()
        };
        let a = world.spawn_empty().id();
        let b = world.spawn_empty().id();
        let c = world.spawn_empty().id();
        let on = Camera2D::default();
        let cameras = [(c, &on, ()), (a, &off, ()), (b, &on, ())];
        let (picked, several) = pick_active(cameras.into_iter());
        assert_eq!(picked.map(|(e, ..)| e), Some(b));
        assert!(several);
        assert!(pick_active([(a, &off, ())].into_iter()).0.is_none());
    }

    #[test]
    fn camera_reads_from_ron_with_defaults() {
        let camera: Camera2D =
            ron::from_str("(zoom: 2, scaling: PixelPerfect((320, 180)))").unwrap();
        assert_eq!(camera.zoom, 2.0);
        assert_eq!(camera.scaling, ScalingMode::PixelPerfect(uvec2(320, 180)));
        assert!(camera.active && camera.pixel_snap);
    }
}
