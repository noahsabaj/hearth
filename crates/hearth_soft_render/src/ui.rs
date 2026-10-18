//! UI nodes as quads in each camera's UI space, as `hearth_ui_render` draws them: backgrounds,
//! borders and outlines, images, the views of other cameras, and text with its shadow,
//! decorations, selection, cursor and inline images.

use hearth_asset::{AssetId, Assets};
use hearth_camera::visibility::InheritedVisibility;
use hearth_color::{Alpha as _, ColorToComponents, LinearRgba, Luminance};
use hearth_ecs::{
    entity::Entity,
    query::{AnyOf, Has, QueryData, With},
    system::{Local, Query, Res, SystemParam},
};
use hearth_image::{Image, TextureAtlasLayout, TRANSPARENT_IMAGE_HANDLE};
use hearth_input_focus::InputFocus;
use hearth_math::{Affine2, Rect, Vec2, Vec4};
use hearth_sprite::{BorderRect, SliceScaleMode, TextureSlicer};
use hearth_text::{
    ComputedTextBlock, EditableText, InlineBox, Strikethrough, StrikethroughColor,
    TextBackgroundColor, TextColor, TextCursorStyle, TextLayoutInfo, TextReadWriteMode, Underline,
    UnderlineColor,
};
use hearth_ui::{
    widget::{ImageNode, ImageNodeSize, InlineImage, NodeImageMode, TextShadow, ViewportNode},
    BackgroundColor, BackgroundGradient, BorderColor, BorderGradient, BorderStyle, BoxShadow,
    CalculatedClip, ComputedNode, ComputedStackIndex, ComputedUiTargetCamera, Display, Node,
    OuterColor, Outline, ResolvedBorderRadius, UiGlobalTransform, VisualBox,
};
use tracing::warn;

use crate::{
    draw::ItemPaint,
    raster::{ClipRect, TexelMap, UiSliceUv},
    rounded::{Part, RoundedBox, BOTTOM, LEFT, RIGHT, TOP},
};

/// Something a UI node draws: the unit square of `(s, t)`, placed in its camera's UI space, and
/// what covers it.
pub(crate) struct UiItem {
    /// The camera it's drawn over what's seen through.
    pub camera: Entity,
    /// Items are drawn from the lowest `z` up: the node's place in the UI stack, then the part.
    pub z: f32,
    /// Takes `(s, t)` into the camera's UI space: physical pixels from the top left of its
    /// viewport.
    pub ui_from_unit: Affine2,
    /// Linear RGBA: the color, or the image's tint.
    pub color: [f32; 4],
    pub paint: ItemPaint,
    /// The node's shape, in the units its `(s, t)` square spans; none for glyphs.
    pub rounded: Option<RoundedBox>,
    pub clip: Option<CalculatedClip>,
}

impl UiItem {
    /// The rectangles the item is clipped to, from the pixels of a target whose UI space
    /// `pixel_from_ui` places.
    pub(crate) fn clip_rects(&self, pixel_from_ui: Affine2) -> impl Iterator<Item = ClipRect> {
        let ui_from_pixel = pixel_from_ui.inverse();
        self.clip
            .iter()
            .flat_map(|clip| clip.rects().unwrap_or_default())
            .map(move |clip| ClipRect {
                clip_from_pixel: clip.world_to_clip_local * ui_from_pixel,
                rect: clip.rect,
            })
    }
}

/// Where each part of a node is in the stack, above the node's place: as in `hearth_ui_render`.
mod z {
    pub const BACKGROUND: f32 = 0.0;
    pub const BORDER: f32 = 0.01;
    pub const IMAGE: f32 = 0.04;
    pub const TEXT_SELECTION: f32 = 0.055;
    pub const TEXT: f32 = 0.06;
    pub const TEXT_STRIKETHROUGH: f32 = 0.07;
    pub const TEXT_CURSOR: f32 = 0.08;
    pub const INLINE_IMAGE: f32 = 0.09;
}

/// The amount a beveled border's sides are lightened or darkened.
const SHADE: f32 = 0.2;

