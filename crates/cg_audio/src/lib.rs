//! Audio for CashGear Engine: sound effects and streamed music on mixer buses, simple 2D
//! positional sound, and volume settings that persist, on desktop, web and Android.
//!
//! | Item | Does |
//! | --- | --- |
//! | [`Sound`], [`SoundLoader`] | OGG Vorbis and WAV files as assets; long ones are streamed |
//! | [`Audio`] | Plays, stops and changes sounds ([`Voice`]s) and music with crossfades |
//! | [`PlaySettings`], [`MusicSettings`] | Bus, volume, speed, pitch jitter, looping, fade-in, position |
//! | [`Bus`], [`AudioSettings`] | Music, effects and interface buses under a master; levels and mute saved through [`Storage`] |
//! | [`AudioListener`], [`AudioEmitter`], [`Spatial`] | Pan and distance falloff from the camera; sounds that follow entities |
//!
//! ```
//! use cg_assets::prelude::*;
//! use cg_audio::prelude::*;
//! use cg_core::prelude::*;
//!
//! #[derive(Resource)]
//! struct Sounds { coin: Handle<Sound>, theme: Handle<Sound> }
//!
//! fn load(mut commands: Commands, server: Res<AssetServer>) {
//!     commands.insert_resource(Sounds {
//!         coin: server.load("sounds/coin.ogg"),
//!         theme: server.load("music/theme.ogg"),
//!     });
//! }
//!
//! fn start(mut audio: ResMut<Audio>, sounds: Res<Sounds>) {
//!     audio.play_music(&sounds.theme, MusicSettings::default());
//! }
//!
//! fn pick_up(mut audio: ResMut<Audio>, sounds: Res<Sounds>) {
//!     let at = vec2(120.0, 40.0);
//!     audio.play_with(&sounds.coin, PlaySettings::new().at(at).with_pitch_jitter(1.0));
//! }
//!
//! App::new()
//!     .add_plugins(AudioPlugin { output: AudioOutput::Off, ..default() })
//!     .add_systems(Startup, load)
//!     .add_systems(Update, (start, pick_up));
//! ```
//!
//! How it works:
//!
//! - **kira** mixes and talks to the device (cpal: WASAPI, CoreAudio, ALSA, AAudio, and
//!   Web Audio in the browser). Each [`Bus`] is a kira track under a master track. The
//!   device opens when the game first asks for a sound; a game without sound never opens
//!   one.
//! - **symphonia** decodes the files. Sounds up to
//!   [`AudioPlugin::stream_longer_than`] seconds are decoded once, at load; longer ones are
//!   decoded while they play, a little every frame on the main thread, into a two-second
//!   buffer. That works the same everywhere, without threads.
//! - **Requests** to [`Audio`] are applied in [`PostUpdate`] after transform propagation, in
//!   [`AudioSystems::Update`], so positions are those of this frame.
//! - **The web** allows sound only after the user has pressed a key, clicked or touched
//!   the page; until then [`Audio::status`] is [`AudioStatus::Waiting`], music and looping
//!   sounds wait, and other sounds are skipped.
//! - **In the background** (Android, a hidden browser tab, and with
//!   [`AudioPlugin::pause_when_hidden`] a minimized window) everything pauses, and resumes
//!   when the app is back.

mod audio;
mod decode;
mod device;
mod settings;
mod sound;
mod spatial;
mod voice;

pub use audio::{Audio, AudioStats, AudioStatus, Bus, MusicSettings, PlaySettings, Voice};
pub use decode::DecodeError;
pub use device::AudioOutput;
pub use settings::AudioSettings;
pub use sound::{Sound, SoundLoader};
pub use spatial::{AudioEmitter, AudioListener, Spatial};

use bevy_ecs::prelude::*;
use cg_assets::AppAssetExt;
use cg_core::{App, Last, Plugin, PostUpdate};
use cg_platform::storage::Storage;
use cg_platform::{Args, LifecycleChanged};
use cg_world::TransformSystems;
use device::{AudioConfig, AudioDevice};
use settings::SettingsKey;

