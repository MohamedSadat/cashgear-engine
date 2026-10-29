//! Tests of the audio plugin with the mock output, and of decoding real files.

// One test writes a WAV file into a temporary asset folder.
#![allow(clippy::disallowed_methods)]

use crate::device::{AudioDevice, MOCK_SAMPLE_RATE, VoiceInfo, jitter};
use crate::prelude::*;
use crate::voice::{Feeder, VoiceShared};
use cg_assets::{AssetPlugin, AssetServer, Assets, Handle};
use cg_core::App;
use cg_core::glam::{UVec2, Vec2, vec2};
use cg_platform::Window;
use cg_platform::storage::Storage;
use cg_render::Camera2D;
use cg_world::{GlobalTransform2D, Transform2D};
use core::time::Duration;
use kira::Frame;
use kira::info::MockInfoBuilder;
use kira::sound::Sound as _;
use std::sync::Arc;

const TONE_OGG: &[u8] = include_bytes!("../test_data/tone.ogg");
const STEREO_OGG: &[u8] = include_bytes!("../test_data/stereo.ogg");

fn mock() -> AudioPlugin {
    AudioPlugin {
        output: AudioOutput::Mock,
        settings_key: None,
        ..AudioPlugin::default()
    }
}

fn app_with(plugin: AudioPlugin) -> App {
    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        hot_reload: Some(false),
        ..Default::default()
    })
    .add_plugins(plugin);
    app
}

fn frame(app: &mut App) {
    app.update(Duration::from_millis(16));
}

/// Lets the mock device play `secs` seconds, then runs a frame so the game sees it.
fn play_for(app: &mut App, secs: f64) {
    app.world_mut()
        .non_send_mut::<AudioDevice>()
        .process_mock((secs * f64::from(MOCK_SAMPLE_RATE)) as usize);
    frame(app);
}

fn voices(app: &App) -> Vec<VoiceInfo> {
    app.world().non_send::<AudioDevice>().voices()
}

fn audio(app: &mut App) -> bevy_ecs::change_detection::Mut<'_, Audio> {
    app.world_mut().resource_mut::<Audio>()
}

fn add(app: &mut App, sound: Sound) -> Handle<Sound> {
    app.world_mut().resource_mut::<Assets<Sound>>().add(sound)
}

/// A steady mono tone of `secs` seconds at the mock device's rate.
fn tone(secs: f32) -> Sound {
    let frames = (secs * MOCK_SAMPLE_RATE as f32) as usize;
    Sound::from_mono(MOCK_SAMPLE_RATE, &vec![0.25; frames])
}

/// A 16-bit PCM WAV file.
fn wav(sample_rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    bytes
}

#[test]
fn wav_files_decode_in_mono_and_stereo() {
    let mono = Sound::from_file_bytes(wav(8000, 1, &[0, 16384, -16384, 32767]), Some("wav"), 10.0)
        .unwrap();
    assert_eq!((mono.sample_rate(), mono.channels()), (8000, 1));
    assert_eq!(mono.duration(), Some(4.0 / 8000.0));
    let frames = mono.decoded.as_ref().unwrap();
    assert_eq!(frames[1], Frame::from_mono(0.5));
    assert_eq!(frames[2], Frame::from_mono(-0.5));

    let stereo =
        Sound::from_file_bytes(wav(11025, 2, &[16384, -16384, 0, 32767]), None, 10.0).unwrap();
    assert_eq!(stereo.channels(), 2);
    let frames = stereo.decoded.as_ref().unwrap();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0], Frame::new(0.5, -0.5));
}

#[test]
fn bad_files_are_errors() {
    let err = Sound::from_file_bytes(b"not a sound".to_vec(), Some("ogg"), 10.0).unwrap_err();
    assert!(!err.to_string().is_empty());
    assert!(Sound::from_file_bytes(Vec::new(), Some("wav"), 10.0).is_err());
}