#[derive(QueryData)]
pub(crate) struct UiNode {
    entity: Entity,
    node: Option<&'static Node>,
    computed: &'static ComputedNode,
    stack_index: &'static ComputedStackIndex,
    transform: &'static UiGlobalTransform,
    visibility: &'static InheritedVisibility,
    clip: Option<&'static CalculatedClip>,
    camera: &'static ComputedUiTargetCamera,
    background: Option<&'static BackgroundColor>,
    outer: Option<&'static OuterColor>,
    border_style: Option<&'static BorderStyle>,
    border_color: Option<&'static BorderColor>,
    outline: Option<&'static Outline>,
    image: Option<(&'static ImageNode, &'static ImageNodeSize)>,
    viewport: Option<&'static ViewportNode>,
    text: Option<(
        &'static TextLayoutInfo,
        &'static ComputedTextBlock,
        &'static TextColor,
    )>,
    text_shadow: Option<&'static TextShadow>,
    editable: Option<&'static EditableText>,
    cursor_style: Option<&'static TextCursorStyle>,
    read_write: Option<&'static TextReadWriteMode>,
    box_shadow: Has<BoxShadow>,
    gradient: Has<BackgroundGradient>,
    border_gradient: Has<BorderGradient>,
}

/// The UI to draw this frame.
#[derive(SystemParam)]
pub(crate) struct Uis<'w, 's> {
    nodes: Query<'w, 's, UiNode>,
    section_colors: Query<'w, 's, &'static TextColor>,
    section_decorations: Query<
        'w,
        's,
        (
            AnyOf<(
                &'static TextBackgroundColor,
                &'static Strikethrough,
                &'static Underline,
            )>,
            &'static TextColor,
            Option<&'static StrikethroughColor>,
            Option<&'static UnderlineColor>,
        ),
    >,
    shadow_decorations: Query<'w, 's, (Has<Strikethrough>, Has<Underline>)>,
    inline_images: Query<'w, 's, &'static InlineImage, With<InlineBox>>,
    input_focus: Option<Res<'w, InputFocus>>,
    warned: Local<'s, [bool; 3]>,
}

/// The node items come from: its camera, place in the stack and clip.
struct Source<'c> {
    camera: Entity,
    stack: f32,
    clip: Option<&'c CalculatedClip>,
}

impl Source<'_> {
    /// Adds a rectangle `size` big whose center `transform` takes into UI space.
    fn push(
        &self,
        items: &mut Vec<UiItem>,
        z: f32,
        transform: Affine2,
        size: Vec2,
        color: LinearRgba,
        paint: ItemPaint,
        rounded: Option<RoundedBox>,
    ) {
        items.push(UiItem {
            camera: self.camera,
            z: self.stack + z,
            ui_from_unit: transform
                * Affine2::from_translation(-0.5 * size)
                * Affine2::from_scale(size),
            color: color.to_f32_array(),
            paint,
            rounded,
            clip: self.clip.cloned(),
        });
    }

    /// Adds a plain rectangle, faded at its edges, as `hearth_ui_render` draws text backgrounds,
    /// decorations and cursors.
    fn push_rect(
        &self,
        items: &mut Vec<UiItem>,
        z: f32,
        transform: Affine2,
        rect: Rect,
        color: LinearRgba,
        radius: ResolvedBorderRadius,
    ) {
        let size = rect.size();
        self.push(
            items,
            z,
            transform * Affine2::from_translation(rect.center()),
            size,
            color,
            ItemPaint::Solid,
            Some(rounded(size, radius, BorderRect::ZERO, Part::Inside)),
        );
    }
}

/// A rounded box `size` big.
fn rounded(size: Vec2, radius: ResolvedBorderRadius, border: BorderRect, part: Part) -> RoundedBox {
    RoundedBox {
        size,
        radius_x: Vec4::new(
            radius.top_left.x,
            radius.top_right.x,
            radius.bottom_right.x,
            radius.bottom_left.x,
        ),
        radius_y: Vec4::new(
            radius.top_left.y,
            radius.top_right.y,
            radius.bottom_right.y,
            radius.bottom_left.y,
        ),
        border: Vec4::new(
            border.min_inset.x,
            border.min_inset.y,
            border.max_inset.x,
            border.max_inset.y,
        ),
        part,
    }
}

