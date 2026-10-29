//! Sprites: the [`Sprite`] description and the per-frame [`Sprites`] queue.

use crate::{Color, TextureId};
use bevy_ecs::resource::Resource;
use cg_core::Rect;
use cg_core::glam::Vec2;

/// One textured quad to draw this frame.
///
/// Build it with struct-update syntax:
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::{Color, Sprite, TextureId};
///
/// let player = Sprite {
///     texture: TextureId::WHITE,
///     position: vec2(100.0, 20.0),
///     size: Some(vec2(16.0, 24.0)),
///     pivot: vec2(0.5, 0.0), // feet
///     color: Color::hex(0x3a86ff),
///     ..default()
/// };
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    /// The texture to draw from.
    pub texture: TextureId,
    /// Where the pivot lands, in world units (y up).
    pub position: Vec2,
    /// Counter-clockwise rotation around the pivot, in radians.
    pub rotation: f32,
    /// Multiplies the size. Negative values mirror the sprite.
    pub scale: Vec2,
    /// Size in world units before `scale`. `None` uses the source rectangle's size in
    /// texels, so one texel is one world unit.
    pub size: Option<Vec2>,
    /// The part of the texture to draw, in texels from the top-left. `None` draws all of it.
    pub source: Option<Rect>,
    /// The point that sits at `position` and that the sprite rotates around, as a fraction
    /// of its size: `(0, 0)` is the bottom-left corner, `(0.5, 0.5)` the centre (the
    /// default), `(0.5, 0)` the bottom centre.
    pub pivot: Vec2,
    /// Multiplies the texture's colours; white leaves them unchanged.
    pub color: Color,
    /// Mirrors the texture horizontally.
    pub flip_x: bool,
    /// Mirrors the texture vertically.
    pub flip_y: bool,
    /// Draw order: higher layers are drawn on top. Within a layer, sprites are grouped by
    /// texture (one draw call each) and keep their queue order within each texture, so use
    /// layers when sprites of different textures must overlap in a set order.
    pub layer: i32,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            texture: TextureId::WHITE,
            position: Vec2::ZERO,
            rotation: 0.0,
            scale: Vec2::ONE,
            size: None,
            source: None,
            pivot: Vec2::splat(0.5),
            color: Color::WHITE,
            flip_x: false,
            flip_y: false,
            layer: 0,
        }
    }
}

impl Sprite {
    /// A sprite showing all of `texture` at the origin.
    pub fn new(texture: TextureId) -> Self {
        Self {
            texture,
            ..Self::default()
        }
    }
}

/// The sprites to draw this frame.
///
/// Systems queue sprites any time before [`Render`](cg_core::Render); the renderer draws
/// them, then empties the queue. Phase 3 adds `Sprite` components that feed it.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::{Sprite, Sprites, TextureId};
///
/// fn draw(mut sprites: ResMut<Sprites>) {
///     sprites.draw(Sprite {
///         position: vec2(0.0, 0.0),
///         size: Some(vec2(32.0, 32.0)),
///         ..Sprite::new(TextureId::WHITE)
///     });
/// }
/// ```
#[derive(Resource, Debug, Default)]
pub struct Sprites {
    queue: Vec<Sprite>,
}

impl Sprites {
    /// Queues a sprite for this frame.
    pub fn draw(&mut self, sprite: Sprite) {
        self.queue.push(sprite);
    }

    /// The sprites queued so far this frame.
    pub fn queued(&self) -> &[Sprite] {
        &self.queue
    }

    /// The number of sprites queued so far this frame.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether nothing is queued.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Drops everything queued. The renderer calls it after drawing.
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

impl Extend<Sprite> for Sprites {
    fn extend<I: IntoIterator<Item = Sprite>>(&mut self, sprites: I) {
        self.queue.extend(sprites);
    }
}
