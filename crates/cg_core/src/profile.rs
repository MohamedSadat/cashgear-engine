//! Profiling. The engine's systems and hot paths are marked with scopes from the
//! [`profiling`] crate, which compile to nothing unless a backend is chosen with a feature:
//!
//! - `profile`: [puffin](https://github.com/EmbarkStudios/puffin). Scopes are recorded while
//!   [`set_recording`] is on (`--profile` turns it on from the start), one puffin frame per
//!   app frame, and the debug overlay's Profiler window shows them on every target.
//! - `profile-tracy`: [Tracy](https://github.com/wolfpld/tracy), desktop only. Scopes are
//!   always recorded; connect with the Tracy profiler while the game runs.
//!
//! Game code marks its own systems the same way, through the re-exported crate:
//!
//! ```
//! use cg_core::prelude::*;
//! use cg_core::profiling;
//!
//! #[profiling::function]
//! fn steer(time: Res<Time>) {
//!     profiling::scope!("think");
//!     let _ = time.delta_secs();
//! }
//! ```
//!
//! wgpu marks its own work with the same crate, so its scopes show up too.

pub use profiling;

/// Whether this build records anything: the `profile` or `profile-tracy` feature.
pub const ENABLED: bool = cfg!(any(feature = "profile", feature = "profile-tracy"));

/// Starts or stops recording puffin scopes (with the `profile` feature; otherwise this does
/// nothing). Tracy records all the time.
pub fn set_recording(on: bool) {
    #[cfg(feature = "profile")]
    puffin::set_scopes_on(on);
    #[cfg(not(feature = "profile"))]
    let _ = on;
}

/// Whether puffin scopes are being recorded.
pub fn is_recording() -> bool {
    #[cfg(feature = "profile")]
    return puffin::are_scopes_on();
    #[cfg(not(feature = "profile"))]
    false
}

/// Starts the Tracy client, if the build has it. Called once when an app is made.
pub(crate) fn start() {
    #[cfg(feature = "profile-tracy")]
    profiling::tracy_client::Client::start();
}

/// Ends the profiler's frame. Called by [`App::update`](crate::App::update) after `Last`.
#[inline]
pub(crate) fn finish_frame() {
    #[cfg(feature = "profile")]
    if puffin::are_scopes_on() {
        puffin::GlobalProfiler::lock().new_frame();
    }
    #[cfg(feature = "profile-tracy")]
    profiling::tracy_client::frame_mark();
}