/// A box's radii less `inset`, to fit `inner_size`.
fn shrink_radius(
    radius: ResolvedBorderRadius,
    inset: BorderRect,
    inner_size: Vec2,
) -> ResolvedBorderRadius {
    let max = 0.5 * inner_size;
    let shrink = |radius: Vec2, by: Vec2| (radius - by).max(Vec2::ZERO).min(max);
    ResolvedBorderRadius {
        top_left: shrink(radius.top_left, inset.min_inset),
        top_right: shrink(
            radius.top_right,
            Vec2::new(inset.max_inset.x, inset.min_inset.y),
        ),
        bottom_right: shrink(radius.bottom_right, inset.max_inset),
        bottom_left: shrink(
            radius.bottom_left,
            Vec2::new(inset.min_inset.x, inset.max_inset.y),
        ),
    }
}

/// A border's colors lightened or darkened to look raised (`outset`) or sunk.
fn bevel(colors: [LinearRgba; 4], outset: bool) -> [LinearRgba; 4] {
    let (light, dark) = (
        |c: LinearRgba| c.lighter(SHADE),
        |c: LinearRgba| c.darker(SHADE),
    );
    let (top_left, bottom_right): (fn(LinearRgba) -> LinearRgba, fn(LinearRgba) -> LinearRgba) =
        if outset { (light, dark) } else { (dark, light) };
    [
        top_left(colors[0]),
        top_left(colors[1]),
        bottom_right(colors[2]),
        bottom_right(colors[3]),
    ]
}

impl Uis<'_, '_> {
    /// Adds the visible UI's items to `items`, each with its camera, in the order each node's
    /// parts are drawn. `camera_image` is the image a camera draws to, for viewport nodes.
    pub(crate) fn collect(
        &mut self,
        images: &Assets<Image>,
        atlases: &Assets<TextureAtlasLayout>,
        camera_image: impl Fn(Entity) -> Option<AssetId<Image>>,
        items: &mut Vec<UiItem>,
    ) {
        let mut unsupported = [false; 3];
        for node in &self.nodes {
            let Some(camera) = node.camera.get() else {
                continue;
            };
            let computed = node.computed;
            if !node.visibility.get() || node.clip.is_some_and(CalculatedClip::is_fully_clipped) {
                continue;
            }
            unsupported[0] |= node.box_shadow;
            unsupported[1] |= node.gradient || node.border_gradient;
            let from = Source {
                camera,
                stack: node.stack_index.0 as f32,
                clip: node.clip,
            };
            let transform = Affine2::from(node.transform);

            if !computed.is_empty() {
                self.background(&node, &from, transform, items);
                if let Some(viewport) = node.viewport
                    && let Some(image) = viewport.camera.and_then(&camera_image)
                {
                    let size = computed.size;
                    from.push(
                        items,
                        z::IMAGE,
                        transform,
                        size,
                        LinearRgba::WHITE,
                        ItemPaint::Texture {
                            image,
                            map: TexelMap::Affine(Affine2::from_scale(
                                images.get(image).map_or(Vec2::ONE, |i| i.size().as_vec2()),
                            )),
                        },
                        Some(rounded(
                            size,
                            computed.border_radius,
                            computed.border,
                            Part::Inside,
                        )),
                    );
                }
            }
            if let Some((image, image_size)) = node.image {
                image_node(
                    &from, computed, transform, image, image_size, images, atlases, items,
                );
            }
            if node.node.is_none_or(|n| n.display != Display::None) {
                borders(&node, &from, transform, items);
            }
            if let Some(text) = node.text
                && !computed.is_empty()
            {
                self.text(&node, &from, transform, text, images, items);
            }
        }
        let what = ["box shadows", "gradients"];
        for (i, what) in what.iter().enumerate() {
            if unsupported[i] && !core::mem::replace(&mut self.warned[i], true) {
                warn!("The soft renderer doesn't draw UI {what} yet");
            }
        }
    }

