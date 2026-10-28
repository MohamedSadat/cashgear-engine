//! CashGear Engine: a 2D game engine for desktop, web and Android.
//!
//! This facade crate re-exports every engine subsystem under a short module name and
//! provides a [`prelude`] for game code:
//!
//! ```
//! use cashgear_engine::prelude::*;
//! ```

pub use cg_anim as anim;
pub use cg_assets as assets;
pub use cg_audio as audio;
pub use cg_core as core;
pub use cg_physics as physics;
pub use cg_platform as platform;
pub use cg_render as render;
#[cfg(feature = "script")]
pub use cg_script as script;
pub use cg_tiled as tiled;
pub use cg_ui as ui;
pub use cg_world as world;

/// The engine version, taken from the crate manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Common imports for game code.
///
/// Empty until Phase 1 adds the app builder, plugins and `Time`.
pub mod prelude {}

#[cfg(test)]
mod tests {
    use super::VERSION;

    #[test]
    fn version_is_semver() {
        let parts: Vec<_> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()));
    }
}
