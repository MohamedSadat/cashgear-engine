//! Sprite sheets: frames cut from one image, and named animation clips over them.

use cg_assets::{Asset, Handle};
use cg_core::Rect;
use cg_core::glam::{Vec2, vec2};
use cg_render::{Image, TextureAtlas};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Frames cut from one image and named [`AnimationClip`]s that play them: an Aseprite
/// export (see [`AsepriteLoader`](crate::AsepriteLoader)), or a sheet built in code from a
/// [`TextureAtlas`].
///
/// ```
/// use cg_anim::{AnimationClip, Direction, SpriteSheet};
/// use cg_assets::Handle;
/// use cg_core::Rect;
/// use cg_render::TextureAtlas;
/// use cg_core::glam::UVec2;
/// use std::collections::BTreeMap;
///
/// // An atlas with three bat frames, as `cg_pack atlas` would make it.
/// let regions = (0..3)
///     .map(|i| (format!("bat_{i}"), Rect::new(i as f32 * 16.0, 0.0, (i + 1) as f32 * 16.0, 16.0)))
///     .collect::<BTreeMap<_, _>>();
/// let atlas = TextureAtlas::new(Handle::default(), UVec2::new(48, 16), regions);
///
/// let mut sheet = SpriteSheet::from_atlas(&atlas);
/// let flap = sheet.clip_from_frames(&["bat_0", "bat_1", "bat_2"], 8.0).unwrap();
/// sheet.add_clip("flap", flap.with_direction(Direction::PingPong));
/// assert_eq!(sheet.clip("flap").unwrap().cycle_len(), 4); // 0 1 2 1
/// ```
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpriteSheet {
    /// The image the frames are cut from.
    pub image: Handle<Image>,
    /// The frames, in the order the export or atlas lists them.
    pub frames: Vec<SheetFrame>,
    /// Each frame's name (Aseprite's file name for the frame, or the atlas region), by
    /// frame index.
    pub frame_names: Vec<String>,
    /// The clips by name: Aseprite tags, or clips added in code.
    pub clips: BTreeMap<String, AnimationClip>,
}

impl Asset for SpriteSheet {}

impl SpriteSheet {
    /// An empty sheet over `image`.
    pub fn new(image: Handle<Image>) -> Self {
        Self {
            image,
            ..Self::default()
        }
    }

    /// A sheet with one frame per atlas region, in name order, named after the regions and
    /// shown for a tenth of a second each. Add clips with
    /// [`clip_from_frames`](Self::clip_from_frames).
    pub fn from_atlas(atlas: &TextureAtlas) -> Self {
        let mut sheet = Self::new(atlas.image.clone());
        for name in atlas.names() {
            if let Some(rect) = atlas.region(name) {
                sheet.add_frame(name, SheetFrame::new(rect, 0.1));
            }
        }
        sheet
    }

    /// Adds a frame and returns its index.
    pub fn add_frame(&mut self, name: impl Into<String>, frame: SheetFrame) -> usize {
        self.frames.push(frame);
        self.frame_names.push(name.into());
        self.frames.len() - 1
    }

    /// The index of the frame called `name`.
    pub fn frame_index(&self, name: &str) -> Option<usize> {
        self.frame_names.iter().position(|n| n == name)
    }

    /// The clip called `name`.
    pub fn clip(&self, name: &str) -> Option<&AnimationClip> {
        self.clips.get(name)
    }

    /// Adds a clip, replacing any clip of the same name.
    pub fn add_clip(&mut self, name: impl Into<String>, clip: AnimationClip) -> &mut Self {
        self.clips.insert(name.into(), clip);
        self
    }

    /// A looping clip of the named frames in this order, at `fps` frames per second; frames
    /// may repeat. Fails with the first name the sheet does not have.
    pub fn clip_from_frames(&self, names: &[&str], fps: f32) -> Result<AnimationClip, String> {
        let duration = if fps > 0.0 { 1.0 / fps } else { f32::INFINITY };
        names
            .iter()
            .map(|name| {
                self.frame_index(name)
                    .map(|frame| ClipFrame { frame, duration })
                    .ok_or_else(|| format!("the sheet has no frame `{name}`"))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(AnimationClip::new)
    }
}

/// One frame of a [`SpriteSheet`]: where it is in the image and, for trimmed exports,
/// where it sat in the untrimmed frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheetFrame {
    /// The frame's texels in the image, from the top-left.
    pub rect: Rect,
    /// The size of the untrimmed frame in texels: `rect`'s size unless the export trimmed
    /// empty borders off.
    pub source_size: Vec2,
    /// Where `rect` sits inside the untrimmed frame, in texels from its top-left.
    pub offset: Vec2,
    /// How long the frame shows by default, in seconds.
    pub duration: f32,
}

impl SheetFrame {
    /// An untrimmed frame.
    pub fn new(rect: Rect, duration: f32) -> Self {
        Self {
            rect,
            source_size: rect.size(),
            offset: Vec2::ZERO,
            duration,
        }
    }

