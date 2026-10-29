//! Text: the [`Text`] component, its [`TextStyle`] and [`TextAlign`], and the computed
//! [`TextLayout`].

mod atlas;
mod engine;

pub(crate) use engine::TextRequest;
pub use engine::TextSystem;

use crate::Node;
use crate::font::{DEFAULT_FONT_SIZE, Font};
use bevy_ecs::prelude::*;
use cg_assets::{AssetId, AssetServer, Handle};
use cg_core::Rect;
use cg_core::glam::Vec2;
use cg_render::{Color, Image};
use cg_world::PrefabComponent;
use serde::{Deserialize, Serialize};

/// Text drawn in a UI [`Node`]: a string, its [`TextStyle`], how lines align and whether
/// they wrap. The node sizes itself to the text unless its width or height is set; lines
/// wrap at the node's width.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::Color;
/// use cg_ui::{Anchor, Node, Text, TextAlign};
///
/// fn spawn_title(mut commands: Commands) {
///     commands.spawn((
///         Node {
///             anchor: Anchor::Top,
///             offset: vec2(0.0, 24.0),
///             ..default()
///         },
///         Text::new("CashGear")
///             .with_size(33.0)
///             .with_color(Color::hex(0xffcc33))
///             .with_align(TextAlign::Center),
///     ));
/// }
/// ```
///
/// Outline fonts are shaped by cosmic-text: Arabic letters join, mixed left-to-right and
/// right-to-left text is ordered correctly, and characters missing from the font come from
/// another loaded font. [`Localized`](crate::Localized) fills the string from the current
/// language.
///
/// In prefabs the font is an asset path: `"Text": (value: "Play", font: "fonts/ui.font.ron",
/// size: 22)`.
#[derive(Component, Debug, Clone, PartialEq)]
#[require(Node, TextLayout)]
pub struct Text {
    /// The string.
    pub value: String,
    /// Font, size and colour.
    pub style: TextStyle,
    /// How lines line up in the node.
    pub align: TextAlign,
    /// Whether lines wrap at the node's width (at spaces, or anywhere for one long word).
    pub wrap: bool,
}

impl Default for Text {
    fn default() -> Self {
        Self {
            value: String::new(),
            style: TextStyle::default(),
            align: TextAlign::Start,
            wrap: true,
        }
    }
}

impl Text {
    /// Text in the default style: the built-in font at its native size, in white.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            ..Self::default()
        }
    }

    /// The same text in another font.
    pub fn with_font(mut self, font: Handle<Font>) -> Self {
        self.style.font = font;
        self
    }

    /// The same text at another size, in UI pixels.
    pub fn with_size(mut self, size: f32) -> Self {
        self.style.size = size;
        self
    }

    /// The same text in another colour.
    pub fn with_color(mut self, color: Color) -> Self {
        self.style.color = color;
        self
    }

    /// The same text aligned differently.
    pub fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// The same text, wrapping or not.
    pub fn with_wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }
}

/// The look of [`Text`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// The font. The default handle is the built-in font.
    pub font: Handle<Font>,
    /// The size (the font's em) in UI pixels.
    pub size: f32,
    /// The colour.
    pub color: Color,
    /// The distance between lines as a multiple of the size; `None` uses the font's.
    pub line_height: Option<f32>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: Handle::default(),
            size: DEFAULT_FONT_SIZE,
            color: Color::WHITE,
            line_height: None,
        }
    }
}

/// How lines of [`Text`] line up in their node. `Start` and `End` follow the node's
/// direction, so start-aligned text is on the right in a right-to-left language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TextAlign {
    /// The start of the line: left, or right in right-to-left layouts. The default.
    #[default]
    Start,
    /// Centred.
    Center,
    /// The end of the line: right, or left in right-to-left layouts.
    End,
    /// Always left.
    Left,
    /// Always right.
    Right,
    /// Stretched to both edges (outline fonts only; others start-align).
    Justified,
}

/// One glyph of laid-out text, relative to the text's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionedGlyph {
    /// The image the glyph is drawn from (a glyph atlas page or a bitmap font's page).
    pub texture: AssetId<Image>,
    /// Where on the image, in texels.
    pub source: Rect,
    /// Where it goes, in UI pixels from the text's top-left corner, y down.
    pub rect: Rect,
    /// Its colour.
    pub color: Color,
}

/// What laid-out [`Text`] became: its glyphs and size. Computed by the UI layout; added
/// with `Text`.
#[derive(Component, Debug, Clone, Default)]
pub struct TextLayout {
    pub(crate) glyphs: Vec<PositionedGlyph>,
    pub(crate) size: Vec2,
    /// The size without wrapping, and wrapped sizes measured for widths.
    pub(crate) natural: Option<Vec2>,
    pub(crate) wrapped: Vec<(f32, Vec2)>,
    pub(crate) key: Option<LayoutKey>,
}

/// What a text layout depends on besides the [`Text`] itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LayoutKey {
    pub(crate) width: f32,
    pub(crate) scale: f32,
    pub(crate) revision: u64,
    pub(crate) rtl: bool,
}

impl TextLayout {
    /// The glyphs, relative to the text's top-left corner.
    pub fn glyphs(&self) -> &[PositionedGlyph] {
        &self.glyphs
    }

    /// The laid-out size in UI pixels.
    pub fn size(&self) -> Vec2 {
        self.size
    }

    /// Forgets everything, so the text is measured and laid out again.
    pub(crate) fn invalidate(&mut self) {
        self.natural = None;
        self.wrapped.clear();
        self.key = None;
    }
}

/// The prefab form of [`Text`]: the font by asset path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextData {
    /// See [`Text::value`].
    pub value: String,
    /// The font's asset path; empty for the built-in font.
    pub font: String,
    /// See [`TextStyle::size`].
    pub size: f32,
    /// See [`TextStyle::color`].
    pub color: Color,
    /// See [`TextStyle::line_height`].
    pub line_height: Option<f32>,
    /// See [`Text::align`].
    pub align: TextAlign,
    /// See [`Text::wrap`].
    pub wrap: bool,
}

impl Default for TextData {
    fn default() -> Self {
        let text = Text::default();
        Self {
            value: String::new(),
            font: String::new(),
            size: text.style.size,
            color: text.style.color,
            line_height: None,
            align: text.align,
            wrap: text.wrap,
        }
    }
}

impl PrefabComponent for Text {
    type Data = TextData;

    fn from_data(data: &TextData, world: &World) -> Result<Self, String> {
        let font = if data.font.is_empty() {
            Handle::default()
        } else {
            world
                .get_resource::<AssetServer>()
                .ok_or_else(|| format!("no asset server to load `{}`", data.font))?
                .load(data.font.as_str())
        };
        Ok(Self {
            value: data.value.clone(),
            style: TextStyle {
                font,
                size: data.size,
                color: data.color,
                line_height: data.line_height,
            },
            align: data.align,
            wrap: data.wrap,
        })
    }
}
