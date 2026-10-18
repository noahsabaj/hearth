//! Keeping each [`Camera`]'s target size and projection up to date, which `hearth_render` does
//! when it's the renderer.

use hearth_asset::{AssetEvent, AssetId, Assets};
use hearth_camera::{Camera, NormalizedRenderTarget, Projection, RenderTarget, RenderTargetInfo};
use hearth_ecs::{
    change_detection::DetectChanges,
    entity::{ContainsEntity, Entity, EntityHashSet},
    message::MessageReader,
    query::With,
    system::{Query, Res},
};
use hearth_image::Image;
use hearth_math::UVec2;
use hearth_platform::collections::HashSet;
use hearth_window::{
    PrimaryWindow, Window, WindowCreated, WindowResized, WindowScaleFactorChanged,
};

/// Updates each camera's [`RenderTargetInfo`] and projection when its target or projection
/// changes, as `hearth_render`'s `camera_system` does.
///
/// Cameras can draw to windows, images and nothing (`RenderTarget::None`); a camera drawing to a
/// texture view is left alone, since only the GPU has those.
pub(crate) fn update_cameras(
    mut window_resized: MessageReader<WindowResized>,
    mut window_created: MessageReader<WindowCreated>,
    mut window_scale_factor_changed: MessageReader<WindowScaleFactorChanged>,
    mut image_events: MessageReader<AssetEvent<Image>>,
    primary_window: Query<Entity, With<PrimaryWindow>>,
    windows: Query<&Window>,
    images: Res<Assets<Image>>,
    mut cameras: Query<(&mut Camera, &RenderTarget, &mut Projection)>,
) {
    let primary_window = primary_window.iter().next();

    let mut changed_windows = EntityHashSet::default();
    changed_windows.extend(window_created.read().map(|event| event.window));
    changed_windows.extend(window_resized.read().map(|event| event.window));
    let rescaled_windows: EntityHashSet = window_scale_factor_changed
        .read()
        .map(|event| event.window)
        .collect();
    changed_windows.extend(rescaled_windows.iter().copied());

    let changed_images: HashSet<AssetId<Image>> = image_events
        .read()
        .filter_map(|event| match event {
            AssetEvent::Modified { id } | AssetEvent::Added { id } => Some(*id),
            _ => None,
        })
        .collect();

    for (mut camera, target, mut projection) in &mut cameras {
        let mut viewport_size = camera
            .viewport
            .as_ref()
            .map(|viewport| viewport.physical_size);
        let Some(target) = target.normalize(primary_window) else {
            continue;
        };
        let target_changed = match &target {
            NormalizedRenderTarget::Window(window) => changed_windows.contains(&window.entity()),
            NormalizedRenderTarget::Image(image) => changed_images.contains(&image.handle.id()),
            NormalizedRenderTarget::TextureView(_) | NormalizedRenderTarget::None { .. } => false,
        };
        if target_changed
            || camera.is_added()
            || projection.is_changed()
            || camera.computed.old_viewport_size != viewport_size
            || camera.computed.old_sub_camera_view != camera.sub_camera_view
        {
            let Some(info) = target_info(&target, &windows, &images) else {
                continue;
            };
            // When a window moves to a screen with another scale factor, keep the viewport the
            // same part of it.
            if let NormalizedRenderTarget::Window(window) = &target
                && rescaled_windows.contains(&window.entity())
                && let Some(old) = camera
                    .computed
                    .target_info
                    .as_ref()
                    .map(|info| info.scale_factor)
                && let Some(viewport) = &mut camera.viewport
            {
                let resize = |size: UVec2| (size.as_vec2() * info.scale_factor / old).as_uvec2();
                viewport.physical_position = resize(viewport.physical_position);
                viewport.physical_size = resize(viewport.physical_size);
                viewport_size = Some(viewport.physical_size);
            }
            if let Some(viewport) = &mut camera.viewport {
                viewport.clamp_to_size(info.physical_size);
            }
            camera.computed.target_info = Some(info);
            if let Some(size) = camera.logical_viewport_size()
                && size.x != 0.0
                && size.y != 0.0
            {
                projection.update(size.x, size.y);
                camera.computed.clip_from_view = match &camera.sub_camera_view {
                    Some(sub_view) => projection.get_clip_from_view_for_sub(sub_view),
                    None => projection.get_clip_from_view(),
                };
            }
        }
        if camera.computed.old_viewport_size != viewport_size {
            camera.computed.old_viewport_size = viewport_size;
        }
        if camera.computed.old_sub_camera_view != camera.sub_camera_view {
            camera.computed.old_sub_camera_view = camera.sub_camera_view;
        }
    }
}

/// The size and scale factor of what a camera draws to.
pub(crate) fn target_info(
    target: &NormalizedRenderTarget,
    windows: &Query<&Window>,
    images: &Assets<Image>,
) -> Option<RenderTargetInfo> {
    match target {
        NormalizedRenderTarget::Window(window) => {
            let window = windows.get(window.entity()).ok()?;
            Some(RenderTargetInfo {
                physical_size: window.physical_size(),
                scale_factor: window.resolution.scale_factor(),
            })
        }
        NormalizedRenderTarget::Image(image) => Some(RenderTargetInfo {
            physical_size: images.get(&image.handle)?.size(),
            scale_factor: image.scale_factor,
        }),
        NormalizedRenderTarget::None { width, height } => Some(RenderTargetInfo {
            physical_size: UVec2::new(*width, *height),
            scale_factor: 1.0,
        }),
        NormalizedRenderTarget::TextureView(_) => None,
    }
}
