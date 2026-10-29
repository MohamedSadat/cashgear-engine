//! Tile collision: [`TileGrid`], a grid of [`TileCell`]s, and the [`TileCollider`]
//! component that puts one in the world.
//!
//! The grid answers the questions kinematic bodies and queries ask: how far a box can move
//! along an axis, where the floor under it is, and where a ray first hits.

use crate::EPSILON;
use crate::shape::WorldShape;
use bevy_ecs::prelude::*;
use cg_core::Rect;
use cg_core::glam::{IVec2, UVec2, Vec2, vec2};
use cg_world::Transform2D;

/// How a tile collides.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum TileShape {
    /// Nothing to collide with.
    #[default]
    Empty,
    /// A full block: blocks from every side.
    Solid,
    /// A platform that is only solid from above, at the top of the cell. Bodies jump up
    /// through it, walk through it sideways, land on it, and can drop through it with
    /// [`KinematicBody::drop_through`](crate::KinematicBody::drop_through).
    OneWay,
    /// A floor slope. The surface runs from `left` at the cell's left edge to `right` at
    /// its right edge, as fractions of the cell height measured from its bottom: `(0, 1)`
    /// rises 45° to the right in a square cell, `(0, 0.5)` and `(0.5, 1)` make a 22.5°
    /// slope over two cells.
    ///
    /// Slopes carry bodies by their foot, the middle of the bottom edge, so a body stands
    /// on a slope with its foot on the surface and its corners over or into the hill, as
    /// platformer characters do. Slopes block only from above: like one-way platforms,
    /// bodies pass them from below and from the sides. The high side of a slope should
    /// meet a solid tile or another slope.
    Slope {
        /// Surface height at the left edge, from 0 (bottom) to 1 (top).
        left: f32,
        /// Surface height at the right edge, from 0 (bottom) to 1 (top).
        right: f32,
    },
}

impl TileShape {
    /// The surface height of a slope at `t` (0 at the left edge, 1 at the right) as a
    /// fraction of the cell height, or `None` for other shapes.
    pub fn surface(&self, t: f32) -> Option<f32> {
        match *self {
            Self::Slope { left, right } => Some(left + (right - left) * t.clamp(0.0, 1.0)),
            _ => None,
        }
    }
}

/// One cell of a [`TileGrid`]: its shape, and whether touching it hurts.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TileCell {
    /// How it collides.
    pub shape: TileShape,
    /// Bodies that overlap the cell, or touch it if it is solid, report
    /// [`KinematicContacts::hazard`](crate::KinematicContacts::hazard). Spikes are
    /// typically empty hazards; lava blocks solid ones.
    pub hazard: bool,
}

impl TileCell {
    /// Nothing.
    pub const EMPTY: Self = Self::new(TileShape::Empty);
    /// A full block.
    pub const SOLID: Self = Self::new(TileShape::Solid);
    /// A one-way platform.
    pub const ONE_WAY: Self = Self::new(TileShape::OneWay);

    /// A cell of `shape`, not a hazard.
    pub const fn new(shape: TileShape) -> Self {
        Self {
            shape,
            hazard: false,
        }
    }

    /// A slope from `left` to `right` (fractions of the cell height); see
    /// [`TileShape::Slope`].
    pub const fn slope(left: f32, right: f32) -> Self {
        Self::new(TileShape::Slope { left, right })
    }

    /// The same cell, marked as a hazard.
    pub const fn with_hazard(mut self) -> Self {
        self.hazard = true;
        self
    }

    /// Whether it neither collides nor hurts.
    pub fn is_empty(&self) -> bool {
        self.shape == TileShape::Empty && !self.hazard
    }
}

/// A floor found under a box, in the grid's own units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GridFloor {
    pub height: f32,
    pub normal: Vec2,
    pub cell: IVec2,
}

