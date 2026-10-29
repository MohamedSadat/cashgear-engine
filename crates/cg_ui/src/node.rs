//! UI nodes: the [`Node`] component that places an element, the values it is built from,
//! and the [`ComputedNode`] the layout produces.

use bevy_ecs::prelude::*;
use cg_core::Rect;
use cg_core::glam::Vec2;
use cg_render::Visibility;
use serde::{Deserialize, Serialize};

/// A UI element: a rectangle placed in its parent node (or in the screen, for a node
/// without one), which may draw a background, a border, an image or text, and lays out
/// its own children.
///
/// Positions and sizes are in UI pixels, y down. With the default pixel-perfect camera the
/// screen is 640×360 UI pixels, scaled with the game; see
/// [`ScreenSpace`](cg_render::ScreenSpace).
///
/// In a parent with [`Layout::Free`] (the default), a node sits at its `anchor`: the
/// anchor point of the node goes to the same point of the parent's content box, then moves
/// by `offset`. In a [`Layout::Row`] or [`Layout::Column`] parent, nodes are placed one
/// after another and only `offset` applies.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::Color;
/// use cg_ui::{Align, Anchor, BackgroundColor, Edges, Layout, Node, Text, Val};
///
/// fn spawn_menu(mut commands: Commands) {
///     commands
///         .spawn((
///             Node {
///                 anchor: Anchor::Center,
///                 width: Val::Px(160.0),
///                 padding: Edges::all(8.0),
///                 layout: Layout::Column,
///                 gap: 4.0,
///                 align: Align::Stretch,
///                 ..default()
///             },
///             BackgroundColor(Color::hex(0x222034)),
///         ))
///         .with_children(|menu| {
///             menu.spawn(Text::new("Play"));
///             menu.spawn(Text::new("Quit"));
///         });
/// }
/// ```
///
/// In a right-to-left language (see [`Locale`](crate::Locale)) layouts are mirrored:
/// anchors, offsets, padding and rows run from the right. `direction` overrides that for
/// a node and its children.
///
/// In prefabs every field is optional: `"Node": (width: Px(64), anchor: TopRight)`.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[require(ComputedNode, Visibility)]
#[serde(default)]
pub struct Node {
    /// The width.
    pub width: Val,
    /// The height.
    pub height: Val,
    /// Where the node sits in a free-layout parent.
    pub anchor: Anchor,
    /// A move from where the layout puts the node, in UI pixels (y down). In right-to-left
    /// layouts, x is mirrored.
    pub offset: Vec2,
    /// Space between the node's edges and its content (children and text).
    pub padding: Edges,
    /// The width of the border drawn with [`BorderColor`](crate::BorderColor), inside the
    /// node.
    pub border: f32,
    /// How children are placed.
    pub layout: Layout,
    /// Space between children in a row or column.
    pub gap: f32,
    /// Where children go across a row or column.
    pub align: Align,
    /// How a row's or column's children share leftover space along it.
    pub justify: Justify,
    /// The reading direction of this node and its children.
    pub direction: LayoutDirection,
}

impl Default for Node {
    fn default() -> Self {
        Self {
            width: Val::Auto,
            height: Val::Auto,
            anchor: Anchor::TopLeft,
            offset: Vec2::ZERO,
            padding: Edges::ZERO,
            border: 0.0,
            layout: Layout::Free,
            gap: 0.0,
            align: Align::Start,
            justify: Justify::Start,
            direction: LayoutDirection::Inherit,
        }
    }
}

impl Node {
    /// A node covering its whole parent (or the screen), for the root of a menu or HUD.
    pub fn fill() -> Self {
        Self {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..Self::default()
        }
    }

    /// A node of a fixed size in UI pixels.
    pub fn sized(width: f32, height: f32) -> Self {
        Self {
            width: Val::Px(width),
            height: Val::Px(height),
            ..Self::default()
        }
    }

    /// The same node at another anchor.
    pub fn at(mut self, anchor: Anchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// The same node moved by `offset`.
    pub fn with_offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }
}

/// A size along one axis.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Val {
    /// The content's size: the text, the image, or what the children need, plus padding.
    #[default]
    Auto,
    /// A fixed number of UI pixels.
    Px(f32),
    /// A share of the parent's content box: `Percent(50.0)` is half of it.
    Percent(f32),
    /// Along a row or column: an equal share of the space its fixed-size siblings leave.
    /// Across one, and in a free layout: the whole of the parent's content box.
    Fill,
}