#[test]
fn ogg_vorbis_decodes_gaplessly() {
    // ffmpeg's 0.25 s tone at 44.1 kHz: the decoder trims the encoder's delay and padding,
    // so the length is exact, which seamless loops rely on.
    let sound = Sound::from_file_bytes(TONE_OGG.to_vec(), Some("ogg"), 10.0).unwrap();
    assert_eq!((sound.sample_rate(), sound.channels()), (44100, 1));
    assert_eq!(sound.decoded.as_ref().unwrap().len(), 11025);
    let peak = sound
        .decoded
        .as_ref()
        .unwrap()
        .iter()
        .fold(0f32, |peak, f| peak.max(f.left.abs()));
    assert!(peak > 0.05 && peak <= 1.0, "{peak}");
    assert!(!sound.is_streamed());
}

#[test]
fn long_sounds_stream_and_loop_seamlessly() {
    let decoded = Sound::from_file_bytes(STEREO_OGG.to_vec(), Some("ogg"), 10.0).unwrap();
    let streamed = Sound::from_file_bytes(STEREO_OGG.to_vec(), Some("ogg"), 0.1).unwrap();
    assert!(streamed.is_streamed());
    assert_eq!(streamed.duration(), Some(0.5));
    let reference = decoded.decoded.clone().unwrap();
    assert_eq!(reference.len(), 11025);

    // Played at its own rate, a stream's output is the decoded file, over and over.
    let shared = Arc::new(VoiceShared::new(1.0, 0.0, 1.0, 0.0));
    let (mut feeder, mut voice) = Feeder::start(&streamed, true, shared.clone()).unwrap();
    let info = MockInfoBuilder::new().build();
    let dt = 1.0 / 22050.0;
    let mut out = Vec::new();
    while out.len() < reference.len() * 5 / 2 {
        feeder.feed().unwrap();
        let mut block = vec![Frame::ZERO; 1000];
        voice.process(&mut block, dt, &info);
        out.extend(block);
    }
    for (i, frame) in out.iter().enumerate() {
        let want = reference[i % reference.len()];
        assert!(
            (frame.left - want.left).abs() < 1e-5 && (frame.right - want.right).abs() < 1e-5,
            "frame {i}: {frame:?} != {want:?}"
        );
    }
    assert_eq!(shared.underruns(), 0);
    assert!(feeder.buffered() > 0);
}

#[test]
fn one_shot_plays_then_finishes() {
    let mut app = app_with(mock());
    let beep = add(&mut app, tone(0.1));
    frame(&mut app);
    let voice = audio(&mut app).play(&beep);
    assert!(audio(&mut app).is_playing(voice));
    frame(&mut app);
    assert_eq!(audio(&mut app).status(), AudioStatus::Running);
    assert_eq!(voices(&app).len(), 1);
    assert!(audio(&mut app).is_playing(voice));
    play_for(&mut app, 0.2);
    assert!(!audio(&mut app).is_playing(voice));
    assert_eq!(audio(&mut app).stats().voices, 0);
}

#[test]
fn copies_past_the_cap_cut_the_oldest_short() {
    let mut app = app_with(mock());
    let coin = add(&mut app, tone(1.0));
    frame(&mut app);
    let played: Vec<Voice> = (0..6).map(|_| audio(&mut app).play(&coin)).collect();
    frame(&mut app);
    let all = voices(&app);
    assert_eq!(all.len(), 6);
    let ending: Vec<Voice> = all.iter().filter(|v| v.ending).map(|v| v.voice).collect();
    assert_eq!(ending, played[..2]);
    assert_eq!(audio(&mut app).stats().stolen, 2);
    play_for(&mut app, 0.1);
    assert_eq!(voices(&app).len(), 4);
    assert!(!audio(&mut app).is_playing(played[0]));
    assert!(audio(&mut app).is_playing(played[5]));

    // A per-play cap of one replaces the copy that is playing.
    audio(&mut app).play_with(&coin, PlaySettings::new().with_max_instances(1));
    frame(&mut app);
    assert_eq!(voices(&app).iter().filter(|v| !v.ending).count(), 1);
    assert_eq!(audio(&mut app).stats().stolen, 6);
}

