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
//! [`tasks::spawn_local`] runs async work (such as GPU setup) without threads, [`Args`]
//! reads launch flags from the command line or the page URL, and [`Target`] says which kind
//! of platform the game was built for. [`io::AssetIo`] reads game files from disk, over
//! `fetch` or from the APK, and [`io::FileWatcher`] reports changed files on desktop for
//! hot reloading. [`storage::Storage`] keeps small text values such as settings between
//! runs: in files on desktop and Android, in `localStorage` on the web;
//! [`StoragePlugin`](storage::StoragePlugin) inserts it. [`page`] sends events to the web
//! page around the game (the first frame is drawn, the game failed), and
//! [`io::Bundle`] packs a whole asset folder into one file for the web and Android.
//!
//! On Android the `android` module (built for Android only) takes the app from the
//! entry point (`cashgear_engine::android_main!`) and reads what the engine's Java
//! activity, `CashGearActivity`, records: back presses and the display cutout's safe area.
//!
//! Games read input as named actions, bound to keys, buttons and touch:
//!
//! ```
//! use cg_core::prelude::*;
//! use cg_platform::input::{ActionMap, ActionState, GamepadButton, InputPlugin, KeyCode};
//!
//! fn jump(actions: Res<ActionState>) {
//!     if actions.just_pressed("jump") {
//!         info!("jump!");
//!     }
//! }
//!
//! App::new()
//!     .add_plugins(InputPlugin { gamepads: false })
//!     .insert_resource(
//!         ActionMap::new()
//!             .bind("jump", KeyCode::Space)
//!             .bind("jump", GamepadButton::South),
//!     )
//!     .add_systems(FixedUpdate, jump)
//!     .run();
//! ```

#[cfg(target_os = "android")]
pub mod android;
mod args;
pub mod input;
pub mod io;
mod log;
pub mod page;
mod runner;
pub mod storage;
mod target;
pub mod tasks;
mod window;

pub use crate::args::Args;
pub use crate::log::{LogCapture, LogLine, LogPlugin};
pub use crate::target::Target;
pub use window::{Lifecycle, LifecycleChanged, Window, WindowConfig, WindowHandle, WindowPlugin};

/// Common imports: the plugins, window types and input types.
pub mod prelude {
    pub use crate::input::{
        ActionMap, ActionState, AxisDirection, Binding, ButtonInput, GamepadAxis, GamepadButton,
        GamepadId, Gamepads, InputPlugin, InputSystems, KeyCode, MouseButton, PointerId, Pointers,
        ScreenRegion, TextInput, VirtualInput,
    };
    pub use crate::storage::{Storage, StoragePlugin};
    pub use crate::{
        Args, Lifecycle, LifecycleChanged, LogPlugin, Target, Window, WindowConfig, WindowPlugin,
    };
}