/// A point of a rectangle, as a fraction of its size from the top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum Anchor {
    /// The top-left corner. The default.
    #[default]
    TopLeft,
    /// The middle of the top edge.
    Top,
    /// The top-right corner.
    TopRight,
    /// The middle of the left edge.
    Left,
    /// The centre.
    Center,
    /// The middle of the right edge.
    Right,
    /// The bottom-left corner.
    BottomLeft,
    /// The middle of the bottom edge.
    Bottom,
    /// The bottom-right corner.
    BottomRight,
    /// Any point: `(0, 0)` is the top-left corner, `(1, 1)` the bottom-right.
    Fraction(Vec2),
}

impl Anchor {
    /// The anchor as a fraction of a rectangle's size from its top-left corner.
    pub fn fraction(self) -> Vec2 {
        match self {
            Self::TopLeft => Vec2::new(0.0, 0.0),
            Self::Top => Vec2::new(0.5, 0.0),
            Self::TopRight => Vec2::new(1.0, 0.0),
            Self::Left => Vec2::new(0.0, 0.5),
            Self::Center => Vec2::new(0.5, 0.5),
            Self::Right => Vec2::new(1.0, 0.5),
            Self::BottomLeft => Vec2::new(0.0, 1.0),
            Self::Bottom => Vec2::new(0.5, 1.0),
            Self::BottomRight => Vec2::new(1.0, 1.0),
            Self::Fraction(f) => f,
        }
    }
}

/// Distances from each edge of a rectangle, in UI pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Edges {
    /// From the left edge (the right edge in right-to-left layouts).
    pub left: f32,
    /// From the right edge (the left edge in right-to-left layouts).
    pub right: f32,
    /// From the top edge.
    pub top: f32,
    /// From the bottom edge.
    pub bottom: f32,
}

impl Edges {
    /// No space.
    pub const ZERO: Self = Self::all(0.0);

    /// The same distance on every side.
    pub const fn all(value: f32) -> Self {
        Self {
            left: value,
            right: value,
            top: value,
            bottom: value,
        }
    }

    /// `horizontal` on the left and right, `vertical` on the top and bottom.
    pub const fn axes(horizontal: f32, vertical: f32) -> Self {
        Self {
            left: horizontal,
            right: horizontal,
            top: vertical,
            bottom: vertical,
        }
    }

    /// Left plus right, and top plus bottom.
    pub fn total(&self) -> Vec2 {
        Vec2::new(self.left + self.right, self.top + self.bottom)
    }
}

/// How a node places its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Layout {
    /// Each child at its own anchor and offset. The default.
    #[default]
    Free,
    /// Side by side, from the start edge (left, or right in right-to-left layouts).
    Row,
    /// One under another, from the top.
    Column,
}

/// Where children go across a row (vertically) or a column (horizontally).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Align {
    /// The top of a row, the start side of a column. The default.
    #[default]
    Start,
    /// The middle.
    Center,
    /// The bottom of a row, the end side of a column.
    End,
    /// Stretched across, for children whose size there is `Auto`.
    Stretch,
}

/// How the children of a row or column use the space they do not fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Justify {
    /// Packed at the start. The default.
    #[default]
    Start,
    /// Packed in the middle.
    Center,
    /// Packed at the end.
    End,
    /// The first at the start, the last at the end, the rest evenly between.
    SpaceBetween,
}

/// The reading direction of a node and its children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LayoutDirection {
    /// The parent's, or the language's at the root. The default.
    #[default]
    Inherit,
    /// Left to right, whatever the language.
    LeftToRight,
    /// Right to left, whatever the language.
    RightToLeft,
}

/// Draw order for a node and its children against other UI: higher on top. Nodes
/// without one take their parent's; roots without one are at 0. Within one z-index,
/// later roots and later children draw on top.
///
/// In prefabs: `"ZIndex": (10)`.
#[derive(
    Component,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    Serialize,
    Deserialize,
)]
pub struct ZIndex(pub i32);

/// Where the layout put a [`Node`] last frame. Added with `Node`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default)]
pub struct ComputedNode {
    pub(crate) rect: Rect,
    pub(crate) content: Rect,
    pub(crate) layer: i32,
    pub(crate) order: u32,
    pub(crate) rtl: bool,
    pub(crate) visible: bool,
}

impl ComputedNode {
    /// The node's rectangle in UI pixels from the screen's top-left corner, y down.
    pub fn rect(&self) -> Rect {
        self.rect
    }

    /// The rectangle inside the padding.
    pub fn content(&self) -> Rect {
        self.content
    }

    /// The node's size.
    pub fn size(&self) -> Vec2 {
        self.rect.size()
    }

    /// Whether it lays out right to left.
    pub fn is_rtl(&self) -> bool {
        self.rtl
    }

    /// Whether it is drawn: its [`Visibility`] and its parents' allow it, and it is not a
    /// hidden touch control.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Its draw order: higher z-index layers first, then depth-first order.
    pub fn stack(&self) -> (i32, u32) {
        (self.layer, self.order)
    }
}
