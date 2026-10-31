//! The mixer (kira's manager and the bus tracks) and the system that applies the game's
//! requests to it every frame.

use crate::audio::{
    Audio, AudioStats, AudioStatus, Bus, Command, MusicSettings, PlaySettings, Voice,
};
use crate::settings::AudioSettings;
use crate::sound::Sound;
use crate::spatial::{AudioEmitter, AudioListener, Spatial};
use crate::voice::{Feeder, Request, VoiceData, VoiceShared, VoiceSound};
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, AssetServer, Assets, Handle, LoadState};
use cg_core::Time;
use cg_core::glam::Vec2;
use cg_platform::Window;
use cg_render::Camera2D;
use cg_world::GlobalTransform2D;
use core::time::Duration;
use kira::backend::cpal::CpalBackend;
use kira::backend::mock::{MockBackend, MockBackendSettings};
use kira::track::{TrackBuilder, TrackHandle};
use kira::{AudioManager, AudioManagerSettings, Decibels, Tween};
use std::collections::HashMap;
use std::sync::Arc;

/// Where the sound goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AudioOutput {
    /// The system's default output device (Web Audio in the browser).
    #[default]
    Device,
    /// A silent device that only advances when told to, for tests.
    Mock,
    /// No sound at all (`--no-audio`). Calls to [`Audio`] still work and do nothing.
    Off,
}

/// Seconds a one-shot sound may wait for its file to load before it is skipped.
const ONE_SHOT_WAIT: f64 = 0.5;
/// Seconds over which positions, settings and pauses for a hidden window take effect, so
/// changes never click.
const SMOOTH: f32 = 0.05;
/// Fade for voices cut short by the caps or removed emitters.
const STEAL_FADE: f32 = 0.02;
const EMITTER_FADE: f32 = 0.1;
/// Ids of voices the engine starts itself (music, emitters) have the top bit set, so they
/// never meet the ids [`Audio::play`] hands out.
const INTERNAL_VOICE: u64 = 1 << 63;
/// Output rate of the mock device.
pub(crate) const MOCK_SAMPLE_RATE: u32 = 48_000;

/// The engine's limits and defaults, from [`AudioPlugin`](crate::AudioPlugin).
#[derive(Resource, Debug, Clone)]
pub(crate) struct AudioConfig {
    pub max_voices: usize,
    pub max_instances: usize,
    pub spatial: Spatial,
    pub pause_when_hidden: bool,
}

/// kira's manager. Holding it keeps the device open; only tests call into it after
/// start-up.
enum Manager {
    Device(#[allow(dead_code, reason = "kept alive, not read")] Box<AudioManager<CpalBackend>>),
    Mock(
        #[cfg_attr(not(test), allow(dead_code, reason = "kept alive, not read"))]
        Box<AudioManager<MockBackend>>,
    ),
}

/// kira's manager and the tracks: master, and under it one per [`Bus`].
struct Mixer {
    #[cfg_attr(not(test), allow(dead_code, reason = "kept alive, not read"))]
    manager: Manager,
    master: TrackHandle,
    buses: [TrackHandle; 3],
}

impl Mixer {
    fn open(output: AudioOutput) -> Result<Self, String> {
        match output {
            AudioOutput::Device => {
                let mut manager = AudioManager::<CpalBackend>::new(AudioManagerSettings::default())
                    .map_err(|err| err.to_string())?;
                let (master, buses) = add_tracks(&mut manager)?;
                Ok(Self {
                    manager: Manager::Device(Box::new(manager)),
                    master,
                    buses,
                })
            }
            AudioOutput::Mock => {
                let mut manager = AudioManager::<MockBackend>::new(AudioManagerSettings {
                    backend_settings: MockBackendSettings {
                        sample_rate: MOCK_SAMPLE_RATE,
                    },
                    ..AudioManagerSettings::default()
                })
                .map_err(|()| "the mock backend failed".to_owned())?;
                let (master, buses) = add_tracks(&mut manager)?;
                Ok(Self {
                    manager: Manager::Mock(Box::new(manager)),
                    master,
                    buses,
                })
            }
            AudioOutput::Off => Err("audio output is off".to_owned()),
        }
    }

