//! The [`Audio`] resource: what game code calls to play, stop and change sounds and music.

use crate::sound::Sound;
use crate::spatial::Spatial;
use bevy_ecs::prelude::*;
use cg_assets::Handle;
use cg_core::glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A mixer bus. Each has its own volume in [`AudioSettings`](crate::AudioSettings), and
/// all of them pass through the master volume.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Bus {
    /// Music tracks, played with [`Audio::play_music`].
    Music,
    /// Sound effects, the default.
    #[default]
    Sfx,
    /// Menu and interface sounds.
    Ui,
}

impl Bus {
    /// Every bus, in a fixed order.
    pub const ALL: [Bus; 3] = [Bus::Music, Bus::Sfx, Bus::Ui];

    pub(crate) fn index(self) -> usize {
        self as usize
    }
}

/// Names a sound started with [`Audio::play`], to stop or change it later. Ids are never
/// reused, so an old one simply finds nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Voice(pub(crate) u64);

/// How to play a sound. Build one with struct-update syntax or the `with_` methods:
///
/// ```
/// use cg_audio::{Bus, PlaySettings};
/// use cg_core::glam::vec2;
///
/// let coin = PlaySettings::new().with_volume(0.7).with_pitch_jitter(1.0).at(vec2(120.0, 48.0));
/// let click = PlaySettings { bus: Bus::Ui, ..Default::default() };
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct PlaySettings {
    /// The bus it plays on. [`Bus::Music`] voices are not counted against the voice caps.
    pub bus: Bus,
    /// An amplitude factor: 1 plays the file as recorded, 0.5 at half amplitude (about 6 dB
    /// quieter).
    pub volume: f32,
    /// Playback speed, which also sets the pitch: 2 is twice as fast and an octave up.
    pub speed: f32,
    /// A random pitch change of up to this many semitones either way, picked for each
    /// play, so repeated sounds (footsteps, coins) do not sound mechanical. 0 turns it off.
    pub pitch_jitter: f32,
    /// Loop until stopped.
    pub looped: bool,
    /// Seconds to fade in from silence. 0 starts at full volume.
    pub fade_in: f32,
    /// Where the sound is in the world. Positioned sounds pan and fade with their distance
    /// from the [`AudioListener`](crate::AudioListener); `None` plays it as is.
    pub position: Option<Vec2>,
    /// An entity whose position the sound follows while it plays. It keeps the last
    /// position if the entity goes away. Overrides `position`.
    pub follow: Option<Entity>,
    /// Distance falloff and panning for a positioned sound; `None` uses
    /// [`AudioPlugin::spatial`](crate::AudioPlugin::spatial).
    pub spatial: Option<Spatial>,
    /// The most copies of this sound that may play at once; `None` uses
    /// [`AudioPlugin::max_instances`](crate::AudioPlugin::max_instances).
    pub max_instances: Option<u32>,
}

impl Default for PlaySettings {
    fn default() -> Self {
        Self {
            bus: Bus::Sfx,
            volume: 1.0,
            speed: 1.0,
            pitch_jitter: 0.0,
            looped: false,
            fade_in: 0.0,
            position: None,
            follow: None,
            spatial: None,
            max_instances: None,
        }
    }
}

impl PlaySettings {
    /// The defaults: a sound effect, as recorded, played once.
    pub fn new() -> Self {
        Self::default()
    }

    /// Plays on `bus`.
    pub fn with_bus(mut self, bus: Bus) -> Self {
        self.bus = bus;
        self
    }

    /// Sets the volume (an amplitude factor).
    pub fn with_volume(mut self, volume: f32) -> Self {
        self.volume = volume;
        self
    }

    /// Sets the playback speed (and so the pitch).
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    /// Varies the pitch randomly by up to `semitones` either way.
    pub fn with_pitch_jitter(mut self, semitones: f32) -> Self {
        self.pitch_jitter = semitones;
        self
    }

    /// Loops until stopped.
    pub fn looped(mut self) -> Self {
        self.looped = true;
        self
    }

    /// Fades in over `secs` seconds.
    pub fn with_fade_in(mut self, secs: f32) -> Self {
        self.fade_in = secs;
        self
    }

