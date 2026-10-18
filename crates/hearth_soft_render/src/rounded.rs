//! The shapes UI nodes cover: boxes with rounded corners, their borders, and what's outside them,
//! faded over a pixel at their edges, as `hearth_ui_render`'s shader draws them.

use hearth_math::{Vec2, Vec4};

/// A UI node's box, in its own units, from its center.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RoundedBox {
    pub size: Vec2,
    /// Each corner's radius across: top left, top right, bottom right, bottom left.
    pub radius_x: Vec4,
    /// Each corner's radius down, in the same order.
    pub radius_y: Vec4,
    /// The border's width on each side: left, top, right, bottom.
    pub border: Vec4,
    pub part: Part,
}

/// Which part of a [`RoundedBox`] is covered.
#[derive(Clone, Copy, PartialEq, Debug)]
#[cfg_attr(
    not(feature = "hearth_ui"),
    expect(dead_code, reason = "Only UI nodes have rounded boxes.")
)]
pub(crate) enum Part {
    /// Inside the border.
    Inside,
    /// Outside the box.
    Outside,
    /// The border, on the sides whose bits are set: [`LEFT`], [`TOP`], [`RIGHT`], [`BOTTOM`].
    Border(u8),
}

pub(crate) const LEFT: u8 = 1;
pub(crate) const TOP: u8 = 2;
pub(crate) const RIGHT: u8 = 4;
pub(crate) const BOTTOM: u8 = 8;
const ALL: u8 = 15;

