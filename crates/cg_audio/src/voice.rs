//! The playing side of a sound: a kira [`Sound`](kira::sound::Sound) that reads either
//! decoded frames or a stream, and the main-thread feeder that keeps a stream topped up.
//!
//! The game controls a voice through [`VoiceShared`], a handful of atomics the audio
//! callback reads once per block. Nothing is queued, so a voice can never fall behind on
//! commands, whatever the frame rate. Streams decode on the main thread, a little every
//! frame, into a lock-free ring buffer the callback reads from; that needs no threads,
//! which the web does not have.

use crate::decode::{Bytes, DecodeError, Decoder};
use crate::sound::Sound;
use kira::info::Info;
use kira::sound::{Sound as KiraSound, SoundData};
use kira::{Frame, Panning, interpolate_frame};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

/// Seconds of decoded audio a stream's ring buffer holds.
pub(crate) const STREAM_BUFFER_SECS: f64 = 2.0;
/// Seconds decoded before a stream starts playing.
pub(crate) const STREAM_PREFILL_SECS: f64 = 0.5;
/// Seconds a stream decodes at most per frame: enough to keep up at 4 frames a second.
pub(crate) const STREAM_FEED_SECS: f64 = 0.25;

/// An `f32` shared between threads.
pub(crate) struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(value: f32) -> Self {
        Self(AtomicU32::new(value.to_bits()))
    }

    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub fn store(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// What the game wants a voice to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Request {
    Play = 0,
    Pause = 1,
    Stop = 2,
}

impl Request {
    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Play,
            1 => Self::Pause,
            _ => Self::Stop,
        }
    }
}

/// The state a voice shares between the game (writes targets) and the audio callback
/// (reads them, and reports back).
pub(crate) struct VoiceShared {
    /// Target amplitude.
    volume: AtomicF32,
    /// Target panning, -1 (left) to 1 (right).
    panning: AtomicF32,
    /// Target playback-rate multiplier.
    speed: AtomicF32,
    /// Seconds to reach new volume, panning and speed targets.
    ramp: AtomicF32,
    /// Seconds for the latest play, pause or stop request.
    fade: AtomicF32,
    request: AtomicU8,
    /// Set by the callback once the voice has stopped or run out.
    finished: AtomicBool,
    /// Set by the feeder after the last frame of a stream that does not loop.
    stream_ended: AtomicBool,
    underruns: AtomicU32,
}

impl VoiceShared {
    pub fn new(volume: f32, panning: f32, speed: f32, fade_in: f32) -> Self {
        Self {
            volume: AtomicF32::new(volume),
            panning: AtomicF32::new(panning),
            speed: AtomicF32::new(speed),
            ramp: AtomicF32::new(0.0),
            fade: AtomicF32::new(fade_in),
            request: AtomicU8::new(Request::Play as u8),
            finished: AtomicBool::new(false),
            stream_ended: AtomicBool::new(false),
            underruns: AtomicU32::new(0),
        }
    }

    /// Sets volume, panning and speed targets, reached over `ramp` seconds.
    pub fn set_params(&self, volume: f32, panning: f32, speed: f32, ramp: f32) {
        self.ramp.store(ramp.max(0.0));
        self.volume.store(volume.max(0.0));
        self.panning.store(panning.clamp(-1.0, 1.0));
        self.speed.store(speed.max(0.0));
    }

    /// Asks the voice to play, pause or stop, fading over `fade` seconds.
    pub fn request(&self, request: Request, fade: f32) {
        self.fade.store(fade.max(0.0));
        self.request.store(request as u8, Ordering::Release);
    }

    pub fn current_request(&self) -> Request {
        Request::from_u8(self.request.load(Ordering::Acquire))
    }

    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    /// The volume, panning and speed targets, for tests.
    #[cfg(test)]
    pub fn targets(&self) -> (f32, f32, f32) {
        (self.volume.load(), self.panning.load(), self.speed.load())
    }

    /// How many times the stream ran dry since it started.
    pub fn underruns(&self) -> u32 {
        self.underruns.load(Ordering::Relaxed)
    }
}

/// A value moving linearly towards a target, one output frame at a time.
#[derive(Debug, Clone, Copy)]
struct Ramp {
    value: f32,
    target: f32,
    step: f32,
}

