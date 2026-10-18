//! The scene being edited, and opening and saving scene files.
//!
//! The scene's top-level entities are children of one root entity, [`EditedScene::root`], which
//! keeps them apart from the editor's own entities. A saved file holds them without that root:
//! the game spawns the file under a `DynamicWorldRoot` entity of its own.

use alloc::{format, string::String, vec::Vec};
use std::path::PathBuf;

use bevy_app::{App, PreUpdate, Startup};
use bevy_asset::{
    AssetServer, Assets, EphemeralHandleBehavior, Handle, HandleSerializeProcessor, LoadState,
};
use bevy_camera::{
    primitives::{Aabb, CascadesFrusta, CubemapFrusta, Frustum},
    visibility::{
        CascadesVisibleEntities, CubemapVisibleEntities, InheritedVisibility, ViewVisibility,
        Visibility, VisibilityClass, VisibleEntities, VisibleMeshEntities,
    },
    Camera, Camera3d, CameraMainTextureUsages,
};
use bevy_color::Color;
use bevy_ecs::{
    component::Component,
    entity::{Entity, EntityHashMap},
    hierarchy::{ChildOf, Children},
    name::Name,
    query::{With, Without},
    reflect::{AppTypeRegistry, ReflectComponent},
    resource::Resource,
    system::{Commands, Query, Res},
    world::World,
};
use bevy_gizmos::transform_gizmo::TransformGizmoFocus;
use bevy_inspector::{InspectorSelection, InspectorTreeRoot};
use bevy_light::{Cascades, DirectionalLight};
use bevy_math::{Quat, Vec2, Vec3};
use bevy_pbr::{PreviousGlobalTransform, Shape3d, ShapeMesh};
use bevy_picking::hover::PickingInteraction;
use bevy_reflect::{
    prelude::ReflectDefault, serde::TypedReflectSerializer, FromReflect, PartialReflect, Reflect,
    TypeRegistry,
};
use bevy_render::camera::CameraRenderGraph;
use bevy_transform::components::{GlobalTransform, Transform, TransformTreeChanged};
use bevy_world_serialization::{
    DynamicWorld, DynamicWorldBuilder, DynamicWorldRoot, WorldAssetRoot, WorldFilter,
};

use crate::{
    history,
    project::{asset_path_string, EditorProject},
    status::Status,
};

/// Marker for the root entity of the scene being edited.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct EditorSceneRoot;

/// The scene being edited.
#[derive(Resource, Debug)]
pub struct EditedScene {
    /// The entity whose children are the scene's top-level entities.
    pub root: Entity,
    /// Where the scene is saved, relative to the assets folder.
    pub path: PathBuf,
    /// Whether it has changed since it was opened or saved.
    pub dirty: bool,
    /// The file being opened, until it has loaded.
    loading: Option<Handle<DynamicWorld>>,
}

impl EditedScene {
    /// Whether the scene is still loading from its file.
    pub fn is_loading(&self) -> bool {
        self.loading.is_some()
    }

    /// The scene's file name, for the window title and status bar.
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| asset_path_string(&self.path))
    }
}

/// A camera of the scene, kept from rendering while the scene is edited; the viewport has its own.
/// Saving writes `active` back as the camera's [`Camera::is_active`].
#[derive(Component, Reflect, Debug, Clone, Copy, PartialEq, Eq)]
#[reflect(Component, Default, Debug, Clone, PartialEq)]
pub struct ParkedCamera {
    /// Whether the camera renders when the game runs.
    pub active: bool,
}

impl Default for ParkedCamera {
    fn default() -> Self {
        Self { active: true }
    }
}

/// The components a scene file never holds: the editor's own, and those the engine derives from
/// others when the scene is spawned.
pub fn save_filter() -> WorldFilter {
    deny_computed(
        WorldFilter::default()
            .deny::<Children>()
            .deny::<TransformGizmoFocus>()
            .deny::<ParkedCamera>(),
    )
}

/// Denies the components the engine works out from others, such as global transforms, bounds and
/// visibility. Scene files and undo steps leave them out; they are computed again.
pub(crate) fn deny_computed(filter: WorldFilter) -> WorldFilter {
    filter
        .deny::<GlobalTransform>()
        .deny::<PreviousGlobalTransform>()
        .deny::<TransformTreeChanged>()
        .deny::<InheritedVisibility>()
        .deny::<ViewVisibility>()
        .deny::<VisibilityClass>()
        .deny::<VisibleEntities>()
        .deny::<VisibleMeshEntities>()
        .deny::<CubemapVisibleEntities>()
        .deny::<CascadesVisibleEntities>()
        .deny::<Aabb>()
        .deny::<Frustum>()
        .deny::<CubemapFrusta>()
        .deny::<CascadesFrusta>()
        .deny::<Cascades>()
        .deny::<PickingInteraction>()
        // Set up by the engine for every camera, and with no saved form.
        .deny::<CameraMainTextureUsages>()
        .deny::<CameraRenderGraph>()
}

