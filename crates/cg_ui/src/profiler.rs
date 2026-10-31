//! The debug overlay's Profiler window (features `debug-overlay` and `profile`): the recent
//! frames as bars, a flame graph of the chosen frame, and its scopes ranked by their own
//! time. It reads what puffin recorded, so it works on every target, the web and Android
//! included.

use bevy_ecs::prelude::*;
use cg_core::profile;
use puffin::{FrameData, GlobalFrameView, NanoSecond, Reader, Scope, ScopeCollection, Stream};
use std::collections::HashMap;
use std::sync::Arc;

use crate::overlay::egui;

/// Frames kept for the bars (about five seconds at 60 frames a second).
const RECENT_FRAMES: usize = 300;
/// Rows in the scope table.
const TABLE_ROWS: usize = 40;

/// What the Profiler window keeps between frames.
#[derive(Resource)]
pub(crate) struct ProfilerView {
    frames: GlobalFrameView,
    /// The frame shown, by index; `None` follows the latest one.
    chosen: Option<u64>,
    /// The part of the chosen frame the flame graph shows, in nanoseconds from its start.
    zoom: Option<(NanoSecond, NanoSecond)>,
    tab: Tab,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Flame,
    Table,
}

impl Default for ProfilerView {
    fn default() -> Self {
        let frames = GlobalFrameView::default();
        frames.lock().set_max_recent(RECENT_FRAMES);
        Self {
            frames,
            chosen: None,
            zoom: None,
            tab: Tab::Flame,
        }
    }
}

#[cfg(test)]
impl ProfilerView {
    /// How many frames are kept, and whether a scope's name contains `name`.
    pub(crate) fn recorded(&self, name: &str) -> (usize, bool) {
        let frames = self.frames.lock();
        let found = frames
            .scope_collection()
            .scopes_by_id()
            .values()
            .any(|details| details.name().contains(name));
        (frames.recent_frames().count(), found)
    }
}

/// Draws the window; `open` is cleared when its close button is pressed.
pub(crate) fn window(ctx: &egui::Context, view: &mut ProfilerView, open: &mut bool) {
    egui::Window::new("Profiler")
        .open(open)
        .default_pos(egui::pos2(290.0, 8.0))
        .default_width(560.0)
        .show(ctx, |ui| contents(ui, view));
}

fn contents(ui: &mut egui::Ui, view: &mut ProfilerView) {
    let mut recording = profile::is_recording();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut recording, "Record").changed() {
            profile::set_recording(recording);
        }
        let mut follow = view.chosen.is_none();
        if ui
            .checkbox(&mut follow, "Follow the latest frame")
            .changed()
        {
            view.chosen = if follow {
                None
            } else {
                view.frames.lock().latest_frame().map(|f| f.frame_index())
            };
            view.zoom = None;
        }
        ui.selectable_value(&mut view.tab, Tab::Flame, "Flame graph");
        ui.selectable_value(&mut view.tab, Tab::Table, "Scopes");
    });
    let (frames, scopes) = {
        let frames = view.frames.lock();
        let recent: Vec<Arc<FrameData>> = frames.recent_frames().cloned().collect();
        (recent, frames.scope_collection().clone())
    };
    if frames.is_empty() {
        ui.label(if recording {
            "Waiting for the first frame..."
        } else {
            "Nothing recorded yet: tick Record (or start with --profile)."
        });
        return;
    }
    frame_bars(ui, view, &frames);
    let frame = match view.chosen {
        Some(index) => frames.iter().find(|f| f.frame_index() == index),
        None => frames.last(),
    };
    let Some(frame) = frame else {
        ui.label("That frame is no longer kept.");
        return;
    };
    // Unpacking can fail only when puffin's `packing` feature is on.
    #[allow(irrefutable_let_patterns)]
    let Ok(unpacked) = frame.unpacked() else {
        ui.label("This frame could not be read.");
        return;
    };
    let start = unpacked.meta.range_ns.0;
    ui.label(format!(
        "Frame {}: {:.2} ms, {} scopes",
        frame.frame_index(),
        ms(unpacked.duration_ns()),
        unpacked.meta.num_scopes,
    ));
    for (thread, stream) in &unpacked.thread_streams {
        if unpacked.thread_streams.len() > 1 {
            ui.label(format!("Thread {}", thread.name));
        }
        match view.tab {
            Tab::Flame => flame(
                ui,
                view,
                &scopes,
                &stream.stream,
                start,
                unpacked.duration_ns(),
            ),
            Tab::Table => table(ui, &scopes, &stream.stream),
        }
    }
}

