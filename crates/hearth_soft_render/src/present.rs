//! Showing each window's [`Framebuffer`] on the desktop, through `softbuffer`.

use core::num::NonZeroU32;

use hearth_ecs::{
    entity::{Entity, EntityHashMap},
    system::{NonSendMut, Query},
};
use hearth_window::{RawHandleWrapper, ThreadLockedRawWindowHandleWrapper};
use softbuffer::{Context, Surface};
use tracing::error;

use crate::Framebuffer;

type Handle = ThreadLockedRawWindowHandleWrapper;

/// The connection to the display, and a surface for each window.
#[derive(Default)]
pub(crate) struct Surfaces {
    context: Option<Context<Handle>>,
    windows: EntityHashMap<Surface<Handle, Handle>>,
}

/// Copies each window's framebuffer to its surface and shows it.
pub(crate) fn present(
    mut surfaces: NonSendMut<Surfaces>,
    windows: Query<(Entity, &RawHandleWrapper, &Framebuffer)>,
) {
    let Surfaces {
        context,
        windows: open,
    } = &mut *surfaces;
    open.retain(|entity, _| windows.contains(*entity));
    for (entity, handle, framebuffer) in &windows {
        let (Some(width), Some(height)) = (
            NonZeroU32::new(framebuffer.width()),
            NonZeroU32::new(framebuffer.height()),
        ) else {
            continue;
        };
        if !open.contains_key(&entity) {
            let context = match context {
                Some(context) => context,
                None => {
                    // SAFETY: This system takes a `NonSendMut`, so it runs on the main thread,
                    // where windows can be used on every platform.
                    #[expect(unsafe_code, reason = "Getting a window's handle is unsafe.")]
                    let display = unsafe { handle.get_handle() };
                    match Context::new(display) {
                        Ok(new) => context.insert(new),
                        Err(err) => {
                            error!("Can't show framebuffers on this display: {err}");
                            return;
                        }
                    }
                }
            };
            // SAFETY: As above, on the main thread.
            #[expect(unsafe_code, reason = "Getting a window's handle is unsafe.")]
            let window = unsafe { handle.get_handle() };
            match Surface::new(context, window) {
                Ok(surface) => {
                    open.insert(entity, surface);
                }
                Err(err) => {
                    error!("Can't show a framebuffer in window {entity}: {err}");
                    continue;
                }
            }
        }
        let surface = open.get_mut(&entity).unwrap();
        let shown = surface.resize(width, height).and_then(|()| {
            let mut buffer = surface.buffer_mut()?;
            buffer.copy_from_slice(framebuffer.pixels());
            buffer.present()
        });
        if let Err(err) = shown {
            error!("Can't show the framebuffer of window {entity}: {err}");
        }
    }
}