impl RoundedBox {
    /// How much of the pixel whose center is at `unit`, from 0 to 1 across the box, is covered.
    #[inline]
    pub(crate) fn coverage(&self, unit: Vec2) -> f32 {
        let point = (unit - 0.5) * self.size;
        match self.part {
            Part::Inside => antialias(self.inner_distance(point)),
            Part::Outside => antialias(-self.inner_distance(point)),
            Part::Border(sides) => {
                if !self.nearest_side_is(point, sides) {
                    return 0.0;
                }
                let outer = rounded_box_distance(point, self.size, self.radius_x, self.radius_y);
                let inner = self.inner_distance(point);
                let distance = outer.max(-inner);
                // Only the border's edges fade: where there's no border, the outer and inner
                // edges meet, and fading there would outline the box.
                if outer < inner {
                    antialias(distance)
                } else if distance < 0.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }

    /// The signed distance from `point` to the edge inside the border: negative inside it.
    fn inner_distance(&self, point: Vec2) -> f32 {
        let border = self.border;
        let inner_size = self.size - Vec2::new(border.x + border.z, border.y + border.w);
        let inner_center = Vec2::new(border.x, border.y) + 0.5 * inner_size - 0.5 * self.size;
        let half = 0.5 * inner_size;
        // Each corner's radius less the borders beside it.
        let radius_x = (self.radius_x - Vec4::new(border.x, border.z, border.z, border.x))
            .max(Vec4::ZERO)
            .min(Vec4::splat(half.x));
        let radius_y = (self.radius_y - Vec4::new(border.y, border.y, border.w, border.w))
            .max(Vec4::ZERO)
            .min(Vec4::splat(half.y));
        let square = radius_x.min(radius_y).cmple(Vec4::ZERO);
        let radius_x = Vec4::select(square, Vec4::ZERO, radius_x);
        let radius_y = Vec4::select(square, Vec4::ZERO, radius_y);
        rounded_box_distance(point - inner_center, inner_size, radius_x, radius_y)
    }

    /// Whether the side of the box nearest `point`, measured in border widths, is one of
    /// `sides`.
    fn nearest_side_is(&self, point: Vec2, sides: u8) -> bool {
        if sides & ALL == ALL {
            return true;
        }
        let size = self.size;
        let point = (point + size * 0.49999).max(Vec2::ZERO).min(size);
        let left = point.x / self.border.x;
        let top = point.y / self.border.y;
        let right = (size.x - point.x) / self.border.z;
        let bottom = (size.y - point.y) / self.border.w;
        let nearest = left.min(top).min(right.min(bottom));
        (sides & LEFT != 0 && nearest == left)
            || (sides & TOP != 0 && nearest == top)
            || (sides & RIGHT != 0 && nearest == right)
            || (sides & BOTTOM != 0 && nearest == bottom)
    }
}

/// How much of a pixel an edge `distance` from its center covers.
#[inline]
fn antialias(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

/// The signed distance from `point`, from the center of a box `size` big, to its edge: negative
/// inside.
fn rounded_box_distance(point: Vec2, size: Vec2, radius_x: Vec4, radius_y: Vec4) -> f32 {
    // The radii of the corner nearest the point.
    let (across, down) = if point.y > 0.0 {
        (
            Vec2::new(radius_x.w, radius_x.z),
            Vec2::new(radius_y.w, radius_y.z),
        )
    } else {
        (
            Vec2::new(radius_x.x, radius_x.y),
            Vec2::new(radius_y.x, radius_y.y),
        )
    };
    let radius = if point.x > 0.0 {
        Vec2::new(across.y, down.y)
    } else {
        Vec2::new(across.x, down.x)
    };
    let corner_to_point = point.abs() - 0.5 * size;
    if radius.min_element() <= 0.0 {
        return corner_to_point.max_element();
    }
    // From the center of the corner's ellipse.
    let q = corner_to_point + radius;
    if q.x > 0.0 && q.y > 0.0 {
        ellipse_distance(q, 1.0 / (radius * radius))
    } else {
        (q - radius).max_element()
    }
}

/// The distance from `p` to the ellipse at the origin with these inverse squared radii,
/// approximated by one step of Newton's method, as in G. Taubin, "Distance Approximations for
/// Rasterizing Implicit Curves".
fn ellipse_distance(p: Vec2, inverse_radii_squared: Vec2) -> f32 {
    let p_r = p * inverse_radii_squared;
    let g = p.dot(p_r) - 1.0;
    let gradient = 2.0 * p_r;
    g / gradient.length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rounded(size: Vec2, radius: f32, border: f32, part: Part) -> RoundedBox {
        RoundedBox {
            size,
            radius_x: Vec4::splat(radius),
            radius_y: Vec4::splat(radius),
            border: Vec4::splat(border),
            part,
        }
    }

    #[test]
    fn rounds_corners() {
        let node = rounded(Vec2::splat(20.0), 8.0, 0.0, Part::Inside);
        // The middle, and the corner, which the rounding cuts off.
        assert_eq!(node.coverage(Vec2::splat(0.5)), 1.0);
        assert_eq!(node.coverage(Vec2::splat(0.01)), 0.0);
        // Half a pixel inside a straight edge.
        assert_eq!(node.coverage(Vec2::new(0.5, 0.025)), 1.0);
        // On the edge.
        assert_eq!(node.coverage(Vec2::new(0.5, 0.0)), 0.5);
    }

    #[test]
    fn borders_cover_only_the_border() {
        let border = rounded(Vec2::splat(20.0), 0.0, 4.0, Part::Border(ALL));
        assert_eq!(border.coverage(Vec2::new(0.5, 0.1)), 1.0);
        assert_eq!(border.coverage(Vec2::splat(0.5)), 0.0);
        let top = rounded(Vec2::splat(20.0), 0.0, 4.0, Part::Border(TOP));
        assert_eq!(top.coverage(Vec2::new(0.5, 0.1)), 1.0);
        assert_eq!(top.coverage(Vec2::new(0.1, 0.5)), 0.0);
        // Inside the border, the background.
        let inside = rounded(Vec2::splat(20.0), 0.0, 4.0, Part::Inside);
        assert_eq!(inside.coverage(Vec2::new(0.5, 0.1)), 0.0);
        assert_eq!(inside.coverage(Vec2::splat(0.5)), 1.0);
    }
}
