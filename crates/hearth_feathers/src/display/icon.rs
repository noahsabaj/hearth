//! BSN Scene for loading images and displaying them as [`ImageNode`]s.
use hearth_ecs::query::{Changed, With};
use hearth_ecs::reflect::ReflectComponent;
use hearth_ecs::{component::Component, system::Query};
use hearth_reflect::Reflect;
use hearth_scene::{bsn, Scene};
use hearth_text::TextColor;
use hearth_ui::{px, widget::ImageNode, Node};

use crate::theme::ThemedText;

/// Marker to tint an icon's `ImageNode` by the text color.
#[derive(Component, Default, Clone, Reflect)]
#[reflect(Component)]
#[require(ThemedText)]
pub struct ThemedIcon;

/// Template which displays an icon.
pub fn icon(image: &'static str) -> impl Scene {
    bsn! {
        Node {
            height: px(14),
        }
        ImageNode {
            image: image
        }
        ThemedIcon
    }
}

/// Template which displays an icon and doesn't tint it by the text color.
pub fn icon_untinted(image: &'static str) -> impl Scene {
    bsn! {
        Node {
            height: px(14),
        }
        ImageNode {
            image: image
        }
    }
}

pub(crate) fn update_themed_icons(
    mut q_icons: Query<(&mut ImageNode, &TextColor), (With<ThemedIcon>, Changed<TextColor>)>,
) {
    for (mut image, color) in &mut q_icons {
        if image.color != color.0 {
            image.color = color.0;
        }
    }
}