/// A grid of collision cells, for tile-based levels.
///
/// Cells are `(column, row)` with y up: cell `(x, y)` covers `x * w .. (x + 1) * w` by
/// `y * h .. (y + 1) * h` in the grid's own units, with `(w, h)` the
/// [`cell_size`](Self::cell_size). The grid stores the cells of a rectangle starting at
/// [`origin`](Self::origin); every cell outside it is empty.
///
/// Tiled maps get one automatically (see [`TileCollider`]). Code can build one cell by
/// cell, or from text with [`parse`](Self::parse):
///
/// ```
/// use cg_core::glam::{IVec2, vec2};
/// use cg_physics::{TileCell, TileGrid, TileShape};
///
/// let grid = TileGrid::parse(
///     vec2(16.0, 16.0),
///     "
///     ..--..
///     ./##\\.
///     ######
///     ",
/// );
/// assert_eq!(grid.get(IVec2::new(0, 0)), TileCell::SOLID);
/// assert_eq!(grid.get(IVec2::new(2, 2)).shape, TileShape::OneWay);
/// assert_eq!(grid.get(IVec2::new(1, 1)), TileCell::slope(0.0, 1.0));
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct TileGrid {
    cell_size: Vec2,
    origin: IVec2,
    width: u32,
    height: u32,
    cells: Vec<TileCell>,
}

impl TileGrid {
    /// An empty grid of `size` cells of `cell_size`, whose bottom-left cell is `origin`.
    pub fn new(cell_size: Vec2, origin: IVec2, size: UVec2) -> Self {
        Self {
            cell_size,
            origin,
            width: size.x,
            height: size.y,
            cells: vec![TileCell::EMPTY; (size.x * size.y) as usize],
        }
    }

