//! Turning queued sprites into sorted GPU instances and per-texture batches.

use crate::sprite::texture_size;
use crate::{Image, QueuedSprite};
use bytemuck::{Pod, Zeroable};
use cg_assets::{AssetId, Assets};
use cg_core::glam::Vec2;
use core::ops::Range;

/// One sprite as the GPU sees it: an affine transform of the unit square, texture
/// coordinates and a tint. Matches `Instance` in `sprite.wgsl`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct SpriteInstance {
    /// The world vector along the sprite's local x edge (its width, rotated).
    pub(crate) x_axis: [f32; 2],
    /// The world vector along the sprite's local y edge (its height, rotated).
    pub(crate) y_axis: [f32; 2],
    /// The world position of the bottom-left corner.
    pub(crate) translation: [f32; 2],
    /// Texture coordinates at the left, bottom, right and top edges, flips applied.
    pub(crate) uv: [f32; 4],
    /// sRGB tint with straight alpha.
    pub(crate) color: [f32; 4],
}

/// A run of instances drawn with one texture in one draw call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Batch {
    pub(crate) texture: AssetId<Image>,
    pub(crate) instances: Range<u32>,
}

/// Reusable buffers for building batches each frame.
#[derive(Default)]
pub(crate) struct Batcher {
    order: Vec<u32>,
    pub(crate) instances: Vec<SpriteInstance>,
    pub(crate) batches: Vec<Batch>,
}

impl Batcher {
    /// Sorts `sprites` by layer then texture (keeping queue order otherwise), builds their
    /// instances and groups them into batches. Sprites with a missing texture or no area
    /// are skipped.
    pub(crate) fn build(&mut self, sprites: &[QueuedSprite], images: &Assets<Image>) {
        self.order.clear();
        self.instances.clear();
        self.batches.clear();

        self.order.extend(0..sprites.len() as u32);
        let key = |&i: &u32| {
            let queued = &sprites[i as usize];
            (queued.layer, queued.texture)
        };
        // Queues are usually built in order already; skip the sort then.
        if !self.order.is_sorted_by_key(key) {
            self.order.sort_by_key(key);
        }

        for &i in &self.order {
            let queued = &sprites[i as usize];
            let Some(instance) = instance(queued, images) else {
                continue;
            };
            let texture = queued.texture;
            let index = self.instances.len() as u32;
            self.instances.push(instance);
            match self.batches.last_mut() {
                Some(batch) if batch.texture == texture => batch.instances.end = index + 1,
                _ => self.batches.push(Batch {
                    texture,
                    instances: index..index + 1,
                }),
            }
        }
    }
}

