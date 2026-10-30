//! The viewport: the editor's camera, panning and zooming, picking, the move, rotate and
//! resize gizmos, placing, and what the editor draws over the scene (the grid, outlines,
//! labels and the selection).

use crate::doc::{Document, Placed, Selected, placed};
use crate::scene;
use crate::tmx::{ObjectData, Shape};
use crate::value::Value;
use crate::{Editor, Mode, Placement, ShapeKind, Tool};
use cashgear_engine::anim::CameraFollow;
use cashgear_engine::platform::input::{ButtonInput, KeyCode, MouseButton, PointerId, Pointers};
use cashgear_engine::prelude::*;
use cashgear_engine::tiled::MapObject;
use cashgear_engine::ui::egui;
use std::hash::{Hash, Hasher};

/// A drag in the viewport.
#[derive(Debug, Clone)]
pub(crate) struct Drag {
    kind: DragKind,
    /// Where it started, in logical pixels.
    from: Vec2,
    gesture: u64,
}

#[derive(Debug, Clone)]
enum DragKind {
    Pan {
        centre: Vec2,
    },
    Box,
    Move {
        axis: Axis,
        start: Vec<Start>,
    },
    Rotate {
        centre: Vec2,
        angle: f32,
        start: Vec<Start>,
    },
    Resize {
        handle: usize,
        start: Box<Start>,
        box_min: Vec2,
        box_max: Vec2,
    },
    Scale {
        centre: Vec2,
        distance: f32,
        start: Vec<Start>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Axis {
    Free,
    X,
    Y,
}

/// A selected thing as it was when a drag started.
#[derive(Debug, Clone)]
enum Start {
    Object {
        id: u32,
        data: ObjectData,
        placed: Placed,
    },
    Node {
        path: Vec<usize>,
        transform: Transform2D,
        /// The parent's world transform, to turn world moves into the node's own space.
        parent: Affine2,
        world: Vec2,
    },
}

/// The gizmo's size in logical pixels.
pub(crate) const GIZMO: f32 = 56.0;
/// How near the pointer must be to pick a line or a point, in logical pixels.
const PICK: f32 = 6.0;

/// The view the editor looks through while editing.
pub(crate) fn edit_view(editor: &Editor) -> CameraView {
    CameraView {
        camera: Camera2D {
            zoom: editor.view.zoom,
            scaling: ScalingMode::WindowSize,
            ..default()
        },
        position: editor.view.centre,
        rotation: 0.0,
    }
}

/// Converts between the world and the window's logical pixels.
struct Screen<'a> {
    view: CameraView,
    window: &'a Window,
}

impl Screen<'_> {
    fn to_screen(&self, world: Vec2) -> Vec2 {
        self.view.world_to_screen(world, self.window)
    }

    fn to_world(&self, screen: Vec2) -> Vec2 {
        self.view.screen_to_world(screen, self.window)
    }

    fn pos(&self, world: Vec2) -> egui::Pos2 {
        let p = self.to_screen(world);
        egui::pos2(p.x, p.y)
    }
}

/// Puts the camera where the editor looks while editing, and remembers how the game had
/// it so Play can give it back.
pub(crate) fn camera(editor: &mut Editor, world: &mut World) {
    if editor.mode != Mode::Editing {
        return;
    }
    let camera = editor
        .camera
        .filter(|&e| world.get::<Camera2D>(e).is_some())
        .or_else(|| {
            let mut cameras = world.query::<(Entity, &Camera2D)>();
            cameras
                .iter(world)
                .filter(|(_, c)| c.active)
                .map(|(e, _)| e)
                .min_by_key(|e| e.index_u32())
        });
    let camera = camera.unwrap_or_else(|| {
        world
            .spawn((
                Name::new("editor camera"),
                Camera2D::default(),
                Transform2D::default(),
            ))
            .id()
    });
    if editor.camera != Some(camera) {
        editor.camera = Some(camera);
        let settings = world.get::<Camera2D>(camera).cloned().unwrap_or_default();
        let transform = world
            .get::<Transform2D>(camera)
            .copied()
            .unwrap_or_default();
        editor.game_camera = Some((settings, transform));
    }
    let clear = editor
        .game_camera
        .as_ref()
        .map_or(Color::hex(0x1d1f24), |(c, _)| c.clear_color);
    let mut entity = world.entity_mut(camera);
    entity.remove::<CameraFollow>();
    if let Some(mut settings) = entity.get_mut::<Camera2D>() {
        let wanted = Camera2D {
            zoom: editor.view.zoom,
            scaling: ScalingMode::WindowSize,
            clear_color: clear,
            bar_color: Color::hex(0x101114),
            active: true,
            ..settings.clone()
        };
        if *settings != wanted {
            *settings = wanted;
        }
    }
    if let Some(mut transform) = entity.get_mut::<Transform2D>() {
        let wanted = Transform2D::from_xy(editor.view.centre.x, editor.view.centre.y);
        if transform.translation != wanted.translation || transform.rotation != 0.0 {
            *transform = wanted;
        }
    }
    if let Some(mut interpolated) = entity.get_mut::<Interpolated>() {
        interpolated.reset();
    }
}

/// The viewport's input and drawing for this frame.
pub(crate) fn update(editor: &mut Editor, world: &mut World, ctx: &egui::Context) {
    let window = world.resource::<Window>().clone();
    let screen = Screen {
        view: edit_view(editor),
        window: &window,
    };
    let painter = ctx
        .layer_painter(egui::LayerId::background())
        .with_clip_rect(editor.viewport);
    if editor.mode != Mode::Editing {
        painter.text(
            editor.viewport.left_top() + egui::vec2(10.0, 8.0),
            egui::Align2::LEFT_TOP,
            if editor.mode == Mode::Playing {
                "Playing"
            } else {
                "Stopping"
            },
            egui::FontId::proportional(14.0),
            egui::Color32::from_rgb(120, 230, 140),
        );
        return;
    }
    frame_document(editor, world);
    input(editor, world, ctx, &screen);
    draw(editor, world, &painter, &screen);
}