    /// A grid drawn as text, one line per row from the top, one character per cell, with
    /// the bottom-left character at cell `(0, 0)`. Leading and trailing blank lines and
    /// each line's indentation (the smallest of all lines) are ignored.
    ///
    /// | Character | Cell |
    /// | --- | --- |
    /// | `#` | solid |
    /// | `-` | one-way platform |
    /// | `/` `\` | 45° slope rising to the right, to the left |
    /// | `a` `b` | 22.5° slope rising to the right: lower half, upper half |
    /// | `B` `A` | 22.5° slope falling to the right: upper half, lower half |
    /// | `^` | hazard (empty, like spikes) |
    /// | `!` | solid hazard |
    /// | anything else | empty |
    pub fn parse(cell_size: Vec2, text: &str) -> Self {
        let lines: Vec<&str> = text.lines().collect();
        let first = lines.iter().position(|l| !l.trim().is_empty());
        let last = lines.iter().rposition(|l| !l.trim().is_empty());
        let rows = match (first, last) {
            (Some(first), Some(last)) => &lines[first..=last],
            _ => &[][..],
        };
        let indent = rows
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);
        let rows: Vec<Vec<char>> = rows
            .iter()
            .map(|l| l.get(indent..).unwrap_or("").trim_end().chars().collect())
            .collect();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0) as u32;
        let height = rows.len() as u32;
        let mut grid = Self::new(cell_size, IVec2::ZERO, UVec2::new(width, height));
        for (i, row) in rows.iter().enumerate() {
            let y = (height as usize - 1 - i) as i32;
            for (x, ch) in row.iter().enumerate() {
                let cell = match ch {
                    '#' => TileCell::SOLID,
                    '-' => TileCell::ONE_WAY,
                    '/' => TileCell::slope(0.0, 1.0),
                    '\\' => TileCell::slope(1.0, 0.0),
                    'a' => TileCell::slope(0.0, 0.5),
                    'b' => TileCell::slope(0.5, 1.0),
                    'B' => TileCell::slope(1.0, 0.5),
                    'A' => TileCell::slope(0.5, 0.0),
                    '^' => TileCell::EMPTY.with_hazard(),
                    '!' => TileCell::SOLID.with_hazard(),
                    _ => continue,
                };
                grid.set(IVec2::new(x as i32, y), cell);
            }
        }
        grid
    }

    /// The size of one cell in world units.
    pub fn cell_size(&self) -> Vec2 {
        self.cell_size
    }

    /// The bottom-left cell of the stored rectangle.
    pub fn origin(&self) -> IVec2 {
        self.origin
    }

    /// The stored rectangle's size in cells.
    pub fn size(&self) -> UVec2 {
        UVec2::new(self.width, self.height)
    }

    /// The stored rectangle in the grid's units.
    pub fn bounds(&self) -> Rect {
        Rect::from_min_size(
            self.origin.as_vec2() * self.cell_size,
            self.size().as_vec2() * self.cell_size,
        )
    }

    fn index(&self, cell: IVec2) -> Option<usize> {
        let local = cell - self.origin;
        (local.x >= 0
            && local.y >= 0
            && local.x < self.width as i32
            && local.y < self.height as i32)
            .then(|| (local.y as u32 * self.width + local.x as u32) as usize)
    }

    /// The cell at `(column, row)`; empty outside the stored rectangle.
    pub fn get(&self, cell: IVec2) -> TileCell {
        self.index(cell)
            .map_or(TileCell::EMPTY, |index| self.cells[index])
    }

    /// Sets a cell. Returns `false`, changing nothing, if the cell is outside the stored
    /// rectangle.
    pub fn set(&mut self, cell: IVec2, tile: TileCell) -> bool {
        match self.index(cell) {
            Some(index) => {
                self.cells[index] = tile;
                true
            }
            None => false,
        }
    }

    /// The cell containing a point in the grid's units.
    pub fn cell_at(&self, point: Vec2) -> IVec2 {
        (point / self.cell_size).floor().as_ivec2()
    }

    /// The area of a cell in the grid's units.
    pub fn cell_rect(&self, cell: IVec2) -> Rect {
        Rect::from_min_size(cell.as_vec2() * self.cell_size, self.cell_size)
    }

    /// Every cell that is not empty, with its position.
    pub fn iter(&self) -> impl Iterator<Item = (IVec2, TileCell)> + '_ {
        self.cells.iter().enumerate().filter_map(|(i, cell)| {
            let i = i as u32;
            let position =
                self.origin + IVec2::new((i % self.width) as i32, (i / self.width) as i32);
            (!cell.is_empty()).then_some((position, *cell))
        })
    }

    /// The stored columns, first and last.
    fn columns(&self) -> (i32, i32) {
        (self.origin.x, self.origin.x + self.width as i32 - 1)
    }

    /// The stored rows, first and last.
    fn rows(&self) -> (i32, i32) {
        (self.origin.y, self.origin.y + self.height as i32 - 1)
    }

    fn column(&self, x: f32) -> i32 {
        (x / self.cell_size.x).floor() as i32
    }

    fn row(&self, y: f32) -> i32 {
        (y / self.cell_size.y).floor() as i32
    }

    fn is_solid(&self, column: i32, row: i32) -> bool {
        self.get(IVec2::new(column, row)).shape == TileShape::Solid
    }

    /// How far `probe` can move by `dx` along x before a solid cell stops it, and the
    /// cell, or `None` if nothing is in the way. Cells the probe already overlaps are
    /// ignored, so a stuck box can move out. The distance can be slightly negative when
    /// the probe is a hair inside a wall.
    pub(crate) fn sweep_x(&self, probe: Rect, dx: f32) -> Option<(f32, IVec2)> {
        let (col_min, col_max) = self.columns();
        let (row_min, row_max) = self.rows();
        let r0 = self.row(probe.min.y + EPSILON).max(row_min);
        let r1 = self.row(probe.max.y - EPSILON).min(row_max);
        if r0 > r1 || dx == 0.0 {
            return None;
        }
        let w = self.cell_size.x;
        let blocked = |c: i32| (r0..=r1).find(|&r| self.is_solid(c, r));
        if dx > 0.0 {
            // Columns whose left edge is at or ahead of the leading edge, up to the target.
            let c0 = ((probe.max.x - EPSILON) / w).ceil() as i32;
            let c1 = ((probe.max.x + dx) / w).ceil() as i32 - 1;
            for c in c0.max(col_min)..=c1.min(col_max) {
                if let Some(r) = blocked(c) {
                    return Some((c as f32 * w - probe.max.x, IVec2::new(c, r)));
                }
            }
        } else {
            let c0 = ((probe.min.x + EPSILON) / w).floor() as i32 - 1;
            let c1 = ((probe.min.x + dx) / w).floor() as i32;
            for c in (c1.max(col_min)..=c0.min(col_max)).rev() {
                if let Some(r) = blocked(c) {
                    return Some(((c + 1) as f32 * w - probe.min.x, IVec2::new(c, r)));
                }
            }
        }
        None
    }

    /// How far `rect` can move up by `dy` before a solid cell stops it, and the cell.
    pub(crate) fn sweep_up(&self, rect: Rect, dy: f32) -> Option<(f32, IVec2)> {
        let (col_min, col_max) = self.columns();
        let (row_min, row_max) = self.rows();
        let c0 = self.column(rect.min.x + EPSILON).max(col_min);
        let c1 = self.column(rect.max.x - EPSILON).min(col_max);
        if c0 > c1 || dy <= 0.0 {
            return None;
        }
        let h = self.cell_size.y;
        let r0 = ((rect.max.y - EPSILON) / h).ceil() as i32;
        let r1 = ((rect.max.y + dy) / h).ceil() as i32 - 1;
        for r in r0.max(row_min)..=r1.min(row_max) {
            if let Some(c) = (c0..=c1).find(|&c| self.is_solid(c, r)) {
                return Some((r as f32 * h - rect.max.y, IVec2::new(c, r)));
            }
        }
        None
    }

    /// The highest floor under `rect` whose height is between `lo` and `hi_flat` (tops
    /// of solid cells and one-way platforms) or `hi_slope` (slope surfaces under the
    /// foot).
    ///
    /// When a slope lies under the foot (the middle of the bottom edge), only the foot's
    /// column counts, so a body walks down a slope instead of hanging off the plateau at
    /// its top. Otherwise every column under the box counts, and slopes in other columns
    /// count at their edge nearest the foot.
    pub(crate) fn floor(
        &self,
        rect: Rect,
        lo: f32,
        hi_flat: f32,
        hi_slope: f32,
        one_way: bool,
    ) -> Option<GridFloor> {
        let (col_min, col_max) = self.columns();
        let (row_min, row_max) = self.rows();
        let (w, h) = (self.cell_size.x, self.cell_size.y);
        let r0 = (self.row(lo) - 1).max(row_min);
        let r1 = self.row(hi_flat.max(hi_slope)).min(row_max);
        if r0 > r1 {
            return None;
        }
        let foot = rect.center().x;
        let foot_column = self.column(foot);
        let mut best: Option<GridFloor> = None;
        let mut consider = |height: f32, normal: Vec2, cell: IVec2| {
            if best.is_none_or(|b| height > b.height) {
                best = Some(GridFloor {
                    height,
                    normal,
                    cell,
                });
            }
        };
        let flat_top = |shape: TileShape, row: i32| {
            let top = (row + 1) as f32 * h;
            let floor = shape == TileShape::Solid || (one_way && shape == TileShape::OneWay);
            (floor && lo <= top && top <= hi_flat).then_some(top)
        };

        let mut on_slope = false;
        if (col_min..=col_max).contains(&foot_column) {
            let t = foot / w - foot_column as f32;
            for r in r0..=r1 {
                let cell = IVec2::new(foot_column, r);
                let shape = self.get(cell).shape;
                if let (TileShape::Slope { left, right }, Some(surface)) = (shape, shape.surface(t))
                {
                    let height = (r as f32 + surface) * h;
                    if lo <= height && height <= hi_slope {
                        on_slope = true;
                        consider(height, slope_normal(left, right, self.cell_size), cell);
                    }
                }
            }
            if on_slope {
                for r in r0..=r1 {
                    let cell = IVec2::new(foot_column, r);
                    if let Some(top) = flat_top(self.get(cell).shape, r) {
                        consider(top, Vec2::Y, cell);
                    }
                }
            }
        }
        if on_slope {
            return best;
        }

        let c0 = self.column(rect.min.x + EPSILON).max(col_min);
        let c1 = self.column(rect.max.x - EPSILON).min(col_max);
        for c in c0..=c1 {
            for r in r0..=r1 {
                let cell = IVec2::new(c, r);
                let shape = self.get(cell).shape;
                if let Some(top) = flat_top(shape, r) {
                    consider(top, Vec2::Y, cell);
                } else if let TileShape::Slope { left, right } = shape {
                    // A slope beside the foot counts at its edge nearest the foot.
                    let t = (foot / w - c as f32).clamp(0.0, 1.0);
                    let height = (r as f32 + left + (right - left) * t) * h;
                    if lo <= height && height <= hi_flat {
                        let normal = if t > 0.0 && t < 1.0 {
                            slope_normal(left, right, self.cell_size)
                        } else {
                            Vec2::Y
                        };
                        consider(height, normal, cell);
                    }
                }
            }
        }
        best
    }

    /// The steepest slope (rise over run) among the cells under the x span of `rect`, in
    /// the row of its bottom and the row above.
    pub(crate) fn steepest_slope_near(&self, rect: Rect) -> f32 {
        let (col_min, col_max) = self.columns();
        let c0 = self.column(rect.min.x + EPSILON).max(col_min);
        let c1 = self.column(rect.max.x - EPSILON).min(col_max);
        let r0 = self.row(rect.min.y - EPSILON);
        let mut steepest = 0.0f32;
        for r in r0..=r0 + 1 {
            for c in c0..=c1 {
                if let TileShape::Slope { left, right } = self.get(IVec2::new(c, r)).shape {
                    steepest =
                        steepest.max((right - left).abs() * self.cell_size.y / self.cell_size.x);
                }
            }
        }
        steepest
    }

    /// A hazard cell that `rect` overlaps, if any.
    pub(crate) fn hazard_in(&self, rect: Rect) -> Option<IVec2> {
        let (col_min, col_max) = self.columns();
        let (row_min, row_max) = self.rows();
        let c0 = self.column(rect.min.x + EPSILON).max(col_min);
        let c1 = self.column(rect.max.x - EPSILON).min(col_max);
        let r0 = self.row(rect.min.y + EPSILON).max(row_min);
        let r1 = self.row(rect.max.y - EPSILON).min(row_max);
        (r0..=r1)
            .flat_map(|r| (c0..=c1).map(move |c| IVec2::new(c, r)))
            .find(|&cell| self.get(cell).hazard)
    }

    /// The first cell a ray from `origin` along the unit vector `direction` hits within
    /// `max_distance`: the distance, the surface normal and the cell. Solid cells are hit
    /// from any side (at distance 0 if the ray starts inside one); one-way platforms and
    /// slopes only from above, as they block bodies.
    pub fn raycast(
        &self,
        origin: Vec2,
        direction: Vec2,
        max_distance: f32,
    ) -> Option<(f32, Vec2, IVec2)> {
        let bounds = self.bounds();
        let (mut enter, mut entered) = if bounds.contains(origin) {
            (0.0, None)
        } else {
            let (t, normal) = WorldShape::Rect(bounds).raycast(origin, direction, max_distance)?;
            (t, Some(normal))
        };
        let size = self.cell_size;
        let start = origin + direction * (enter + 1e-4);
        let mut cell = (start / size).floor().as_ivec2();
        let step = IVec2::new(
            direction.x.signum() as i32 * (direction.x != 0.0) as i32,
            direction.y.signum() as i32 * (direction.y != 0.0) as i32,
        );
        let boundary = |c: i32, s: i32, axis: usize| {
            let edge = if s > 0 { c + 1 } else { c } as f32 * size[axis];
            (edge - origin[axis]) / direction[axis]
        };
        let mut t_max = vec2(
            if step.x != 0 {
                boundary(cell.x, step.x, 0)
            } else {
                f32::INFINITY
            },
            if step.y != 0 {
                boundary(cell.y, step.y, 1)
            } else {
                f32::INFINITY
            },
        );
        let t_delta = vec2(
            if step.x != 0 {
                size.x / direction.x.abs()
            } else {
                f32::INFINITY
            },
            if step.y != 0 {
                size.y / direction.y.abs()
            } else {
                f32::INFINITY
            },
        );
        const SLACK: f32 = 1e-4;
        while self.index(cell).is_some() && enter <= max_distance {
            let exit = t_max.x.min(t_max.y);
            let in_cell =
                |t: f32| t >= enter - SLACK && t <= exit + SLACK && t >= 0.0 && t <= max_distance;
            match self.get(cell).shape {
                TileShape::Solid => {
                    return Some(match entered {
                        Some(normal) => (enter, normal, cell),
                        None => (0.0, -direction, cell),
                    });
                }
                TileShape::OneWay if direction.y < 0.0 => {
                    let top = (cell.y + 1) as f32 * size.y;
                    let t = (top - origin.y) / direction.y;
                    if in_cell(t) {
                        return Some((t.max(0.0), Vec2::Y, cell));
                    }
                }
                TileShape::Slope { left, right } => {
                    let normal = slope_normal(left, right, size);
                    let along = direction.dot(normal);
                    if along < 0.0 {
                        let corner = vec2(cell.x as f32 * size.x, (cell.y as f32 + left) * size.y);
                        let t = (corner - origin).dot(normal) / along;
                        if in_cell(t) {
                            return Some((t.max(0.0), normal, cell));
                        }
                    }
                }
                _ => {}
            }
            if t_max.x < t_max.y {
                cell.x += step.x;
                enter = t_max.x;
                t_max.x += t_delta.x;
                entered = Some(vec2(-step.x as f32, 0.0));
            } else {
                cell.y += step.y;
                enter = t_max.y;
                t_max.y += t_delta.y;
                entered = Some(vec2(0.0, -step.y as f32));
            }
        }
        None
    }
}

