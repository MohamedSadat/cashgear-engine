//! The game window, its lifecycle messages and the plugin that runs the frame loop.

use cg_core::ecs::prelude::*;
use cg_core::ecs::schedule::ScheduleLabel;
use cg_core::glam::{UVec2, Vec2};
use cg_core::{App, Plugin, Rect};
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
    /// Desktop only: F11 and Alt+Enter toggle fullscreen. On the web the browser's own F11
    /// fills the screen; games offer [`Window::set_fullscreen`] in a menu there.
    pub fullscreen_keys: bool,
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
            fullscreen_keys: true,
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
    pub(crate) user_activated: bool,
    pub(crate) touch_screen: bool,
    pub(crate) fullscreen: bool,
    pub(crate) can_fullscreen: bool,
    /// A change asked for by the game, applied by the runner after the frame.
    pub(crate) fullscreen_request: Option<bool>,
    /// Physical pixels at each edge covered by a display cutout: left, top, right, bottom.
    pub(crate) safe_insets: [u32; 4],
    /// Physical pixels at each edge kept for a tool drawn over the window (an editor's
    /// panels): left, top, right, bottom.
    pub(crate) view_insets: [u32; 4],
    /// Closing the window asks the app (`Lifecycle::CloseRequested`) instead of exiting.
    pub(crate) close_guard: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            physical_size: UVec2::ZERO,
            scale_factor: 1.0,
            focused: false,
            occluded: false,
            suspended: true,
            user_activated: false,
            touch_screen: false,
            fullscreen: false,
            can_fullscreen: false,
            fullscreen_request: None,
            safe_insets: [0; 4],
            view_insets: [0; 4],
            close_guard: false,
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
            user_activated: true,
            touch_screen: false,
            fullscreen: false,
            can_fullscreen: true,
            fullscreen_request: None,
            safe_insets: [0; 4],
            view_insets: [0; 4],
            close_guard: false,
        }
    }

    /// The same window, marked as being on a touch screen or not (see
    /// [`touch_screen`](Self::touch_screen)), for tests.
    pub fn with_touch_screen(mut self, touch_screen: bool) -> Self {
        self.touch_screen = touch_screen;
        self
    }

    /// The same window with a display cutout covering `left`, `top`, `right` and `bottom`
    /// physical pixels at its edges (see [`safe_area`](Self::safe_area)), for tests.
    pub fn with_safe_insets(mut self, left: u32, top: u32, right: u32, bottom: u32) -> Self {
        self.safe_insets = [left, top, right, bottom];
        self
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

    /// The part of the window that no display cutout (a notch, a punch-hole camera) covers,
    /// in physical pixels from the top-left. The renderer fits the camera's view inside it,
    /// so the bars cover the cutout. The whole window on desktop and the web; on Android
    /// it comes from the engine's activity (`CashGearActivity`) and changes when the
    /// window does.
    ///
    /// The [`view_insets`](Self::view_insets) a tool keeps at the edges count as covered
    /// too, so the view, the UI and pointer conversions fit between an editor's panels.
    pub fn safe_area(&self) -> Rect {
        let size = self.physical_size.as_vec2();
        let insets: [u32; 4] =
            core::array::from_fn(|i| self.safe_insets[i].max(self.view_insets[i]));
        let [left, top, right, bottom] = insets.map(|inset| inset as f32);
        let min = Vec2::new(left, top).min(size);
        let max = (size - Vec2::new(right, bottom)).max(min);
        Rect { min, max }
    }

    /// Physical pixels kept clear of the view at the left, top, right and bottom edges by a
    /// tool drawn over the window, such as the editor's panels. Zero unless a tool sets them.
    pub fn view_insets(&self) -> [u32; 4] {
        self.view_insets
    }

    /// Keeps `insets` physical pixels at the left, top, right and bottom edges clear of the
    /// view (see [`safe_area`](Self::safe_area)). The editor sets them to the space its
    /// panels take; the runner never changes them.
    pub fn set_view_insets(&mut self, insets: [u32; 4]) {
        self.view_insets = insets;
    }

    /// Whether closing the window asks the app first (see
    /// [`set_close_guard`](Self::set_close_guard)).
    pub fn close_guard(&self) -> bool {
        self.close_guard
    }

    /// While on, closing the window (its close button, Alt+F4) does not exit: the runner
    /// writes [`Lifecycle::CloseRequested`] and the app decides, writing `AppExit` when it
    /// is ready. The editor turns it on to ask about unsaved changes. Off by default.
    pub fn set_close_guard(&mut self, guard: bool) {
        self.close_guard = guard;
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

    /// Whether the user has pressed a key, clicked or touched the app yet. Browsers start
    /// sound only after that (their autoplay rules), so the audio waits for it. Always true
    /// on desktop and Android.
    pub fn user_activated(&self) -> bool {
        self.user_activated
    }

    /// Whether the device's main pointer is a finger: true on Android and in browsers
    /// whose primary pointer is coarse (phones and tablets), false on desktop. On-screen
    /// touch controls use it to show themselves from the start.
    pub fn touch_screen(&self) -> bool {
        self.touch_screen
    }

    /// Whether the game fills the screen: a borderless fullscreen window on desktop, the
    /// canvas in the browser's fullscreen mode on the web, always on Android (immersive,
    /// with the system bars hidden). It follows changes made outside the game too, such as
    /// Escape leaving fullscreen in a browser.
    pub fn fullscreen(&self) -> bool {
        self.fullscreen_request.unwrap_or(self.fullscreen)
    }

    /// Whether [`set_fullscreen`](Self::set_fullscreen) can work here: on desktop, and in
    /// browsers that let a page go fullscreen (not Safari on iPhone). Hide a fullscreen
    /// setting where this is false. False on Android, where games are always fullscreen.
    pub fn can_fullscreen(&self) -> bool {
        self.can_fullscreen
    }

    /// Enters or leaves fullscreen after this frame. On the web, browsers allow entering
    /// it only within a few seconds of a key press, click or touch, so call it while
    /// handling one (reacting to a menu button's `Clicked` message is fine); otherwise the
    /// browser refuses and [`fullscreen`](Self::fullscreen) turns false again.
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        if self.can_fullscreen {
            self.fullscreen_request = Some(fullscreen);
        }
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
///
/// On Android, leaving the app (home, another app, the screen turning off) sends
/// `Focused(false)` and then `Suspended`; coming back sends `Resumed` and `Focused(true)`.
/// Android's back button arrives as a key instead:
/// [`KeyCode::BrowserBack`](crate::input::KeyCode::BrowserBack).
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
    /// The user asked to close the window while [`Window::close_guard`] is on; the app
    /// exits only if it writes `AppExit`.
    CloseRequested,
}

/// A schedule the runner runs at once when the app is suspended or resumed, or the window
/// is hidden or shown, after updating [`Window`] and writing the [`Lifecycle`] message.
///
/// It exists because no frame may follow: an Android app in the background and a hidden
/// browser tab stop getting frames, so a system in [`Update`](cg_core::Update) would only
/// see the change once the app is back. Keep its systems short: pause the audio, save the
/// game. Read the state from [`Window`] (`suspended`, `occluded`), not from `Lifecycle`
/// messages, which the next frame's systems read too.
#[derive(ScheduleLabel, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LifecycleChanged;

/// Opens the window and makes it drive the app's frames on desktop, web and Android.
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
