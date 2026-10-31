//! Widgets: what a [`Node`] draws ([`BackgroundColor`], [`BorderColor`], [`UiImage`]) and
//! what it does ([`Button`], [`Slider`], [`ProgressBar`]).

use crate::Node;
use crate::node::{Anchor, Edges, Val};
use bevy_ecs::prelude::*;
use cg_assets::{AssetServer, Handle};
use cg_core::Rect;
use cg_render::{Color, Image};
use cg_world::PrefabComponent;
use serde::{Deserialize, Serialize};

/// Fills the node's rectangle with a colour.
///
/// In prefabs: `"BackgroundColor": ("#222034")`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[require(Node)]
pub struct BackgroundColor(pub Color);

/// Draws a border of the node's [`Node::border`] width inside its rectangle.
///
/// In prefabs: `"BorderColor": ("#ffffff")`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[require(Node)]
pub struct BorderColor(pub Color);

/// Draws an image (or part of one) over the node. With `slice` it is a nine-slice image:
/// the corners keep their size, the edges stretch along their side and the middle
/// stretches both ways, so one small panel image fits any size.
///
/// A node with an `Auto` size takes the image's size, one texel per UI pixel.
///
/// ```
/// use cg_assets::AssetServer;
/// use cg_core::prelude::*;
/// use cg_ui::{Edges, Node, UiImage, Val};
///
/// fn spawn_panel(mut commands: Commands, server: Res<AssetServer>) {
///     commands.spawn((
///         Node::sized(120.0, 80.0),
///         UiImage::new(server.load("images/panel.png")).sliced(Edges::all(4.0)),
///     ));
/// }
/// ```
///
/// In prefabs the texture is an asset path: `"UiImage": (texture: "images/panel.png",
/// slice: Some((left: 4, right: 4, top: 4, bottom: 4)))`.
#[derive(Component, Debug, Clone, PartialEq)]
#[require(Node)]
pub struct UiImage {
    /// The image.
    pub texture: Handle<Image>,
    /// The part of it to draw, in texels from the top-left; `None` for all of it.
    pub source: Option<Rect>,
    /// Multiplies the image's colours.
    pub color: Color,
    /// The nine-slice borders, in texels; `None` stretches the whole image.
    pub slice: Option<Edges>,
    /// Mirrors the image horizontally.
    pub flip_x: bool,
}

impl Default for UiImage {
    fn default() -> Self {
        Self {
            texture: Handle::default(),
            source: None,
            color: Color::WHITE,
            slice: None,
            flip_x: false,
        }
    }
}

impl UiImage {
    /// All of an image.
    pub fn new(texture: Handle<Image>) -> Self {
        Self {
            texture,
            ..Self::default()
        }
    }

    /// The same image cut from `source` texels.
    pub fn with_source(mut self, source: Rect) -> Self {
        self.source = Some(source);
        self
    }

    /// The same image as a nine-slice with these borders in texels.
    pub fn sliced(mut self, borders: Edges) -> Self {
        self.slice = Some(borders);
        self
    }

    /// The same image tinted.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

/// The prefab form of [`UiImage`]: the texture by asset path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiImageData {
    /// The image's asset path; empty for a white square.
    pub texture: String,
    /// See [`UiImage::source`].
    pub source: Option<Rect>,
    /// See [`UiImage::color`].
    pub color: Color,
    /// See [`UiImage::slice`].
    pub slice: Option<Edges>,
    /// See [`UiImage::flip_x`].
    pub flip_x: bool,
}

impl Default for UiImageData {
    fn default() -> Self {
        Self {
            texture: String::new(),
            source: None,
            color: Color::WHITE,
            slice: None,
            flip_x: false,
        }
    }
}

impl PrefabComponent for UiImage {
    type Data = UiImageData;

    fn from_data(data: &UiImageData, world: &World) -> Result<Self, String> {
        let texture = if data.texture.is_empty() {
            Handle::default()
        } else {
            world
                .get_resource::<AssetServer>()
                .ok_or_else(|| format!("no asset server to load `{}`", data.texture))?
                .load(data.texture.as_str())
        };
        Ok(Self {
            texture,
            source: data.source,
            color: data.color,
            slice: data.slice,
            flip_x: data.flip_x,
        })
    }
}

/// What the pointer and focus are doing with an interactive node ([`Button`], [`Slider`]).
/// Updated every frame in `PreUpdate`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Interaction {
    /// Nothing.
    #[default]
    None,
    /// The mouse is over it, or it has the keyboard and gamepad focus.
    Hovered,
    /// A pointer pressed it and is still down.
    Pressed,
}

