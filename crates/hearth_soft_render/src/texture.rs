//! Images as the renderer reads them: 8-bit texels, and how to filter them.

use hearth_asset::{AssetEvent, AssetId, Assets};
use hearth_color::ColorToPacked;
use hearth_ecs::{message::MessageReader, resource::Resource, system::ResMut};
use hearth_image::{
    Image, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor, TextureFormat,
};
use hearth_platform::collections::{HashMap, HashSet};
use tracing::warn;

use crate::color::Tables;

/// The sampler for images whose [`ImageSampler`] is `Default`, from the `ImagePlugin`.
#[derive(Resource, Clone)]
pub(crate) struct DefaultSampler(pub ImageSamplerDescriptor);

impl Default for DefaultSampler {
    fn default() -> Self {
        Self(ImageSamplerDescriptor::linear())
    }
}

/// Images in formats the renderer can't read directly, converted to 8-bit sRGB RGBA.
///
/// An image is converted the first frame it's drawn, and again after it changes.
#[derive(Resource, Default)]
pub(crate) struct ConvertedImages {
    images: HashMap<AssetId<Image>, Option<Vec<u8>>>,
    warned: HashSet<AssetId<Image>>,
}

/// Forgets the conversions of images that changed or went.
pub(crate) fn forget_changed_images(
    mut events: MessageReader<AssetEvent<Image>>,
    mut converted: ResMut<ConvertedImages>,
) {
    for event in events.read() {
        match event {
            AssetEvent::Modified { id }
            | AssetEvent::Removed { id }
            | AssetEvent::Unused { id } => {
                converted.images.remove(id);
            }
            AssetEvent::Added { .. } | AssetEvent::LoadedWithDependencies { .. } => {}
        }
    }
}

impl ConvertedImages {
    /// Converts the images among `ids` that need it.
    pub(crate) fn convert(
        &mut self,
        ids: impl IntoIterator<Item = AssetId<Image>>,
        images: &Assets<Image>,
    ) {
        for id in ids {
            let Some(image) = images.get(id) else {
                continue;
            };
            if Layout::of(image.texture_descriptor.format).is_some() || image.data.is_none() {
                continue;
            }
            let ConvertedImages {
                images: done,
                warned,
            } = self;
            done.entry(id).or_insert_with(|| {
                let size = image.size();
                let mut texels = Vec::with_capacity(size.element_product() as usize * 4);
                for y in 0..size.y {
                    for x in 0..size.x {
                        let Ok(color) = image.get_color_at(x, y) else {
                            if warned.insert(id) {
                                warn!(
                                    "The soft renderer can't read {:?} images, so it skips {id}",
                                    image.texture_descriptor.format
                                );
                            }
                            return None;
                        };
                        texels.extend_from_slice(&color.to_srgba().to_u8_array());
                    }
                }
                Some(texels)
            });
        }
    }
}

/// How an image's texels are laid out, for the formats the renderer reads directly.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Layout {
    /// Blue first, as in `Bgra8Unorm`.
    bgra: bool,
    /// The texels are sRGB-encoded.
    srgb: bool,
}

impl Layout {
    fn of(format: TextureFormat) -> Option<Self> {
        let (bgra, srgb) = match format {
            TextureFormat::Rgba8UnormSrgb => (false, true),
            TextureFormat::Rgba8Unorm => (false, false),
            TextureFormat::Bgra8UnormSrgb => (true, true),
            TextureFormat::Bgra8Unorm => (true, false),
            _ => return None,
        };
        Some(Self { bgra, srgb })
    }
}

/// An image the renderer can sample: its texels and how to filter and wrap them.
pub(crate) struct Texture<'a> {
    pub width: u32,
    pub height: u32,
    texels: &'a [u8],
    layout: Layout,
    pub mag_filter: ImageFilterMode,
    pub min_filter: ImageFilterMode,
    address_u: ImageAddressMode,
    address_v: ImageAddressMode,
}

impl<'a> Texture<'a> {
    /// The image with this id, if it has pixels the renderer can read.
    pub(crate) fn new(
        id: AssetId<Image>,
        images: &'a Assets<Image>,
        converted: &'a ConvertedImages,
        default_sampler: &'a ImageSamplerDescriptor,
    ) -> Option<Self> {
        let image = images.get(id)?;
        let (texels, layout) = match Layout::of(image.texture_descriptor.format) {
            Some(layout) => (image.data.as_deref()?, layout),
            None => (
                converted.images.get(&id)?.as_deref()?,
                Layout {
                    bgra: false,
                    srgb: true,
                },
            ),
        };
        let size = image.size();
        if size.x == 0 || size.y == 0 || texels.len() < size.element_product() as usize * 4 {
            return None;
        }
        let sampler = match &image.sampler {
            ImageSampler::Default => default_sampler,
            ImageSampler::Descriptor(descriptor) => descriptor,
        };
        Some(Self {
            width: size.x,
            height: size.y,
            texels,
            layout,
            mag_filter: sampler.mag_filter,
            min_filter: sampler.min_filter,
            address_u: sampler.address_mode_u,
            address_v: sampler.address_mode_v,
        })
    }