/// Frames a document once it has loaded.
fn frame_document(editor: &mut Editor, world: &World) {
    if editor.view.framed || editor.viewport.width() < 10.0 {
        return;
    }
    let bounds = match &editor.doc {
        Some(Document::Map(doc)) => {
            let Some(map) = world.resource::<Assets<TiledMap>>().get(&doc.handle) else {
                return;
            };
            map.bounds()
        }
        Some(Document::Prefab(_)) => Rect::from_center_size(Vec2::ZERO, vec2(160.0, 120.0)),
        None => return,
    };
    // A map wider than the view starts at its left end, two times, where levels start;
    // F frames the whole of it.
    let view = vec2(editor.viewport.width(), editor.viewport.height());
    let fit = (view * 0.9 / bounds.size().max(Vec2::ONE)).min_element();
    if editor.map().is_some() && fit < 1.0 {
        editor.view.zoom = 2.0;
        editor.view.centre = vec2(
            bounds.min.x + view.x / 4.0 - 8.0,
            bounds.center().y.min(bounds.min.y + view.y / 4.0 - 8.0),
        );
        editor.view.framed = true;
    } else {
        frame_rect(editor, bounds);
    }
}

/// Centres the view on `bounds`, zoomed to fit (by whole or quarter steps, for pixel art).
fn frame_rect(editor: &mut Editor, bounds: Rect) {
    let size = bounds.size().max(Vec2::splat(16.0));
    let fit = (vec2(editor.viewport.width(), editor.viewport.height()) * 0.9 / size).min_element();
    let zoom = if fit >= 1.0 {
        fit.floor()
    } else {
        (fit * 4.0).floor().max(1.0) / 4.0
    };
    editor.view.zoom = zoom.clamp(0.125, 16.0);
    editor.view.centre = bounds.center();
    editor.view.framed = true;
}

fn input(editor: &mut Editor, world: &mut World, ctx: &egui::Context, screen: &Screen) {
    let keys = world.resource::<ButtonInput<KeyCode>>().clone();
    let mouse = world.resource::<ButtonInput<MouseButton>>().clone();
    let pointers = world.resource::<Pointers>();
    let pointer = pointers.get(PointerId::Mouse).map(|p| p.position());
    let scroll = pointers.scroll();
    let ctrl = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let snap = editor.grid.snap != ctrl;

    if !ctx.egui_wants_keyboard_input() {
        shortcuts(editor, world, &keys, ctrl, shift);
    }

    let Some(at) = pointer else {
        return;
    };
    let over_view = editor.viewport.contains(egui::pos2(at.x, at.y)) && !ctx.is_pointer_over_egui();
    if over_view && scroll.y != 0.0 {
        let before = screen.to_world(at);
        let zoom = (editor.view.zoom * 1.2f32.powf(scroll.y)).clamp(0.125, 32.0);
        editor.view.centre = before - (before - editor.view.centre) * (editor.view.zoom / zoom);
        editor.view.zoom = zoom;
    }

    // A drag going on.
    if let Some(drag) = editor.drag.clone() {
        let held = mouse.pressed(MouseButton::Left)
            || mouse.pressed(MouseButton::Right)
            || mouse.pressed(MouseButton::Middle);
        if held {
            continue_drag(editor, world, &drag, at, snap, screen);
        } else {
            finish_drag(editor, world, &drag, at, shift, screen);
            editor.drag = None;
            editor.history.end_gesture();
        }
        return;
    }
    if !over_view {
        return;
    }
    let gesture = gesture_id(world);
    let pan = mouse.just_pressed(MouseButton::Middle)
        || mouse.just_pressed(MouseButton::Right)
        || (mouse.just_pressed(MouseButton::Left) && keys.pressed(KeyCode::Space));
    if pan {
        editor.drag = Some(Drag {
            kind: DragKind::Pan {
                centre: editor.view.centre,
            },
            from: at,
            gesture,
        });
        return;
    }
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    if let Some(placement) = editor.placing.clone() {
        place(editor, world, &placement, screen.to_world(at), snap);
        if !shift {
            editor.placing = None;
        }
        return;
    }
    // A gizmo handle first, then the thing under the pointer.
    if let Some(kind) = gizmo_hit(editor, world, at, screen) {
        editor.drag = Some(Drag {
            kind,
            from: at,
            gesture,
        });
        return;
    }
    let hit = pick(editor, world, at, screen);
    match hit {
        Some(hit) => {
            if shift {
                if let Some(i) = editor.selection.iter().position(|s| *s == hit) {
                    editor.selection.remove(i);
                } else {
                    editor.selection.push(hit);
                }
            } else if !editor.selection.contains(&hit) {
                editor.selection = vec![hit];
            }
            if editor
                .selection
                .iter()
                .any(|s| matches!(s, Selected::Object(_) | Selected::Node(_)))
            {
                let start = starts(editor, world);
                editor.drag = Some(Drag {
                    kind: DragKind::Move {
                        axis: Axis::Free,
                        start,
                    },
                    from: at,
                    gesture,
                });
            }
        }
        None => {
            if !shift {
                editor.selection.clear();
            }
            editor.drag = Some(Drag {
                kind: DragKind::Box,
                from: at,
                gesture,
            });
        }
    }
}

/// An id for an undo gesture: one per drag.
fn gesture_id(world: &World) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    world.resource::<Time>().frame_count().hash(&mut hasher);
    "viewport drag".hash(&mut hasher);
    hasher.finish()
}

