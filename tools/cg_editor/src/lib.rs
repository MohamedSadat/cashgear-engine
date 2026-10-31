//! The CashGear scene editor: a desktop tool for placing and tuning a level's entities
//! without code, drawn with the engine's own renderer and egui panels around it.
//!
//! It edits two kinds of documents:
//!
//! - **Tiled maps** (`.tmx`): the object layers. Place prefabs, classed tiles and shapes;
//!   move, rotate and resize them with gizmos and grid snapping; edit names, classes and
//!   custom properties (properties named after a registered component are edited field by
//!   field). Saving writes the object layers back into the file in place and leaves the
//!   rest as Tiled wrote it, so Tiled stays the tilemap editor: save the tiles there and
//!   the editor shows them at once.
//! - **Prefabs** (`.prefab.ron`): the component tree, with children dragged between
//!   parents and every registered component edited through the same serde data the file
//!   holds.
//!
//! Every change can be undone. **Play** runs the scene as the game would and **Stop**
//! puts the edited scene back as it was before Play.
//!
//! Play runs game code, so a game builds the editor in: it adds [`EditorPlugin`] after the
//! engine's plugins and before its own, and reacts to [`PlayStarted`] and [`PlayStopped`]
//! (see [`EditMode`]). The platformer's editor is `cargo run -p platformer_editor`. The
//! `cg_editor` binary runs alone for any asset folder; there Play runs the engine's own
//! systems on the scene: physics, animation and Lua scripts.
//!
//! A game's editor build, as the game template's `editor` crate has it:
//!
//! ```no_run
//! use cashgear_engine::prelude::*;
//! use cg_editor::{EditorPlugin, PlayMode};
//!
//! # struct Game;
//! # impl Plugin for Game { fn build(&self, _: &mut App) {} }
//! fn main() -> AppExit {
//!     let editor = EditorPlugin::new(PlayMode::Game)
//!         .with_args(&Args::from_env())
//!         .open("maps/level1.tmx");
//!     App::new()
//!         .add_plugins(DefaultPlugins::default())
//!         .add_plugins(editor)
//!         .add_plugins(Game)
//!         .run()
//! }
//! ```
//!
//! | Key | Does |
//! | --- | --- |
//! | W, E, R | Move, rotate, resize tool |
//! | Middle or right drag, Space + drag | Pan |
//! | Wheel | Zoom at the pointer |
//! | Click, Shift + click, drag on empty space | Select, add to the selection, select in a box |
//! | Arrows | Nudge by a grid step (Shift: one pixel) |
//! | Ctrl (held) | Snap to the grid or not, whichever is off |
//! | Delete, Ctrl+D | Delete, duplicate |
//! | Ctrl+Z, Ctrl+Y (Ctrl+Shift+Z) | Undo, redo |
//! | Ctrl+S | Save |
//! | F5, Shift+F5 | Play, stop |
//! | F | Frame the selection (or the whole scene) |
//! | Escape | Stop placing; clear the selection |

#![allow(clippy::disallowed_methods)] // A desktop tool: files are read and written directly.

mod demo;
pub mod doc;
mod inspector;
mod panels;
mod play;
pub mod scene;
#[cfg(test)]
mod tests;
pub mod tmx;
pub mod value;
mod view;

pub use cashgear_engine::world::{EditMode, PlayStarted, PlayStopped};

use crate::doc::{Document, History, MapDoc, PrefabDoc, Selected};
use crate::scene::EditorScene;
use cashgear_engine::assets::PrefabFiles;
use cashgear_engine::core::ecs::error::{FallbackErrorHandler, warn};
use cashgear_engine::core::ecs::message::MessageCursor;
use cashgear_engine::platform::{LogCapture, LogLine};
use cashgear_engine::prelude::*;
use cashgear_engine::ui::{DebugOverlay, EguiContext, egui};
use cashgear_engine::world::PrefabRegistry;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

/// How Play runs the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayMode {
    /// A game is built in: Play sends [`PlayStarted`] and the game starts its level from
    /// the edited map (the editor's own copy of the scene is hidden meanwhile); Stop sends
    /// [`PlayStopped`].
    Game,
    /// No game code: the edited scene itself runs, with the engine's systems (physics,
    /// animation, Lua scripts).
    Scene,
}

