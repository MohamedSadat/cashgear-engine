//! Decoding OGG Vorbis and WAV files into stereo frames with symphonia.

use kira::Frame;
use std::io::Cursor;
use std::sync::Arc;
use symphonia::core::audio::conv::{FromSample, IntoSample};
use symphonia::core::audio::sample::Sample;
use symphonia::core::audio::{Audio, AudioBuffer, GenericAudioBufferRef};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

/// The encoded bytes of a sound file, shared by the asset and every stream playing it.
pub(crate) type Bytes = Arc<[u8]>;

/// An error while reading a sound file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(pub(crate) String);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

impl From<SymphoniaError> for DecodeError {
    fn from(err: SymphoniaError) -> Self {
        Self(match err {
            SymphoniaError::Unsupported(what) => {
                format!("unsupported audio ({what}); use OGG Vorbis or WAV")
            }
            other => other.to_string(),
        })
    }
}

/// Decodes one sound file packet by packet.
pub(crate) struct Decoder {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    /// Frames per second of the file.
    pub sample_rate: u32,
    /// Channels in the file: 1 (mono) or 2 (stereo).
    pub channels: u16,
    /// The length in frames, if the file says.
    pub frames: Option<u64>,
}

impl Decoder {
    /// Opens `bytes`, guessing the format from the contents (and `extension`, if given).
    pub fn open(bytes: Bytes, extension: Option<&str>) -> Result<Self, DecodeError> {
        let mut hint = Hint::new();
        if let Some(extension) = extension {
            hint.with_extension(extension);
        }
        let source = MediaSourceStream::new(
            Box::new(Cursor::new(bytes)),
            MediaSourceStreamOptions::default(),
        );
        let reader = symphonia::default::get_probe().probe(
            &hint,
            source,
            FormatOptions::default(),
            MetadataOptions::default(),
        )?;
        let track = reader
            .default_track(TrackType::Audio)
            .ok_or_else(|| DecodeError("no audio track".to_owned()))?;
        let Some(CodecParameters::Audio(params)) = track.codec_params.as_ref() else {
            return Err(DecodeError("no audio track".to_owned()));
        };
        let sample_rate = params
            .sample_rate
            .filter(|rate| *rate > 0)
            .ok_or_else(|| DecodeError("unknown sample rate".to_owned()))?;
        let channels = params.channels.as_ref().map_or(0, |c| c.count());
        if !(1..=2).contains(&channels) {
            return Err(DecodeError(format!(
                "{channels} channels; only mono and stereo sounds are supported"
            )));
        }
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())?;
        Ok(Self {
            track: track.id,
            frames: track.num_frames,
            reader,
            decoder,
            sample_rate,
            channels: channels as u16,
        })
    }

    /// Decodes the next packet and appends its frames to `out`. Returns `false` at the end
    /// of the file. Damaged packets are skipped, as players do.
    pub fn decode_next(&mut self, out: &mut Vec<Frame>) -> Result<bool, DecodeError> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(Some(packet)) => packet,
                Ok(None) => return Ok(false),
                Err(SymphoniaError::IoError(err))
                    if err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(false);
                }
                Err(err) => return Err(err.into()),
            };
            if packet.track_id != self.track {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(buffer) => {
                    append_frames(&buffer, out);
                    return Ok(true);
                }
                Err(SymphoniaError::DecodeError(why)) => {
                    log::debug!("skipping a damaged audio packet: {why}");
                }
                Err(err) => return Err(err.into()),
            }
        }
    }
}

/// Decodes a whole file, or `None` if it has more than `limit` frames.
pub(crate) fn decode_all(
    decoder: &mut Decoder,
    limit: u64,
) -> Result<Option<Vec<Frame>>, DecodeError> {
    let mut frames = Vec::with_capacity(decoder.frames.unwrap_or(0).min(limit) as usize);
    while decoder.decode_next(&mut frames)? {
        if frames.len() as u64 > limit {
            return Ok(None);
        }
    }
    Ok(Some(frames))
}

fn append_frames(buffer: &GenericAudioBufferRef, out: &mut Vec<Frame>) {
    match buffer {
        GenericAudioBufferRef::U8(buffer) => append_planes(buffer, out),
        GenericAudioBufferRef::U16(buffer) => append_planes(buffer, out),
        GenericAudioBufferRef::U24(buffer) => append_planes(buffer, out),
        GenericAudioBufferRef::U32(buffer) => append_planes(buffer, out),
        GenericAudioBufferRef::S8(buffer) => append_planes(buffer, out),
        GenericAudioBufferRef::S16(buffer) => append_planes(buffer, out),
        GenericAudioBufferRef::S24(buffer) => append_planes(buffer, out),
        GenericAudioBufferRef::S32(buffer) => append_planes(buffer, out),
        GenericAudioBufferRef::F32(buffer) => append_planes(buffer, out),
        GenericAudioBufferRef::F64(buffer) => append_planes(buffer, out),
    }
}

fn append_planes<S: Sample>(buffer: &AudioBuffer<S>, out: &mut Vec<Frame>)
where
    f32: FromSample<S>,
{
    match (buffer.plane(0), buffer.plane(1)) {
        (Some(left), Some(right)) => out.extend(
            left.iter()
                .zip(right)
                .map(|(l, r)| Frame::new((*l).into_sample(), (*r).into_sample())),
        ),
        (Some(mono), None) => out.extend(
            mono.iter()
                .map(|sample| Frame::from_mono((*sample).into_sample())),
        ),
        _ => {}
    }
}
