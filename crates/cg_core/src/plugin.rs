//! The [`Plugin`] trait.

use crate::App;

/// A reusable piece of app setup: resources, messages, systems or a runner.
///
/// Engine subsystems are plugins, and games use them to split their own setup:
///
/// ```
/// use cg_core::prelude::*;
///
/// #[derive(Resource, Default)]
/// struct Score(u32);
///
/// struct ScorePlugin;
///
/// impl Plugin for ScorePlugin {
///     fn build(&self, app: &mut App) {
///         app.init_resource::<Score>();
///     }
/// }
///
/// App::new().add_plugins(ScorePlugin);
/// ```
pub trait Plugin: 'static {
    /// Configures the app.
    fn build(&self, app: &mut App);

    /// A unique name. Adding two plugins with the same name panics. Defaults to the type name.
    fn name(&self) -> &str {
        core::any::type_name::<Self>()
    }
}