/// The upward normal of a slope surface from `left` to `right` in a cell of `size`.
pub(crate) fn slope_normal(left: f32, right: f32, size: Vec2) -> Vec2 {
    vec2(-(right - left) * size.y, size.x).normalize()
}

/// Puts a [`TileGrid`] in the world at the entity's position (its global translation;
/// rotation and scale are ignored), so kinematic bodies collide with it and queries find
/// it.
///
/// Entities with a [`Tilemap`](cg_tiled::Tilemap) get one automatically, built from the
/// tile layers that have a `collision` property set to `true`. Each tile collides by the
/// custom properties its tileset gives it:
///
/// | Tile property | Cell |
/// | --- | --- |
/// | `solid` (bool) | [`TileShape::Solid`] |
/// | `one_way` (bool) | [`TileShape::OneWay`] |
/// | `slope_left`, `slope_right` (numbers) | [`TileShape::Slope`]: the surface height at the tile's left and right edge, in pixels from its bottom |
/// | `hazard` (bool) | marks the cell as a hazard, together with any of the above |
///
/// A slope tile flipped horizontally in Tiled slopes the other way. Flipped vertically
/// or diagonally it collides as a solid block (ceiling slopes are not supported). Where
/// several collision layers have a tile in the same cell, the topmost layer's shape wins
/// and a hazard on any of them counts. The map's [`CollisionLayers`](crate::CollisionLayers)
/// go on the map entity. When the map changes, the grid is rebuilt.
#[derive(Component, Debug, Clone, PartialEq)]
#[require(Transform2D)]
pub struct TileCollider {
    /// The cells.
    pub grid: TileGrid,
}

