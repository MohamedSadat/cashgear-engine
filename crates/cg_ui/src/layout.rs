//! The layout pass: sizes and places every [`Node`], lays out [`Text`], and records the
//! draw order in [`UiStack`].

use crate::font::Font;
use crate::node::{Align, ComputedNode, Edges, Justify, Layout, LayoutDirection, Val, ZIndex};
use crate::text::{LayoutKey, TextLayout, TextRequest, TextSystem};
use crate::touch::{TouchControl, TouchControls};
use crate::{Locale, Node, Text, UiImage};
use bevy_ecs::prelude::*;
use bevy_ecs::system::SystemParam;
use cg_assets::Assets;
use cg_core::Rect;
use cg_core::glam::Vec2;
use cg_render::{Image, InheritedVisibility, MainCamera, ScreenSpace};

/// Every visible UI node in draw order (z-index, then depth-first), as of the last
/// layout. Pointer hit testing walks it backwards.
#[derive(Resource, Debug, Clone, Default)]
pub struct UiStack {
    pub(crate) entities: Vec<Entity>,
    pub(crate) space: ScreenSpace,
}

impl UiStack {
    /// The nodes, bottom first.
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    /// The screen space the UI was laid out in.
    pub fn space(&self) -> ScreenSpace {
        self.space
    }
}

type NodeItem = (
    &'static Node,
    Option<&'static Children>,
    Option<&'static ZIndex>,
    Option<&'static InheritedVisibility>,
    Has<TouchControl>,
    Option<&'static UiImage>,
    Has<Text>,
);

#[derive(SystemParam)]
pub(crate) struct LayoutParams<'w, 's> {
    candidates: Query<'w, 's, (Entity, Option<&'static ChildOf>), With<Node>>,
    nodes: Query<'w, 's, NodeItem>,
    texts: Query<'w, 's, (Ref<'static, Text>, &'static mut TextLayout)>,
    computed: Query<'w, 's, &'static mut ComputedNode>,
    camera: MainCamera<'w, 's>,
    text_system: ResMut<'w, TextSystem>,
    fonts: Res<'w, Assets<Font>>,
    images: ResMut<'w, Assets<Image>>,
    locale: Option<Res<'w, Locale>>,
    touch: Option<Res<'w, TouchControls>>,
    stack: ResMut<'w, UiStack>,
}

/// The layout's lists, kept from frame to frame so a steady UI allocates nothing.
#[derive(Default)]
pub(crate) struct Scratch {
    roots: Vec<Entity>,
    /// Visible nodes with their layer and order.
    stack: Vec<(i32, u32, Entity)>,
    /// Nodes with text, their content box and direction, shaped after the pass.
    texts: Vec<(Entity, Rect, bool)>,
    /// Children lists and main-axis sizes, lent to one node at a time down the tree.
    lists: Vec<Vec<Entity>>,
    sizes: Vec<Vec<Option<f32>>>,
}

/// One pass over the tree.
struct Pass<'p, 'w, 's> {
    params: &'p mut LayoutParams<'w, 's>,
    scratch: &'p mut Scratch,
    scale: f32,
    order: u32,
    touch_visible: bool,
}

/// Lays out the UI: runs in `PostUpdate`.
#[profiling::function]
pub(crate) fn layout_ui(mut params: LayoutParams, mut scratch: Local<Scratch>) {
    params.text_system.sync_fonts(&params.fonts);
    let space = params.camera.screen_space();
    let revision = params.text_system.revision();
    for (text, mut layout) in &mut params.texts {
        if text.is_changed() || layout.key.is_some_and(|k| k.revision != revision) {
            layout.invalidate();
        }
    }
    let rtl = params.locale.as_ref().is_some_and(|l| l.is_rtl());
    let touch_visible = params.touch.as_ref().is_none_or(|t| t.visible());

    let scratch = &mut *scratch;
    let mut roots = core::mem::take(&mut scratch.roots);
    roots.clear();
    roots.extend(
        params
            .candidates
            .iter()
            .filter(|(_, parent)| parent.is_none_or(|p| !params.nodes.contains(p.parent())))
            .map(|(entity, _)| entity),
    );
    // Unstable sorts allocate nothing, and the keys are unique.
    roots.sort_unstable_by_key(|e| (e.index_u32(), e.generation().to_bits()));
    scratch.stack.clear();
    scratch.texts.clear();

    let mut pass = Pass {
        params: &mut params,
        scratch,
        scale: space.scale.max(f32::EPSILON),
        order: 0,
        touch_visible,
    };
    let screen = space.rect();
    for &root in &roots {
        pass.place_free(root, screen, rtl, 0, true);
    }
    scratch.roots = roots;
    for &(entity, content, rtl) in &scratch.texts {
        params.shape_text(entity, content, rtl, space.scale);
    }
    scratch
        .stack
        .sort_unstable_by_key(|(layer, order, _)| (*layer, *order));
    let entities = &mut params.stack.entities;
    entities.clear();
    entities.extend(scratch.stack.iter().map(|(_, _, e)| *e));
    params.stack.space = space;
}