    fn bus(&mut self, bus: Bus) -> &mut TrackHandle {
        &mut self.buses[bus.index()]
    }
}

fn add_tracks<B: kira::backend::Backend>(
    manager: &mut AudioManager<B>,
) -> Result<(TrackHandle, [TrackHandle; 3]), String> {
    let error = |err: kira::ResourceLimitReached| err.to_string();
    let mut master = manager
        .add_sub_track(TrackBuilder::new().sound_capacity(8).sub_track_capacity(3))
        .map_err(error)?;
    // Room beyond the voice caps for voices still fading out.
    let music = master
        .add_sub_track(TrackBuilder::new().sound_capacity(16))
        .map_err(error)?;
    let sfx = master
        .add_sub_track(TrackBuilder::new().sound_capacity(256))
        .map_err(error)?;
    let ui = master
        .add_sub_track(TrackBuilder::new().sound_capacity(64))
        .map_err(error)?;
    Ok((master, [music, sfx, ui]))
}

fn tween(secs: f32) -> Tween {
    Tween {
        duration: Duration::from_secs_f32(secs.max(0.0)),
        ..Tween::default()
    }
}

fn decibels(amplitude: f32) -> Decibels {
    if amplitude <= 0.001 {
        Decibels::SILENCE
    } else {
        Decibels((20.0 * amplitude.log10()).max(Decibels::SILENCE.0))
    }
}

/// A voice handed to kira.
struct Playing {
    voice: Voice,
    sound: AssetId<Sound>,
    bus: Bus,
    looped: bool,
    shared: Arc<VoiceShared>,
    feeder: Option<Feeder>,
    volume: f32,
    speed: f32,
    position: Option<Vec2>,
    follow: Option<Entity>,
    spatial: Spatial,
    /// Stopped, stolen or replaced: fading out and no longer counted against the caps.
    ending: bool,
}

/// A voice as tests see it.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VoiceInfo {
    pub voice: Voice,
    pub bus: Bus,
    pub volume: f32,
    pub panning: f32,
    pub speed: f32,
    pub ending: bool,
    pub streamed: bool,
}

/// A play waiting for its sound to load or for the device to open.
struct Pending {
    voice: Voice,
    sound: Handle<Sound>,
    settings: PlaySettings,
    since: f64,
}

/// The mixer and every voice. A non-send resource: kira's web backend must stay on the
/// browser's thread.
#[derive(Default)]
pub(crate) struct AudioDevice {
    output: AudioOutput,
    mixer: Option<Mixer>,
    /// The device could not be opened, or the output is off.
    off: bool,
    /// The master track is paused because the app is hidden or suspended.
    hidden: bool,
    settings_applied: bool,
    playing: Vec<Playing>,
    pending: Vec<Pending>,
    music: Option<(Voice, AssetId<Sound>)>,
    pending_music: Option<(Handle<Sound>, MusicSettings)>,
    emitters: HashMap<Entity, Voice>,
    /// Underruns of voices that have finished.
    past_underruns: u32,
    stolen: u32,
    internal_voices: u64,
}

impl AudioDevice {
    pub fn new(output: AudioOutput) -> Self {
        Self {
            output,
            off: output == AudioOutput::Off,
            ..Self::default()
        }
    }

    fn status(&self) -> AudioStatus {
        match (&self.mixer, self.off, self.hidden) {
            (_, true, _) => AudioStatus::Off,
            (None, false, _) => AudioStatus::Waiting,
            (Some(_), false, true) => AudioStatus::Paused,
            (Some(_), false, false) => AudioStatus::Running,
        }
    }

    fn open(&mut self) {
        match Mixer::open(self.output) {
            Ok(mixer) => {
                log::info!("audio started ({:?})", self.output);
                self.mixer = Some(mixer);
                self.settings_applied = false;
                self.hidden = false;
            }
            Err(err) => {
                log::warn!("no audio device ({err}); the game runs without sound");
                self.off = true;
            }
        }
    }