    fn background(
        &self,
        node: &UiNodeItem<'_, '_>,
        from: &Source,
        transform: Affine2,
        items: &mut Vec<UiItem>,
    ) {
        let computed = node.computed;
        let size = computed.size;
        if let Some(background) = node.background
            && !background.0.is_fully_transparent()
        {
            // A double border's gap shows the background.
            let inset = match node.border_style {
                Some(BorderStyle::Double) => BorderRect::ZERO,
                _ => computed.border,
            };
            from.push(
                items,
                z::BACKGROUND,
                transform,
                size,
                background.0.to_linear(),
                ItemPaint::Solid,
                Some(rounded(size, computed.border_radius, inset, Part::Inside)),
            );
        }
        if let Some(outer) = node.outer
            && !outer.0.is_fully_transparent()
        {
            from.push(
                items,
                z::BACKGROUND,
                transform,
                size,
                outer.0.to_linear(),
                ItemPaint::Solid,
                Some(rounded(
                    size,
                    computed.border_radius,
                    BorderRect::ZERO,
                    Part::Outside,
                )),
            );
        }
    }

    fn text(
        &self,
        node: &UiNodeItem<'_, '_>,
        from: &Source,
        transform: Affine2,
        (layout, block, text_color): (&TextLayoutInfo, &ComputedTextBlock, &TextColor),
        images: &Assets<Image>,
        items: &mut Vec<UiItem>,
    ) {
        let computed = node.computed;
        let scroll = node
            .editable
            .map_or(Vec2::ZERO, |text| text.viewport.offset);
        let content = transform * Affine2::from_translation(computed.content_box().min - scroll);
        // Editable text scrolls inside its content box.
        let clip = node.editable.map(|_| {
            node.clip
                .cloned()
                .unwrap_or_default()
                .with_rect(computed.content_box(), node.transform)
        });
        let from = Source {
            clip: clip.as_ref().or(from.clip),
            ..*from
        };
        let section = |index: u32| block.entities().get(index as usize).map(|e| e.entity);

        // Backgrounds and decorations.
        for run in &layout.run_geometry {
            let Some(Ok((
                (background, strikethrough, underline),
                color,
                strikethrough_color,
                underline_color,
            ))) = section(run.section_index).map(|entity| self.section_decorations.get(entity))
            else {
                continue;
            };
            if let Some(background) = background {
                from.push_rect(
                    items,
                    z::TEXT,
                    content,
                    run.bounds,
                    background.0.to_linear(),
                    ResolvedBorderRadius::ZERO,
                );
            }
            if strikethrough.is_some() {
                let color = strikethrough_color.map_or(color.0, |c| c.0).to_linear();
                let rect =
                    Rect::from_center_size(run.strikethrough_position(), run.strikethrough_size());
                let radius = ResolvedBorderRadius::ZERO;
                from.push_rect(items, z::TEXT_STRIKETHROUGH, content, rect, color, radius);
            }
            if underline.is_some() {
                let color = underline_color.map_or(color.0, |c| c.0).to_linear();
                let rect = Rect::from_center_size(run.underline_position(), run.underline_size());
                let radius = ResolvedBorderRadius::ZERO;
                from.push_rect(items, z::TEXT_STRIKETHROUGH, content, rect, color, radius);
            }
        }

        // The shadow: glyphs and decorations, all its color.
        if let Some(shadow) = node.text_shadow {
            let offset = shadow.offset / computed.inverse_scale_factor;
            let shadow_transform =
                transform * Affine2::from_translation(computed.content_box().min + offset - scroll);
            let color = shadow.color.to_linear();
            glyphs(&from, shadow_transform, layout, &|_| color, items);
            for run in &layout.run_geometry {
                let Some(Ok((strikethrough, underline))) =
                    section(run.section_index).map(|entity| self.shadow_decorations.get(entity))
                else {
                    continue;
                };
                let radius = ResolvedBorderRadius::ZERO;
                if strikethrough {
                    let rect = Rect::from_center_size(
                        run.strikethrough_position(),
                        run.strikethrough_size(),
                    );
                    from.push_rect(items, z::TEXT, shadow_transform, rect, color, radius);
                }
                if underline {
                    let rect =
                        Rect::from_center_size(run.underline_position(), run.underline_size());
                    from.push_rect(items, z::TEXT, shadow_transform, rect, color, radius);
                }
            }
        }

        // The glyphs, in their sections' colors; selected ones maybe in another.
        let selected_color = node
            .cursor_style
            .and_then(|style| style.selected_text_color)
            .map(|color| color.to_linear());
        let text_color = text_color.0.to_linear();
        let color_of =
            |glyph: &hearth_text::PositionedGlyph| {
                if !glyph.atlas_info.is_alpha_mask {
                    return LinearRgba::WHITE;
                }
                if let Some(selected) = selected_color {
                    let rect = Rect::from_center_size(glyph.position, glyph.atlas_info.rect.size());
                    if layout.selection_rects.iter().any(|selection| {
                        selection.contains(rect.min) && selection.contains(rect.max)
                    }) {
                        return selected;
                    }
                }
                if glyph.section_index == 0 {
                    return text_color;
                }
                section(glyph.section_index)
                    .and_then(|entity| self.section_colors.get(entity).ok())
                    .map(|color| color.0.to_linear())
                    .unwrap_or_default()
            };
        glyphs_by(&from, content, layout, &color_of, items);

        // Selection, cursor and the underline of text being composed.
        if let (Some(style), Some(read_write)) = (node.cursor_style, node.read_write) {
            self.selection_and_cursor(node, &from, content, layout, style, *read_write, items);
        }
        if node.editable.is_some() {
            for rect in &layout.preedit_underline_rects {
                from.push_rect(
                    items,
                    z::TEXT_STRIKETHROUGH,
                    content,
                    *rect,
                    text_color,
                    ResolvedBorderRadius::ZERO,
                );
            }
        }

        // Images inline with the text.
        for (inline, _, rect) in &layout.inline_boxes {
            let Ok(image) = self.inline_images.get(*inline) else {
                continue;
            };
            if rect.is_empty()
                || image.color.is_fully_transparent()
                || image.image.id() == TRANSPARENT_IMAGE_HANDLE.id()
            {
                continue;
            }
            let id = image.image.id();
            let Some(image_size) = images.get(id).map(|i| i.size().as_vec2()) else {
                continue;
            };
            let size = rect.size();
            from.push(
                items,
                z::INLINE_IMAGE,
                transform * Affine2::from_translation(computed.content_box().min + rect.center()),
                size,
                image.color.to_linear(),
                ItemPaint::Texture {
                    image: id,
                    map: TexelMap::Affine(flipped(image.flip_x, image.flip_y, image_size)),
                },
                Some(rounded(
                    size,
                    ResolvedBorderRadius::ZERO,
                    BorderRect::ZERO,
                    Part::Inside,
                )),
            );
        }
    }

