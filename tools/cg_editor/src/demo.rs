//! The scripted demo (`--demo`): with a map open, it moves an object, undoes and redoes
//! the move, places a tile, changes a property, saves (into a scratch folder), plays for
//! two seconds and stops, checking after each step, then exits. CI runs it, and a test
//! runs it without a window.

use crate::doc::{Document, Selected};
use crate::tmx::{self, PropertyData};
use crate::{Editor, Mode, Placement, play, view};
use cashgear_engine::prelude::*;

/// The demo fails if it has not finished after this many frames.
const TIMEOUT: u32 = 3000;
/// Frames of play.
const PLAY_FRAMES: u32 = 120;

/// The demo's progress.
#[derive(Default)]
pub(crate) struct Demo {
    step: usize,
    frames: u32,
    wait: u32,
    target: Option<(u32, f32)>,
    objects: usize,
    entities: usize,
    fixed_steps: u64,
}

pub(crate) fn update(editor: &mut Editor, world: &mut World) {
    let Some(mut demo) = editor.demo.take() else {
        return;
    };
    demo.frames += 1;
    let result = if demo.frames > TIMEOUT {
        Err(format!("timed out at step {}", demo.step))
    } else if demo.wait > 0 {
        demo.wait -= 1;
        Ok(false)
    } else {
        step(&mut demo, editor, world)
    };
    match result {
        Ok(true) => {
            log::info!("editor demo: finished after {} frames", demo.frames);
            world.write_message(AppExit::Success);
        }
        Ok(false) => editor.demo = Some(demo),
        Err(err) => {
            log::error!("editor demo: {err}");
            world.write_message(AppExit::error());
        }
    }
}

fn check(ok: bool, what: impl FnOnce() -> String) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what()) }
}

