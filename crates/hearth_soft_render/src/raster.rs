//! Filling a framebuffer with quads.
//!
//! Everything the renderer draws is a quad: the unit square of `(s, t)`, `s` rightward and `t`
//! downward, placed in the scene by an affine transform and seen through a camera. Seen through a
//! camera, the square lands on the framebuffer as a quadrilateral, and a 3x3 matrix takes each
//! pixel's center back to the `(s, t)` there. A pixel is covered if its center is inside the
//! square, and then gets the quad's color, or a sample of its texture at the texel `(s, t)` maps
//! to.
//!
//! A camera that multisamples, as cameras do on the GPU unless their `Msaa` is off, covers a
//! pixel sample by sample instead, at the points a GPU samples, and colors each covered sample
//! with what's at the pixel's center. A pixel's samples are its color until an edge crosses it;
//! then they're kept apart, in [`Samples`], until they're averaged into it.
//!
//! A UI node's quad also has a [`RoundedBox`], which fades it at its edges and rounds its
//! corners, and the rectangles its ancestors clip it to.

use hearth_image::ImageFilterMode;
use hearth_math::{Affine2, Mat3, Rect, Vec2, Vec3, Vec3A};

use crate::{
    color::{alpha_byte, Tables, OPAQUE},
    rounded::RoundedBox,
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
    /// A UI image's nine-slicing or tiling.
    #[cfg_attr(
        not(feature = "hearth_ui"),
        expect(dead_code, reason = "Only UI images slice this way.")
    )]
    UiSlice(UiSliceUv),
}

impl TexelMap {
    #[inline(always)]
    fn texel(&self, unit: Vec2) -> Vec2 {
        match self {
            TexelMap::Affine(map) => map.transform_point2(unit),
            TexelMap::Sprite(uv) => uv.texel(unit),
            TexelMap::UiSlice(uv) => uv.texel(unit),
        }
    }
}

/// A sliced or tiled UI image's mapping from its quad to its texture, as `hearth_ui_render`'s
/// slicing shader does it.
#[derive(Clone, Debug)]
pub(crate) struct UiSliceUv {
    /// Where the slices divide the texture's rect, as fractions of it: left, top, right, bottom.
    pub texture: [f32; 4],
    /// Where they divide the quad, in the same order.
    pub target: [f32; 4],
    /// How many times the sides repeat across and down, then the center.
    pub repeat: [f32; 4],
    /// The texture's rect, in texels: its left, top, right and bottom, swapped to flip.
    pub rect: [f32; 4],
}

impl UiSliceUv {
    fn texel(&self, uv: Vec2) -> Vec2 {
        let [left, top, right, bottom] = self.target;
        let inside = (left..=right).contains(&uv.x) && (top..=bottom).contains(&uv.y);
        let [x_repeat, y_repeat] = if inside {
            [self.repeat[2], self.repeat[3]]
        } else {
            [self.repeat[0], self.repeat[1]]
        };
        let axis =
            |p: f32, low: f32, high: f32, texture_low: f32, texture_high: f32, repeat: f32| {
                if p < low {
                    p / low * texture_low
                } else if high < p {
                    texture_high + (p - high) / (1.0 - high) * (1.0 - texture_high)
                } else {
                    let tiled = repeat * (p - low) / (high - low);
                    texture_low + (tiled - tiled.floor()) * (texture_high - texture_low)
                }
            };
        let x = axis(
            uv.x,
            left,
            right,
            self.texture[0],
            self.texture[2],
            x_repeat,
        );
        let y = axis(
            uv.y,
            top,
            bottom,
            self.texture[1],
            self.texture[3],
            y_repeat,
        );
        let [rect_left, rect_top, rect_right, rect_bottom] = self.rect;
        Vec2::new(
            rect_left + x * (rect_right - rect_left),
            rect_top + y * (rect_bottom - rect_top),
        )
    }
}

/// A rectangle a UI node is clipped to, in the space of the node that clips it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClipRect {
    /// Takes a pixel position into the clipping node's space.
    pub clip_from_pixel: Affine2,
    /// Its edges there; any may be infinite.
    pub rect: Rect,
}

