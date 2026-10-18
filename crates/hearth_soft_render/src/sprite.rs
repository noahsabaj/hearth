//! [`Sprite`]s as quads, sized and mapped to their images as the GPU sprite material does.

use hearth_asset::Assets;
use hearth_camera::visibility::{InheritedVisibility, RenderLayers};
use hearth_color::ColorToComponents;
use hearth_ecs::system::Query;
use hearth_image::{Image, TextureAtlasLayout};
use hearth_math::{vec2, Affine2, Affine3A, Vec2, Vec3};
use hearth_sprite::{
    Anchor, SliceScaleMode, Sprite, SpriteAlphaMode, SpriteImageMode, SpriteScalingMode,
};
use hearth_transform::components::GlobalTransform;
use tracing::error;

use crate::{
    draw::{Item, ItemPaint},
    raster::{Alpha, SliceUv, SpriteUv, TexelMap},
};

/// The sprites to draw this frame.
pub(crate) type SpriteQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Sprite,
        &'static Anchor,
        &'static GlobalTransform,
        &'static InheritedVisibility,
        Option<&'static RenderLayers>,
    ),
>;

/// Adds the visible sprites whose images are loaded to `items`.
pub(crate) fn collect<'a>(
    sprites: &'a SpriteQuery,
    images: &Assets<Image>,
    atlases: &Assets<TextureAtlasLayout>,
    items: &mut Vec<Item<'a>>,
) {
    for (sprite, anchor, transform, visibility, layers) in sprites {
        if !visibility.get() {
            continue;
        }
        let Some(image) = images.get(&sprite.image) else {
            continue;
        };
        let Some((quad_size, quad_offset, uv)) =
            layout(sprite, anchor, image.size().as_vec2(), atlases)
        else {
            continue;
        };
        // (s, t) to the sprite's own space: s rightward from the left edge, t down from the top.
        let local_from_unit = Affine3A::from_cols(
            Vec3::X.into(),
            Vec3::NEG_Y.into(),
            Vec3::Z.into(),
            Vec3::new(-0.5, 0.5, 0.0).into(),
        );
        let world_from_unit = transform.affine()
            * Affine3A::from_translation(quad_offset.extend(0.0))
            * Affine3A::from_scale(quad_size.extend(1.0))
            * local_from_unit;
        let map = match uv.tile == [None, None] && uv.slice.is_none() {
            true => {
                let mut flip = Affine2::IDENTITY;
                if uv.flip_x {
                    flip = Affine2::from_cols_array(&[-1.0, 0.0, 0.0, 1.0, 1.0, 0.0]) * flip;
                }
                if uv.flip_y {
                    flip = Affine2::from_cols_array(&[1.0, 0.0, 0.0, -1.0, 0.0, 1.0]) * flip;
                }
                TexelMap::Affine(Affine2::from_scale(uv.image_size) * uv.uv_transform * flip)
            }
            false => TexelMap::Sprite(uv),
        };
        items.push(Item {
            world_from_unit,
            z: transform.translation().z,
            layers,
            color: sprite.color.to_linear().to_f32_array(),
            paint: ItemPaint::Texture {
                image: sprite.image.id(),
                map,
            },
            alpha: match sprite.alpha_mode {
                SpriteAlphaMode::Blend => Alpha::Blend,
                SpriteAlphaMode::Opaque => Alpha::Opaque,
                SpriteAlphaMode::Mask(cutoff) => Alpha::Mask(cutoff),
            },
        });
    }
}

