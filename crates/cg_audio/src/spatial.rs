//! Simple 2D positional audio: pan and distance falloff from a listener.

use crate::audio::PlaySettings;
use crate::sound::Sound;
use bevy_ecs::prelude::*;
use cg_assets::Handle;
use cg_core::glam::Vec2;
use serde::{Deserialize, Serialize};

/// How a positioned sound fades with distance and pans with direction, in world units
/// (pixels).
///
/// Within `near` of the listener a sound plays at full volume; from there it fades along a
/// smooth curve to silence at `far`. It pans by its horizontal offset: `pan_width` to the
/// side reaches `max_pan`, which stays short of a hard pan so a sound never vanishes from
/// one ear.
///
/// The defaults suit the engine's 640×360 virtual screen: a sound at the edge of the view
/// is clearly to that side at about a third of full volume, and one a whole screen away is
/// silent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spatial {
    /// Distance within which the sound plays at full volume.
    pub near: f32,
    /// Distance at and beyond which the sound is silent.
    pub far: f32,
    /// Horizontal offset that pans the sound fully (to `max_pan`).
    pub pan_width: f32,
    /// The strongest pan, from 0 (never pan) to 1 (hard left or right).
    pub max_pan: f32,
}

impl Default for Spatial {
    fn default() -> Self {
        Self {
            near: 96.0,
            far: 640.0,
            pan_width: 320.0,
            max_pan: 0.8,
        }
    }
}

impl Spatial {
    /// The volume factor (0 to 1) and panning (-1 to 1) of a sound at `source` heard from
    /// `listener`.
    ///
    /// ```
    /// use cg_audio::Spatial;
    /// use cg_core::glam::Vec2;
    ///
    /// let spatial = Spatial::default();
    /// assert_eq!(spatial.gain_and_pan(Vec2::ZERO, Vec2::new(10.0, 0.0)).0, 1.0);
    /// let (gain, pan) = spatial.gain_and_pan(Vec2::ZERO, Vec2::new(320.0, 0.0));
    /// assert!(gain > 0.3 && gain < 0.4);
    /// assert_eq!(pan, 0.8);
    /// assert_eq!(spatial.gain_and_pan(Vec2::ZERO, Vec2::new(-700.0, 0.0)).0, 0.0);
    /// ```
    pub fn gain_and_pan(&self, listener: Vec2, source: Vec2) -> (f32, f32) {
        let offset = source - listener;
        let distance = offset.length();
        let gain = if distance <= self.near {
            1.0
        } else if distance >= self.far || self.far <= self.near {
            0.0
        } else {
            let t = (distance - self.near) / (self.far - self.near);
            (1.0 - t) * (1.0 - t)
        };
        let pan = if self.pan_width > 0.0 {
            (offset.x / self.pan_width).clamp(-1.0, 1.0) * self.max_pan.clamp(0.0, 1.0)
        } else {
            0.0
        };
        (gain, pan)
    }

    /// Whether a sound at `source` is beyond hearing from `listener`.
    pub fn out_of_range(&self, listener: Vec2, source: Vec2) -> bool {
        listener.distance(source) >= self.far
    }
}

/// Marks the entity whose position positioned sounds are heard from, usually the camera.
///
/// Without one, sounds are heard from the active [`Camera2D`](cg_render::Camera2D)'s
/// transform (its position before shake), and from the origin if there is no camera
/// either. With several, the one with the lowest entity index is used.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AudioListener;

/// A sound that plays from an entity for as long as the entity has this component: a
/// waterfall, a campfire, a machine.
///
/// The sound starts once it has loaded (and, on the web, once the user has interacted),
/// loops by default, and follows the entity's [`GlobalTransform2D`](cg_world::GlobalTransform2D).
/// It stops, with a short fade, when the component is removed or the entity despawned.
/// Replacing the component restarts it with the new sound and settings.
///
/// ```
/// use cg_audio::{AudioEmitter, Bus};
/// # use cg_assets::Handle;
/// # let fire: Handle<cg_audio::Sound> = Handle::default();
/// let emitter = AudioEmitter::new(fire).with_volume(0.6);
/// assert!(emitter.settings.looped);
/// assert_eq!(emitter.settings.bus, Bus::Sfx);
/// ```
#[derive(Component, Debug, Clone)]
pub struct AudioEmitter {
    /// The sound to play.
    pub sound: Handle<Sound>,
    /// How to play it. The position comes from the entity.
    pub settings: PlaySettings,
}

impl AudioEmitter {
    /// An emitter looping `sound` on the effects bus.
    pub fn new(sound: Handle<Sound>) -> Self {
        Self {
            sound,
            settings: PlaySettings {
                looped: true,
                ..PlaySettings::default()
            },
        }
    }

    /// Sets the volume (an amplitude factor, 1 as recorded).
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.settings.volume = volume;
        self
    }

    /// Sets the distance falloff and panning.
    pub fn with_spatial(mut self, spatial: Spatial) -> Self {
        self.settings.spatial = Some(spatial);
        self
    }
}
