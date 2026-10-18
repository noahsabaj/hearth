//! `window.screenshot+watch`: capturing a window and sending its pixels.
//!
//! Observing `ScreenshotCaptured` with `world.observe+watch` also delivers the pixels, but as the
//! reflected `Image`, with one JSON number per byte, which takes seconds for a whole window. This
//! method sends them as a single base64 string instead.

use alloc::sync::Arc;
use std::sync::Mutex;

use anyhow::anyhow;
use base64::{prelude::BASE64_STANDARD, Engine as _};
use hearth_ecs::{
    entity::Entity, observer::On, query::With, resource::Resource, system::In, world::World,
};
use hearth_platform::collections::HashMap;
use hearth_render::view::screenshot::{Screenshot, ScreenshotCaptured};
use hearth_window::{PrimaryWindow, Window};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    builtin_methods::parse, error_codes, CurrentWatchingRequest, HrpError, HrpResult,
    WatchingRequestId,
};

/// The HRP method that captures a window: `window.screenshot+watch`.
///
/// It's a watching method because a capture takes a few frames. The stream sends one value, then
/// nothing more.
pub const HRP_SCREENSHOT_METHOD: &str = "window.screenshot+watch";

/// `window.screenshot+watch`: the window to capture.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HrpScreenshotParams {
    /// The window entity. Without it, the primary window.
    #[serde(default)]
    pub window: Option<Entity>,
}

/// The captures that `window.screenshot+watch` requests are waiting for.
///
/// A request's entry goes when its stream closes.
#[derive(Resource, Default)]
pub struct HrpScreenshots {
    pub(crate) captures: HashMap<WatchingRequestId, Capture>,
}

pub(crate) enum Capture {
    /// Spawned, and the observer fills this in once the image is read back.
    Pending(Arc<Mutex<Option<Value>>>),
    /// The result went to the client.
    Sent,
}

/// Handles a `window.screenshot+watch` request coming from a client.
///
/// The result has the image's `width` and `height` in pixels, its texture `format` (8-bit RGBA or
/// BGRA), and `data`: the rows of pixels, top to bottom, in base64.
///
/// This must be run by the server as a watching request, which sets [`CurrentWatchingRequest`].
pub fn process_remote_screenshot_watching_request(
    In(params): In<Option<Value>>,
    world: &mut World,
) -> HrpResult<Option<Value>> {
    let HrpScreenshotParams { window } = match params {
        Some(params) => parse(params)?,
        None => HrpScreenshotParams::default(),
    };
    let Some(request) = world
        .get_resource::<CurrentWatchingRequest>()
        .and_then(|current| current.0)
    else {
        return Err(HrpError::internal(anyhow!(
            "`{HRP_SCREENSHOT_METHOD}` must be run as a watching request"
        )));
    };

    let capture = world
        .get_resource_or_init::<HrpScreenshots>()
        .captures
        .get(&request)
        .map(|capture| match capture {
            Capture::Pending(slot) => Some(slot.clone()),
            Capture::Sent => None,
        });
    let slot = match capture {
        Some(Some(slot)) => slot,
        Some(None) => return Ok(None),
        None => {
            let screenshot = match window {
                Some(window) => {
                    if world.get_entity(window).is_err() {
                        return Err(HrpError::entity_not_found(window));
                    }
                    if world.get::<Window>(window).is_none() {
                        return Err(HrpError::component_not_present(
                            "hearth_window::window::Window",
                            window,
                        ));
                    }
                    Screenshot::window(window)
                }
                None => {
                    let mut primary = world.query_filtered::<(), With<PrimaryWindow>>();
                    if primary.iter(world).next().is_none() {
                        return Err(HrpError {
                            code: error_codes::RESOURCE_NOT_PRESENT,
                            message: "The app has no primary window".to_string(),
                            data: None,
                        });
                    }
                    Screenshot::primary_window()
                }
            };
            let slot = Arc::new(Mutex::new(None));
            let sink = slot.clone();
            world
                .spawn(screenshot)
                .observe(move |captured: On<ScreenshotCaptured>| {
                    *sink.lock().unwrap() = Some(encode(&captured));
                });
            world
                .resource_mut::<HrpScreenshots>()
                .captures
                .insert(request, Capture::Pending(slot));
            return Ok(None);
        }
    };

    let Some(result) = slot.lock().unwrap().take() else {
        return Ok(None);
    };
    world
        .resource_mut::<HrpScreenshots>()
        .captures
        .insert(request, Capture::Sent);
    Ok(Some(result))
}

/// The captured image as the method's result.
fn encode(captured: &ScreenshotCaptured) -> Value {
    let descriptor = &captured.image.texture_descriptor;
    let data = captured.image.data.as_deref().unwrap_or_default();
    json!({
        "width": descriptor.size.width,
        "height": descriptor.size.height,
        "format": descriptor.format,
        "data": BASE64_STANDARD.encode(data),
    })
}

#[cfg(test)]
mod tests {
    use hearth_ecs::system::RunSystemOnce;

    use super::*;

    fn poll(world: &mut World, params: Option<Value>) -> HrpResult<Option<Value>> {
        world
            .run_system_once_with(process_remote_screenshot_watching_request, params)
            .unwrap()
    }

    #[test]
    fn needs_a_window() {
        let mut world = World::new();
        world.insert_resource(CurrentWatchingRequest(Some(WatchingRequestId(0))));
        let error = poll(&mut world, None).unwrap_err();
        assert_eq!(error.code, error_codes::RESOURCE_NOT_PRESENT);
        let entity = world.spawn_empty().id();
        let error = poll(&mut world, Some(json!({ "window": entity }))).unwrap_err();
        assert_eq!(error.code, error_codes::COMPONENT_NOT_PRESENT);
    }

    #[test]
    fn sends_the_capture_once() {
        let mut world = World::new();
        world.spawn((Window::default(), PrimaryWindow));
        world.insert_resource(CurrentWatchingRequest(Some(WatchingRequestId(0))));
        assert_eq!(poll(&mut world, None).unwrap(), None);
        let mut screenshots = world.query_filtered::<Entity, With<Screenshot>>();
        let screenshot = screenshots.single(&world).unwrap();
        assert_eq!(poll(&mut world, None).unwrap(), None);

        world.trigger(ScreenshotCaptured {
            entity: screenshot,
            image: Default::default(),
        });
        let sent = poll(&mut world, None).unwrap().unwrap();
        assert_eq!(
            (sent["width"].as_u64(), sent["height"].as_u64()),
            (Some(1), Some(1))
        );
        assert_eq!(sent["format"], "rgba8unorm-srgb");
        assert_eq!(sent["data"], "/////w==");
        assert_eq!(poll(&mut world, None).unwrap(), None);
        // Only the one capture was taken.
        assert_eq!(screenshots.iter(&world).count(), 1);
    }
}
