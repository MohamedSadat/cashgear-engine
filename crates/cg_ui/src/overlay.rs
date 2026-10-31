//! The egui debug overlay: frame times, render stats, debug drawing toggles, the entity
//! list and a component inspector. Fed from the engine's own input and drawn with
//! `egui-wgpu` as the last pass of each frame.

use crate::UiSystems;
use crate::interaction::UiPointer;
use crate::layout::UiStack;
use bevy_ecs::prelude::*;
use cg_core::{App, PreUpdate, Render, Time};
use cg_physics::PhysicsDebug;
use cg_platform::Window;
use cg_platform::input::{
    ButtonInput, InputSystems, KeyCode, MouseButton, PointerId, Pointers, TextInput,
};
use cg_render::{OverlayFrame, RenderOverlay, RenderOverlays, RenderStats, RenderSystems};
use cg_world::Transform2D;
use core::any::TypeId;
use core::fmt::Debug;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};

/// The `egui` crate the overlay is built with, for game code drawing its own debug windows.
pub use egui;

/// Frame times kept for the graph.
const SAMPLES: usize = 240;

/// The debug overlay's state. Its key (F12 by default) opens and closes it.
#[derive(Resource, Debug, Clone)]
pub struct DebugOverlay {
    /// Whether it shows.
    pub open: bool,
    /// The key that opens and closes it.
    pub key: Option<KeyCode>,
    /// Whether the overlay draws its own windows (frame times, render stats, entities and
    /// the inspector) and the UI outlines. On by default; a tool that draws its own egui
    /// panels through [`EguiContext`], such as the editor, turns it off.
    pub windows: bool,
    /// Outline every UI node's rectangle.
    pub ui_outlines: bool,
    /// Show the Profiler window (with the `profile` feature). `--profile` opens it.
    pub profiler: bool,
    /// The entity shown in the inspector.
    pub selected: Option<Entity>,
    filter: String,
    frame_times: VecDeque<f32>,
}

impl DebugOverlay {
    /// The recent frame times in milliseconds, oldest first.
    pub fn frame_times(&self) -> impl ExactSizeIterator<Item = f32> + '_ {
        self.frame_times.iter().copied()
    }
}

/// The overlay's egui context. While the overlay is open, game systems can draw their own
/// egui windows with it between `PreUpdate` and `Render`:
///
/// ```
/// use cg_core::prelude::*;
/// use cg_ui::{EguiContext, egui};
///
/// fn my_window(egui: Res<EguiContext>) {
///     if let Some(ctx) = egui.get() {
///         egui::Window::new("Spawner").show(ctx, |ui| {
///             ui.label("hello");
///         });
///     }
/// }
/// ```
#[derive(Resource, Clone, Default)]
pub struct EguiContext {
    ctx: egui::Context,
    running: bool,
}

impl EguiContext {
    /// The context, while an overlay frame is being built (the overlay is open).
    pub fn get(&self) -> Option<&egui::Context> {
        self.running.then_some(&self.ctx)
    }
}

/// How the inspector shows a component type.
type ShowFn = fn(&World, Entity) -> Option<String>;

/// The component types the inspector can show the values of, with their `Debug` output.
/// Others are listed by name only. Register more with [`InspectExt::register_inspect`].
#[derive(Resource, Default)]
pub struct Inspector {
    types: HashMap<TypeId, ShowFn>,
}

impl Inspector {
    /// Makes the inspector show `T`'s values.
    pub fn register<T: Component + Debug>(&mut self) {
        self.types.insert(TypeId::of::<T>(), |world, entity| {
            world.get::<T>(entity).map(|c| format!("{c:#?}"))
        });
    }

    /// The `Debug` text of `entity`'s component of type `type_id`, if that type is
    /// registered and the entity has one. The editor shows spawned objects with it.
    pub fn describe(&self, world: &World, entity: Entity, type_id: TypeId) -> Option<String> {
        self.types
            .get(&type_id)
            .and_then(|show| show(world, entity))
    }
}

/// Registering component types with the debug overlay's [`Inspector`].
pub trait InspectExt {
    /// Shows `T`'s values (its `Debug` output) in the inspector.
    fn register_inspect<T: Component + Debug>(&mut self) -> &mut Self;
}

impl InspectExt for App {
    fn register_inspect<T: Component + Debug>(&mut self) -> &mut Self {
        self.world_mut()
            .get_resource_or_insert_with(Inspector::default)
            .register::<T>();
        self
    }
}

