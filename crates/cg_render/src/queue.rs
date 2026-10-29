//! Turning ECS entities into a frame: the active camera becomes the [`FrameView`], and
//! visible [`Sprite`] entities inside it join the [`Sprites`] queue.

use crate::camera::pick_active;
use crate::{
    Camera2D, CameraView, Image, InheritedVisibility, Layer, RenderStats, Sprite, Sprites,
};
use bevy_ecs::prelude::*;
use cg_assets::Assets;
use cg_core::Rect;
use cg_core::glam::Affine2;
use cg_platform::Window;
use cg_world::{GlobalTransform2D, Interpolated, InterpolatedTransforms};

/// The view the renderer draws this frame with, chosen from the active [`Camera2D`] by
/// [`RenderSystems::View`].
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct FrameView(pub CameraView);

/// System sets in the [`Render`](cg_core::Render) schedule, in this order.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderSystems {
    /// Picks the active camera and sets the [`FrameView`].
    View,
    /// Queues the visible sprite entities. Systems that queue more sprites from the
    /// [`FrameView`] (tilemaps, say) go here too, `.after(RenderSystems::View)`.
    Queue,
    /// Draws the queues into the window and empties them.
    Draw,
}

type SpriteItem = (
    &'static Sprite,
    &'static GlobalTransform2D,
    &'static InheritedVisibility,
    Option<&'static Layer>,
    Option<&'static Interpolated>,
    Option<&'static ChildOf>,
);

type CameraItem = (
    Entity,
    &'static Camera2D,
    (
        &'static GlobalTransform2D,
        Option<&'static Interpolated>,
        Option<&'static ChildOf>,
    ),
);

/// Picks the active camera and makes its view this frame's [`FrameView`].
pub(crate) fn pick_camera(
    cameras: Query<CameraItem>,
    transforms: InterpolatedTransforms,
    mut view: ResMut<FrameView>,
    mut stats: ResMut<RenderStats>,
    mut warned: Local<bool>,
) {
    // The queueing systems after this one add to it.
    stats.culled = 0;
    let (active, several) = pick_active(cameras.iter());
    if several && !*warned {
        *warned = true;
        log::warn!("several active cameras; drawing through the one with the lowest entity index");
    }
    view.0 = match active {
        Some((_, camera, (global, interpolated, parent))) => CameraView::from_transform(
            camera.clone(),
            transforms.blend(global.0, interpolated, parent),
        ),
        None => CameraView::default(),
    };
}

/// Queues every visible sprite entity that overlaps the [`FrameView`].
pub(crate) fn queue_world(
    sprites: Query<SpriteItem>,
    transforms: InterpolatedTransforms,
    window: Res<Window>,
    images: Res<Assets<Image>>,
    view: Res<FrameView>,
    mut queue: ResMut<Sprites>,
    mut stats: ResMut<RenderStats>,
) {
    let visible = view.0.visible_rect(&window);

    let mut culled = 0;
    for (sprite, global, shown, layer, interpolated, parent) in &sprites {
        if !shown.get() {
            continue;
        }
        let transform = transforms.blend(global.0, interpolated, parent);
        let Some(bounds) = world_bounds(sprite, transform, &images) else {
            continue;
        };
        if !bounds.overlaps(&visible) {
            culled += 1;
            continue;
        }
        queue.draw_on(layer.map_or(0, |layer| layer.0), sprite, transform);
    }
    stats.add_culled(culled);
}

/// The world-space bounding box of a sprite drawn at `transform`.
fn world_bounds(sprite: &Sprite, transform: Affine2, images: &Assets<Image>) -> Option<Rect> {
    let local = sprite.local_rect(images)?;
    Rect::from_points(
        [
            local.min,
            local.max,
            local.min.with_x(local.max.x),
            local.min.with_y(local.max.y),
        ]
        .map(|corner| transform.transform_point2(corner)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, ScalingMode, Visibility};
    use cg_core::glam::{Vec2, uvec2, vec2};
    use cg_core::{App, Render};
    use cg_world::{Transform2D, WorldPlugin};
    use core::time::Duration;

    /// An app with the queueing systems but no GPU, and a 640×360 window.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(WorldPlugin)
            .init_resource::<Assets<Image>>()
            .init_resource::<Sprites>()
            .init_resource::<FrameView>()
            .init_resource::<RenderStats>()
            .insert_resource(Window::with_size(uvec2(640, 360), 1.0))
            .add_systems(cg_core::PostUpdate, crate::visibility::propagate_visibility)
            .add_systems(Render, (pick_camera, queue_world).chain());
        app
    }

    fn square(x: f32, y: f32) -> (Sprite, Transform2D) {
        (
            Sprite::solid(Color::WHITE, vec2(10.0, 10.0)),
            Transform2D::from_xy(x, y),
        )
    }

    fn queued_positions(app: &App) -> Vec<Vec2> {
        app.world()
            .resource::<Sprites>()
            .queued()
            .iter()
            .map(|s| s.transform.translation)
            .collect()
    }

    #[test]
    fn visible_entities_are_queued_and_the_rest_culled_or_hidden() {
        let mut app = app();
        app.world_mut().spawn(square(0.0, 0.0));
        app.world_mut().spawn(square(1000.0, 0.0));
        // Straddling the right edge (x = 320): still drawn.
        app.world_mut().spawn(square(322.0, 0.0));
        app.world_mut()
            .spawn((square(10.0, 10.0), Visibility::Hidden));
        app.update(Duration::ZERO);
        assert_eq!(queued_positions(&app), [vec2(0.0, 0.0), vec2(322.0, 0.0)]);
        assert_eq!(app.world().resource::<RenderStats>().culled(), 1);
    }

    #[test]
    fn the_camera_entity_sets_the_view_and_moves_culling() {
        let mut app = app();
        app.world_mut().spawn((
            Camera2D {
                zoom: 2.0,
                scaling: ScalingMode::PixelPerfect(uvec2(640, 360)),
                ..Camera2D::default()
            },
            Transform2D::from_xy(1000.0, 0.0).with_rotation(0.25),
        ));
        app.world_mut().spawn(square(0.0, 0.0));
        app.world_mut().spawn(square(1000.0, 0.0));
        app.update(Duration::ZERO);
        let view = &app.world().resource::<FrameView>().0;
        assert_eq!(view.position, vec2(1000.0, 0.0));
        assert!((view.rotation - 0.25).abs() < 1e-6);
        assert_eq!(view.camera.zoom, 2.0);
        assert_eq!(queued_positions(&app), [vec2(1000.0, 0.0)]);
    }

    #[test]
    fn layers_and_child_transforms_reach_the_queue() {
        let mut app = app();
        let parent = app.world_mut().spawn((square(5.0, 0.0), Layer(3))).id();
        app.world_mut().spawn((square(0.0, 7.0), ChildOf(parent)));
        app.update(Duration::ZERO);
        let queued = app.world().resource::<Sprites>().queued().to_vec();
        let mut found: Vec<_> = queued
            .iter()
            .map(|s| (s.layer, s.transform.translation))
            .collect();
        found.sort_by_key(|(layer, _)| *layer);
        assert_eq!(found, [(0, vec2(5.0, 7.0)), (3, vec2(5.0, 0.0))]);
    }
}
