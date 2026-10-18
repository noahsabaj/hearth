//! [`Text2d`](hearth_sprite::Text2d) as quads: its glyphs from the font atlases, and its
//! backgrounds, shadow, underlines and strikethroughs, as `hearth_sprite_render` draws them.

use hearth_camera::visibility::{InheritedVisibility, RenderLayers};
use hearth_color::{ColorToComponents, LinearRgba};
use hearth_ecs::{
    query::Has,
    system::{Query, SystemParam},
};
use hearth_math::{Affine2, Affine3A, Rect, Vec2, Vec3};
use hearth_sprite::{Anchor, Text2dShadow};
use hearth_text::{
    ComputedTextBlock, Strikethrough, StrikethroughColor, TextBackgroundColor, TextBounds,
    TextColor, TextLayoutInfo, Underline, UnderlineColor,
};
use hearth_transform::components::GlobalTransform;

use crate::{
    draw::{Item, ItemPaint},
    raster::{Alpha, TexelMap},
};

/// The 2D text to draw this frame.
#[derive(SystemParam)]
pub(crate) struct Texts<'w, 's> {
    texts: Query<
        'w,
        's,
        (
            &'static InheritedVisibility,
            &'static ComputedTextBlock,
            &'static TextLayoutInfo,
            &'static TextBounds,
            &'static Anchor,
            Option<&'static Text2dShadow>,
            &'static GlobalTransform,
            Option<&'static RenderLayers>,
        ),
    >,
    colors: Query<'w, 's, &'static TextColor>,
    backgrounds: Query<'w, 's, &'static TextBackgroundColor>,
    decorations: Query<
        'w,
        's,
        (
            &'static TextColor,
            Has<Strikethrough>,
            Has<Underline>,
            Option<&'static StrikethroughColor>,
            Option<&'static UnderlineColor>,
        ),
    >,
}

/// A rectangle of the text's layout as `(s, t)` to world, with `t` down the layout like its rows.
fn rect_to_world(layout_to_world: &Affine3A, rect: Rect) -> Affine3A {
    *layout_to_world
        * Affine3A::from_translation(rect.min.extend(0.0))
        * Affine3A::from_scale(rect.size().extend(1.0))
}

impl Texts<'_, '_> {
    /// Adds the visible text's quads to `items`, in the order they're drawn: backgrounds, shadow,
    /// glyphs, then decorations.
    pub(crate) fn collect<'a>(&'a self, items: &mut Vec<Item<'a>>) {
        for (visibility, block, layout, bounds, anchor, shadow, transform, layers) in &self.texts {
            if !visibility.get() {
                continue;
            }
            let inverse_scale = layout.scale_factor.recip();
            let size = Vec2::new(
                bounds.width.unwrap_or(layout.size.x * inverse_scale),
                bounds.height.unwrap_or(layout.size.y * inverse_scale),
            );
            let top_left = (Anchor::TOP_LEFT.0 - anchor.as_vec()) * size;
            // The layout's y runs down, in physical pixels.
            let layout_to_world = transform.affine()
                * Affine3A::from_translation(top_left.extend(0.0))
                * Affine3A::from_scale(Vec3::new(inverse_scale, -inverse_scale, 1.0));
            let z = transform.translation().z;
            let mut push = |world_from_unit: Affine3A, color: LinearRgba, paint: ItemPaint| {
                items.push(Item {
                    world_from_unit,
                    z,
                    layers,
                    color: color.to_f32_array(),
                    paint,
                    alpha: Alpha::Blend,
                });
            };
            let section = |index: u32| block.entities().get(index as usize).map(|e| e.entity);

            for run in &layout.run_geometry {
                if let Some(background) =
                    section(run.section_index).and_then(|entity| self.backgrounds.get(entity).ok())
                {
                    push(
                        rect_to_world(&layout_to_world, run.bounds),
                        background.0.into(),
                        ItemPaint::Solid,
                    );
                }
            }

            let glyphs = |push: &mut dyn FnMut(Affine3A, LinearRgba, ItemPaint),
                          layout_to_world: &Affine3A,
                          color_of: &dyn Fn(u32) -> LinearRgba| {
                for glyph in &layout.glyphs {
                    let rect = glyph.atlas_info.rect;
                    let quad = Rect::from_center_size(glyph.position, rect.size());
                    push(
                        rect_to_world(layout_to_world, quad),
                        color_of(glyph.section_index),
                        ItemPaint::Texture {
                            image: glyph.atlas_info.texture,
                            map: TexelMap::Affine(
                                Affine2::from_translation(rect.min)
                                    * Affine2::from_scale(rect.size()),
                            ),
                        },
                    );
                }
            };
            let decorations = |push: &mut dyn FnMut(Affine3A, LinearRgba, ItemPaint),
                               layout_to_world: &Affine3A,
                               shadow: Option<LinearRgba>| {
                for run in &layout.run_geometry {
                    let Some((
                        color,
                        strikethrough,
                        underline,
                        strikethrough_color,
                        underline_color,
                    )) = section(run.section_index)
                        .and_then(|entity| self.decorations.get(entity).ok())
                    else {
                        continue;
                    };
                    if strikethrough {
                        let color = shadow.unwrap_or_else(|| {
                            strikethrough_color.map_or(color.0, |c| c.0).to_linear()
                        });
                        let rect = Rect::from_center_size(
                            run.strikethrough_position(),
                            run.strikethrough_size(),
                        );
                        push(
                            rect_to_world(layout_to_world, rect),
                            color,
                            ItemPaint::Solid,
                        );
                    }
                    if underline {
                        let color = shadow.unwrap_or_else(|| {
                            underline_color.map_or(color.0, |c| c.0).to_linear()
                        });
                        let rect =
                            Rect::from_center_size(run.underline_position(), run.underline_size());
                        push(
                            rect_to_world(layout_to_world, rect),
                            color,
                            ItemPaint::Solid,
                        );
                    }
                }
            };

            if let Some(shadow) = shadow {
                let shadow_to_world = transform.affine()
                    * Affine3A::from_translation((top_left + shadow.offset).extend(0.0))
                    * Affine3A::from_scale(Vec3::new(inverse_scale, -inverse_scale, 1.0));
                let color = shadow.color.to_linear();
                glyphs(&mut push, &shadow_to_world, &|_| color);
                decorations(&mut push, &shadow_to_world, Some(color));
            }

            glyphs(&mut push, &layout_to_world, &|index| {
                section(index)
                    .and_then(|entity| self.colors.get(entity).ok())
                    .map(|color| color.0.to_linear())
                    .unwrap_or_default()
            });
            decorations(&mut push, &layout_to_world, None);
        }
    }
}
