//! The 3D viewport: the editor's camera, the grid, selecting by clicking, and the transform gizmo.

use alloc::vec::Vec;

use hearth_app::{App, PostUpdate, PreUpdate, Startup, Update};
use hearth_camera::{
    primitives::Aabb, visibility::RenderLayers, Camera, Camera2d, Camera3d, ClearColorConfig,
    Viewport,
};
use hearth_camera_controller::free_camera::{FreeCamera, FreeCameraState};
use hearth_color::Color;
use hearth_dev_tools::infinite_grid::InfiniteGrid;
use hearth_ecs::{
    component::Component,
    entity::Entity,
    name::Name,
    observer::On,
    query::With,
    resource::Resource,
    schedule::{IntoScheduleConfigs, SystemSet},
    system::{Commands, Query, Res, ResMut, Single},
    world::World,
};
use hearth_gizmos::{
    gizmos::Gizmos,
    transform_gizmo::{
        TransformGizmoCamera, TransformGizmoFocus, TransformGizmoMode, TransformGizmoSettings,
        TransformGizmoState, TransformGizmoSystems,
    },
};
use hearth_input::{mouse::MouseButton, ButtonInput};
use hearth_inspector::{InspectorSelection, InspectorSource};
use hearth_math::{UVec2, Vec2, Vec3};
use hearth_picking::{events::PointerClick, pointer::PointerButton};
use hearth_render::view::Msaa;
use hearth_shape::Aabb3d;
use hearth_transform::components::{GlobalTransform, Transform};
use hearth_ui::{ComputedNode, IsDefaultUiCamera, UiGlobalTransform};
use hearth_window::{PrimaryWindow, Window};

use crate::{history, scene};

/// Marker for the UI node the viewport renders into.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ViewportPanel;

/// Marker for the camera the viewport renders the scene with.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct EditorCamera;

/// The render layer the editor's UI camera draws, so it doesn't also draw the scene's gizmos.
pub const UI_LAYER: usize = 31;

/// The UI camera's order: after the scene camera (0) and the transform gizmo's overlay (1).
const UI_CAMERA_ORDER: isize = 10;

/// Where the pointer is, as far as the viewport is concerned.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct ViewportPointer {
    /// The viewport's rectangle in physical window pixels: its corner and size.
    pub rect: Option<(Vec2, Vec2)>,
    /// Whether the cursor is over the viewport.
    pub hovered: bool,
    /// Whether the camera is flying, which lasts while the right button is held after pressing it
    /// over the viewport.
    pub flying: bool,
}

/// Systems that read the pointer over the viewport, which run before the camera and the gizmo.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct ViewportInputSystems;

/// The color the selection's bounds are outlined with.
pub const SELECTION_COLOR: Color = Color::srgb(1.0, 0.62, 0.15);