fn shortcuts(
    editor: &mut Editor,
    world: &mut World,
    keys: &ButtonInput<KeyCode>,
    ctrl: bool,
    shift: bool,
) {
    let pressed = |key| keys.just_pressed(key);
    if ctrl && pressed(KeyCode::KeyZ) {
        if shift {
            editor.redo();
        } else {
            editor.undo();
        }
    } else if ctrl && pressed(KeyCode::KeyY) {
        editor.redo();
    } else if ctrl && pressed(KeyCode::KeyS) {
        crate::save(editor, world);
    } else if ctrl && pressed(KeyCode::KeyD) {
        duplicate(editor, world);
    } else if pressed(KeyCode::Delete) {
        delete(editor);
    } else if pressed(KeyCode::Escape) {
        if editor.placing.is_some() {
            editor.placing = None;
        } else {
            editor.selection.clear();
        }
    } else if !ctrl && pressed(KeyCode::KeyW) {
        editor.tool = Tool::Move;
    } else if !ctrl && pressed(KeyCode::KeyE) {
        editor.tool = Tool::Rotate;
    } else if !ctrl && pressed(KeyCode::KeyR) {
        editor.tool = Tool::Scale;
    } else if !ctrl && pressed(KeyCode::KeyF) {
        let bounds = selection_bounds(editor, world).or_else(|| match &editor.doc {
            Some(Document::Map(doc)) => world
                .resource::<Assets<TiledMap>>()
                .get(&doc.handle)
                .map(TiledMap::bounds),
            _ => None,
        });
        if let Some(bounds) = bounds {
            frame_rect(editor, bounds);
        }
    }
    let step = if shift { 1.0 } else { editor.grid_step(world) };
    let nudge = [
        (KeyCode::ArrowLeft, vec2(-step, 0.0)),
        (KeyCode::ArrowRight, vec2(step, 0.0)),
        (KeyCode::ArrowUp, vec2(0.0, step)),
        (KeyCode::ArrowDown, vec2(0.0, -step)),
    ]
    .into_iter()
    .filter(|(key, _)| !ctrl && pressed(*key))
    .map(|(_, by)| by)
    .sum::<Vec2>();
    if nudge != Vec2::ZERO && !editor.selection.is_empty() {
        self::nudge(editor, world, nudge);
    }
}

/// Moves the selection by `delta` world units as one undo step, without snapping (the
/// arrow keys do this).
pub(crate) fn nudge(editor: &mut Editor, world: &World, delta: Vec2) {
    let start = starts(editor, world);
    move_by(editor, world, &start, delta, false, None);
}

/// Deletes the selection.
pub(crate) fn delete(editor: &mut Editor) {
    if editor.selection.is_empty() {
        return;
    }
    let selection = editor.selection.clone();
    editor.edit("delete", None, |doc, selected| {
        match doc {
            Document::Map(map) => {
                let ids: Vec<u32> = selection
                    .iter()
                    .filter_map(|s| match s {
                        Selected::Object(id) => Some(*id),
                        Selected::Node(_) => None,
                    })
                    .collect();
                map.remove(&ids);
            }
            Document::Prefab(prefab) => {
                // Deepest and last first, so earlier paths stay valid.
                let mut paths: Vec<&Vec<usize>> = selection
                    .iter()
                    .filter_map(|s| match s {
                        Selected::Node(path) if !path.is_empty() => Some(path),
                        _ => None,
                    })
                    .collect();
                paths.sort();
                for path in paths.into_iter().rev() {
                    let (parent, index) = path.split_at(path.len() - 1);
                    if let Some(parent) = prefab.root.at_mut(parent)
                        && index[0] < parent.children.len()
                    {
                        parent.children.remove(index[0]);
                    }
                }
            }
        }
        selected.clear();
    });
}

/// Duplicates the selection, one grid step to the right.
pub(crate) fn duplicate(editor: &mut Editor, world: &World) {
    if editor.selection.is_empty() {
        return;
    }
    let step = editor.grid_step(world);
    let selection = editor.selection.clone();
    editor.edit("duplicate", None, |doc, selected| match doc {
        Document::Map(map) => {
            let mut copies = Vec::new();
            for s in &selection {
                if let Selected::Object(id) = s
                    && let Some((layer, object)) = map.find(*id)
                {
                    copies.push((layer, object.clone()));
                }
            }
            selected.clear();
            for (layer, mut copy) in copies {
                copy.id = map.next_id();
                copy.x += step;
                selected.push(Selected::Object(copy.id));
                map.objects.layers[layer].objects.push(copy);
            }
        }
        Document::Prefab(prefab) => {
            selected.clear();
            for s in &selection {
                if let Selected::Node(path) = s
                    && let Some((index, parent_path)) = path.split_last()
                    && let Some(parent) = prefab.root.at_mut(parent_path)
                    && let Some(node) = parent.children.get(*index).cloned()
                {
                    parent.children.push(node);
                    let mut copy = parent_path.to_vec();
                    copy.push(parent.children.len() - 1);
                    selected.push(Selected::Node(copy));
                }
            }
        }
    });
}

