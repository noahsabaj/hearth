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
use hearth_math::{Affine3A, Mat3, Mat4, URect, Vec3, Vec4};
use hearth_platform::collections::HashMap;
use hearth_tasks::{ComputeTaskPool, ParallelSliceMut, TaskPool};
use hearth_transform::components::GlobalTransform;
use hearth_window::{PrimaryWindow, Window};
use tracing::warn;

use crate::{
    color::{self, Tables},
    raster::{Alpha, Paint, Quad, TexelMap},
    sprite::{self, SpriteQuery},
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

/// Draws what each camera sees into the framebuffer of its window.
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
    images: Res<Assets<Image>>,
    atlases: Res<Assets<TextureAtlasLayout>>,
    default_sampler: Res<DefaultSampler>,
    mut converted: ResMut<ConvertedImages>,
    sprites: SpriteQuery,
    #[cfg(feature = "hearth_text")] texts: crate::text::Texts,
    mut drawn: Local<Vec<Entity>>,
    mut warned_about_images: Local<bool>,
) {
    let mut items = Vec::new();
    sprite::collect(&sprites, &images, &atlases, &mut items);
    #[cfg(feature = "hearth_text")]
    texts.collect(&mut items);
    // A stable sort keeps the items of one entity, like a text's shadow and glyphs, in order.
    items.sort_by(|a, b| a.z.total_cmp(&b.z));

    converted.convert(
        items.iter().filter_map(|item| match item.paint {
            ItemPaint::Texture { image, .. } => Some(image),
            ItemPaint::Solid => None,
        }),
        &images,
    );
    let converted = &*converted;
    let mut textures = HashMap::<AssetId<Image>, Option<Texture>>::default();
    for item in &items {
        if let ItemPaint::Texture { image, .. } = item.paint {
            textures
                .entry(image)
                .or_insert_with(|| Texture::new(image, &images, converted, &default_sampler.0));
        }
    }

    let mut cameras: Vec<_> = cameras
        .iter()
        .filter(|(_, camera, ..)| camera.is_active)
        .collect();
    cameras.sort_by_key(|(entity, camera, ..)| (camera.order, *entity));

    let primary_window = primary_window.iter().next();
    let default_layers = RenderLayers::default();
    let tables = Tables::get();
    let pool = ComputeTaskPool::get_or_init(TaskPool::default);
    drawn.clear();
    for (_, camera, target, camera_transform, camera_layers) in cameras {
        let target = match target.normalize(primary_window) {
            Some(NormalizedRenderTarget::Window(window)) => window.entity(),
            Some(NormalizedRenderTarget::Image(_) | NormalizedRenderTarget::TextureView(_)) => {
                if !core::mem::replace(&mut *warned_about_images, true) {
                    warn!("The soft renderer draws only to windows, so it skips cameras that draw to images");
                }
                continue;
            }
            _ => continue,
        };
        let Ok((_, mut framebuffer)) = framebuffers.get_mut(target) else {
            continue;
        };
        let (width, height) = (framebuffer.width(), framebuffer.height());
        let Some(viewport) = viewport(camera, width, height) else {
            continue;
        };
        let clear = match &camera.clear_color {
            ClearColorConfig::Default => Some(clear_color.0),
            ClearColorConfig::Custom(color) => Some(*color),
            ClearColorConfig::None => None,
        }
        .map(|color| color::pixel(color.to_linear()));
        if !drawn.contains(&target) {
            drawn.push(target);
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

        framebuffer.pixels_mut().par_chunk_map_mut(
            pool,
            (BAND_ROWS * width) as usize,
            |band_index, band| {
                let first_row = band_index as u32 * BAND_ROWS;
                if let Some(clear) = clear {
                    let rows = viewport.min.y.max(first_row)
                        ..viewport.max.y.min(first_row + band.len() as u32 / width);
                    for y in rows {
                        let row = &mut band[((y - first_row) * width) as usize..][..width as usize];
                        row[viewport.min.x as usize..viewport.max.x as usize].fill(clear);
                    }
                }
                for quad in &quads {
                    quad.fill(&tables, band, width, first_row);
                }
            },
        );
    }

    // A window no camera draws to shows black, as with `hearth_render`.
    for (entity, mut framebuffer) in &mut framebuffers {
        if !drawn.contains(&entity) {
            framebuffer.pixels_mut().fill(0);
        }
    }
}

/// The part of a framebuffer a camera draws to, in pixels.
fn viewport(camera: &Camera, width: u32, height: u32) -> Option<URect> {
    let rect = camera
        .physical_viewport_rect()
        .unwrap_or(URect::new(0, 0, width, height));
    let rect = URect {
        min: rect.min.min(hearth_math::UVec2::new(width, height)),
        max: rect.max.min(hearth_math::UVec2::new(width, height)),
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