/// Marks a node that keyboard and gamepad focus can move to. Buttons and sliders have it.
/// With `auto_focus`, it gets the focus first when a menu opens.
///
/// In prefabs: `"Focusable": (auto_focus: true)`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Focusable {
    /// Take the focus first in its menu (the UI tree it belongs to).
    pub auto_focus: bool,
}

/// A clickable node. A click (pointer released over the node it pressed, or the
/// `ui_accept` action while it has the focus) sends [`Clicked`].
///
/// ```
/// use cg_core::prelude::*;
/// use cg_ui::{Button, Clicked};
///
/// #[derive(Component)]
/// struct PlayButton;
///
/// fn on_click(mut clicks: MessageReader<Clicked>, play: Query<(), With<PlayButton>>) {
///     for click in clicks.read() {
///         if play.contains(click.entity) {
///             info!("play!");
///         }
///     }
/// }
/// ```
///
/// In prefabs: `"Button": ()`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[require(Node, Interaction, Focusable)]
#[serde(default)]
pub struct Button {
    /// Ignores pointers and focus, and draws with [`StateColors::disabled`].
    pub disabled: bool,
}

/// A [`Button`] was clicked, or `ui_accept` pressed while it had the focus.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clicked {
    /// The button.
    pub entity: Entity,
}

/// Colours for a widget's states, applied to its [`BackgroundColor`] and [`UiImage`]
/// tint each frame. Focus counts as hovered.
///
/// In prefabs: `"StateColors": (normal: "#3b5dc9", hovered: "#41a6f6")`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StateColors {
    /// When nothing is happening.
    pub normal: Color,
    /// Under the mouse, or focused.
    pub hovered: Color,
    /// Held down.
    pub pressed: Color,
    /// For a disabled button.
    pub disabled: Color,
}

impl Default for StateColors {
    fn default() -> Self {
        Self {
            normal: Color::WHITE,
            hovered: Color::WHITE,
            pressed: Color::WHITE,
            disabled: Color::rgba(1.0, 1.0, 1.0, 0.5),
        }
    }
}

/// Parts of a widget's [`UiImage`] for its states, in texels: a button sheet with normal,
/// hovered and pressed frames, say. `None` keeps the image's own source.
///
/// In prefabs: `"StateImages": (normal: Some((min: (0, 0), max: (16, 16))))`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StateImages {
    /// When nothing is happening.
    pub normal: Option<Rect>,
    /// Under the mouse, or focused.
    pub hovered: Option<Rect>,
    /// Held down.
    pub pressed: Option<Rect>,
    /// For a disabled button.
    pub disabled: Option<Rect>,
}

/// A value between `min` and `max` set by dragging with a pointer, or with `ui_left` and
/// `ui_right` while it has the focus. Sends [`SliderChanged`] when the user changes it.
///
/// The node draws its track (give it a [`BackgroundColor`] or [`UiImage`]); a child with
/// [`BarFill`] is sized to the value and a child with [`SliderThumb`] moves along. In a
/// right-to-left layout the slider runs from the right.
///
/// In prefabs: `"Slider": (value: 0.5, step: 0.1)`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[require(Node, Interaction, Focusable)]
#[serde(default)]
pub struct Slider {
    /// The value.
    pub value: f32,
    /// The value at the start.
    pub min: f32,
    /// The value at the end.
    pub max: f32,
    /// Values are multiples of this above `min`, and the keys move by it; 0 for any value,
    /// with keys moving a tenth of the range.
    pub step: f32,
}

impl Default for Slider {
    fn default() -> Self {
        Self {
            value: 0.0,
            min: 0.0,
            max: 1.0,
            step: 0.0,
        }
    }
}

impl Slider {
    /// A slider from 0 to 1 at `value`, in steps of `step`.
    pub fn new(value: f32, step: f32) -> Self {
        Self {
            value,
            step,
            ..Self::default()
        }
    }

    /// The value as a fraction of the range, from 0 to 1.
    pub fn fraction(&self) -> f32 {
        let range = self.max - self.min;
        if range.abs() <= f32::EPSILON {
            0.0
        } else {
            ((self.value - self.min) / range).clamp(0.0, 1.0)
        }
    }

    /// Sets the value from a fraction of the range, snapped to the step.
    pub fn set_fraction(&mut self, fraction: f32) {
        let value = self.min + fraction.clamp(0.0, 1.0) * (self.max - self.min);
        self.value = self.snap(value);
    }

    /// Moves the value by `steps` key presses.
    pub fn nudge(&mut self, steps: f32) {
        let step = if self.step > 0.0 {
            self.step
        } else {
            (self.max - self.min) / 10.0
        };
        self.value = self.snap(self.value + step * steps);
    }