/// Whether `component` holds its type's default value, so leaving it out of a file loses nothing:
/// a component another requires comes back at its default when the scene is spawned.
fn is_default(component: &dyn PartialReflect, registry: &TypeRegistry) -> bool {
    component
        .get_represented_type_info()
        .and_then(|info| registry.get(info.type_id()))
        .and_then(|registration| registration.data::<ReflectDefault>())
        .and_then(|default| default.default().reflect_partial_eq(component))
        .unwrap_or(false)
}

/// Whether `component` can be written to a scene file and read back: it has a serialized form,
/// and any asset handles in it point at files rather than at assets made in memory, which would
/// come back as empty handles.
fn serializes(component: &dyn PartialReflect, registry: &TypeRegistry) -> bool {
    let processor = HandleSerializeProcessor {
        ephemeral_handle_behavior: EphemeralHandleBehavior::Error,
    };
    ron::to_string(&TypedReflectSerializer::with_processor(
        component, registry, &processor,
    ))
    .is_ok()
}

/// Whether `entity` is a glTF or scene instance, whose descendants are spawned from its asset.
pub fn is_instance(world: &World, entity: Entity) -> bool {
    world.get_entity(entity).is_ok_and(|entity| {
        entity.contains::<WorldAssetRoot>() || entity.contains::<DynamicWorldRoot>()
    })
}

/// Whether `entity` belongs to the scene being edited, and isn't spawned by an instance in it.
pub fn in_scene(world: &World, entity: Entity) -> bool {
    let Some(scene) = world.get_resource::<EditedScene>() else {
        return false;
    };
    let mut current = entity;
    let mut inside_instance = false;
    while let Some(child_of) = world.get::<ChildOf>(current) {
        current = child_of.parent();
        if current == scene.root {
            return !inside_instance;
        }
        if is_instance(world, current) {
            inside_instance = true;
        }
    }
    false
}

/// The scene entity a click on `entity` selects: `entity`, or the instance it was spawned by.
pub fn selectable(world: &World, entity: Entity) -> Option<Entity> {
    let root = world.get_resource::<EditedScene>()?.root;
    let mut chain = Vec::new();
    let mut current = entity;
    loop {
        chain.push(current);
        let parent = world.get::<ChildOf>(current)?.parent();
        if parent == root {
            break;
        }
        current = parent;
    }
    // The outermost instance on the way down from the root wins.
    chain
        .iter()
        .rev()
        .copied()
        .find(|entity| is_instance(world, *entity))
        .or(Some(entity))
}

/// The scene's entities, outermost first, without the descendants of instances.
pub fn scene_entities(world: &World) -> Vec<Entity> {
    let Some(scene) = world.get_resource::<EditedScene>() else {
        return Vec::new();
    };
    let top: Vec<Entity> = world
        .get::<Children>(scene.root)
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    history::with_descendants(world, &top)
}

/// The scene as a scene file would hold it, and the short names of the components it had to leave
/// out because they can't be saved.
pub fn to_dynamic_world(world: &World) -> (DynamicWorld, Vec<String>) {
    let root = world.resource::<EditedScene>().root;
    let entities = scene_entities(world);
    let registry = world.resource::<AppTypeRegistry>().read();
    let mut dynamic = DynamicWorldBuilder::from_world(world, &registry)
        .with_component_filter(save_filter())
        .deny_all_resources()
        .extract_entities(entities.iter().copied())
        .build();
    // Keep the hierarchy's order, which the file's order gives back when it is opened.
    let position: EntityHashMap<usize> = entities
        .iter()
        .enumerate()
        .map(|(index, entity)| (*entity, index))
        .collect();
    dynamic
        .entities
        .sort_by_key(|entity| position.get(&entity.entity).copied().unwrap_or(usize::MAX));

    let mut left_out = Vec::new();
    for entity in &mut dynamic.entities {
        let shape = world.get::<ShapeMesh>(entity.entity).is_some();
        let parked = world.get::<ParkedCamera>(entity.entity).copied();
        entity.components.retain_mut(|component| {
            let Some(info) = component.get_represented_type_info() else {
                return true;
            };
            let path = info.type_path();
            // Top-level entities are spawned under the game's own root.
            if component.represents::<ChildOf>()
                && let Some(child_of) = ChildOf::from_reflect(component.as_partial_reflect())
                && child_of.parent() == root
            {
                return false;
            }
            // A shape makes its mesh and material when it is spawned.
            if shape
                && (path == "bevy_mesh::components::Mesh3d"
                    || path.starts_with("bevy_pbr::mesh_material::MeshMaterial3d<"))
            {
                return false;
            }
            if let Some(parked) = parked
                && component.represents::<Camera>()
                && let Some(mut camera) = Camera::from_reflect(component.as_partial_reflect())
            {
                camera.is_active = parked.active;
                *component = Box::new(camera);
            }
            let saved = serializes(component.as_partial_reflect(), &registry);
            if !saved && !is_default(component.as_partial_reflect(), &registry) {
                left_out.push(String::from(info.type_path_table().short_path()));
            }
            saved
        });
    }
    left_out.sort();
    left_out.dedup();
    (dynamic, left_out)
}

