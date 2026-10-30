//! The Scripts window in the egui debug overlay (feature `debug-overlay`): every script
//! file with its instances and state, and the latest errors.

use crate::{ScriptErrors, ScriptStatus, Scripting};
use bevy_ecs::prelude::*;
use cg_core::prelude::*;
use cg_ui::egui::{self, Color32, RichText};
use cg_ui::{DebugOverlay, EguiContext};

pub(crate) fn build(app: &mut App, show_errors: bool) {
    app.insert_resource(ShowErrors(show_errors))
        .add_systems(Update, (open_on_error, scripts_window).chain());
}

#[derive(Resource)]
struct ShowErrors(bool);

const RED: Color32 = Color32::from_rgb(0xff, 0x6b, 0x6b);
const GREEN: Color32 = Color32::from_rgb(0x8f, 0xd9, 0x8f);

/// Opens the overlay when a new error comes in.
fn open_on_error(
    show: Res<ShowErrors>,
    errors: Res<ScriptErrors>,
    overlay: Option<ResMut<DebugOverlay>>,
    mut seen: Local<u64>,
) {
    if errors.count() == *seen {
        return;
    }
    *seen = errors.count();
    if show.0
        && let Some(mut overlay) = overlay
        && !overlay.open
    {
        overlay.open = true;
    }
}

fn scripts_window(
    egui: Option<Res<EguiContext>>,
    scripting: Res<Scripting>,
    status: Res<ScriptStatus>,
    mut errors: ResMut<ScriptErrors>,
) {
    let Some(ctx) = egui.as_ref().and_then(|egui| egui.get()) else {
        return;
    };
    let failing = !errors.is_empty()
        || status
            .scripts
            .iter()
            .any(|s| s.error.is_some() || s.failed > 0);
    egui::Window::new("Scripts")
        .default_open(failing)
        .default_pos([12.0, 300.0])
        .default_width(420.0)
        .show(ctx, |ui| {
            if !scripting.runs() {
                ui.label(if Scripting::LUA {
                    "Scripts are turned off (--no-scripts)."
                } else {
                    "Lua scripts do not run on this platform."
                });
                return;
            }
            ui.label(format!("{} hook calls in the latest step", status.calls));
            egui::Grid::new("scripts").striped(true).show(ui, |ui| {
                ui.strong("File");
                ui.strong("Entities");
                ui.strong("Loads");
                ui.strong("State");
                ui.end_row();
                for script in &status.scripts {
                    ui.monospace(&script.path);
                    ui.label(script.instances.to_string());
                    ui.label(script.loads.to_string());
                    if script.error.is_some() {
                        ui.label(RichText::new("does not compile").color(RED));
                    } else if script.failed > 0 {
                        ui.label(RichText::new(format!("{} stopped", script.failed)).color(RED));
                    } else {
                        ui.label(RichText::new("running").color(GREEN));
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong(format!("Errors ({})", errors.count()));
                if ui.button("Clear").clicked() {
                    errors.clear();
                }
            });
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for error in errors.iter().rev() {
                        let entity = error
                            .entity
                            .map_or(String::new(), |e| format!(", entity {e}"));
                        ui.label(
                            RichText::new(format!(
                                "frame {}: {}{entity}",
                                error.frame, error.during
                            ))
                            .weak(),
                        );
                        ui.label(RichText::new(error.summary()).color(RED).monospace());
                        ui.collapsing("Traceback", |ui| {
                            ui.monospace(&error.message);
                        });
                        ui.add_space(4.0);
                    }
                });
            ui.label(
                RichText::new("Save a script to reload it and start its stopped entities again.")
                    .weak(),
            );
        });
}
