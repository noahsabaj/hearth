//! Drawing what each camera sees into the framebuffers.

use hearth_asset::{AssetId, Assets};
use hearth_camera::{
    visibility::RenderLayers, Camera, ClearColor, ClearColorConfig, NormalizedRenderTarget,
    RenderTarget,
};
use hearth_ecs::{
    entity::{ContainsEntity, Entity},
    query::With,
    system::{Local, Query, Res, ResMut},
};
use hearth_image::{Image, ImageFilterMode, TextureAtlasLayout};
use hearth_math::{Affine3A, Mat3, Mat4, URect, UVec2, Vec3, Vec4};
use hearth_platform::collections::{HashMap, HashSet};
use hearth_tasks::{ComputeTaskPool, ParallelSliceMut, TaskPool};
use hearth_transform::components::GlobalTransform;
use hearth_window::{PrimaryWindow, Window};
use tracing::warn;

use crate::{
    color::{self, Tables},
    raster::{Alpha, Paint, Quad, TexelMap},
    sprite::{self, SpriteQuery},
    target::ImageTarget,
    texture::{ConvertedImages, DefaultSampler, Texture},
    Framebuffer,
};

/// Something to draw: the unit square of `(s, t)`, placed in the world, and what covers it.
pub(crate) struct Item<'a> {
    /// Takes `(s, t, 0)` into the world.
    pub world_from_unit: Affine3A,
    /// Items are drawn from the lowest `z` up.
    pub z: f32,
    pub layers: Option<&'a RenderLayers>,
    /// Linear RGBA: the color, or the image's tint.
    pub color: [f32; 4],
    pub paint: ItemPaint,
    pub alpha: Alpha,
}

/// What covers an [`Item`].
pub(crate) enum ItemPaint {
    /// Its color.
    #[cfg_attr(
        not(feature = "hearth_text"),
        expect(dead_code, reason = "Only text draws plain rectangles so far.")
    )]
    Solid,
    /// An image, sampled where `map` takes each `(s, t)`.
    Texture {
        image: AssetId<Image>,
        map: TexelMap,
    },
}

/// Gives each window a [`Framebuffer`] its size, in physical pixels.
pub(crate) fn fit_framebuffers(
    mut commands: hearth_ecs::system::Commands,
    mut windows: Query<(Entity, &Window, Option<&mut Framebuffer>)>,
) {
    for (entity, window, framebuffer) in &mut windows {
        let (width, height) = (window.physical_width(), window.physical_height());
        match framebuffer {
            Some(mut framebuffer) => {
                if framebuffer.width() != width || framebuffer.height() != height {
                    framebuffer.resize(width, height);
                }
            }
            None => {
                commands
                    .entity(entity)
                    .insert(Framebuffer::new(width, height));
            }
        }
    }
}

/// The rows of a framebuffer each task fills at once.
const BAND_ROWS: u32 = 16;