impl Ramp {
    fn new(value: f32) -> Self {
        Self {
            value,
            target: value,
            step: 0.0,
        }
    }

    fn set(&mut self, target: f32, secs: f32, dt: f64) {
        self.target = target;
        let frames = f64::from(secs) / dt;
        if frames < 1.0 {
            self.value = target;
            self.step = 0.0;
        } else {
            self.step = ((f64::from(target - self.value)) / frames) as f32;
        }
    }

    fn advance(&mut self) {
        if self.value == self.target {
            return;
        }
        self.value += self.step;
        let passed = if self.step > 0.0 {
            self.value >= self.target
        } else {
            self.value <= self.target
        };
        if passed || self.step == 0.0 {
            self.value = self.target;
        }
    }

    fn settled(&self) -> bool {
        self.value == self.target
    }
}

/// Where a voice's frames come from.
enum Source {
    /// A decoded sound, read at a fractional position.
    Decoded {
        frames: Arc<[Frame]>,
        position: f64,
        looped: bool,
    },
    /// A stream: a ring buffer the main thread fills, read through a window of four frames
    /// for interpolation.
    Stream {
        consumer: rtrb::Consumer<Frame>,
        window: [Frame; 4],
        /// Position between `window[1]` and `window[2]`; 1 or more means frames are due.
        fraction: f64,
        /// Silent frames shifted in after the stream ended.
        tail: u8,
        starved: bool,
    },
}

/// The playing state of a voice, as the callback sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Playing,
    Pausing,
    Paused,
    Stopping,
    Stopped,
}

/// A playing sound, owned by kira's audio callback.
pub(crate) struct VoiceSound {
    source: Source,
    source_rate: f64,
    shared: Arc<VoiceShared>,
    volume: Ramp,
    panning: Ramp,
    speed: Ramp,
    /// The pause, stop and fade-in envelope, 0 to 1, applied as an equal-power curve so a
    /// crossfade keeps the loudness even.
    envelope: Ramp,
    state: State,
    seen: (f32, f32, f32, Request),
    started: bool,
}

impl VoiceSound {
    /// A voice reading decoded frames.
    pub fn decoded(
        frames: Arc<[Frame]>,
        sample_rate: u32,
        looped: bool,
        shared: Arc<VoiceShared>,
    ) -> Self {
        Self::new(
            Source::Decoded {
                frames,
                position: 0.0,
                looped,
            },
            sample_rate,
            shared,
        )
    }

    /// A voice reading a stream that a [`Feeder`] fills.
    pub fn stream(
        consumer: rtrb::Consumer<Frame>,
        sample_rate: u32,
        shared: Arc<VoiceShared>,
    ) -> Self {
        Self::new(
            Source::Stream {
                consumer,
                window: [Frame::ZERO; 4],
                fraction: 3.0,
                tail: 0,
                starved: false,
            },
            sample_rate,
            shared,
        )
    }

    fn new(source: Source, sample_rate: u32, shared: Arc<VoiceShared>) -> Self {
        let volume = shared.volume.load();
        let panning = shared.panning.load();
        let speed = shared.speed.load();
        Self {
            source,
            source_rate: f64::from(sample_rate),
            volume: Ramp::new(volume),
            panning: Ramp::new(panning),
            speed: Ramp::new(speed),
            envelope: Ramp::new(1.0),
            state: State::Playing,
            seen: (volume, panning, speed, Request::Play),
            started: false,
            shared,
        }
    }

    /// Picks up what the game asked for since the last block.
    fn read_shared(&mut self, dt: f64) {
        let shared = &self.shared;
        if !self.started {
            self.started = true;
            let fade_in = shared.fade.load();
            if fade_in > 0.0 {
                self.envelope = Ramp::new(0.0);
                self.envelope.set(1.0, fade_in, dt);
            }
        }
        let ramp = shared.ramp.load();
        let (volume, panning, speed) = (
            shared.volume.load(),
            shared.panning.load(),
            shared.speed.load(),
        );
        if volume != self.seen.0 {
            self.volume.set(volume, ramp, dt);
        }
        if panning != self.seen.1 {
            self.panning.set(panning, ramp, dt);
        }
        if speed != self.seen.2 {
            self.speed.set(speed, ramp, dt);
        }
        let request = shared.current_request();
        if request != self.seen.3 && self.state != State::Stopped {
            let fade = shared.fade.load();
            match request {
                Request::Play => {
                    self.state = State::Playing;
                    self.envelope.set(1.0, fade, dt);
                }
                Request::Pause => {
                    self.state = State::Pausing;
                    self.envelope.set(0.0, fade, dt);
                }
                Request::Stop => {
                    self.state = State::Stopping;
                    self.envelope.set(0.0, fade, dt);
                }
            }
        }
        self.seen = (volume, panning, speed, request);
    }

