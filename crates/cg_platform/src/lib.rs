//! Platform layer of CashGear Engine: `winit` glue, input state, per-target file and
//! asset IO, save storage and app lifecycle.
//!
//! This is the only engine crate allowed to use `#[cfg(target_os/arch)]`.
//!
//! | Plugin | Adds |
//! | --- | --- |
//! | [`LogPlugin`] | Log output to the terminal or the browser console |
//! | [`InputPlugin`](input::InputPlugin) | Keyboard, mouse, pointer, gamepad and action state |
//! | [`WindowPlugin`] | The window, its [`WindowHandle`], [`Lifecycle`] messages and the runner that drives frames |
//!
//! [`tasks::spawn_local`] runs async work (such as GPU setup) without threads, and [`Args`]
//! reads launch flags from the command line or the page URL. Asset IO and storage arrive in
//! later phases.

mod args;
pub mod input;
mod log;
mod runner;
pub mod tasks;
mod window;

pub use crate::args::Args;
pub use crate::log::LogPlugin;
pub use window::{Lifecycle, Window, WindowConfig, WindowHandle, WindowPlugin};

/// Common imports: the plugins, window types and input types.
pub mod prelude {
    pub use crate::input::{
        ActionMap, ActionState, AxisDirection, Binding, ButtonInput, GamepadAxis, GamepadButton,
        GamepadId, Gamepads, InputPlugin, KeyCode, MouseButton, PointerId, Pointers, ScreenRegion,
    };
    pub use crate::{Args, Lifecycle, LogPlugin, Window, WindowConfig, WindowPlugin};
}
