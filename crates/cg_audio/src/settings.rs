//! Volume and mute settings, kept between runs through [`Storage`].

use crate::audio::Bus;
use bevy_ecs::prelude::*;
use cg_platform::storage::Storage;
use serde::{Deserialize, Serialize};

/// The player's volume settings: a level from 0 to 1 for the master and each bus, and a
/// mute switch.
///
/// Change it like any resource; the mixer follows within a frame. When
/// [`AudioPlugin::settings_key`](crate::AudioPlugin::settings_key) is set (the default), it
/// is loaded from [`Storage`] at start-up and saved there whenever it changes, so the
/// player's choice survives a restart.
///
/// Levels are loudness, not amplitude: the amplitude is the level squared, so 0.5 sounds
/// about half as loud (-12 dB) and the steps of a slider feel even.
#[derive(Resource, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// Applies to everything.
    pub master: f32,
    /// The music bus.
    pub music: f32,
    /// The effects bus.
    pub sfx: f32,
    /// The interface bus.
    pub ui: f32,
    /// Silences everything without losing the levels.
    pub muted: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            master: 1.0,
            music: 1.0,
            sfx: 1.0,
            ui: 1.0,
            muted: false,
        }
    }
}

impl AudioSettings {
    /// The level of `bus`.
    pub fn level(&self, bus: Bus) -> f32 {
        match bus {
            Bus::Music => self.music,
            Bus::Sfx => self.sfx,
            Bus::Ui => self.ui,
        }
    }

    /// Sets the level of `bus`, clamped to 0..=1.
    pub fn set_level(&mut self, bus: Bus, level: f32) {
        let level = level.clamp(0.0, 1.0);
        match bus {
            Bus::Music => self.music = level,
            Bus::Sfx => self.sfx = level,
            Bus::Ui => self.ui = level,
        }
    }

    /// Moves the level of `bus` by `step` (say 0.1 or -0.1), clamped to 0..=1 and rounded
    /// to two decimals so repeated steps land on round numbers.
    pub fn step_level(&mut self, bus: Bus, step: f32) {
        let level = ((self.level(bus) + step) * 100.0).round() / 100.0;
        self.set_level(bus, level);
    }

    /// Moves the master level by `step`, as [`step_level`](Self::step_level) does.
    pub fn step_master(&mut self, step: f32) {
        self.master = (((self.master + step) * 100.0).round() / 100.0).clamp(0.0, 1.0);
    }

    /// The amplitude the master track plays at: 0 when muted.
    pub fn master_amplitude(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            level_to_amplitude(self.master)
        }
    }

    /// The amplitude `bus` plays at, before the master.
    pub fn bus_amplitude(&self, bus: Bus) -> f32 {
        level_to_amplitude(self.level(bus))
    }

    /// Reads settings saved under `key`, or `None` if there are none or they cannot be
    /// read (which is logged).
    pub fn load(storage: &Storage, key: &str) -> Option<Self> {
        match storage.read(key) {
            Ok(Some(text)) => match ron::from_str::<Self>(&text) {
                Ok(settings) => Some(settings.clamped()),
                Err(err) => {
                    log::warn!("ignoring audio settings in `{key}`: {err}");
                    None
                }
            },
            Ok(None) => None,
            Err(err) => {
                log::warn!("cannot read audio settings: {err}");
                None
            }
        }
    }

    /// Saves the settings under `key`.
    pub fn save(&self, storage: &Storage, key: &str) -> Result<(), String> {
        let text = ron::to_string(self).map_err(|err| err.to_string())?;
        storage.write(key, &text).map_err(|err| err.to_string())
    }

    fn clamped(mut self) -> Self {
        for level in [
            &mut self.master,
            &mut self.music,
            &mut self.sfx,
            &mut self.ui,
        ] {
            *level = if level.is_finite() {
                level.clamp(0.0, 1.0)
            } else {
                1.0
            };
        }
        self
    }
}

fn level_to_amplitude(level: f32) -> f32 {
    let level = level.clamp(0.0, 1.0);
    level * level
}

/// Where [`save_settings`] writes, set by the plugin.
#[derive(Resource, Debug, Clone)]
pub(crate) struct SettingsKey(pub String);

/// Saves [`AudioSettings`] when the game changed them this frame.
pub(crate) fn save_settings(
    settings: Res<AudioSettings>,
    key: Option<Res<SettingsKey>>,
    storage: Option<Res<Storage>>,
) {
    if !settings.is_changed() || settings.is_added() {
        return;
    }
    let (Some(key), Some(storage)) = (key, storage) else {
        return;
    };
    match settings.save(&storage, &key.0) {
        Ok(()) => log::debug!("saved audio settings to {}", storage.location()),
        Err(err) => log::warn!("cannot save audio settings: {err}"),
    }
}
