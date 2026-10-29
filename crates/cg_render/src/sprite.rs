//! Sprites: the [`Sprite`] component, the [`Layer`] component, and the per-frame
//! [`Sprites`] queue for immediate-mode drawing.

use crate::{Color, TextureId, Textures, Visibility};
use bevy_ecs::prelude::*;
use cg_core::Rect;
use cg_core::glam::{Affine2, Vec2};
use cg_world::{PrefabComponent, Transform2D};
use serde::{Deserialize, Serialize};

/// What a sprite looks like: a texture (or part of one), a size, a pivot, a tint and flips.
///
/// On an entity, the renderer draws it every frame at the entity's transform (blended
/// between fixed steps if the entity is [`Interpolated`](cg_world::Interpolated)), on its
/// [`Layer`], unless its [`Visibility`] hides it or it is outside the camera's view.
/// Adding a `Sprite` also adds a [`Transform2D`] and a [`Visibility`].
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::{Color, Layer, Sprite, TextureId};
/// use cg_world::Transform2D;
///
/// fn spawn_player(mut commands: Commands) {
///     commands.spawn((
///         Sprite {
///             size: Some(vec2(16.0, 24.0)),
///             pivot: vec2(0.5, 0.0), // feet
///             color: Color::hex(0x3a86ff),
///             ..Sprite::new(TextureId::WHITE)
///         },
///         Transform2D::from_xy(100.0, 20.0),
///         Layer(1),
///     ));
/// }
/// ```
///
/// For immediate-mode drawing without entities, queue it in [`Sprites`] instead.
///
/// In prefabs the texture is named (see [`Textures::add_named`]) and every field is
/// optional: `"Sprite": (texture: "bunny", pivot: (0.5, 0), color: "#ffcc00")`.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[require(Transform2D, Visibility)]
pub struct Sprite {
    /// The texture to draw from.
    pub texture: TextureId,
    /// Size in world units before the transform's scale. `None` uses the source
    /// rectangle's size in texels, so one texel is one world unit.
    pub size: Option<Vec2>,
    /// The part of the texture to draw, in texels from the top-left. `None` draws all of it.
    pub source: Option<Rect>,
    /// The point that sits at the transform's origin and that the sprite rotates around,
    /// as a fraction of its size: `(0, 0)` is the bottom-left corner, `(0.5, 0.5)` the
    /// centre (the default), `(0.5, 0)` the bottom centre.
    pub pivot: Vec2,
    /// Multiplies the texture's colours; white leaves them unchanged.
    pub color: Color,
    /// Mirrors the texture horizontally.
    pub flip_x: bool,
    /// Mirrors the texture vertically.
    pub flip_y: bool,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            texture: TextureId::WHITE,
            size: None,
            source: None,
            pivot: Vec2::splat(0.5),
            color: Color::WHITE,
            flip_x: false,
            flip_y: false,
        }
    }
}

impl Sprite {
    /// A sprite showing all of `texture`.
    pub fn new(texture: TextureId) -> Self {
        Self {
            texture,
            ..Self::default()
        }
    }

    /// A solid rectangle of `size` world units in `color`.
    pub fn solid(color: Color, size: Vec2) -> Self {
        Self {
            size: Some(size),
            color,
            ..Self::default()
        }
    }

    /// The source rectangle in texels and the size in world units, or `None` if the
    /// texture is missing.
    pub(crate) fn source_and_size(&self, textures: &Textures) -> Option<(Rect, Vec2)> {
        let texture_size = textures.size(self.texture)?.as_vec2();
        let source = self
            .source
            .unwrap_or_else(|| Rect::from_min_size(Vec2::ZERO, texture_size));
        let size = self.size.unwrap_or_else(|| source.size().abs());
        Some((source, size))
    }

    /// The quad in the sprite's local space, before its transform: `size` placed so the
    /// pivot is at the origin. `None` if the texture is missing.
    pub fn local_rect(&self, textures: &Textures) -> Option<Rect> {
        let (_, size) = self.source_and_size(textures)?;
        let corner = -self.pivot * size;
        Some(Rect::from_corners(corner, corner + size))
    }
}

/// The prefab form of a [`Sprite`]: the same fields, with the texture given by name.
///
/// An empty or missing texture name means [`TextureId::WHITE`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpriteData {
    /// The texture's name in [`Textures`].
    pub texture: String,
    /// See [`Sprite::size`].
    pub size: Option<Vec2>,
    /// See [`Sprite::source`].
    pub source: Option<Rect>,
    /// See [`Sprite::pivot`].
    pub pivot: Vec2,
    /// See [`Sprite::color`].
    pub color: Color,
    /// See [`Sprite::flip_x`].
    pub flip_x: bool,
    /// See [`Sprite::flip_y`].
    pub flip_y: bool,
}

impl Default for SpriteData {
    fn default() -> Self {
        let sprite = Sprite::default();
        Self {
            texture: String::new(),
            size: sprite.size,
            source: sprite.source,
            pivot: sprite.pivot,
            color: sprite.color,
            flip_x: sprite.flip_x,
            flip_y: sprite.flip_y,
        }
    }
}

