//! Images cameras draw to: their pixels while a frame is drawn, written back into the images
//! after it.

use hearth_asset::{AssetId, Assets};
use hearth_color::{Color, ColorToComponents, LinearRgba};
use hearth_image::{Image, TextureFormatPixelInfo};

use crate::{
    color::{self, Tables},
    texture::Layout,
};

/// An image cameras draw to this frame, as pixels like a framebuffer's that keep alpha in the
/// top byte: `0xAARRGGBB`, the color in sRGB and the alpha linear.
pub(crate) struct ImageTarget {
    pub id: AssetId<Image>,
    pub width: u32,
    pub height: u32,
    /// Empty while a camera draws to it.
    pub pixels: Vec<u32>,
}

impl ImageTarget {
    /// The pixels of `image`, or with `load` false, transparent black, for a camera that clears
    /// all of it. `None` if the renderer can't read the image's format, or it has layers.
    pub(crate) fn new(
        id: AssetId<Image>,
        image: &Image,
        load: bool,
        tables: &Tables,
    ) -> Option<Self> {
        if image.texture_descriptor.size.depth_or_array_layers != 1 {
            return None;
        }
        let size = image.size();
        let count = size.element_product() as usize;
        let pixels = match image.data.as_deref() {
            Some(data) if load => match Layout::of(image.texture_descriptor.format) {
                Some(layout) => data
                    .get(..count * 4)?
                    .chunks_exact(4)
                    .map(|texel| {
                        let [r, g, b, a] = layout.rgba([texel[0], texel[1], texel[2], texel[3]]);
                        let [r, g, b] = [r, g, b].map(|channel| {
                            if layout.srgb {
                                u32::from(channel)
                            } else {
                                tables.encode(f32::from(channel) / 255.0)
                            }
                        });
                        (u32::from(a) << 24) | (r << 16) | (g << 8) | b
                    })
                    .collect(),
                None => (0..size.y)
                    .flat_map(|y| (0..size.x).map(move |x| (x, y)))
                    .map(|(x, y)| {
                        let color = image.get_color_at(x, y).ok()?;
                        Some(color::pixel_with_alpha(color.to_linear()))
                    })
                    .collect::<Option<_>>()?,
            },
            _ => vec![0; count],
        };
        Some(Self {
            id,
            width: size.x,
            height: size.y,
            pixels,
        })
    }

    /// Writes the pixels into the image, in its format, if they changed it. Returns `false` if
    /// the renderer can't write the format.
    pub(crate) fn write(&self, images: &mut Assets<Image>, tables: &Tables) -> bool {
        let Some(image) = images.get(self.id) else {
            return true;
        };
        if image.size().element_product() as usize != self.pixels.len() {
            // It changed size since this frame began: it's drawn at its new size next frame.
            return true;
        }
        if let Some(layout) = Layout::of(image.texture_descriptor.format) {
            let bytes: Vec<u8> = self
                .pixels
                .iter()
                .flat_map(|&pixel| {
                    let [b, g, r, a] = pixel.to_le_bytes();
                    let [r, g, b] = [r, g, b].map(|channel| {
                        if layout.srgb {
                            channel
                        } else {
                            (tables.decode(u32::from(channel)) * 255.0 + 0.5) as u8
                        }
                    });
                    layout.rgba([r, g, b, a])
                })
                .collect();
            if image.data.as_deref() != Some(&bytes)
                && let Some(mut image) = images.get_mut(self.id)
            {
                image.data = Some(bytes);
            }
            return true;
        }

        let Some(texel_size) = image.texture_descriptor.format.pixel_size().ok() else {
            return false;
        };
        let Some(mut image) = images.get_mut(self.id) else {
            return true;
        };
        if image.data.is_none() {
            image.data = Some(vec![0; self.pixels.len() * texel_size]);
        }
        for (i, &pixel) in self.pixels.iter().enumerate() {
            let (x, y) = (i as u32 % self.width, i as u32 / self.width);
            let [red, green, blue] = tables.unpack(pixel);
            let alpha = (pixel >> 24) as f32 / 255.0;
            let color = Color::LinearRgba(LinearRgba::from_f32_array([red, green, blue, alpha]));
            if image.set_color_at(x, y, color).is_err() {
                return false;
            }
        }
        true
    }
}
