//! Platform layer of CashGear Engine: `winit` glue, input state, per-target file and
//! asset IO, save storage and app lifecycle.
//!
//! This is the only engine crate allowed to use `#[cfg(target_os/arch)]`.
//!
//! | Plugin | Adds |
//! | --- | --- |
//! | [`LogPlugin`] | Log output to the terminal or the browser console |
//! | [`InputPlugin`](input::InputPlugin) | Keyboard, mouse, pointer, gamepad and action state |
//! | [`WindowPlugin`] | The window, [`Lifecycle`] messages and the runner that drives frames |
//!
//! Asset IO and storage arrive in later phases.

pub mod input;
mod log;
mod runner;
mod window;

pub use crate::log::LogPlugin;
pub use window::{Lifecycle, Window, WindowConfig, WindowPlugin};

/// Common imports: the plugins, window types and input types.
pub mod prelude {
    pub use crate::input::{
        ActionMap, ActionState, AxisDirection, Binding, ButtonInput, GamepadAxis, GamepadButton,
        GamepadId, Gamepads, InputPlugin, KeyCode, MouseButton, PointerId, Pointers, ScreenRegion,
    };
    pub use crate::{Lifecycle, LogPlugin, Window, WindowConfig, WindowPlugin};
}