#[test]
fn voice_cap_skips_new_sounds_when_everything_loops() {
    let mut app = app_with(AudioPlugin {
        max_voices: 2,
        ..mock()
    });
    let [a, b, c] = [tone(1.0), tone(1.0), tone(1.0)].map(|sound| add(&mut app, sound));
    frame(&mut app);
    audio(&mut app).play_with(&a, PlaySettings::new().looped());
    audio(&mut app).play_with(&b, PlaySettings::new().looped());
    let skipped = audio(&mut app).play(&c);
    frame(&mut app);
    assert!(!audio(&mut app).is_playing(skipped));
    assert_eq!(voices(&app).len(), 2);

    // Music is not counted.
    audio(&mut app).play_music(&c, MusicSettings::default());
    frame(&mut app);
    assert_eq!(voices(&app).len(), 3);
}

#[test]
fn positioned_sounds_pan_and_fade_from_the_camera() {
    let mut app = app_with(mock());
    let beep = add(&mut app, tone(1.0));
    app.world_mut()
        .spawn((Camera2D::default(), Transform2D::from_xy(100.0, 0.0)));
    frame(&mut app);
    audio(&mut app).play_at(&beep, vec2(420.0, 0.0));
    let far = audio(&mut app).play_at(&beep, vec2(-1000.0, 0.0));
    audio(&mut app).play_with(&beep, PlaySettings::new().with_volume(0.5));
    frame(&mut app);
    let all = voices(&app);
    assert_eq!(all.len(), 2, "the far one-shot is skipped");
    assert!(!audio(&mut app).is_playing(far));
    let (gain, pan) = Spatial::default().gain_and_pan(Vec2::ZERO, vec2(320.0, 0.0));
    assert!((all[0].volume - gain).abs() < 1e-6);
    assert_eq!(all[0].panning, pan);
    assert_eq!((all[1].volume, all[1].panning), (0.5, 0.0));

    // A listener entity wins over the camera.
    app.world_mut()
        .spawn((AudioListener, Transform2D::from_xy(420.0, 0.0)));
    frame(&mut app);
    assert_eq!(voices(&app)[0].volume, 1.0);
    assert_eq!(voices(&app)[0].panning, 0.0);
}

#[test]
fn emitters_follow_their_entity_and_stop_with_it() {
    let mut app = app_with(mock());
    let hum = add(&mut app, tone(0.5));
    app.world_mut()
        .spawn((Camera2D::default(), Transform2D::default()));
    let machine = app
        .world_mut()
        .spawn((
            AudioEmitter::new(hum.clone()),
            Transform2D::from_xy(200.0, 0.0),
        ))
        .id();
    frame(&mut app);
    let all = voices(&app);
    assert_eq!(all.len(), 1);
    assert!(all[0].panning > 0.0);

    app.world_mut()
        .get_mut::<Transform2D>(machine)
        .unwrap()
        .translation = vec2(-200.0, 0.0);
    frame(&mut app);
    assert!(voices(&app)[0].panning < 0.0);
    assert_eq!(
        app.world()
            .get::<GlobalTransform2D>(machine)
            .unwrap()
            .translation(),
        vec2(-200.0, 0.0)
    );

    // It loops past its length.
    play_for(&mut app, 1.2);
    assert_eq!(voices(&app).len(), 1);

    // Replacing the component restarts it; despawning stops it.
    app.world_mut()
        .entity_mut(machine)
        .insert(AudioEmitter::new(hum).with_volume(0.5));
    frame(&mut app);
    assert_eq!(voices(&app).iter().filter(|v| !v.ending).count(), 1);
    app.world_mut().despawn(machine);
    frame(&mut app);
    assert!(voices(&app).iter().all(|v| v.ending));
    play_for(&mut app, 0.2);
    assert!(voices(&app).is_empty());
}

#[test]
fn music_crossfades_and_ignores_a_repeat() {
    let mut app = app_with(mock());
    let [day, night] = [tone(2.0), tone(2.0)].map(|sound| add(&mut app, sound));
    frame(&mut app);
    audio(&mut app).play_music(&day, MusicSettings::default());
    frame(&mut app);
    assert_eq!(voices(&app).len(), 1);
    assert_eq!(voices(&app)[0].bus, Bus::Music);

    audio(&mut app).play_music(&day, MusicSettings::default());
    frame(&mut app);
    assert_eq!(voices(&app).len(), 1, "the same track keeps playing");

    audio(&mut app).play_music(
        &night,
        MusicSettings {
            crossfade: 0.5,
            ..MusicSettings::default()
        },
    );
    frame(&mut app);
    let all = voices(&app);
    assert_eq!(all.len(), 2);
    assert!(all[0].ending && !all[1].ending);
    assert_eq!(audio(&mut app).music(), Some(&night));
    play_for(&mut app, 0.6);
    assert_eq!(voices(&app).len(), 1);

    audio(&mut app).stop_music(0.1);
    assert_eq!(audio(&mut app).music(), None);
    frame(&mut app);
    assert!(voices(&app)[0].ending);
    play_for(&mut app, 0.2);
    assert!(voices(&app).is_empty());
}

