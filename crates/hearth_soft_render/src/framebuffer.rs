use hearth_ecs::component::Component;

/// The pixels a window shows, drawn by the soft renderer.
///
/// The renderer gives each window one, its size in physical pixels, and fills it in the `Last`
/// schedule. A platform shows it: with the `desktop` feature, this crate does through
/// `softbuffer`.
#[derive(Component, Clone, Debug, Default)]
pub struct Framebuffer {
    width: u32,
    height: u32,
    pixels: Vec<u32>,
}

impl Framebuffer {
    /// A black framebuffer of this size.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize],
        }
    }

    /// Its width, in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Its height, in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Its rows of pixels, top to bottom, each pixel `0x00RRGGBB` in sRGB: in memory, the bytes
    /// blue, green, red and one unused.
    pub fn pixels(&self) -> &[u32] {
        &self.pixels
    }

    /// Its pixels, to change.
    pub fn pixels_mut(&mut self) -> &mut [u32] {
        &mut self.pixels
    }

    /// Its pixels as bytes: blue, green, red and one unused for each.
    pub fn bytes(&self) -> &[u8] {
        bytemuck::cast_slice(&self.pixels)
    }

    /// Changes its size, leaving it black.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.pixels.clear();
        self.pixels.resize(width as usize * height as usize, 0);
    }
}
