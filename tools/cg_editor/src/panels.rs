//! The panels around the viewport: the menu bar and toolbar, the hierarchy, the palette of
//! things to place, the inspector (in `inspector`), the asset browser and the console.

use crate::doc::{Document, Selected};
use crate::{Editor, Mode, Placement, Prompt, ShapeKind, Then, Tool, inspector, play, view};
use cashgear_engine::prelude::*;
use cashgear_engine::ui::{DebugOverlay, egui};
use std::path::PathBuf;

/// Which tab a panel shows, and the asset browser's place.
#[derive(Default)]
pub(crate) struct Panels {
    pub left: LeftTab,
    pub bottom: BottomTab,
    /// The asset browser's folder, an asset path ("" for the root).
    pub folder: String,
    /// The class given to shapes placed from the palette.
    pub shape_class: String,
    /// The folder's entries, read again when the folder changes or on Refresh.
    listing: Option<(String, Vec<Entry>)>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeftTab {
    #[default]
    Hierarchy,
    Place,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BottomTab {
    #[default]
    Assets,
    Console,
}

#[derive(Clone)]
struct Entry {
    name: String,
    /// The asset path.
    path: String,
    folder: bool,
}

/// An image shown in egui: the asset, then its egui copy once it has loaded.
pub(crate) struct Thumbnail {
    handle: Handle<Image>,
    texture: Option<egui::TextureHandle>,
}

pub(crate) fn show(editor: &mut Editor, world: &mut World, ctx: &egui::Context) {
    let viewport = ctx.root_ui(|ui| {
        menu_bar(editor, world, ui);
        egui::Panel::bottom("cg_editor bottom")
            .resizable(true)
            .default_size(190.0)
            .show(ui, |ui| bottom(editor, world, ui));
        egui::Panel::left("cg_editor left")
            .resizable(true)
            .default_size(250.0)
            .show(ui, |ui| left(editor, world, ui));
        egui::Panel::right("cg_editor inspector")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| inspector::show(editor, world, ui));
            });
        ui.available_rect_before_wrap()
    });
    prompt(editor, world, ctx);
    conflict(editor, world, ctx);

    // What is left is the viewport: keep the view there.
    editor.viewport = viewport;
    let mut window = world.resource_mut::<Window>();
    let scale = window.scale_factor() as f32;
    let size = window.logical_size();
    let insets = [
        viewport.min.x,
        viewport.min.y,
        size.x - viewport.max.x,
        size.y - viewport.max.y,
    ]
    .map(|inset| (inset.max(0.0) * scale).round() as u32);
    if window.view_insets() != insets {
        window.set_view_insets(insets);
    }
}

fn menu_bar(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui) {
    egui::Panel::top("cg_editor menu").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                let dirty = editor.doc.as_ref().is_some_and(Document::dirty);
                if ui
                    .add_enabled(
                        dirty && !editor.playing(),
                        egui::Button::new("Save").shortcut_text("Ctrl+S"),
                    )
                    .clicked()
                {
                    crate::save(editor, world);
                }
                if ui.button("Quit").clicked() {
                    if dirty {
                        editor.prompt = Some(Prompt::Unsaved(Then::Quit));
                    } else {
                        world.write_message(AppExit::Success);
                    }
                }
            });
            ui.menu_button("Edit", |ui| {
                let undo = editor.history.undo_label().map(|l| format!("Undo {l}"));
                if ui
                    .add_enabled(
                        undo.is_some(),
                        egui::Button::new(undo.unwrap_or("Undo".into())).shortcut_text("Ctrl+Z"),
                    )
                    .clicked()
                {
                    editor.undo();
                }
                let redo = editor.history.redo_label().map(|l| format!("Redo {l}"));
                if ui
                    .add_enabled(
                        redo.is_some(),
                        egui::Button::new(redo.unwrap_or("Redo".into())).shortcut_text("Ctrl+Y"),
                    )
                    .clicked()
                {
                    editor.redo();
                }
                ui.separator();
                let some = !editor.selection.is_empty();
                if ui
                    .add_enabled(some, egui::Button::new("Duplicate").shortcut_text("Ctrl+D"))
                    .clicked()
                {
                    view::duplicate(editor, world);
                }
                if ui
                    .add_enabled(some, egui::Button::new("Delete").shortcut_text("Del"))
                    .clicked()
                {
                    view::delete(editor);
                }
            });
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut editor.grid.show, "Grid");
                ui.checkbox(&mut editor.grid.snap, "Snap to the grid");
                if let Some(mut overlay) = world.get_resource_mut::<DebugOverlay>() {
                    let mut windows = overlay.windows;
                    if ui
                        .checkbox(&mut windows, "Debug windows (stats, entities)")
                        .changed()
                    {
                        overlay.windows = windows;
                    }
                }
            });
            ui.separator();
            toolbar(editor, world, ui);
        });
    });
}

