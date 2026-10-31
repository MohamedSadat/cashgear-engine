//! Tile animations from Tiled's animation editor, played on tile layers and tile objects.

use crate::map::Tileset;
use bevy_ecs::prelude::*;
use cg_core::{Rect, Time};
use cg_render::Sprite;
use std::sync::Arc;

/// A tile's animation, ready to draw: the source rectangle of each frame and how long it
/// shows. Tiles animated in Tiled's tile animation editor play in tile layers on their
/// own; tile objects of an animated tile get this component, which sets their sprite's
/// source every frame.
///
/// Every copy of a tile plays in step, as in Tiled, on the game clock
/// ([`Time::elapsed`]), so animations stop while the time scale is 0. Remove the
/// component to hold a tile object on its own tile.
#[derive(Component, Debug, Clone, PartialEq)]
#[require(Sprite)]
pub struct TileAnimation {
    frames: Arc<[(Rect, f32)]>,
    cycle: f32,
}

impl TileAnimation {
    /// An animation of these frames: a source rectangle in texels and seconds each. Frames
    /// with no time are skipped; `None` if none are left.
    pub fn new(frames: impl IntoIterator<Item = (Rect, f32)>) -> Option<Self> {
        let frames: Arc<[(Rect, f32)]> = frames.into_iter().filter(|(_, d)| *d > 0.0).collect();
        let cycle = frames.iter().map(|(_, d)| d).sum();
        (!frames.is_empty()).then_some(Self { frames, cycle })
    }

    /// The animation of tile `id` of `tileset`, if Tiled gives it one.
    pub fn of_tile(tileset: &Tileset, id: u32) -> Option<Self> {
        let info = tileset.tiles.get(&id)?;
        Self::new(
            info.animation
                .iter()
                .map(|frame| (tileset.tile_rect(frame.tile), frame.duration)),
        )
    }

    /// The frames: source rectangles and seconds each.
    pub fn frames(&self) -> &[(Rect, f32)] {
        &self.frames
    }

    /// Seconds for one pass through the frames.
    pub fn cycle(&self) -> f32 {
        self.cycle
    }

    /// The source rectangle shown `seconds` after the clock started.
    pub fn source_at(&self, seconds: f64) -> Rect {
        let mut t = (seconds.rem_euclid(f64::from(self.cycle))) as f32;
        for &(rect, duration) in self.frames.iter() {
            if t < duration {
                return rect;
            }
            t -= duration;
        }
        self.frames[self.frames.len() - 1].0
    }
}

/// Shows the current frame on every animated tile object.
#[profiling::function]
pub(crate) fn animate_tile_objects(
    time: Res<Time>,
    mut tiles: Query<(&TileAnimation, &mut Sprite)>,
) {
    let seconds = time.elapsed_secs();
    for (animation, mut sprite) in &mut tiles {
        let source = Some(animation.source_at(seconds));
        if sprite.source != source {
            sprite.source = source;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_follow_the_clock_and_loop() {
        let a = Rect::new(0.0, 0.0, 8.0, 8.0);
        let b = Rect::new(8.0, 0.0, 16.0, 8.0);
        let animation = TileAnimation::new([(a, 0.1), (b, 0.3), (a, 0.0)]).unwrap();
        assert_eq!(animation.frames().len(), 2, "the empty frame is skipped");
        assert!((animation.cycle() - 0.4).abs() < 1e-6);
        assert_eq!(animation.source_at(0.0), a);
        assert_eq!(animation.source_at(0.15), b);
        assert_eq!(animation.source_at(0.45), a, "loops");
        assert_eq!(
            animation.source_at(-0.05),
            b,
            "before the start counts back"
        );
        assert!(TileAnimation::new([(a, 0.0)]).is_none());
    }
}