    /// Plays from a point in the world.
    pub fn at(mut self, position: Vec2) -> Self {
        self.position = Some(position);
        self
    }

    /// Plays from an entity, following it.
    pub fn following(mut self, entity: Entity) -> Self {
        self.follow = Some(entity);
        self
    }

    /// Sets the distance falloff and panning.
    pub fn with_spatial(mut self, spatial: Spatial) -> Self {
        self.spatial = Some(spatial);
        self
    }

    /// Caps the copies of this sound playing at once.
    pub fn with_max_instances(mut self, max: u32) -> Self {
        self.max_instances = Some(max);
        self
    }
}

/// How to play a music track with [`Audio::play_music`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MusicSettings {
    /// An amplitude factor, as in [`PlaySettings::volume`].
    pub volume: f32,
    /// Seconds over which the new track fades in and the one playing fades out.
    pub crossfade: f32,
    /// Loop the track. On by default.
    pub looped: bool,
    /// Start the track again even if it is already the one playing. Off by default, so
    /// re-entering a level keeps its music going.
    pub restart: bool,
}

impl Default for MusicSettings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            crossfade: 1.0,
            looped: true,
            restart: false,
        }
    }
}

/// Whether sound can be heard yet.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AudioStatus {
    /// Waiting to open the audio device: on the web, until the user presses a key, clicks
    /// or touches the page. Music and looping sounds asked for meanwhile start then; other
    /// sounds are skipped.
    #[default]
    Waiting,
    /// Playing.
    Running,
    /// Paused while the app is in the background or the window hidden.
    Paused,
    /// No sound: the device could not be opened, or the output is off (`--no-audio`).
    /// Calls still work and do nothing.
    Off,
}

/// Counts for a status line or a test.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AudioStats {
    /// Voices playing, music included.
    pub voices: usize,
    /// Voices fading out after a stop, or cut short by the voice caps.
    pub fading: usize,
    /// Of those, the ones being streamed.
    pub streams: usize,
    /// Times a stream ran dry since the app started, because frames came too far apart.
    pub underruns: u32,
    /// Plays refused or cut short by the voice caps since the app started.
    pub stolen: u32,
}

/// What the game asked for this frame, applied in [`PostUpdate`](cg_core::PostUpdate).
#[derive(Debug, Clone)]
pub(crate) enum Command {
    Play {
        voice: Voice,
        sound: Handle<Sound>,
        settings: PlaySettings,
    },
    Stop {
        voice: Voice,
        fade: f32,
    },
    Pause {
        voice: Voice,
        fade: f32,
    },
    Resume {
        voice: Voice,
        fade: f32,
    },
    SetVolume {
        voice: Voice,
        volume: f32,
        ramp: f32,
    },
    SetSpeed {
        voice: Voice,
        speed: f32,
        ramp: f32,
    },
    SetPosition {
        voice: Voice,
        position: Vec2,
    },
    StopBus {
        bus: Bus,
        fade: f32,
    },
    PlayMusic {
        sound: Handle<Sound>,
        settings: MusicSettings,
    },
    StopMusic {
        fade: f32,
    },
}

/// Plays sounds and music. Calls are queued and applied once a frame, in
/// [`PostUpdate`](cg_core::PostUpdate) after transforms are updated.
///
/// ```
/// use cg_assets::Handle;
/// use cg_audio::prelude::*;
/// use cg_core::prelude::*;
///
/// #[derive(Resource)]
/// struct Sounds { jump: Handle<Sound>, theme: Handle<Sound> }
///
/// fn jump(mut audio: ResMut<Audio>, sounds: Res<Sounds>) {
///     audio.play_with(&sounds.jump, PlaySettings::new().with_pitch_jitter(1.0));
/// }
///
/// fn start_level(mut audio: ResMut<Audio>, sounds: Res<Sounds>) {
///     audio.play_music(&sounds.theme, MusicSettings::default());
/// }
/// ```
#[derive(Resource, Debug, Default)]
pub struct Audio {
    pub(crate) commands: Vec<Command>,
    next_voice: u64,
    pub(crate) live: HashSet<Voice>,
    pub(crate) music: Option<Handle<Sound>>,
    pub(crate) status: AudioStatus,
    pub(crate) stats: AudioStats,
}