fn toolbar(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui) {
    let editing = editor.mode == Mode::Editing;
    ui.add_enabled_ui(editing, |ui| {
        for (tool, label, key) in [
            (Tool::Move, "Move", "W"),
            (Tool::Rotate, "Rotate", "E"),
            (Tool::Scale, "Resize", "R"),
        ] {
            if ui
                .selectable_label(editor.tool == tool, label)
                .on_hover_text(key)
                .clicked()
            {
                editor.tool = tool;
            }
        }
        ui.separator();
        ui.label("Grid");
        let step = editor.grid_step(world);
        let tile = editor.grid.step == 0.0;
        egui::ComboBox::from_id_salt("cg_editor grid")
            .selected_text(if tile {
                format!("tile ({step})")
            } else {
                format!("{step}")
            })
            .width(70.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut editor.grid.step, 0.0, "tile");
                for step in [1.0, 2.0, 4.0, 8.0, 9.0, 16.0, 18.0, 32.0] {
                    ui.selectable_value(&mut editor.grid.step, step, format!("{step}"));
                }
            });
        ui.toggle_value(&mut editor.grid.snap, "Snap");
    });
    ui.separator();
    match editor.mode {
        Mode::Editing => {
            let can = editor.doc.is_some();
            if ui
                .add_enabled(can, egui::Button::new("Play"))
                .on_hover_text("F5")
                .clicked()
            {
                play::start(editor, world);
            }
        }
        Mode::Playing => {
            if ui.button("Stop").on_hover_text("Shift+F5").clicked() {
                play::stop(editor, world);
            }
        }
        Mode::Stopping(_) => {
            ui.label("Stopping...");
        }
    }
    ui.separator();
    if let Some(doc) = &editor.doc {
        let dirty = if doc.dirty() { " *" } else { "" };
        ui.label(format!("{}{dirty}", doc.path()));
    }
    if !editor.status.is_empty() {
        ui.separator();
        ui.weak(&editor.status);
    }
}

fn left(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut editor.panels.left, LeftTab::Hierarchy, "Hierarchy");
        let map = editor.map().is_some();
        ui.add_enabled_ui(map, |ui| {
            ui.selectable_value(&mut editor.panels.left, LeftTab::Place, "Place");
        });
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| match editor.panels.left {
            LeftTab::Place if editor.map().is_some() => palette(editor, world, ui),
            _ => hierarchy(editor, world, ui),
        });
}

/// What a hierarchy row carries while dragged.
#[derive(Clone)]
enum Dragged {
    Object(u32),
    Node(Vec<usize>),
}

