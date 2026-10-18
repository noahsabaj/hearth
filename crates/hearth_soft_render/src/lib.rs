#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]

//! Draws Hearth's 2D scenes on the CPU.
//!
//! [`SoftRenderPlugin`] draws what each [`Camera`](hearth_camera::Camera) sees into the
//! [`Framebuffer`] of its window: rows of blue, green, red and an unused byte, the layout desktop
//! windowing systems and Axiom take. It needs no GPU, and gives the same pixels wherever it runs.
//!
//! It draws [`Sprite`](hearth_sprite::Sprite)s, with their atlases, rects, flips, tints, anchors,
//! scaling, tiling and nine-slicing, and with the `hearth_text` feature
//! [`Text2d`](hearth_sprite::Text2d). Colors mix in linear light, as on the GPU, and images are
//! filtered by their samplers. With the `desktop` feature, it shows the framebuffers in their
//! windows through `softbuffer`.
//!
//! Use it instead of `hearth_render`, not beside it: both would draw to the same windows.

extern crate alloc;

mod camera;
mod color;
mod draw;
mod framebuffer;
#[cfg(feature = "desktop")]
mod present;
mod raster;
mod sprite;
#[cfg(feature = "hearth_text")]
mod text;
mod texture;

pub use framebuffer::Framebuffer;

use hearth_app::{App, Last, Plugin, PostStartup, PostUpdate};
use hearth_asset::{AssetApp, AssetEventSystems};
use hearth_camera::CameraUpdateSystems;
use hearth_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use hearth_image::{CompressedImageFormats, ImageLoader, ImagePlugin};

/// The soft renderer's systems, in the `Last` schedule.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum SoftRenderSystems {
    /// Drawing into the [`Framebuffer`]s.
    Draw,
    /// Showing them, with the `desktop` feature.
    Present,
}

/// Draws 2D scenes on the CPU into each window's [`Framebuffer`]; see the [crate docs](crate).
#[derive(Default)]
pub struct SoftRenderPlugin;

impl Plugin for SoftRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<texture::DefaultSampler>()
            .init_resource::<texture::ConvertedImages>()
            .configure_sets(
                Last,
                SoftRenderSystems::Draw.before(SoftRenderSystems::Present),
            )
            .add_systems(
                PostStartup,
                camera::update_cameras.in_set(CameraUpdateSystems),
            )
            .add_systems(
                PostUpdate,
                camera::update_cameras
                    .in_set(CameraUpdateSystems)
                    .before(AssetEventSystems)
                    .before(hearth_camera::visibility::update_frusta),
            )
            .add_systems(
                Last,
                (
                    texture::forget_changed_images,
                    draw::fit_framebuffers,
                    draw::draw,
                )
                    .chain()
                    .in_set(SoftRenderSystems::Draw),
            );

        #[cfg(feature = "desktop")]
        app.init_non_send::<present::Surfaces>()
            .add_systems(Last, present::present.in_set(SoftRenderSystems::Present));
    }

    fn finish(&self, app: &mut App) {
        // `hearth_render` registers the image loader once it knows which compressed formats the
        // GPU takes. Here there's no GPU: compressed images are decompressed where the loader can.
        if !ImageLoader::SUPPORTED_FORMATS.is_empty() {
            app.register_asset_loader(ImageLoader::new(CompressedImageFormats::NONE));
        }
        let sampler = app
            .get_added_plugins::<ImagePlugin>()
            .first()
            .map(|plugin| plugin.default_sampler.clone());
        if let Some(sampler) = sampler {
            app.insert_resource(texture::DefaultSampler(sampler));
        }
    }
}
