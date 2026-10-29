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
//! | [`StoragePlugin`](storage::StoragePlugin) | The [`Storage`](storage::Storage) resource for saves and settings |
//!
//! [`tasks::spawn_local`] runs async work (such as GPU setup) without threads, and [`Args`]
//! reads launch flags from the command line or the page URL. [`io::AssetIo`] reads game
//! files from disk, over `fetch` or (from Phase 11) from the APK, and [`io::FileWatcher`]
//! reports changed files on desktop for hot reloading. [`storage::Storage`] keeps small
//! text values such as settings between runs: in files on desktop, in `localStorage` on the
//! web; [`StoragePlugin`](storage::StoragePlugin) inserts it.

mod args;
pub mod input;
pub mod io;
mod log;
mod runner;
pub mod storage;
pub mod tasks;
mod window;

pub use crate::args::Args;
pub use crate::log::LogPlugin;
pub use window::{Lifecycle, LifecycleChanged, Window, WindowConfig, WindowHandle, WindowPlugin};

/// Common imports: the plugins, window types and input types.
pub mod prelude {
    pub use crate::input::{
        ActionMap, ActionState, AxisDirection, Binding, ButtonInput, GamepadAxis, GamepadButton,
        GamepadId, Gamepads, InputPlugin, KeyCode, MouseButton, PointerId, Pointers, ScreenRegion,
    };
    pub use crate::storage::{Storage, StoragePlugin};
    pub use crate::{
        Args, Lifecycle, LifecycleChanged, LogPlugin, Window, WindowConfig, WindowPlugin,
    };
}