    fn selection_and_cursor(
        &self,
        node: &UiNodeItem<'_, '_>,
        from: &Source,
        content: Affine2,
        layout: &TextLayoutInfo,
        style: &TextCursorStyle,
        read_write: TextReadWriteMode,
        items: &mut Vec<UiItem>,
    ) {
        if read_write == TextReadWriteMode::Static {
            return;
        }
        let focused = self
            .input_focus
            .as_ref()
            .is_some_and(|focus| focus.get() == Some(node.entity));
        let selection_color = if focused && read_write == TextReadWriteMode::Editable {
            style.selection_color
        } else {
            style.unfocused_selection_color
        };
        if !selection_color.is_fully_transparent() {
            let color = selection_color.to_linear();
            let rounding = style.selection_radius.clamp(0.0, 0.5);
            let rects = &layout.selection_rects;
            for (i, selection) in rects.iter().enumerate() {
                let radius = selection.height() * rounding;
                let mut corners = ResolvedBorderRadius {
                    top_left: Vec2::splat(radius),
                    top_right: Vec2::splat(radius),
                    bottom_right: Vec2::splat(radius),
                    bottom_left: Vec2::splat(radius),
                };
                // Where a line's selection meets the one above or below, its corners square off.
                if let Some(prev) = i.checked_sub(1).map(|i| rects[i]) {
                    if selection.min.x <= prev.max.x {
                        corners.top_left.x = (prev.min.x - selection.min.x).clamp(0.0, radius);
                    }
                    if prev.min.x <= selection.max.x {
                        corners.top_right.x = (selection.max.x - prev.max.x).clamp(0.0, radius);
                    }
                }
                if let Some(next) = rects.get(i + 1) {
                    if selection.min.x <= next.max.x {
                        corners.bottom_left.x = (next.min.x - selection.min.x).clamp(0.0, radius);
                    }
                    if next.min.x <= selection.max.x {
                        corners.bottom_right.x = (selection.max.x - next.max.x).clamp(0.0, radius);
                    }
                }
                from.push_rect(
                    items,
                    z::TEXT_SELECTION,
                    content,
                    *selection,
                    color,
                    corners,
                );
            }
        }
        if let Some((true, cursor)) = layout.cursor
            && !cursor.is_empty()
            && !style.color.is_fully_transparent()
        {
            from.push_rect(
                items,
                z::TEXT_CURSOR,
                content,
                cursor,
                style.color.to_linear(),
                ResolvedBorderRadius::ZERO,
            );
        }
    }
}