/// The editor. Add it after the engine's plugins (it needs the debug overlay's egui) and
/// before the game's, so the game sees [`EditMode`] while it builds:
///
/// ```no_run
/// use cashgear_engine::prelude::*;
/// use cg_editor::{EditorPlugin, PlayMode};
///
/// App::new()
///     .add_plugins(DefaultPlugins::default())
///     .add_plugins(EditorPlugin::new(PlayMode::Game).open("maps/level1.tmx"))
///     // .add_plugins(MyGame)
///     .run();
/// ```
#[derive(Debug, Clone)]
pub struct EditorPlugin {
    /// How Play runs the scene.
    pub play: PlayMode,
    /// The document to open first (an asset path: a `.tmx` or a `.prefab.ron`).
    pub open: Option<String>,
    /// Save into this folder instead of the asset folder (the demo saves into a scratch
    /// folder so it never changes the game's files).
    pub save_dir: Option<PathBuf>,
    /// Run the scripted demo: open, edit, undo, save, play and stop, then exit (with an
    /// error if any step fails). CI runs it.
    pub demo: bool,
    /// Load every `.prefab.ron` file in the asset folder that the game has not, named
    /// after the file in PascalCase (`checkpoint_sparks.prefab.ron` is `CheckpointSparks`),
    /// so map objects of those classes spawn as their prefabs. The standalone editor does;
    /// a game that builds the editor in loads its own.
    pub load_prefabs: bool,
}

impl EditorPlugin {
    /// An editor whose Play works as `play` says.
    pub fn new(play: PlayMode) -> Self {
        Self {
            play,
            open: None,
            save_dir: None,
            demo: false,
            load_prefabs: false,
        }
    }

    /// Opens `path` (an asset path) at start.
    pub fn open(mut self, path: impl Into<String>) -> Self {
        self.open = Some(path.into());
        self
    }

    /// Reads the editor's flags: the first argument ending in `.tmx` or `.ron` is the
    /// document to open; `--demo` runs the demo, `--save-dir <folder>` saves elsewhere.
    pub fn with_args(mut self, args: &Args) -> Self {
        if let Some(path) = args
            .iter()
            .find(|arg| !arg.starts_with("--") && (arg.ends_with(".tmx") || arg.ends_with(".ron")))
        {
            self.open = Some(path.replace('\\', "/"));
        }
        self.demo |= args.flag("demo");
        if let Some(dir) = args.value("save-dir") {
            self.save_dir = Some(PathBuf::from(dir));
        }
        self
    }
}

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        LogCapture::start(CONSOLE_LINES);
        if self.load_prefabs {
            load_prefabs(app);
        }
        app.insert_resource(EditMode::Editing)
            .insert_resource(Editor::new(self))
            // A mistake in game code while playing is logged in the console instead of
            // closing the editor with unsaved work.
            .insert_resource(FallbackErrorHandler(warn))
            .add_systems(Startup, setup)
            .add_systems(First, play::sweep)
            .add_systems(Update, frame);
        if let Some(mut overlay) = app.world_mut().get_resource_mut::<DebugOverlay>() {
            overlay.open = true;
            overlay.key = None;
            overlay.windows = false;
        } else {
            log::error!("the editor needs the debug overlay (the `debug-overlay` feature)");
        }
    }
}

/// Loads the prefab files of the asset folder that no prefab name points at yet.
fn load_prefabs(app: &mut App) {
    let Some(server) = app.world().get_resource::<AssetServer>() else {
        return;
    };
    let root = PathBuf::from(server.io().root());
    let mut files = Vec::new();
    find_prefabs(&root, &root, &mut files);
    files.sort();
    for path in files {
        let stem = path
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .trim_end_matches(".prefab.ron");
        let name: String = stem
            .split(['_', '-', ' '])
            .map(|word| {
                let mut chars = word.chars();
                chars
                    .next()
                    .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                    .unwrap_or_default()
            })
            .collect();
        let taken = app
            .world()
            .get_resource::<PrefabFiles>()
            .is_some_and(|files| files.get(&name).is_some());
        if !name.is_empty() && !taken {
            log::info!("editor: prefab `{name}` from {path}");
            app.load_prefab(&name, &path);
        }
    }
}

fn find_prefabs(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_prefabs(root, &path, out);
        } else if path.to_string_lossy().ends_with(".prefab.ron")
            && let Ok(relative) = path.strip_prefix(root)
        {
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

/// Lines the console keeps.
const CONSOLE_LINES: usize = 5000;

/// The tools of the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tool {
    Move,
    Rotate,
    Scale,
}