/// One step; `Ok(true)` when the demo is done.
fn step(demo: &mut Demo, editor: &mut Editor, world: &mut World) -> Result<bool, String> {
    let objects_now = |editor: &Editor| {
        editor.map().map_or(0, |d| {
            d.objects
                .layers
                .iter()
                .map(|l| l.objects.len())
                .sum::<usize>()
        })
    };
    match demo.step {
        // The map has loaded and its objects are spawned.
        0 => {
            let Some(doc) = editor.map() else {
                if editor.doc.is_none() && editor.pending_open.is_none() && demo.frames > 30 {
                    return Err("the demo needs a map: pass a .tmx".into());
                }
                return Ok(false);
            };
            if doc.synced_version.is_none() || editor.scene.is_none() {
                return Ok(false);
            }
            if editor.save_dir.is_none() {
                let dir =
                    std::env::temp_dir().join(format!("cg_editor_demo_{}", std::process::id()));
                log::info!("editor demo: saving into {}", dir.display());
                editor.save_dir = Some(dir);
            }
            demo.objects = objects_now(editor);
            check(demo.objects > 0, || "the map has no objects".into())?;
            demo.wait = 5;
        }
        // Select a tile object and move it one grid step right.
        1 => {
            let doc = editor.map().ok_or("no map")?;
            let (id, x) = doc
                .objects
                .layers
                .iter()
                .flat_map(|l| l.objects.iter())
                .find(|o| o.gid != 0)
                .map(|o| (o.id, o.x))
                .ok_or("the map has no tile object to move")?;
            demo.target = Some((id, x));
            editor.selection = vec![Selected::Object(id)];
            let step = editor.grid_step(world);
            view::nudge(editor, world, vec2(step, 0.0));
            let moved = editor.map().and_then(|d| d.find(id)).map(|(_, o)| o.x);
            check(moved == Some(x + step), || {
                format!("object {id} is at {moved:?}, not {}", x + step)
            })?;
            demo.wait = 3;
        }
        // The world shows it moved; undo, then redo.
        2 => {
            let (id, x) = demo.target.ok_or("no target")?;
            let step = editor.grid_step(world);
            let entity_x =
                object_entity_x(editor, world, id).ok_or("the moved object has no entity")?;
            let expected = world_x(editor, world, id).ok_or("no map")?;
            check((entity_x - expected).abs() < 0.01, || {
                format!("object {id} is drawn at {entity_x}, not {expected}")
            })?;
            editor.undo();
            let after_undo = editor.map().and_then(|d| d.find(id)).map(|(_, o)| o.x);
            check(after_undo == Some(x), || {
                format!("undo left object {id} at {after_undo:?}")
            })?;
            editor.redo();
            let after_redo = editor.map().and_then(|d| d.find(id)).map(|(_, o)| o.x);
            check(after_redo == Some(x + step), || {
                format!("redo left object {id} at {after_redo:?}")
            })?;
            demo.wait = 3;
        }
        // Place a classed tile at the middle of the map.
        3 => {
            let doc = editor.map().ok_or("no map")?;
            let map = world
                .resource::<Assets<TiledMap>>()
                .get(&doc.handle)
                .ok_or("the map is not loaded")?;
            let mut classed: Vec<(usize, u32, String)> = map
                .tilesets
                .iter()
                .enumerate()
                .flat_map(|(i, t)| {
                    t.tiles
                        .iter()
                        .filter(|(_, info)| !info.class.is_empty())
                        .map(move |(id, info)| (i, *id, info.class.clone()))
                })
                .collect();
            // A spring if the map has one (the platformer), else the first.
            classed.sort_by_key(|c| (c.2 != "Spring", c.0, c.1));
            let (tileset, tile, class) = classed.first().cloned().ok_or("no tile has a class")?;
            let at = map.bounds().center();
            let before = objects_now(editor);
            view::place(editor, world, &Placement::Tile { tileset, tile }, at, true);
            check(objects_now(editor) == before + 1, || {
                "placing added no object".into()
            })?;
            log::info!("editor demo: placed a {class} tile");
            demo.wait = 3;
        }
        // Change an int property (a walker's speed, in the platformer).
        4 => {
            let doc = editor.map().ok_or("no map")?;
            let found = doc
                .objects
                .layers
                .iter()
                .flat_map(|l| l.objects.iter())
                .find_map(|o| {
                    o.properties.iter().find_map(|p| match p.value {
                        PropertyData::Int(v) => Some((o.id, p.name.clone(), v)),
                        _ => None,
                    })
                });
            if let Some((id, name, value)) = found {
                editor.edit("edit property", None, |doc, _| {
                    if let Document::Map(map) = doc
                        && let Some(object) = map.find_mut(id)
                    {
                        crate::doc::set_property(object, &name, PropertyData::Int(value + 1));
                    }
                });
                log::info!("editor demo: object {id}'s `{name}` is now {}", value + 1);
            }
            demo.wait = 3;
        }
        // Save, and read the file back.
        5 => {
            check(editor.doc.as_ref().is_some_and(Document::dirty), || {
                "the edits did not make the map dirty".into()
            })?;
            check(crate::save(editor, world), || "saving failed".into())?;
            let doc = editor.map().ok_or("no map")?;
            let saved =
                std::fs::read_to_string(editor.save_path(&doc.path)).map_err(|e| e.to_string())?;
            let read = tmx::read_map(&saved)?;
            check(read == doc.objects, || {
                "the saved file reads back differently".into()
            })?;
            let original =
                std::fs::read_to_string(editor.root.join(&doc.path)).map_err(|e| e.to_string())?;
            // Everything before the first object layer is as it was, but for the map's
            // next object id.
            let head = |text: &str| {
                let head = text.split("<objectgroup").next().unwrap_or_default();
                match head.split_once(" nextobjectid=\"") {
                    Some((before, after)) => {
                        format!(
                            "{before}{}",
                            after.split_once('"').map_or("", |(_, rest)| rest)
                        )
                    }
                    None => head.to_owned(),
                }
            };
            check(head(&saved) == head(&original), || {
                "saving changed the tiles or tilesets".into()
            })?;
            check(!editor.doc.as_ref().is_some_and(Document::dirty), || {
                "the map is still dirty after saving".into()
            })?;
            demo.wait = 3;
        }
        // Play.
        6 => {
            demo.entities = play::entities(world).len();
            demo.fixed_steps = world.resource::<Time>().fixed_step_count();
            play::start(editor, world);
            check(editor.mode == Mode::Playing, || "Play did not start".into())?;
            demo.wait = PLAY_FRAMES;
        }
        // Stop.
        7 => {
            let steps = world.resource::<Time>().fixed_step_count() - demo.fixed_steps;
            check(steps > 0, || "no fixed step ran while playing".into())?;
            log::info!(
                "editor demo: played {steps} fixed steps with {} entities",
                play::entities(world).len()
            );
            play::stop(editor, world);
        }
        // Back to editing, with the scene as it was before Play.
        8 => {
            if editor.mode != Mode::Editing {
                return Ok(false);
            }
            // Give the tilemap a frame or two to spawn its objects again.
            demo.wait = 3;
        }
        9 => {
            let entities = play::entities(world).len();
            check(entities == demo.entities, || {
                format!(
                    "{entities} entities after Stop, {} before Play",
                    demo.entities
                )
            })?;
            check(!editor.doc.as_ref().is_some_and(Document::dirty), || {
                "Play changed the document".into()
            })?;
            check(objects_now(editor) == demo.objects + 1, || {
                "the placed object is gone".into()
            })?;
            return Ok(true);
        }
        _ => return Ok(true),
    }
    demo.step += 1;
    Ok(false)
}

/// Where the object's spawned entity is (x), in the world.
fn object_entity_x(editor: &Editor, world: &mut World, id: u32) -> Option<f32> {
    let scene = editor.scene?;
    let mut objects =
        world.query::<(&cashgear_engine::tiled::TiledObject, &ChildOf, &Transform2D)>();
    objects
        .iter(world)
        .find(|(o, parent, _)| o.id == id && parent.parent() == scene)
        .map(|(.., t)| t.translation.x)
}

/// Where the document says the object's entity should be (x).
fn world_x(editor: &Editor, world: &World, id: u32) -> Option<f32> {
    let doc = editor.map()?;
    let maps = world.resource::<Assets<TiledMap>>();
    let object = crate::scene::engine_object(doc, maps, id)?;
    let map = maps.get(&doc.handle)?;
    Some(crate::doc::placed(map, &object).centre.x)
}
