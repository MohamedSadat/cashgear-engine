//! UI of CashGear Engine: text, the runtime game UI, focus navigation, on-screen touch
//! controls, localization and the egui debug overlay.
//!
//! The UI is made of entities with a [`Node`], arranged by the `ChildOf` hierarchy:
//!
//! | Component | Does |
//! | --- | --- |
//! | [`Node`] | Size, anchor, offset, padding and how children are laid out (free, row, column) |
//! | [`Text`] | Text in a font, size and colour, aligned and wrapped in the node |
//! | [`BackgroundColor`], [`BorderColor`] | A filled rectangle and a border |
//! | [`UiImage`] | An image or part of one, optionally nine-sliced |
//! | [`Button`] | Clickable by pointer, touch, keyboard and gamepad; sends [`Clicked`] |
//! | [`Slider`], [`ProgressBar`] | A value set by dragging or keys, a value shown by a [`BarFill`] child |
//! | [`StateColors`], [`StateImages`] | A widget's look when hovered, focused, pressed or disabled |
//! | [`TouchButton`], [`TouchDpad`] | On-screen controls that press actions on touch screens |
//! | [`Localized`] | Text from the current language's [`Strings`] |
//! | [`ZIndex`] | Draw order between menus |
//!
//! Layout happens in UI pixels, y down, from the top-left corner of the camera's view; with
//! the default pixel-perfect camera that is 640×360 UI pixels scaled by a whole number
//! with the game, so pixel-art panels and pixel fonts stay sharp. The UI is drawn after the
//! world, over the whole window at its own resolution, and the screen fade covers it.
//!
//! [`Text`] uses [`Font`] assets: outline fonts are shaped by cosmic-text (Arabic joins,
//! mixed directions are ordered, missing characters fall back to other loaded fonts);
//! bitmap fonts in BMFont's format draw as drawn. Pixel fonts are rasterized at UI pixels
//! without smoothing; other fonts at the screen's resolution. The engine has one built-in
//! font, Pixelify Sans (SIL OFL 1.1), sharp at 11 UI pixels and multiples.
//!
//! The arrow keys and WASD, the d-pad and left stick move the focus between widgets
//! ([`UiFocus`]); Enter, Space and the south button click; see [`actions`].
//!
//! With the `debug-overlay` feature (on by default), F12 opens an egui overlay with frame
//! times, render stats, the entity list and a component inspector, and debug drawing
//! toggles. With the `profile` feature as well, its Profiler window shows what the engine's
//! profiling scopes recorded: recent frames, a flame graph and the slowest scopes.
//!
//! ```no_run
//! use cg_core::prelude::*;
//! use cg_platform::prelude::*;
//! use cg_render::prelude::*;
//! use cg_ui::prelude::*;
//!
//! fn setup(mut commands: Commands) {
//!     commands.spawn(Camera2D::default());
//!     commands
//!         .spawn((Node {
//!             anchor: Anchor::Center,
//!             layout: Layout::Column,
//!             gap: 6.0,
//!             ..default()
//!         },))
//!         .with_children(|menu| {
//!             menu.spawn((Button::default(), BackgroundColor(Color::hex(0x3b5dc9)), Text::new("Play")));
//!         });
//! }
//!
//! App::new()
//!     .add_plugins(InputPlugin::default())
//!     .add_plugins(WindowPlugin::default())
//!     .add_plugins(RenderPlugin::default())
//!     .add_plugins(UiPlugin::default())
//!     .add_systems(Startup, setup)
//!     .run();
//! ```

mod draw;
mod focus;
mod font;
mod interaction;
mod layout;
mod locale;
mod node;
#[cfg(feature = "debug-overlay")]
mod overlay;
#[cfg(all(feature = "debug-overlay", feature = "profile"))]
mod profiler;
mod text;
mod touch;
mod widgets;