/// What clicking in the viewport places.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Placement {
    /// A tile object of the map's tileset `tileset`, tile `tile`.
    Tile { tileset: usize, tile: u32 },
    /// A shape with a class (a prefab's name, or one the game knows).
    Shape { kind: ShapeKind, class: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShapeKind {
    Point,
    Rect,
    Ellipse,
}

/// The grid and snapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Grid {
    pub show: bool,
    pub snap: bool,
    /// The step in world units; 0 means the map's tile size.
    pub step: f32,
    /// The rotation step in degrees.
    pub angle: f32,
}

/// Where the viewport looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ViewState {
    pub centre: Vec2,
    pub zoom: f32,
    /// Framed on the document once it loaded.
    pub framed: bool,
}

/// Play's progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Editing,
    Playing,
    /// Stop was pressed; the game is given this many frames to leave its level first.
    Stopping(u8),
}

/// A question the editor waits on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Prompt {
    /// The document has unsaved changes; then do this.
    Unsaved(Then),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Then {
    Open(String),
    Quit,
}

/// The editor's state.
#[derive(Resource)]
pub struct Editor {
    pub(crate) play_mode: PlayMode,
    pub(crate) root: PathBuf,
    pub(crate) save_dir: Option<PathBuf>,
    pub(crate) doc: Option<Document>,
    pub(crate) history: History,
    pub(crate) selection: Vec<Selected>,
    pub(crate) tool: Tool,
    pub(crate) placing: Option<Placement>,
    pub(crate) grid: Grid,
    pub(crate) view: ViewState,
    pub(crate) mode: Mode,
    pub(crate) camera: Option<Entity>,
    pub(crate) game_camera: Option<(Camera2D, Transform2D)>,
    pub(crate) before_play: HashSet<Entity>,
    pub(crate) scene: Option<Entity>,
    pub(crate) scene_dirty: bool,
    pub(crate) console: VecDeque<LogLine>,
    pub(crate) console_filter: String,
    pub(crate) console_levels: [bool; 5],
    pub(crate) panels: panels::Panels,
    pub(crate) drag: Option<view::Drag>,
    pub(crate) prompt: Option<Prompt>,
    pub(crate) pending_open: Option<String>,
    pub(crate) thumbnails: HashMap<String, panels::Thumbnail>,
    pub(crate) demo: Option<demo::Demo>,
    /// The viewport, in logical pixels.
    pub(crate) viewport: egui::Rect,
    pub(crate) status: String,
    pub(crate) inspector: inspector::InspectorState,
    lifecycle: MessageCursor<Lifecycle>,
    pub(crate) play_requested: bool,
    pub(crate) stop_requested: bool,
}

impl Editor {
    fn new(plugin: &EditorPlugin) -> Self {
        Self {
            play_mode: plugin.play,
            root: PathBuf::new(),
            save_dir: plugin.save_dir.clone(),
            doc: None,
            history: History::default(),
            selection: Vec::new(),
            tool: Tool::Move,
            placing: None,
            grid: Grid {
                show: true,
                snap: true,
                step: 0.0,
                angle: 15.0,
            },
            view: ViewState {
                centre: Vec2::ZERO,
                zoom: 2.0,
                framed: false,
            },
            mode: Mode::Editing,
            camera: None,
            game_camera: None,
            before_play: HashSet::new(),
            scene: None,
            scene_dirty: false,
            console: VecDeque::new(),
            console_filter: String::new(),
            console_levels: [true, true, true, false, false],
            panels: panels::Panels::default(),
            drag: None,
            prompt: None,
            pending_open: plugin.open.clone(),
            thumbnails: HashMap::new(),
            demo: plugin.demo.then(demo::Demo::default),
            viewport: egui::Rect::NOTHING,
            status: String::new(),
            inspector: inspector::InspectorState::default(),
            lifecycle: MessageCursor::default(),
            play_requested: false,
            stop_requested: false,
        }
    }

    /// The open document.
    pub fn document(&self) -> Option<&Document> {
        self.doc.as_ref()
    }

    /// What is selected.
    pub fn selection(&self) -> &[Selected] {
        &self.selection
    }

    /// The undo history.
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Whether Play is running (or stopping).
    pub fn playing(&self) -> bool {
        self.mode != Mode::Editing
    }

    /// Selects the map object with this id (as clicking it would).
    pub fn select_object(&mut self, id: u32) {
        self.selection = vec![Selected::Object(id)];
    }

