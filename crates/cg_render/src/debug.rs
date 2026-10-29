//! Debug shapes: lines, rectangles and circles drawn on top of everything.

use crate::Color;
use bevy_ecs::resource::Resource;
use bytemuck::{Pod, Zeroable};
use cg_core::Rect;
use cg_core::glam::Vec2;
use core::f32::consts::TAU;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Shape {
    Line {
        a: Vec2,
        b: Vec2,
        color: Color,
        width: f32,
    },
    Rect {
        rect: Rect,
        color: Color,
        width: f32,
        filled: bool,
    },
    Circle {
        center: Vec2,
        radius: f32,
        color: Color,
        width: f32,
        filled: bool,
    },
}

/// Debug shapes to draw this frame, on top of the sprites.
///
/// Positions and sizes are in world units; outlines are [`line_width`](Self::line_width)
/// logical pixels wide whatever the zoom. Shapes are drawn at the window's resolution, even
/// in [`ScalingMode::PixelPerfect`](crate::ScalingMode::PixelPerfect), so thin outlines stay
/// sharp over pixel art. Like [`Sprites`](crate::Sprites), the queue empties after each
/// frame.
///
/// ```
/// use cg_core::prelude::*;
/// use cg_render::{Color, DebugShapes};
///
/// fn draw_bounds(mut debug: ResMut<DebugShapes>) {
///     debug.rect(Rect::from_center_size(Vec2::ZERO, vec2(32.0, 48.0)), Color::GREEN);
///     debug.circle(vec2(0.0, 24.0), 4.0, Color::RED);
///     debug.line(Vec2::ZERO, vec2(40.0, 0.0), Color::YELLOW);
/// }
/// ```
#[derive(Resource, Debug)]
pub struct DebugShapes {
    shapes: Vec<Shape>,
    line_width: f32,
    enabled: bool,
}

impl Default for DebugShapes {
    fn default() -> Self {
        Self {
            shapes: Vec::new(),
            line_width: 1.0,
            enabled: true,
        }
    }
}

impl DebugShapes {
    /// A line from `a` to `b`.
    pub fn line(&mut self, a: Vec2, b: Vec2, color: Color) {
        self.push(Shape::Line {
            a,
            b,
            color,
            width: self.line_width,
        });
    }

    /// The outline of a rectangle. The line is centred on the rectangle's edges.
    pub fn rect(&mut self, rect: Rect, color: Color) {
        self.push(Shape::Rect {
            rect,
            color,
            width: self.line_width,
            filled: false,
        });
    }

    /// A filled rectangle.
    pub fn rect_filled(&mut self, rect: Rect, color: Color) {
        self.push(Shape::Rect {
            rect,
            color,
            width: 0.0,
            filled: true,
        });
    }

    /// The outline of a circle.
    pub fn circle(&mut self, center: Vec2, radius: f32, color: Color) {
        self.push(Shape::Circle {
            center,
            radius,
            color,
            width: self.line_width,
            filled: false,
        });
    }

    /// A filled circle.
    pub fn circle_filled(&mut self, center: Vec2, radius: f32, color: Color) {
        self.push(Shape::Circle {
            center,
            radius,
            color,
            width: 0.0,
            filled: true,
        });
    }

    /// The width of lines and outlines added from now on, in logical pixels. Defaults to 1.
    pub fn line_width(&self) -> f32 {
        self.line_width
    }

    /// Sets the width of lines and outlines added from now on, in logical pixels.
    pub fn set_line_width(&mut self, width: f32) {
        self.line_width = width.max(0.0);
    }

    /// Whether shapes are recorded. While disabled, every drawing call does nothing, so
    /// debug drawing can stay in the code behind a toggle key.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Turns recording on or off. Turning it off also drops this frame's shapes.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.shapes.clear();
        }
    }

    /// The number of shapes queued this frame.
    pub fn len(&self) -> usize {
        self.shapes.len()
    }

    /// Whether no shapes are queued.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }

    /// Drops everything queued. The renderer calls it after drawing.
    pub fn clear(&mut self) {
        self.shapes.clear();
    }

    pub(crate) fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    fn push(&mut self, shape: Shape) {
        if self.enabled {
            self.shapes.push(shape);
        }
    }
}

/// A debug-shape vertex: a world position and an sRGB colour.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct ShapeVertex {
    pub(crate) position: [f32; 2],
    pub(crate) color: [f32; 4],
}

/// Turns shapes into triangles. `world_per_pixel` is the size of one physical pixel in world
/// units, and `scale_factor` the physical pixels per logical pixel.
pub(crate) fn tessellate(
    shapes: &[Shape],
    world_per_pixel: f32,
    scale_factor: f32,
    out: &mut Vec<ShapeVertex>,
) {
    let half_width = |width: f32| 0.5 * width * scale_factor * world_per_pixel;
    for shape in shapes {
        match *shape {
            Shape::Line { a, b, color, width } => {
                line(a, b, half_width(width), color, out);
            }
            Shape::Rect {
                rect,
                color,
                filled: true,
                ..
            } => {
                quad(rect.min, rect.max, color, out);
            }
            Shape::Rect {
                rect, color, width, ..
            } => {
                rect_outline(rect, half_width(width), color, out);
            }
            Shape::Circle {
                center,
                radius,
                color,
                width,
                filled,
            } => {
                let pixels = radius / world_per_pixel.max(f32::EPSILON);
                let segments = (TAU * pixels / 4.0).ceil().clamp(12.0, 128.0) as u32;
                let point = |i: u32, r: f32| {
                    let angle = i as f32 / segments as f32 * TAU;
                    center + Vec2::from_angle(angle) * r
                };
                if filled {
                    for i in 0..segments {
                        triangle([center, point(i, radius), point(i + 1, radius)], color, out);
                    }
                } else {
                    let h = half_width(width);
                    let (inner, outer) = ((radius - h).max(0.0), radius + h);
                    for i in 0..segments {
                        let corners = [
                            point(i, inner),
                            point(i, outer),
                            point(i + 1, outer),
                            point(i + 1, inner),
                        ];
                        quad_corners(corners, color, out);
                    }
                }
            }
        }
    }
}

