//! Sprites: the [`Sprite`] component, the [`Layer`] component, and the per-frame
//! [`Sprites`] queue for immediate-mode drawing.

use crate::{Color, Image, Visibility};
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, AssetServer, Assets, Handle};
use cg_core::Rect;
use cg_core::glam::{Affine2, UVec2, Vec2};
use cg_world::{PrefabComponent, Transform2D};
use serde::{Deserialize, Serialize};

/// What a sprite looks like: a texture (or part of one), a size, a pivot, a tint and flips.
///
/// On an entity, the renderer draws it every frame at the entity's transform (blended
/// between fixed steps if the entity is [`Interpolated`](cg_world::Interpolated)), on its
/// [`Layer`], unless its [`Visibility`] hides it or it is outside the camera's view.
/// Adding a `Sprite` also adds a [`Transform2D`] and a [`Visibility`].
///
/// The texture is a [`Handle<Image>`], which keeps the image loaded while the sprite
/// exists. Until the image has loaded the sprite draws nothing. The default handle is a
/// 1×1 white texture, for solid rectangles.
///
/// ```
/// use cg_assets::AssetServer;
/// use cg_core::prelude::*;
/// use cg_render::{Color, Layer, Sprite};
/// use cg_world::Transform2D;
///
/// fn spawn_player(mut commands: Commands, server: Res<AssetServer>) {
///     commands.spawn((
///         Sprite {
///             pivot: vec2(0.5, 0.0), // feet
///             ..Sprite::new(server.load("images/player.png"))
///         },
///         Transform2D::from_xy(100.0, 20.0),
///         Layer(1),
///     ));
///     commands.spawn((
///         Sprite::solid(Color::hex(0x3a86ff), vec2(16.0, 24.0)),
///         Transform2D::from_xy(140.0, 20.0),
///     ));
/// }
/// ```
///
/// For immediate-mode drawing without entities, queue it in [`Sprites`] instead.
///
/// In prefabs the texture is an asset path and every field is optional:
/// `"Sprite": (texture: "images/bunny.png", pivot: (0.5, 0), color: "#ffcc00")`.
#[derive(Component, Debug, Clone, PartialEq)]
#[require(Transform2D, Visibility)]
pub struct Sprite {
    /// The texture to draw from.
    pub texture: Handle<Image>,
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
    /// Mixes the drawn colour towards this colour by this colour's alpha, keeping the
    /// sprite's shape and transparency: white at alpha 1 draws a white silhouette, for hit
    /// flashes. Transparent (the default) leaves the sprite as it is.
    pub flash: Color,
}

impl Default for Sprite {
    fn default() -> Self {
        Self {
            texture: Handle::default(),
            size: None,
            source: None,
            pivot: Vec2::splat(0.5),
            color: Color::WHITE,
            flip_x: false,
            flip_y: false,
            flash: Color::TRANSPARENT,
        }
    }
}

impl Sprite {
    /// A sprite showing all of `texture`.
    pub fn new(texture: Handle<Image>) -> Self {
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

    /// The quad in the sprite's local space, before its transform: `size` placed so the
    /// pivot is at the origin. `None` until the texture has loaded.
    pub fn local_rect(&self, images: &Assets<Image>) -> Option<Rect> {
        QueuedSprite::new(self, Affine2::IDENTITY, 0).local_rect(images)
    }
}

/// The size of a texture, counting the built-in white texture at [`AssetId::DEFAULT`].
pub(crate) fn texture_size(images: &Assets<Image>, id: AssetId<Image>) -> Option<UVec2> {
    images
        .get(id)
        .map(Image::size)
        .or_else(|| (id == AssetId::DEFAULT).then_some(UVec2::ONE))
}

/// The prefab form of a [`Sprite`]: the same fields, with the texture given by its asset
/// path.
///
/// An empty or missing texture means the white texture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpriteData {
    /// The texture's asset path, such as `images/bunny.png`, loaded through the
    /// [`AssetServer`] when the prefab is spawned.
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
    /// See [`Sprite::flash`].
    pub flash: Color,
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
            flash: sprite.flash,
        }
    }
}

