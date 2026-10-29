//! The `winit` runner: owns the event loop and the window, feeds window and input events
//! into the app, and runs one app frame per redraw.
//!
//! Desktop: `run_app` blocks until the app exits. Vsync in the renderer paces the frames;
//! an optional `max_fps` cap, and a 60 fps cap while the window is hidden, wait with
//! `ControlFlow::WaitUntil` between frames instead. Web: `spawn_app` hands the loop to the
//! browser and returns at once; `request_redraw` maps to `requestAnimationFrame`, which
//! paces the frames.

// The runner is the one place that reads the clock. `web_time::Instant` is the browser's
// `performance.now()` on the web and `std::time::Instant` on desktop.
#![allow(clippy::disallowed_methods)]

use crate::input::{ButtonInput, KeyCode, MouseButton, PointerId, Pointers, TextInput};
use crate::window::{Lifecycle, LifecycleChanged, Window, WindowConfig, WindowHandle};
use cg_core::ecs::change_detection::Mut;
use cg_core::glam::{UVec2, Vec2};
use cg_core::{App, AppExit};
use core::time::Duration;
use std::sync::Arc;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Fullscreen, Window as WinitWindow, WindowAttributes, WindowId};

/// Frame rate on desktop while the window is hidden or has no area. Nothing is presented
/// then, so vsync cannot pace the loop.
const IDLE_FPS: f64 = 60.0;

/// Runs `app` in a window until it exits.
pub(crate) fn run(app: App, config: WindowConfig) -> AppExit {
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(err) => {
            log::error!("cannot start the event loop: {err}");
            return AppExit::error();
        }
    };
    run_event_loop(event_loop, Runner::new(app, config))
}

