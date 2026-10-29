//! Drawing the UI: each visible node's background, image, border and text become
//! screen-space sprites, in [`UiStack`] order.

use crate::Node;
use crate::layout::UiStack;
use crate::node::{ComputedNode, Edges};
use crate::text::TextLayout;
use crate::widgets::{BackgroundColor, BorderColor, UiImage};
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, Assets};
use cg_core::Rect;
use cg_core::glam::Vec2;
use cg_render::{Color, Image, QueuedSprite, Sprites};

type DrawItem = (
    &'static ComputedNode,
    &'static Node,
    Option<&'static BackgroundColor>,
    Option<&'static BorderColor>,
    Option<&'static UiImage>,
    Option<&'static TextLayout>,
);

/// Queues the UI into the screen-space sprite queue. Runs in `Render`, in
/// `RenderSystems::Queue`.
pub(crate) fn draw_ui(
    stack: Res<UiStack>,
    nodes: Query<DrawItem>,
    images: Res<Assets<Image>>,
    mut sprites: ResMut<Sprites>,
) {
    let scale = stack.space.scale.max(f32::EPSILON);
    for &entity in &stack.entities {
        let Ok((computed, node, background, border, image, text)) = nodes.get(entity) else {
            continue;
        };
        let rect = computed.rect;
        let layer = computed.layer;
        if let Some(BackgroundColor(color)) = background
            && color.a > 0.0
        {
            sprites.push_screen(QueuedSprite::screen_rect(
                AssetId::DEFAULT,
                None,
                rect,
                *color,
                layer,
            ));
        }
        if let Some(image) = image {
            draw_image(image, rect, layer, &images, &mut sprites);
        }
        if let Some(BorderColor(color)) = border
            && node.border > 0.0
            && color.a > 0.0
        {
            draw_border(rect, node.border, *color, layer, &mut sprites);
        }
        if let Some(text) = text {
            let content = computed.content;
            // Text sits at the top of the content box's width and is centred in its
            // height, on whole physical pixels.
            let drop = ((content.height() - text.size.y) * 0.5).max(0.0);
            let origin = content.min + Vec2::new(0.0, (drop * scale).floor() / scale);
            for glyph in &text.glyphs {
                sprites.push_screen(QueuedSprite::screen_rect(
                    glyph.texture,
                    Some(glyph.source),
                    Rect::from_min_size(origin + glyph.rect.min, glyph.rect.size()),
                    glyph.color,
                    layer,
                ));
            }
        }
    }
}

fn draw_border(rect: Rect, width: f32, color: Color, layer: i32, sprites: &mut Sprites) {
    let w = width.min(rect.width() * 0.5).min(rect.height() * 0.5);
    let pieces = [
        Rect::new(rect.min.x, rect.min.y, rect.max.x, rect.min.y + w),
        Rect::new(rect.min.x, rect.max.y - w, rect.max.x, rect.max.y),
        Rect::new(rect.min.x, rect.min.y + w, rect.min.x + w, rect.max.y - w),
        Rect::new(rect.max.x - w, rect.min.y + w, rect.max.x, rect.max.y - w),
    ];
    for piece in pieces.into_iter().filter(|p| !p.is_empty()) {
        sprites.push_screen(QueuedSprite::screen_rect(
            AssetId::DEFAULT,
            None,
            piece,
            color,
            layer,
        ));
    }
}

fn draw_image(
    image: &UiImage,
    rect: Rect,
    layer: i32,
    images: &Assets<Image>,
    sprites: &mut Sprites,
) {
    let texture = image.texture.id();
    let texture_size = images
        .get(texture)
        .map(|i| i.size().as_vec2())
        .or_else(|| (texture == AssetId::DEFAULT).then_some(Vec2::ONE));
    let Some(texture_size) = texture_size else {
        return;
    };
    let source = image
        .source
        .unwrap_or_else(|| Rect::from_min_size(Vec2::ZERO, texture_size));
    let Some(slice) = image.slice else {
        let mut sprite = QueuedSprite::screen_rect(texture, Some(source), rect, image.color, layer);
        sprite.flip_x = image.flip_x;
        sprites.push_screen(sprite);
        return;
    };
    for (from, to) in nine_slices(source, rect, slice) {
        sprites.push_screen(QueuedSprite::screen_rect(
            texture,
            Some(from),
            to,
            image.color,
            layer,
        ));
    }
}

/// The nine (source, destination) pieces of a nine-slice image, without empty ones.
/// Corners keep their texel size, one texel per UI pixel, shrinking evenly when the
/// destination is smaller than two corners.
pub(crate) fn nine_slices(source: Rect, dest: Rect, slice: Edges) -> Vec<(Rect, Rect)> {
    let fit = |a: f32, b: f32, room: f32| {
        let total = a + b;
        if total > room && total > 0.0 {
            (a * room / total, b * room / total)
        } else {
            (a, b)
        }
    };
    let (dl, dr) = fit(slice.left, slice.right, dest.width());
    let (dt, db) = fit(slice.top, slice.bottom, dest.height());
    let sx = [
        source.min.x,
        source.min.x + slice.left,
        source.max.x - slice.right,
        source.max.x,
    ];
    let sy = [
        source.min.y,
        source.min.y + slice.top,
        source.max.y - slice.bottom,
        source.max.y,
    ];
    let dx = [dest.min.x, dest.min.x + dl, dest.max.x - dr, dest.max.x];
    let dy = [dest.min.y, dest.min.y + dt, dest.max.y - db, dest.max.y];
    let mut pieces = Vec::with_capacity(9);
    for row in 0..3 {
        for col in 0..3 {
            let from = Rect::new(sx[col], sy[row], sx[col + 1], sy[row + 1]);
            let to = Rect::new(dx[col], dy[row], dx[col + 1], dy[row + 1]);
            if !from.is_empty() && !to.is_empty() {
                pieces.push((from, to));
            }
        }
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_slices_keep_corners_and_stretch_the_rest() {
        let source = Rect::new(0.0, 0.0, 12.0, 12.0);
        let dest = Rect::new(10.0, 10.0, 110.0, 50.0);
        let pieces = nine_slices(source, dest, Edges::all(4.0));
        assert_eq!(pieces.len(), 9);
        assert_eq!(
            pieces[0],
            (
                Rect::new(0.0, 0.0, 4.0, 4.0),
                Rect::new(10.0, 10.0, 14.0, 14.0)
            )
        );
        assert_eq!(
            pieces[4],
            (
                Rect::new(4.0, 4.0, 8.0, 8.0),
                Rect::new(14.0, 14.0, 106.0, 46.0)
            )
        );
        assert_eq!(
            pieces[8],
            (
                Rect::new(8.0, 8.0, 12.0, 12.0),
                Rect::new(106.0, 46.0, 110.0, 50.0)
            )
        );
    }

    #[test]
    fn small_nine_slices_shrink_their_corners() {
        let pieces = nine_slices(
            Rect::new(0.0, 0.0, 12.0, 12.0),
            Rect::new(0.0, 0.0, 6.0, 40.0),
            Edges::all(4.0),
        );
        // No middle column is left; the corners take 3 UI pixels each.
        assert_eq!(pieces.len(), 6);
        assert_eq!(pieces[0].1, Rect::new(0.0, 0.0, 3.0, 4.0));
        assert_eq!(pieces[1].1, Rect::new(3.0, 0.0, 6.0, 4.0));
    }
}