impl TileCollider {
    /// A collider for `grid`.
    pub fn new(grid: TileGrid) -> Self {
        Self { grid }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: f32 = 10.0;

    fn grid(text: &str) -> TileGrid {
        TileGrid::parse(Vec2::splat(S), text)
    }

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::new(x0, y0, x1, y1)
    }

    #[test]
    fn parse_reads_rows_from_the_top() {
        let g = grid(
            "
            -  ab
            ^#BA!
            ",
        );
        assert_eq!(g.size(), UVec2::new(5, 2));
        assert_eq!(g.get(IVec2::new(0, 1)), TileCell::ONE_WAY);
        assert_eq!(g.get(IVec2::new(3, 1)), TileCell::slope(0.0, 0.5));
        assert_eq!(g.get(IVec2::new(0, 0)), TileCell::EMPTY.with_hazard());
        assert_eq!(g.get(IVec2::new(1, 0)), TileCell::SOLID);
        assert_eq!(g.get(IVec2::new(2, 0)), TileCell::slope(1.0, 0.5));
        assert_eq!(g.get(IVec2::new(4, 0)), TileCell::SOLID.with_hazard());
        assert_eq!(g.get(IVec2::new(9, 9)), TileCell::EMPTY);
        assert_eq!(g.iter().count(), 8);
        assert_eq!(g.bounds(), rect(0.0, 0.0, 50.0, 20.0));
        assert_eq!(g.cell_at(Vec2::new(-0.5, 19.9)), IVec2::new(-1, 1));
    }

