//! The [`Rect`] type.

use glam::Vec2;

/// An axis-aligned rectangle, stored as its minimum and maximum corners.
///
/// The axes mean whatever the caller's space means: in the world `min` is the bottom-left
/// corner (y up); for texels and screen pixels it is the top-left corner (y down).
///
/// ```
/// use cg_core::prelude::*;
///
/// let rect = Rect::from_center_size(Vec2::ZERO, vec2(4.0, 2.0));
/// assert_eq!(rect.min, vec2(-2.0, -1.0));
/// assert!(rect.contains(vec2(1.0, 0.5)));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// The corner with the smallest coordinates.
    pub min: Vec2,
    /// The corner with the largest coordinates.
    pub max: Vec2,
}

impl Rect {
    /// A rectangle from its minimum corner `(x0, y0)` and maximum corner `(x1, y1)`.
    pub const fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self {
            min: Vec2::new(x0, y0),
            max: Vec2::new(x1, y1),
        }
    }

    /// The rectangle spanned by two opposite corners, in any order.
    pub fn from_corners(a: Vec2, b: Vec2) -> Self {
        Self {
            min: a.min(b),
            max: a.max(b),
        }
    }

    /// A rectangle from its centre and full size.
    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        let half = size.abs() * 0.5;
        Self {
            min: center - half,
            max: center + half,
        }
    }

    /// A rectangle from its minimum corner and size.
    pub fn from_min_size(min: Vec2, size: Vec2) -> Self {
        Self::from_corners(min, min + size)
    }

    /// Width (`max.x - min.x`).
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    /// Height (`max.y - min.y`).
    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    /// Width and height.
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    /// The centre point.
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }

    /// Whether the rectangle has no area.
    pub fn is_empty(&self) -> bool {
        self.max.x <= self.min.x || self.max.y <= self.min.y
    }

    /// Whether `point` lies inside or on the edge.
    pub fn contains(&self, point: Vec2) -> bool {
        point.cmpge(self.min).all() && point.cmple(self.max).all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::vec2;

    #[test]
    fn constructors_agree() {
        let a = Rect::new(1.0, 2.0, 5.0, 4.0);
        assert_eq!(Rect::from_corners(vec2(5.0, 4.0), vec2(1.0, 2.0)), a);
        assert_eq!(Rect::from_center_size(vec2(3.0, 3.0), vec2(4.0, 2.0)), a);
        assert_eq!(Rect::from_min_size(vec2(1.0, 2.0), vec2(4.0, 2.0)), a);
        assert_eq!(a.size(), vec2(4.0, 2.0));
        assert_eq!(a.center(), vec2(3.0, 3.0));
        assert!(!a.is_empty());
        assert!(Rect::default().is_empty());
    }

    #[test]
    fn contains_includes_edges() {
        let r = Rect::new(0.0, 0.0, 2.0, 2.0);
        assert!(r.contains(vec2(0.0, 2.0)));
        assert!(!r.contains(vec2(2.1, 1.0)));
    }
}