impl PrefabComponent for Sprite {
    type Data = SpriteData;

    fn from_data(data: &SpriteData, world: &World) -> Result<Self, String> {
        let texture = if data.texture.is_empty() {
            TextureId::WHITE
        } else {
            world
                .get_resource::<Textures>()
                .and_then(|textures| textures.named(&data.texture))
                .ok_or_else(|| format!("no texture called `{}`", data.texture))?
        };
        Ok(Self {
            texture,
            size: data.size,
            source: data.source,
            pivot: data.pivot,
            color: data.color,
            flip_x: data.flip_x,
            flip_y: data.flip_y,
        })
    }
}

/// Draw order for an entity's [`Sprite`]: higher layers are drawn on top. Entities
/// without one are on layer 0.
///
/// Within a layer, sprites are grouped by texture (one draw call each), so when sprites
/// of different textures overlap, put them on different layers.
///
/// In prefabs: `"Layer": (2)`.
#[derive(
    Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
)]
pub struct Layer(pub i32);

/// One sprite in the [`Sprites`] queue: its look, where it goes and its layer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueuedSprite {
    /// What to draw.
    pub sprite: Sprite,
    /// Where: the world transform the sprite's local quad is mapped through.
    pub transform: Affine2,
    /// Draw order; see [`Layer`].
    pub layer: i32,
}

/// The sprites to draw this frame, for immediate-mode drawing.
///
/// Systems queue sprites any time before [`Render`](cg_core::Render); the renderer adds the
/// visible [`Sprite`] entities, draws everything, then empties the queue.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::{Color, Sprite, Sprites};
/// use cg_world::Transform2D;
///
/// fn draw(mut sprites: ResMut<Sprites>) {
///     let square = Sprite::solid(Color::hex(0xffbe0b), vec2(32.0, 32.0));
///     sprites.draw(square, Transform2D::from_xy(0.0, 0.0));
///     sprites.draw_on(2, square, Transform2D::from_xy(40.0, 0.0).with_rotation(0.5));
/// }
/// ```
#[derive(Resource, Debug, Default)]
pub struct Sprites {
    queue: Vec<QueuedSprite>,
}

impl Sprites {
    /// Queues a sprite on layer 0. The transform is a [`Transform2D`], a
    /// [`GlobalTransform2D`](cg_world::GlobalTransform2D) or an [`Affine2`].
    pub fn draw(&mut self, sprite: Sprite, transform: impl Into<Affine2>) {
        self.draw_on(0, sprite, transform);
    }

    /// Queues a sprite on a layer.
    pub fn draw_on(&mut self, layer: i32, sprite: Sprite, transform: impl Into<Affine2>) {
        self.queue.push(QueuedSprite {
            sprite,
            transform: transform.into(),
            layer,
        });
    }

    /// Queues a sprite.
    pub fn push(&mut self, sprite: QueuedSprite) {
        self.queue.push(sprite);
    }

    /// The sprites queued so far this frame.
    pub fn queued(&self) -> &[QueuedSprite] {
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

impl Extend<QueuedSprite> for Sprites {
    fn extend<I: IntoIterator<Item = QueuedSprite>>(&mut self, sprites: I) {
        self.queue.extend(sprites);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Image;
    use cg_core::glam::vec2;

    #[test]
    fn local_rect_follows_size_and_pivot() {
        let mut textures = Textures::default();
        let tall = textures.add(Image::filled(4, 8, Color::RED));
        let sprite = Sprite {
            pivot: vec2(0.5, 0.0),
            ..Sprite::new(tall)
        };
        assert_eq!(
            sprite.local_rect(&textures),
            Some(Rect::new(-2.0, 0.0, 2.0, 8.0))
        );
        let sized = Sprite::solid(Color::RED, vec2(10.0, 2.0));
        assert_eq!(
            sized.local_rect(&textures),
            Some(Rect::new(-5.0, -1.0, 5.0, 1.0))
        );
        textures.remove(tall);
        assert_eq!(sprite.local_rect(&textures), None);
    }

    #[test]
    fn prefab_data_resolves_texture_names() {
        let mut world = World::new();
        let mut textures = Textures::default();
        let bunny = textures.add_named("bunny", Image::filled(2, 2, Color::RED));
        world.insert_resource(textures);

        let data: SpriteData = ron::from_str(
            r##"(texture: "bunny", pivot: (0.5, 0), color: "#ffcc00", flip_x: true)"##,
        )
        .unwrap();
        let sprite = Sprite::from_data(&data, &world).unwrap();
        assert_eq!(
            sprite,
            Sprite {
                pivot: vec2(0.5, 0.0),
                color: Color::hex(0xffcc00),
                flip_x: true,
                ..Sprite::new(bunny)
            }
        );

        let plain = Sprite::from_data(&SpriteData::default(), &world).unwrap();
        assert_eq!(plain, Sprite::default());

        let missing = SpriteData {
            texture: "carrot".into(),
            ..SpriteData::default()
        };
        assert_eq!(
            Sprite::from_data(&missing, &world),
            Err("no texture called `carrot`".to_owned())
        );
    }
}
