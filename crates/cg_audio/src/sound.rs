//! The [`Sound`] asset and its loader.

use crate::decode::{Bytes, DecodeError, Decoder, decode_all};
use cg_assets::{Asset, AssetLoader, BoxError, BoxedFuture, LoadContext};
use kira::Frame;
use std::sync::Arc;

/// A sound file (OGG Vorbis or WAV, mono or stereo), or a sound made in code.
///
/// Load one through the asset server: `server.load::<Sound>("sounds/jump.ogg")`. Short
/// sounds are decoded when they load, so they start at once and any number of voices can
/// share them. Longer ones (by default over 10 seconds, see
/// [`AudioPlugin::stream_longer_than`](crate::AudioPlugin)) stay compressed in memory and
/// are decoded a little each frame while they play, so a three-minute track costs its
/// file size, not 60 MB of samples.
#[derive(Clone)]
pub struct Sound {
    pub(crate) bytes: Option<Bytes>,
    pub(crate) extension: Option<String>,
    pub(crate) sample_rate: u32,
    pub(crate) channels: u16,
    pub(crate) frames: Option<u64>,
    pub(crate) decoded: Option<Arc<[Frame]>>,
}

impl Asset for Sound {}

impl std::fmt::Debug for Sound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sound")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .field("duration", &self.duration())
            .field("streamed", &self.is_streamed())
            .finish()
    }
}

impl Sound {
    /// Reads a sound from the bytes of an OGG Vorbis or WAV file. `extension` (such as
    /// `"ogg"`) helps guess the format; the contents decide. Sounds longer than
    /// `stream_after` seconds are kept compressed and streamed when played.
    pub fn from_file_bytes(
        bytes: Vec<u8>,
        extension: Option<&str>,
        stream_after: f32,
    ) -> Result<Self, DecodeError> {
        let bytes: Bytes = bytes.into();
        let mut decoder = Decoder::open(bytes.clone(), extension)?;
        let limit = (f64::from(stream_after.max(0.0)) * f64::from(decoder.sample_rate)) as u64;
        let decoded = match decoder.frames {
            // Long and says so: check that it decodes, then leave it compressed.
            Some(frames) if frames > limit => {
                decoder.decode_next(&mut Vec::new())?;
                None
            }
            _ => decode_all(&mut decoder, limit)?.map(Arc::from),
        };
        Ok(Self {
            frames: decoded
                .as_ref()
                .map(|frames: &Arc<[Frame]>| frames.len() as u64)
                .or(decoder.frames),
            sample_rate: decoder.sample_rate,
            channels: decoder.channels,
            extension: extension.map(str::to_owned),
            bytes: Some(bytes),
            decoded,
        })
    }

    /// A sound made in code from mono samples (-1 to 1) at `sample_rate` frames per second.
    pub fn from_mono(sample_rate: u32, samples: &[f32]) -> Self {
        Self::from_frames(
            sample_rate,
            1,
            samples.iter().map(|s| Frame::from_mono(*s)).collect(),
        )
    }

    /// A sound made in code from stereo `[left, right]` frames at `sample_rate` frames per
    /// second.
    pub fn from_stereo(sample_rate: u32, frames: &[[f32; 2]]) -> Self {
        Self::from_frames(
            sample_rate,
            2,
            frames.iter().map(|[l, r]| Frame::new(*l, *r)).collect(),
        )
    }

    fn from_frames(sample_rate: u32, channels: u16, frames: Arc<[Frame]>) -> Self {
        Self {
            bytes: None,
            extension: None,
            sample_rate: sample_rate.max(1),
            channels,
            frames: Some(frames.len() as u64),
            decoded: Some(frames),
        }
    }

    /// Frames per second.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// 1 for mono, 2 for stereo.
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// The length in seconds, when known. Streamed files that do not state their length
    /// return `None`.
    pub fn duration(&self) -> Option<f64> {
        self.frames
            .map(|frames| frames as f64 / f64::from(self.sample_rate))
    }

    /// Whether the sound is decoded while it plays rather than kept decoded.
    pub fn is_streamed(&self) -> bool {
        self.decoded.is_none()
    }
}

/// Loads [`Sound`]s from `.ogg`, `.oga` and `.wav` files.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoundLoader {
    /// Sounds longer than this many seconds are streamed.
    pub stream_after: f32,
}

impl AssetLoader for SoundLoader {
    type Asset = Sound;

    fn extensions(&self) -> &[&str] {
        &["ogg", "oga", "wav"]
    }

    fn load<'a>(&'a self, ctx: &'a mut LoadContext) -> BoxedFuture<'a, Result<Sound, BoxError>> {
        Box::pin(async move {
            let extension = ctx
                .path()
                .rsplit_once('.')
                .map(|(_, ext)| ext.to_ascii_lowercase());
            let bytes = ctx.take_bytes();
            Ok(Sound::from_file_bytes(
                bytes,
                extension.as_deref(),
                self.stream_after,
            )?)
        })
    }
}
