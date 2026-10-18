use hearth_camera::ManualTextureViewHandle;
use hearth_ecs::resource::Resource;
use hearth_extract_macros::ExtractResource;
use hearth_math::UVec2;
use hearth_platform::collections::HashMap;
use wgpu::TextureFormat;

use crate::{render_resource::TextureView, RenderApp};

/// A manually managed [`TextureView`] for use as a [`hearth_camera::RenderTarget`].
#[derive(Debug, Clone)]
pub struct ManualTextureView {
    pub texture_view: TextureView,
    pub size: UVec2,
    pub view_format: TextureFormat,
}

impl ManualTextureView {
    pub fn with_default_format(texture_view: TextureView, size: UVec2) -> Self {
        Self {
            texture_view,
            size,
            view_format: TextureFormat::Rgba8UnormSrgb,
        }
    }
}

/// Resource that stores manually managed [`ManualTextureView`]s for use as a [`RenderTarget`](hearth_camera::RenderTarget).
/// This type dereferences to a `HashMap<ManualTextureViewHandle, ManualTextureView>`.
/// To add a new texture view, pick a new [`ManualTextureViewHandle`] and insert it into the map.
/// Then, to render to the view, set a [`Camera`](hearth_camera::Camera)s `target` to `RenderTarget::TextureView(handle)`.
/// ```ignore
/// # use hearth_ecs::prelude::*;
/// # let mut world = World::default();
/// # world.insert_resource(ManualTextureViews::default());
/// # let texture_view = todo!();
/// let manual_views = world.resource_mut::<ManualTextureViews>();
/// let manual_view = ManualTextureView::with_default_format(texture_view, UVec2::new(1024, 1024));
///
/// // Choose an unused handle value; it's likely only you are inserting manual views.
/// const MANUAL_VIEW_HANDLE: ManualTextureViewHandle = ManualTextureViewHandle::new(42);
/// manual_views.insert(MANUAL_VIEW_HANDLE, manual_view);
///
/// // Now you can spawn a Camera that renders to the manual view:
/// # use hearth_camera::{Camera, RenderTarget};
/// world.spawn(Camera {
///     target: RenderTarget::TextureView(MANUAL_VIEW_HANDLE),
///     ..Default::default()
/// });
/// ```
/// Hearth will then use the `ManualTextureViews` resource to find your texture view and render to it.
#[derive(Default, Clone, Resource, ExtractResource)]
#[extract_app(RenderApp)]
pub struct ManualTextureViews(HashMap<ManualTextureViewHandle, ManualTextureView>);

impl core::ops::Deref for ManualTextureViews {
    type Target = HashMap<ManualTextureViewHandle, ManualTextureView>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for ManualTextureViews {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