/// A line with square caps.
fn line(a: Vec2, b: Vec2, half_width: f32, color: Color, out: &mut Vec<ShapeVertex>) {
    let dir = (b - a).try_normalize().unwrap_or(Vec2::X) * half_width;
    let normal = dir.perp();
    let (a, b) = (a - dir, b + dir);
    quad_corners([a - normal, b - normal, b + normal, a + normal], color, out);
}

/// Four edge strips that meet at the corners without overlapping.
fn rect_outline(rect: Rect, h: f32, color: Color, out: &mut Vec<ShapeVertex>) {
    let outer = Rect::from_corners(rect.min - h, rect.max + h);
    let inner = Rect::from_corners(rect.min + h, rect.max - h);
    if rect.width() <= 2.0 * h || rect.height() <= 2.0 * h {
        quad(outer.min, outer.max, color, out);
        return;
    }
    let Rect { min: o0, max: o1 } = outer;
    let Rect { min: i0, max: i1 } = inner;
    quad(o0, Vec2::new(o1.x, i0.y), color, out);
    quad(Vec2::new(o0.x, i1.y), o1, color, out);
    quad(Vec2::new(o0.x, i0.y), Vec2::new(i0.x, i1.y), color, out);
    quad(Vec2::new(i1.x, i0.y), Vec2::new(o1.x, i1.y), color, out);
}

/// An axis-aligned quad from its minimum and maximum corners.
fn quad(min: Vec2, max: Vec2, color: Color, out: &mut Vec<ShapeVertex>) {
    let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
    quad_corners(corners, color, out);
}

/// A quad from four corners in order around it.
fn quad_corners(c: [Vec2; 4], color: Color, out: &mut Vec<ShapeVertex>) {
    triangle([c[0], c[1], c[2]], color, out);
    triangle([c[0], c[2], c[3]], color, out);
}

fn triangle(points: [Vec2; 3], color: Color, out: &mut Vec<ShapeVertex>) {
    let color = color.to_array();
    out.extend(points.map(|p| ShapeVertex {
        position: p.to_array(),
        color,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use cg_core::glam::vec2;

    fn bounds(vertices: &[ShapeVertex]) -> Rect {
        let points = vertices.iter().map(|v| Vec2::from_array(v.position));
        let min = points.clone().fold(Vec2::MAX, Vec2::min);
        let max = points.fold(Vec2::MIN, Vec2::max);
        Rect { min, max }
    }

    #[test]
    fn disabled_shapes_are_dropped() {
        let mut debug = DebugShapes::default();
        debug.line(Vec2::ZERO, Vec2::ONE, Color::RED);
        debug.set_enabled(false);
        assert!(debug.is_empty());
        debug.circle(Vec2::ZERO, 2.0, Color::RED);
        assert!(debug.is_empty());
    }

    #[test]
    fn lines_have_square_caps_and_pixel_width() {
        let mut debug = DebugShapes::default();
        debug.set_line_width(2.0);
        debug.line(vec2(0.0, 0.0), vec2(10.0, 0.0), Color::RED);
        let mut out = Vec::new();
        // Two world units per physical pixel, scale factor 1: a 2 px line is 4 units wide.
        tessellate(debug.shapes(), 2.0, 1.0, &mut out);
        assert_eq!(out.len(), 6);
        assert_eq!(bounds(&out), Rect::new(-2.0, -2.0, 12.0, 2.0));
    }

    #[test]
    fn rect_outline_strips_cover_the_border_once() {
        let mut out = Vec::new();
        let shapes = [Shape::Rect {
            rect: Rect::new(0.0, 0.0, 10.0, 6.0),
            color: Color::WHITE,
            width: 2.0,
            filled: false,
        }];
        tessellate(&shapes, 1.0, 1.0, &mut out);
        assert_eq!(out.len(), 4 * 6);
        assert_eq!(bounds(&out), Rect::new(-1.0, -1.0, 11.0, 7.0));
        // Total strip area equals outer minus inner area: 12×8 − 8×4.
        let area: f32 = out
            .chunks(3)
            .map(|t| {
                let [a, b, c] = [t[0], t[1], t[2]].map(|v| Vec2::from_array(v.position));
                (b - a).perp_dot(c - a).abs() * 0.5
            })
            .sum();
        assert!((area - (96.0 - 32.0)).abs() < 1e-3);
    }

    #[test]
    fn circles_scale_their_segment_count() {
        let mut small = Vec::new();
        let mut large = Vec::new();
        let circle = |radius| Shape::Circle {
            center: Vec2::ZERO,
            radius,
            color: Color::WHITE,
            width: 0.0,
            filled: true,
        };
        tessellate(&[circle(1.0)], 1.0, 1.0, &mut small);
        tessellate(&[circle(100.0)], 1.0, 1.0, &mut large);
        assert_eq!(small.len(), 12 * 3);
        assert_eq!(large.len(), 128 * 3);
    }
}