/// A saved scene: its file, and the components that couldn't be saved.
#[derive(Debug, Clone)]
pub struct Saved {
    /// The file, relative to the assets folder.
    pub path: PathBuf,
    /// The short names of the components left out of the file.
    pub left_out: Vec<String>,
}

/// Saves the scene to its file.
pub fn save(world: &mut World) -> Result<Saved, String> {
    let (dynamic, left_out) = to_dynamic_world(world);
    let text = {
        let registry = world.resource::<AppTypeRegistry>().read();
        dynamic
            .serialize(&registry)
            .map_err(|error| format!("could not write the scene: {error}"))?
    };
    let path = world.resource::<EditedScene>().path.clone();
    let file = world.resource::<EditorProject>().asset_file(&path);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|error| format!("could not create {}: {error}", dir.display()))?;
    }
    std::fs::write(&file, text)
        .map_err(|error| format!("could not save {}: {error}", file.display()))?;
    world.resource_mut::<EditedScene>().dirty = false;
    Ok(Saved { path, left_out })
}

/// Saves the scene, reporting the outcome in the status bar.
pub fn save_and_report(world: &mut World) -> bool {
    match save(world) {
        Ok(saved) => {
            let path = asset_path_string(&saved.path);
            if saved.left_out.is_empty() {
                Status::info(world, format!("Saved {path}"));
            } else {
                Status::error(
                    world,
                    format!(
                        "Saved {path}, without what can't be saved: {}",
                        saved.left_out.join(", ")
                    ),
                );
            }
            true
        }
        Err(error) => {
            Status::error(world, error);
            false
        }
    }
}

/// Replaces the scene with the starter scene: ground, a cube, a light and a camera. It will be
/// saved to `path`, relative to the assets folder.
pub fn new_scene(world: &mut World, path: PathBuf) {
    clear(world);
    let root = world.resource::<EditedScene>().root;
    world.spawn((
        Name::new("Ground"),
        ShapeMesh::new(Shape3d::Plane {
            size: Vec2::splat(10.0),
        })
        .with_color(Color::srgb(0.3, 0.5, 0.3)),
        ChildOf(root),
    ));
    world.spawn((
        Name::new("Cube"),
        ShapeMesh::new(Shape3d::Cuboid { size: Vec3::ONE })
            .with_color(Color::srgb_u8(124, 144, 255)),
        Transform::from_xyz(0.0, 0.5, 0.0),
        ChildOf(root),
    ));
    world.spawn((
        Name::new("Sun"),
        DirectionalLight {
            shadow_maps_enabled: true,
            ..Default::default()
        },
        Transform::from_rotation(Quat::from_euler(bevy_math::EulerRot::YXZ, 0.6, -0.9, 0.0)),
        ChildOf(root),
    ));
    world.spawn((
        Name::new("Camera"),
        Camera3d::default(),
        Transform::from_xyz(-4.0, 4.0, 8.0).looking_at(Vec3::ZERO, Vec3::Y),
        ChildOf(root),
    ));
    let mut scene = world.resource_mut::<EditedScene>();
    scene.path = path;
    scene.dirty = true;
}

/// Starts opening the scene file at `path`, relative to the assets folder. It replaces the scene
/// once it has loaded.
pub fn open(world: &mut World, path: PathBuf) {
    let handle = world
        .resource::<AssetServer>()
        .load::<DynamicWorld>(asset_path_string(&path));
    let mut scene = world.resource_mut::<EditedScene>();
    scene.path = path;
    scene.loading = Some(handle);
}

/// Despawns the scene's entities, and forgets the selection and the undo history.
fn clear(world: &mut World) {
    let root = world.resource::<EditedScene>().root;
    let children: Vec<Entity> = world
        .get::<Children>(root)
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    for child in children {
        if let Ok(entity) = world.get_entity_mut(child) {
            entity.despawn();
        }
    }
    world.resource_mut::<InspectorSelection>().0 = None;
    history::clear(world);
}