/// The size and offset of a sprite's quad, and how its UVs map into its image: the GPU sprite
/// material's numbers, from `hearth_sprite_render`.
fn layout(
    sprite: &Sprite,
    anchor: &Anchor,
    mut image_size: Vec2,
    atlases: &Assets<TextureAtlasLayout>,
) -> Option<(Vec2, Vec2, SpriteUv)> {
    let full_size = image_size;
    let mut quad_size = image_size;
    let mut quad_offset = Vec2::ZERO;
    let mut uv = SpriteUv {
        flip_x: sprite.flip_x,
        flip_y: sprite.flip_y,
        image_size: full_size,
        ..Default::default()
    };

    if let Some(atlas) = &sprite.texture_atlas
        && let Some(layout) = atlases.get(&atlas.layout)
        && !layout.textures.is_empty()
    {
        let rect = layout.textures[atlas.index.min(layout.textures.len() - 1)].as_rect();
        uv.uv_transform *= Affine2::from_scale(rect.size() / image_size);
        uv.uv_transform *= Affine2::from_translation(rect.min / rect.size());
        quad_size = rect.size();
        image_size = rect.size();
    }

    if let Some(rect) = sprite.rect {
        uv.uv_transform *= Affine2::from_scale(rect.size() / image_size);
        uv.uv_transform *= Affine2::from_translation(rect.min / rect.size());
        quad_size = rect.size();
        image_size = rect.size();
    }

    if let Some(custom_size) = sprite.custom_size {
        match &sprite.image_mode {
            SpriteImageMode::Auto => quad_size = custom_size,
            SpriteImageMode::Scale(mode) => {
                let quad_ratio = quad_size.x / quad_size.y;
                let custom_ratio = custom_size.x / custom_size.y;
                let fill_size = if quad_ratio > custom_ratio {
                    vec2(custom_size.y * quad_ratio, custom_size.y)
                } else {
                    vec2(custom_size.x, custom_size.x / quad_ratio)
                };
                let fit_size = if quad_ratio > custom_ratio {
                    vec2(custom_size.x, custom_size.x / quad_ratio)
                } else {
                    vec2(custom_size.y * quad_ratio, custom_size.y)
                };
                match mode {
                    SpriteScalingMode::FillCenter => {
                        uv.uv_transform *= Affine2::from_scale(custom_size / fill_size);
                        uv.uv_transform *= Affine2::from_translation(
                            (fill_size - custom_size) * 0.5 / custom_size,
                        );
                        quad_size = custom_size;
                    }
                    SpriteScalingMode::FillStart => {
                        uv.uv_transform *= Affine2::from_scale(custom_size / fill_size);
                        quad_size = custom_size;
                    }
                    SpriteScalingMode::FillEnd => {
                        uv.uv_transform *= Affine2::from_scale(custom_size / fill_size);
                        uv.uv_transform *=
                            Affine2::from_translation((fill_size - custom_size) / custom_size);
                        quad_size = custom_size;
                    }
                    SpriteScalingMode::FitCenter => quad_size = fit_size,
                    SpriteScalingMode::FitStart => {
                        quad_offset += (custom_size - fit_size) * vec2(-0.5, 0.5);
                        quad_size = fit_size;
                    }
                    SpriteScalingMode::FitEnd => {
                        quad_offset += (custom_size - fit_size) * vec2(0.5, -0.5);
                        quad_size = fit_size;
                    }
                }
            }
            SpriteImageMode::Tiled {
                tile_x,
                tile_y,
                stretch_value,
            } => {
                let stretch = (image_size * *stretch_value) / custom_size;
                uv.tile = [
                    (*tile_x && stretch.x > 0.0).then_some(stretch.x),
                    (*tile_y && stretch.y > 0.0).then_some(stretch.y),
                ];
                quad_size = custom_size;
            }
            SpriteImageMode::Sliced(_)
                if sprite.image_mode.slice_border_out_of_bounds(quad_size) =>
            {
                error!(
                    "TextureSlicer::border has out of bounds values. No slicing will be applied."
                );
                quad_size = custom_size;
            }
            SpriteImageMode::Sliced(slicer) => {
                let min_inset = slicer.border.min_inset / quad_size;
                let max_inset = slicer.border.max_inset / quad_size;
                let coef = custom_size / quad_size;
                let corner_scale = coef
                    .min_element()
                    .min(slicer.max_corner_scale)
                    .max(f32::EPSILON);
                let scale = coef / corner_scale;
                let stretch = |mode: SliceScaleMode| match mode {
                    SliceScaleMode::Stretch => Vec2::ZERO,
                    SliceScaleMode::Tile { stretch_value } => {
                        stretch_value * (image_size * (1.0 - max_inset - min_inset))
                            / (custom_size * (1.0 - max_inset / scale - min_inset / scale))
                    }
                };
                uv.slice = Some(SliceUv {
                    scale,
                    min_inset,
                    max_inset,
                    side_stretch: stretch(slicer.sides_scale_mode),
                    center_stretch: stretch(slicer.center_scale_mode),
                });
                quad_size = custom_size;
            }
        }
    }

    if quad_size.x == 0.0 || quad_size.y == 0.0 || !quad_size.is_finite() {
        return None;
    }
    quad_offset -= quad_size * anchor.as_vec();
    Some((quad_size, quad_offset, uv))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hearth_math::{Rect, Vec2};

    fn texel_at(sprite: &Sprite, image_size: Vec2, unit: Vec2) -> Vec2 {
        let (_, _, uv) = layout(sprite, &Anchor::CENTER, image_size, &Assets::default()).unwrap();
        uv.texel(unit)
    }

    #[test]
    fn maps_the_quad_onto_the_image() {
        let sprite = Sprite::default();
        let size = Vec2::new(8.0, 4.0);
        assert_eq!(texel_at(&sprite, size, Vec2::ZERO), Vec2::ZERO);
        assert_eq!(texel_at(&sprite, size, Vec2::ONE), size);
        let flipped = Sprite {
            flip_x: true,
            ..Sprite::default()
        };
        assert_eq!(texel_at(&flipped, size, Vec2::ZERO), Vec2::new(8.0, 0.0));
    }

    #[test]
    fn rect_picks_part_of_the_image() {
        let sprite = Sprite {
            rect: Some(Rect::new(2.0, 1.0, 6.0, 3.0)),
            ..Sprite::default()
        };
        let size = Vec2::new(8.0, 4.0);
        assert_eq!(texel_at(&sprite, size, Vec2::ZERO), Vec2::new(2.0, 1.0));
        assert_eq!(texel_at(&sprite, size, Vec2::ONE), Vec2::new(6.0, 3.0));
        let (quad_size, ..) = layout(&sprite, &Anchor::CENTER, size, &Assets::default()).unwrap();
        assert_eq!(quad_size, Vec2::new(4.0, 2.0));
    }

    #[test]
    fn anchor_moves_the_quad() {
        let (size, offset, _) = layout(
            &Sprite::sized(Vec2::new(10.0, 20.0)),
            &Anchor::BOTTOM_LEFT,
            Vec2::ONE,
            &Assets::default(),
        )
        .unwrap();
        assert_eq!(
            (size, offset),
            (Vec2::new(10.0, 20.0), Vec2::new(5.0, 10.0))
        );
    }
}