    /// Pauses everything while the app is suspended or (with `pause_when_hidden`) the
    /// window hidden, and resumes after.
    pub fn sync_hidden(&mut self, window: Option<&Window>, config: &AudioConfig) {
        let hide = window.is_some_and(|window| {
            window.suspended() || (config.pause_when_hidden && window.occluded())
        });
        let Some(mixer) = &mut self.mixer else {
            return;
        };
        if hide != self.hidden {
            self.hidden = hide;
            if hide {
                log::debug!("audio paused: app hidden");
                mixer.master.pause(tween(SMOOTH));
            } else {
                log::debug!("audio resumed");
                mixer.master.resume(tween(SMOOTH));
            }
        }
    }

    fn apply_settings(&mut self, settings: &AudioSettings) {
        let Some(mixer) = &mut self.mixer else {
            return;
        };
        mixer
            .master
            .set_volume(decibels(settings.master_amplitude()), tween(SMOOTH));
        for bus in Bus::ALL {
            mixer
                .bus(bus)
                .set_volume(decibels(settings.bus_amplitude(bus)), tween(SMOOTH));
        }
        self.settings_applied = true;
    }

    fn next_internal_voice(&mut self) -> Voice {
        self.internal_voices += 1;
        Voice(INTERNAL_VOICE | self.internal_voices)
    }

    fn find(&mut self, voice: Voice) -> Option<&mut Playing> {
        self.playing.iter_mut().find(|p| p.voice == voice)
    }

    fn end(&mut self, index: usize, fade: f32) {
        let playing = &mut self.playing[index];
        playing.ending = true;
        playing.shared.request(Request::Stop, fade);
    }

    /// Makes room for one more voice of `sound` on `bus`, cutting the oldest copies short.
    /// Returns `false` if there is no room.
    fn make_room(
        &mut self,
        sound: AssetId<Sound>,
        bus: Bus,
        max_instances: usize,
        config: &AudioConfig,
    ) -> bool {
        if bus == Bus::Music {
            return true;
        }
        let counted = |p: &Playing| !p.ending && p.bus != Bus::Music;
        let copy = |p: &Playing| counted(p) && p.sound == sound;
        while self.playing.iter().filter(|p| copy(p)).count() >= max_instances {
            if !self.cut_oldest(|p| copy(p)) {
                return false;
            }
        }
        while self.playing.iter().filter(|p| counted(p)).count() >= config.max_voices {
            if !self.cut_oldest(counted) {
                return false;
            }
        }
        true
    }

    /// Cuts short the oldest voice that matches and does not loop. Returns `false` if
    /// there is none. Either way it counts as a sound lost to the caps.
    fn cut_oldest(&mut self, matches: impl Fn(&Playing) -> bool) -> bool {
        self.stolen += 1;
        match self.playing.iter().position(|p| matches(p) && !p.looped) {
            Some(oldest) => {
                self.end(oldest, STEAL_FADE);
                true
            }
            None => false,
        }
    }

    /// Starts a voice now. Returns `false` if it was skipped.
    fn start(
        &mut self,
        voice: Voice,
        id: AssetId<Sound>,
        sound: &Sound,
        settings: &PlaySettings,
        ctx: &Ctx,
    ) -> bool {
        if self.mixer.is_none() {
            return false;
        }
        let spatial = settings.spatial.unwrap_or(ctx.config.spatial);
        let position = settings
            .follow
            .and_then(|entity| ctx.position_of(entity))
            .or(settings.position);
        let (gain, panning) =
            position.map_or((1.0, 0.0), |at| spatial.gain_and_pan(ctx.listener, at));
        if !settings.looped && position.is_some_and(|at| spatial.out_of_range(ctx.listener, at)) {
            return false;
        }
        let max_instances = settings
            .max_instances
            .map_or(ctx.config.max_instances, |max| max as usize)
            .max(1);
        if !self.make_room(id, settings.bus, max_instances, ctx.config) {
            log::debug!("voice caps reached; skipping a sound");
            return false;
        }
        let speed = settings.speed.max(0.0) * jitter(voice, settings.pitch_jitter);
        let shared = Arc::new(VoiceShared::new(
            settings.volume.max(0.0) * gain,
            panning,
            speed,
            settings.fade_in,
        ));
        let (feeder, voice_sound) = match &sound.decoded {
            Some(frames) => (
                None,
                VoiceSound::decoded(
                    frames.clone(),
                    sound.sample_rate,
                    settings.looped,
                    shared.clone(),
                ),
            ),
            None => match Feeder::start(sound, settings.looped, shared.clone()) {
                Ok((feeder, voice_sound)) => (Some(feeder), voice_sound),
                Err(err) => {
                    log::warn!("cannot stream a sound: {err}");
                    return false;
                }
            },
        };
        let mixer = self.mixer.as_mut().expect("checked above");
        if let Err(err) = mixer.bus(settings.bus).play(VoiceData(voice_sound)) {
            log::warn!("cannot play a sound: {err:?}");
            return false;
        }
        self.playing.push(Playing {
            voice,
            sound: id,
            bus: settings.bus,
            looped: settings.looped,
            shared,
            feeder,
            volume: settings.volume.max(0.0),
            speed,
            position,
            follow: settings.follow,
            spatial,
            ending: false,
        });
        true
    }