impl ClipRect {
    /// Whether a pixel whose center is at `point` is inside.
    #[inline]
    fn contains(&self, point: Vec2) -> bool {
        let point = self.clip_from_pixel.transform_point2(point);
        self.rect.min.x <= point.x
            && point.x <= self.rect.max.x
            && self.rect.min.y <= point.y
            && point.y <= self.rect.max.y
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

/// Where a pixel's samples are, from its center, when a camera takes this many: the positions a
/// GPU samples at, which Direct3D, Vulkan and Metal share. None for one sample.
pub(crate) fn sample_offsets(samples: u32) -> &'static [Vec2] {
    const fn at(x: f32, y: f32) -> Vec2 {
        Vec2::new(x / 16.0, y / 16.0)
    }
    const TWO: [Vec2; 2] = [at(4.0, 4.0), at(-4.0, -4.0)];
    const FOUR: [Vec2; 4] = [at(-2.0, -6.0), at(6.0, -2.0), at(-6.0, 2.0), at(2.0, 6.0)];
    const EIGHT: [Vec2; 8] = [
        at(1.0, -3.0),
        at(-1.0, 3.0),
        at(5.0, 1.0),
        at(-3.0, -5.0),
        at(-5.0, 5.0),
        at(-7.0, -1.0),
        at(3.0, 7.0),
        at(7.0, -7.0),
    ];
    match samples {
        2 => &TWO,
        4 => &FOUR,
        8 => &EIGHT,
        _ => &[],
    }
}

/// While `SAME`, a pixel's samples are all its color.
const SAME: u32 = u32::MAX;

/// The samples of a band's pixels, for a camera that multisamples.
pub(crate) struct Samples {
    /// Where each sample is, from the pixel's center.
    offsets: &'static [Vec2],
    /// For each pixel of the band, where its samples start in `values`, or `SAME`.
    index: Vec<u32>,
    /// The samples kept apart, as pixels.
    values: Vec<u32>,
}

impl Samples {
    /// A band of `pixels` pixels, each with its samples at `offsets`, all its color so far.
    pub(crate) fn new(offsets: &'static [Vec2], pixels: usize) -> Self {
        Self {
            offsets,
            index: vec![SAME; pixels],
            values: Vec::new(),
        }
    }

    /// The samples of the `i`th pixel, which from now on are kept apart, starting as `pixel`.
    #[inline(always)]
    fn split(&mut self, i: usize, pixel: u32) -> &mut [u32] {
        let count = self.offsets.len();
        if self.index[i] == SAME {
            self.index[i] = self.values.len() as u32;
            self.values.extend(core::iter::repeat_n(pixel, count));
        }
        let start = self.index[i] as usize;
        &mut self.values[start..start + count]
    }

    /// The samples of the `i`th pixel, if they're kept apart.
    #[inline(always)]
    fn kept(&mut self, i: usize) -> Option<&mut [u32]> {
        let start = self.index[i];
        (start != SAME).then(|| &mut self.values[start as usize..][..self.offsets.len()])
    }

    /// Makes the `i`th pixel's samples all its color again.
    #[inline(always)]
    fn join(&mut self, i: usize) {
        self.index[i] = SAME;
    }