    /// The bytes of the texel at a column and row inside the image, as R, G, B, A.
    #[inline(always)]
    fn bytes(&self, x: u32, y: u32) -> [u8; 4] {
        let at = (y as usize * self.width as usize + x as usize) * 4;
        let mut bytes: [u8; 4] = self.texels[at..at + 4].try_into().unwrap();
        if self.layout.bgra {
            bytes.swap(0, 2);
        }
        bytes
    }

    /// The texel at a column and row inside the image, in linear color.
    #[inline(always)]
    fn texel(&self, tables: &Tables, x: u32, y: u32) -> [f32; 4] {
        let [r, g, b, a] = self.bytes(x, y);
        let a = f32::from(a) / 255.0;
        if self.layout.srgb {
            [
                tables.decode(u32::from(r)),
                tables.decode(u32::from(g)),
                tables.decode(u32::from(b)),
                a,
            ]
        } else {
            [
                f32::from(r) / 255.0,
                f32::from(g) / 255.0,
                f32::from(b) / 255.0,
                a,
            ]
        }
    }

    /// Whether the texels are sRGB R, G, B, A bytes, which [`Texture::rgba8`] reads.
    pub(crate) fn is_srgb_rgba(&self) -> bool {
        self.layout.srgb && !self.layout.bgra
    }

    /// The bytes of a texel, without filtering, for [`Texture::is_srgb_rgba`] images.
    #[inline(always)]
    pub(crate) fn rgba8(&self, x: u32, y: u32) -> [u8; 4] {
        self.bytes(x, y)
    }

    /// Samples at a point in texels (the texel at column `x`, row `y` covers `x..x + 1` and
    /// `y..y + 1`), with the given filter, in linear color.
    #[inline(always)]
    pub(crate) fn sample(
        &self,
        tables: &Tables,
        x: f32,
        y: f32,
        filter: ImageFilterMode,
    ) -> [f32; 4] {
        match filter {
            ImageFilterMode::Nearest => {
                let column = wrap(x.floor() as i64, self.width, self.address_u);
                let row = wrap(y.floor() as i64, self.height, self.address_v);
                self.texel(tables, column, row)
            }
            ImageFilterMode::Linear => {
                let (x, y) = (x - 0.5, y - 0.5);
                let (left, top) = (x.floor(), y.floor());
                let (fx, fy) = (x - left, y - top);
                let (left, top) = (left as i64, top as i64);
                let columns = [
                    wrap(left, self.width, self.address_u),
                    wrap(left + 1, self.width, self.address_u),
                ];
                let rows = [
                    wrap(top, self.height, self.address_v),
                    wrap(top + 1, self.height, self.address_v),
                ];
                let mix = |a: [f32; 4], b: [f32; 4], t: f32| {
                    core::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
                };
                let upper = mix(
                    self.texel(tables, columns[0], rows[0]),
                    self.texel(tables, columns[1], rows[0]),
                    fx,
                );
                let lower = mix(
                    self.texel(tables, columns[0], rows[1]),
                    self.texel(tables, columns[1], rows[1]),
                    fx,
                );
                mix(upper, lower, fy)
            }
        }
    }
}

/// A texel's column or row, brought inside the image by its address mode.
#[inline(always)]
fn wrap(index: i64, size: u32, mode: ImageAddressMode) -> u32 {
    let size = i64::from(size);
    let index = match mode {
        ImageAddressMode::Repeat => index.rem_euclid(size),
        ImageAddressMode::MirrorRepeat => {
            let period = index.rem_euclid(size * 2);
            if period < size {
                period
            } else {
                size * 2 - 1 - period
            }
        }
        ImageAddressMode::ClampToEdge | ImageAddressMode::ClampToBorder => index.clamp(0, size - 1),
    };
    index as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_by_address_mode() {
        let wrapped = |mode| [-2, -1, 0, 3, 4, 5].map(|index| wrap(index, 4, mode));
        assert_eq!(wrapped(ImageAddressMode::ClampToEdge), [0, 0, 0, 3, 3, 3]);
        assert_eq!(wrapped(ImageAddressMode::Repeat), [2, 3, 0, 3, 0, 1]);
        assert_eq!(wrapped(ImageAddressMode::MirrorRepeat), [1, 0, 0, 3, 3, 2]);
    }
}