    /// The next frame of the source, advancing by `step` source frames; `None` once the
    /// source has run out.
    fn next_source_frame(&mut self, step: f64) -> Option<Frame> {
        match &mut self.source {
            Source::Decoded {
                frames,
                position,
                looped,
            } => {
                let len = frames.len();
                if len == 0 || (!*looped && *position >= len as f64) {
                    return None;
                }
                let index = position.floor() as isize;
                let fraction = (*position - index as f64) as f32;
                let at = |offset: isize| -> Frame {
                    let i = index + offset;
                    if *looped {
                        frames[i.rem_euclid(len as isize) as usize]
                    } else if (0..len as isize).contains(&i) {
                        frames[i as usize]
                    } else {
                        Frame::ZERO
                    }
                };
                let frame = interpolate_frame(at(-1), at(0), at(1), at(2), fraction);
                *position += step;
                if *looped {
                    *position = position.rem_euclid(len as f64);
                }
                Some(frame)
            }
            Source::Stream {
                consumer,
                window,
                fraction,
                tail,
                starved,
            } => {
                while *fraction >= 1.0 {
                    let next = match consumer.pop() {
                        Ok(frame) => {
                            *starved = false;
                            frame
                        }
                        Err(_) if self.shared.stream_ended.load(Ordering::Acquire) => {
                            // Shift silence in until the last frame has played.
                            if *tail >= 2 {
                                return None;
                            }
                            *tail += 1;
                            Frame::ZERO
                        }
                        Err(_) => {
                            // The main thread fell behind: play silence and wait.
                            if !*starved {
                                *starved = true;
                                self.shared.underruns.fetch_add(1, Ordering::Relaxed);
                            }
                            return Some(Frame::ZERO);
                        }
                    };
                    window.rotate_left(1);
                    window[3] = next;
                    *fraction -= 1.0;
                }
                let frame =
                    interpolate_frame(window[0], window[1], window[2], window[3], *fraction as f32);
                *fraction += step;
                Some(frame)
            }
        }
    }

    fn finish(&mut self) {
        self.state = State::Stopped;
        self.shared.finished.store(true, Ordering::Release);
    }
}

impl KiraSound for VoiceSound {
    fn process(&mut self, out: &mut [Frame], dt: f64, _info: &Info) {
        self.read_shared(dt);
        for frame in out.iter_mut() {
            if matches!(self.state, State::Stopped | State::Paused) {
                *frame = Frame::ZERO;
                continue;
            }
            self.volume.advance();
            self.panning.advance();
            self.speed.advance();
            self.envelope.advance();
            let step = f64::from(self.speed.value) * self.source_rate * dt;
            let Some(sample) = self.next_source_frame(step) else {
                self.finish();
                *frame = Frame::ZERO;
                continue;
            };
            let gain =
                self.volume.value * (self.envelope.value * core::f32::consts::FRAC_PI_2).sin();
            *frame = (sample * gain).panned(Panning(self.panning.value));
            if self.envelope.settled() && self.envelope.value == 0.0 {
                match self.state {
                    State::Pausing => self.state = State::Paused,
                    State::Stopping => self.finish(),
                    _ => {}
                }
            }
        }
    }

    fn finished(&self) -> bool {
        self.state == State::Stopped
    }
}

/// Hands a [`VoiceSound`] to kira.
pub(crate) struct VoiceData(pub VoiceSound);

impl SoundData for VoiceData {
    type Error = ();
    type Handle = ();

    fn into_sound(self) -> Result<(Box<dyn KiraSound>, ()), ()> {
        Ok((Box::new(self.0), ()))
    }
}