fn hierarchy(editor: &mut Editor, world: &World, ui: &mut egui::Ui) {
    match &editor.doc {
        None => {
            ui.weak("Open a map (.tmx) or a prefab (.prefab.ron) from the asset browser below.");
        }
        Some(Document::Map(_)) => map_hierarchy(editor, world, ui),
        Some(Document::Prefab(_)) => {
            let mut action = None;
            node_row(editor, ui, &[], &mut action);
            if let Some((from, to)) = action {
                let from_clone: Vec<usize> = from;
                editor.edit("reparent", None, |doc, selected| {
                    if let Document::Prefab(prefab) = doc
                        && prefab.root.reparent(&from_clone, &to)
                    {
                        selected.clear();
                    }
                });
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Add child").clicked() {
                    let parent = match editor.selection.first() {
                        Some(Selected::Node(path)) => path.clone(),
                        _ => Vec::new(),
                    };
                    editor.edit("add child", None, |doc, selected| {
                        if let Document::Prefab(prefab) = doc
                            && let Some(node) = prefab.root.at_mut(&parent)
                        {
                            node.children.push(crate::doc::PrefabNode {
                                components: vec![(
                                    "Transform2D".into(),
                                    crate::value::Value::from_serialize(&Transform2D::default())
                                        .unwrap_or(crate::value::Value::Unit),
                                )],
                                children: Vec::new(),
                            });
                            let mut path = parent.clone();
                            path.push(node.children.len() - 1);
                            *selected = vec![Selected::Node(path)];
                        }
                    });
                }
            });
        }
    }
}

fn map_hierarchy(editor: &mut Editor, world: &World, ui: &mut egui::Ui) {
    let Some(doc) = editor.map() else {
        return;
    };
    let map = world.resource::<Assets<TiledMap>>().get(&doc.handle);
    // A tile object without a class of its own is named by its tile's.
    let tile_class = |gid: u32| {
        let map = map?;
        let class = &map.tile_info(map.tile_for_gid(gid)?)?.class;
        (!class.is_empty()).then(|| class.clone())
    };
    // (layer index, object id or none for the layer row, label)
    let layers: Vec<(String, Vec<(u32, String)>)> = doc
        .objects
        .layers
        .iter()
        .map(|layer| {
            let objects = layer
                .objects
                .iter()
                .map(|o| {
                    let class = if o.class.is_empty() && o.gid != 0 {
                        tile_class(o.gid).unwrap_or_default()
                    } else {
                        o.class.clone()
                    };
                    let what = match (o.name.is_empty(), class.is_empty()) {
                        (false, false) => format!("{} ({class})", o.name),
                        (false, true) => o.name.clone(),
                        (true, false) => class,
                        (true, true) if o.gid != 0 => "tile".into(),
                        _ => "object".into(),
                    };
                    (o.id, format!("{what}  #{}", o.id))
                })
                .collect();
            (layer.name.clone(), objects)
        })
        .collect();
    if layers.is_empty() {
        ui.weak("This map has no object layer. Add one in Tiled, save, and it shows here.");
    }
    let mut moved: Option<(u32, usize, Option<u32>)> = None;
    let mut clicked: Option<(u32, bool)> = None;
    let shift = ui.input(|i| i.modifiers.shift);
    for (index, (name, objects)) in layers.iter().enumerate() {
        let header = egui::CollapsingHeader::new(format!("{name}  ({})", objects.len()))
            .id_salt(("cg_editor layer", index))
            .default_open(true);
        let response = header.show(ui, |ui| {
            for (id, label) in objects {
                let selected = editor.selection.contains(&Selected::Object(*id));
                let row = ui.dnd_drag_source(
                    egui::Id::unique(("cg_editor object", *id)),
                    Dragged::Object(*id),
                    |ui| ui.selectable_label(selected, label),
                );
                if row.inner.clicked() {
                    clicked = Some((*id, shift));
                }
                if let Some(payload) = row.response.dnd_release_payload::<Dragged>()
                    && let Dragged::Object(dragged) = *payload
                    && dragged != *id
                {
                    moved = Some((dragged, index, Some(*id)));
                }
            }
        });
        if let Some(payload) = response.header_response.dnd_release_payload::<Dragged>()
            && let Dragged::Object(dragged) = *payload
        {
            moved = Some((dragged, index, None));
        }
    }
    if let Some((id, add)) = clicked {
        if add {
            match editor
                .selection
                .iter()
                .position(|s| *s == Selected::Object(id))
            {
                Some(i) => {
                    editor.selection.remove(i);
                }
                None => editor.selection.push(Selected::Object(id)),
            }
        } else {
            editor.selection = vec![Selected::Object(id)];
        }
    }
    if let Some((id, layer, before)) = moved {
        // Dragging one of several selected objects moves them all.
        let ids: Vec<u32> = if editor.selection.contains(&Selected::Object(id)) {
            editor
                .selection
                .iter()
                .filter_map(|s| match s {
                    Selected::Object(id) => Some(*id),
                    Selected::Node(_) => None,
                })
                .collect()
        } else {
            vec![id]
        };
        editor.edit("move to layer", None, |doc, _| {
            if let Document::Map(map) = doc {
                map.move_to_layer(&ids, layer, before);
            }
        });
    }
}