    /// Starts `sound` now if it has loaded, queues it if it is loading, skips it otherwise.
    fn play(
        &mut self,
        voice: Voice,
        sound: Handle<Sound>,
        settings: PlaySettings,
        ctx: &Ctx,
        live: &mut std::collections::HashSet<Voice>,
    ) {
        if self.off || (self.mixer.is_none() && !settings.looped) {
            live.remove(&voice);
            return;
        }
        match ctx.sounds.get(sound.id()) {
            Some(loaded) if self.mixer.is_some() => {
                if !self.start(voice, sound.id(), loaded, &settings, ctx) {
                    live.remove(&voice);
                }
            }
            _ if ctx.can_wait(&sound) || self.mixer.is_none() => self.pending.push(Pending {
                voice,
                sound,
                settings,
                since: ctx.now,
            }),
            _ => {
                log::warn!(
                    "cannot play a sound that is not loaded ({:?})",
                    ctx.load_state(&sound)
                );
                live.remove(&voice);
            }
        }
    }

    fn apply(&mut self, command: Command, ctx: &Ctx, live: &mut std::collections::HashSet<Voice>) {
        match command {
            Command::Play {
                voice,
                sound,
                settings,
            } => self.play(voice, sound, settings, ctx, live),
            Command::Stop { voice, fade } => {
                if let Some(index) = self.playing.iter().position(|p| p.voice == voice) {
                    self.end(index, fade);
                }
                if let Some(index) = self.pending.iter().position(|p| p.voice == voice) {
                    self.pending.remove(index);
                    live.remove(&voice);
                }
            }
            Command::Pause { voice, fade } => {
                if let Some(playing) = self.find(voice) {
                    playing.shared.request(Request::Pause, fade);
                }
            }
            Command::Resume { voice, fade } => {
                if let Some(playing) = self.find(voice)
                    && !playing.ending
                {
                    playing.shared.request(Request::Play, fade);
                }
            }
            Command::SetVolume {
                voice,
                volume,
                ramp,
            } => {
                if let Some(playing) = self.find(voice) {
                    playing.volume = volume.max(0.0);
                    update_params(playing, ctx.listener, ramp);
                }
                if let Some(pending) = self.pending.iter_mut().find(|p| p.voice == voice) {
                    pending.settings.volume = volume;
                }
            }
            Command::SetSpeed { voice, speed, ramp } => {
                if let Some(playing) = self.find(voice) {
                    playing.speed = speed.max(0.0);
                    update_params(playing, ctx.listener, ramp);
                }
                if let Some(pending) = self.pending.iter_mut().find(|p| p.voice == voice) {
                    pending.settings.speed = speed;
                }
            }
            Command::SetPosition { voice, position } => {
                if let Some(playing) = self.find(voice) {
                    playing.position = Some(position);
                    playing.follow = None;
                }
                if let Some(pending) = self.pending.iter_mut().find(|p| p.voice == voice) {
                    pending.settings.position = Some(position);
                    pending.settings.follow = None;
                }
            }
            Command::StopBus { bus, fade } => {
                for index in 0..self.playing.len() {
                    if self.playing[index].bus == bus {
                        self.end(index, fade);
                    }
                }
                self.pending.retain(|p| {
                    let keep = p.settings.bus != bus;
                    if !keep {
                        live.remove(&p.voice);
                    }
                    keep
                });
                if bus == Bus::Music {
                    self.music = None;
                    self.pending_music = None;
                }
            }
            Command::PlayMusic { sound, settings } => {
                let current = self
                    .pending_music
                    .as_ref()
                    .map(|(handle, _)| handle.id())
                    .or(self.music.map(|(_, id)| id));
                if current == Some(sound.id()) && !settings.restart {
                    // Already on (or coming): only follow the new volume.
                    if let Some((voice, _)) = self.music
                        && let Some(playing) = self.find(voice)
                    {
                        playing.volume = settings.volume;
                        update_params(playing, ctx.listener, settings.crossfade);
                    }
                    return;
                }
                self.pending_music = Some((sound, settings));
            }
            Command::StopMusic { fade } => {
                self.pending_music = None;
                if let Some((voice, _)) = self.music.take()
                    && let Some(index) = self.playing.iter().position(|p| p.voice == voice)
                {
                    self.end(index, fade);
                }
            }
        }
    }