/// Decodes a stream on the main thread into its voice's ring buffer.
pub(crate) struct Feeder {
    bytes: Bytes,
    extension: Option<String>,
    decoder: Decoder,
    producer: rtrb::Producer<Frame>,
    pending: Vec<Frame>,
    pending_start: usize,
    looped: bool,
    /// Frames decoded since the file was last opened, to stop an empty file from looping
    /// for ever.
    pass_frames: u64,
    shared: Arc<VoiceShared>,
    done: bool,
    sample_rate: u32,
}

impl Feeder {
    /// Opens a stream of `sound` and decodes its first half second. Returns the feeder and
    /// the voice that plays it.
    pub fn start(
        sound: &Sound,
        looped: bool,
        shared: Arc<VoiceShared>,
    ) -> Result<(Self, VoiceSound), DecodeError> {
        let bytes = sound
            .bytes
            .clone()
            .ok_or_else(|| DecodeError("a sound made in code cannot be streamed".to_owned()))?;
        let decoder = Decoder::open(bytes.clone(), sound.extension.as_deref())?;
        let sample_rate = decoder.sample_rate;
        let capacity = (f64::from(sample_rate) * STREAM_BUFFER_SECS) as usize;
        let (producer, consumer) = rtrb::RingBuffer::new(capacity.max(1024));
        let mut feeder = Self {
            bytes,
            extension: sound.extension.clone(),
            decoder,
            producer,
            pending: Vec::new(),
            pending_start: 0,
            looped,
            pass_frames: 0,
            shared: shared.clone(),
            done: false,
            sample_rate,
        };
        feeder.feed_secs(STREAM_PREFILL_SECS)?;
        Ok((feeder, VoiceSound::stream(consumer, sample_rate, shared)))
    }

    /// Decodes up to `secs` seconds of frames into the ring buffer, as far as it has room.
    pub fn feed_secs(&mut self, secs: f64) -> Result<(), DecodeError> {
        let budget = (f64::from(self.sample_rate) * secs) as usize;
        let mut written = 0;
        while written < budget && !self.done {
            if self.pending_start == self.pending.len() {
                self.pending.clear();
                self.pending_start = 0;
                if self.decoder.decode_next(&mut self.pending)? {
                    self.pass_frames += self.pending.len() as u64;
                } else if self.looped && self.pass_frames > 0 {
                    // Back to the start: a fresh decoder trims the encoder delay again,
                    // so the loop is seamless.
                    self.decoder = Decoder::open(self.bytes.clone(), self.extension.as_deref())?;
                    self.pass_frames = 0;
                } else {
                    self.done = true;
                    self.shared.stream_ended.store(true, Ordering::Release);
                }
                continue;
            }
            let room = self.producer.slots();
            if room == 0 {
                break;
            }
            let count = room
                .min(self.pending.len() - self.pending_start)
                .min(budget - written);
            for frame in &self.pending[self.pending_start..self.pending_start + count] {
                // Cannot fail: `count` is at most the free slots.
                let _ = self.producer.push(*frame);
            }
            self.pending_start += count;
            written += count;
        }
        Ok(())
    }

    /// Decodes this frame's share.
    pub fn feed(&mut self) -> Result<(), DecodeError> {
        self.feed_secs(STREAM_FEED_SECS)
    }