/// Adds a text's glyphs, each in the color `color_of` gives it.
fn glyphs_by(
    from: &Source,
    transform: Affine2,
    layout: &TextLayoutInfo,
    color_of: &dyn Fn(&hearth_text::PositionedGlyph) -> LinearRgba,
    items: &mut Vec<UiItem>,
) {
    for glyph in &layout.glyphs {
        let rect = glyph.atlas_info.rect;
        from.push(
            items,
            z::TEXT,
            transform * Affine2::from_translation(glyph.position),
            rect.size(),
            color_of(glyph),
            ItemPaint::Texture {
                image: glyph.atlas_info.texture,
                map: TexelMap::Affine(
                    Affine2::from_translation(rect.min) * Affine2::from_scale(rect.size()),
                ),
            },
            None,
        );
    }
}

/// Adds a text's glyphs, all in one color by section.
fn glyphs(
    from: &Source,
    transform: Affine2,
    layout: &TextLayoutInfo,
    color_of: &dyn Fn(u32) -> LinearRgba,
    items: &mut Vec<UiItem>,
) {
    glyphs_by(
        from,
        transform,
        layout,
        &|glyph| color_of(glyph.section_index),
        items,
    );
}

/// From `(s, t)` to the texel there in an image `size` big, flipped as asked.
fn flipped(flip_x: bool, flip_y: bool, size: Vec2) -> Affine2 {
    let mut uv = Affine2::IDENTITY;
    if flip_x {
        uv = Affine2::from_cols_array(&[-1.0, 0.0, 0.0, 1.0, 1.0, 0.0]) * uv;
    }
    if flip_y {
        uv = Affine2::from_cols_array(&[1.0, 0.0, 0.0, -1.0, 0.0, 1.0]) * uv;
    }
    Affine2::from_scale(size) * uv
}