    /// Starts the music asked for once it has loaded, fading out what was playing.
    fn start_pending_music(&mut self, ctx: &Ctx) {
        if self.mixer.is_none() {
            return;
        }
        let Some((handle, settings)) = &self.pending_music else {
            return;
        };
        let Some(sound) = ctx.sounds.get(handle.id()) else {
            if !ctx.can_wait(handle) {
                log::warn!(
                    "cannot play music that is not loaded ({:?})",
                    ctx.load_state(handle)
                );
                self.pending_music = None;
            }
            return;
        };
        let (handle, settings) = (handle.clone(), *settings);
        self.pending_music = None;
        if let Some((old, _)) = self.music.take()
            && let Some(index) = self.playing.iter().position(|p| p.voice == old)
        {
            self.end(index, settings.crossfade);
        }
        let voice = self.next_internal_voice();
        let play = PlaySettings {
            bus: Bus::Music,
            volume: settings.volume,
            looped: settings.looped,
            fade_in: settings.crossfade,
            ..PlaySettings::default()
        };
        if self.start(voice, handle.id(), sound, &play, ctx) {
            self.music = Some((voice, handle.id()));
        }
    }

    /// Stops the voices of emitters that went away, and starts new or changed ones.
    fn sync_emitters(
        &mut self,
        ctx: &Ctx,
        emitters: &Query<(Entity, Ref<AudioEmitter>)>,
        removed: &mut RemovedComponents<AudioEmitter>,
    ) {
        for entity in removed.read() {
            if emitters.contains(entity) {
                continue;
            }
            if let Some(voice) = self.emitters.remove(&entity)
                && let Some(index) = self.playing.iter().position(|p| p.voice == voice)
            {
                self.end(index, EMITTER_FADE);
            }
        }
        if self.mixer.is_none() {
            return;
        }
        for (entity, emitter) in emitters {
            let known = self.emitters.get(&entity).copied();
            if known.is_some() && !emitter.is_changed() {
                continue;
            }
            let Some(sound) = ctx.sounds.get(emitter.sound.id()) else {
                continue;
            };
            if let Some(old) = known
                && let Some(index) = self.playing.iter().position(|p| p.voice == old)
            {
                self.end(index, EMITTER_FADE);
            }
            let settings = PlaySettings {
                follow: Some(entity),
                ..emitter.settings.clone()
            };
            let voice = self.next_internal_voice();
            // A voice skipped by the caps is not retried until the emitter changes.
            self.start(voice, emitter.sound.id(), sound, &settings, ctx);
            self.emitters.insert(entity, voice);
        }
    }

