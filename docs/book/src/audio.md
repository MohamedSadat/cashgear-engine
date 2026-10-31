# Audio

Sounds are OGG Vorbis or WAV files, mono or stereo, loaded as `Sound` assets. `Audio` (a resource) plays them:

```rust,ignore
fn coin(mut audio: ResMut<Audio>, sounds: Res<Sounds>) {
    audio.play(&sounds.coin);
    audio.play_with(&sounds.step, PlaySettings { volume: 0.6, pitch_jitter: 0.1, ..default() });
}

fn music(mut audio: ResMut<Audio>, server: Res<AssetServer>) {
    audio.play_music(&server.load("music/meadow.ogg"), MusicSettings { crossfade: 1.5, ..default() });
}
```

Sounds up to 10 seconds are decoded when they load; longer ones (music) stream while they play. Music crossfades from track to track; asking for the track already playing leaves it playing.

## Buses and settings

Every sound plays on a bus: `Sfx`, `Ui` or `Music`, all under a master bus. `AudioSettings` holds their levels and mute; it is saved through `Storage` when it changes and comes back on the next run, so a settings menu only changes the resource.

## Positional sound

An `AudioEmitter` plays a sound from its entity, and `PlaySettings` can place a one-shot at a position or follow an entity. Sounds pan left and right and fade with distance from the `AudioListener`, or the camera. 

## Limits

At most 32 effect and interface sounds play at once, and 4 copies of one sound; past a cap the oldest one is cut short.

## Platforms

In browsers, sound starts after the player's first key press, click or touch (browsers require it); music asked for earlier starts then. Sound pauses while the window is minimized, the tab hidden or the Android app in the background. `--no-audio` turns it off.
