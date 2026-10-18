//! Converting between the framebuffer's sRGB bytes and the linear values colors are mixed in.
//!
//! A pixel is `0x00RRGGBB` in sRGB. An image a camera draws to keeps its alpha in the top byte,
//! linear, as `0xAARRGGBB`: the alpha `ALPHA` speaks of.

use std::sync::LazyLock;

use hearth_color::LinearRgba;
use hearth_math::ops;

/// How many steps [`ENCODE`] divides linear values from 0 to 1 into.
const ENCODE_STEPS: usize = 1 << 14;

/// Each sRGB byte as a linear value.
pub(crate) static DECODE: LazyLock<[f32; 256]> =
    LazyLock::new(|| core::array::from_fn(|byte| srgb_to_linear(byte as f32 / 255.0)));

/// Linear values, `ENCODE_STEPS` to 1, as the nearest sRGB byte.
static ENCODE: LazyLock<Box<[u8]>> = LazyLock::new(|| {
    (0..=ENCODE_STEPS)
        .map(|step| (linear_to_srgb(step as f32 / ENCODE_STEPS as f32) * 255.0 + 0.5) as u8)
        .collect()
});

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ops::powf((value + 0.055) / 1.055, 2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * ops::powf(value, 1.0 / 2.4) - 0.055
    }
}

/// The tables, for loops that look values up many times.
pub(crate) struct Tables {
    decode: &'static [f32; 256],
    encode: &'static [u8],
}

impl Tables {
    pub(crate) fn get() -> Self {
        Self {
            decode: &DECODE,
            encode: &ENCODE,
        }
    }

    /// A linear value from 0 to 1 as an sRGB byte.
    #[inline(always)]
    pub(crate) fn encode(&self, linear: f32) -> u32 {
        let step = (linear.clamp(0.0, 1.0) * ENCODE_STEPS as f32 + 0.5) as usize;
        u32::from(self.encode[step.min(ENCODE_STEPS)])
    }

    /// An sRGB byte as a linear value.
    #[inline(always)]
    pub(crate) fn decode(&self, byte: u32) -> f32 {
        self.decode[(byte & 0xFF) as usize]
    }

    /// Linear red, green and blue as a framebuffer pixel.
    #[inline(always)]
    pub(crate) fn pack(&self, [r, g, b]: [f32; 3]) -> u32 {
        (self.encode(r) << 16) | (self.encode(g) << 8) | self.encode(b)
    }

    /// A framebuffer pixel as linear red, green and blue.
    #[inline(always)]
    pub(crate) fn unpack(&self, pixel: u32) -> [f32; 3] {
        [
            self.decode(pixel >> 16),
            self.decode(pixel >> 8),
            self.decode(pixel),
        ]
    }

    /// Mixes linear `rgb` over `pixel` by `alpha`, as a GPU blends into an sRGB target. With
    /// `ALPHA`, the pixel's alpha mixes as the GPU's does, to `alpha + under * (1 - alpha)`;
    /// without, its top byte stays 0.
    #[inline(always)]
    pub(crate) fn blend<const ALPHA: bool>(&self, pixel: &mut u32, rgb: [f32; 3], alpha: f32) {
        if alpha >= 1.0 {
            *pixel = self.pack(rgb) | if ALPHA { OPAQUE } else { 0 };
        } else if alpha > 0.0 {
            let under = self.unpack(*pixel);
            let keep = 1.0 - alpha;
            let mut mixed = self.pack([
                rgb[0] * alpha + under[0] * keep,
                rgb[1] * alpha + under[1] * keep,
                rgb[2] * alpha + under[2] * keep,
            ]);
            if ALPHA {
                let under_alpha = (*pixel >> 24) as f32 / 255.0;
                mixed |= alpha_byte(alpha + under_alpha * keep) << 24;
            }
            *pixel = mixed;
        }
    }
}

/// An opaque pixel's alpha, in the top byte.
pub(crate) const OPAQUE: u32 = 0xFF00_0000;

/// An alpha from 0 to 1 as a byte.
#[inline(always)]
pub(crate) fn alpha_byte(alpha: f32) -> u32 {
    (alpha.clamp(0.0, 1.0) * 255.0 + 0.5) as u32
}

/// A color as a framebuffer pixel, ignoring its alpha.
pub(crate) fn pixel(color: LinearRgba) -> u32 {
    Tables::get().pack([color.red, color.green, color.blue])
}

/// A color as a pixel with its alpha in the top byte.
pub(crate) fn pixel_with_alpha(color: LinearRgba) -> u32 {
    pixel(color) | alpha_byte(color.alpha) << 24
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_survive_the_round_trip() {
        let tables = Tables::get();
        for byte in 0..=255 {
            assert_eq!(tables.encode(tables.decode(byte)), byte);
        }
    }

    #[test]
    fn half_coverage_mixes_in_linear() {
        let tables = Tables::get();
        let mut pixel = 0;
        tables.blend::<false>(&mut pixel, [1.0; 3], 0.5);
        // Linear 0.5 is sRGB 188, not 128.
        assert_eq!(pixel, 0xBC_BC_BC);
    }

    #[test]
    fn alpha_mixes_as_on_the_gpu() {
        let tables = Tables::get();
        // Half white over transparent black: half alpha, and color mixed with the black.
        let mut pixel = 0;
        tables.blend::<true>(&mut pixel, [1.0; 3], 0.5);
        assert_eq!(pixel, 0x80_BC_BC_BC);
        // Half again: three quarters.
        tables.blend::<true>(&mut pixel, [1.0; 3], 0.5);
        assert!((pixel >> 24).abs_diff(191) <= 1);
    }
}