/// Places `placement` at `at` (world units) in the layer of the selection (or the first
/// object layer) and selects it.
pub(crate) fn place(
    editor: &mut Editor,
    world: &World,
    placement: &Placement,
    at: Vec2,
    snap: bool,
) {
    let step = editor.grid_step(world);
    let Some(Document::Map(doc)) = &editor.doc else {
        return;
    };
    let Some(map) = world.resource::<Assets<TiledMap>>().get(&doc.handle) else {
        return;
    };
    let layer = editor
        .selection
        .iter()
        .find_map(|s| match s {
            Selected::Object(id) => doc.find(*id).map(|(layer, _)| layer),
            Selected::Node(_) => None,
        })
        .unwrap_or(0);
    let Some(layer_id) = doc.objects.layers.get(layer).map(|l| l.id) else {
        log::warn!(
            "{} has no object layer to place into; add one in Tiled",
            doc.path
        );
        return;
    };
    let offset = scene::layer_offset(map, layer_id);
    let snap_to = |v: Vec2| {
        if snap {
            (v / step).round() * step
        } else {
            v.round()
        }
    };
    let mut object = ObjectData::new(0, Shape::Rect);
    match placement {
        Placement::Tile { tileset, tile } => {
            let Some(set) = map.tilesets.get(*tileset) else {
                return;
            };
            let size = set.tile_size.as_vec2();
            // The tile's bottom-left corner on the grid, the tile around the pointer.
            let corner = snap_to(at - size / 2.0);
            let tiled = map.to_tiled(corner) - offset;
            object.gid = set.first_gid + tile;
            (object.x, object.y, object.width, object.height) = (tiled.x, tiled.y, size.x, size.y);
        }
        Placement::Shape { kind, class } => {
            object.class.clone_from(class);
            match kind {
                ShapeKind::Point => {
                    object.shape = Shape::Point;
                    let tiled = map.to_tiled(snap_to(at)) - offset;
                    (object.x, object.y) = (tiled.x, tiled.y);
                }
                ShapeKind::Rect | ShapeKind::Ellipse => {
                    if *kind == ShapeKind::Ellipse {
                        object.shape = Shape::Ellipse;
                    }
                    let size = Vec2::splat(step);
                    // Top-left corner in the world (y up).
                    let corner = snap_to(at + vec2(-size.x, size.y) / 2.0);
                    let tiled = map.to_tiled(corner) - offset;
                    (object.x, object.y, object.width, object.height) =
                        (tiled.x, tiled.y, size.x, size.y);
                }
            }
        }
    }
    editor.edit("place", None, |doc, selected| {
        if let Document::Map(map) = doc {
            object.id = map.next_id();
            selected.clear();
            selected.push(Selected::Object(object.id));
            map.objects.layers[layer].objects.push(object);
        }
    });
}

/// The selection as it is now, for a drag to work from.
fn starts(editor: &Editor, world: &World) -> Vec<Start> {
    let mut out = Vec::new();
    match &editor.doc {
        Some(Document::Map(doc)) => {
            let maps = world.resource::<Assets<TiledMap>>();
            let Some(map) = maps.get(&doc.handle) else {
                return out;
            };
            for s in &editor.selection {
                if let Selected::Object(id) = s
                    && let Some(object) = scene::engine_object(doc, maps, *id)
                    && let Some((_, data)) = doc.find(*id)
                {
                    out.push(Start::Object {
                        id: *id,
                        data: data.clone(),
                        placed: placed(map, &object),
                    });
                }
            }
        }
        Some(Document::Prefab(doc)) => {
            for s in &editor.selection {
                let Selected::Node(path) = s else {
                    continue;
                };
                let Some(node) = doc.root.at(path) else {
                    continue;
                };
                let transform = node
                    .component("Transform2D")
                    .and_then(|v| ron::from_str::<Transform2D>(&v.to_ron()).ok())
                    .unwrap_or_default();
                let entity = editor
                    .scene
                    .and_then(|root| scene::node_entity(world, root, path));
                let parent = entity
                    .and_then(|e| world.get::<ChildOf>(e))
                    .and_then(|c| world.get::<GlobalTransform2D>(c.parent()))
                    .map_or(Affine2::IDENTITY, GlobalTransform2D::affine);
                let world_at = entity
                    .and_then(|e| world.get::<GlobalTransform2D>(e))
                    .map_or(transform.translation, GlobalTransform2D::translation);
                out.push(Start::Node {
                    path: path.clone(),
                    transform,
                    parent,
                    world: world_at,
                });
            }
        }
        None => {}
    }
    out
}

fn continue_drag(
    editor: &mut Editor,
    world: &mut World,
    drag: &Drag,
    at: Vec2,
    snap: bool,
    screen: &Screen,
) {
    let delta_world = screen.to_world(at) - screen.to_world(drag.from);
    match &drag.kind {
        DragKind::Pan { centre } => {
            let moved = (at - drag.from) / editor.view.zoom;
            editor.view.centre = *centre - vec2(moved.x, -moved.y);
        }
        DragKind::Box => {}
        DragKind::Move { axis, start } => {
            if (at - drag.from).length() < 2.0 {
                return;
            }
            let delta = match axis {
                Axis::Free => delta_world,
                Axis::X => vec2(delta_world.x, 0.0),
                Axis::Y => vec2(0.0, delta_world.y),
            };
            move_by(editor, world, start, delta, snap, Some(drag.gesture));
        }
        DragKind::Rotate {
            centre,
            angle,
            start,
        } => {
            let now = (screen.to_world(at) - *centre).to_angle();
            let mut turn = (now - angle).to_degrees();
            if snap {
                turn = (turn / editor.grid.angle).round() * editor.grid.angle;
            }
            rotate_by(editor, start, *centre, turn, Some(drag.gesture));
        }
        DragKind::Resize {
            handle,
            start,
            box_min,
            box_max,
        } => {
            let step = editor.grid_step(world);
            let grab = Grab {
                handle: *handle,
                box_min: *box_min,
                box_max: *box_max,
                pointer: screen.to_world(at),
            };
            resize(editor, start, grab, snap.then_some(step), drag.gesture);
        }
        DragKind::Scale {
            centre,
            distance,
            start,
        } => {
            let now = (screen.to_world(at) - *centre).length();
            let mut factor = now / distance.max(1e-3);
            if snap {
                factor = (factor * 8.0).round() / 8.0;
            }
            scale_nodes(editor, start, factor.max(0.01), Some(drag.gesture));
        }
    }
}

fn finish_drag(
    editor: &mut Editor,
    world: &World,
    drag: &Drag,
    at: Vec2,
    shift: bool,
    screen: &Screen,
) {
    if let DragKind::Box = drag.kind {
        let rect = Rect::from_corners(screen.to_world(drag.from), screen.to_world(at));
        if rect.size().min_element() <= 0.0 {
            return;
        }
        if !shift {
            editor.selection.clear();
        }
        for hit in in_box(editor, world, rect) {
            if !editor.selection.contains(&hit) {
                editor.selection.push(hit);
            }
        }
    }
}