fn node_row(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    path: &[usize],
    action: &mut Option<(Vec<usize>, Vec<usize>)>,
) {
    let Some(doc) = editor.prefab() else {
        return;
    };
    let Some(node) = doc.root.at(path) else {
        return;
    };
    let label = node_label(node, path);
    let children = node.children.len();
    let id = egui::Id::unique(("cg_editor node", path.to_vec()));
    if children == 0 {
        node_line(editor, ui, path, &label, action);
        return;
    }
    egui::CollapsingHeader::new("")
        .id_salt(id.with("open"))
        .default_open(true)
        .show_unindented(ui, |ui| {
            node_line(editor, ui, path, &label, action);
            ui.indent(id.with("children"), |ui| {
                for i in 0..children {
                    let mut child = path.to_vec();
                    child.push(i);
                    node_row(editor, ui, &child, action);
                }
            });
        });
}

/// One node's row: select by clicking, drop another node on it to make that its child.
fn node_line(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    path: &[usize],
    label: &str,
    action: &mut Option<(Vec<usize>, Vec<usize>)>,
) {
    let selected = editor.selection.contains(&Selected::Node(path.to_vec()));
    let id = egui::Id::unique(("cg_editor node", path.to_vec()));
    let response = ui
        .dnd_drag_source(id, Dragged::Node(path.to_vec()), |ui| {
            ui.selectable_label(selected, label)
        })
        .response;
    if response.clicked() {
        editor.selection = vec![Selected::Node(path.to_vec())];
    }
    if let Some(payload) = response.dnd_release_payload::<Dragged>()
        && let Dragged::Node(from) = &*payload
        && from != path
    {
        *action = Some((from.clone(), path.to_vec()));
    }
}

/// A node's label: its name if it has one, else its components.
fn node_label(node: &crate::doc::PrefabNode, path: &[usize]) -> String {
    let name = node.component("Name").and_then(|v| match v {
        crate::value::Value::Str(name) => Some(name.clone()),
        _ => None,
    });
    let what = name.unwrap_or_else(|| {
        let names: Vec<&str> = node
            .components
            .iter()
            .map(|(n, _)| n.as_str())
            .filter(|n| *n != "Transform2D")
            .take(3)
            .collect();
        if names.is_empty() {
            "entity".into()
        } else {
            names.join(", ")
        }
    });
    if path.is_empty() {
        format!("{what} (root)")
    } else {
        what
    }
}