    fn snap(&self, value: f32) -> f32 {
        let (low, high) = (self.min.min(self.max), self.min.max(self.max));
        let value = if self.step > 0.0 {
            self.min + ((value - self.min) / self.step).round() * self.step
        } else {
            value
        };
        value.clamp(low, high)
    }
}

/// The user changed a [`Slider`].
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct SliderChanged {
    /// The slider.
    pub entity: Entity,
    /// Its new value.
    pub value: f32,
}

/// Shows a value from 0 to 1 by the width of a child with [`BarFill`]: health, loading,
/// a volume level.
///
/// In prefabs: `"ProgressBar": (value: 0.75)`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[require(Node)]
#[serde(default)]
pub struct ProgressBar {
    /// From 0 (empty) to 1 (full).
    pub value: f32,
}

/// Marks the child of a [`Slider`] or [`ProgressBar`] whose width shows the value: its
/// width is set to that share of the bar, from the start edge.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[require(Node)]
pub struct BarFill;

/// Marks the child of a [`Slider`] that moves along it with the value.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[require(Node)]
pub struct SliderThumb;

type LookItem = (
    &'static Interaction,
    Option<&'static Button>,
    Option<&'static StateColors>,
    Option<&'static StateImages>,
    Option<&'static mut BackgroundColor>,
    Option<&'static mut UiImage>,
);

type WithLook = Or<(With<StateColors>, With<StateImages>)>;

/// Shows widgets' states through their [`StateColors`] and [`StateImages`].
#[profiling::function]
pub(crate) fn apply_state_looks(mut widgets: Query<LookItem, WithLook>) {
    for (interaction, button, colors, sources, background, image) in &mut widgets {
        let state = match (button.is_some_and(|b| b.disabled), interaction) {
            (true, _) => 3,
            (false, Interaction::None) => 0,
            (false, Interaction::Hovered) => 1,
            (false, Interaction::Pressed) => 2,
        };
        let color = colors.map(|c| [c.normal, c.hovered, c.pressed, c.disabled][state]);
        if let (Some(color), Some(mut background)) = (color, background)
            && background.0 != color
        {
            background.0 = color;
        }
        if let Some(mut image) = image {
            if let Some(color) = color
                && image.color != color
            {
                image.color = color;
            }
            if let Some(s) = sources {
                let source = [s.normal, s.hovered, s.pressed, s.disabled][state].or(s.normal);
                if source.is_some() && image.source != source {
                    image.source = source;
                }
            }
        }
    }
}

/// Sizes bar fills and places slider thumbs from their parents' values.
#[profiling::function]
pub(crate) fn update_bars(
    sliders: Query<(Ref<Slider>, &Children)>,
    bars: Query<(Ref<ProgressBar>, &Children)>,
    mut fills: Query<&mut Node, (With<BarFill>, Without<SliderThumb>)>,
    mut thumbs: Query<&mut Node, (With<SliderThumb>, Without<BarFill>)>,
    added_fills: Query<(), Added<BarFill>>,
    added_thumbs: Query<(), Added<SliderThumb>>,
) {
    let mut apply = |fraction: f32, children: &Children, force: bool| {
        for child in children.iter() {
            if !force && !added_fills.contains(child) && !added_thumbs.contains(child) {
                continue;
            }
            if let Ok(mut node) = fills.get_mut(child) {
                let width = Val::Percent(fraction * 100.0);
                if node.width != width {
                    node.width = width;
                }
            }
            if let Ok(mut node) = thumbs.get_mut(child) {
                let anchor = Anchor::Fraction(cg_core::glam::Vec2::new(fraction, 0.5));
                if node.anchor != anchor {
                    node.anchor = anchor;
                }
            }
        }
    };
    for (slider, children) in &sliders {
        apply(slider.fraction(), children, slider.is_changed());
    }
    for (bar, children) in &bars {
        apply(bar.value.clamp(0.0, 1.0), children, bar.is_changed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliders_snap_to_steps_and_stay_in_range() {
        let mut slider = Slider::new(0.5, 0.1);
        slider.nudge(1.0);
        assert!((slider.value - 0.6).abs() < 1e-6);
        slider.set_fraction(0.93);
        assert!((slider.value - 0.9).abs() < 1e-6);
        slider.nudge(5.0);
        assert_eq!(slider.value, 1.0);
        let mut free = Slider {
            value: 50.0,
            min: 0.0,
            max: 200.0,
            step: 0.0,
        };
        free.nudge(-1.0);
        assert_eq!(free.value, 30.0);
        assert_eq!(free.fraction(), 0.15);
    }
}