/// Moves the started selection by `delta` world units from where it started.
fn move_by(
    editor: &mut Editor,
    world: &World,
    start: &[Start],
    delta: Vec2,
    snap: bool,
    gesture: Option<u64>,
) {
    let step = editor.grid_step(world);
    editor.edit("move", gesture, |doc, _| match doc {
        Document::Map(doc) => {
            for s in start {
                let Start::Object {
                    id, data, placed, ..
                } = s
                else {
                    continue;
                };
                let mut anchor = placed.anchor + delta;
                if snap {
                    anchor = (anchor / step).round() * step;
                }
                let tiled = tiled_point(placed, data, anchor);
                if let Some(object) = doc.find_mut(*id) {
                    object.x = tiled.x;
                    object.y = tiled.y;
                }
            }
        }
        Document::Prefab(doc) => {
            for s in start {
                let Start::Node {
                    path,
                    transform,
                    parent,
                    world: at,
                } = s
                else {
                    continue;
                };
                let mut target = *at + delta;
                if snap {
                    target = (target / step).round() * step;
                }
                let local = parent.inverse().transform_vector2(target - *at);
                let moved = Transform2D {
                    translation: transform.translation + local,
                    ..*transform
                };
                set_node_transform(doc, path, moved);
            }
        }
    });
}

/// Turns the started selection by `turn` degrees counter-clockwise about `centre`.
fn rotate_by(editor: &mut Editor, start: &[Start], centre: Vec2, turn: f32, gesture: Option<u64>) {
    editor.edit("rotate", gesture, |doc, _| match doc {
        Document::Map(doc) => {
            for s in start {
                let Start::Object {
                    id, data, placed, ..
                } = s
                else {
                    continue;
                };
                // One object turns about its own centre, several about theirs together.
                let pivot = if start.len() == 1 {
                    placed.centre
                } else {
                    centre
                };
                let r = Vec2::from_angle(turn.to_radians());
                let anchor = pivot + r.rotate(placed.anchor - pivot);
                let tiled_anchor = tiled_point(placed, data, anchor);
                if let Some(object) = doc.find_mut(*id) {
                    object.rotation = normalize_degrees(data.rotation - turn);
                    object.x = tiled_anchor.x;
                    object.y = tiled_anchor.y;
                }
            }
        }
        Document::Prefab(doc) => {
            for s in start {
                let Start::Node {
                    path, transform, ..
                } = s
                else {
                    continue;
                };
                let turned = Transform2D {
                    rotation: transform.rotation + turn.to_radians(),
                    ..*transform
                };
                set_node_transform(doc, path, turned);
            }
        }
    });
}

/// The file's position (y down) for a world anchor (y up): the start's anchor is the
/// start's x and y.
fn tiled_point(placed: &Placed, data: &ObjectData, anchor: Vec2) -> Vec2 {
    let moved = anchor - placed.anchor;
    vec2(data.x + moved.x, data.y - moved.y)
}

fn normalize_degrees(degrees: f32) -> f32 {
    let d = degrees.rem_euclid(360.0);
    if (d - 360.0).abs() < 1e-3 {
        0.0
    } else {
        (d * 1000.0).round() / 1000.0
    }
}

/// A resize handle being dragged: corners 0 to 3 in outline order, then the middles of
/// the edges after each corner (4 to 7); the box it started as, in the object's own
/// frame; and where the pointer is in the world.
struct Grab {
    handle: usize,
    box_min: Vec2,
    box_max: Vec2,
    pointer: Vec2,
}

/// Resizes one box-shaped object by a handle, the opposite side staying put.
fn resize(editor: &mut Editor, start: &Start, grab: Grab, snap: Option<f32>, gesture: u64) {
    let Start::Object {
        id, data, placed, ..
    } = start
    else {
        return;
    };
    let Grab {
        handle,
        box_min,
        box_max,
        pointer,
    } = grab;
    let unturn = Vec2::from_angle(-placed.rotation);
    let mut local = unturn.rotate(pointer - placed.anchor);
    if let Some(step) = snap {
        local = (local / step).round() * step;
    }
    let (mut min, mut max) = (box_min, box_max);
    // Which bounds the handle moves: x (-1 min, 1 max, 0 none), then y.
    let corner = |i: usize| -> Vec2 {
        let p = placed.outline[i % 4];
        unturn.rotate(p - placed.anchor)
    };
    let moves = |p: Vec2| -> (i8, i8) {
        let x = if (p.x - box_min.x).abs() < 1e-3 {
            -1
        } else if (p.x - box_max.x).abs() < 1e-3 {
            1
        } else {
            0
        };
        let y = if (p.y - box_min.y).abs() < 1e-3 {
            -1
        } else if (p.y - box_max.y).abs() < 1e-3 {
            1
        } else {
            0
        };
        (x, y)
    };
    let (mx, my) = if handle < 4 {
        moves(corner(handle))
    } else {
        let a = moves(corner(handle - 4));
        let b = moves(corner(handle - 3));
        (
            if a.0 == b.0 { a.0 } else { 0 },
            if a.1 == b.1 { a.1 } else { 0 },
        )
    };
    match mx {
        -1 => min.x = local.x.min(max.x - 1.0),
        1 => max.x = local.x.max(min.x + 1.0),
        _ => {}
    }
    match my {
        -1 => min.y = local.y.min(max.y - 1.0),
        1 => max.y = local.y.max(min.y + 1.0),
        _ => {}
    }
    let size = max - min;
    // The anchor in the box: bottom-left for tile objects, top-left for the rest.
    let tile = data.gid != 0;
    let anchor_local = if tile { min } else { vec2(min.x, max.y) };
    let anchor = placed.anchor + Vec2::from_angle(placed.rotation).rotate(anchor_local);
    let tiled_anchor =
        vec2(data.x, data.y) + vec2(anchor.x - placed.anchor.x, placed.anchor.y - anchor.y);
    let id = *id;
    editor.edit("resize", Some(gesture), |doc, _| {
        if let Document::Map(doc) = doc
            && let Some(object) = doc.find_mut(id)
        {
            object.x = tiled_anchor.x;
            object.y = tiled_anchor.y;
            object.width = size.x;
            object.height = size.y;
        }
    });
}

