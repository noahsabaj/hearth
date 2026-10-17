//! `bevy screenshot`: capturing a window to a PNG file.
//!
//! The capture spawns a `Screenshot` entity and observes the `ScreenshotCaptured` event on that
//! entity alone. The app despawns the entity once it delivered the image.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};

use crate::brp::brp_error;
use crate::cli::ScreenshotArgs;
use crate::context::{Attachment, Ctx};
use crate::known;
use crate::output::{CliError, Exit, Outcome};

#[derive(Deserialize)]
struct Frame {
    result: Option<Vec<Captured>>,
    error: Option<Value>,
}

#[derive(Deserialize)]
struct Captured {
    image: Image,
}

#[derive(Deserialize)]
struct Image {
    data: Option<Vec<u8>>,
    texture_descriptor: Descriptor,
}

#[derive(Deserialize)]
struct Descriptor {
    format: Value,
    size: Size,
}

#[derive(Deserialize)]
struct Size {
    width: u32,
    height: u32,
}

/// An RGB image, 8 bits per channel, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pixels {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 3` bytes.
    pub rgb: Vec<u8>,
}

/// `bevy screenshot`.
pub fn screenshot(ctx: &mut Ctx, args: ScreenshotArgs) -> Result<Outcome, CliError> {
    let render_target = match &args.window {
        Some(window) => json!({ "Window": { "Entity": ctx.entity(window)? } }),
        None => json!({ "Window": "Primary" }),
    };
    let spawned = ctx.call(
        "world.spawn_entity",
        json!({ "components": { known::SCREENSHOT: render_target } }),
    )?;
    let entity = spawned
        .get("entity")
        .cloned()
        .ok_or_else(|| CliError::internal("Spawning the screenshot returned no entity"))?;

    let wait = ctx.timeout.max(Duration::from_secs(10));
    let started = Instant::now();
    let mut watch = ctx.client()?.watch(
        "world.observe+watch",
        Some(json!({ "event": known::SCREENSHOT_CAPTURED, "entity": entity })),
    )?;
    let image = loop {
        let left = wait.saturating_sub(started.elapsed());
        let text = if left.is_zero() {
            None
        } else {
            watch.next_raw(Some(left))?
        };
        let Some(text) = text else {
            if !left.is_zero() && !watch.ended() {
                continue;
            }
            drop(watch);
            let _ = ctx.call("world.despawn_entity", json!({ "entity": entity }));
            return Err(CliError::new(
                "CAPTURE_FAILED",
                Exit::Timeout,
                format!("No screenshot arrived within {:.0}s", wait.as_secs_f64()),
            )
            .hint("The window must be rendering: a minimized window, or an app without a renderer or window, can't be captured."));
        };
        let frame: Frame = serde_json::from_str(&text).map_err(|error| {
            CliError::new(
                "CAPTURE_FAILED",
                Exit::Error,
                format!("The screenshot event couldn't be read: {error}"),
            )
        })?;
        if let Some(error) = frame.error {
            return Err(brp_error("world.observe+watch", &error));
        }
        if let Some(captured) = frame.result.and_then(|mut events| events.pop()) {
            break captured.image;
        }
    };
    drop(watch);

    let format = image
        .texture_descriptor
        .format
        .as_str()
        .unwrap_or_default()
        .to_string();
    let pixels = decode(image)?;
    if pixels.width <= 1 && pixels.height <= 1 {
        ctx.out.warn(
            "The screenshot is empty: the window is minimized or has no size. Restore the window and capture again.",
        );
    }
    let pixels = match args.max_size {
        Some(max) => downscale(&pixels, max),
        None => pixels,
    };
    let png = encode_png(&pixels)?;
    let path = match args.out {
        Some(path) => path,
        None => default_path(ctx)?,
    };
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|error| CliError::io(dir.display(), error))?;
    }
    std::fs::write(&path, &png).map_err(|error| CliError::io(path.display(), error))?;
    let path = std::fs::canonicalize(&path).unwrap_or(path);

    if let Some(limits) = ctx.mcp {
        let preview = downscale(&pixels, limits.image_size);
        let data = if preview == pixels {
            png.clone()
        } else {
            encode_png(&preview)?
        };
        ctx.attachments.push(Attachment {
            data,
            mime: "image/png",
        });
    }
    let shown = display_path(&path);
    Ok(Outcome::new(
        json!({
            "path": shown,
            "width": pixels.width,
            "height": pixels.height,
            "format": format,
            "bytes": png.len(),
        }),
        format!(
            "Saved a {}x{} screenshot to {shown}",
            pixels.width, pixels.height
        ),
    ))
}

/// Where a screenshot goes without `--out`: a new file in the temporary directory.
fn default_path(ctx: &mut Ctx) -> Result<PathBuf, CliError> {
    let target = ctx.target()?;
    let name = match &target.session {
        Some(session) => session.name.clone(),
        None => format!("port-{}", target.endpoint.port),
    };
    Ok(std::env::temp_dir()
        .join("bevy-cli")
        .join("screenshots")
        .join(format!("{name}-{}.png", crate::session::now_millis())))
}