    /// Frames waiting in the ring buffer.
    #[cfg(test)]
    pub fn buffered(&self) -> usize {
        self.producer.buffer().capacity() - self.producer.slots()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kira::info::MockInfoBuilder;

    const RATE: f64 = 100.0;

    fn run(sound: &mut VoiceSound, frames: usize) -> Vec<Frame> {
        let info = MockInfoBuilder::new().build();
        let mut out = vec![Frame::ZERO; frames];
        sound.process(&mut out, 1.0 / RATE, &info);
        out
    }

    fn ramp_frames(n: usize) -> Arc<[Frame]> {
        (0..n).map(|i| Frame::from_mono(i as f32)).collect()
    }

    fn shared(volume: f32, fade_in: f32) -> Arc<VoiceShared> {
        Arc::new(VoiceShared::new(volume, 0.0, 1.0, fade_in))
    }

    #[test]
    fn decoded_voice_plays_once_and_finishes() {
        let shared = shared(1.0, 0.0);
        let mut voice = VoiceSound::decoded(ramp_frames(10), RATE as u32, false, shared.clone());
        let out = run(&mut voice, 12);
        // Whole-frame steps read the frames exactly.
        for (i, frame) in out.iter().take(10).enumerate() {
            assert!((frame.left - i as f32).abs() < 1e-4, "{i}: {frame:?}");
        }
        assert_eq!(out[10], Frame::ZERO);
        assert!(voice.finished());
        assert!(shared.finished());
    }

    #[test]
    fn looped_voice_wraps_and_speed_doubles_the_step() {
        let shared = Arc::new(VoiceShared::new(1.0, 0.0, 2.0, 0.0));
        let mut voice = VoiceSound::decoded(ramp_frames(10), RATE as u32, true, shared);
        let out = run(&mut voice, 8);
        let lefts: Vec<i32> = out.iter().map(|f| f.left.round() as i32).collect();
        assert_eq!(lefts, [0, 2, 4, 6, 8, 0, 2, 4]);
        assert!(!voice.finished());
    }

    #[test]
    fn source_rate_is_resampled() {
        // A 50 Hz sound on a 100 Hz output advances half a frame per output frame. The
        // ramp starts at 1 so the silence before the first frame continues it.
        let frames: Arc<[Frame]> = (0..10).map(|i| Frame::from_mono(i as f32 + 1.0)).collect();
        let mut voice = VoiceSound::decoded(frames, 50, false, shared(1.0, 0.0));
        let out = run(&mut voice, 4);
        let lefts: Vec<f32> = out.iter().map(|f| f.left).collect();
        for (got, want) in lefts.iter().zip([1.0, 1.5, 2.0, 2.5]) {
            assert!((got - want).abs() < 1e-4, "{lefts:?}");
        }
    }

    #[test]
    fn stop_fades_out_then_finishes() {
        let frames: Arc<[Frame]> = vec![Frame::from_mono(1.0); 1000].into();
        let shared = shared(1.0, 0.0);
        let mut voice = VoiceSound::decoded(frames, RATE as u32, true, shared.clone());
        run(&mut voice, 10);
        shared.request(Request::Stop, 0.1);
        let out = run(&mut voice, 20);
        assert!(out[0].left > 0.9 && out[0].left < 1.0, "{:?}", out[0]);
        assert!(out[5].left < out[0].left);
        assert_eq!(out[15], Frame::ZERO);
        assert!(shared.finished());
    }

    #[test]
    fn pause_holds_the_position_and_resume_continues() {
        let shared = shared(1.0, 0.0);
        let mut voice = VoiceSound::decoded(ramp_frames(100), RATE as u32, false, shared.clone());
        run(&mut voice, 5);
        shared.request(Request::Pause, 0.0);
        let paused = run(&mut voice, 10);
        assert!(paused.iter().all(|f| *f == Frame::ZERO));
        shared.request(Request::Play, 0.0);
        let out = run(&mut voice, 1);
        assert!((out[0].left - 6.0).abs() < 1e-4, "{:?}", out[0]);
    }

    #[test]
    fn fade_in_and_volume_ramp() {
        let frames: Arc<[Frame]> = vec![Frame::from_mono(1.0); 1000].into();
        let shared = shared(0.5, 0.1);
        let mut voice = VoiceSound::decoded(frames, RATE as u32, true, shared.clone());
        let out = run(&mut voice, 12);
        assert!(out[0].left < 0.1);
        assert!((out[11].left - 0.5).abs() < 1e-4);
        shared.set_params(1.0, 0.0, 1.0, 0.1);
        let out = run(&mut voice, 12);
        assert!(out[4].left > 0.5 && out[4].left < 1.0);
        assert!((out[11].left - 1.0).abs() < 1e-4);
    }

    #[test]
    fn panning_moves_sound_between_channels() {
        let frames: Arc<[Frame]> = vec![Frame::from_mono(1.0); 100].into();
        let shared = Arc::new(VoiceShared::new(1.0, 1.0, 1.0, 0.0));
        let mut voice = VoiceSound::decoded(frames, RATE as u32, true, shared);
        let out = run(&mut voice, 1);
        assert!(
            out[0].left.abs() < 1e-4 && out[0].right > 1.0,
            "{:?}",
            out[0]
        );
    }
}