fn scale_nodes(editor: &mut Editor, start: &[Start], factor: f32, gesture: Option<u64>) {
    editor.edit("scale", gesture, |doc, _| {
        if let Document::Prefab(doc) = doc {
            for s in start {
                if let Start::Node {
                    path, transform, ..
                } = s
                {
                    let scaled = Transform2D {
                        scale: transform.scale * factor,
                        ..*transform
                    };
                    set_node_transform(doc, path, scaled);
                }
            }
        }
    });
}

/// Writes a node's `Transform2D` component, adding one if it has none.
fn set_node_transform(doc: &mut crate::doc::PrefabDoc, path: &[usize], transform: Transform2D) {
    let Some(node) = doc.root.at_mut(path) else {
        return;
    };
    let Ok(value) = Value::from_serialize(&transform) else {
        return;
    };
    match node.components.iter_mut().find(|(n, _)| n == "Transform2D") {
        Some((_, old)) => *old = value,
        None => node.components.insert(0, ("Transform2D".into(), value)),
    }
}

/// The gizmo handle under the pointer, as the drag it starts.
fn gizmo_hit(editor: &Editor, world: &World, at: Vec2, screen: &Screen) -> Option<DragKind> {
    if editor.selection.is_empty() {
        return None;
    }
    let start = starts(editor, world);
    let centre = gizmo_centre(&start)?;
    let c = screen.to_screen(centre);
    match editor.tool {
        Tool::Move => {
            let axis = if (at - c).abs().max_element() <= 7.0 {
                Axis::Free
            } else if segment_distance(at, c, c + vec2(GIZMO, 0.0)) <= PICK {
                Axis::X
            } else if segment_distance(at, c, c - vec2(0.0, GIZMO)) <= PICK {
                Axis::Y
            } else {
                return None;
            };
            Some(DragKind::Move { axis, start })
        }
        Tool::Rotate => ((at - c).length() - GIZMO)
            .abs()
            .le(&PICK)
            .then(|| DragKind::Rotate {
                centre,
                angle: (screen.to_world(at) - centre).to_angle(),
                start,
            }),
        Tool::Scale => match start.as_slice() {
            [s @ Start::Object { placed, .. }] if placed.outline.len() == 4 && placed.closed => {
                let handle = handles(placed)
                    .iter()
                    .position(|h| (screen.to_screen(*h) - at).abs().max_element() <= 6.0)?;
                let unturn = Vec2::from_angle(-placed.rotation);
                let local: Vec<Vec2> = placed
                    .outline
                    .iter()
                    .map(|p| unturn.rotate(*p - placed.anchor))
                    .collect();
                let box_min = local.iter().copied().reduce(Vec2::min)?;
                let box_max = local.iter().copied().reduce(Vec2::max)?;
                Some(DragKind::Resize {
                    handle,
                    start: Box::new(s.clone()),
                    box_min,
                    box_max,
                })
            }
            nodes if nodes.iter().all(|s| matches!(s, Start::Node { .. })) => {
                let handle = c + vec2(GIZMO, -GIZMO) * 0.7;
                ((at - handle).abs().max_element() <= 7.0).then(|| DragKind::Scale {
                    centre,
                    distance: (screen.to_world(at) - centre).length(),
                    start,
                })
            }
            _ => None,
        },
    }
}

/// The resize handles of a box: its corners, then the middles of its edges.
fn handles(placed: &Placed) -> Vec<Vec2> {
    let o = &placed.outline;
    let mut out = o.clone();
    for i in 0..4 {
        out.push((o[i] + o[(i + 1) % 4]) / 2.0);
    }
    out
}

fn gizmo_centre(start: &[Start]) -> Option<Vec2> {
    let points: Vec<Vec2> = start
        .iter()
        .map(|s| match s {
            Start::Object { placed, .. } => placed.centre,
            Start::Node { world, .. } => *world,
        })
        .collect();
    if points.is_empty() {
        return None;
    }
    let min = points.iter().copied().reduce(Vec2::min)?;
    let max = points.iter().copied().reduce(Vec2::max)?;
    Some((min + max) / 2.0)
}

fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

/// Every map object in drawing order with its engine view and placement.
fn map_objects(editor: &Editor, world: &World) -> Vec<(u32, MapObject, Placed)> {
    let Some(doc) = editor.map() else {
        return Vec::new();
    };
    let maps = world.resource::<Assets<TiledMap>>();
    let Some(map) = maps.get(&doc.handle) else {
        return Vec::new();
    };
    doc.objects
        .layers
        .iter()
        .flat_map(|layer| {
            let offset = scene::layer_offset(map, layer.id);
            layer.objects.iter().map(move |o| (offset, o))
        })
        .map(|(offset, o)| {
            let object = scene::to_map_object(map, offset, o, doc.templates.get(&o.id), &doc.path);
            let at = placed(map, &object);
            (o.id, object, at)
        })
        .collect()
}

