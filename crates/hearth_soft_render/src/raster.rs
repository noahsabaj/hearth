//! Filling a framebuffer with quads.
//!
//! Everything the renderer draws is a quad: the unit square of `(s, t)`, `s` rightward and `t`
//! downward, placed in the scene by an affine transform and seen through a camera. Seen through a
//! camera, the square lands on the framebuffer as a quadrilateral, and a 3x3 matrix takes each
//! pixel's center back to the `(s, t)` there. A pixel is covered if its center is inside the
//! square, and then gets the quad's color, or a sample of its texture at the texel `(s, t)` maps
//! to.

use hearth_image::ImageFilterMode;
use hearth_math::{Affine2, Mat3, Vec2, Vec3, Vec3A};

use crate::{
    color::{Tables, OPAQUE},
    texture::Texture,
};

/// How a quad's alpha is used.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Alpha {
    /// Mixed with what's below.
    Blend,
    /// Ignored: the quad covers what's below.
    Opaque,
    /// Covers what's below where alpha is at least this, and leaves it elsewhere.
    Mask(f32),
}

/// Where in a texture each point of a quad samples.
#[derive(Clone, Debug)]
pub(crate) enum TexelMap {
    /// A texel position from `(s, t)`.
    Affine(Affine2),
    /// A sprite's UV steps: flipping, tiling and nine-slicing.
    Sprite(SpriteUv),
}

impl TexelMap {
    #[inline(always)]
    fn texel(&self, unit: Vec2) -> Vec2 {
        match self {
            TexelMap::Affine(map) => map.transform_point2(unit),
            TexelMap::Sprite(uv) => uv.texel(unit),
        }
    }
}

/// A sprite's mapping from its quad to its texture, as the GPU sprite material does it.
#[derive(Clone, Debug, Default)]
pub(crate) struct SpriteUv {
    pub flip_x: bool,
    pub flip_y: bool,
    /// How much of the quad, from 0 to 1, one repeat of the texture covers, on axes that tile.
    pub tile: [Option<f32>; 2],
    pub slice: Option<SliceUv>,
    /// From UV after tiling and slicing to UV in the whole image.
    pub uv_transform: Affine2,
    pub image_size: Vec2,
}

/// A nine-sliced sprite's corners and edges, as fractions of its texture and of its quad.
#[derive(Clone, Debug)]
pub(crate) struct SliceUv {
    pub scale: Vec2,
    pub min_inset: Vec2,
    pub max_inset: Vec2,
    pub side_stretch: Vec2,
    pub center_stretch: Vec2,
}

impl SpriteUv {
    /// The texel a point of the quad samples.
    pub(crate) fn texel(&self, mut uv: Vec2) -> Vec2 {
        if self.flip_x {
            uv.x = 1.0 - uv.x;
        }
        if self.flip_y {
            uv.y = 1.0 - uv.y;
        }
        for (axis, tile) in self.tile.iter().enumerate() {
            if let Some(stretch) = *tile {
                uv[axis] = (uv[axis] - stretch * (uv[axis] / stretch).floor()) / stretch;
            }
        }
        if let Some(slice) = &self.slice {
            uv = slice.uv(uv);
        }
        self.uv_transform.transform_point2(uv) * self.image_size
    }
}

impl SliceUv {
    fn uv(&self, uv: Vec2) -> Vec2 {
        let min_scaled = self.min_inset / self.scale;
        let max_scaled = self.max_inset / self.scale;
        let low = uv.cmplt(min_scaled);
        let high = uv.cmpgt(Vec2::ONE - max_scaled);
        let axis = |i: usize, stretch: Vec2| {
            if low.test(i) {
                uv[i] * self.scale[i]
            } else if high.test(i) {
                1.0 - (1.0 - uv[i]) * self.scale[i]
            } else {
                tile_or_stretch(
                    uv[i],
                    min_scaled[i],
                    1.0 - max_scaled[i],
                    self.min_inset[i],
                    1.0 - self.max_inset[i],
                    stretch[i],
                )
            }
        };
        // An edge stretches or tiles along itself by the sides' mode, the center by its own.
        let on_edge = |i: usize| low.test(i) || high.test(i);
        let stretch_x = if on_edge(1) {
            self.side_stretch
        } else {
            self.center_stretch
        };
        let stretch_y = if on_edge(0) {
            self.side_stretch
        } else {
            self.center_stretch
        };
        Vec2::new(axis(0, stretch_x), axis(1, stretch_y))
    }
}

