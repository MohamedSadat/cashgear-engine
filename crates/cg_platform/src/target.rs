//! Which kind of platform the game was built for.

/// The kind of platform the game runs on, for the few choices games make by platform
/// (a Quit button, the wording of a hint). Engine and game code read it instead of using
/// `#[cfg]` themselves.
///
/// ```
/// use cg_platform::Target;
///
/// // A Quit button only where there is somewhere to quit to.
/// let show_quit = Target::CURRENT.can_quit();
/// # let _ = show_quit;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// Windows, Linux or macOS.
    Desktop,
    /// A browser (WebAssembly).
    Web,
    /// An Android app.
    Android,
}

impl Target {
    /// The platform this build is for.
    #[cfg(target_arch = "wasm32")]
    pub const CURRENT: Self = Self::Web;
    /// The platform this build is for.
    #[cfg(target_os = "android")]
    pub const CURRENT: Self = Self::Android;
    /// The platform this build is for.
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    pub const CURRENT: Self = Self::Desktop;

    /// Whether the game can close itself. On the web `AppExit` stops the game but leaves
    /// the page, so games hide their Quit buttons there.
    pub const fn can_quit(self) -> bool {
        !matches!(self, Self::Web)
    }
}