pub use focus::{UiFocus, actions};
pub use font::{
    BitmapFont, BitmapGlyph, DEFAULT_FONT_SIZE, Font, FontError, FontFile, FontKind, FontLoader,
    FontSettings, OutlineFont,
};
pub use interaction::UiPointer;
pub use layout::UiStack;
pub use locale::{Locale, LocaleSettings, Localized, Strings, StringsFile, StringsLoader};
pub use node::{
    Align, Anchor, ComputedNode, Edges, Justify, Layout, LayoutDirection, Node, Val, ZIndex,
};
#[cfg(feature = "debug-overlay")]
pub use overlay::{DebugOverlay, EguiContext, InspectExt, Inspector, egui};
pub use text::{PositionedGlyph, Text, TextAlign, TextData, TextLayout, TextStyle, TextSystem};
pub use touch::{TouchButton, TouchControl, TouchControls, TouchDpad, TouchMode};
pub use widgets::{
    BackgroundColor, BarFill, BorderColor, Button, Clicked, Focusable, Interaction, ProgressBar,
    Slider, SliderChanged, SliderThumb, StateColors, StateImages, UiImage, UiImageData,
};

use bevy_ecs::prelude::*;
use cg_assets::{AppAssetExt, Assets};
use cg_core::{App, Last, Plugin, PostUpdate, PreUpdate, Render};
use cg_platform::Args;
use cg_platform::input::{InputPlugin, InputSystems, KeyCode};
use cg_render::{RenderPlugin, RenderSystems, VisibilitySystems};
use cg_world::AppWorldExt;

/// The UI's system sets.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiSystems {
    /// `PreUpdate`, before the actions: on-screen touch controls press theirs.
    Touch,
    /// `PreUpdate`, after the actions: pointers and focus act on widgets, which then show
    /// their state. [`Clicked`] and [`SliderChanged`] are ready for `Update`.
    Input,
    /// `PostUpdate`, after visibility: localized text, bars, then the layout.
    Layout,
    /// `Render`, in `RenderSystems::Queue`: the UI becomes screen-space sprites.
    Draw,
}

/// Adds the UI: fonts and text, nodes and their layout, widgets, focus navigation, touch
/// controls, localization and (with the `debug-overlay` feature) the debug overlay.
///
/// Add it after the input and render plugins (it adds `RenderPlugin` if missing);
/// `DefaultPlugins` includes it.
#[derive(Debug, Clone, PartialEq)]
pub struct UiPlugin {
    /// The game's languages; `None` for no localization (then [`Localized`] text shows
    /// its ids).
    pub locale: Option<LocaleSettings>,
    /// When the on-screen touch controls show. `--touch` makes it `Always`, `--no-touch`
    /// `Never`.
    pub touch: TouchMode,
    /// Bind the [`actions`] the game's action map lacks.
    pub default_bindings: bool,
    /// The key that opens and closes the debug overlay (with the `debug-overlay`
    /// feature).
    pub overlay_key: Option<KeyCode>,
    /// Start with the debug overlay open. `--overlay` turns it on.
    pub overlay_open: bool,
    /// Start with the overlay's Profiler window open (with the `profile` feature).
    /// `--profile` turns it on, and the overlay with it.
    pub profiler_open: bool,
}

impl Default for UiPlugin {
    fn default() -> Self {
        Self {
            locale: None,
            touch: TouchMode::Auto,
            default_bindings: true,
            overlay_key: Some(KeyCode::F12),
            overlay_open: false,
            profiler_open: false,
        }
    }
}