/// What the ECS side hands the painter each frame.
#[derive(Default)]
struct Shared {
    primitives: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
    pixels_per_point: f32,
    /// The painter got a new GPU device; the context must start over to resend its fonts.
    reset: bool,
}

// egui insists that texture changes are handled, and panics when they are dropped.
impl Drop for Shared {
    fn drop(&mut self) {
        self.textures.clear();
    }
}

#[derive(Resource, Clone, Default)]
struct SharedFrame(Arc<Mutex<Shared>>);

impl SharedFrame {
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Draws the overlay's triangles over the frame.
struct EguiPainter {
    shared: SharedFrame,
    renderer: Option<(u64, egui_wgpu::Renderer)>,
}

impl RenderOverlay for EguiPainter {
    fn draw(&mut self, frame: &mut OverlayFrame<'_>) {
        let mut shared = self.shared.lock();
        if self
            .renderer
            .as_ref()
            .is_none_or(|(generation, _)| *generation != frame.device_generation)
        {
            if self.renderer.is_some() {
                // A new device lost the old textures; start the context again.
                shared.reset = true;
                shared.primitives.clear();
                shared.textures.clear();
            }
            let renderer = egui_wgpu::Renderer::new(
                frame.device,
                frame.format,
                egui_wgpu::RendererOptions::default(),
            );
            self.renderer = Some((frame.device_generation, renderer));
        }
        let Some((_, renderer)) = self.renderer.as_mut() else {
            return;
        };
        let mut textures = core::mem::take(&mut shared.textures);
        for (id, deltas) in &textures.set {
            for delta in deltas {
                renderer.update_texture(frame.device, frame.queue, *id, delta);
            }
        }
        if !shared.primitives.is_empty() {
            let screen = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [frame.size.x, frame.size.y],
                pixels_per_point: shared.pixels_per_point,
            };
            // Paint callbacks would return command buffers; the overlay uses none.
            let _ = renderer.update_buffers(
                frame.device,
                frame.queue,
                frame.encoder,
                &shared.primitives,
                &screen,
            );
            let mut pass = frame
                .encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("cg_ui debug overlay"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: frame.target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &shared.primitives, &screen);
        }
        for id in &textures.free {
            renderer.free_texture(id);
        }
        textures.clear();
    }
}

use cg_render::wgpu;

/// Sets the overlay up. Called by `UiPlugin`.
pub(crate) fn build(app: &mut App, key: Option<KeyCode>, open: bool, profiler: bool) {
    let shared = SharedFrame::default();
    app.insert_resource(DebugOverlay {
        open,
        key,
        windows: true,
        ui_outlines: false,
        profiler,
        selected: None,
        filter: String::new(),
        frame_times: VecDeque::with_capacity(SAMPLES),
    })
    .init_resource::<EguiContext>()
    .init_resource::<Inspector>()
    .insert_resource(shared.clone())
    .init_resource::<RenderOverlays>()
    .add_systems(
        PreUpdate,
        begin_overlay
            .after(InputSystems::Actions)
            .before(UiSystems::Input),
    )
    .add_systems(Render, draw_overlay.before(RenderSystems::Draw));
    app.world_mut()
        .resource_mut::<RenderOverlays>()
        .add(EguiPainter {
            shared,
            renderer: None,
        });
    style(&app.world().resource::<EguiContext>().ctx);
    #[cfg(feature = "profile")]
    app.init_resource::<crate::profiler::ProfilerView>();
    if profiler {
        cg_core::profile::set_recording(true);
    }

    use cg_physics::{
        Collider, CollisionLayers, KinematicBody, KinematicContacts, Sensor, TileCollider,
    };
    use cg_render::{AtlasSprite, Camera2D, InheritedVisibility, Layer, Sprite, Visibility};
    app.register_inspect::<Transform2D>()
        .register_inspect::<cg_world::GlobalTransform2D>()
        .register_inspect::<cg_world::Interpolated>()
        .register_inspect::<bevy_ecs::name::Name>()
        .register_inspect::<ChildOf>()
        .register_inspect::<Children>()
        .register_inspect::<Sprite>()
        .register_inspect::<AtlasSprite>()
        .register_inspect::<Layer>()
        .register_inspect::<Visibility>()
        .register_inspect::<InheritedVisibility>()
        .register_inspect::<Camera2D>()
        .register_inspect::<Collider>()
        .register_inspect::<Sensor>()
        .register_inspect::<KinematicBody>()
        .register_inspect::<KinematicContacts>()
        .register_inspect::<CollisionLayers>()
        .register_inspect::<TileCollider>()
        .register_inspect::<crate::Node>()
        .register_inspect::<crate::ComputedNode>()
        .register_inspect::<crate::Text>()
        .register_inspect::<crate::UiImage>()
        .register_inspect::<crate::BackgroundColor>()
        .register_inspect::<crate::BorderColor>()
        .register_inspect::<crate::Button>()
        .register_inspect::<crate::Interaction>()
        .register_inspect::<crate::Slider>()
        .register_inspect::<crate::ProgressBar>()
        .register_inspect::<crate::Localized>()
        .register_inspect::<crate::TouchButton>()
        .register_inspect::<crate::TouchDpad>()
        .register_inspect::<crate::ZIndex>();
}