#[cfg(not(target_arch = "wasm32"))]
fn run_event_loop(event_loop: EventLoop<()>, mut runner: Runner) -> AppExit {
    if let Err(err) = event_loop.run_app(&mut runner) {
        log::error!("event loop failed: {err}");
        return AppExit::error();
    }
    runner.exit.unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
fn run_event_loop(event_loop: EventLoop<()>, runner: Runner) -> AppExit {
    use winit::platform::web::EventLoopExtWebSys;
    event_loop.spawn_app(runner);
    AppExit::Success
}

struct Runner {
    app: App,
    config: WindowConfig,
    window: Option<Arc<WinitWindow>>,
    suspended: bool,
    last_frame: Option<Instant>,
    next_frame: Instant,
    exit: Option<AppExit>,
}

impl Runner {
    fn new(app: App, config: WindowConfig) -> Self {
        Self {
            app,
            config,
            window: None,
            suspended: true,
            last_frame: None,
            next_frame: Instant::now(),
            exit: None,
        }
    }

    /// Time between frames when a desktop frame cap applies: the configured `max_fps`, or
    /// [`IDLE_FPS`] while the window is hidden or has no area.
    fn frame_interval(&self) -> Option<Duration> {
        if cfg!(target_arch = "wasm32") {
            return None;
        }
        let window = self.app.world().resource::<Window>();
        let idle = window.occluded || window.physical_size.cmpeq(UVec2::ZERO).any();
        let max_fps = self
            .config
            .max_fps
            .filter(|fps| fps.is_finite() && *fps > 0.0);
        let fps = match (max_fps, idle) {
            (Some(fps), true) => Some(fps.min(IDLE_FPS)),
            (Some(fps), false) => Some(fps),
            (None, true) => Some(IDLE_FPS),
            (None, false) => None,
        };
        fps.map(|fps| Duration::from_secs_f64(1.0 / fps))
    }

    fn window_attributes(&self) -> WindowAttributes {
        let attributes = WinitWindow::default_attributes()
            .with_title(self.config.title.clone())
            .with_resizable(self.config.resizable);
        platform_attributes(attributes, &self.config)
    }

    fn window_mut(&mut self) -> Mut<'_, Window> {
        self.app.world_mut().resource_mut::<Window>()
    }

    fn lifecycle(&mut self, message: Lifecycle) {
        self.app.world_mut().write_message(message);
    }

    /// Runs [`LifecycleChanged`] now, since no frame may follow a suspend or a hidden tab.
    fn lifecycle_changed(&mut self) {
        // Apps without systems in the schedule do not have it.
        let _ = self.app.world_mut().try_run_schedule(LifecycleChanged);
    }

    fn exit(&mut self, event_loop: &ActiveEventLoop, exit: AppExit) {
        self.exit = Some(exit);
        event_loop.exit();
    }

    /// Runs one app frame.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        if self.suspended {
            return;
        }
        if !self.app.world().resource::<Window>().user_activated
            && platform_user_activation() == Some(true)
        {
            self.user_activated();
        }
        self.sync_fullscreen();
        let now = Instant::now();
        let delta = self
            .last_frame
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
        self.last_frame = Some(now);

        self.app.update(delta);

        if self
            .app
            .world()
            .resource::<Window>()
            .fullscreen_request
            .is_some()
            && let Some(fullscreen) = self.window_mut().fullscreen_request.take()
        {
            self.set_fullscreen(fullscreen);
        }
        if let Some(exit) = self.app.should_exit() {
            log::info!("exiting: {exit:?}");
            if !exit.is_success() && cfg!(target_arch = "wasm32") {
                crate::page::fatal("the game stopped because of an error");
            }
            self.exit(event_loop, exit);
        }
    }

    /// Copies the window's fullscreen state into [`Window`], which also catches changes
    /// made outside the game (Escape in a browser, the window manager).
    fn sync_fullscreen(&mut self) {
        let Some(window) = &self.window else {
            return;
        };
        let fullscreen = window.fullscreen().is_some();
        if self.app.world().resource::<Window>().fullscreen != fullscreen {
            self.window_mut().fullscreen = fullscreen;
        }
    }

    fn set_fullscreen(&mut self, fullscreen: bool) {
        let Some(window) = &self.window else {
            return;
        };
        log::info!(
            "{} fullscreen",
            if fullscreen { "entering" } else { "leaving" }
        );
        // `Window::fullscreen` follows at the start of the next frame (a browser switches a
        // moment later).
        window.set_fullscreen(fullscreen.then_some(Fullscreen::Borderless(None)));
    }

    /// Desktop: F11 and Alt+Enter toggle fullscreen (see [`WindowConfig::fullscreen_keys`]).
    /// Returns whether the key was used for that, so the game does not see it too (Enter
    /// would also press the focused menu button).
    fn fullscreen_key(&mut self, key: KeyCode, state: ElementState, repeat: bool) -> bool {
        if !self.config.fullscreen_keys
            || cfg!(target_arch = "wasm32")
            || cfg!(target_os = "android")
        {
            return false;
        }
        let alt = self
            .app
            .world()
            .get_resource::<ButtonInput<KeyCode>>()
            .is_some_and(|keys| keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]));
        let toggle = key == KeyCode::F11 || (key == KeyCode::Enter && alt);
        if toggle && state == ElementState::Pressed && !repeat {
            let fullscreen = !self.app.world().resource::<Window>().fullscreen;
            self.set_fullscreen(fullscreen);
        }
        toggle
    }

    fn pointer_position(&self, position: PhysicalPosition<f64>) -> (Vec2, Vec2) {
        let window = self.app.world().resource::<Window>();
        let scale = window.scale_factor();
        let logical = Vec2::new((position.x / scale) as f32, (position.y / scale) as f32);
        (logical, window.logical_size())
    }

    /// Records the first user input that browsers count as activation (see
    /// [`Window::user_activated`]).
    fn user_activated(&mut self) {
        if !self.app.world().resource::<Window>().user_activated {
            log::debug!("user activation: audio may start");
            self.window_mut().user_activated = true;
        }
    }

    fn on_input(&mut self, event: WindowEvent) {
        // The inputs browsers count as user activation: a key press other than Escape, a
        // mouse button press, and the end of a touch.
        let activates = match &event {
            WindowEvent::KeyboardInput { event, .. } => {
                event.state == ElementState::Pressed
                    && event.physical_key != PhysicalKey::Code(KeyCode::Escape)
            }
            WindowEvent::MouseInput { state, .. } => *state == ElementState::Pressed,
            WindowEvent::Touch(touch) => touch.phase == TouchPhase::Ended,
            _ => false,
        };
        // Where the browser keeps its own record, trust that instead: synthetic events
        // (from scripts) look like input here but do not let a page start sound.
        if activates && platform_user_activation().is_none() {
            self.user_activated();
        }
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && let Some(text) = &event.text
                    && let Some(mut typed) = self.app.world_mut().get_resource_mut::<TextInput>()
                {
                    typed.push(text);
                }
                let PhysicalKey::Code(key) = event.physical_key else {
                    return;
                };
                if self.fullscreen_key(key, event.state, event.repeat) {
                    return;
                }
                if let Some(mut keys) = self
                    .app
                    .world_mut()
                    .get_resource_mut::<ButtonInput<KeyCode>>()
                {
                    match event.state {
                        ElementState::Pressed => keys.press(key),
                        ElementState::Released => keys.release(key),
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let world = self.app.world_mut();
                if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
                    match state {
                        ElementState::Pressed => mouse.press(button),
                        ElementState::Released => mouse.release(button),
                    }
                }
                if button == MouseButton::Left
                    && let Some(mut pointers) = world.get_resource_mut::<Pointers>()
                {
                    match state {
                        ElementState::Pressed => pointers.press(PointerId::Mouse),
                        ElementState::Released => pointers.release(PointerId::Mouse),
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (position, size) = self.pointer_position(position);
                if let Some(mut pointers) = self.app.world_mut().get_resource_mut::<Pointers>() {
                    pointers.move_to(PointerId::Mouse, position, size);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if let Some(mut pointers) = self.app.world_mut().get_resource_mut::<Pointers>() {
                    pointers.mouse_left();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self.app.world().resource::<Window>().scale_factor();
                if let Some(mut pointers) = self.app.world_mut().get_resource_mut::<Pointers>() {
                    match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            pointers.scroll_lines(Vec2::new(x, y));
                        }
                        MouseScrollDelta::PixelDelta(pixels) => {
                            let logical = pixels.to_logical::<f32>(scale);
                            pointers.scroll_pixels(Vec2::new(logical.x, logical.y));
                        }
                    }
                }
            }
            WindowEvent::Touch(touch) => {
                let id = PointerId::Touch(touch.id);
                let (position, size) = self.pointer_position(touch.location);
                if let Some(mut pointers) = self.app.world_mut().get_resource_mut::<Pointers>() {
                    pointers.move_to(id, position, size);
                    match touch.phase {
                        TouchPhase::Started => pointers.press(id),
                        TouchPhase::Moved => {}
                        TouchPhase::Ended | TouchPhase::Cancelled => pointers.release(id),
                    }
                }
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window = match event_loop.create_window(self.window_attributes()) {
                Ok(window) => window,
                Err(err) => {
                    log::error!("cannot create the window: {err}");
                    self.exit(event_loop, AppExit::error());
                    return;
                }
            };
            let window = Arc::new(window);
            let PhysicalSize { width, height } = window.inner_size();
            let size = match platform_window_created(&window) {
                Some(measured) if width == 0 || height == 0 => measured,
                _ => UVec2::new(width, height),
            };
            let scale_factor = window.scale_factor();
            let focused = window.has_focus();
            log::info!(
                "window created: {}x{} physical pixels, scale factor {scale_factor}",
                size.x,
                size.y
            );
            platform_resized(&window, size);
            let mut state = self.window_mut();
            state.physical_size = size;
            state.scale_factor = scale_factor;
            state.focused = focused;
            // Only browsers hold sound back until the user has interacted.
            state.user_activated = !cfg!(target_arch = "wasm32");
            state.touch_screen = platform_touch_screen();
            state.can_fullscreen = platform_can_fullscreen();
            state.fullscreen = window.fullscreen().is_some();
            self.app
                .world_mut()
                .insert_resource(WindowHandle(window.clone()));
            self.window = Some(window);
        }
        self.suspended = false;
        self.last_frame = None;
        self.window_mut().suspended = false;
        self.lifecycle(Lifecycle::Resumed);
        self.lifecycle_changed();
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.suspended = true;
        self.window_mut().suspended = true;
        self.lifecycle(Lifecycle::Suspended);
        self.lifecycle_changed();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::RedrawRequested => self.frame(event_loop),
            WindowEvent::CloseRequested => {
                log::info!("window closed");
                self.exit(event_loop, AppExit::Success);
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                let size = UVec2::new(width, height);
                if let Some(window) = &self.window {
                    platform_resized(window, size);
                }
                self.window_mut().physical_size = size;
                self.lifecycle(Lifecycle::Resized { width, height });
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.window_mut().scale_factor = scale_factor;
                self.lifecycle(Lifecycle::ScaleFactorChanged { scale_factor });
            }
            WindowEvent::Focused(focused) => {
                self.window_mut().focused = focused;
                if !focused {
                    // Key-up events go to whichever window has focus now; release
                    // everything so no key stays stuck down.
                    let world = self.app.world_mut();
                    if let Some(mut keys) = world.get_resource_mut::<ButtonInput<KeyCode>>() {
                        keys.release_all();
                    }
                    if let Some(mut mouse) = world.get_resource_mut::<ButtonInput<MouseButton>>() {
                        mouse.release_all();
                    }
                    if let Some(mut pointers) = world.get_resource_mut::<Pointers>() {
                        pointers.release_all();
                    }
                }
                self.lifecycle(Lifecycle::Focused(focused));
            }
            WindowEvent::Occluded(occluded) => {
                // A hidden browser tab gets no frames at all; without this, the first frame
                // back would count the whole time away (clamped to a quarter second, which
                // still lurches the game forward).
                self.last_frame = None;
                self.window_mut().occluded = occluded;
                self.lifecycle(Lifecycle::Occluded(occluded));
                self.lifecycle_changed();
            }
            other => self.on_input(other),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = &self.window else {
            return;
        };
        if self.suspended {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        match self.frame_interval() {
            Some(interval) => {
                let now = Instant::now();
                if now >= self.next_frame {
                    window.request_redraw();
                    self.next_frame += interval;
                    if self.next_frame <= now {
                        self.next_frame = now + interval;
                    }
                }
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
            }
            // Ask for the next frame at once; vsync or the browser paces them.
            None => {
                window.request_redraw();
                event_loop.set_control_flow(ControlFlow::Wait);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn platform_attributes(attributes: WindowAttributes, config: &WindowConfig) -> WindowAttributes {
    attributes.with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height))
}

#[cfg(target_arch = "wasm32")]
fn platform_attributes(attributes: WindowAttributes, config: &WindowConfig) -> WindowAttributes {
    use wasm_bindgen::JsCast;
    use winit::platform::web::WindowAttributesExtWebSys;

    let canvas = config.canvas_id.as_deref().and_then(|id| {
        let canvas = web_sys::window()?
            .document()?
            .get_element_by_id(id)?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .ok();
        if canvas.is_none() {
            log::warn!("no <canvas id=\"{id}\"> found; appending a new canvas");
        }
        canvas
    });
    // The page's CSS sizes the canvas; winit reports its size through `Resized`.
    match canvas {
        Some(canvas) => attributes.with_canvas(Some(canvas)),
        None => attributes.with_append(true),
    }
}

/// Per-platform setup after the window exists. Returns the measured size in physical
/// pixels when the platform knows it before winit does.
#[cfg(not(target_arch = "wasm32"))]
fn platform_window_created(_window: &WinitWindow) -> Option<UVec2> {
    None
}

/// Whether the device's main pointer is a finger (see [`Window::touch_screen`]).
#[cfg(not(target_arch = "wasm32"))]
fn platform_touch_screen() -> bool {
    cfg!(target_os = "android")
}

/// `matchMedia("(pointer: coarse)")`: the primary pointer is a finger (phones, tablets).
#[cfg(target_arch = "wasm32")]
fn platform_touch_screen() -> bool {
    web_sys::window()
        .and_then(|window| window.match_media("(pointer: coarse)").ok().flatten())
        .is_some_and(|query| query.matches())
}

/// Per-platform work when the window's size in physical pixels changes.
#[cfg(not(target_arch = "wasm32"))]
fn platform_resized(_window: &WinitWindow, _size: UVec2) {}

/// Desktop windows can always go fullscreen; Android's immersive mode is Phase 11's.
#[cfg(not(target_arch = "wasm32"))]
fn platform_can_fullscreen() -> bool {
    !cfg!(target_os = "android")
}

/// `document.fullscreenEnabled` (or Safari's prefixed one): false on iPhone, where only
/// videos go fullscreen, and in frames not allowed to (an `<iframe>` without
/// `allowfullscreen`).
#[cfg(target_arch = "wasm32")]
fn platform_can_fullscreen() -> bool {
    use js_sys::Reflect;
    use wasm_bindgen::JsValue;

    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return false;
    };
    ["fullscreenEnabled", "webkitFullscreenEnabled"]
        .iter()
        .any(|name| {
            Reflect::get(&document, &JsValue::from_str(name))
                .ok()
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
        })
}

/// The platform's own record of user activation, if it keeps one. Desktop has none (and
/// needs none: the window starts activated).
#[cfg(not(target_arch = "wasm32"))]
fn platform_user_activation() -> Option<bool> {
    None
}

/// `navigator.userActivation.hasBeenActive`, where the browser has it (Chrome, Edge,
/// Firefox 120+, Safari 16.4+).
#[cfg(target_arch = "wasm32")]
fn platform_user_activation() -> Option<bool> {
    use js_sys::Reflect;
    use wasm_bindgen::JsValue;

    let window = web_sys::window()?;
    let navigator = Reflect::get(&window, &JsValue::from_str("navigator")).ok()?;
    let activation = Reflect::get(&navigator, &JsValue::from_str("userActivation"))
        .ok()
        .filter(JsValue::is_object)?;
    Reflect::get(&activation, &JsValue::from_str("hasBeenActive"))
        .ok()?
        .as_bool()
}

#[cfg(target_arch = "wasm32")]
fn platform_resized(window: &WinitWindow, size: UVec2) {
    use winit::platform::web::WindowExtWebSys;

    // CSS sizes the canvas element, but its drawing buffer keeps its own size (300x150 by
    // default). WebGPU resizes it when the surface is configured; WebGL2 does not.
    if let Some(canvas) = window.canvas()
        && size.x > 0
        && size.y > 0
    {
        canvas.set_width(size.x);
        canvas.set_height(size.y);
    }
}

#[cfg(target_arch = "wasm32")]
fn platform_window_created(window: &WinitWindow) -> Option<UVec2> {
    use winit::platform::web::WindowExtWebSys;

    // Give the canvas keyboard focus so key presses reach the game without a click first.
    window.focus_window();
    // winit learns the canvas size from a ResizeObserver a moment later. Measure the canvas
    // now so `Startup` systems already see the real size.
    let canvas = window.canvas()?;
    let scale = window.scale_factor();
    let width = (f64::from(canvas.client_width()) * scale).round() as u32;
    let height = (f64::from(canvas.client_height()) * scale).round() as u32;
    (width > 0 && height > 0).then(|| UVec2::new(width, height))
}
