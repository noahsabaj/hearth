use alloc::vec::Vec;
use core::fmt;

use hearth_ecs::prelude::Component;

#[cfg(feature = "hearth_reflect")]
use {hearth_ecs::prelude::ReflectComponent, hearth_reflect::Reflect};

/// The picture a [`Window`](crate::Window) shows in its title bar and the taskbar.
///
/// Add it to a window's entity and the window shows it; change it and the window follows; remove
/// it and the window goes back to the platform's default. On macOS and the web the icon comes
/// from the app bundle or the page instead, so there it does nothing.
///
/// ```
/// # use hearth_window::WindowIcon;
/// // A 2x2 icon: red, green, blue and white.
/// let rgba = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
/// let icon = WindowIcon::from_rgba(rgba, 2, 2).unwrap();
/// assert_eq!(icon.width(), 2);
/// ```
#[derive(Component, Clone, PartialEq)]
#[cfg_attr(
    feature = "hearth_reflect",
    derive(Reflect),
    reflect(opaque),
    reflect(Component, Debug, PartialEq, Clone)
)]
pub struct WindowIcon {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl WindowIcon {
    /// An icon `width` pixels wide and `height` tall, from 8-bit RGBA pixels given row by row
    /// from the top left.
    ///
    /// Returns `None` when `rgba` doesn't hold exactly four bytes for each pixel, or the icon has
    /// no pixels.
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        (expected > 0 && rgba.len() == expected).then_some(Self {
            width,
            height,
            rgba,
        })
    }

    /// The icon's width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The icon's height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The icon's 8-bit RGBA pixels, row by row from the top left.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

impl fmt::Debug for WindowIcon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowIcon")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn only_whole_pixels_make_an_icon() {
        assert!(WindowIcon::from_rgba(vec![0; 16], 2, 2).is_some());
        assert!(WindowIcon::from_rgba(vec![0; 15], 2, 2).is_none());
        assert!(WindowIcon::from_rgba(Vec::new(), 0, 0).is_none());
        assert!(WindowIcon::from_rgba(vec![0; 4], u32::MAX, u32::MAX).is_none());
    }
}