fn style(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.visuals.window_shadow = egui::epaint::Shadow::NONE;
    });
}

/// Toggles the overlay, records the frame time and starts the egui frame from this
/// frame's input.
#[allow(clippy::too_many_arguments)]
#[profiling::function]
fn begin_overlay(
    mut overlay: ResMut<DebugOverlay>,
    mut egui_context: ResMut<EguiContext>,
    shared: Res<SharedFrame>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pointers: Res<Pointers>,
    text: Res<TextInput>,
    window: Res<Window>,
    time: Res<Time>,
    mut ui_pointer: ResMut<UiPointer>,
) {
    if overlay.frame_times.len() == SAMPLES {
        overlay.frame_times.pop_front();
    }
    overlay
        .frame_times
        .push_back(time.raw_delta_secs() * 1000.0);
    if let Some(key) = overlay.key
        && keys.just_pressed(key)
    {
        overlay.open = !overlay.open;
    }
    {
        let mut shared = shared.lock();
        if shared.reset {
            shared.reset = false;
            egui_context.ctx = egui::Context::default();
            style(&egui_context.ctx);
        }
        if !overlay.open {
            shared.primitives.clear();
        }
    }
    let ctx = egui_context.ctx.clone();
    // What egui used the pointer for last frame decides whether the game UI gets it now.
    ui_pointer.blocked =
        overlay.open && (ctx.egui_wants_pointer_input() || ctx.egui_is_using_pointer());
    egui_context.running = overlay.open;
    if overlay.open {
        ctx.begin_pass(raw_input(&window, &pointers, &keys, &mouse, &text, &time));
    }
}