impl UiPlugin {
    /// Applies the `--touch`, `--no-touch`, `--overlay` and `--profile` launch flags.
    pub fn with_args(mut self, args: &Args) -> Self {
        if args.flag("touch") {
            self.touch = TouchMode::Always;
        }
        if args.flag("no-touch") {
            self.touch = TouchMode::Never;
        }
        if args.flag("overlay") {
            self.overlay_open = true;
        }
        if args.flag("profile") {
            self.overlay_open = true;
            self.profiler_open = true;
        }
        self
    }
}

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<InputPlugin>() {
            app.add_plugins(InputPlugin { gamepads: false });
        }
        if !app.is_plugin_added::<RenderPlugin>() {
            app.add_plugins(RenderPlugin::default());
        }
        app.init_asset::<Font>()
            .register_loader(FontLoader)
            .init_asset::<Strings>()
            .register_loader(StringsLoader)
            .init_resource::<TextSystem>()
            .init_resource::<UiStack>()
            .init_resource::<UiFocus>()
            .init_resource::<UiPointer>()
            .insert_resource(TouchControls::new(self.touch))
            .add_message::<Clicked>()
            .add_message::<SliderChanged>()
            .register_component::<Node>("Node")
            .register_component::<BackgroundColor>("BackgroundColor")
            .register_component::<BorderColor>("BorderColor")
            .register_component::<ZIndex>("ZIndex")
            .register_component::<Button>("Button")
            .register_component::<Focusable>("Focusable")
            .register_component::<StateColors>("StateColors")
            .register_component::<StateImages>("StateImages")
            .register_component::<Slider>("Slider")
            .register_component::<ProgressBar>("ProgressBar")
            .register_component::<BarFill>("BarFill")
            .register_component::<SliderThumb>("SliderThumb")
            .register_component::<TouchButton>("TouchButton")
            .register_component::<TouchDpad>("TouchDpad")
            .register_component::<TouchControl>("TouchControl")
            .register_component::<Localized>("Localized")
            .register_prefab_component::<Text>("Text")
            .register_prefab_component::<UiImage>("UiImage");
        app.world_mut()
            .resource_mut::<Assets<Font>>()
            .insert(Font::DEFAULT, Font::default_font());

        let locale = match &self.locale {
            Some(settings) => {
                if let Some(key) = &settings.storage_key {
                    app.insert_resource(locale::SavedLanguage {
                        key: key.clone(),
                        language: String::new(),
                    })
                    .add_systems(
                        Last,
                        locale::save_language.run_if(resource_exists::<locale::SavedLanguage>),
                    );
                }
                let locale = locale::setup_locale(app.world_mut(), settings);
                if let Some(mut saved) = app.world_mut().get_resource_mut::<locale::SavedLanguage>()
                {
                    // Starting in the saved language is not a change worth writing.
                    saved.language = locale.language().to_owned();
                }
                locale
            }
            None => Locale::default(),
        };
        app.insert_resource(locale);

        if self.default_bindings {
            focus::bind_ui_actions(
                &mut app
                    .world_mut()
                    .resource_mut::<cg_platform::input::ActionMap>(),
            );
            app.add_systems(
                PreUpdate,
                focus::keep_ui_bindings.before(InputSystems::Actions),
            );
        }
        app.configure_sets(
            PreUpdate,
            (
                UiSystems::Touch.before(InputSystems::Actions),
                UiSystems::Input.after(InputSystems::Actions),
            ),
        )
        .configure_sets(
            PostUpdate,
            UiSystems::Layout.after(VisibilitySystems::Propagate),
        )
        .configure_sets(
            Render,
            UiSystems::Draw
                .in_set(RenderSystems::Queue)
                .after(RenderSystems::View),
        )
        .add_systems(
            PreUpdate,
            (
                touch::update_touch_controls.in_set(UiSystems::Touch),
                (
                    interaction::interact,
                    focus::navigate,
                    interaction::update_interactions,
                    widgets::apply_state_looks,
                )
                    .chain()
                    .in_set(UiSystems::Input),
            ),
        )
        .add_systems(
            PostUpdate,
            (
                locale::update_locale,
                locale::apply_localized,
                widgets::update_bars,
                layout::layout_ui,
            )
                .chain()
                .in_set(UiSystems::Layout),
        )
        .add_systems(Render, draw::draw_ui.in_set(UiSystems::Draw));

        #[cfg(feature = "debug-overlay")]
        overlay::build(app, self.overlay_key, self.overlay_open, self.profiler_open);
    }
}

/// Common imports: the plugin, nodes, text, fonts, widgets, focus, touch controls and
/// localization.
pub mod prelude {
    pub use crate::{
        Align, Anchor, BackgroundColor, BarFill, BorderColor, Button, Clicked, ComputedNode, Edges,
        Focusable, Font, Interaction, Justify, Layout, LayoutDirection, Locale, LocaleSettings,
        Localized, Node, ProgressBar, Slider, SliderChanged, SliderThumb, StateColors, StateImages,
        Text, TextAlign, TouchButton, TouchControls, TouchDpad, TouchMode, UiFocus, UiImage,
        UiPlugin, UiPointer, Val, ZIndex,
    };
}

#[cfg(test)]
mod tests;
