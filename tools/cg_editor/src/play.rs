//! Play and Stop. Play gives the camera back to the game, lets fixed steps run and tells
//! the game; Stop tells the game, waits a moment for it to leave its level, then despawns
//! every entity made since Play and shows the edited scene again. The document itself is
//! never touched by Play.

use crate::doc::Document;
use crate::{Editor, Mode, PlayMode};
use cashgear_engine::core::ecs::resource::IsResource;
use cashgear_engine::platform::input::{ButtonInput, KeyCode};
use cashgear_engine::prelude::*;

/// Frames the game gets after Stop to go back to its idle state before the sweep.
const STOP_FRAMES: u8 = 2;

pub(crate) fn update(editor: &mut Editor, world: &mut World) {
    let keys = world.resource::<ButtonInput<KeyCode>>();
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let f5 = keys.just_pressed(KeyCode::F5);
    match editor.mode {
        Mode::Editing => {
            let mut time = world.resource_mut::<Time>();
            if !time.fixed_paused() {
                time.set_fixed_paused(true);
            }
            if (f5 && !shift) || core::mem::take(&mut editor.play_requested) {
                start(editor, world);
            }
        }
        Mode::Playing => {
            if f5 || core::mem::take(&mut editor.stop_requested) {
                stop(editor, world);
            }
        }
        // The sweep itself waits for the next frame's start (`sweep`).
        Mode::Stopping(0) => {}
        Mode::Stopping(n) => editor.mode = Mode::Stopping(n - 1),
    }
}

/// The entities of the world, resources (which are entities too) left out.
pub(crate) fn entities(world: &mut World) -> Vec<Entity> {
    world
        .query_filtered::<Entity, Without<IsResource>>()
        .iter(world)
        .collect()
}

/// Finishes a Stop at the start of a frame, before any system of the frame has queued
/// commands for entities the sweep despawns.
pub(crate) fn sweep(world: &mut World) {
    world.resource_scope::<Editor, _>(|world, mut editor| {
        if editor.mode == Mode::Stopping(0) {
            finish(&mut editor, world);
        }
    });
}

/// Starts playing the open document.
pub(crate) fn start(editor: &mut Editor, world: &mut World) {
    if editor.mode != Mode::Editing || editor.doc.is_none() {
        return;
    }
    if let Some(Document::Map(doc)) = &editor.doc
        && doc.synced_version.is_none()
    {
        log::warn!("{} has not loaded yet", doc.path);
        return;
    }
    editor.drag = None;
    editor.placing = None;
    editor.history.end_gesture();
    editor.before_play = entities(world).into_iter().collect();
    // The game's camera as the game set it up.
    if let (Some(camera), Some((settings, transform))) = (editor.camera, editor.game_camera.clone())
        && let Ok(mut entity) = world.get_entity_mut(camera)
    {
        entity.insert((settings, transform));
        if let Some(mut interpolated) = entity.get_mut::<Interpolated>() {
            interpolated.reset();
        }
    }
    if editor.play_mode == PlayMode::Game {
        // The game spawns the level itself, from the edited map asset.
        crate::despawn_scene(editor, world);
    }
    *world.resource_mut::<EditMode>() = EditMode::Playing;
    let mut time = world.resource_mut::<Time>();
    time.set_fixed_paused(false);
    time.set_time_scale(1.0);
    let map = match &editor.doc {
        Some(Document::Map(doc)) => Some(doc.path.clone()),
        _ => None,
    };
    log::info!(
        "editor: play {}",
        editor.doc.as_ref().map_or("", Document::path)
    );
    world.write_message(PlayStarted { map });
    editor.mode = Mode::Playing;
    editor.status = "Playing".into();
}

/// Stops playing.
pub(crate) fn stop(editor: &mut Editor, world: &mut World) {
    if editor.mode != Mode::Playing {
        return;
    }
    log::info!("editor: stop");
    world.write_message(PlayStopped);
    *world.resource_mut::<EditMode>() = EditMode::Editing;
    world.resource_mut::<Time>().set_fixed_paused(true);
    editor.mode = Mode::Stopping(STOP_FRAMES);
}

/// Puts the edited scene back: everything made since Play goes, the scene is shown again.
fn finish(editor: &mut Editor, world: &mut World) {
    let before = core::mem::take(&mut editor.before_play);
    let made: Vec<Entity> = entities(world)
        .into_iter()
        .filter(|e| !before.contains(e))
        .collect();
    let count = made.len();
    for entity in made {
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
    if let Some(mut audio) = world.get_resource_mut::<Audio>() {
        audio.stop_music(0.2);
    }
    let mut time = world.resource_mut::<Time>();
    time.set_time_scale(1.0);
    time.set_fixed_paused(true);
    // The scene again, freshly spawned from the document.
    crate::despawn_scene(editor, world);
    editor.scene_dirty = true;
    editor.mode = Mode::Editing;
    editor.status = "Stopped".into();
    log::info!("editor: stopped; despawned {count} entities left from playing");
}