/// The egui input for this frame, from the engine's input state.
fn raw_input(
    window: &Window,
    pointers: &Pointers,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    text: &TextInput,
    time: &Time,
) -> egui::RawInput {
    let size = window.logical_size();
    let mut raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(size.x, size.y),
        )),
        time: Some(time.raw_elapsed().as_secs_f64()),
        focused: window.focused(),
        ..Default::default()
    };
    raw.viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .native_pixels_per_point = Some(window.scale_factor() as f32);
    let held = |a: KeyCode, b: KeyCode| keys.pressed(a) || keys.pressed(b);
    let modifiers = egui::Modifiers {
        alt: held(KeyCode::AltLeft, KeyCode::AltRight),
        ctrl: held(KeyCode::ControlLeft, KeyCode::ControlRight),
        shift: held(KeyCode::ShiftLeft, KeyCode::ShiftRight),
        mac_cmd: false,
        command: held(KeyCode::ControlLeft, KeyCode::ControlRight),
    };
    raw.events.push(egui::Event::ModifiersChanged(modifiers));

    let pointer = pointers
        .get(PointerId::Mouse)
        .or_else(|| pointers.iter().find(|p| p.is_touch()));
    match pointer {
        Some(pointer) => {
            let pos = egui::pos2(pointer.position().x, pointer.position().y);
            raw.events.push(egui::Event::PointerMoved(pos));
            let mut button = |egui_button, pressed| {
                raw.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui_button,
                    pressed,
                    modifiers,
                });
            };
            if pointer.is_touch() {
                if pointer.just_pressed() {
                    button(egui::PointerButton::Primary, true);
                }
                if pointer.just_released() {
                    button(egui::PointerButton::Primary, false);
                }
            } else {
                for (ours, theirs) in [
                    (MouseButton::Left, egui::PointerButton::Primary),
                    (MouseButton::Right, egui::PointerButton::Secondary),
                    (MouseButton::Middle, egui::PointerButton::Middle),
                ] {
                    if mouse.just_pressed(ours) {
                        button(theirs, true);
                    }
                    if mouse.just_released(ours) {
                        button(theirs, false);
                    }
                }
            }
        }
        None => raw.events.push(egui::Event::PointerGone),
    }
    let scroll = pointers.scroll();
    if scroll != cg_core::glam::Vec2::ZERO {
        raw.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(scroll.x, scroll.y),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers,
        });
    }
    if !text.typed().is_empty() {
        raw.events.push(egui::Event::Text(text.typed().to_owned()));
    }
    use egui::Key;
    for (code, key) in [
        (KeyCode::Backspace, Key::Backspace),
        (KeyCode::Delete, Key::Delete),
        (KeyCode::Enter, Key::Enter),
        (KeyCode::NumpadEnter, Key::Enter),
        (KeyCode::Escape, Key::Escape),
        (KeyCode::Tab, Key::Tab),
        (KeyCode::ArrowLeft, Key::ArrowLeft),
        (KeyCode::ArrowRight, Key::ArrowRight),
        (KeyCode::ArrowUp, Key::ArrowUp),
        (KeyCode::ArrowDown, Key::ArrowDown),
        (KeyCode::Home, Key::Home),
        (KeyCode::End, Key::End),
        (KeyCode::KeyA, Key::A),
        (KeyCode::KeyC, Key::C),
        (KeyCode::KeyV, Key::V),
        (KeyCode::KeyX, Key::X),
        (KeyCode::KeyZ, Key::Z),
    ] {
        for (pressed, happened) in [
            (true, keys.just_pressed(code)),
            (false, keys.just_released(code)),
        ] {
            if happened {
                raw.events.push(egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers,
                });
            }
        }
    }
    raw
}

/// Builds the overlay's windows and hands the result to the painter. An exclusive system
/// in `Render`, so the inspector can read and edit any component.
#[profiling::function]
fn draw_overlay(world: &mut World) {
    let Some(ctx) = world
        .get_resource::<EguiContext>()
        .and_then(|e| e.get().cloned())
    else {
        return;
    };
    world.resource_scope::<DebugOverlay, _>(|world, mut overlay| {
        if overlay.windows {
            windows(&ctx, world, &mut overlay);
        }
    });
    let output = ctx.end_pass();
    world.resource_mut::<EguiContext>().running = false;
    let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
    let shared = world.resource::<SharedFrame>().clone();
    let mut shared = shared.lock();
    shared.primitives = primitives;
    shared.textures.append(output.textures_delta);
    shared.pixels_per_point = output.pixels_per_point;
}

fn windows(ctx: &egui::Context, world: &mut World, overlay: &mut DebugOverlay) {
    let stats = world.resource::<RenderStats>().clone();
    let window = world.resource::<Window>().clone();
    egui::Window::new("CashGear")
        .default_pos(egui::pos2(8.0, 8.0))
        .default_width(270.0)
        .show(ctx, |ui| {
            ui.label(format!(
                "{:.0} fps, {:.2} ms a frame",
                stats.fps(),
                stats.frame_time().as_secs_f64() * 1000.0
            ));
            frame_graph(ui, overlay);
            egui::CollapsingHeader::new("Render")
                .default_open(true)
                .show(ui, |ui| render_stats(ui, world, &stats, &window));
            egui::CollapsingHeader::new("Debug drawing")
                .default_open(true)
                .show(ui, |ui| {
                    if let Some(physics) = world.get_resource::<PhysicsDebug>() {
                        let mut enabled = physics.enabled;
                        if ui.checkbox(&mut enabled, "Colliders").changed()
                            && let Some(mut physics) = world.get_resource_mut::<PhysicsDebug>()
                        {
                            physics.enabled = enabled;
                        }
                    }
                    ui.checkbox(&mut overlay.ui_outlines, "UI outlines");
                });
            profiler_toggle(ui, overlay);
            egui::CollapsingHeader::new("Entities")
                .default_open(false)
                .show(ui, |ui| entity_list(ui, world, overlay));
        });
    if let Some(selected) = overlay.selected {
        let mut open = true;
        egui::Window::new("Inspector")
            .open(&mut open)
            .default_pos(egui::pos2(290.0, 8.0))
            .default_width(300.0)
            .show(ctx, |ui| inspector(ui, world, selected));
        if !open || world.get_entity(selected).is_err() {
            overlay.selected = None;
        }
    }
    #[cfg(feature = "profile")]
    if overlay.profiler
        && let Some(mut view) = world.get_resource_mut::<crate::profiler::ProfilerView>()
    {
        crate::profiler::window(ctx, &mut view, &mut overlay.profiler);
    }
    if overlay.ui_outlines {
        ui_outlines(ctx, world, &window);
    }
}