/// The GPU instance for one sprite: its local quad (pivot at the origin) mapped through
/// its transform, which may rotate, scale, mirror or skew it.
fn instance(queued: &QueuedSprite, images: &Assets<Image>) -> Option<SpriteInstance> {
    let texture_size = texture_size(images, queued.texture)?.as_vec2();
    let (source, size) = queued.source_and_size(images)?;
    let transform = queued.transform;
    if size.x == 0.0 || size.y == 0.0 || transform.matrix2.determinant() == 0.0 {
        return None;
    }

    let x_axis = transform.matrix2 * Vec2::new(size.x, 0.0);
    let y_axis = transform.matrix2 * Vec2::new(0.0, size.y);
    let translation = transform.transform_point2(-queued.pivot * size);

    // Texel rows run top to bottom, so the sprite's bottom edge samples `source.max.y`.
    let (mut left, mut right) = (source.min.x, source.max.x);
    let (mut bottom, mut top) = (source.max.y, source.min.y);
    if queued.flip_x {
        core::mem::swap(&mut left, &mut right);
    }
    if queued.flip_y {
        core::mem::swap(&mut bottom, &mut top);
    }

    Some(SpriteInstance {
        x_axis: x_axis.to_array(),
        y_axis: y_axis.to_array(),
        translation: translation.to_array(),
        uv: [
            left / texture_size.x,
            bottom / texture_size.y,
            right / texture_size.x,
            top / texture_size.y,
        ],
        color: queued.color.to_array(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Sprite};
    use cg_assets::Handle;
    use cg_core::Rect;
    use cg_core::glam::vec2;
    use cg_world::Transform2D;

    fn at(sprite: &Sprite, transform: Transform2D) -> QueuedSprite {
        QueuedSprite::new(sprite, transform, 0)
    }

    fn on(layer: i32, sprite: &Sprite) -> QueuedSprite {
        QueuedSprite::new(sprite, Transform2D::IDENTITY, layer)
    }

    fn images() -> (Assets<Image>, Handle<Image>, Handle<Image>) {
        let mut images = Assets::default();
        let a = images.add(Image::filled(8, 4, Color::RED));
        let b = images.add(Image::filled(2, 2, Color::BLUE));
        (images, a, b)
    }

    #[test]
    fn sorts_by_layer_then_texture_and_batches_runs() {
        let (images, a, b) = images();
        let (sa, sb) = (Sprite::new(a.clone()), Sprite::new(b.clone()));
        let sprites = [on(1, &sa), on(0, &sb), on(0, &sa), on(0, &sb), on(-1, &sb)];
        let mut batcher = Batcher::default();
        batcher.build(&sprites, &images);
        let (a, b) = (a.id(), b.id());
        assert_eq!(
            batcher.batches,
            [
                Batch {
                    texture: b,
                    instances: 0..1
                },
                Batch {
                    texture: a,
                    instances: 1..2
                },
                Batch {
                    texture: b,
                    instances: 2..4
                },
                Batch {
                    texture: a,
                    instances: 4..5
                },
            ]
        );
    }

    #[test]
    fn skips_missing_textures_and_empty_sprites() {
        let (mut images, a, b) = images();
        images.remove(&b);
        let (sa, sb) = (Sprite::new(a), Sprite::new(b));
        let sprites = [
            on(0, &sb),
            at(&sa, Transform2D::from_scale(Vec2::ZERO)),
            on(0, &sa),
        ];
        let mut batcher = Batcher::default();
        batcher.build(&sprites, &images);
        assert_eq!(batcher.instances.len(), 1);
        assert_eq!(batcher.batches.len(), 1);
    }

    #[test]
    fn the_default_texture_is_white_even_when_not_loaded() {
        let images = Assets::<Image>::default();
        let square = Sprite::solid(Color::RED, vec2(3.0, 3.0));
        let mut batcher = Batcher::default();
        batcher.build(&[on(0, &square)], &images);
        assert_eq!(batcher.batches[0].texture, AssetId::DEFAULT);
        assert_eq!(batcher.instances[0].x_axis, [3.0, 0.0]);
    }

    #[test]
    fn instance_geometry_follows_pivot_size_and_rotation() {
        let (images, a, _) = images();
        let sprite = Sprite {
            pivot: vec2(0.5, 0.0),
            ..Sprite::new(a)
        };
        let placed = Transform2D::from_xy(10.0, 20.0);
        let instance = instance_of(&at(&sprite, placed), &images);
        // The texture is 8×4 texels, so the sprite is 8×4 world units.
        assert_eq!(instance.x_axis, [8.0, 0.0]);
        assert_eq!(instance.y_axis, [0.0, 4.0]);
        assert_eq!(instance.translation, [6.0, 20.0]);

        let turned = placed.with_rotation(core::f32::consts::FRAC_PI_2);
        let instance = instance_of(&at(&sprite, turned), &images);
        assert!(Vec2::from_array(instance.x_axis).distance(vec2(0.0, 8.0)) < 1e-5);
        assert!(Vec2::from_array(instance.y_axis).distance(vec2(-4.0, 0.0)) < 1e-5);
    }

    #[test]
    fn instance_keeps_skew_from_the_transform() {
        let (images, a, _) = images();
        // A parent stretched along x holding a turned child: a skewed matrix.
        let parent = Transform2D::from_scale(vec2(2.0, 1.0)).affine();
        let child = Transform2D::from_rotation(core::f32::consts::FRAC_PI_4).affine();
        let queued = QueuedSprite::new(&Sprite::new(a), parent * child, 0);
        let instance = instance_of(&queued, &images);
        let x_axis = (parent * child).matrix2 * vec2(8.0, 0.0);
        let y_axis = (parent * child).matrix2 * vec2(0.0, 4.0);
        assert!(Vec2::from_array(instance.x_axis).distance(x_axis) < 1e-5);
        assert!(Vec2::from_array(instance.y_axis).distance(y_axis) < 1e-5);
    }

    fn instance_of(queued: &QueuedSprite, images: &Assets<Image>) -> SpriteInstance {
        instance(queued, images).unwrap()
    }

    #[test]
    fn uvs_follow_source_rect_and_flips() {
        let (images, a, _) = images();
        let sprite = Sprite {
            source: Some(Rect::new(2.0, 0.0, 6.0, 2.0)),
            ..Sprite::new(a)
        };
        let instance = instance_of(&on(0, &sprite), &images);
        assert_eq!(instance.uv, [0.25, 0.5, 0.75, 0.0]);
        assert_eq!(instance.x_axis, [4.0, 0.0]);

        let flipped = Sprite {
            flip_x: true,
            flip_y: true,
            ..sprite
        };
        assert_eq!(
            instance_of(&on(0, &flipped), &images).uv,
            [0.75, 0.0, 0.25, 0.5]
        );
    }
}