/// Maps `p` from `a..b` to `c..d`, repeating if `stretch` isn't 0.
fn tile_or_stretch(p: f32, a: f32, b: f32, c: f32, d: f32, stretch: f32) -> f32 {
    let value = (p - a) / (b - a);
    let value = if stretch == 0.0 {
        value
    } else {
        (value - stretch * (value / stretch).floor()) / stretch
    };
    value * (d - c) + c
}

/// What covers a quad.
pub(crate) enum Paint<'a> {
    /// Its color.
    Solid,
    /// A texture, tinted by its color.
    Texture {
        texture: &'a Texture<'a>,
        map: &'a TexelMap,
        filter: ImageFilterMode,
    },
}

/// A quad placed on the framebuffer, ready to fill.
pub(crate) struct Quad<'a> {
    /// Takes a pixel position `(x, y, 1)` to `(s, t, 1)` times some factor.
    unit_from_pixel: Mat3,
    /// `unit_from_pixel` is affine: the factor is always 1.
    affine: bool,
    /// The pixels the quad may cover: columns `x0..x1`, rows `y0..y1`.
    bounds: [u32; 4],
    paint: Paint<'a>,
    /// Linear RGBA: the color, or the texture's tint.
    color: [f32; 4],
    alpha: Alpha,
}

impl<'a> Quad<'a> {
    /// Places a quad whose `(s, t, 1)` lands on the pixel `(x, y, 1)` times some factor by
    /// `pixel_from_unit`, inside `clip` (`x0, y0, x1, y1`), or `None` if it covers nothing there.
    pub(crate) fn new(
        pixel_from_unit: Mat3,
        clip: [u32; 4],
        paint: Paint<'a>,
        color: [f32; 4],
        alpha: Alpha,
    ) -> Option<Self> {
        let corners = [Vec2::ZERO, Vec2::X, Vec2::Y, Vec2::ONE]
            .map(|corner| pixel_from_unit * corner.extend(1.0));
        // A corner behind the camera: the quad crosses the eye's plane. 2D scenes don't do this.
        if corners.iter().any(|corner| corner.z <= f32::EPSILON) {
            return None;
        }
        let corners = corners.map(|corner| corner.truncate() / corner.z);
        let min = corners.iter().fold(Vec2::INFINITY, |min, &c| min.min(c));
        let max = corners
            .iter()
            .fold(Vec2::NEG_INFINITY, |max, &c| max.max(c));
        // Pixels whose centers can be inside: the center of column x is at x + 0.5.
        let bounds = [
            (min.x - 0.5).ceil().max(clip[0] as f32) as u32,
            (min.y - 0.5).ceil().max(clip[1] as f32) as u32,
            ((max.x - 0.5).floor() + 1.0).clamp(0.0, clip[2] as f32) as u32,
            ((max.y - 0.5).floor() + 1.0).clamp(0.0, clip[3] as f32) as u32,
        ];
        if bounds[0] >= bounds[2] || bounds[1] >= bounds[3] {
            return None;
        }
        if pixel_from_unit.determinant().abs() <= f32::EPSILON {
            return None;
        }
        let mut unit_from_pixel = pixel_from_unit.inverse();
        let w = unit_from_pixel.row(2);
        let affine = w.x.abs() <= w.z.abs() * 1e-7 && w.y.abs() <= w.z.abs() * 1e-7;
        if affine {
            unit_from_pixel = Mat3::from_cols(
                (unit_from_pixel.x_axis / w.z).with_z(0.0),
                (unit_from_pixel.y_axis / w.z).with_z(0.0),
                (unit_from_pixel.z_axis / w.z).with_z(1.0),
            );
        }
        Some(Self {
            unit_from_pixel,
            affine,
            bounds,
            paint,
            color,
            alpha,
        })
    }

    /// The `(s, t)` at the center of a pixel, and whether it's inside the quad.
    #[inline(always)]
    fn unit_at(&self, x: f32, y: f32) -> Vec2 {
        let p = self.unit_from_pixel * Vec3::new(x, y, 1.0);
        if self.affine {
            p.truncate()
        } else {
            p.truncate() / p.z
        }
    }