/// The checkbox that opens the Profiler window, which needs the `profile` feature.
fn profiler_toggle(ui: &mut egui::Ui, overlay: &mut DebugOverlay) {
    let available = cfg!(feature = "profile");
    let was = overlay.profiler;
    ui.add_enabled_ui(available, |ui| {
        ui.checkbox(&mut overlay.profiler, "Profiler")
            .on_disabled_hover_text("Build with the `profile` feature to record frames.");
    });
    // Opening the window starts recording, which costs a little per scope.
    if overlay.profiler && !was {
        cg_core::profile::set_recording(true);
    }
}

fn frame_graph(ui: &mut egui::Ui, overlay: &DebugOverlay) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 48.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(160));
    // A full graph is 33 ms; the line marks 60 fps.
    let full = 1000.0 / 30.0;
    let y_of = |ms: f32| rect.bottom() - (ms / full).min(1.0) * rect.height();
    let n = overlay.frame_times.len().max(1);
    let bar = rect.width() / SAMPLES as f32;
    for (i, ms) in overlay.frame_times.iter().enumerate() {
        let x = rect.right() - (n - i) as f32 * bar;
        let color = if *ms <= 17.5 {
            egui::Color32::from_rgb(90, 200, 120)
        } else if *ms <= 34.0 {
            egui::Color32::from_rgb(240, 200, 80)
        } else {
            egui::Color32::from_rgb(240, 90, 80)
        };
        painter.rect_filled(
            egui::Rect::from_min_max(egui::pos2(x, y_of(*ms)), egui::pos2(x + bar, rect.bottom())),
            0.0,
            color,
        );
    }
    let sixty = y_of(1000.0 / 60.0);
    painter.line_segment(
        [
            egui::pos2(rect.left(), sixty),
            egui::pos2(rect.right(), sixty),
        ],
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(90)),
    );
}

fn render_stats(ui: &mut egui::Ui, world: &World, stats: &RenderStats, window: &Window) {
    let ui_stack = world.resource::<UiStack>();
    let glyphs = world
        .get_resource::<crate::TextSystem>()
        .map_or(0, crate::TextSystem::glyph_count);
    let rows = [
        ("GPU", format!("{} ({})", stats.adapter(), stats.backend())),
        ("Draw calls", stats.draw_calls().to_string()),
        ("Sprites", stats.sprites().to_string()),
        ("Batches", stats.batches().to_string()),
        ("Culled", stats.culled().to_string()),
        ("Debug shapes", stats.shapes().to_string()),
        ("Entities", world.entities().len().to_string()),
        ("UI nodes", ui_stack.entities().len().to_string()),
        ("Glyphs", glyphs.to_string()),
        (
            "Window",
            format!(
                "{}x{} at {:.2}",
                window.physical_size().x,
                window.physical_size().y,
                window.scale_factor()
            ),
        ),
        (
            "UI",
            format!(
                "{}x{} at {:.2}",
                ui_stack.space().size.x,
                ui_stack.space().size.y,
                ui_stack.space().scale
            ),
        ),
    ];
    egui::Grid::new("cg_ui render stats")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (name, value) in rows {
                ui.label(name);
                ui.label(value);
                ui.end_row();
            }
        });
}

