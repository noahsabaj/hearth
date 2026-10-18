//! The editor's window icon: a flame on a dark tile, drawn in code so the editor ships no image.

use bevy_app::{App, Startup};
use bevy_ecs::prelude::*;
use bevy_window::{PrimaryWindow, WindowIcon};

/// The icon's width and height in pixels. Platforms scale it to the sizes they show.
const SIZE: u32 = 64;
/// Samples per pixel along each axis, to smooth the edges.
const SAMPLES: u32 = 4;

/// A layer of the flame: a round base and a curved taper up to a tip. Heights run down from the
/// top of the icon, 0 to 1.
struct Layer {
    /// The height of the base's centre.
    base: f32,
    /// The base's radius.
    radius: f32,
    /// The height of the tip.
    tip: f32,
    /// How far right the tip leans.
    lean: f32,
    color: [f32; 3],
}

/// The flame's layers, drawn in order: red outside, yellow at the heart.
const LAYERS: [Layer; 3] = [
    Layer {
        base: 0.63,
        radius: 0.27,
        tip: 0.08,
        lean: 0.07,
        color: [255.0, 86.0, 34.0],
    },
    Layer {
        base: 0.69,
        radius: 0.19,
        tip: 0.30,
        lean: 0.05,
        color: [255.0, 158.0, 46.0],
    },
    Layer {
        base: 0.75,
        radius: 0.11,
        tip: 0.49,
        lean: 0.03,
        color: [255.0, 228.0, 138.0],
    },
];
const TILE_TOP: [f32; 3] = [58.0, 42.0, 36.0];
const TILE_BOTTOM: [f32; 3] = [28.0, 21.0, 19.0];
const GLOW: [f32; 3] = [120.0, 52.0, 28.0];

/// Draws the icon.
pub fn editor_icon() -> WindowIcon {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    let step = 1.0 / (SIZE * SAMPLES) as f32;
    for row in 0..SIZE {
        for column in 0..SIZE {
            let mut sum = [0.0; 3];
            let mut covered = 0;
            for sample_row in 0..SAMPLES {
                for sample_column in 0..SAMPLES {
                    let x = ((column * SAMPLES + sample_column) as f32 + 0.5) * step;
                    let y = ((row * SAMPLES + sample_row) as f32 + 0.5) * step;
                    if let Some(color) = color_at(x, y) {
                        covered += 1;
                        for (total, channel) in sum.iter_mut().zip(color) {
                            *total += channel;
                        }
                    }
                }
            }
            let average = |total: f32| (total / covered.max(1) as f32).round() as u8;
            rgba.extend(sum.map(average));
            rgba.push((covered as f32 / (SAMPLES * SAMPLES) as f32 * 255.0).round() as u8);
        }
    }
    WindowIcon::from_rgba(rgba, SIZE, SIZE).expect("the icon has four bytes per pixel")
}

/// The icon's color at `x`, `y` (0 to 1 from the top left), or `None` outside its tile.
fn color_at(x: f32, y: f32) -> Option<[f32; 3]> {
    if !in_tile(x, y) {
        return None;
    }
    let mut color = mix(TILE_TOP, TILE_BOTTOM, y);
    // A warm glow behind the flame.
    let distance = (x - 0.5).hypot((y - 0.66) * 1.2);
    color = mix(color, GLOW, (1.0 - distance / 0.45).max(0.0).powi(2) * 0.8);
    for layer in &LAYERS {
        if in_layer(layer, x, y) {
            color = layer.color;
        }
    }
    Some(color)
}

/// Whether `x`, `y` is on the rounded square behind the flame.
fn in_tile(x: f32, y: f32) -> bool {
    const INSET: f32 = 0.03;
    const RADIUS: f32 = 0.2;
    let (low, high) = (INSET + RADIUS, 1.0 - INSET - RADIUS);
    let dx = (low - x).max(x - high).max(0.0);
    let dy = (low - y).max(y - high).max(0.0);
    dx * dx + dy * dy <= RADIUS * RADIUS
}

fn in_layer(layer: &Layer, x: f32, y: f32) -> bool {
    if y >= layer.base {
        return (x - 0.5).powi(2) + (y - layer.base).powi(2) <= layer.radius * layer.radius;
    }
    if y < layer.tip {
        return false;
    }
    // 0 at the tip, 1 level with the base's centre.
    let along = (y - layer.tip) / (layer.base - layer.tip);
    let center = 0.5 + layer.lean * (1.0 - along).powi(2);
    (x - center).abs() <= layer.radius * along.powf(1.6)
}

fn mix(from: [f32; 3], to: [f32; 3], amount: f32) -> [f32; 3] {
    [0, 1, 2].map(|channel| from[channel] + (to[channel] - from[channel]) * amount)
}

fn give_window_its_icon(windows: Query<Entity, With<PrimaryWindow>>, mut commands: Commands) {
    for window in &windows {
        commands.entity(window).insert(editor_icon());
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, give_window_its_icon);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flame_sits_on_a_tile_with_clear_corners() {
        let icon = editor_icon();
        let alpha = |x: u32, y: u32| icon.rgba()[((y * SIZE + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(SIZE - 1, SIZE - 1), 0);
        assert_eq!(alpha(SIZE / 2, SIZE / 2), 255);
        // The heart of the flame is yellow.
        let heart = ((SIZE * 3 / 4 * SIZE + SIZE / 2) * 4) as usize;
        assert_eq!(&icon.rgba()[heart..heart + 3], &[255, 228, 138]);
    }
}