    /// Shows the palette of things to place in the left panel.
    pub fn show_palette(&mut self) {
        self.panels.left = panels::LeftTab::Place;
    }

    /// Opens a map or a prefab by asset path (in the next frame), asking first if the open
    /// document has unsaved changes.
    pub fn request_open(&mut self, path: &str) {
        request_open(self, path);
    }

    /// Selects the prefab node at `path` (child indices from the root).
    pub fn select_node(&mut self, path: &[usize]) {
        self.selection = vec![Selected::Node(path.to_vec())];
    }

    /// Presses Play (in the next frame).
    pub fn request_play(&mut self) {
        self.play_requested = true;
    }

    /// Presses Stop (in the next frame).
    pub fn request_stop(&mut self) {
        self.stop_requested = true;
    }

    /// The open map, if a map is open.
    pub(crate) fn map(&self) -> Option<&MapDoc> {
        match &self.doc {
            Some(Document::Map(map)) => Some(map),
            _ => None,
        }
    }

    /// The open prefab, if a prefab is open.
    pub(crate) fn prefab(&self) -> Option<&PrefabDoc> {
        match &self.doc {
            Some(Document::Prefab(prefab)) => Some(prefab),
            _ => None,
        }
    }

    /// Changes the document as one undo step called `label` (see [`History::record`]),
    /// unless Play runs. Returns what `change` returns, if there is a document.
    pub(crate) fn edit<R>(
        &mut self,
        label: &str,
        gesture: Option<u64>,
        change: impl FnOnce(&mut Document, &mut Vec<Selected>) -> R,
    ) -> Option<R> {
        if self.playing() {
            return None;
        }
        let doc = self.doc.as_mut()?;
        self.history.record(label, gesture, doc, &self.selection);
        let result = change(doc, &mut self.selection);
        self.scene_dirty = true;
        Some(result)
    }

    /// Undoes the latest change.
    pub(crate) fn undo(&mut self) {
        if self.playing() {
            return;
        }
        if let Some(doc) = self.doc.as_mut()
            && let Some(label) = self.history.undo(doc, &mut self.selection)
        {
            self.scene_dirty = true;
            self.status = format!("Undid {label}");
        }
    }

    /// Redoes the latest undone change.
    pub(crate) fn redo(&mut self) {
        if self.playing() {
            return;
        }
        if let Some(doc) = self.doc.as_mut()
            && let Some(label) = self.history.redo(doc, &mut self.selection)
        {
            self.scene_dirty = true;
            self.status = format!("Redid {label}");
        }
    }

    /// The file an asset path is saved to.
    pub(crate) fn save_path(&self, asset: &str) -> PathBuf {
        self.save_dir.as_ref().unwrap_or(&self.root).join(asset)
    }

    /// The file an asset path is read from: the save folder's copy if the demo saved one.
    pub(crate) fn read_path(&self, asset: &str) -> PathBuf {
        let saved = self.save_path(asset);
        if self.save_dir.is_some() && saved.exists() {
            saved
        } else {
            self.root.join(asset)
        }
    }

    /// The grid step in world units.
    pub(crate) fn grid_step(&self, world: &World) -> f32 {
        if self.grid.step > 0.0 {
            return self.grid.step;
        }
        self.map()
            .and_then(|doc| world.resource::<Assets<TiledMap>>().get(&doc.handle))
            .map_or(16.0, |map| map.tile_size.x.max(1) as f32)
    }
}

fn setup(world: &mut World) {
    let root = world.resource::<AssetServer>().io().root().to_owned();
    world.resource_mut::<Window>().set_close_guard(true);
    let mut editor = world.resource_mut::<Editor>();
    editor.root = PathBuf::from(root);
    log::info!("editor: assets in {}", editor.root.display());
}

/// One frame of the editor: its panels and viewport, then the document shown in the
/// world, the camera, and Play.
fn frame(world: &mut World) {
    let Some(ctx) = world
        .get_resource::<EguiContext>()
        .and_then(|egui| egui.get().cloned())
    else {
        return;
    };
    world.resource_scope::<Editor, _>(|world, mut editor| {
        let editor = &mut *editor;
        editor.console.extend(LogCapture::take());
        while editor.console.len() > CONSOLE_LINES {
            editor.console.pop_front();
        }
        if let Some(path) = editor.pending_open.take() {
            open(editor, world, &path);
        }
        play::update(editor, world);
        watch_document(editor, world);
        panels::show(editor, world, &ctx);
        view::update(editor, world, &ctx);
        if editor.scene_dirty {
            editor.scene_dirty = false;
            sync_scene(editor, world);
        }
        view::camera(editor, world);
        if editor.drag.is_none() && !ctx.egui_is_using_pointer() && !ctx.egui_wants_keyboard_input()
        {
            editor.history.end_gesture();
        }
        demo::update(editor, world);
        close_requests(editor, world);
    });
}

