//! Turning queued sprites into sorted GPU instances and per-texture batches.

use crate::{Sprite, TextureId, Textures};
use bytemuck::{Pod, Zeroable};
use cg_core::Rect;
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
    pub(crate) texture: TextureId,
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
    pub(crate) fn build(&mut self, sprites: &[Sprite], textures: &Textures) {
        self.order.clear();
        self.instances.clear();
        self.batches.clear();

        self.order.extend(0..sprites.len() as u32);
        let key = |&i: &u32| {
            let sprite = &sprites[i as usize];
            (sprite.layer, sprite.texture)
        };
        // Queues are usually built in order already; skip the sort then.
        if !self.order.is_sorted_by_key(key) {
            self.order.sort_by_key(key);
        }

        for &i in &self.order {
            let sprite = &sprites[i as usize];
            let Some(instance) = instance(sprite, textures) else {
                continue;
            };
            let index = self.instances.len() as u32;
            self.instances.push(instance);
            match self.batches.last_mut() {
                Some(batch) if batch.texture == sprite.texture => batch.instances.end = index + 1,
                _ => self.batches.push(Batch {
                    texture: sprite.texture,
                    instances: index..index + 1,
                }),
            }
        }
    }
}

/// The GPU instance for one sprite.
fn instance(sprite: &Sprite, textures: &Textures) -> Option<SpriteInstance> {
    let texture_size = textures.size(sprite.texture)?.as_vec2();
    let source = sprite
        .source
        .unwrap_or_else(|| Rect::from_min_size(Vec2::ZERO, texture_size));
    let size = sprite.size.unwrap_or_else(|| source.size().abs()) * sprite.scale;
    if size.x == 0.0 || size.y == 0.0 {
        return None;
    }

    let rotation = Vec2::from_angle(sprite.rotation);
    let x_axis = rotation.rotate(Vec2::new(size.x, 0.0));
    let y_axis = rotation.rotate(Vec2::new(0.0, size.y));
    let translation = sprite.position - x_axis * sprite.pivot.x - y_axis * sprite.pivot.y;

    // Texel rows run top to bottom, so the sprite's bottom edge samples `source.max.y`.
    let (mut left, mut right) = (source.min.x, source.max.x);
    let (mut bottom, mut top) = (source.max.y, source.min.y);
    if sprite.flip_x {
        core::mem::swap(&mut left, &mut right);
    }
    if sprite.flip_y {
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
        color: sprite.color.to_array(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Color, Image};
    use cg_core::glam::vec2;

    fn textures() -> (Textures, TextureId, TextureId) {
        let mut textures = Textures::default();
        let a = textures.add(Image::filled(8, 4, Color::RED));
        let b = textures.add(Image::filled(2, 2, Color::BLUE));
        (textures, a, b)
    }

    #[test]
    fn sorts_by_layer_then_texture_and_batches_runs() {
        let (textures, a, b) = textures();
        let sprites = [
            Sprite {
                layer: 1,
                ..Sprite::new(a)
            },
            Sprite::new(b),
            Sprite::new(a),
            Sprite::new(b),
            Sprite {
                layer: -1,
                ..Sprite::new(b)
            },
        ];
        let mut batcher = Batcher::default();
        batcher.build(&sprites, &textures);
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
        let (mut textures, a, b) = textures();
        textures.remove(b);
        let sprites = [
            Sprite::new(b),
            Sprite {
                scale: Vec2::ZERO,
                ..Sprite::new(a)
            },
            Sprite::new(a),
        ];
        let mut batcher = Batcher::default();
        batcher.build(&sprites, &textures);
        assert_eq!(batcher.instances.len(), 1);
        assert_eq!(batcher.batches.len(), 1);
    }

    #[test]
    fn instance_geometry_follows_pivot_size_and_rotation() {
        let (textures, a, _) = textures();
        let sprite = Sprite {
            position: vec2(10.0, 20.0),
            pivot: vec2(0.5, 0.0),
            ..Sprite::new(a)
        };
        let instance = instance(&sprite, &textures).unwrap();
        // The texture is 8×4 texels, so the sprite is 8×4 world units.
        assert_eq!(instance.x_axis, [8.0, 0.0]);
        assert_eq!(instance.y_axis, [0.0, 4.0]);
        assert_eq!(instance.translation, [6.0, 20.0]);

        let turned = Sprite {
            rotation: core::f32::consts::FRAC_PI_2,
            ..sprite
        };
        let instance = instance_of(&turned, &textures);
        assert!(Vec2::from_array(instance.x_axis).distance(vec2(0.0, 8.0)) < 1e-5);
        assert!(Vec2::from_array(instance.y_axis).distance(vec2(-4.0, 0.0)) < 1e-5);
    }

    fn instance_of(sprite: &Sprite, textures: &Textures) -> SpriteInstance {
        instance(sprite, textures).unwrap()
    }

    #[test]
    fn uvs_follow_source_rect_and_flips() {
        let (textures, a, _) = textures();
        let sprite = Sprite {
            source: Some(Rect::new(2.0, 0.0, 6.0, 2.0)),
            ..Sprite::new(a)
        };
        let instance = instance_of(&sprite, &textures);
        assert_eq!(instance.uv, [0.25, 0.5, 0.75, 0.0]);
        assert_eq!(instance.x_axis, [4.0, 0.0]);

        let flipped = Sprite {
            flip_x: true,
            flip_y: true,
            ..sprite
        };
        assert_eq!(instance_of(&flipped, &textures).uv, [0.75, 0.0, 0.25, 0.5]);
    }
}