    #[test]
    fn sweeps_stop_at_the_first_solid_cell() {
        let g = grid(
            "
            ...#....#
            .........
            ",
        );
        let body = rect(10.0, 0.0, 18.0, 20.0);
        // Right: the wall in column 3 (x 30) stops the right edge (x 18) after 12.
        assert_eq!(g.sweep_x(body, 50.0), Some((12.0, IVec2::new(3, 1))));
        assert_eq!(g.sweep_x(body, 5.0), None);
        // A huge step still finds the first wall: no tunnelling.
        assert_eq!(g.sweep_x(body, 1.0e6).map(|h| h.0), Some(12.0));
        // Left: nothing; and nothing either for a box below the walls' row.
        assert_eq!(g.sweep_x(body, -100.0), None);
        assert_eq!(g.sweep_x(rect(10.0, 0.0, 18.0, 10.0), 50.0), None);
        // Touching a wall: zero distance. Already inside one: ignored.
        assert_eq!(
            g.sweep_x(rect(22.0, 0.0, 30.0, 20.0), 1.0).map(|h| h.0),
            Some(0.0)
        );
        assert_eq!(
            g.sweep_x(rect(25.0, 5.0, 35.0, 20.0), 100.0).map(|h| h.0),
            Some(45.0)
        );
        let left = rect(90.0, 0.0, 98.0, 20.0);
        assert_eq!(g.sweep_x(left, -100.0), Some((0.0, IVec2::new(8, 1))));
        let left = rect(91.0, 0.0, 99.0, 20.0);
        assert_eq!(g.sweep_x(left, -100.0), Some((-1.0, IVec2::new(8, 1))));
        // Up into the ceiling.
        let low = rect(31.0, 0.0, 39.0, 8.0);
        assert_eq!(g.sweep_up(low, 100.0), Some((2.0, IVec2::new(3, 1))));
        assert_eq!(g.sweep_up(rect(41.0, 0.0, 49.0, 8.0), 100.0), None);
    }