/// Adds an [`ImageNode`]: stretched, fit, sliced or tiled, in its node's box.
fn image_node(
    from: &Source,
    computed: &ComputedNode,
    transform: Affine2,
    image: &ImageNode,
    image_size: &ImageNodeSize,
    images: &Assets<Image>,
    atlases: &Assets<TextureAtlasLayout>,
    items: &mut Vec<UiItem>,
) {
    let visual_box = match image.visual_box {
        VisualBox::ContentBox => computed.content_box(),
        VisualBox::PaddingBox => computed.padding_box(),
        VisualBox::BorderBox => computed.border_box(),
    };
    let id = image.image.id();
    if image.color.is_fully_transparent()
        || id == TRANSPARENT_IMAGE_HANDLE.id()
        || visual_box.size().cmple(Vec2::ZERO).any()
    {
        return;
    }
    let Some(texture_size) = images.get(id).map(|i| i.size().as_vec2()) else {
        return;
    };
    let atlas_rect = image
        .texture_atlas
        .as_ref()
        .and_then(|atlas| atlas.texture_rect(atlases))
        .map(|rect| rect.as_rect());
    // The part of the image shown, in texels.
    let source = match (atlas_rect, image.rect) {
        (None, None) => None,
        (None, Some(rect)) => Some(rect),
        (Some(atlas), None) => Some(atlas),
        (Some(atlas), Some(mut rect)) => {
            rect.min += atlas.min;
            rect.max += atlas.min;
            Some(rect)
        }
    };
    let color = image.color.to_linear();

    if image.image_mode.uses_slices() {
        let size = visual_box.size();
        let source = source.unwrap_or(Rect::from_corners(Vec2::ZERO, texture_size));
        let mut rect = [source.min.x, source.min.y, source.max.x, source.max.y];
        if image.flip_x {
            rect.swap(0, 2);
        }
        if image.flip_y {
            rect.swap(1, 3);
        }
        let Some([texture, target, repeat]) = slices(
            source.size(),
            size * computed.inverse_scale_factor,
            &image.image_mode,
        ) else {
            return;
        };
        from.push(
            items,
            z::IMAGE,
            transform * Affine2::from_translation(visual_box.center()),
            size,
            color,
            ItemPaint::Texture {
                image: id,
                map: TexelMap::UiSlice(UiSliceUv {
                    texture,
                    target,
                    repeat,
                    rect,
                }),
            },
            None,
        );
        return;
    }

    let size = if matches!(image.image_mode, NodeImageMode::Auto) {
        let natural = image_size.size().as_vec2();
        if natural.cmple(Vec2::ZERO).any() {
            visual_box.size()
        } else {
            natural * (visual_box.size() / natural).min_element()
        }
    } else {
        visual_box.size()
    };
    // The node's corners, less how far the image is inset from its edges.
    let mut inset = match image.visual_box {
        VisualBox::ContentBox => computed.content_inset(),
        VisualBox::PaddingBox => computed.border,
        VisualBox::BorderBox => BorderRect::ZERO,
    };
    let centered = 0.5 * (visual_box.size() - size);
    inset.min_inset += centered;
    inset.max_inset += centered;
    let radius = shrink_radius(computed.border_radius, inset, size);
    let source = source.unwrap_or(Rect::from_corners(Vec2::ZERO, texture_size));
    let map = Affine2::from_translation(source.min)
        * Affine2::from_scale(source.size())
        * flipped(image.flip_x, image.flip_y, Vec2::ONE);
    from.push(
        items,
        z::IMAGE,
        transform * Affine2::from_translation(visual_box.center()),
        size,
        color,
        ItemPaint::Texture {
            image: id,
            map: TexelMap::Affine(map),
        },
        Some(rounded(size, radius, BorderRect::ZERO, Part::Inside)),
    );
}

/// For a sliced or tiled image whose shown part is `image_size` texels, drawn `target_size`
/// big: where the slices divide the image and the node, and how often the sides and center
/// repeat.
fn slices(image_size: Vec2, target_size: Vec2, mode: &NodeImageMode) -> Option<[[f32; 4]; 3]> {
    let repeats = |image: f32, target: f32, mode: &SliceScaleMode| match mode {
        SliceScaleMode::Stretch => 1.0,
        SliceScaleMode::Tile { stretch_value } => target / (image * stretch_value),
    };
    match mode {
        NodeImageMode::Sliced(TextureSlicer {
            border,
            center_scale_mode,
            sides_scale_mode,
            max_corner_scale,
        }) => {
            let scale = (target_size / image_size)
                .min_element()
                .min(*max_corner_scale);
            let texture = [
                border.min_inset.x / image_size.x,
                border.min_inset.y / image_size.y,
                1.0 - border.max_inset.x / image_size.x,
                1.0 - border.max_inset.y / image_size.y,
            ];
            let target = [
                border.min_inset.x / target_size.x * scale,
                border.min_inset.y / target_size.y * scale,
                1.0 - border.max_inset.x / target_size.x * scale,
                1.0 - border.max_inset.y / target_size.y * scale,
            ];
            let image_side =
                image_size * Vec2::new(texture[2] - texture[0], texture[3] - texture[1]);
            let target_side = target_size * Vec2::new(target[2] - target[0], target[3] - target[1]);
            Some([
                texture,
                target,
                [
                    repeats(image_side.x, target_side.x, sides_scale_mode),
                    repeats(image_side.y, target_side.y, sides_scale_mode),
                    repeats(image_side.x, target_side.x, center_scale_mode),
                    repeats(image_side.y, target_side.y, center_scale_mode),
                ],
            ])
        }
        NodeImageMode::Tiled {
            tile_x,
            tile_y,
            stretch_value,
        } => {
            let tiles = |tile: bool, image: f32, target: f32| {
                if tile {
                    target / (image * stretch_value)
                } else {
                    1.0
                }
            };
            Some([
                [0.0, 0.0, 1.0, 1.0],
                [0.0, 0.0, 1.0, 1.0],
                [
                    1.0,
                    1.0,
                    tiles(*tile_x, image_size.x, target_size.x),
                    tiles(*tile_y, image_size.y, target_size.y),
                ],
            ])
        }
        _ => None,
    }
}

