//! Playing sprite sheet clips on entities: the [`SpriteAnimation`] component and the
//! messages it sends.

use crate::sheet::{AnimationClip, SpriteSheet};
use bevy_ecs::prelude::*;
use cg_assets::{AssetServer, Assets, Handle};
use cg_core::Time;
use cg_core::glam::{Vec2, vec2};
use cg_render::Sprite;
use cg_world::PrefabComponent;
use serde::{Deserialize, Serialize};

/// Plays a clip of a [`SpriteSheet`] on the entity's [`Sprite`]: every fixed step it
/// advances the clip and sets the sprite's texture, source rectangle and pivot to the
/// current frame. The sprite's size, colour and flips stay the game's; leave the size at
/// `None` for trimmed sheets, and scale with the transform instead.
///
/// Animation runs in fixed steps (after the physics, in
/// [`AnimSystems::Animate`](crate::AnimSystems::Animate)), so frames and
/// [`AnimationEvent`]s line up with gameplay whatever the frame rate. A new or switched
/// clip shows its first frame in the same frame, before the next fixed step. While the
/// sheet is loading the sprite shows whatever it showed before, so spawn animated
/// entities once their sheets have loaded (behind a loading state) or start them hidden.
///
/// ```
/// use cg_anim::SpriteAnimation;
/// use cg_assets::AssetServer;
/// use cg_core::prelude::*;
///
/// fn spawn_hero(mut commands: Commands, server: Res<AssetServer>) {
///     commands.spawn(
///         SpriteAnimation::new(server.load("images/hero.json"), "idle")
///             .with_pivot(vec2(0.5, 0.0)),
///     );
/// }
///
/// fn run(mut heroes: Query<&mut SpriteAnimation>) {
///     for mut animation in &mut heroes {
///         animation.play("run"); // no restart if it is already running
///     }
/// }
/// ```
///
/// In prefabs: `"SpriteAnimation": (sheet: "images/hero.json", clip: "idle", pivot: (0.5, 0))`.
/// An [`Animator`](crate::Animator) on the same entity picks clips from a state machine.
#[derive(Component, Debug, Clone)]
#[require(Sprite)]
pub struct SpriteAnimation {
    /// The sheet the clips come from.
    pub sheet: Handle<SpriteSheet>,
    /// Playback speed: 1 is the clip's own timing, 2 twice as fast. Negative counts as 0.
    pub speed: f32,
    /// Stops the clip where it is.
    pub paused: bool,
    /// The point of the (untrimmed) frame that sits at the entity's origin, as a fraction
    /// of the frame: `(0.5, 0.5)` the centre (the default), `(0.5, 0)` the feet. With
    /// [`Sprite::flip_x`] the frame mirrors around it.
    pub pivot: Vec2,
    clip: String,
    playback: Playback,
}

/// Where a clip is.
#[derive(Debug, Clone, Default, PartialEq)]
struct Playback {
    /// The position in the current play, `0..cycle_len`.
    step: usize,
    /// Seconds into the current frame.
    elapsed: f32,
    /// Finished plays.
    loops: u32,
    finished: bool,
    /// Whether the first frame has been shown (and its events sent).
    started: bool,
    /// A missing clip was reported.
    warned: bool,
}

impl Default for SpriteAnimation {
    fn default() -> Self {
        Self {
            sheet: Handle::default(),
            speed: 1.0,
            paused: false,
            pivot: Vec2::splat(0.5),
            clip: String::new(),
            playback: Playback::default(),
        }
    }
}

impl SpriteAnimation {
    /// Plays `clip` of `sheet` from its start.
    pub fn new(sheet: Handle<SpriteSheet>, clip: impl Into<String>) -> Self {
        Self {
            sheet,
            clip: clip.into(),
            ..Self::default()
        }
    }

    /// Plays at `speed`.
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Uses `pivot`; see [`pivot`](Self::pivot).
    pub fn with_pivot(mut self, pivot: Vec2) -> Self {
        self.pivot = pivot;
        self
    }

    /// The clip playing (or finished).
    pub fn clip(&self) -> &str {
        &self.clip
    }

    /// Switches to `clip` from its start, unless it is already the current clip.
    pub fn play(&mut self, clip: &str) {
        if self.clip != clip {
            self.restart_with(clip);
        }
    }

    /// Plays `clip` from its start, even if it is the current clip.
    pub fn restart_with(&mut self, clip: &str) {
        clip.clone_into(&mut self.clip);
        self.restart();
    }

    /// Plays the current clip again from its start.
    pub fn restart(&mut self) {
        self.playback = Playback::default();
    }

    /// The index into the clip's [`frames`](AnimationClip::frames) being shown.
    pub fn frame(&self, clip: &AnimationClip) -> usize {
        if self.playback.finished {
            clip.end_frame()
        } else {
            clip.frame_at(self.playback.step)
        }
    }

    /// Whether a clip with a repeat count has played to its end.
    pub fn is_finished(&self) -> bool {
        self.playback.finished
    }

    /// How many plays of the current clip have finished.
    pub fn loops(&self) -> u32 {
        self.playback.loops
    }
}

/// A clip reached a frame that carries an event (see
/// [`AnimationClip::events`]): footsteps, the moment a sword hits, a wing flap.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct AnimationEvent {
    /// The animated entity.
    pub entity: Entity,
    /// The clip's name.
    pub clip: String,
    /// The event's name.
    pub name: String,
    /// The index into the clip's frames.
    pub frame: usize,
}

/// A clip with a repeat count played to its end.
#[derive(Message, Debug, Clone, PartialEq)]
pub struct AnimationFinished {
    /// The animated entity.
    pub entity: Entity,
    /// The clip's name.
    pub clip: String,
}