/// Opens a document by asset path, unless the open one has unsaved changes (then asks).
pub(crate) fn request_open(editor: &mut Editor, path: &str) {
    if editor.playing() {
        return;
    }
    if editor.doc.as_ref().is_some_and(Document::dirty) {
        editor.prompt = Some(Prompt::Unsaved(Then::Open(path.to_owned())));
    } else {
        editor.pending_open = Some(path.to_owned());
    }
}

/// Opens a document by asset path now, dropping the open one.
pub(crate) fn open(editor: &mut Editor, world: &mut World, path: &str) {
    let file = editor.read_path(path);
    let source = match std::fs::read_to_string(&file) {
        Ok(source) => source,
        Err(err) => {
            log::error!("cannot open {}: {err}", file.display());
            return;
        }
    };
    let doc = if path.ends_with(".tmx") {
        match tmx::read_map(&source) {
            Ok(objects) => Document::Map(MapDoc {
                path: path.to_owned(),
                handle: world.resource::<AssetServer>().load(path),
                saved: objects.clone(),
                objects,
                templates: HashMap::new(),
                synced_version: None,
                conflict: false,
            }),
            Err(err) => {
                log::error!("cannot open {path}: {err}");
                return;
            }
        }
    } else if path.ends_with(".ron") {
        let read = scene::read_prefab(&source, world.resource::<PrefabRegistry>());
        match read {
            Ok(root) => Document::Prefab(PrefabDoc {
                path: path.to_owned(),
                header: scene::prefab_header(&source),
                saved: root.clone(),
                root,
            }),
            Err(err) => {
                log::error!("cannot open {path}: {err}");
                return;
            }
        }
    } else {
        log::warn!("the editor opens maps (.tmx) and prefabs (.prefab.ron), not {path}");
        return;
    };
    despawn_scene(editor, world);
    editor.doc = Some(doc);
    editor.history.clear();
    editor.selection.clear();
    editor.placing = None;
    editor.view.framed = false;
    editor.scene_dirty = true;
    editor.status = format!("Opened {path}");
    log::info!("editor: opened {path}");
}

/// Saves the open document. Returns whether it was saved.
pub(crate) fn save(editor: &mut Editor, world: &mut World) -> bool {
    let Some(doc) = editor.doc.as_ref() else {
        return false;
    };
    let path = doc.path().to_owned();
    let target = editor.save_path(&path);
    let text = match doc {
        Document::Map(map) => {
            // Saving splices into the file as it is now, so a change made in Tiled since
            // (to the tiles, say) is kept.
            let source = match std::fs::read_to_string(editor.read_path(&path)) {
                Ok(source) => source,
                Err(err) => {
                    log::error!("cannot save {path}: {err}");
                    return false;
                }
            };
            match tmx::write_map(&source, &map.objects) {
                Ok(text) => text,
                Err(err) => {
                    log::error!("cannot save {path}: {err}");
                    return false;
                }
            }
        }
        Document::Prefab(prefab) => {
            let text = prefab.root.to_ron(&prefab.header);
            if let Err(err) = Prefab::from_ron(&text, world.resource::<PrefabRegistry>()) {
                log::error!("cannot save {path}: the prefab does not read back: {err}");
                return false;
            }
            text
        }
    };
    if let Some(parent) = target.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        log::error!("cannot save {}: {err}", target.display());
        return false;
    }
    if let Err(err) = std::fs::write(&target, text) {
        log::error!("cannot save {}: {err}", target.display());
        return false;
    }
    match editor.doc.as_mut() {
        Some(Document::Map(map)) => {
            map.saved = map.objects.clone();
            map.conflict = false;
        }
        Some(Document::Prefab(prefab)) => {
            prefab.saved = prefab.root.clone();
            // Prefabs the game loaded from this file use the new version from now on.
            if editor.save_dir.is_none() {
                world.resource::<AssetServer>().reload(&path);
            }
        }
        None => {}
    }
    editor.status = format!("Saved {path}");
    log::info!("editor: saved {}", target.display());
    true
}