fn setup(mut commands: Commands) {
    commands.spawn((
        Name::new("Editor Camera"),
        EditorCamera,
        Camera3d::default(),
        Camera {
            order: 0,
            ..Default::default()
        },
        Transform::from_xyz(-6.0, 5.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        FreeCamera::default(),
        TransformGizmoCamera,
    ));
    // The UI draws into a texture of its own, cleared to transparent and blended over the scene:
    // without MSAA its texture isn't shared with the scene camera's, which would composite a copy
    // of the scene taken before the gizmos were drawn.
    commands.spawn((
        Name::new("Editor UI Camera"),
        Camera2d,
        Camera {
            order: UI_CAMERA_ORDER,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..Default::default()
        },
        Msaa::Off,
        IsDefaultUiCamera,
        RenderLayers::layer(UI_LAYER),
    ));
    commands.spawn((Name::new("Editor Grid"), InfiniteGrid));
}

/// Fits the editor camera's viewport to the viewport panel.
fn fit_camera_to_panel(
    panel: Option<Single<(&ComputedNode, &UiGlobalTransform), With<ViewportPanel>>>,
    mut camera: Single<&mut Camera, With<EditorCamera>>,
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut pointer: ResMut<ViewportPointer>,
) {
    let (Some(panel), Some(window)) = (panel, window) else {
        return;
    };
    let (node, transform) = *panel;
    let window_size = window.physical_size().as_vec2();
    let size = node.size();
    let corner = (transform.translation - size / 2.0).max(Vec2::ZERO);
    let size = size.min(window_size - corner).max(Vec2::ONE);
    if size.x < 2.0 || size.y < 2.0 {
        return;
    }
    let viewport = Viewport {
        physical_position: corner.as_uvec2(),
        physical_size: size.as_uvec2().max(UVec2::ONE),
        ..Default::default()
    };
    if camera.viewport.as_ref().is_none_or(|current| {
        current.physical_position != viewport.physical_position
            || current.physical_size != viewport.physical_size
    }) {
        camera.viewport = Some(viewport);
    }
    pointer.rect = Some((corner, size));
}

/// Tracks whether the cursor is over the viewport, and flies the camera while the right button
/// is held after pressing it there.
fn track_pointer(
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut pointer: ResMut<ViewportPointer>,
    mut camera: Single<&mut FreeCameraState, With<EditorCamera>>,
) {
    let cursor = window.and_then(|window| window.physical_cursor_position());
    pointer.hovered = match (cursor, pointer.rect) {
        (Some(cursor), Some((corner, size))) => {
            cursor.cmpge(corner).all() && cursor.cmplt(corner + size).all()
        }
        _ => false,
    };
    if buttons.just_pressed(MouseButton::Right) && pointer.hovered {
        pointer.flying = true;
    }
    if !buttons.pressed(MouseButton::Right) {
        pointer.flying = false;
    }
    if camera.enabled != pointer.flying {
        camera.enabled = pointer.flying;
    }
}

/// Whether the gizmo may react to the pointer: over the viewport, or while it is dragging.
fn gizmo_may_run(pointer: Res<ViewportPointer>, state: Res<TransformGizmoState>) -> bool {
    state.active || (pointer.hovered && !pointer.flying)
}

/// Selects what a click in the viewport hits, or clears the selection on a click on nothing.
fn select_on_click(
    click: On<PointerClick>,
    pointer: Res<ViewportPointer>,
    gizmo: Res<TransformGizmoState>,
    windows: Query<(), With<Window>>,
    mut commands: Commands,
) {
    // The click bubbles up from what was hit to its ancestors; look at it once, where it began,
    // and let it go on bubbling for the UI's own observers.
    let target = click.original_event_target();
    if click.entity != target
        || click.button != PointerButton::Primary
        || !pointer.hovered
        || gizmo.hovered_axis.is_some()
    {
        return;
    }
    let on_nothing = windows.contains(target);
    commands.queue(move |world: &mut World| {
        if !matches!(*world.resource::<InspectorSource>(), InspectorSource::Local) {
            return;
        }
        let selected = if on_nothing {
            None
        } else {
            match scene::selectable(world, target) {
                Some(entity) => Some(entity),
                // Clicks on the editor's own things, such as the gizmo, change nothing.
                None => return,
            }
        };
        let mut selection = world.resource_mut::<InspectorSelection>();
        if selection.0 != selected {
            selection.0 = selected;
        }
    });
}

/// Puts the gizmo on the selection, when it is a scene entity with a transform.
fn focus_gizmo_on_selection(world: &mut World) {
    let selected = world.resource::<InspectorSelection>().0;
    let target = selected.filter(|entity| {
        matches!(*world.resource::<InspectorSource>(), InspectorSource::Local)
            && world.get::<Transform>(*entity).is_some()
            && scene::in_scene(world, *entity)
    });
    let focused: Vec<Entity> = world
        .query_filtered::<Entity, With<TransformGizmoFocus>>()
        .iter(world)
        .collect();
    for entity in focused {
        if Some(entity) != target {
            world.entity_mut(entity).remove::<TransformGizmoFocus>();
        }
    }
    if let Some(entity) = target
        && world.get::<TransformGizmoFocus>(entity).is_none()
    {
        world.entity_mut(entity).insert(TransformGizmoFocus);
    }
}

/// Starts recording a gizmo drag as one edit when it begins, and keeps it open while it goes on.
fn record_gizmo_drags(world: &mut World) {
    let pressed = world
        .resource::<ButtonInput<MouseButton>>()
        .just_pressed(MouseButton::Left);
    let state = world.resource::<TransformGizmoState>();
    let (hovered, active) = (state.hovered_axis.is_some(), state.active);
    if active {
        world.resource_mut::<history::History>().touch();
        return;
    }
    if !pressed || !hovered || !world.resource::<ViewportPointer>().hovered {
        return;
    }
    let Some(entity) = world
        .query_filtered::<Entity, With<TransformGizmoFocus>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let label = match world.resource::<TransformGizmoSettings>().mode {
        TransformGizmoMode::Translate => "Move",
        TransformGizmoMode::Rotate => "Rotate",
        TransformGizmoMode::Scale => "Scale",
    };
    history::begin(world, label, &[entity]);
}

/// Outlines the selection's bounds.
fn outline_selection(
    selection: Res<InspectorSelection>,
    source: Res<InspectorSource>,
    bounds: Query<(&Aabb, &GlobalTransform)>,
    children: Query<&hearth_ecs::hierarchy::Children>,
    mut gizmos: Gizmos,
) {
    let (Some(selected), InspectorSource::Local) = (selection.0, &*source) else {
        return;
    };
    // An instance such as a glTF model has its meshes below it.
    let mut stack = alloc::vec![selected];
    let mut drawn = 0;
    while let Some(entity) = stack.pop() {
        if let Ok((aabb, transform)) = bounds.get(entity) {
            gizmos.aabb_3d(
                Aabb3d::new(Vec3::from(aabb.center), Vec3::from(aabb.half_extents)),
                *transform,
                SELECTION_COLOR,
            );
            drawn += 1;
        }
        if drawn > 256 {
            break;
        }
        if let Ok(children) = children.get(entity) {
            stack.extend(children.iter());
        }
    }
}

/// Moves the camera so the selection fills the view.
pub fn frame_selection(world: &mut World) {
    let Some(selected) = world.resource::<InspectorSelection>().0 else {
        return;
    };
    let Some(target) = world
        .get::<GlobalTransform>(selected)
        .map(GlobalTransform::translation)
    else {
        return;
    };
    let radius = world
        .get::<Aabb>(selected)
        .map(|aabb| Vec3::from(aabb.half_extents).length())
        .unwrap_or(1.0)
        .max(0.5);
    let mut cameras = world.query_filtered::<&mut Transform, With<EditorCamera>>();
    let Ok(mut camera) = cameras.single_mut(world) else {
        return;
    };
    let back = camera.back();
    camera.translation = target + *back * radius * 3.0;
    camera.look_at(target, Vec3::Y);
    // The fly camera keeps its own yaw and pitch; let it read them from the new rotation.
    let rotation = camera.rotation;
    let mut states = world.query_filtered::<&mut FreeCameraState, With<EditorCamera>>();
    if let Ok(mut state) = states.single_mut(world) {
        let (_, yaw, pitch) = rotation.to_euler(hearth_math::EulerRot::ZYX);
        state.yaw = yaw;
        state.pitch = pitch;
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<ViewportPointer>()
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, track_pointer.in_set(ViewportInputSystems))
        .configure_sets(PostUpdate, TransformGizmoSystems.run_if(gizmo_may_run))
        .add_systems(
            PostUpdate,
            (
                fit_camera_to_panel.after(hearth_ui::UiSystems::Layout),
                (focus_gizmo_on_selection, record_gizmo_drags)
                    .chain()
                    .before(TransformGizmoSystems),
            ),
        )
        .add_systems(Update, outline_selection)
        .add_observer(select_on_click);
}