/// Prefab nodes with where their preview entities are and how big they look.
fn prefab_nodes(editor: &Editor, world: &World) -> Vec<(Vec<usize>, Vec<Vec2>, Vec2)> {
    let (Some(doc), Some(root)) = (editor.prefab(), editor.scene) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut stack = vec![Vec::new()];
    while let Some(path) = stack.pop() {
        let Some(node) = doc.root.at(&path) else {
            continue;
        };
        for i in (0..node.children.len()).rev() {
            let mut child = path.clone();
            child.push(i);
            stack.push(child);
        }
        let Some(entity) = scene::node_entity(world, root, &path) else {
            continue;
        };
        let Some(global) = world.get::<GlobalTransform2D>(entity) else {
            continue;
        };
        let affine = global.affine();
        let corners = world
            .get::<Sprite>(entity)
            .map(|sprite| {
                let size = sprite.size.unwrap_or(Vec2::splat(8.0));
                let min = -sprite.pivot * size;
                [
                    min,
                    min + vec2(size.x, 0.0),
                    min + size,
                    min + vec2(0.0, size.y),
                ]
                .map(|p| affine.transform_point2(p))
                .to_vec()
            })
            .unwrap_or_default();
        out.push((path, corners, affine.translation));
    }
    out
}

/// The thing under the pointer: the topmost object, or the deepest prefab node.
fn pick(editor: &Editor, world: &World, at: Vec2, screen: &Screen) -> Option<Selected> {
    let point = screen.to_world(at);
    if editor.map().is_some() {
        return map_objects(editor, world)
            .into_iter()
            .rev()
            .find(|(_, _, placed)| hits(placed, point, at, screen))
            .map(|(id, ..)| Selected::Object(id));
    }
    prefab_nodes(editor, world)
        .into_iter()
        .rev()
        .find(|(_, corners, origin)| {
            if corners.len() == 4 {
                inside(corners, point)
            } else {
                (screen.to_screen(*origin) - at).length() <= PICK + 2.0
            }
        })
        .map(|(path, ..)| Selected::Node(path))
}

fn hits(placed: &Placed, point: Vec2, at: Vec2, screen: &Screen) -> bool {
    match placed.outline.len() {
        0 => (screen.to_screen(placed.anchor) - at).length() <= PICK + 2.0,
        _ if placed.closed && inside(&placed.outline, point) => true,
        n => (0..n - usize::from(!placed.closed)).any(|i| {
            let a = screen.to_screen(placed.outline[i]);
            let b = screen.to_screen(placed.outline[(i + 1) % n]);
            segment_distance(at, a, b) <= PICK
        }),
    }
}

fn inside(polygon: &[Vec2], p: Vec2) -> bool {
    let mut inside = false;
    let n = polygon.len();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + n - 1) % n]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
    }
    inside
}

/// Things inside a world rectangle (by their centre).
fn in_box(editor: &Editor, world: &World, rect: Rect) -> Vec<Selected> {
    if editor.map().is_some() {
        return map_objects(editor, world)
            .into_iter()
            .filter(|(_, _, placed)| rect.contains(placed.centre))
            .map(|(id, ..)| Selected::Object(id))
            .collect();
    }
    prefab_nodes(editor, world)
        .into_iter()
        .filter(|(path, _, origin)| !path.is_empty() && rect.contains(*origin))
        .map(|(path, ..)| Selected::Node(path))
        .collect()
}

/// The world rectangle around the selection.
fn selection_bounds(editor: &Editor, world: &World) -> Option<Rect> {
    let start = starts(editor, world);
    let points: Vec<Vec2> = start
        .iter()
        .flat_map(|s| match s {
            Start::Object { placed, .. } if !placed.outline.is_empty() => placed.outline.clone(),
            Start::Object { placed, .. } => vec![placed.anchor],
            Start::Node { world, .. } => vec![*world],
        })
        .collect();
    let min = points.iter().copied().reduce(Vec2::min)?;
    let max = points.iter().copied().reduce(Vec2::max)?;
    Some(Rect::from_corners(
        min - Vec2::splat(24.0),
        max + Vec2::splat(24.0),
    ))
}