    /// Picks the filter a GPU would for this quad: the texture's minifying filter where a pixel
    /// spans more than a texel, its magnifying one elsewhere.
    pub(crate) fn pick_filter(&mut self) {
        let Paint::Texture {
            texture,
            map,
            filter,
        } = &mut self.paint
        else {
            return;
        };
        let center = Vec2::new(
            (self.bounds[0] + self.bounds[2]) as f32 * 0.5,
            (self.bounds[1] + self.bounds[3]) as f32 * 0.5,
        );
        let unit = |p: Vec2| {
            let p = self.unit_from_pixel * p.extend(1.0);
            if self.affine {
                p.truncate()
            } else {
                p.truncate() / p.z
            }
        };
        let at = map.texel(unit(center));
        let across = (map.texel(unit(center + Vec2::X)) - at).length();
        let down = (map.texel(unit(center + Vec2::Y)) - at).length();
        *filter = if across.max(down) > 1.0 + 1e-4 {
            texture.min_filter
        } else {
            texture.mag_filter
        };
    }

    /// Fills the quad's pixels in a band of rows of a framebuffer `width` pixels wide whose first
    /// row is `first_row`. With `ALPHA`, the pixels keep alpha in their top byte, as an image a
    /// camera draws to does.
    pub(crate) fn fill<const ALPHA: bool>(
        &self,
        tables: &Tables,
        band: &mut [u32],
        width: u32,
        first_row: u32,
    ) {
        let band_rows = band.len() as u32 / width;
        let rows = self.bounds[1].max(first_row)..self.bounds[3].min(first_row + band_rows);
        if rows.is_empty() {
            return;
        }
        let [x0, _, x1, _] = self.bounds;
        if self.fill_exact::<ALPHA>(tables, band, width, first_row, rows.clone()) {
            return;
        }
        let [r, g, b, a] = self.color;
        for y in rows {
            let row = &mut band[((y - first_row) * width) as usize..][..width as usize];
            let start =
                Vec3A::from(self.unit_from_pixel * Vec3::new(x0 as f32 + 0.5, y as f32 + 0.5, 1.0));
            let step = Vec3A::from(self.unit_from_pixel.x_axis);
            for (i, pixel) in row[x0 as usize..x1 as usize].iter_mut().enumerate() {
                let p = start + step * i as f32;
                let unit = if self.affine {
                    Vec2::new(p.x, p.y)
                } else {
                    Vec2::new(p.x / p.z, p.y / p.z)
                };
                if !(0.0..1.0).contains(&unit.x) || !(0.0..1.0).contains(&unit.y) {
                    continue;
                }
                let rgba = match &self.paint {
                    Paint::Solid => self.color,
                    Paint::Texture {
                        texture,
                        map,
                        filter,
                    } => {
                        let texel = map.texel(unit);
                        let [tr, tg, tb, ta] = texture.sample(tables, texel.x, texel.y, *filter);
                        [tr * r, tg * g, tb * b, ta * a]
                    }
                };
                let alpha = match self.alpha {
                    Alpha::Blend => rgba[3],
                    Alpha::Opaque => 1.0,
                    Alpha::Mask(cutoff) if rgba[3] >= cutoff => 1.0,
                    Alpha::Mask(_) => continue,
                };
                tables.blend::<ALPHA>(pixel, [rgba[0], rgba[1], rgba[2]], alpha);
            }
        }
    }