/// Draws what each camera sees into the framebuffer of its window, or into the image it draws
/// to.
///
/// Cameras draw in their order. One that samples an image an earlier camera drew to this frame
/// sees what it drew, as on the GPU; the images get their pixels once all have drawn.
pub(crate) fn draw(
    cameras: Query<(
        Entity,
        &Camera,
        &RenderTarget,
        &GlobalTransform,
        Option<&RenderLayers>,
    )>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    mut framebuffers: Query<(Entity, &mut Framebuffer), With<Window>>,
    clear_color: Res<ClearColor>,
    mut images: ResMut<Assets<Image>>,
    atlases: Res<Assets<TextureAtlasLayout>>,
    default_sampler: Res<DefaultSampler>,
    mut converted: ResMut<ConvertedImages>,
    sprites: SpriteQuery,
    #[cfg(feature = "hearth_text")] texts: crate::text::Texts,
    mut drawn: Local<Vec<Entity>>,
    mut warned: Local<HashSet<AssetId<Image>>>,
    mut warned_about_views: Local<bool>,
) {
    let mut items = Vec::new();
    sprite::collect(&sprites, &images, &atlases, &mut items);
    #[cfg(feature = "hearth_text")]
    texts.collect(&mut items);
    // A stable sort keeps the items of one entity, like a text's shadow and glyphs, in order.
    items.sort_by(|a, b| a.z.total_cmp(&b.z));

    let mut texture_ids: Vec<AssetId<Image>> = items
        .iter()
        .filter_map(|item| match item.paint {
            ItemPaint::Texture { image, .. } => Some(image),
            ItemPaint::Solid => None,
        })
        .collect();
    texture_ids.sort_unstable();
    texture_ids.dedup();
    converted.convert(texture_ids.iter().copied(), &images);
    let converted = &*converted;

    let mut cameras: Vec<_> = cameras
        .iter()
        .filter(|(_, camera, ..)| camera.is_active)
        .collect();
    cameras.sort_by_key(|(entity, camera, ..)| (camera.order, *entity));

    let primary_window = primary_window.iter().next();
    let default_layers = RenderLayers::default();
    let tables = Tables::get();
    let pool = ComputeTaskPool::get_or_init(TaskPool::default);
    let mut targets: Vec<ImageTarget> = Vec::new();
    drawn.clear();
    for (_, camera, target, camera_transform, camera_layers) in cameras {
        let images = &*images;
        let target = match target.normalize(primary_window) {
            Some(NormalizedRenderTarget::Window(window)) => Target::Window(window.entity()),
            Some(NormalizedRenderTarget::Image(image)) => Target::Image(image.handle.id()),
            Some(NormalizedRenderTarget::TextureView(_)) => {
                if !core::mem::replace(&mut *warned_about_views, true) {
                    warn!("The soft renderer has no texture views, so it skips cameras that draw to them");
                }
                continue;
            }
            _ => continue,
        };
        let size = match target {
            Target::Window(window) => framebuffers
                .get(window)
                .ok()
                .map(|(_, framebuffer)| UVec2::new(framebuffer.width(), framebuffer.height())),
            Target::Image(id) => images.get(id).map(Image::size),
        };
        let Some(size) = size else {
            continue;
        };
        let Some(viewport) = viewport(camera, size.x, size.y) else {
            continue;
        };
        let clear = match &camera.clear_color {
            ClearColorConfig::Default => Some(clear_color.0),
            ClearColorConfig::Custom(color) => Some(*color),
            ClearColorConfig::None => None,
        }
        .map(|color| color.to_linear());

        // An image's pixels leave `targets` while the camera draws to them, so that the
        // camera can sample the other images there.
        let mut image_pixels = None;
        if let Target::Image(id) = target {
            let index = match targets.iter().position(|target| target.id == id) {
                Some(index) => index,
                None => {
                    // A camera that clears all of the image needn't read what's there.
                    let load =
                        clear.is_none() || viewport != URect::from_corners(UVec2::ZERO, size);
                    let Some(target) = images
                        .get(id)
                        .and_then(|image| ImageTarget::new(id, image, load, &tables))
                    else {
                        if warned.insert(id) {
                            warn!("The soft renderer can't draw to {id}: it can't read its format, or it has layers");
                        }
                        continue;
                    };
                    targets.push(target);
                    targets.len() - 1
                }
            };
            image_pixels = Some((index, core::mem::take(&mut targets[index].pixels)));
        }

        let mut textures = HashMap::<AssetId<Image>, Option<Texture>>::default();
        for &id in &texture_ids {
            let texture = match targets.iter().find(|target| target.id == id) {
                Some(target) => images
                    .get(id)
                    .and_then(|image| Texture::from_target(target, image, &default_sampler.0)),
                None => Texture::new(id, images, converted, &default_sampler.0),
            };
            textures.insert(id, texture);
        }

        let clip_from_world =
            camera.clip_from_view() * Mat4::from(camera_transform.affine().inverse());
        let camera_layers = camera_layers.unwrap_or(&default_layers);
        let quads: Vec<Quad> = items
            .iter()
            .filter(|item| camera_layers.intersects(item.layers.unwrap_or(&default_layers)))
            .filter_map(|item| {
                let paint = match &item.paint {
                    ItemPaint::Solid => Paint::Solid,
                    ItemPaint::Texture { image, map } => Paint::Texture {
                        texture: textures.get(image)?.as_ref()?,
                        map,
                        filter: ImageFilterMode::Nearest,
                    },
                };
                let pixel_from_unit =
                    pixel_from_unit(clip_from_world * Mat4::from(item.world_from_unit), viewport)?;
                let clip = [
                    viewport.min.x,
                    viewport.min.y,
                    viewport.max.x,
                    viewport.max.y,
                ];
                let mut quad = Quad::new(pixel_from_unit, clip, paint, item.color, item.alpha)?;
                quad.pick_filter();
                Some(quad)
            })
            .collect();

        match (target, &mut image_pixels) {
            (Target::Image(_), Some((_, pixels))) => {
                let clear = clear.map(color::pixel_with_alpha);
                fill::<true>(pool, &tables, pixels, size.x, viewport, clear, &quads);
            }
            (Target::Window(window), _) => {
                if let Ok((_, mut framebuffer)) = framebuffers.get_mut(window) {
                    let clear = clear.map(color::pixel);
                    let pixels = framebuffer.pixels_mut();
                    fill::<false>(pool, &tables, pixels, size.x, viewport, clear, &quads);
                    if !drawn.contains(&window) {
                        drawn.push(window);
                    }
                }
            }
            (Target::Image(_), None) => {}
        }
        drop(quads);
        drop(textures);
        if let Some((index, pixels)) = image_pixels {
            targets[index].pixels = pixels;
        }
    }

    for target in &targets {
        if !target.write(&mut images, &tables) && warned.insert(target.id) {
            let format = images
                .get(target.id)
                .map(|image| image.texture_descriptor.format);
            warn!(
                "The soft renderer can't write {format:?} images, so {} doesn't get what cameras drew to it",
                target.id
            );
        }
    }

    // A window no camera draws to shows black, as with `hearth_render`.
    for (entity, mut framebuffer) in &mut framebuffers {
        if !drawn.contains(&entity) {
            framebuffer.pixels_mut().fill(0);
        }
    }
}