fn palette(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui) {
    let Some(doc) = editor.map() else {
        return;
    };
    let handle = doc.handle.clone();
    ui.weak("Choose, then click in the view. Shift keeps placing; Escape stops.");
    let maps = world.resource::<Assets<TiledMap>>();
    let Some(map) = maps.get(&handle) else {
        ui.weak("The map is loading...");
        return;
    };
    // Tiles with a class: what the game turns into things.
    let mut tiles: Vec<PaletteTile> = Vec::new();
    for (index, tileset) in map.tilesets.iter().enumerate() {
        let mut ids: Vec<(&u32, &cashgear_engine::tiled::TileInfo)> = tileset
            .tiles
            .iter()
            .filter(|(_, info)| !info.class.is_empty())
            .collect();
        ids.sort_by_key(|(id, _)| **id);
        for (id, info) in ids {
            tiles.push(PaletteTile {
                tileset: index,
                tile: *id,
                class: info.class.clone(),
                image: tileset.image.clone(),
                rect: tileset.tile_rect(*id),
                image_size: tileset.image_size,
            });
        }
    }
    ui.strong("Tiles with a class");
    if tiles.is_empty() {
        ui.weak("None of the map's tilesets gives a tile a class.");
    }
    ui.horizontal_wrapped(|ui| {
        for PaletteTile {
            tileset,
            tile,
            class,
            image,
            rect,
            image_size: size,
        } in tiles
        {
            let chosen = editor.placing == Some(Placement::Tile { tileset, tile });
            let picture = image.and_then(|image| texture(editor, world, &image));
            let response = match picture {
                Some(texture) => {
                    let uv = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x / size.x as f32, rect.min.y / size.y as f32),
                        egui::pos2(rect.max.x / size.x as f32, rect.max.y / size.y as f32),
                    );
                    let image = egui::Image::new((texture.id(), egui::vec2(36.0, 36.0))).uv(uv);
                    ui.add(egui::Button::image(image).selected(chosen))
                }
                None => ui.add(egui::Button::new(&class).selected(chosen)),
            };
            if response.on_hover_text(&class).clicked() {
                editor.placing = Some(Placement::Tile { tileset, tile });
            }
        }
    });
    ui.add_space(8.0);
    ui.strong("Shapes");
    ui.horizontal(|ui| {
        ui.label("Class");
        ui.text_edit_singleline(&mut editor.panels.shape_class);
    });
    let prefabs: Vec<String> = world
        .resource::<Prefabs>()
        .names()
        .into_iter()
        .map(str::to_owned)
        .collect();
    if !prefabs.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.weak("Prefabs:");
            for name in prefabs {
                if ui.small_button(&name).clicked() {
                    editor.panels.shape_class = name;
                }
            }
        });
    }
    ui.horizontal(|ui| {
        for (kind, label) in [
            (ShapeKind::Point, "Point"),
            (ShapeKind::Rect, "Rectangle"),
            (ShapeKind::Ellipse, "Ellipse"),
        ] {
            let placement = Placement::Shape {
                kind,
                class: editor.panels.shape_class.clone(),
            };
            let chosen = editor.placing.as_ref() == Some(&placement);
            if ui.selectable_label(chosen, label).clicked() {
                editor.placing = Some(placement);
            }
        }
    });
}

/// A tile of the palette: which it is, its class, and where its picture is.
struct PaletteTile {
    tileset: usize,
    tile: u32,
    class: String,
    image: Option<Handle<Image>>,
    rect: Rect,
    image_size: UVec2,
}

/// The egui copy of an image asset (loading it if needed), once it has loaded.
pub(crate) fn texture(
    editor: &mut Editor,
    world: &World,
    image: &Handle<Image>,
) -> Option<egui::TextureHandle> {
    let path = world.resource::<AssetServer>().path(image)?;
    let entry = editor
        .thumbnails
        .entry(path.clone())
        .or_insert_with(|| Thumbnail {
            handle: image.clone(),
            texture: None,
        });
    if entry.texture.is_none() {
        let data = world.resource::<Assets<Image>>().get(&entry.handle)?;
        let pixels = egui::ColorImage::from_rgba_unmultiplied(
            [data.width() as usize, data.height() as usize],
            data.data(),
        );
        let ctx = world
            .resource::<cashgear_engine::ui::EguiContext>()
            .get()?
            .clone();
        entry.texture = Some(ctx.load_texture(path, pixels, egui::TextureOptions::NEAREST));
    }
    entry.texture.clone()
}

fn bottom(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut editor.panels.bottom, BottomTab::Assets, "Assets");
        let warnings = editor
            .console
            .iter()
            .filter(|l| l.level <= log::Level::Warn)
            .count();
        let label = if warnings > 0 {
            format!("Console ({warnings})")
        } else {
            "Console".into()
        };
        ui.selectable_value(&mut editor.panels.bottom, BottomTab::Console, label);
    });
    ui.separator();
    match editor.panels.bottom {
        BottomTab::Assets => assets(editor, world, ui),
        BottomTab::Console => console(editor, ui),
    }
}