/// Adds a node's border, in rings for its style, and its outline.
fn borders(node: &UiNodeItem<'_, '_>, from: &Source, transform: Affine2, items: &mut Vec<UiItem>) {
    let computed = node.computed;
    if computed.border != BorderRect::ZERO
        && let Some(border_color) = node.border_color
    {
        let colors = [
            border_color.left.to_linear(),
            border_color.top.to_linear(),
            border_color.right.to_linear(),
            border_color.bottom.to_linear(),
        ];
        let colors = match node.border_style {
            Some(BorderStyle::Inset) => bevel(colors, false),
            Some(BorderStyle::Outset) => bevel(colors, true),
            _ => colors,
        };
        let size = computed.size;
        let radius = computed.border_radius;
        let border = computed.border;
        let ring = |items: &mut Vec<UiItem>,
                    transform: Affine2,
                    size: Vec2,
                    radius: ResolvedBorderRadius,
                    thickness: BorderRect,
                    colors: [LinearRgba; 4]| {
            border_ring(from, items, transform, size, radius, thickness, colors);
        };
        match node.border_style {
            Some(style @ (BorderStyle::Double | BorderStyle::Groove | BorderStyle::Ridge)) => {
                let (stripe, inner_inset) = if *style == BorderStyle::Double {
                    let third = border / 3.0;
                    (third, third * 2.0)
                } else {
                    let half = border / 2.0;
                    (half, half)
                };
                let (outer_colors, inner_colors) = match style {
                    BorderStyle::Groove => (bevel(colors, false), bevel(colors, true)),
                    BorderStyle::Ridge => (bevel(colors, true), bevel(colors, false)),
                    _ => (colors, colors),
                };
                ring(items, transform, size, radius, stripe, outer_colors);
                let inner_size = size - inner_inset.min_inset - inner_inset.max_inset;
                if inner_size.cmpgt(Vec2::ZERO).all() {
                    let inner_radius = shrink_radius(radius, inner_inset, inner_size);
                    let offset = 0.5 * (inner_inset.min_inset - inner_inset.max_inset);
                    ring(
                        items,
                        transform * Affine2::from_translation(offset),
                        inner_size,
                        inner_radius,
                        stripe,
                        inner_colors,
                    );
                }
            }
            _ => ring(items, transform, size, radius, border, colors),
        }
    }

    if computed.outline_width() > 0.0
        && let Some(outline) = node.outline
        && !outline.color.is_fully_transparent()
    {
        let size = computed.outlined_node_size();
        from.push(
            items,
            z::BORDER,
            transform,
            size,
            outline.color.to_linear(),
            ItemPaint::Solid,
            Some(rounded(
                size,
                computed.outline_radius(),
                BorderRect::all(computed.outline_width()),
                Part::Border(LEFT | TOP | RIGHT | BOTTOM),
            )),
        );
    }
}

/// Adds a ring of border, one item per color, each covering the sides of its color.
fn border_ring(
    from: &Source,
    items: &mut Vec<UiItem>,
    transform: Affine2,
    size: Vec2,
    radius: ResolvedBorderRadius,
    thickness: BorderRect,
    colors: [LinearRgba; 4],
) {
    const SIDES: [u8; 4] = [LEFT, TOP, RIGHT, BOTTOM];
    let mut done = 0;
    for (i, &color) in colors.iter().enumerate() {
        if color.is_fully_transparent() || done & SIDES[i] != 0 {
            continue;
        }
        let mut sides = SIDES[i];
        for (&side, &other) in SIDES.iter().zip(&colors).skip(i + 1) {
            if other == color {
                sides |= side;
            }
        }
        done |= sides;
        from.push(
            items,
            z::BORDER,
            transform,
            size,
            color,
            ItemPaint::Solid,
            Some(rounded(size, radius, thickness, Part::Border(sides))),
        );
    }
}