    /// The [`Sprite::pivot`](cg_render::Sprite::pivot) that puts the point `pivot` of the
    /// untrimmed frame (a fraction of its size, `(0, 0)` at the bottom-left) at the
    /// sprite's origin. With a flip, the frame mirrors around that point, so a character
    /// turns round on the spot even when its frames are trimmed unevenly.
    pub fn pivot(&self, pivot: Vec2, flip_x: bool, flip_y: bool) -> Vec2 {
        let size = self.rect.size();
        if size.x <= 0.0 || size.y <= 0.0 {
            return pivot;
        }
        // The trimmed rectangle inside the untrimmed frame, with y up.
        let min = vec2(self.offset.x, self.source_size.y - self.offset.y - size.y);
        let mut fraction = (pivot * self.source_size - min) / size;
        if flip_x {
            fraction.x = 1.0 - fraction.x;
        }
        if flip_y {
            fraction.y = 1.0 - fraction.y;
        }
        fraction
    }
}

/// Which way an [`AnimationClip`] plays its frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Direction {
    /// First to last.
    #[default]
    Forward,
    /// Last to first.
    Reverse,
    /// First to last and back, without showing the ends twice: 0 1 2 1, then again.
    PingPong,
    /// Last to first and back: 2 1 0 1, then again.
    PingPongReverse,
}

/// A frame of a clip: which sheet frame, for how long.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClipFrame {
    /// The index of the frame in [`SpriteSheet::frames`].
    pub frame: usize,
    /// Seconds it shows at speed 1.
    pub duration: f32,
}

/// Something that happens when a clip reaches a frame, such as a footstep; sent as an
/// [`AnimationEvent`](crate::AnimationEvent).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipEvent {
    /// The index into the clip's [`frames`](AnimationClip::frames) (not the sheet's).
    pub frame: usize,
    /// The event's name.
    pub name: String,
}

/// A named run of a sheet's frames, played by a
/// [`SpriteAnimation`](crate::SpriteAnimation): an Aseprite tag, or a clip built in code.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationClip {
    /// The frames in forward order; [`direction`](Self::direction) decides how they play.
    pub frames: Vec<ClipFrame>,
    /// Which way the frames play.
    pub direction: Direction,
    /// How many times the clip plays before it finishes and holds its last frame. `None`
    /// loops for ever. One play of a ping-pong clip goes there and back, and a finished
    /// ping-pong clip holds the frame it started on.
    pub repeat: Option<u32>,
    /// Events fired when a frame starts showing, each time it does.
    pub events: Vec<ClipEvent>,
}

impl AnimationClip {
    /// A looping, forward clip of these frames.
    pub fn new(frames: impl IntoIterator<Item = ClipFrame>) -> Self {
        Self {
            frames: frames.into_iter().collect(),
            ..Self::default()
        }
    }