fn assets(editor: &mut Editor, world: &mut World, ui: &mut egui::Ui) {
    let folder = editor.panels.folder.clone();
    let mut refresh = false;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!folder.is_empty(), egui::Button::new("Up"))
            .clicked()
        {
            editor.panels.folder = cashgear_engine::assets::path::parent(&folder)
                .trim_end_matches('/')
                .to_owned();
        }
        ui.label(if folder.is_empty() {
            "assets/".to_owned()
        } else {
            format!("assets/{folder}/")
        });
        refresh = ui.small_button("Refresh").clicked();
    });
    let stale = editor
        .panels
        .listing
        .as_ref()
        .is_none_or(|(at, _)| *at != editor.panels.folder);
    if stale || refresh {
        let entries = list(&editor.root, &editor.panels.folder);
        editor.panels.listing = Some((editor.panels.folder.clone(), entries));
    }
    let entries = editor
        .panels
        .listing
        .as_ref()
        .map(|(_, e)| e.clone())
        .unwrap_or_default();
    let mut go = None;
    let mut open = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for entry in entries {
                    let thumb = if entry.path.ends_with(".png") {
                        let handle: Handle<Image> =
                            world.resource::<AssetServer>().load(&entry.path);
                        texture(editor, world, &handle)
                    } else {
                        None
                    };
                    let icon = if entry.folder {
                        "folder"
                    } else {
                        entry.name.rsplit('.').next().unwrap_or("file")
                    };
                    let response = ui
                        .allocate_ui(egui::vec2(84.0, 84.0), |ui| {
                            ui.vertical_centered(|ui| {
                                let size = egui::vec2(52.0, 52.0);
                                let picture = match &thumb {
                                    Some(texture) => {
                                        let [w, h] = texture.size();
                                        let fit = (52.0 / w.max(h) as f32).min(4.0);
                                        ui.add(
                                            egui::Image::new((
                                                texture.id(),
                                                egui::vec2(w as f32 * fit, h as f32 * fit),
                                            ))
                                            .sense(egui::Sense::click()),
                                        )
                                    }
                                    None => {
                                        let color = if entry.folder {
                                            egui::Color32::from_rgb(220, 180, 90)
                                        } else if entry.path.ends_with(".tmx")
                                            || entry.path.ends_with(".prefab.ron")
                                        {
                                            egui::Color32::from_rgb(120, 190, 255)
                                        } else {
                                            ui.visuals().weak_text_color()
                                        };
                                        ui.add_sized(
                                            size,
                                            egui::Button::new(
                                                egui::RichText::new(icon).color(color).strong(),
                                            )
                                            .fill(ui.visuals().extreme_bg_color),
                                        )
                                    }
                                };
                                let name = ui.add(
                                    egui::Label::new(egui::RichText::new(&entry.name).small())
                                        .truncate(),
                                );
                                picture.union(name)
                            })
                            .inner
                        })
                        .inner
                        .on_hover_text(&entry.path);
                    if response.double_clicked() || (entry.folder && response.clicked()) {
                        if entry.folder {
                            go = Some(entry.path.clone());
                        } else {
                            open = Some(entry.path.clone());
                        }
                    }
                }
            });
        });
    if let Some(folder) = go {
        editor.panels.folder = folder;
    }
    if let Some(path) = open {
        if path.ends_with(".tmx") || path.ends_with(".prefab.ron") {
            crate::request_open(editor, &path);
        } else {
            editor.status = format!("The editor opens maps and prefabs; {path} is neither");
        }
    }
}

/// The folders and files of an asset folder, folders first.
fn list(root: &std::path::Path, folder: &str) -> Vec<Entry> {
    let dir: PathBuf = if folder.is_empty() {
        root.to_path_buf()
    } else {
        root.join(folder)
    };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = read
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = if folder.is_empty() {
                name.clone()
            } else {
                format!("{folder}/{name}")
            };
            Some(Entry {
                folder: entry.file_type().is_ok_and(|t| t.is_dir()),
                name,
                path,
            })
        })
        .collect();
    entries.sort_by(|a, b| b.folder.cmp(&a.folder).then_with(|| a.name.cmp(&b.name)));
    entries
}