/// What a camera draws to.
#[derive(Clone, Copy)]
enum Target {
    /// A window's [`Framebuffer`].
    Window(Entity),
    /// An image.
    Image(AssetId<Image>),
}

/// Clears a camera's viewport of `pixels`, rows `width` long, and fills its quads, a band of
/// rows on each task. With `ALPHA`, the pixels keep alpha in their top byte.
fn fill<const ALPHA: bool>(
    pool: &TaskPool,
    tables: &Tables,
    mut pixels: &mut [u32],
    width: u32,
    viewport: URect,
    clear: Option<u32>,
    quads: &[Quad],
) {
    pixels.par_chunk_map_mut(pool, (BAND_ROWS * width) as usize, |band_index, band| {
        let first_row = band_index as u32 * BAND_ROWS;
        if let Some(clear) = clear {
            let rows = viewport.min.y.max(first_row)
                ..viewport.max.y.min(first_row + band.len() as u32 / width);
            for y in rows {
                let row = &mut band[((y - first_row) * width) as usize..][..width as usize];
                row[viewport.min.x as usize..viewport.max.x as usize].fill(clear);
            }
        }
        for quad in quads {
            quad.fill::<ALPHA>(tables, band, width, first_row);
        }
    });
}

/// The part of a framebuffer a camera draws to, in pixels.
fn viewport(camera: &Camera, width: u32, height: u32) -> Option<URect> {
    let rect = camera
        .physical_viewport_rect()
        .unwrap_or(URect::new(0, 0, width, height));
    let rect = URect {
        min: rect.min.min(UVec2::new(width, height)),
        max: rect.max.min(UVec2::new(width, height)),
    };
    (!rect.is_empty()).then_some(rect)
}

/// From `clip_from_unit`, which takes `(s, t, 0, 1)` to clip space, the matrix that takes
/// `(s, t, 1)` to a pixel position times its `w`, or `None` if the quad is all in front of the
/// near plane or past the far one.
fn pixel_from_unit(clip_from_unit: Mat4, viewport: URect) -> Option<Mat3> {
    let (x, y) = (viewport.min.x as f32, viewport.min.y as f32);
    let (half_width, half_height) = (
        viewport.width() as f32 * 0.5,
        viewport.height() as f32 * 0.5,
    );
    // Depth: clip space keeps 0 <= z <= w.
    let corners = [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]
        .map(|(s, t)| clip_from_unit * Vec4::new(s, t, 0.0, 1.0));
    if corners.iter().all(|c| c.z < 0.0) || corners.iter().all(|c| c.z > c.w) {
        return None;
    }
    let to_pixel = |clip: Vec4| {
        Vec3::new(
            half_width * clip.x + (x + half_width) * clip.w,
            -half_height * clip.y + (y + half_height) * clip.w,
            clip.w,
        )
    };
    Some(Mat3::from_cols(
        to_pixel(clip_from_unit.x_axis),
        to_pixel(clip_from_unit.y_axis),
        to_pixel(clip_from_unit.w_axis),
    ))
}