    #[test]
    fn floors_take_the_highest_surface_in_range() {
        let g = grid(
            "
            ..-...
            ......
            ######
            ",
        );
        let body = rect(20.0, 25.0, 28.0, 35.0);
        // Falling 30: the one-way top (30) is above the bottom (25), out of range; ground
        // (10) is in range.
        let floor = g.floor(body, -5.0, 25.0, 25.0, true).unwrap();
        assert_eq!((floor.height, floor.cell), (10.0, IVec2::new(2, 0)));
        // From above, the one-way platform catches it, unless dropping through.
        let above = rect(20.0, 31.0, 28.0, 41.0);
        assert_eq!(g.floor(above, 0.0, 31.0, 31.0, true).unwrap().height, 30.0);
        assert_eq!(g.floor(above, 0.0, 31.0, 31.0, false).unwrap().height, 10.0);
        // Out of reach.
        assert_eq!(g.floor(above, 20.0, 31.0, 31.0, true).unwrap().height, 30.0);
        assert_eq!(
            g.floor(rect(40.0, 31.0, 48.0, 41.0), 20.0, 31.0, 31.0, true),
            None
        );
    }

    #[test]
    fn slopes_carry_the_foot() {
        let g = grid(
            "
            ../##\\..
            ########
            ",
        );
        // Foot at x 25, halfway up the slope in column 2 (surface 15).
        let body = rect(21.0, 14.0, 29.0, 24.0);
        let floor = g.floor(body, 10.0, 14.01, 18.0, true).unwrap();
        assert!((floor.height - 15.0).abs() < 1e-4, "{floor:?}");
        assert!((floor.normal - vec2(-1.0, 1.0).normalize()).length() < 1e-5);
        // The plateau to the right, under the box's corner and in reach, is ignored while
        // the foot is on the slope.
        let body = rect(25.0, 18.0, 33.0, 28.0);
        let floor = g.floor(body, 15.0, 22.0, 22.0, true).unwrap();
        assert!((floor.height - 19.0).abs() < 1e-4, "{floor:?}");
        // Once the foot is over the plateau, it is the floor.
        let body = rect(26.0, 19.5, 34.0, 29.5);
        let floor = g.floor(body, 16.0, 23.51, 23.51, true).unwrap();
        assert!(
            (floor.height - 20.0).abs() < 1e-4,
            "foot at 30 is on the plateau"
        );
        // Walking off the plateau's right edge towards the down slope: the slope counts
        // at its edge nearest the foot (full height), so there is no bump.
        let body = rect(44.0, 20.0, 52.0, 30.0);
        let floor = g.floor(body, 18.0, 20.01, 20.01, true).unwrap();
        assert_eq!(floor.height, 20.0);
        // Foot on flat ground left of the slope: the slope counts at its low edge.
        let body = rect(14.0, 10.0, 22.0, 20.0);
        assert_eq!(g.floor(body, 8.0, 10.01, 13.0, true).unwrap().height, 10.0);
    }