/// A path as people expect to read it, without Windows' `\\?\` prefix.
pub fn display_path(path: &std::path::Path) -> String {
    let text = path.display().to_string();
    let text = text
        .strip_prefix(r"\\?\")
        .map(str::to_string)
        .unwrap_or(text);
    // A path given as `C:/dir` and then joined would show both separators.
    if cfg!(windows) {
        text.replace('/', "\\")
    } else {
        text
    }
}

/// Converts captured BGRA or RGBA bytes to RGB, dropping alpha like Bevy's own screenshot saving.
fn decode(image: Image) -> Result<Pixels, CliError> {
    let format = image
        .texture_descriptor
        .format
        .as_str()
        .unwrap_or_default()
        .to_lowercase();
    let Size { width, height } = image.texture_descriptor.size;
    let data = image.data.ok_or_else(|| {
        CliError::new(
            "CAPTURE_FAILED",
            Exit::Error,
            "The screenshot has no pixel data",
        )
    })?;
    let bgra = format.starts_with("bgra8");
    if !bgra && !format.starts_with("rgba8") {
        return Err(CliError::new(
            "CAPTURE_FAILED",
            Exit::Error,
            format!("Screenshots in the `{format}` format aren't supported"),
        ));
    }
    let (w, h) = (width as usize, height as usize);
    // Rows may be padded, as GPU readback often aligns them.
    let stride = data.len().checked_div(h).unwrap_or(0);
    if w == 0 || h == 0 || stride < w * 4 {
        return Err(CliError::new(
            "CAPTURE_FAILED",
            Exit::Error,
            format!(
                "The screenshot's {} bytes don't make a {width}x{height} image",
                data.len()
            ),
        ));
    }
    let mut rgb = Vec::with_capacity(w * h * 3);
    for row in data.chunks_exact(stride).take(h) {
        for pixel in row[..w * 4].chunks_exact(4) {
            if bgra {
                rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
            } else {
                rgb.extend_from_slice(&pixel[..3]);
            }
        }
    }
    Ok(Pixels { width, height, rgb })
}

/// Scales an image down, averaging pixels, so neither side exceeds `max`.
pub fn downscale(pixels: &Pixels, max: u32) -> Pixels {
    let longest = pixels.width.max(pixels.height);
    if max == 0 || longest <= max {
        return pixels.clone();
    }
    let scale = f64::from(longest) / f64::from(max);
    let width = ((f64::from(pixels.width) / scale).round() as u32).clamp(1, max);
    let height = ((f64::from(pixels.height) / scale).round() as u32).clamp(1, max);
    let (sw, sh) = (pixels.width as usize, pixels.height as usize);
    let (tw, th) = (width as usize, height as usize);
    let mut rgb = Vec::with_capacity(tw * th * 3);
    for ty in 0..th {
        let y0 = ty * sh / th;
        let y1 = ((ty + 1) * sh / th).max(y0 + 1);
        for tx in 0..tw {
            let x0 = tx * sw / tw;
            let x1 = ((tx + 1) * sw / tw).max(x0 + 1);
            let mut sum = [0u64; 3];
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * sw + x) * 3;
                    for (channel, total) in sum.iter_mut().enumerate() {
                        *total += u64::from(pixels.rgb[i + channel]);
                    }
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as u64;
            rgb.extend(sum.iter().map(|total| (total / count) as u8));
        }
    }
    Pixels { width, height, rgb }
}

/// Encodes an image as PNG.
pub fn encode_png(pixels: &Pixels) -> Result<Vec<u8>, CliError> {
    let failed = |error: png::EncodingError| {
        CliError::new(
            "CAPTURE_FAILED",
            Exit::Error,
            format!("Encoding the PNG failed: {error}"),
        )
    };
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, pixels.width, pixels.height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(failed)?;
    writer.write_image_data(&pixels.rgb).map_err(failed)?;
    writer.finish().map_err(failed)?;
    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(format: &str, width: u32, height: u32, data: Vec<u8>) -> Image {
        Image {
            data: Some(data),
            texture_descriptor: Descriptor {
                format: json!(format),
                size: Size { width, height },
            },
        }
    }

    #[test]
    fn decodes_bgra_and_padded_rows() {
        let pixels = decode(image("bgra8unorm-srgb", 1, 1, vec![1, 2, 3, 255])).unwrap();
        assert_eq!(pixels.rgb, vec![3, 2, 1]);
        // Two rows of one pixel, each padded to 8 bytes.
        let padded = vec![10, 20, 30, 255, 0, 0, 0, 0, 40, 50, 60, 255, 0, 0, 0, 0];
        let pixels = decode(image("rgba8unorm", 1, 2, padded)).unwrap();
        assert_eq!(pixels.rgb, vec![10, 20, 30, 40, 50, 60]);
        assert!(decode(image("rgba16float", 1, 1, vec![0; 8])).is_err());
        assert!(decode(image("rgba8unorm", 2, 2, vec![0; 4])).is_err());
    }

    #[test]
    fn downscales_by_averaging() {
        let pixels = Pixels {
            width: 2,
            height: 2,
            rgb: vec![0, 0, 0, 100, 100, 100, 200, 200, 200, 100, 100, 100],
        };
        let small = downscale(&pixels, 1);
        assert_eq!((small.width, small.height), (1, 1));
        assert_eq!(small.rgb, vec![100, 100, 100]);
        assert_eq!(downscale(&pixels, 4), pixels);
    }

    #[test]
    fn encodes_png() {
        let pixels = Pixels {
            width: 2,
            height: 1,
            rgb: vec![255, 0, 0, 0, 255, 0],
        };
        let png = encode_png(&pixels).unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    }
}