    /// What every voice is doing, for tests.
    #[cfg(test)]
    pub fn voices(&self) -> Vec<VoiceInfo> {
        self.playing
            .iter()
            .map(|p| {
                let (volume, panning, speed) = p.shared.targets();
                VoiceInfo {
                    voice: p.voice,
                    bus: p.bus,
                    volume,
                    panning,
                    speed,
                    ending: p.ending,
                    streamed: p.feeder.is_some(),
                }
            })
            .collect()
    }

    /// Mock output: renders `frames` frames (rounded up to whole blocks).
    #[cfg(test)]
    pub fn process_mock(&mut self, frames: usize) {
        if let Some(Mixer {
            manager: Manager::Mock(manager),
            ..
        }) = &mut self.mixer
        {
            let backend = manager.backend_mut();
            for _ in 0..frames.div_ceil(128) {
                backend.on_start_processing();
                backend.process();
            }
        }
    }
}

/// A random factor for a play's speed, up to `semitones` either way. Taken from the voice
/// id, so runs are repeatable.
pub(crate) fn jitter(voice: Voice, semitones: f32) -> f32 {
    if semitones <= 0.0 {
        return 1.0;
    }
    // SplitMix64.
    let mut z = voice.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    let unit = (z >> 40) as f32 / (1u64 << 24) as f32; // 0..1
    2f32.powf((unit * 2.0 - 1.0) * semitones / 12.0)
}

/// Sends a voice's volume, panning and speed (from its position) to the callback.
fn update_params(playing: &Playing, listener: Vec2, ramp: f32) {
    let (gain, panning) = playing
        .position
        .map_or((1.0, 0.0), |at| playing.spatial.gain_and_pan(listener, at));
    playing
        .shared
        .set_params(playing.volume * gain, panning, playing.speed, ramp);
}

/// What the device needs from the world for one frame.
struct Ctx<'a, 'w, 's> {
    sounds: &'a Assets<Sound>,
    server: Option<&'a AssetServer>,
    transforms: &'a Query<'w, 's, &'static GlobalTransform2D>,
    listener: Vec2,
    config: &'a AudioConfig,
    now: f64,
}

impl Ctx<'_, '_, '_> {
    fn position_of(&self, entity: Entity) -> Option<Vec2> {
        self.transforms
            .get(entity)
            .ok()
            .map(GlobalTransform2D::translation)
    }

    fn load_state(&self, sound: &Handle<Sound>) -> LoadState {
        self.server
            .map_or(LoadState::NotLoaded, |server| server.load_state(sound))
    }

    /// Whether a sound not loaded yet is on its way.
    fn can_wait(&self, sound: &Handle<Sound>) -> bool {
        self.load_state(sound) == LoadState::Loading
    }
}

/// Where sounds are heard from: the first [`AudioListener`], else the active camera.
fn listener_position(
    listeners: &Query<(Entity, &GlobalTransform2D), With<AudioListener>>,
    cameras: &Query<(Entity, &Camera2D, &GlobalTransform2D)>,
) -> Vec2 {
    let lowest = |a: &Entity, b: &Entity| a.index_u32().cmp(&b.index_u32());
    if let Some((_, transform)) = listeners.iter().min_by(|a, b| lowest(&a.0, &b.0)) {
        return transform.translation();
    }
    cameras
        .iter()
        .filter(|(_, camera, _)| camera.active)
        .min_by(|a, b| lowest(&a.0, &b.0))
        .map_or(Vec2::ZERO, |(_, _, transform)| transform.translation())
}

