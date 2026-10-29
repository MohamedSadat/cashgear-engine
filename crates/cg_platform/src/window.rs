//! The game window, its lifecycle messages and the plugin that runs the frame loop.

use cg_core::ecs::prelude::*;
use cg_core::glam::{UVec2, Vec2};
use cg_core::{App, Plugin};
use std::sync::Arc;

/// Settings for the game window.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowConfig {
    /// Window title; the page title on the web.
    pub title: String,
    /// Initial width in logical pixels. Ignored on the web, where the canvas fills the space
    /// the page gives it.
    pub width: u32,
    /// Initial height in logical pixels. Ignored on the web.
    pub height: u32,
    /// Whether the user can resize the window.
    pub resizable: bool,
    /// Web only: the id of an existing `<canvas>` to draw into. When `None` or not found, a
    /// canvas is appended to the page body.
    pub canvas_id: Option<String>,
    /// Desktop only: the most frames per second to run. `None` relies on the renderer's
    /// vsync; without a renderer (or with vsync off) an uncapped loop runs flat out. While the
    /// window is hidden or has no area, frames are capped at 60 per second either way. On
    /// the web the browser paces frames with `requestAnimationFrame`.
    pub max_fps: Option<f64>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "CashGear".to_owned(),
            width: 1280,
            height: 720,
            resizable: true,
            canvas_id: None,
            max_fps: None,
        }
    }
}

/// The window's current state. Updated by the runner before each frame.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct Window {
    pub(crate) physical_size: UVec2,
    pub(crate) scale_factor: f64,
    pub(crate) focused: bool,
    pub(crate) occluded: bool,
    pub(crate) suspended: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            physical_size: UVec2::ZERO,
            scale_factor: 1.0,
            focused: false,
            occluded: false,
            suspended: true,
        }
    }
}

impl Window {
    /// A focused, running window of the given size, for tests and tools that work without
    /// a real window. The runner keeps the real one up to date.
    pub fn with_size(physical_size: UVec2, scale_factor: f64) -> Self {
        Self {
            physical_size,
            scale_factor,
            focused: true,
            occluded: false,
            suspended: false,
        }
    }

    /// Size in physical pixels (what the renderer draws).
    pub fn physical_size(&self) -> UVec2 {
        self.physical_size
    }

    /// Size in logical pixels (what layout and pointer positions use).
    pub fn logical_size(&self) -> Vec2 {
        self.physical_size.as_vec2() / self.scale_factor as f32
    }

    /// Physical pixels per logical pixel (the device pixel ratio on the web).
    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    /// Whether the window has keyboard focus.
    pub fn focused(&self) -> bool {
        self.focused
    }

    /// Whether the window is hidden: minimized, fully covered, or a background browser tab.
    pub fn occluded(&self) -> bool {
        self.occluded
    }

    /// Whether the app is suspended (Android background). No frames run while suspended.
    pub fn suspended(&self) -> bool {
        self.suspended
    }
}

/// A shared handle to the native window, for creating a GPU surface.
///
/// The runner inserts it when the window is created, before the first frame. The window
/// lives as long as the app; on Android its native surface comes and goes with
/// [`Lifecycle::Suspended`] and [`Lifecycle::Resumed`], so recreate GPU surfaces then.
#[derive(Resource, Clone, Debug)]
pub struct WindowHandle(pub(crate) Arc<winit::window::Window>);

impl WindowHandle {
    /// The `winit` window. It implements the `raw-window-handle` traits that `wgpu` needs.
    pub fn winit(&self) -> &Arc<winit::window::Window> {
        &self.0
    }
}

/// Window and app lifecycle changes, written by the runner. Read them with a
/// `MessageReader<Lifecycle>` in [`Update`](cg_core::Update).
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub enum Lifecycle {
    /// The app can run and draw: at start, and when returning from the background on
    /// Android.
    Resumed,
    /// The app went to the background (Android) and no frames run until `Resumed`.
    Suspended,
    /// The window gained (`true`) or lost (`false`) keyboard focus. Held keys and buttons
    /// are released on focus loss.
    Focused(bool),
    /// The window became hidden (`true`) or visible again (`false`).
    Occluded(bool),
    /// The window's size changed, in physical pixels.
    Resized {
        /// New width in physical pixels.
        width: u32,
        /// New height in physical pixels.
        height: u32,
    },
    /// The scale factor changed, for example when the window moved to another monitor.
    ScaleFactorChanged {
        /// New physical pixels per logical pixel.
        scale_factor: f64,
    },
}

/// Opens the window and makes it drive the app's frames on desktop and web.
///
/// Add [`InputPlugin`](crate::input::InputPlugin) too, so the runner has input resources to
/// feed.
#[derive(Debug, Clone, Default)]
pub struct WindowPlugin {
    /// Window settings.
    pub window: WindowConfig,
}

impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        let config = self.window.clone();
        app.init_resource::<Window>()
            .add_message::<Lifecycle>()
            .set_runner(move |app| crate::runner::run(app, config));
    }
}