/// Spawns a scene file that has loaded into the scene.
fn finish_opening(world: &mut World) {
    let Some(handle) = world.resource::<EditedScene>().loading.clone() else {
        return;
    };
    let state = world.resource::<AssetServer>().load_state(&handle);
    match state {
        LoadState::Loaded => {}
        LoadState::Failed(error) => {
            world.resource_mut::<EditedScene>().loading = None;
            Status::error(world, format!("Could not open the scene: {error}"));
            return;
        }
        _ => return,
    }

    clear(world);
    let root = world.resource::<EditedScene>().root;
    let registry = world.resource::<AppTypeRegistry>().clone();
    let mut map = EntityHashMap::default();
    let mut order = Vec::new();
    let written = world.resource_scope(
        |world, worlds: bevy_ecs::world::Mut<Assets<DynamicWorld>>| {
            let Some(dynamic) = worlds.get(&handle) else {
                return Err(String::from("the scene file is empty"));
            };
            order = dynamic
                .entities
                .iter()
                .map(|entity| entity.entity)
                .collect();
            dynamic
                .write_to_world_with(world, &mut map, &registry.read())
                .map_err(|error| format!("{error}"))
        },
    );
    let name = {
        let mut scene = world.resource_mut::<EditedScene>();
        // Drop the handle so the asset is freed and the next open reads the file again.
        scene.loading = None;
        scene.dirty = false;
        scene.file_name()
    };
    if let Err(error) = written {
        Status::error(world, format!("Could not open {name}: {error}"));
        return;
    }
    // Top-level entities go under the root in the file's order.
    for entity in order.iter().filter_map(|entity| map.get(entity).copied()) {
        if world.get_entity(entity).is_ok() && world.get::<ChildOf>(entity).is_none() {
            world.entity_mut(entity).insert(ChildOf(root));
        }
    }
    Status::info(world, format!("Opened {name}"));
}

/// Keeps the scene's cameras from rendering over the viewport.
fn park_scene_cameras(
    cameras: Query<(Entity, &Camera), (Without<ParkedCamera>, With<ChildOf>)>,
    world_scene: Option<Res<EditedScene>>,
    parents: Query<&ChildOf>,
    mut commands: Commands,
) {
    let Some(scene) = world_scene else {
        return;
    };
    for (entity, camera) in &cameras {
        let in_scene = parents
            .iter_ancestors(entity)
            .any(|ancestor| ancestor == scene.root);
        if !in_scene {
            continue;
        }
        commands.entity(entity).insert(ParkedCamera {
            active: camera.is_active,
        });
        commands.queue(move |world: &mut World| {
            if let Some(mut camera) = world.get_mut::<Camera>(entity) {
                camera.is_active = false;
            }
        });
    }
}

fn setup(world: &mut World) {
    let root = world
        .spawn((
            Name::new("Scene"),
            EditorSceneRoot,
            Transform::default(),
            Visibility::default(),
        ))
        .id();
    let path = world.resource::<EditorProject>().scene.clone();
    world.insert_resource(EditedScene {
        root,
        path: path.clone(),
        dirty: false,
        loading: None,
    });
    world.resource_mut::<InspectorTreeRoot>().0 = Some(root);

    let file = world.resource::<EditorProject>().asset_file(&path);
    if file.is_file() {
        open(world, path);
    } else {
        new_scene(world, path);
        Status::info(world, "New scene; Ctrl+S saves it");
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.register_type::<ParkedCamera>()
        .add_systems(Startup, setup)
        .add_systems(PreUpdate, (finish_opening, park_scene_cameras));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instances_keep_their_spawned_children_out_of_the_scene() {
        let mut world = World::new();
        let root = world.spawn_empty().id();
        world.insert_resource(EditedScene {
            root,
            path: PathBuf::from("scenes/a.scn.ron"),
            dirty: false,
            loading: None,
        });
        let plain = world.spawn(ChildOf(root)).id();
        let instance = world
            .spawn((WorldAssetRoot(Handle::default()), ChildOf(root)))
            .id();
        let spawned = world.spawn(ChildOf(instance)).id();
        let deep = world.spawn(ChildOf(spawned)).id();

        assert!(in_scene(&world, plain));
        assert!(in_scene(&world, instance));
        assert!(!in_scene(&world, spawned));
        assert_eq!(selectable(&world, deep), Some(instance));
        assert_eq!(selectable(&world, plain), Some(plain));
        assert!(!in_scene(&world, root));
        assert_eq!(selectable(&world, root), None);
    }
}