/// Opens the device once allowed, then applies the frame's requests, moves positioned
/// voices, feeds streams and clears out finished voices.
#[allow(clippy::too_many_arguments)]
#[profiling::function]
pub(crate) fn update_audio(
    mut device: NonSendMut<AudioDevice>,
    mut audio: ResMut<Audio>,
    config: Res<AudioConfig>,
    settings: Res<AudioSettings>,
    sounds: Res<Assets<Sound>>,
    server: Option<Res<AssetServer>>,
    window: Option<Res<Window>>,
    time: Res<Time>,
    listeners: Query<(Entity, &GlobalTransform2D), With<AudioListener>>,
    cameras: Query<(Entity, &Camera2D, &GlobalTransform2D)>,
    transforms: Query<&'static GlobalTransform2D>,
    emitters: Query<(Entity, Ref<AudioEmitter>)>,
    mut removed: RemovedComponents<AudioEmitter>,
) {
    let device = &mut *device;
    let audio = &mut *audio;
    // The device opens when the game first asks for sound, so games and examples
    // without any never open one.
    let wanted = !audio.commands.is_empty()
        || !device.pending.is_empty()
        || device.pending_music.is_some()
        || !emitters.is_empty();
    if wanted
        && device.mixer.is_none()
        && !device.off
        && window.as_ref().is_none_or(|window| window.user_activated())
    {
        device.open();
    }
    device.sync_hidden(window.as_deref(), &config);
    if settings.is_changed() || !device.settings_applied {
        device.apply_settings(&settings);
    }

    let ctx = Ctx {
        sounds: &sounds,
        server: server.as_deref(),
        transforms: &transforms,
        listener: listener_position(&listeners, &cameras),
        config: &config,
        now: time.raw_elapsed().as_secs_f64(),
    };
    let live = &mut audio.live;
    for command in std::mem::take(&mut audio.commands) {
        device.apply(command, &ctx, live);
    }

    // Plays that were waiting for their sound or for the device.
    if device.mixer.is_some() {
        for pending in std::mem::take(&mut device.pending) {
            if let Some(sound) = ctx.sounds.get(pending.sound.id()) {
                let expired = !pending.settings.looped && ctx.now - pending.since > ONE_SHOT_WAIT;
                if expired
                    || !device.start(
                        pending.voice,
                        pending.sound.id(),
                        sound,
                        &pending.settings,
                        &ctx,
                    )
                {
                    live.remove(&pending.voice);
                }
            } else if ctx.can_wait(&pending.sound) {
                device.pending.push(pending);
            } else {
                live.remove(&pending.voice);
            }
        }
    } else if device.off {
        for pending in device.pending.drain(..) {
            live.remove(&pending.voice);
        }
    }

    device.start_pending_music(&ctx);
    device.sync_emitters(&ctx, &emitters, &mut removed);

    for playing in &mut device.playing {
        if let Some(entity) = playing.follow
            && let Some(at) = ctx.position_of(entity)
        {
            playing.position = Some(at);
        }
        if playing.position.is_some() && !playing.ending {
            update_params(playing, ctx.listener, SMOOTH);
        }
        if let Some(feeder) = &mut playing.feeder
            && let Err(err) = feeder.feed()
        {
            log::warn!("a streamed sound failed: {err}");
            playing.ending = true;
            playing.shared.request(Request::Stop, 0.0);
        }
    }

    // Finished voices.
    let mut finished_music = false;
    let music = device.music;
    let past_underruns = &mut device.past_underruns;
    device.playing.retain(|playing| {
        let done = playing.shared.finished();
        if done {
            *past_underruns += playing.shared.underruns();
            live.remove(&playing.voice);
            finished_music |= music.is_some_and(|(voice, _)| voice == playing.voice);
        }
        !done
    });
    if finished_music {
        device.music = None;
        if device.pending_music.is_none() {
            audio.music = None;
        }
    }
    audio.status = device.status();
    audio.stats = AudioStats {
        voices: device.playing.iter().filter(|p| !p.ending).count(),
        fading: device.playing.iter().filter(|p| p.ending).count(),
        streams: device.playing.iter().filter(|p| p.feeder.is_some()).count(),
        underruns: device.past_underruns
            + device
                .playing
                .iter()
                .map(|p| p.shared.underruns())
                .sum::<u32>(),
        stolen: device.stolen,
    };
}

/// Pauses or resumes the mixer the moment the app is hidden, shown, suspended or resumed,
/// in [`LifecycleChanged`](cg_platform::LifecycleChanged): no frame may follow.
pub(crate) fn sync_hidden(
    mut device: NonSendMut<AudioDevice>,
    config: Res<AudioConfig>,
    window: Option<Res<Window>>,
) {
    device.sync_hidden(window.as_deref(), &config);
}