impl PrefabComponent for Sprite {
    type Data = SpriteData;

    fn from_data(data: &SpriteData, world: &World) -> Result<Self, String> {
        let texture = if data.texture.is_empty() {
            Handle::default()
        } else {
            world
                .get_resource::<AssetServer>()
                .ok_or_else(|| format!("no asset server to load `{}`", data.texture))?
                .load(data.texture.as_str())
        };
        Ok(Self {
            texture,
            size: data.size,
            source: data.source,
            pivot: data.pivot,
            color: data.color,
            flip_x: data.flip_x,
            flip_y: data.flip_y,
            flash: data.flash,
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

/// One sprite in the [`Sprites`] queue: a [`Sprite`]'s look with its texture as an id,
/// where it goes and its layer. It is `Copy`, so queueing never touches reference counts;
/// whoever queues it keeps the texture loaded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueuedSprite {
    /// The texture to draw from.
    pub texture: AssetId<Image>,
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
    /// See [`Sprite::flash`].
    pub flash: Color,
    /// Where: the world transform the sprite's local quad is mapped through.
    pub transform: Affine2,
    /// Draw order; see [`Layer`].
    pub layer: i32,
}

impl QueuedSprite {
    /// `sprite` at `transform` on `layer`.
    pub fn new(sprite: &Sprite, transform: impl Into<Affine2>, layer: i32) -> Self {
        Self {
            texture: sprite.texture.id(),
            size: sprite.size,
            source: sprite.source,
            pivot: sprite.pivot,
            color: sprite.color,
            flip_x: sprite.flip_x,
            flip_y: sprite.flip_y,
            flash: sprite.flash,
            transform: transform.into(),
            layer,
        }
    }

    /// The source rectangle in texels and the size in world units, or `None` if the
    /// texture is missing.
    pub(crate) fn source_and_size(&self, images: &Assets<Image>) -> Option<(Rect, Vec2)> {
        let texture_size = texture_size(images, self.texture)?.as_vec2();
        let source = self
            .source
            .unwrap_or_else(|| Rect::from_min_size(Vec2::ZERO, texture_size));
        let size = self.size.unwrap_or_else(|| source.size().abs());
        Some((source, size))
    }

    /// The quad in local space, before the transform; see [`Sprite::local_rect`].
    pub fn local_rect(&self, images: &Assets<Image>) -> Option<Rect> {
        let (_, size) = self.source_and_size(images)?;
        let corner = -self.pivot * size;
        Some(Rect::from_corners(corner, corner + size))
    }

    /// A sprite filling `rect` in screen space, for [`Sprites::push_screen`]: `texture`
    /// (or its `source` part) stretched over the rectangle, upright, tinted by `color`.
    /// Screen space runs y down, so the sprite's quad is mapped with y flipped.
    pub fn screen_rect(
        texture: AssetId<Image>,
        source: Option<Rect>,
        rect: Rect,
        color: Color,
        layer: i32,
    ) -> Self {
        Self {
            texture,
            size: Some(rect.size()),
            source,
            pivot: Vec2::ZERO,
            color,
            flip_x: false,
            flip_y: false,
            flash: Color::TRANSPARENT,
            transform: Affine2::from_cols(Vec2::X, Vec2::NEG_Y, Vec2::new(rect.min.x, rect.max.y)),
            layer,
        }
    }
}

/// The sprites to draw this frame, for immediate-mode drawing.
///
/// Systems queue sprites any time before [`Render`](cg_core::Render); the renderer adds the
/// visible [`Sprite`] entities, draws everything, then empties the queue. The queue holds
/// texture ids, not handles: keep the handles somewhere (a resource, say) for as long as
/// you draw with them.
///
/// There are two queues. World sprites are drawn through the camera, sorted by layer then
/// texture. Screen sprites ([`push_screen`](Self::push_screen)) are drawn after them, over
/// the whole window at its own resolution, in [`ScreenSpace`](crate::ScreenSpace) units
/// (the UI's pixels: origin at the view's top-left corner, y down), sorted by layer and
/// otherwise in queue order. The UI and the screen fade draw there.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::{Color, Sprite, Sprites};
/// use cg_world::Transform2D;
///
/// fn draw(mut sprites: ResMut<Sprites>) {
///     let square = Sprite::solid(Color::hex(0xffbe0b), vec2(32.0, 32.0));
///     sprites.draw(&square, Transform2D::from_xy(0.0, 0.0));
///     sprites.draw_on(2, &square, Transform2D::from_xy(40.0, 0.0).with_rotation(0.5));
/// }
/// ```
#[derive(Resource, Debug, Default)]
pub struct Sprites {
    queue: Vec<QueuedSprite>,
    screen: Vec<QueuedSprite>,
}

impl Sprites {
    /// Queues a sprite on layer 0. The transform is a [`Transform2D`], a
    /// [`GlobalTransform2D`](cg_world::GlobalTransform2D) or an [`Affine2`].
    pub fn draw(&mut self, sprite: &Sprite, transform: impl Into<Affine2>) {
        self.draw_on(0, sprite, transform);
    }

    /// Queues a sprite on a layer.
    pub fn draw_on(&mut self, layer: i32, sprite: &Sprite, transform: impl Into<Affine2>) {
        self.queue.push(QueuedSprite::new(sprite, transform, layer));
    }

    /// Queues a sprite.
    pub fn push(&mut self, sprite: QueuedSprite) {
        self.queue.push(sprite);
    }

    /// Queues a sprite in screen space; see [`QueuedSprite::screen_rect`].
    pub fn push_screen(&mut self, sprite: QueuedSprite) {
        self.screen.push(sprite);
    }

    /// The world sprites queued so far this frame.
    pub fn queued(&self) -> &[QueuedSprite] {
        &self.queue
    }

    /// The screen sprites queued so far this frame.
    pub fn screen_queued(&self) -> &[QueuedSprite] {
        &self.screen
    }

    /// The number of world sprites queued so far this frame.
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// Whether no world sprite is queued.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Drops everything queued, in both queues. The renderer calls it after drawing.
    pub fn clear(&mut self) {
        self.queue.clear();
        self.screen.clear();
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
    use cg_assets::{AppAssetExt, AssetPlugin};
    use cg_core::glam::vec2;

    #[test]
    fn local_rect_follows_size_and_pivot() {
        let mut images = Assets::<Image>::default();
        let tall = images.add(Image::filled(4, 8, Color::RED));
        let sprite = Sprite {
            pivot: vec2(0.5, 0.0),
            ..Sprite::new(tall.clone())
        };
        assert_eq!(
            sprite.local_rect(&images),
            Some(Rect::new(-2.0, 0.0, 2.0, 8.0))
        );
        let sized = Sprite::solid(Color::RED, vec2(10.0, 2.0));
        assert_eq!(
            sized.local_rect(&images),
            Some(Rect::new(-5.0, -1.0, 5.0, 1.0))
        );
        images.remove(&tall);
        assert_eq!(sprite.local_rect(&images), None);
    }

    #[test]
    fn prefab_data_loads_texture_paths() {
        let mut app = cg_core::App::new();
        app.add_plugins(AssetPlugin {
            hot_reload: Some(false),
            ..AssetPlugin::default()
        })
        .init_asset::<Image>();
        let world = app.world();
        let data: SpriteData = ron::from_str(
            r##"(texture: "images/bunny.png", pivot: (0.5, 0), color: "#ffcc00", flip_x: true)"##,
        )
        .unwrap();
        let sprite = Sprite::from_data(&data, world).unwrap();
        let server = world.resource::<AssetServer>();
        assert_eq!(
            sprite,
            Sprite {
                pivot: vec2(0.5, 0.0),
                color: Color::hex(0xffcc00),
                flip_x: true,
                ..Sprite::new(server.load("images/bunny.png"))
            }
        );
        assert!(sprite.texture.is_strong());

        let plain = Sprite::from_data(&SpriteData::default(), world).unwrap();
        assert_eq!(plain, Sprite::default());

        let no_server = World::new();
        assert_eq!(
            Sprite::from_data(&data, &no_server),
            Err("no asset server to load `images/bunny.png`".to_owned())
        );
    }
}