fn ms(ns: NanoSecond) -> f64 {
    ns as f64 / 1e6
}

/// The recent frames as bars, tallest the slowest; a click chooses one.
fn frame_bars(ui: &mut egui::Ui, view: &mut ProfilerView, frames: &[Arc<FrameData>]) {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 56.0), egui::Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(160));
    let longest = frames
        .iter()
        .map(|f| f.duration_ns())
        .max()
        .unwrap_or(1)
        .max(16_700_000);
    let bar = rect.width() / RECENT_FRAMES as f32;
    let first = RECENT_FRAMES.saturating_sub(frames.len());
    let hovered = response.hover_pos();
    let mut hovered_frame = None;
    for (i, frame) in frames.iter().enumerate() {
        let x = rect.left() + (first + i) as f32 * bar;
        let height = frame.duration_ns() as f32 / longest as f32 * rect.height();
        let bar_rect = egui::Rect::from_min_max(
            egui::pos2(x, rect.bottom() - height),
            egui::pos2(x + bar.max(1.0), rect.bottom()),
        );
        let chosen = view.chosen == Some(frame.frame_index());
        let column = egui::Rect::from_x_y_ranges(x..=x + bar, rect.y_range());
        if hovered.is_some_and(|p| column.contains(p)) {
            hovered_frame = Some(frame);
        }
        let color = if chosen {
            egui::Color32::WHITE
        } else if frame.duration_ns() <= 17_500_000 {
            egui::Color32::from_rgb(90, 200, 120)
        } else if frame.duration_ns() <= 34_000_000 {
            egui::Color32::from_rgb(240, 200, 80)
        } else {
            egui::Color32::from_rgb(240, 90, 80)
        };
        painter.rect_filled(bar_rect, 0.0, color);
    }
    if let Some(frame) = hovered_frame {
        painter.text(
            rect.left_top() + egui::vec2(4.0, 2.0),
            egui::Align2::LEFT_TOP,
            format!(
                "frame {}: {:.2} ms",
                frame.frame_index(),
                ms(frame.duration_ns())
            ),
            egui::FontId::monospace(11.0),
            egui::Color32::WHITE,
        );
        if response.clicked() {
            view.chosen = Some(frame.frame_index());
            view.zoom = None;
        }
    }
}

/// A colour per scope name, so a scope looks the same in every frame.
fn color_of(name: &str) -> egui::Color32 {
    let hash = name.bytes().fold(2_166_136_261_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(16_777_619)
    });
    let hue = (hash % 360) as f32 / 360.0;
    egui::ecolor::Hsva::new(hue, 0.45, 0.75, 1.0).into()
}

fn scope_name(scopes: &ScopeCollection, scope: &Scope<'_>) -> String {
    let name = scopes
        .fetch_by_id(&scope.id)
        .map_or_else(|| "?".to_owned(), |details| details.name().to_string());
    if scope.record.data.is_empty() {
        name
    } else {
        format!("{name} {}", scope.record.data)
    }
}