/// The audio systems.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioSystems {
    /// In [`PostUpdate`], after transform propagation: applies the frame's [`Audio`]
    /// requests and settings, moves positioned sounds and feeds streams.
    Update,
}

/// Adds sounds as assets, the [`Audio`] resource and [`AudioSettings`], and the mixer.
///
/// `DefaultPlugins` includes it, with the launch flags applied (see
/// [`with_args`](Self::with_args)).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioPlugin {
    /// Where the sound goes.
    pub output: AudioOutput,
    /// The most sound effects and interface sounds playing at once (music is not counted).
    /// Past it, the oldest one that does not loop is cut short.
    pub max_voices: usize,
    /// The most copies of one sound playing at once, unless its [`PlaySettings`] say
    /// otherwise. Past it, the oldest copy is cut short.
    pub max_instances: usize,
    /// Distance falloff and panning for positioned sounds without their own.
    pub spatial: Spatial,
    /// Sounds longer than this many seconds are streamed instead of decoded at load.
    pub stream_longer_than: f32,
    /// Pause all sound while the window is hidden (minimized, or a background browser
    /// tab). Sound always pauses while the app is suspended (Android).
    pub pause_when_hidden: bool,
    /// The [`Storage`] key [`AudioSettings`] are loaded from and saved to; `None` keeps
    /// them for this run only.
    pub settings_key: Option<String>,
}

impl Default for AudioPlugin {
    fn default() -> Self {
        Self {
            output: AudioOutput::Device,
            max_voices: 32,
            max_instances: 4,
            spatial: Spatial::default(),
            stream_longer_than: 10.0,
            pause_when_hidden: true,
            settings_key: Some("audio-settings.ron".to_owned()),
        }
    }
}

impl AudioPlugin {
    /// Applies the launch flag `--no-audio` (`?no-audio` in the page URL), which turns the
    /// output off.
    pub fn with_args(mut self, args: &Args) -> Self {
        if args.flag("no-audio") {
            self.output = AudioOutput::Off;
        }
        self
    }
}

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        let settings = match (&self.settings_key, app.world().get_resource::<Storage>()) {
            (Some(key), Some(storage)) => AudioSettings::load(storage, key).unwrap_or_default(),
            (Some(_), None) => {
                log::debug!("no Storage resource: audio settings are not kept between runs");
                AudioSettings::default()
            }
            (None, _) => AudioSettings::default(),
        };
        if let Some(key) = &self.settings_key {
            app.insert_resource(SettingsKey(key.clone()));
        }
        app.init_asset::<Sound>()
            .register_loader(SoundLoader {
                stream_after: self.stream_longer_than,
            })
            .init_resource::<Audio>()
            .insert_resource(settings)
            .insert_resource(AudioConfig {
                max_voices: self.max_voices.max(1),
                max_instances: self.max_instances.max(1),
                spatial: self.spatial,
                pause_when_hidden: self.pause_when_hidden,
            })
            .add_systems(
                PostUpdate,
                device::update_audio
                    .in_set(AudioSystems::Update)
                    .after(TransformSystems::Propagate),
            )
            .add_systems(Last, settings::save_settings)
            .add_systems(LifecycleChanged, device::sync_hidden);
        app.world_mut()
            .insert_non_send(AudioDevice::new(self.output));
    }
}

/// Common imports: the plugin, sounds, the [`Audio`] resource and its settings types,
/// buses, volume settings, and positional components.
pub mod prelude {
    pub use crate::{
        Audio, AudioEmitter, AudioListener, AudioOutput, AudioPlugin, AudioSettings, AudioStatus,
        Bus, MusicSettings, PlaySettings, Sound, Spatial, Voice,
    };
}

#[cfg(test)]
mod tests;