/// Notices the open map changing on disk (saved in Tiled, say) and follows it.
fn watch_document(editor: &mut Editor, world: &mut World) {
    if editor.playing() {
        return;
    }
    let root = editor.read_path(editor.doc.as_ref().map_or("", Document::path));
    let Some(Document::Map(doc)) = editor.doc.as_mut() else {
        return;
    };
    let maps = world.resource::<Assets<TiledMap>>();
    let Some(version) = maps.version(&doc.handle) else {
        return;
    };
    let Some(synced) = doc.synced_version else {
        // The map just loaded: it is the file, as the document is.
        scene::capture_templates(doc, maps);
        doc.synced_version = Some(version);
        return;
    };
    if version == synced {
        return;
    }
    // The asset changed, and not by the editor: the file was saved by someone else.
    let on_disk = std::fs::read_to_string(&root)
        .map_err(|err| err.to_string())
        .and_then(|text| tmx::read_map(&text));
    let on_disk = match on_disk {
        Ok(objects) => objects,
        Err(err) => {
            log::warn!("{} changed on disk but does not read: {err}", doc.path);
            doc.synced_version = Some(version);
            return;
        }
    };
    scene::capture_templates(doc, world.resource::<Assets<TiledMap>>());
    if on_disk == doc.saved {
        // The tiles changed (or the editor's own save came back): keep the objects.
        doc.synced_version = Some(version);
        if doc.objects != doc.saved {
            editor.scene_dirty = true;
        }
        log::info!("{} changed on disk; its objects are as before", doc.path);
        return;
    }
    if doc.objects == doc.saved {
        log::info!("{} changed on disk; reloaded its objects", doc.path);
        doc.saved = on_disk.clone();
        let before = doc::Snapshot::Map(core::mem::replace(&mut doc.objects, on_disk));
        doc.synced_version = Some(version);
        editor
            .history
            .push("reload from disk", before, &editor.selection);
        editor.scene_dirty = true;
        editor.status = format!("Reloaded {} from disk", doc.path);
    } else {
        // Both changed: ask, and show the editor's objects meanwhile.
        log::warn!(
            "{} changed on disk while it has unsaved changes here",
            doc.path
        );
        doc.saved = on_disk;
        doc.conflict = true;
        editor.scene_dirty = true;
    }
}

/// Shows the document in the world again after a change.
fn sync_scene(editor: &mut Editor, world: &mut World) {
    if editor.playing() && editor.play_mode == PlayMode::Game {
        return;
    }
    match editor.doc.as_mut() {
        Some(Document::Map(doc)) => {
            if editor.scene.is_none_or(|e| world.get_entity(e).is_err()) {
                let name = format!("editor: {}", doc.path);
                editor.scene = Some(
                    world
                        .spawn((
                            Name::new(name),
                            Tilemap::new(doc.handle.clone()),
                            EditorScene,
                        ))
                        .id(),
                );
            }
            if doc.synced_version.is_some() {
                let mut maps = world.resource_mut::<Assets<TiledMap>>();
                scene::apply_map(doc, &mut maps);
            } else {
                // Not loaded yet: try again next frame.
                editor.scene_dirty = true;
            }
        }
        Some(Document::Prefab(doc)) => {
            let root = doc.root.clone();
            despawn_scene(editor, world);
            match scene::spawn_prefab(world, &root) {
                Ok(entity) => editor.scene = Some(entity),
                Err(err) => log::error!("prefab preview: {err}"),
            }
        }
        None => {}
    }
}

/// Despawns the editor's view of the document.
pub(crate) fn despawn_scene(editor: &mut Editor, world: &mut World) {
    if let Some(entity) = editor.scene.take()
        && let Ok(entity) = world.get_entity_mut(entity)
    {
        entity.despawn();
    }
}

/// Asks before closing the window with unsaved changes.
fn close_requests(editor: &mut Editor, world: &mut World) {
    let Some(messages) = world.get_resource::<Messages<Lifecycle>>() else {
        return;
    };
    let asked = editor
        .lifecycle
        .read(messages)
        .any(|m| *m == Lifecycle::CloseRequested);
    if !asked {
        return;
    }
    if editor.doc.as_ref().is_some_and(Document::dirty) {
        editor.prompt = Some(Prompt::Unsaved(Then::Quit));
    } else {
        world.write_message(AppExit::Success);
    }
}