fn entity_list(ui: &mut egui::Ui, world: &mut World, overlay: &mut DebugOverlay) {
    ui.horizontal(|ui| {
        ui.label("Find");
        ui.text_edit_singleline(&mut overlay.filter);
    });
    let filter = overlay.filter.to_lowercase();
    let mut query = world.query::<(Entity, Option<&bevy_ecs::name::Name>)>();
    let mut entities: Vec<(Entity, String)> = query
        .iter(world)
        .map(|(entity, name)| {
            let label = match name {
                Some(name) => format!("{name} ({entity})"),
                None => entity.to_string(),
            };
            (entity, label)
        })
        .filter(|(_, label)| filter.is_empty() || label.to_lowercase().contains(&filter))
        .collect();
    entities.sort_by_key(|(e, _)| (e.index_u32(), e.generation().to_bits()));
    ui.label(format!("{} shown", entities.len()));
    let row = ui.text_style_height(&egui::TextStyle::Body) + 2.0;
    egui::ScrollArea::vertical().max_height(240.0).show_rows(
        ui,
        row,
        entities.len(),
        |ui, range| {
            for (entity, label) in &entities[range] {
                let selected = overlay.selected == Some(*entity);
                if ui.selectable_label(selected, label).clicked() {
                    overlay.selected = Some(*entity);
                }
            }
        },
    );
}

fn inspector(ui: &mut egui::Ui, world: &mut World, entity: Entity) {
    let Ok(components) = world.inspect_entity(entity) else {
        ui.label("This entity is gone.");
        return;
    };
    let components: Vec<(String, Option<TypeId>)> = components
        .map(|(_, info)| (info.name().shortname().to_string(), info.type_id()))
        .collect();
    ui.label(format!("{entity}: {} components", components.len()));
    egui::ScrollArea::vertical()
        .max_height(420.0)
        .show(ui, |ui| {
            for (name, type_id) in components {
                if type_id == Some(TypeId::of::<Transform2D>()) {
                    egui::CollapsingHeader::new(name)
                        .default_open(true)
                        .show(ui, |ui| edit_transform(ui, world, entity));
                    continue;
                }
                let show = type_id
                    .and_then(|id| world.get_resource::<Inspector>()?.types.get(&id).copied());
                match show.and_then(|show| show(world, entity)) {
                    Some(text) => {
                        egui::CollapsingHeader::new(name).show(ui, |ui| {
                            ui.monospace(text);
                        });
                    }
                    None => {
                        ui.label(name);
                    }
                }
            }
        });
}

fn edit_transform(ui: &mut egui::Ui, world: &mut World, entity: Entity) {
    let Some(transform) = world.get::<Transform2D>(entity).copied() else {
        return;
    };
    let mut edited = transform;
    let mut degrees = edited.rotation.to_degrees();
    egui::Grid::new(("cg_ui transform", entity))
        .num_columns(3)
        .show(ui, |ui| {
            ui.label("Position");
            ui.add(egui::DragValue::new(&mut edited.translation.x).speed(0.5));
            ui.add(egui::DragValue::new(&mut edited.translation.y).speed(0.5));
            ui.end_row();
            ui.label("Rotation");
            ui.add(egui::DragValue::new(&mut degrees).speed(0.5).suffix("°"));
            ui.end_row();
            ui.label("Scale");
            ui.add(egui::DragValue::new(&mut edited.scale.x).speed(0.01));
            ui.add(egui::DragValue::new(&mut edited.scale.y).speed(0.01));
            ui.end_row();
        });
    edited.rotation = degrees.to_radians();
    if edited != transform
        && let Some(mut target) = world.get_mut::<Transform2D>(entity)
    {
        *target = edited;
    }
}

/// Outlines every visible UI node, the one under the mouse in yellow.
fn ui_outlines(ctx: &egui::Context, world: &World, window: &Window) {
    let stack = world.resource::<UiStack>();
    let hovered = world.resource::<UiPointer>().hovered();
    let space = stack.space();
    let scale = window.scale_factor() as f32;
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Background,
        egui::Id::unique("cg_ui outlines"),
    ));
    for &entity in stack.entities() {
        let Some(computed) = world.get::<crate::ComputedNode>(entity) else {
            continue;
        };
        let to_points = |ui: cg_core::glam::Vec2| {
            let p = space.to_physical(ui) / scale;
            egui::pos2(p.x, p.y)
        };
        let rect = egui::Rect::from_min_max(
            to_points(computed.rect().min),
            to_points(computed.rect().max),
        );
        let color = if hovered == Some(entity) {
            egui::Color32::YELLOW
        } else {
            egui::Color32::from_rgba_unmultiplied(255, 64, 160, 160)
        };
        painter.rect_stroke(
            rect,
            0.0,
            egui::Stroke::new(1.0, color),
            egui::StrokeKind::Inside,
        );
    }
}