    /// Averages each pixel's samples into it, as a GPU resolves them: in linear light. With
    /// `ALPHA`, alpha too.
    pub(crate) fn resolve<const ALPHA: bool>(&self, tables: &Tables, band: &mut [u32]) {
        let count = self.offsets.len();
        let scale = 1.0 / count as f32;
        for (pixel, &start) in band.iter_mut().zip(&self.index) {
            if start == SAME {
                continue;
            }
            let mut sum = [0.0; 4];
            for &value in &self.values[start as usize..][..count] {
                let [r, g, b] = tables.unpack(value);
                sum[0] += r;
                sum[1] += g;
                sum[2] += b;
                sum[3] += (value >> 24) as f32;
            }
            *pixel = tables.pack([sum[0] * scale, sum[1] * scale, sum[2] * scale]);
            if ALPHA {
                *pixel |= alpha_byte(sum[3] * scale / 255.0) << 24;
            }
        }
    }
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
    /// When the camera multisamples, how far `unit_from_pixel * (x, y, 1)` moves from a pixel's
    /// center to each of its samples.
    sample_steps: [Vec3A; 8],
    /// How many samples a pixel has, or 0 if the camera doesn't multisample.
    samples: usize,
    /// For an affine quad, the farthest a sample's `(s, t)` is from its pixel center's, on each
    /// axis.
    sample_reach: Vec2,
    /// Its corners are on pixel corners, so its samples cover a pixel all or not at all.
    on_pixels: bool,
    /// A UI node's shape, which fades the quad at its edges.
    rounded: Option<RoundedBox>,
    /// Rectangles it's clipped to that `bounds` can't hold: turned ones.
    clip: Vec<ClipRect>,
}

impl<'a> Quad<'a> {
    /// Places a quad whose `(s, t, 1)` lands on the pixel `(x, y, 1)` times some factor by
    /// `pixel_from_unit`, inside `clip` (`x0, y0, x1, y1`), or `None` if it covers nothing there.
    /// Pixels are sampled at `sample_offsets` from their centers, or at their centers if there
    /// are none.
    pub(crate) fn new(
        pixel_from_unit: Mat3,
        clip: [u32; 4],
        paint: Paint<'a>,
        color: [f32; 4],
        alpha: Alpha,
        sample_offsets: &[Vec2],
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
        // Pixels whose samples can be inside: the center of column x is at x + 0.5, and its
        // samples are up to `spread` from it.
        let spread = sample_offsets.iter().fold(0.0_f32, |spread, offset| {
            spread.max(offset.abs().max_element())
        });
        let bounds = [
            (min.x - 0.5 - spread).ceil().max(clip[0] as f32) as u32,
            (min.y - 0.5 - spread).ceil().max(clip[1] as f32) as u32,
            ((max.x - 0.5 + spread).floor() + 1.0).clamp(0.0, clip[2] as f32) as u32,
            ((max.y - 0.5 + spread).floor() + 1.0).clamp(0.0, clip[3] as f32) as u32,
        ];
        let rounded = corners.map(Vec2::round);
        let square = |a: Vec2, b: Vec2| a.x == b.x || a.y == b.y;
        let on_pixels = rounded
            .iter()
            .zip(&corners)
            .all(|(rounded, corner)| rounded.abs_diff_eq(*corner, 1e-4))
            && square(rounded[0], rounded[1])
            && square(rounded[0], rounded[2]);
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
        let mut sample_steps = [Vec3A::ZERO; 8];
        let mut sample_reach = Vec2::ZERO;
        for (step, offset) in sample_steps.iter_mut().zip(sample_offsets) {
            *step =
                Vec3A::from(unit_from_pixel.x_axis * offset.x + unit_from_pixel.y_axis * offset.y);
            sample_reach = sample_reach.max(Vec2::new(step.x.abs(), step.y.abs()));
        }
        Some(Self {
            unit_from_pixel,
            affine,
            bounds,
            paint,
            color,
            alpha,
            sample_steps,
            samples: sample_offsets.len().min(8),
            sample_reach,
            on_pixels,
            rounded: None,
            clip: Vec::new(),
        })
    }