impl LayoutParams<'_, '_> {
    /// Lays the text of a node out in its content box, if anything it depends on changed.
    fn shape_text(&mut self, entity: Entity, content: Rect, rtl: bool, scale: f32) {
        let Ok((text, mut layout)) = self.texts.get_mut(entity) else {
            return;
        };
        let key = LayoutKey {
            width: content.width(),
            scale,
            revision: self.text_system.revision(),
            rtl,
        };
        if layout.key == Some(key) {
            return;
        }
        let request = request(&text, Some(content.width()), rtl);
        match self
            .text_system
            .shape(&request, &self.fonts, &mut self.images, scale)
        {
            Some(shaped) => {
                layout.glyphs = shaped.glyphs;
                layout.size = shaped.size;
                // Shaping may have reset the atlas, which changes the revision.
                layout.key = Some(LayoutKey {
                    revision: self.text_system.revision(),
                    ..key
                });
            }
            None => {
                layout.glyphs.clear();
                layout.key = None;
            }
        }
    }
}

fn request(text: &Text, width: Option<f32>, rtl: bool) -> TextRequest<'_> {
    TextRequest {
        text: &text.value,
        font: text.style.font.id(),
        size: text.style.size,
        color: text.style.color,
        line_height: text.style.line_height,
        align: text.align,
        wrap: text.wrap,
        width,
        rtl,
    }
}