fn console(editor: &mut Editor, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        for (i, level) in ["Errors", "Warnings", "Info", "Debug", "Trace"]
            .iter()
            .enumerate()
        {
            ui.toggle_value(&mut editor.console_levels[i], *level);
        }
        ui.label("Find");
        ui.add(egui::TextEdit::singleline(&mut editor.console_filter).desired_width(140.0));
        if ui.small_button("Clear").clicked() {
            editor.console.clear();
        }
    });
    let filter = editor.console_filter.to_lowercase();
    let lines: Vec<_> = editor
        .console
        .iter()
        .filter(|l| editor.console_levels[l.level as usize - 1])
        .filter(|l| {
            filter.is_empty()
                || l.message.to_lowercase().contains(&filter)
                || l.target.contains(&filter)
        })
        .collect();
    let row = ui.text_style_height(&egui::TextStyle::Monospace);
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .stick_to_bottom(true)
        .show_rows(ui, row, lines.len(), |ui, range| {
            for line in &lines[range] {
                let color = match line.level {
                    log::Level::Error => egui::Color32::from_rgb(240, 100, 90),
                    log::Level::Warn => egui::Color32::from_rgb(240, 200, 90),
                    log::Level::Info => ui.visuals().text_color(),
                    _ => ui.visuals().weak_text_color(),
                };
                ui.label(
                    egui::RichText::new(format!(
                        "{:5} {}: {}",
                        line.level, line.target, line.message
                    ))
                    .monospace()
                    .color(color),
                );
            }
        });
}

/// The "unsaved changes" question.
fn prompt(editor: &mut Editor, world: &mut World, ctx: &egui::Context) {
    let Some(Prompt::Unsaved(then)) = editor.prompt.clone() else {
        return;
    };
    let name = editor.doc.as_ref().map_or("", Document::path).to_owned();
    let mut answer = None;
    egui::Modal::new(egui::Id::unique("cg_editor unsaved")).show(ctx, |ui| {
        ui.heading("Unsaved changes");
        ui.label(format!("Save the changes to {name}?"));
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                answer = Some(Some(true));
            }
            if ui.button("Don't save").clicked() {
                answer = Some(Some(false));
            }
            if ui.button("Cancel").clicked() {
                answer = Some(None);
            }
        });
    });
    let Some(answer) = answer else {
        return;
    };
    editor.prompt = None;
    let Some(save) = answer else {
        return;
    };
    if save && !crate::save(editor, world) {
        return;
    }
    match then {
        Then::Open(path) => editor.pending_open = Some(path),
        Then::Quit => {
            world.write_message(AppExit::Success);
        }
    }
}

/// The question when the open map changed on disk and here.
fn conflict(editor: &mut Editor, _world: &mut World, ctx: &egui::Context) {
    let Some(Document::Map(doc)) = &editor.doc else {
        return;
    };
    if !doc.conflict {
        return;
    }
    let path = doc.path.clone();
    let mut answer = None;
    egui::Window::new("Changed on disk")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
        .show(ctx, |ui| {
            ui.label(format!(
                "{path} was saved elsewhere while it has unsaved changes here."
            ));
            ui.horizontal(|ui| {
                if ui.button("Reload from disk").clicked() {
                    answer = Some(true);
                }
                if ui.button("Keep my changes").clicked() {
                    answer = Some(false);
                }
            });
        });
    match answer {
        Some(true) => {
            editor.edit("reload from disk", None, |doc, _| {
                if let Document::Map(map) = doc {
                    map.objects = map.saved.clone();
                    map.conflict = false;
                }
            });
        }
        Some(false) => {
            if let Some(Document::Map(map)) = editor.doc.as_mut() {
                map.conflict = false;
            }
        }
        None => {}
    }
}