    /// Plays in `direction`.
    pub fn with_direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }

    /// Plays `times` times, then finishes on its last frame.
    pub fn with_repeat(mut self, times: u32) -> Self {
        self.repeat = Some(times);
        self
    }

    /// Plays once, then finishes on its last frame.
    pub fn once(self) -> Self {
        self.with_repeat(1)
    }

    /// Fires the event `name` whenever clip frame `frame` starts showing.
    pub fn with_event(mut self, frame: usize, name: impl Into<String>) -> Self {
        self.events.push(ClipEvent {
            frame,
            name: name.into(),
        });
        self
    }

    /// The number of frames shown in one play: the frame count, or for ping-pong clips
    /// the way there and back without repeating the ends.
    pub fn cycle_len(&self) -> usize {
        let n = self.frames.len();
        match self.direction {
            Direction::Forward | Direction::Reverse => n,
            Direction::PingPong | Direction::PingPongReverse if n > 1 => 2 * n - 2,
            Direction::PingPong | Direction::PingPongReverse => n,
        }
    }

    /// The index into [`frames`](Self::frames) shown at step `step` of a play
    /// (`0..cycle_len()`).
    pub fn frame_at(&self, step: usize) -> usize {
        let n = self.frames.len();
        if n == 0 {
            return 0;
        }
        let step = step % self.cycle_len().max(1);
        let there_and_back = if step < n { step } else { 2 * n - 2 - step };
        match self.direction {
            Direction::Forward => step,
            Direction::Reverse => n - 1 - step,
            Direction::PingPong => there_and_back,
            Direction::PingPongReverse => n - 1 - there_and_back,
        }
    }

    /// The index into [`frames`](Self::frames) a finished clip holds.
    pub fn end_frame(&self) -> usize {
        match self.direction {
            Direction::Forward | Direction::PingPongReverse => self.frames.len().saturating_sub(1),
            Direction::Reverse | Direction::PingPong => 0,
        }
        .min(self.frames.len().saturating_sub(1))
    }

    /// Seconds one play takes at speed 1.
    pub fn duration(&self) -> f32 {
        (0..self.cycle_len())
            .map(|step| self.frames[self.frame_at(step)].duration)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(n: usize, direction: Direction) -> AnimationClip {
        AnimationClip::new((0..n).map(|frame| ClipFrame {
            frame,
            duration: 0.1,
        }))
        .with_direction(direction)
    }

    fn order(clip: &AnimationClip) -> Vec<usize> {
        (0..clip.cycle_len()).map(|s| clip.frame_at(s)).collect()
    }

    #[test]
    fn directions_order_the_frames() {
        assert_eq!(order(&clip(3, Direction::Forward)), [0, 1, 2]);
        assert_eq!(order(&clip(3, Direction::Reverse)), [2, 1, 0]);
        assert_eq!(order(&clip(3, Direction::PingPong)), [0, 1, 2, 1]);
        assert_eq!(order(&clip(3, Direction::PingPongReverse)), [2, 1, 0, 1]);
        assert_eq!(order(&clip(1, Direction::PingPong)), [0]);
        assert_eq!(order(&clip(2, Direction::PingPong)), [0, 1]);
        assert_eq!(clip(3, Direction::PingPong).end_frame(), 0);
        assert_eq!(clip(3, Direction::Forward).end_frame(), 2);
        assert!((clip(3, Direction::PingPong).duration() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn untrimmed_frames_keep_the_pivot_and_mirror_it() {
        let frame = SheetFrame::new(Rect::new(0.0, 0.0, 24.0, 24.0), 0.1);
        let feet = vec2(0.5, 0.0);
        assert_eq!(frame.pivot(feet, false, false), feet);
        assert_eq!(frame.pivot(feet, true, false), feet);
        assert_eq!(frame.pivot(vec2(0.25, 0.0), true, false), vec2(0.75, 0.0));
    }

    #[test]
    fn trimmed_frames_move_the_pivot_so_the_picture_stays_put() {
        // A 32×32 frame trimmed to the 10×20 box 4 texels from its left and 8 from its top.
        let frame = SheetFrame {
            rect: Rect::new(100.0, 0.0, 110.0, 20.0),
            source_size: vec2(32.0, 32.0),
            offset: vec2(4.0, 8.0),
            duration: 0.1,
        };
        // The untrimmed frame's centre is at (16, 16) with y up; the box spans x 4..14 and
        // y 4..24 (y up), so the centre is at (12, 12) inside it.
        let pivot = frame.pivot(vec2(0.5, 0.5), false, false);
        assert_eq!(pivot, vec2(12.0 / 10.0, 12.0 / 20.0));
        // Flipped, the box mirrors around the same point.
        assert_eq!(
            frame.pivot(vec2(0.5, 0.5), true, false),
            vec2(1.0 - 1.2, 0.6)
        );
    }

    #[test]
    fn atlas_clips_name_their_frames() {
        let regions = BTreeMap::from([
            ("b".to_owned(), Rect::new(8.0, 0.0, 16.0, 8.0)),
            ("a".to_owned(), Rect::new(0.0, 0.0, 8.0, 8.0)),
        ]);
        let atlas = TextureAtlas::new(Handle::default(), cg_core::glam::UVec2::new(16, 8), regions);
        let sheet = SpriteSheet::from_atlas(&atlas);
        assert_eq!(sheet.frame_names, ["a", "b"]);
        let clip = sheet.clip_from_frames(&["b", "a", "b"], 4.0).unwrap();
        assert_eq!(
            clip.frames.iter().map(|f| f.frame).collect::<Vec<_>>(),
            [1, 0, 1]
        );
        assert_eq!(clip.frames[0].duration, 0.25);
        assert_eq!(
            sheet.clip_from_frames(&["c"], 4.0).unwrap_err(),
            "the sheet has no frame `c`"
        );
    }
}