impl Pass<'_, '_, '_> {
    /// Snaps a coordinate to the physical pixel grid.
    fn snap(&self, value: f32) -> f32 {
        (value * self.scale).round() / self.scale
    }

    /// The padding plus the border, with sides swapped in right-to-left layouts.
    fn insets(node: &Node, rtl: bool) -> Edges {
        let p = node.padding;
        let b = node.border;
        let (start, end) = if rtl {
            (p.right, p.left)
        } else {
            (p.left, p.right)
        };
        Edges {
            left: start + b,
            right: end + b,
            top: p.top + b,
            bottom: p.bottom + b,
        }
    }

    /// The size of a node's text for a content width.
    fn text_size(&mut self, entity: Entity, width: f32) -> Vec2 {
        let params = &mut *self.params;
        let Ok((text, mut layout)) = params.texts.get_mut(entity) else {
            return Vec2::ZERO;
        };
        let natural = match layout.natural {
            Some(natural) => natural,
            None => {
                let Some(natural) = params
                    .text_system
                    .measure(&request(&text, None, false), &params.fonts)
                else {
                    return Vec2::ZERO;
                };
                layout.natural = Some(natural);
                natural
            }
        };
        if !text.wrap || natural.x <= width + 0.01 {
            return natural;
        }
        let width = width.max(0.0);
        if let Some((_, size)) = layout
            .wrapped
            .iter()
            .find(|(w, _)| (w - width).abs() < 0.01)
        {
            return *size;
        }
        let size = params
            .text_system
            .measure(&request(&text, Some(width), false), &params.fonts)
            .unwrap_or(natural);
        if layout.wrapped.len() >= 8 {
            layout.wrapped.remove(0);
        }
        layout.wrapped.push((width, size));
        size
    }

    /// What a node's content needs (text, image, children), plus its insets, when it may
    /// use up to `available`.
    fn content_size(&mut self, entity: Entity, available: Vec2) -> Vec2 {
        let Ok((node, children, _, _, _, image, has_text)) = self.params.nodes.get(entity) else {
            return Vec2::ZERO;
        };
        let node = node.clone();
        let mut list = self.scratch.lists.pop().unwrap_or_default();
        list.extend(children.into_iter().flat_map(|c| c.iter()));
        let image_size = image.filter(|i| i.slice.is_none()).and_then(|image| {
            image.source.map(|s| s.size().abs()).or_else(|| {
                self.params
                    .images
                    .get(image.texture.id())
                    .map(|i| i.size().as_vec2())
            })
        });
        let insets = Self::insets(&node, false).total();
        let inner = (available - insets).max(Vec2::ZERO);

        let mut size = image_size.unwrap_or(Vec2::ZERO);
        if has_text {
            size = size.max(self.text_size(entity, inner.x));
        }
        let gaps = node.gap * list.len().saturating_sub(1) as f32;
        let mut along = Vec2::ZERO;
        for &child in &list {
            let child_size = self.outer_size(child, inner);
            match node.layout {
                Layout::Free => {
                    let offset = self
                        .params
                        .nodes
                        .get(child)
                        .map_or(Vec2::ZERO, |(n, ..)| n.offset.abs());
                    size = size.max(child_size + offset);
                }
                Layout::Row => {
                    along.x += child_size.x;
                    along.y = along.y.max(child_size.y);
                }
                Layout::Column => {
                    along.y += child_size.y;
                    along.x = along.x.max(child_size.x);
                }
            }
        }
        match node.layout {
            Layout::Free => {}
            Layout::Row => size = size.max(along + Vec2::new(gaps, 0.0)),
            Layout::Column => size = size.max(along + Vec2::new(0.0, gaps)),
        }
        list.clear();
        self.scratch.lists.push(list);
        size + insets
    }

    /// A node's size when measured inside an `available` content box: fixed sizes as
    /// they are, `Auto` (and, while measuring, `Fill`) from the content.
    fn outer_size(&mut self, entity: Entity, available: Vec2) -> Vec2 {
        let Ok((node, ..)) = self.params.nodes.get(entity) else {
            return Vec2::ZERO;
        };
        let (width, height) = (node.width, node.height);
        let fixed = |val: Val, of: f32| match val {
            Val::Px(v) => Some(v.max(0.0)),
            Val::Percent(p) => Some((of * p / 100.0).max(0.0)),
            Val::Auto | Val::Fill => None,
        };
        let w = fixed(width, available.x);
        let h = fixed(height, available.y);
        if let (Some(w), Some(h)) = (w, h) {
            return Vec2::new(w, h);
        }
        let content = self.content_size(entity, Vec2::new(w.unwrap_or(available.x), available.y));
        Vec2::new(w.unwrap_or(content.x), h.unwrap_or(content.y))
    }

    /// The size of a child of a free-layout parent with content box `inner`.
    fn free_size(&mut self, entity: Entity, inner: Vec2) -> Vec2 {
        let Ok((node, ..)) = self.params.nodes.get(entity) else {
            return Vec2::ZERO;
        };
        let (width, height) = (node.width, node.height);
        let resolve = |val: Val, of: f32| match val {
            Val::Px(v) => Some(v.max(0.0)),
            Val::Percent(p) => Some((of * p / 100.0).max(0.0)),
            Val::Fill => Some(of),
            Val::Auto => None,
        };
        let w = resolve(width, inner.x);
        let h = resolve(height, inner.y);
        if let (Some(w), Some(h)) = (w, h) {
            return Vec2::new(w, h);
        }
        let content = self.content_size(entity, Vec2::new(w.unwrap_or(inner.x), inner.y));
        Vec2::new(w.unwrap_or(content.x), h.unwrap_or(content.y))
    }

    /// Places a node in a free-layout parent's content box (or the screen, for a root).
    fn place_free(&mut self, entity: Entity, parent: Rect, rtl: bool, layer: i32, visible: bool) {
        let Ok((node, ..)) = self.params.nodes.get(entity) else {
            return;
        };
        let (anchor, offset) = (node.anchor, node.offset);
        let size = self.free_size(entity, parent.size());
        let mut fraction = anchor.fraction();
        let mut offset = offset;
        if rtl {
            fraction.x = 1.0 - fraction.x;
            offset.x = -offset.x;
        }
        let min = parent.min + (parent.size() - size) * fraction + offset;
        self.arrange(entity, Rect::from_min_size(min, size), rtl, layer, visible);
    }

    /// Records a node at `rect` and lays out its children.
    fn arrange(&mut self, entity: Entity, rect: Rect, parent_rtl: bool, layer: i32, visible: bool) {
        let Ok((node, children, z, inherited, touch_control, _, has_text)) =
            self.params.nodes.get(entity)
        else {
            return;
        };
        let node = node.clone();
        let mut list = self.scratch.lists.pop().unwrap_or_default();
        list.extend(children.into_iter().flat_map(|c| c.iter()));
        let layer = z.map_or(layer, |z| z.0);
        let visible =
            visible && inherited.is_none_or(|v| v.get()) && (!touch_control || self.touch_visible);
        let rtl = match node.direction {
            LayoutDirection::Inherit => parent_rtl,
            LayoutDirection::LeftToRight => false,
            LayoutDirection::RightToLeft => true,
        };

        let rect = Rect::from_corners(
            Vec2::new(self.snap(rect.min.x), self.snap(rect.min.y)),
            Vec2::new(self.snap(rect.max.x), self.snap(rect.max.y)),
        );
        let insets = Self::insets(&node, rtl);
        let content = Rect::from_corners(
            rect.min + Vec2::new(insets.left, insets.top),
            (rect.max - Vec2::new(insets.right, insets.bottom))
                .max(rect.min + Vec2::new(insets.left, insets.top)),
        );
        let order = self.order;
        self.order += 1;
        if let Ok(mut computed) = self.params.computed.get_mut(entity) {
            let new = ComputedNode {
                rect,
                content,
                layer,
                order,
                rtl,
                visible,
            };
            if *computed != new {
                *computed = new;
            }
        }
        if visible {
            self.scratch.stack.push((layer, order, entity));
        }
        if has_text {
            self.scratch.texts.push((entity, content, rtl));
        }

        match node.layout {
            Layout::Free => {
                for &child in &list {
                    self.place_free(child, content, rtl, layer, visible);
                }
            }
            Layout::Row | Layout::Column => {
                self.place_line(&node, &list, content, rtl, layer, visible);
            }
        }
        list.clear();
        self.scratch.lists.push(list);
    }

    /// Places the children of a row or column.
    fn place_line(
        &mut self,
        node: &Node,
        children: &[Entity],
        content: Rect,
        rtl: bool,
        layer: i32,
        visible: bool,
    ) {
        let row = node.layout == Layout::Row;
        let main_of = |v: Vec2| if row { v.x } else { v.y };
        let cross_of = |v: Vec2| if row { v.y } else { v.x };
        let (main_total, cross_total) = (main_of(content.size()), cross_of(content.size()));

        // Main-axis sizes; `Fill` children share what is left.
        let mut mains = self.scratch.sizes.pop().unwrap_or_default();
        mains.clear();
        let mut fills = 0;
        for &child in children {
            let Ok((n, ..)) = self.params.nodes.get(child) else {
                mains.push(Some(0.0));
                continue;
            };
            let val = if row { n.width } else { n.height };
            mains.push(match val {
                Val::Px(v) => Some(v.max(0.0)),
                Val::Percent(p) => Some((main_total * p / 100.0).max(0.0)),
                Val::Fill => {
                    fills += 1;
                    None
                }
                Val::Auto => {
                    let size = self.outer_size(child, content.size());
                    Some(main_of(size))
                }
            });
        }
        let gaps = node.gap * children.len().saturating_sub(1) as f32;
        let fixed: f32 = mains.iter().flatten().sum();
        let share = if fills > 0 {
            ((main_total - fixed - gaps) / fills as f32).max(0.0)
        } else {
            0.0
        };
        let used: f32 = mains.iter().map(|m| m.unwrap_or(share)).sum::<f32>() + gaps;
        let free = (main_total - used).max(0.0);
        let (mut cursor, extra) = match node.justify {
            Justify::Start => (0.0, 0.0),
            Justify::Center => (free * 0.5, 0.0),
            Justify::End => (free, 0.0),
            Justify::SpaceBetween if children.len() > 1 => {
                (0.0, free / (children.len() - 1) as f32)
            }
            Justify::SpaceBetween => (0.0, 0.0),
        };

        for (&child, main) in children.iter().zip(&mains) {
            let main = main.unwrap_or(share);
            let Ok((n, ..)) = self.params.nodes.get(child) else {
                continue;
            };
            let (cross_val, offset) = (if row { n.height } else { n.width }, n.offset);
            let cross = match cross_val {
                Val::Px(v) => v.max(0.0),
                Val::Percent(p) => (cross_total * p / 100.0).max(0.0),
                Val::Fill => cross_total,
                Val::Auto if node.align == Align::Stretch => cross_total,
                Val::Auto => {
                    // The main size is known now, so text wraps at it.
                    let available = if row {
                        Vec2::new(main, cross_total)
                    } else {
                        Vec2::new(cross_total, main)
                    };
                    cross_of(self.content_size(child, available))
                }
            };
            let cross_free = cross_total - cross;
            let cross_pos = match node.align {
                Align::Start | Align::Stretch => 0.0,
                Align::Center => (cross_free * 0.5).floor(),
                Align::End => cross_free,
            };
            let (size, pos) = if row {
                (Vec2::new(main, cross), Vec2::new(cursor, cross_pos))
            } else {
                (Vec2::new(cross, main), Vec2::new(cross_pos, cursor))
            };
            // Rows run from the start edge; in right-to-left layouts that is the right, and
            // columns line up from the right too.
            let mut min = content.min + pos;
            let mut offset = offset;
            if rtl {
                min.x = content.max.x - pos.x - size.x;
                offset.x = -offset.x;
            }
            self.arrange(
                child,
                Rect::from_min_size(min + offset, size),
                rtl,
                layer,
                visible,
            );
            cursor += main + node.gap + extra;
        }
        self.scratch.sizes.push(mains);
    }
}