impl Audio {
    /// Plays `sound` once on the effects bus, as recorded.
    pub fn play(&mut self, sound: &Handle<Sound>) -> Voice {
        self.play_with(sound, PlaySettings::default())
    }

    /// Plays `sound` once from a point in the world.
    pub fn play_at(&mut self, sound: &Handle<Sound>, position: Vec2) -> Voice {
        self.play_with(sound, PlaySettings::default().at(position))
    }

    /// Plays `sound` with `settings`.
    ///
    /// A sound that has not loaded yet starts when it loads (a one-shot only within half a
    /// second; later it is skipped). Past the voice caps, the oldest copy that does not
    /// loop is cut short to make room; if every one loops, the new sound is skipped. A
    /// positioned one-shot out of earshot is skipped too.
    pub fn play_with(&mut self, sound: &Handle<Sound>, settings: PlaySettings) -> Voice {
        self.next_voice += 1;
        let voice = Voice(self.next_voice);
        self.live.insert(voice);
        self.commands.push(Command::Play {
            voice,
            sound: sound.clone(),
            settings,
        });
        voice
    }

    /// Stops a voice, fading out over `fade` seconds.
    pub fn stop(&mut self, voice: Voice, fade: f32) {
        self.commands.push(Command::Stop { voice, fade });
    }

    /// Pauses a voice, fading out over `fade` seconds. It keeps its place.
    pub fn pause(&mut self, voice: Voice, fade: f32) {
        self.commands.push(Command::Pause { voice, fade });
    }

    /// Resumes a paused voice, fading in over `fade` seconds.
    pub fn resume(&mut self, voice: Voice, fade: f32) {
        self.commands.push(Command::Resume { voice, fade });
    }

    /// Changes a voice's volume over `ramp` seconds.
    pub fn set_volume(&mut self, voice: Voice, volume: f32, ramp: f32) {
        self.commands.push(Command::SetVolume {
            voice,
            volume,
            ramp,
        });
    }

    /// Changes a voice's speed (and pitch) over `ramp` seconds.
    pub fn set_speed(&mut self, voice: Voice, speed: f32, ramp: f32) {
        self.commands.push(Command::SetSpeed { voice, speed, ramp });
    }

    /// Moves a positioned voice.
    pub fn set_position(&mut self, voice: Voice, position: Vec2) {
        self.commands.push(Command::SetPosition { voice, position });
    }

    /// Stops every voice on `bus`, music included for [`Bus::Music`], fading out over
    /// `fade` seconds. Emitters stay: they start again only when their component changes.
    pub fn stop_bus(&mut self, bus: Bus, fade: f32) {
        if bus == Bus::Music {
            self.music = None;
        }
        self.commands.push(Command::StopBus { bus, fade });
    }

    /// Whether a voice is playing or about to: from [`play`](Self::play) until it ends, is
    /// stopped, or is skipped.
    pub fn is_playing(&self, voice: Voice) -> bool {
        self.live.contains(&voice)
    }

    /// Plays a music track on the music bus, crossfading from the one playing. Playing the
    /// track that is already on does nothing unless [`MusicSettings::restart`] is set. On
    /// the web, music asked for before the user has interacted starts once they do.
    pub fn play_music(&mut self, sound: &Handle<Sound>, settings: MusicSettings) {
        self.music = Some(sound.clone());
        self.commands.push(Command::PlayMusic {
            sound: sound.clone(),
            settings,
        });
    }

    /// Fades the music out over `fade` seconds.
    pub fn stop_music(&mut self, fade: f32) {
        self.music = None;
        self.commands.push(Command::StopMusic { fade });
    }

    /// The music track playing or about to, if any.
    pub fn music(&self) -> Option<&Handle<Sound>> {
        self.music.as_ref()
    }

    /// Whether sound can be heard yet; see [`AudioStatus`].
    pub fn status(&self) -> AudioStatus {
        self.status
    }

    /// Voice, stream and underrun counts, as of the end of the last frame.
    pub fn stats(&self) -> AudioStats {
        self.stats
    }
}