    #[test]
    fn hazards_count_when_overlapped() {
        let g = grid(
            "
            .^.
            ###
            ",
        );
        assert_eq!(
            g.hazard_in(rect(12.0, 12.0, 18.0, 18.0)),
            Some(IVec2::new(1, 1))
        );
        // Standing next to the spikes, or on the ground under them, is safe.
        assert_eq!(g.hazard_in(rect(2.0, 10.0, 10.0, 20.0)), None);
        assert_eq!(g.hazard_in(rect(12.0, 0.0, 18.0, 10.0)), None);
    }

    #[test]
    fn rays_hit_solids_from_anywhere_and_platforms_from_above() {
        let g = grid(
            "
            ......
            .-.../
            ######
            ",
        );
        // Down onto the floor.
        let (t, n, cell) = g.raycast(vec2(5.0, 25.0), -Vec2::Y, 100.0).unwrap();
        assert_eq!((t, n, cell), (15.0, Vec2::Y, IVec2::new(0, 0)));
        // Down onto the one-way platform; up through it to nothing.
        let (t, n, _) = g.raycast(vec2(15.0, 29.0), -Vec2::Y, 100.0).unwrap();
        assert!((t - 9.0).abs() < 1e-4 && n == Vec2::Y);
        assert_eq!(g.raycast(vec2(15.0, 12.0), Vec2::Y, 100.0), None);
        // Sideways along row 1: through the platform, onto the slope surface.
        let (t, n, cell) = g.raycast(vec2(0.0, 15.0), Vec2::X, 100.0).unwrap();
        assert!((t - 55.0).abs() < 1e-3, "{t}");
        assert!((n - vec2(-1.0, 1.0).normalize()).length() < 1e-5);
        assert_eq!(cell, IVec2::new(5, 1));
        // From outside the grid, and from inside a solid cell.
        let (t, n, _) = g.raycast(vec2(-20.0, 5.0), Vec2::X, 100.0).unwrap();
        assert_eq!((t, n), (20.0, -Vec2::X));
        assert_eq!(g.raycast(vec2(5.0, 5.0), Vec2::X, 100.0).unwrap().0, 0.0);
        assert_eq!(g.raycast(vec2(-20.0, 5.0), Vec2::X, 10.0), None);
        // A diagonal ray from above the slope.
        let (t, _, cell) = g
            .raycast(vec2(45.0, 29.0), vec2(1.0, -1.0).normalize(), 100.0)
            .unwrap();
        assert_eq!(cell, IVec2::new(5, 1));
        let hit = vec2(45.0, 29.0) + vec2(1.0, -1.0).normalize() * t;
        assert!(
            (hit.y - 10.0 - (hit.x - 50.0)).abs() < 1e-3,
            "on the surface: {hit}"
        );
    }
}