/// Scopes stacked by depth along the frame's time; a click zooms in on one.
fn flame(
    ui: &mut egui::Ui,
    view: &mut ProfilerView,
    scopes: &ScopeCollection,
    stream: &Stream,
    frame_start: NanoSecond,
    frame_duration: NanoSecond,
) {
    const ROW: f32 = 16.0;
    let (from, to) = view.zoom.unwrap_or((0, frame_duration.max(1)));
    let mut rows = Vec::new();
    collect(stream, 0, 0, &mut rows);
    let depth = rows.iter().map(|(d, _)| d + 1).max().unwrap_or(1);
    if view.zoom.is_some() && ui.button("Show the whole frame").clicked() {
        view.zoom = None;
    }
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, depth as f32 * ROW), egui::Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_black_alpha(160));
    let x_of = |ns: NanoSecond| {
        rect.left() + ((ns - frame_start - from) as f32 / (to - from) as f32) * rect.width()
    };
    let mut hovered = None;
    for (depth, scope) in &rows {
        let x0 = x_of(scope.record.start_ns).max(rect.left());
        let x1 = x_of(scope.record.stop_ns()).min(rect.right());
        if x1 < rect.left() || x0 > rect.right() || x1 - x0 < 0.5 {
            continue;
        }
        let y = rect.top() + *depth as f32 * ROW;
        let scope_rect = egui::Rect::from_min_max(egui::pos2(x0, y), egui::pos2(x1, y + ROW - 1.0));
        let name = scope_name(scopes, scope);
        painter.rect_filled(scope_rect, 1.0, color_of(&name));
        if scope_rect.width() > 40.0 {
            painter.with_clip_rect(scope_rect.intersect(rect)).text(
                scope_rect.left_center() + egui::vec2(3.0, 0.0),
                egui::Align2::LEFT_CENTER,
                &name,
                egui::FontId::proportional(11.0),
                egui::Color32::BLACK,
            );
        }
        if response.hover_pos().is_some_and(|p| scope_rect.contains(p)) {
            hovered = Some((name, *scope));
        }
    }
    match hovered {
        Some((name, scope)) => {
            ui.label(format!("{name}: {:.3} ms", ms(scope.record.duration_ns)));
            if response.clicked() {
                let start = scope.record.start_ns - frame_start;
                view.zoom = Some((start, start + scope.record.duration_ns.max(1)));
                if view.chosen.is_none() {
                    view.chosen = view.frames.lock().latest_frame().map(|f| f.frame_index());
                }
            }
        }
        None => {
            ui.label("Hover a scope for its time; click it to zoom in.");
        }
    }
}

/// Every scope of a stream with its depth, parents first.
fn collect<'s>(stream: &'s Stream, offset: u64, depth: usize, out: &mut Vec<(usize, Scope<'s>)>) {
    let Ok(reader) = Reader::with_offset(stream, offset) else {
        return;
    };
    for scope in reader.flatten() {
        out.push((depth, scope));
        collect(stream, scope.child_begin_position, depth + 1, out);
    }
}

/// Time spent in each scope name, without the time of the scopes inside it.
fn table(ui: &mut egui::Ui, scopes: &ScopeCollection, stream: &Stream) {
    #[derive(Default)]
    struct Row {
        own: NanoSecond,
        total: NanoSecond,
        calls: u32,
    }
    let mut rows: HashMap<String, Row> = HashMap::new();
    let mut all = Vec::new();
    collect(stream, 0, 0, &mut all);
    for (_, scope) in &all {
        let children: NanoSecond = Reader::with_offset(stream, scope.child_begin_position)
            .map(|r| r.flatten().map(|c| c.record.duration_ns).sum())
            .unwrap_or(0);
        let row = rows.entry(scope_name(scopes, scope)).or_default();
        row.own += scope.record.duration_ns - children;
        row.total += scope.record.duration_ns;
        row.calls += 1;
    }
    let mut rows: Vec<_> = rows.into_iter().collect();
    rows.sort_by(|a, b| b.1.own.cmp(&a.1.own).then_with(|| a.0.cmp(&b.0)));
    egui::ScrollArea::vertical()
        .max_height(320.0)
        .show(ui, |ui| {
            egui::Grid::new("cg_ui profiler scopes")
                .num_columns(4)
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Scope");
                    ui.strong("Own ms");
                    ui.strong("Total ms");
                    ui.strong("Calls");
                    ui.end_row();
                    for (name, row) in rows.iter().take(TABLE_ROWS) {
                        ui.label(name);
                        ui.monospace(format!("{:.3}", ms(row.own)));
                        ui.monospace(format!("{:.3}", ms(row.total)));
                        ui.monospace(row.calls.to_string());
                        ui.end_row();
                    }
                });
        });
}