#[test]
fn stop_pause_and_stop_bus() {
    let mut app = app_with(mock());
    let beep = add(&mut app, tone(1.0));
    frame(&mut app);
    let a = audio(&mut app).play(&beep);
    let b = audio(&mut app).play_with(&beep, PlaySettings::new().with_bus(Bus::Ui));
    frame(&mut app);
    audio(&mut app).pause(a, 0.0);
    audio(&mut app).set_speed(a, 2.0, 0.0);
    frame(&mut app);
    play_for(&mut app, 1.5);
    // Paused, it keeps its place instead of running out.
    assert!(audio(&mut app).is_playing(a));
    assert!(!audio(&mut app).is_playing(b));
    assert_eq!(voices(&app)[0].speed, 2.0);
    audio(&mut app).stop_bus(Bus::Sfx, 0.0);
    frame(&mut app);
    play_for(&mut app, 0.01);
    assert!(!audio(&mut app).is_playing(a));
}

#[test]
fn the_web_waits_for_user_activation() {
    let mut app = app_with(mock());
    let beep = add(&mut app, tone(0.2));
    let theme = add(&mut app, tone(2.0));
    // A window the user has not touched yet.
    app.insert_resource(Window::default());
    frame(&mut app);
    assert_eq!(audio(&mut app).status(), AudioStatus::Waiting);
    let skipped = audio(&mut app).play(&beep);
    let ambience = audio(&mut app).play_with(&beep, PlaySettings::new().looped());
    audio(&mut app).play_music(&theme, MusicSettings::default());
    frame(&mut app);
    assert!(!audio(&mut app).is_playing(skipped));
    assert!(audio(&mut app).is_playing(ambience));
    assert!(voices(&app).is_empty());

    app.insert_resource(Window::with_size(UVec2::new(640, 360), 1.0));
    frame(&mut app);
    assert_eq!(audio(&mut app).status(), AudioStatus::Running);
    assert_eq!(voices(&app).len(), 2);
}

#[test]
fn a_suspended_app_pauses_and_resumes() {
    let mut app = app_with(mock());
    app.insert_resource(Window::with_size(UVec2::new(640, 360), 1.0));
    let theme = add(&mut app, tone(1.0));
    frame(&mut app);
    audio(&mut app).play_music(&theme, MusicSettings::default());
    frame(&mut app);
    // `Window::default()` is suspended, as the runner leaves it in the background.
    app.insert_resource(Window::default());
    app.world_mut().run_schedule(cg_platform::LifecycleChanged);
    assert_eq!(app.world().non_send::<AudioDevice>().voices().len(), 1);
    frame(&mut app);
    assert_eq!(audio(&mut app).status(), AudioStatus::Paused);
    app.insert_resource(Window::with_size(UVec2::new(640, 360), 1.0));
    app.world_mut().run_schedule(cg_platform::LifecycleChanged);
    frame(&mut app);
    assert_eq!(audio(&mut app).status(), AudioStatus::Running);
}

