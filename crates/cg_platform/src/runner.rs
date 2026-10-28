//! The `winit` runner: owns the event loop and the window, feeds window and input events
//! into the app, and runs one app frame per redraw.
//!
//! Desktop: `run_app` blocks until the app exits. An optional `max_fps` cap waits with
//! `ControlFlow::WaitUntil` between frames. Web: `spawn_app` hands the loop to the browser
//! and returns at once; `request_redraw` maps to `requestAnimationFrame`, which paces the
//! frames.

// The runner is the one place that reads the clock. `web_time::Instant` is the browser's
// `performance.now()` on the web and `std::time::Instant` on desktop.
#![allow(clippy::disallowed_methods)]

use crate::input::{ButtonInput, KeyCode, MouseButton, PointerId, Pointers};
use crate::window::{Lifecycle, Window, WindowConfig};
use cg_core::ecs::change_detection::Mut;
use cg_core::glam::{UVec2, Vec2};
use cg_core::{App, AppExit};
use core::time::Duration;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window as WinitWindow, WindowAttributes, WindowId};

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
    window: Option<WinitWindow>,
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

    /// Time between frames when the desktop frame cap is on.
    fn frame_interval(&self) -> Option<Duration> {
        if cfg!(target_arch = "wasm32") {
            return None;
        }
        self.config
            .max_fps
            .filter(|fps| fps.is_finite() && *fps > 0.0)
            .map(|fps| Duration::from_secs_f64(1.0 / fps))
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

    fn exit(&mut self, event_loop: &ActiveEventLoop, exit: AppExit) {
        self.exit = Some(exit);
        event_loop.exit();
    }

    /// Runs one app frame.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        if self.suspended {
            return;
        }
        let now = Instant::now();
        let delta = self
            .last_frame
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
        self.last_frame = Some(now);

        self.app.update(delta);

        if let Some(exit) = self.app.should_exit() {
            log::info!("exiting: {exit:?}");
            self.exit(event_loop, exit);
            return;
        }
        if self.frame_interval().is_none()
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
    }

    fn pointer_position(&self, position: PhysicalPosition<f64>) -> (Vec2, Vec2) {
        let window = self.app.world().resource::<Window>();
        let scale = window.scale_factor();
        let logical = Vec2::new((position.x / scale) as f32, (position.y / scale) as f32);
        (logical, window.logical_size())
    }

    fn on_input(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(key) = event.physical_key else {
                    return;
                };
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
            let mut state = self.window_mut();
            state.physical_size = size;
            state.scale_factor = scale_factor;
            state.focused = focused;
            self.window = Some(window);
        }
        self.suspended = false;
        self.last_frame = None;
        self.window_mut().suspended = false;
        self.lifecycle(Lifecycle::Resumed);
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.suspended = true;
        self.window_mut().suspended = true;
        self.lifecycle(Lifecycle::Suspended);
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
                self.window_mut().physical_size = UVec2::new(width, height);
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
                self.window_mut().occluded = occluded;
                self.lifecycle(Lifecycle::Occluded(occluded));
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
            // Each frame requests the next redraw; vsync or the browser paces them.
            None => event_loop.set_control_flow(ControlFlow::Wait),
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
