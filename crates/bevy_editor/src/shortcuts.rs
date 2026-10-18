//! Keyboard shortcuts.

use bevy_app::{App, Update};
use bevy_ecs::{
    system::{Commands, Query, Res},
    world::World,
};
use bevy_gizmos::transform_gizmo::{TransformGizmoMode, TransformGizmoSettings};
use bevy_input::{keyboard::KeyCode, ButtonInput};
use bevy_input_focus::InputFocus;
use bevy_text::EditableText;

use crate::{history, layout, ops, play, scene, status::Status, viewport};

/// Turns key presses into editor operations. Keys go to a text field instead while one has focus,
/// and to the camera while it flies.
fn shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    texts: Query<(), bevy_ecs::query::With<EditableText>>,
    pointer: Res<viewport::ViewportPointer>,
    mut commands: Commands,
) {
    let typing = focus.get().is_some_and(|entity| texts.contains(entity));
    if typing || pointer.flying {
        return;
    }
    let ctrl = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    let pressed = |key| keys.just_pressed(key);

    let operation: Option<fn(&mut World)> = if ctrl {
        if pressed(KeyCode::KeyZ) && shift || pressed(KeyCode::KeyY) {
            Some(|world| match history::redo(world) {
                Some(label) => Status::info(world, alloc::format!("Redid {label}")),
                None => Status::info(world, "Nothing to redo"),
            })
        } else if pressed(KeyCode::KeyZ) {
            Some(|world| match history::undo(world) {
                Some(label) => Status::info(world, alloc::format!("Undid {label}")),
                None => Status::info(world, "Nothing to undo"),
            })
        } else if pressed(KeyCode::KeyS) {
            Some(|world| {
                history::commit(world);
                scene::save_and_report(world);
            })
        } else if pressed(KeyCode::KeyN) {
            Some(layout::new_scene_checked)
        } else if pressed(KeyCode::KeyD) {
            Some(ops::duplicate_selected)
        } else {
            None
        }
    } else if pressed(KeyCode::F5) {
        Some(if shift { play::stop } else { play::play })
    } else if pressed(KeyCode::Delete) {
        Some(ops::delete_selected)
    } else if pressed(KeyCode::KeyF) {
        Some(viewport::frame_selection)
    } else if pressed(KeyCode::Escape) {
        Some(|world| ops::select(world, None))
    } else if pressed(KeyCode::KeyW) {
        Some(|world| {
            world.resource_mut::<TransformGizmoSettings>().mode = TransformGizmoMode::Translate;
        })
    } else if pressed(KeyCode::KeyE) {
        Some(|world| {
            world.resource_mut::<TransformGizmoSettings>().mode = TransformGizmoMode::Rotate;
        })
    } else if pressed(KeyCode::KeyR) {
        Some(|world| {
            world.resource_mut::<TransformGizmoSettings>().mode = TransformGizmoMode::Scale;
        })
    } else {
        None
    };
    if let Some(operation) = operation {
        commands.queue(operation);
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Update, shortcuts);
}