/// A colour per class, so objects of one kind look alike.
fn class_color(class: &str) -> egui::Color32 {
    if class.is_empty() {
        return egui::Color32::from_rgb(170, 180, 200);
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    class.hash(&mut hasher);
    let hue = (hasher.finish() % 360) as f32 / 360.0;
    let rgb = egui::ecolor::Hsva::new(hue, 0.55, 1.0, 1.0);
    egui::Color32::from(rgb)
}

fn draw(editor: &Editor, world: &World, painter: &egui::Painter, screen: &Screen) {
    let viewport = editor.viewport;
    if viewport.width() <= 0.0 {
        return;
    }
    // The grid, over the visible part of the world.
    let step = editor.grid_step(world);
    if editor.grid.show && step * editor.view.zoom >= 4.0 {
        let a = screen.to_world(vec2(viewport.min.x, viewport.max.y));
        let b = screen.to_world(vec2(viewport.max.x, viewport.min.y));
        // Dark and faint, so it shows over a pale sky and a dark background alike.
        let stroke = egui::Stroke::new(1.0, egui::Color32::from_black_alpha(28));
        let mut x = (a.x / step).floor() * step;
        while x <= b.x {
            painter.line_segment([screen.pos(vec2(x, a.y)), screen.pos(vec2(x, b.y))], stroke);
            x += step;
        }
        let mut y = (a.y / step).floor() * step;
        while y <= b.y {
            painter.line_segment([screen.pos(vec2(a.x, y)), screen.pos(vec2(b.x, y))], stroke);
            y += step;
        }
    }
    let selected = |s: &Selected| editor.selection.contains(s);
    let highlight = egui::Color32::from_rgb(255, 214, 64);
    if let Some(doc) = editor.map()
        && let Some(map) = world.resource::<Assets<TiledMap>>().get(&doc.handle)
    {
        let bounds = map.bounds();
        let r = egui::Rect::from_two_pos(screen.pos(bounds.min), screen.pos(bounds.max));
        painter.rect_stroke(
            r,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(120)),
            egui::StrokeKind::Outside,
        );
        for (id, object, at) in map_objects(editor, world) {
            let is_selected = selected(&Selected::Object(id));
            let tile = object.tile.is_some();
            let base = class_color(&object.class);
            let color = if is_selected {
                highlight
            } else if tile {
                base.gamma_multiply(0.35)
            } else {
                base
            };
            let stroke = egui::Stroke::new(if is_selected { 2.0 } else { 1.0 }, color);
            if at.outline.is_empty() {
                let p = screen.pos(at.anchor);
                let d = 5.0;
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        p + egui::vec2(0.0, -d),
                        p + egui::vec2(d, 0.0),
                        p + egui::vec2(0.0, d),
                        p + egui::vec2(-d, 0.0),
                    ],
                    color.gamma_multiply(0.6),
                    stroke,
                ));
            } else {
                let points: Vec<egui::Pos2> = at.outline.iter().map(|p| screen.pos(*p)).collect();
                if at.closed {
                    painter.add(egui::Shape::closed_line(points, stroke));
                } else {
                    painter.add(egui::Shape::line(points, stroke));
                }
            }
            if matches!(
                object.shape,
                cashgear_engine::tiled::ObjectShape::Ellipse(_)
            ) {
                let c = screen.pos(at.centre);
                let radius = at.size * editor.view.zoom / 2.0;
                painter.add(egui::Shape::ellipse_stroke(
                    c,
                    egui::vec2(radius.x, radius.y),
                    stroke,
                ));
            }
            // Labels for objects that have no picture.
            if !tile && editor.view.zoom >= 1.0 {
                let label = if object.name.is_empty() {
                    &object.class
                } else {
                    &object.name
                };
                if !label.is_empty() {
                    let p = at.outline.first().map_or(at.anchor, |p| *p);
                    painter.text(
                        screen.pos(p)
                            + egui::vec2(3.0, if at.outline.is_empty() { 6.0 } else { 2.0 }),
                        egui::Align2::LEFT_TOP,
                        label,
                        egui::FontId::proportional(11.0),
                        color,
                    );
                }
            }
        }
    }
    if editor.prefab().is_some() {
        for (path, corners, origin) in prefab_nodes(editor, world) {
            let is_selected = selected(&Selected::Node(path));
            let color = if is_selected {
                highlight
            } else {
                egui::Color32::from_white_alpha(60)
            };
            let stroke = egui::Stroke::new(if is_selected { 2.0 } else { 1.0 }, color);
            if corners.len() == 4 {
                let points: Vec<egui::Pos2> = corners.iter().map(|p| screen.pos(*p)).collect();
                painter.add(egui::Shape::closed_line(points, stroke));
            }
            painter.circle_stroke(screen.pos(origin), 3.0, stroke);
        }
        // The prefab's origin.
        let o = screen.pos(Vec2::ZERO);
        let axis = egui::Stroke::new(1.0, egui::Color32::from_white_alpha(50));
        painter.line_segment([o - egui::vec2(12.0, 0.0), o + egui::vec2(12.0, 0.0)], axis);
        painter.line_segment([o - egui::vec2(0.0, 12.0), o + egui::vec2(0.0, 12.0)], axis);
    }
    draw_gizmo(editor, world, painter, screen);
    // The box being dragged, and what placing would place.
    let pointer = world
        .resource::<Pointers>()
        .get(PointerId::Mouse)
        .map(|p| p.position());
    if let (Some(drag), Some(at)) = (&editor.drag, pointer)
        && matches!(drag.kind, DragKind::Box)
    {
        let r =
            egui::Rect::from_two_pos(egui::pos2(drag.from.x, drag.from.y), egui::pos2(at.x, at.y));
        painter.rect(
            r,
            0.0,
            egui::Color32::from_rgba_unmultiplied(255, 214, 64, 20),
            egui::Stroke::new(1.0, highlight),
            egui::StrokeKind::Inside,
        );
    }
    if let (Some(placement), Some(at)) = (&editor.placing, pointer) {
        let label = match placement {
            Placement::Tile { .. } => "place tile".to_owned(),
            Placement::Shape { class, .. } => format!("place {class}"),
        };
        painter.text(
            egui::pos2(at.x + 12.0, at.y + 12.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(12.0),
            highlight,
        );
    }
}

fn draw_gizmo(editor: &Editor, world: &World, painter: &egui::Painter, screen: &Screen) {
    let start = starts(editor, world);
    let Some(centre) = gizmo_centre(&start) else {
        return;
    };
    let c = screen.pos(centre);
    let red = egui::Color32::from_rgb(235, 80, 80);
    let green = egui::Color32::from_rgb(90, 210, 110);
    let yellow = egui::Color32::from_rgb(255, 214, 64);
    match editor.tool {
        Tool::Move => {
            let x = c + egui::vec2(GIZMO, 0.0);
            let y = c - egui::vec2(0.0, GIZMO);
            painter.arrow(c, x - c, egui::Stroke::new(2.0, red));
            painter.arrow(c, y - c, egui::Stroke::new(2.0, green));
            painter.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)),
                1.0,
                yellow.gamma_multiply(0.8),
            );
        }
        Tool::Rotate => {
            painter.circle_stroke(
                c,
                GIZMO,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(110, 160, 255)),
            );
            painter.circle_filled(c, 3.0, yellow);
        }
        Tool::Scale => match start.as_slice() {
            [Start::Object { placed, .. }] if placed.outline.len() == 4 && placed.closed => {
                for h in handles(placed) {
                    painter.rect_filled(
                        egui::Rect::from_center_size(screen.pos(h), egui::vec2(8.0, 8.0)),
                        1.0,
                        yellow,
                    );
                }
            }
            nodes if !nodes.is_empty() && nodes.iter().all(|s| matches!(s, Start::Node { .. })) => {
                let h = c + egui::vec2(GIZMO, -GIZMO) * 0.7;
                painter.line_segment([c, h], egui::Stroke::new(2.0, yellow));
                painter.rect_filled(
                    egui::Rect::from_center_size(h, egui::vec2(10.0, 10.0)),
                    1.0,
                    yellow,
                );
            }
            _ => {}
        },
    }
}