    /// Fills the quad by copying texels, when each pixel's center lands on a texel's center and
    /// nothing tints them: an image drawn at its own size, like a canvas the size of the window.
    /// Returns whether it could.
    fn fill_exact<const ALPHA: bool>(
        &self,
        tables: &Tables,
        band: &mut [u32],
        width: u32,
        first_row: u32,
        rows: core::ops::Range<u32>,
    ) -> bool {
        let Paint::Texture {
            texture,
            map: TexelMap::Affine(map),
            ..
        } = &self.paint
        else {
            return false;
        };
        if !self.affine
            || self.color != [1.0; 4]
            || matches!(self.alpha, Alpha::Mask(_))
            || !texture.is_srgb()
        {
            return false;
        }
        // Pixel to texel: identity, plus an offset that takes pixel centers to texel centers.
        let texel_from_pixel = *map
            * Affine2::from_mat3(Mat3::from_cols(
                self.unit_from_pixel.x_axis.with_z(0.0),
                self.unit_from_pixel.y_axis.with_z(0.0),
                self.unit_from_pixel.z_axis.with_z(1.0),
            ));
        let near = |a: f32, b: f32| (a - b).abs() <= 1e-4;
        let m = texel_from_pixel.matrix2;
        if !(near(m.x_axis.x, 1.0)
            && near(m.x_axis.y, 0.0)
            && near(m.y_axis.x, 0.0)
            && near(m.y_axis.y, 1.0))
        {
            return false;
        }
        let offset = texel_from_pixel.translation;
        let (dx, dy) = (offset.x.round(), offset.y.round());
        if !near(offset.x, dx) || !near(offset.y, dy) {
            return false;
        }
        let [x0, _, x1, _] = self.bounds;
        let opaque = self.alpha == Alpha::Opaque;
        for y in rows {
            let row = &mut band[((y - first_row) * width) as usize..][..width as usize];
            let texel_y = y as i64 + dy as i64;
            if texel_y < 0 || texel_y >= i64::from(texture.height) {
                continue;
            }
            // The quad covers a run of whole texels; check its ends by their centers like any
            // other pixel, which also keeps to the texture.
            for x in x0..x1 {
                let unit = self.unit_at(x as f32 + 0.5, y as f32 + 0.5);
                if !(0.0..1.0).contains(&unit.x) || !(0.0..1.0).contains(&unit.y) {
                    continue;
                }
                let texel_x = x as i64 + dx as i64;
                if texel_x < 0 || texel_x >= i64::from(texture.width) {
                    continue;
                }
                let [r, g, b, a] = texture.rgba8(texel_x as u32, texel_y as u32);
                let pixel = &mut row[x as usize];
                if a == 255 || opaque {
                    *pixel = (u32::from(r) << 16)
                        | (u32::from(g) << 8)
                        | u32::from(b)
                        | if ALPHA { OPAQUE } else { 0 };
                } else if a != 0 {
                    tables.blend::<ALPHA>(
                        pixel,
                        [
                            tables.decode(u32::from(r)),
                            tables.decode(u32::from(g)),
                            tables.decode(u32::from(b)),
                        ],
                        f32::from(a) / 255.0,
                    );
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill(quad: &Quad, width: u32, height: u32) -> Vec<u32> {
        let mut pixels = vec![0; (width * height) as usize];
        quad.fill::<false>(&Tables::get(), &mut pixels, width, 0);
        pixels
    }

    #[test]
    fn covers_pixels_whose_centers_are_inside() {
        // The square spans pixels 1..3 across and down.
        let pixel_from_unit =
            Mat3::from_cols(Vec3::X * 2.0, Vec3::Y * 2.0, Vec3::new(1.0, 1.0, 1.0));
        let quad = Quad::new(
            pixel_from_unit,
            [0, 0, 4, 4],
            Paint::Solid,
            [1.0; 4],
            Alpha::Blend,
        )
        .unwrap();
        assert_eq!(quad.bounds, [1, 1, 3, 3]);
        let white = 0xFF_FF_FF;
        assert_eq!(
            fill(&quad, 4, 4),
            [0, 0, 0, 0, 0, white, white, 0, 0, white, white, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn keeps_inside_the_clip() {
        let pixel_from_unit = Mat3::from_cols(Vec3::X * 4.0, Vec3::Y * 4.0, Vec3::Z);
        let quad = Quad::new(
            pixel_from_unit,
            [1, 0, 3, 2],
            Paint::Solid,
            [1.0; 4],
            Alpha::Blend,
        )
        .unwrap();
        assert_eq!(quad.bounds, [1, 0, 3, 2]);
        let outside = Quad::new(
            pixel_from_unit,
            [5, 5, 8, 8],
            Paint::Solid,
            [1.0; 4],
            Alpha::Blend,
        );
        assert!(outside.is_none());
    }

    #[test]
    fn slices_keep_corners_unscaled() {
        // A texture with 0.25 borders on a quad twice its size: corners keep their texels.
        let slice = SliceUv {
            scale: Vec2::splat(2.0),
            min_inset: Vec2::splat(0.25),
            max_inset: Vec2::splat(0.25),
            side_stretch: Vec2::ZERO,
            center_stretch: Vec2::ZERO,
        };
        for (uv, expected) in [(0.1, 0.2), (0.9, 0.8), (0.5, 0.5)] {
            let mapped = slice.uv(Vec2::splat(uv));
            assert!(
                mapped.abs_diff_eq(Vec2::splat(expected), 1e-6),
                "{uv} -> {mapped}"
            );
        }
    }
}