    /// Makes the quad a UI node's: faded at the edges of `rounded`, and clipped to `clip`, or
    /// `None` if that leaves nothing.
    #[cfg_attr(
        all(not(feature = "hearth_ui"), not(test)),
        expect(dead_code, reason = "Only UI nodes are rounded and clipped.")
    )]
    pub(crate) fn with_ui(
        mut self,
        rounded: Option<RoundedBox>,
        clip: impl IntoIterator<Item = ClipRect>,
    ) -> Option<Self> {
        self.rounded = rounded;
        for clip in clip {
            let m = clip.clip_from_pixel.matrix2;
            if m.x_axis.y != 0.0 || m.y_axis.x != 0.0 || m.x_axis.x == 0.0 || m.y_axis.y == 0.0 {
                self.clip.push(clip);
                continue;
            }
            // Upright: the pixels whose centers are inside, as bounds. Each axis maps alone, as
            // an edge may be infinite.
            let pixel_from_clip = clip.clip_from_pixel.inverse();
            let (scale, offset) = (
                Vec2::new(
                    pixel_from_clip.matrix2.x_axis.x,
                    pixel_from_clip.matrix2.y_axis.y,
                ),
                pixel_from_clip.translation,
            );
            let a = clip.rect.min * scale + offset;
            let b = clip.rect.max * scale + offset;
            let (min, max) = (a.min(b), a.max(b));
            let [x0, y0, x1, y1] = &mut self.bounds;
            *x0 = (*x0 as f32).max((min.x - 0.5).ceil()) as u32;
            *y0 = (*y0 as f32).max((min.y - 0.5).ceil()) as u32;
            *x1 = (*x1 as f32).min((max.x - 0.5).floor() + 1.0).max(0.0) as u32;
            *y1 = (*y1 as f32).min((max.y - 0.5).floor() + 1.0).max(0.0) as u32;
        }
        (self.bounds[0] < self.bounds[2] && self.bounds[1] < self.bounds[3]).then_some(self)
    }

    /// Which of a pixel's samples are inside the quad, a bit each, from `p`, which is
    /// `unit_from_pixel` times its center, and `unit`, the `(s, t)` there.
    #[inline(always)]
    fn coverage(&self, p: Vec3A, unit: Vec2) -> u32 {
        let all = (1 << self.samples) - 1;
        let inside = |unit: Vec2| (0.0..1.0).contains(&unit.x) && (0.0..1.0).contains(&unit.y);
        if self.affine {
            // Most pixels are well inside or outside; only those near an edge need each sample.
            let reach = self.sample_reach;
            if unit.cmpge(reach).all() && unit.cmplt(Vec2::ONE - reach).all() {
                return all;
            }
            if unit.cmplt(-reach).any() || unit.cmpge(Vec2::ONE + reach).any() {
                return 0;
            }
        }
        let mut covered = 0;
        for (i, step) in self.sample_steps[..self.samples].iter().enumerate() {
            let q = p + *step;
            let unit = if self.affine {
                Vec2::new(q.x, q.y)
            } else {
                Vec2::new(q.x / q.z, q.y / q.z)
            };
            if inside(unit) {
                covered |= 1 << i;
            }
        }
        covered
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
    /// row is `first_row`, with the band's `samples` if the camera multisamples. With `ALPHA`,
    /// the pixels keep alpha in their top byte, as an image a camera draws to does.
    pub(crate) fn fill<const ALPHA: bool>(
        &self,
        tables: &Tables,
        band: &mut [u32],
        width: u32,
        first_row: u32,
        mut samples: Option<&mut Samples>,
    ) {
        let band_rows = band.len() as u32 / width;
        let rows = self.bounds[1].max(first_row)..self.bounds[3].min(first_row + band_rows);
        if rows.is_empty() {
            return;
        }
        let [x0, _, x1, _] = self.bounds;
        if self.fill_exact::<ALPHA>(tables, band, width, first_row, rows.clone(), &mut samples) {
            return;
        }
        // Without samples, a covered pixel is one "sample", its center.
        let all = (1 << self.samples.max(1)) - 1;
        let [r, g, b, a] = self.color;
        for y in rows {
            let row_start = ((y - first_row) * width) as usize;
            let row = &mut band[row_start..][..width as usize];
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
                let covered = if samples.is_some() {
                    self.coverage(p, unit)
                } else if (0.0..1.0).contains(&unit.x) && (0.0..1.0).contains(&unit.y) {
                    all
                } else {
                    0
                };
                if covered == 0 {
                    continue;
                }
                if !self.clip.is_empty() {
                    let center = Vec2::new((x0 as usize + i) as f32 + 0.5, y as f32 + 0.5);
                    if !self.clip.iter().all(|clip| clip.contains(center)) {
                        continue;
                    }
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
                let alpha = match &self.rounded {
                    Some(rounded) => alpha * rounded.coverage(unit),
                    None => alpha,
                };
                if alpha <= 0.0 {
                    continue;
                }
                let rgb = [rgba[0], rgba[1], rgba[2]];
                let Some(samples) = samples.as_deref_mut() else {
                    tables.blend::<ALPHA>(pixel, rgb, alpha);
                    continue;
                };
                let index = row_start + x0 as usize + i;
                if covered == all && (alpha >= 1.0 || samples.index[index] == SAME) {
                    tables.blend::<ALPHA>(pixel, rgb, alpha);
                    samples.join(index);
                } else {
                    let values = samples.split(index, *pixel);
                    for (sample, value) in values.iter_mut().enumerate() {
                        if covered & (1 << sample) != 0 {
                            tables.blend::<ALPHA>(value, rgb, alpha);
                        }
                    }
                }
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
        samples: &mut Option<&mut Samples>,
    ) -> bool {
        let Paint::Texture {
            texture,
            map: TexelMap::Affine(map),
            ..
        } = &self.paint
        else {
            return false;
        };
        // Multisampled, it covers pixels all or not at all only if its corners are on theirs.
        if !self.affine
            || self.color != [1.0; 4]
            || matches!(self.alpha, Alpha::Mask(_))
            || !texture.is_srgb()
            || (samples.is_some() && !self.on_pixels)
            || self.rounded.is_some()
            || !self.clip.is_empty()
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
            let row_start = ((y - first_row) * width) as usize;
            let row = &mut band[row_start..][..width as usize];
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
                let index = row_start + x as usize;
                if a == 255 || opaque {
                    *pixel = (u32::from(r) << 16)
                        | (u32::from(g) << 8)
                        | u32::from(b)
                        | if ALPHA { OPAQUE } else { 0 };
                    if let Some(samples) = samples.as_deref_mut() {
                        samples.join(index);
                    }
                } else if a != 0 {
                    let rgb = [
                        tables.decode(u32::from(r)),
                        tables.decode(u32::from(g)),
                        tables.decode(u32::from(b)),
                    ];
                    let alpha = f32::from(a) / 255.0;
                    match samples
                        .as_deref_mut()
                        .and_then(|samples| samples.kept(index))
                    {
                        Some(values) => {
                            for value in values {
                                tables.blend::<ALPHA>(value, rgb, alpha);
                            }
                        }
                        None => tables.blend::<ALPHA>(pixel, rgb, alpha),
                    }
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
        quad.fill::<false>(&Tables::get(), &mut pixels, width, 0, None);
        pixels
    }

    /// Fills quads into a framebuffer with 4 samples a pixel, and resolves them.
    fn fill_multisampled(quads: &[Quad], width: u32, height: u32) -> Vec<u32> {
        let tables = Tables::get();
        let mut pixels = vec![0; (width * height) as usize];
        let mut samples = Samples::new(sample_offsets(4), pixels.len());
        for quad in quads {
            quad.fill::<false>(&tables, &mut pixels, width, 0, Some(&mut samples));
        }
        samples.resolve::<false>(&tables, &mut pixels);
        pixels
    }

    fn solid(pixel_from_unit: Mat3, color: [f32; 4]) -> Quad<'static> {
        Quad::new(
            pixel_from_unit,
            [0, 0, 4, 4],
            Paint::Solid,
            color,
            Alpha::Blend,
            sample_offsets(4),
        )
        .unwrap()
    }

    #[test]
    fn multisamples_edges() {
        // White over columns 0 to 1.5: column 1 has half its samples, linear 0.5, sRGB 188.
        let quad = solid(
            Mat3::from_cols(Vec3::X * 1.5, Vec3::Y * 4.0, Vec3::Z),
            [1.0; 4],
        );
        let pixels = fill_multisampled(&[quad], 4, 4);
        assert_eq!(&pixels[..4], &[0xFF_FF_FF, 0xBC_BC_BC, 0, 0]);
    }

    #[test]
    fn multisampled_neighbors_leave_no_seam() {
        // Red to column 1.5, green after it: column 1 is half each, with no black between.
        let red = solid(
            Mat3::from_cols(Vec3::X * 1.5, Vec3::Y * 4.0, Vec3::Z),
            [1.0, 0.0, 0.0, 1.0],
        );
        let green = solid(
            Mat3::from_cols(Vec3::X * 2.5, Vec3::Y * 4.0, Vec3::new(1.5, 0.0, 1.0)),
            [0.0, 1.0, 0.0, 1.0],
        );
        let pixels = fill_multisampled(&[red, green], 4, 4);
        assert_eq!(pixels[1], 0xBC_BC_00);
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
            &[],
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
            &[],
        )
        .unwrap();
        assert_eq!(quad.bounds, [1, 0, 3, 2]);
        let outside = Quad::new(
            pixel_from_unit,
            [5, 5, 8, 8],
            Paint::Solid,
            [1.0; 4],
            Alpha::Blend,
            &[],
        );
        assert!(outside.is_none());
    }

    #[test]
    fn clips_to_upright_rects_by_bounds() {
        let pixel_from_unit = Mat3::from_cols(Vec3::X * 4.0, Vec3::Y * 4.0, Vec3::Z);
        let solid = || {
            Quad::new(
                pixel_from_unit,
                [0, 0, 4, 4],
                Paint::Solid,
                [1.0; 4],
                Alpha::Blend,
                &[],
            )
            .unwrap()
        };
        // Clip space is pixels shifted by 1: its rect from 0.5 to 1 holds column 1's centers only.
        let clip = ClipRect {
            clip_from_pixel: Affine2::from_translation(Vec2::splat(-1.0)),
            rect: Rect::new(0.5, -f32::INFINITY, 1.0, f32::INFINITY),
        };
        let quad = solid().with_ui(None, [clip]).unwrap();
        assert_eq!(quad.bounds, [1, 0, 2, 4]);
        assert!(quad.clip.is_empty());

        // Turned a quarter, it's checked pixel by pixel, and covers the same.
        let turned = ClipRect {
            clip_from_pixel: Affine2::from_angle(core::f32::consts::FRAC_PI_2)
                * Affine2::from_translation(Vec2::splat(-1.0)),
            rect: Rect::new(-f32::INFINITY, 0.5, f32::INFINITY, 1.0),
        };
        let quad = solid().with_ui(None, [turned]).unwrap();
        assert_eq!(quad.clip.len(), 1);
        let pixels = fill(&quad, 4, 4);
        let white = 0xFF_FF_FF;
        assert_eq!(&pixels[..4], &[0, white, 0, 0]);
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