/// The prefab form of a [`SpriteAnimation`]: the sheet by its asset path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpriteAnimationData {
    /// The sheet's asset path, such as `images/hero.json`.
    pub sheet: String,
    /// The clip to start with.
    pub clip: String,
    /// See [`SpriteAnimation::speed`].
    pub speed: f32,
    /// See [`SpriteAnimation::pivot`].
    pub pivot: Vec2,
    /// See [`SpriteAnimation::paused`].
    pub paused: bool,
}

impl Default for SpriteAnimationData {
    fn default() -> Self {
        let animation = SpriteAnimation::default();
        Self {
            sheet: String::new(),
            clip: String::new(),
            speed: animation.speed,
            pivot: animation.pivot,
            paused: animation.paused,
        }
    }
}

impl PrefabComponent for SpriteAnimation {
    type Data = SpriteAnimationData;

    fn from_data(data: &SpriteAnimationData, world: &World) -> Result<Self, String> {
        let server = world
            .get_resource::<AssetServer>()
            .ok_or_else(|| format!("no asset server to load `{}`", data.sheet))?;
        Ok(Self {
            speed: data.speed,
            pivot: data.pivot,
            paused: data.paused,
            ..Self::new(server.load(data.sheet.as_str()), &data.clip)
        })
    }
}

/// Frames shorter than this count as this long, so a bad clip cannot stall the loop.
const MIN_FRAME: f32 = 1e-4;

/// Advances every [`SpriteAnimation`] by one fixed step and shows its frame.
#[profiling::function]
pub(crate) fn play_animations(
    time: Res<Time>,
    sheets: Res<Assets<SpriteSheet>>,
    mut animations: Query<(Entity, &mut SpriteAnimation, &mut Sprite)>,
    mut events: MessageWriter<AnimationEvent>,
    mut finished: MessageWriter<AnimationFinished>,
) {
    let dt = time.fixed_delta_secs();
    for (entity, mut animation, mut sprite) in &mut animations {
        let Some(sheet) = sheets.get(&animation.sheet) else {
            continue;
        };
        let animation = animation.bypass_change_detection();
        let Some(clip) = sheet.clip(&animation.clip).filter(|c| !c.frames.is_empty()) else {
            if !animation.playback.warned {
                animation.playback.warned = true;
                log::warn!(
                    "entity {entity}: the sprite sheet has no clip `{}` with frames",
                    animation.clip
                );
            }
            continue;
        };
        let mut send = |frame: usize| {
            for event in clip.events.iter().filter(|e| e.frame == frame) {
                events.write(AnimationEvent {
                    entity,
                    clip: animation.clip.clone(),
                    name: event.name.clone(),
                    frame,
                });
            }
        };
        let playback = &mut animation.playback;
        // A reloaded sheet may have a shorter clip.
        playback.step = playback.step.min(clip.cycle_len() - 1);
        if !playback.started {
            playback.started = true;
            send(clip.frame_at(0));
        } else if !playback.finished && !animation.paused {
            playback.elapsed += dt * animation.speed.max(0.0);
            // Bounded so that even absurd speeds end the loop.
            for _ in 0..1024 {
                let shown = clip.frame_at(playback.step);
                let duration = clip.frames[shown].duration.max(MIN_FRAME);
                if playback.elapsed < duration {
                    break;
                }
                playback.elapsed -= duration;
                playback.step += 1;
                if playback.step == clip.cycle_len() {
                    playback.step = 0;
                    playback.loops += 1;
                    if clip.repeat.is_some_and(|times| playback.loops >= times) {
                        playback.finished = true;
                        playback.elapsed = 0.0;
                        finished.write(AnimationFinished {
                            entity,
                            clip: animation.clip.clone(),
                        });
                        break;
                    }
                }
                send(clip.frame_at(playback.step));
            }
        }
        show(sheet, clip, animation, &mut sprite);
    }
}

/// Shows the current frame of animations added or changed (a new clip, say) since the
/// last frame, so they appear at once rather than at the next fixed step, even while the
/// game is paused.
#[profiling::function]
pub(crate) fn show_changed_animations(
    sheets: Res<Assets<SpriteSheet>>,
    mut animations: Query<(&SpriteAnimation, &mut Sprite), Changed<SpriteAnimation>>,
) {
    for (animation, mut sprite) in &mut animations {
        let Some(sheet) = sheets.get(&animation.sheet) else {
            continue;
        };
        if let Some(clip) = sheet.clip(&animation.clip).filter(|c| !c.frames.is_empty()) {
            show(sheet, clip, animation, &mut sprite);
        }
    }
}

/// Points the sprite at the animation's current frame, touching it only if something
/// changed.
fn show(
    sheet: &SpriteSheet,
    clip: &AnimationClip,
    animation: &SpriteAnimation,
    sprite: &mut Mut<Sprite>,
) {
    let shown = animation.frame(clip);
    let Some(frame) = sheet.frames.get(clip.frames[shown].frame) else {
        return;
    };
    let pivot = frame.pivot(animation.pivot, sprite.flip_x, sprite.flip_y);
    if sprite.texture.id() != sheet.image.id() {
        sprite.texture = sheet.image.clone();
    }
    if sprite.source != Some(frame.rect) {
        sprite.source = Some(frame.rect);
    }
    if sprite.pivot != pivot {
        sprite.pivot = pivot;
    }
}

/// The pivot fraction `(0.5, 0)`: the bottom centre, where a character's feet are.
pub const FEET: Vec2 = vec2(0.5, 0.0);