#[test]
fn settings_load_and_save_through_storage() {
    let storage = Storage::memory();
    storage
        .write("audio.ron", "(master: 0.5, music: 2.0, muted: true)")
        .unwrap();
    let mut app = App::new();
    app.insert_resource(storage.clone())
        .add_plugins(AudioPlugin {
            settings_key: Some("audio.ron".to_owned()),
            ..mock()
        });
    let settings = app.world().resource::<AudioSettings>().clone();
    assert_eq!(
        settings,
        AudioSettings {
            master: 0.5,
            music: 1.0,
            sfx: 1.0,
            ui: 1.0,
            muted: true,
        }
    );
    assert_eq!(settings.master_amplitude(), 0.0);
    frame(&mut app);
    // Loading is not a change, so nothing is written back yet.
    assert!(storage.read("audio.ron").unwrap().unwrap().contains("2.0"));

    let mut settings = app.world_mut().resource_mut::<AudioSettings>();
    settings.muted = false;
    settings.step_level(Bus::Sfx, -0.3);
    frame(&mut app);
    let saved = storage.read("audio.ron").unwrap().unwrap();
    let reread: AudioSettings = ron::from_str(&saved).unwrap();
    assert_eq!(reread.sfx, 0.7);
    assert!(!reread.muted);
    assert_eq!(reread.bus_amplitude(Bus::Sfx), 0.7 * 0.7);
}

#[test]
fn files_load_through_the_asset_server_and_wait_for_it() {
    let dir = std::env::temp_dir().join(format!("cg_audio_files_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("sounds/blip.wav"), wav(48000, 1, &[8000; 4800])).unwrap();
    std::fs::write(dir.join("sounds/tone.ogg"), TONE_OGG).unwrap();

    let mut app = App::new();
    app.add_plugins(AssetPlugin {
        root: Some(dir.to_string_lossy().into_owned()),
        hot_reload: Some(false),
        ..Default::default()
    })
    .add_plugins(mock());
    frame(&mut app);
    let server = app.world().resource::<AssetServer>().clone();
    let blip: Handle<Sound> = server.load("sounds/blip.wav");
    let tone: Handle<Sound> = server.load("sounds/tone.ogg");
    let missing: Handle<Sound> = server.load("sounds/missing.ogg");
    // Played before they have loaded: they start once they have.
    let a = audio(&mut app).play(&blip);
    let b = audio(&mut app).play(&tone);
    let c = audio(&mut app).play(&missing);
    frame(&mut app);
    frame(&mut app);
    assert!(audio(&mut app).is_playing(a));
    assert!(audio(&mut app).is_playing(b));
    assert!(!audio(&mut app).is_playing(c));
    assert_eq!(voices(&app).len(), 2);
    let sounds = app.world().resource::<Assets<Sound>>();
    assert_eq!(sounds.get(blip.id()).unwrap().duration(), Some(0.1));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn streamed_sounds_play_through_the_app() {
    let mut app = app_with(mock());
    let long = Sound::from_file_bytes(STEREO_OGG.to_vec(), Some("ogg"), 0.1).unwrap();
    let long = add(&mut app, long);
    frame(&mut app);
    let voice = audio(&mut app).play(&long);
    frame(&mut app);
    assert!(voices(&app)[0].streamed);
    assert_eq!(audio(&mut app).stats().streams, 1);
    for _ in 0..40 {
        play_for(&mut app, 0.016);
    }
    assert!(!audio(&mut app).is_playing(voice));
    assert_eq!(audio(&mut app).stats().underruns, 0);
}

#[test]
fn no_output_is_silent_but_safe() {
    let mut app = app_with(AudioPlugin {
        output: AudioOutput::Off,
        ..mock()
    });
    let beep = add(&mut app, tone(0.1));
    frame(&mut app);
    let voice = audio(&mut app).play_with(&beep, PlaySettings::new().looped());
    audio(&mut app).play_music(&beep, MusicSettings::default());
    frame(&mut app);
    assert_eq!(audio(&mut app).status(), AudioStatus::Off);
    assert!(!audio(&mut app).is_playing(voice));
    assert!(voices(&app).is_empty());
}

#[test]
fn pitch_jitter_is_bounded_and_repeatable() {
    let factors: Vec<f32> = (1..200).map(|id| jitter(Voice(id), 2.0)).collect();
    let (low, high) = (2f32.powf(-2.0 / 12.0), 2f32.powf(2.0 / 12.0));
    assert!(factors.iter().all(|f| (low..=high).contains(f)));
    assert!(factors.iter().any(|f| *f < 0.95) && factors.iter().any(|f| *f > 1.05));
    assert_eq!(jitter(Voice(7), 2.0), jitter(Voice(7), 2.0));
    assert_eq!(jitter(Voice(7), 0.0), 1.0);
}
