//! The edits the editor's menus, panels and shortcuts make. Each is recorded for undo.

use alloc::{
    borrow::ToOwned,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::any::TypeId;
use std::path::Path;

use hearth_asset::AssetServer;
use hearth_camera::{
    primitives::Aabb,
    visibility::{InheritedVisibility, ViewVisibility, Visibility, VisibilityClass},
    Camera, Camera3d,
};
use hearth_ecs::{
    entity::{Entity, EntityHashMap, EntityHashSet},
    hierarchy::{ChildOf, Children},
    name::Name,
    query::With,
    reflect::{AppTypeRegistry, ReflectComponent, ReflectFromWorld},
    system::SystemState,
    world::World,
};
use hearth_gizmos::transform_gizmo::TransformGizmoFocus;
use hearth_gltf::GltfAssetLabel;
use hearth_inspector::InspectorSelection;
use hearth_light::{DirectionalLight, PointLight, SpotLight};
use hearth_math::{Quat, Vec2, Vec3};
use hearth_mesh::Mesh3d;
use hearth_pbr::{MeshMaterial3d, Shape3d, ShapeMesh, StandardMaterial};
use hearth_picking::{
    hover::PickingInteraction,
    mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings},
};
use hearth_reflect::{prelude::ReflectDefault, TypeRegistry};
use hearth_transform::components::{GlobalTransform, Transform, TransformTreeChanged};
use hearth_world_serialization::{DynamicWorldBuilder, DynamicWorldRoot, WorldAssetRoot};

use crate::{
    history,
    project::asset_path_string,
    scene::{self, EditedScene, ParkedCamera},
    status::Status,
    viewport::EditorCamera,
};

/// Something the Add menu can put in the scene.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NewObject {
    /// An entity with only a transform.
    Empty,
    /// A [`ShapeMesh`] of this shape.
    Shape(Shape3d),
    /// A point light.
    PointLight,
    /// A spot light.
    SpotLight,
    /// A directional light, like the sun.
    DirectionalLight,
    /// A camera.
    Camera,
}

impl NewObject {
    /// Everything the Add menu offers, with its menu label.
    pub const ALL: [(&'static str, NewObject); 12] = [
        ("Empty", NewObject::Empty),
        (
            "Cube",
            NewObject::Shape(Shape3d::Cuboid { size: Vec3::ONE }),
        ),
        ("Sphere", NewObject::Shape(Shape3d::Sphere { radius: 0.5 })),
        (
            "Plane",
            NewObject::Shape(Shape3d::Plane {
                size: Vec2::splat(4.0),
            }),
        ),
        (
            "Cylinder",
            NewObject::Shape(Shape3d::Cylinder {
                radius: 0.5,
                height: 1.0,
            }),
        ),
        (
            "Capsule",
            NewObject::Shape(Shape3d::Capsule {
                radius: 0.5,
                length: 1.0,
            }),
        ),
        (
            "Cone",
            NewObject::Shape(Shape3d::Cone {
                radius: 0.5,
                height: 1.0,
            }),
        ),
        (
            "Torus",
            NewObject::Shape(Shape3d::Torus {
                minor_radius: 0.15,
                major_radius: 0.5,
            }),
        ),
        ("Point Light", NewObject::PointLight),
        ("Spot Light", NewObject::SpotLight),
        ("Directional Light", NewObject::DirectionalLight),
        ("Camera", NewObject::Camera),
    ];

    fn name(&self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, object)| object == self)
            .map(|(name, _)| *name)
            .unwrap_or("Object")
    }
}

/// Where new objects go: on the scene's surface at the middle of the viewport, else on the ground
/// a few meters ahead, else a few meters in front of the camera. Returns the point and the
/// surface's normal there.
fn placement(world: &mut World) -> (Vec3, Vec3) {
    let mut cameras = world.query_filtered::<(&Camera, &GlobalTransform), With<EditorCamera>>();
    let Ok((camera, transform)) = cameras.single(world) else {
        return (Vec3::ZERO, Vec3::Y);
    };
    // Rays start from window coordinates; the viewport's middle is offset by its corner.
    let ray = camera
        .logical_viewport_rect()
        .and_then(|rect| camera.viewport_to_world(transform, rect.center()).ok());
    let origin = transform.translation();
    let forward = ray.map_or(*transform.forward(), |ray| *ray.direction);

    if let Some(ray) = ray {
        let root = world.resource::<EditedScene>().root;
        let mut candidates = EntityHashSet::default();
        let mut stack = alloc::vec![root];
        while let Some(entity) = stack.pop() {
            if let Some(children) = world.get::<Children>(entity) {
                candidates.extend(children.iter());
                stack.extend(children.iter());
            }
        }
        let filter = |entity: Entity| candidates.contains(&entity);
        let settings = MeshRayCastSettings::default()
            .with_filter(&filter)
            .always_early_exit();
        let mut state = SystemState::<MeshRayCast>::new(world);
        if let Ok(mut ray_cast) = state.get_mut(world)
            && let Some((_, hit)) = ray_cast.cast_ray(ray, &settings).first()
            && hit.distance < 200.0
        {
            return (hit.point, hit.normal.normalize_or(Vec3::Y));
        }
    }

    if forward.y < -0.05 {
        let distance = -origin.y / forward.y;
        if (0.0..40.0).contains(&distance) {
            return (origin + forward * distance, Vec3::Y);
        }
    }
    (origin + forward * 6.0, Vec3::Y)
}

/// A name for a new entity that no top-level entity has yet: `Cube`, `Cube (1)`, ...
fn unique_name(world: &mut World, base: &str) -> String {
    let taken: Vec<String> = scene::scene_entities(world)
        .into_iter()
        .filter_map(|entity| {
            world
                .get::<Name>(entity)
                .map(|name| name.as_str().to_owned())
        })
        .collect();
    if !taken.iter().any(|name| name == base) {
        return base.to_owned();
    }
    (1..)
        .map(|n| format!("{base} ({n})"))
        .find(|name| !taken.contains(name))
        .unwrap_or_else(|| base.to_owned())
}

/// The base of a duplicate's name: `Cube (2)` becomes `Cube`.
fn base_name(name: &str) -> &str {
    if let Some(open) = name.rfind(" (")
        && name.ends_with(')')
        && name[open + 2..name.len() - 1].parse::<u32>().is_ok()
    {
        return &name[..open];
    }
    name
}

/// Adds `object` to the scene in front of the camera and selects it.
pub fn add_object(world: &mut World, object: NewObject) -> Entity {
    let root = world.resource::<EditedScene>().root;
    let (at, normal) = placement(world);
    let name = unique_name(world, object.name());
    let mut spawned = Entity::PLACEHOLDER;
    history::record(world, format!("Add {name}"), &[], |world| {
        let mut entity = world.spawn((Name::new(name), ChildOf(root)));
        match object {
            NewObject::Empty => {
                entity.insert((Transform::from_translation(at), Visibility::default()));
            }
            NewObject::Shape(shape) => {
                let lift = match shape {
                    Shape3d::Cuboid { size } => size.y / 2.0,
                    Shape3d::Sphere { radius } => radius,
                    Shape3d::Cylinder { height, .. } | Shape3d::Cone { height, .. } => height / 2.0,
                    Shape3d::Capsule { radius, length } => radius + length / 2.0,
                    Shape3d::Torus { minor_radius, .. } => minor_radius,
                    // Just above the surface, so the two don't flicker through each other.
                    Shape3d::Plane { .. } => 0.005,
                };
                entity.insert((
                    ShapeMesh::new(shape),
                    Transform::from_translation(at + normal * lift),
                ));
            }
            NewObject::PointLight => {
                entity.insert((
                    PointLight {
                        shadow_maps_enabled: true,
                        ..Default::default()
                    },
                    Transform::from_translation(at + Vec3::Y * 2.0),
                ));
            }
            NewObject::SpotLight => {
                entity.insert((
                    SpotLight {
                        shadow_maps_enabled: true,
                        ..Default::default()
                    },
                    Transform::from_translation(at + Vec3::Y * 3.0).looking_at(at, Vec3::Z),
                ));
            }
            NewObject::DirectionalLight => {
                entity.insert((
                    DirectionalLight {
                        shadow_maps_enabled: true,
                        ..Default::default()
                    },
                    Transform::from_rotation(Quat::from_euler(
                        hearth_math::EulerRot::YXZ,
                        0.6,
                        -0.9,
                        0.0,
                    )),
                ));
            }
            NewObject::Camera => {
                entity.insert((
                    Camera3d::default(),
                    Transform::from_translation(at + Vec3::new(0.0, 2.0, 6.0))
                        .looking_at(at, Vec3::Y),
                ));
            }
        }
        spawned = entity.id();
        alloc::vec![spawned]
    });
    select(world, Some(spawned));
    spawned
}

/// Adds the glTF model or scene file at `path`, relative to the assets folder, as an instance.
pub fn add_asset(world: &mut World, path: &Path) -> Option<Entity> {
    let root = world.resource::<EditedScene>().root;
    let text = asset_path_string(path);
    let lower = text.to_lowercase();
    let (at, _) = placement(world);
    let stem = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| text.clone());
    let stem = stem.split('.').next().unwrap_or(&stem).to_owned();
    let name = unique_name(world, &stem);
    let mut spawned = None;
    history::record(world, format!("Add {name}"), &[], |world| {
        let server = world.resource::<AssetServer>().clone();
        let mut entity = world.spawn((
            Name::new(name),
            ChildOf(root),
            Transform::from_translation(at),
        ));
        if lower.ends_with(".gltf") || lower.ends_with(".glb") {
            entity.insert(WorldAssetRoot(
                server.load(GltfAssetLabel::Scene(0).from_asset(text.clone())),
            ));
        } else if lower.ends_with(".scn.ron") || lower.ends_with(".scn") {
            entity.insert(DynamicWorldRoot(server.load(text.clone())));
        } else {
            entity.despawn();
            return Vec::new();
        }
        let id = entity.id();
        spawned = Some(id);
        alloc::vec![id]
    });
    match spawned {
        Some(entity) => select(world, Some(entity)),
        None => Status::error(
            world,
            format!("{text} isn't a model or scene the editor can add"),
        ),
    }
    spawned
}

/// Selects `entity`, or nothing.
pub fn select(world: &mut World, entity: Option<Entity>) {
    let mut selection = world.resource_mut::<InspectorSelection>();
    if selection.0 != entity {
        selection.0 = entity;
    }
}

/// The selected scene entity, if the selection is one.
pub fn selected(world: &World) -> Option<Entity> {
    world
        .resource::<InspectorSelection>()
        .0
        .filter(|entity| scene::in_scene(world, *entity))
}

/// Deletes the selection, with its descendants.
pub fn delete_selected(world: &mut World) {
    let Some(entity) = selected(world) else {
        return;
    };
    let label = format!("Delete {}", label_of(world, entity));
    history::record(world, label, &[entity], |world| {
        world.entity_mut(entity).despawn();
        Vec::new()
    });
    select(world, None);
}

/// Duplicates the selection, with its descendants, next to it, and selects the copy.
pub fn duplicate_selected(world: &mut World) {
    let Some(entity) = selected(world) else {
        return;
    };
    let base = world
        .get::<Name>(entity)
        .map(|name| base_name(name.as_str()).to_owned());
    let label = format!("Duplicate {}", label_of(world, entity));
    let mut copy = None;
    history::record(world, label, &[], |world| {
        let originals = history::with_descendants(world, &[entity]);
        let dynamic = {
            let registry = world.resource::<AppTypeRegistry>().read();
            DynamicWorldBuilder::from_world(world, &registry)
                .with_component_filter(history::snapshot_filter())
                .deny_all_resources()
                .extract_entities(originals.iter().copied())
                .build()
        };
        // Everything outside the copied entities, such as their parent, maps to itself.
        let mut map = EntityHashMap::default();
        for other in world.iter_entities() {
            if !originals.contains(&other.id()) {
                map.insert(other.id(), other.id());
            }
        }
        let registry = world.resource::<AppTypeRegistry>().clone();
        if let Err(error) = dynamic.write_to_world_with(world, &mut map, &registry.read()) {
            Status::error(world, format!("Could not duplicate: {error}"));
            return Vec::new();
        }
        let copies: Vec<Entity> = originals
            .iter()
            .filter_map(|original| map.get(original).copied())
            .collect();
        copy = copies.first().copied();
        if let (Some(copy), Some(base)) = (copy, &base) {
            let name = unique_name(world, base);
            world.entity_mut(copy).insert(Name::new(name));
        }
        copies
    });
    select(world, copy);
}

/// Renames `entity`. Typing in the name field makes one edit, not one per key.
pub fn rename(world: &mut World, entity: Entity, name: &str) {
    if world
        .get::<Name>(entity)
        .is_some_and(|current| current.as_str() == name)
    {
        return;
    }
    if !world
        .resource::<history::History>()
        .is_pending("Rename", &[entity])
    {
        history::begin(world, "Rename", &[entity]);
    }
    world.resource_mut::<history::History>().touch();
    if name.is_empty() {
        world.entity_mut(entity).remove::<Name>();
    } else {
        world.entity_mut(entity).insert(Name::new(name.to_owned()));
    }
}

/// Moves `entity` under `parent`, or to the top of the scene, keeping where it is in the world.
pub fn reparent(world: &mut World, entity: Entity, parent: Option<Entity>) {
    let root = world.resource::<EditedScene>().root;
    let parent = parent.unwrap_or(root);
    if parent == entity || is_ancestor(world, entity, parent) {
        Status::error(world, "An entity can't be moved under its own descendant");
        return;
    }
    if world.get::<ChildOf>(entity).map(ChildOf::parent) == Some(parent) {
        return;
    }
    let label = format!("Reparent {}", label_of(world, entity));
    history::record(world, label, &[entity], |world| {
        let global = world.get::<GlobalTransform>(entity).copied();
        let parent_global = world
            .get::<GlobalTransform>(parent)
            .copied()
            .unwrap_or_default();
        world.entity_mut(entity).insert(ChildOf(parent));
        if let Some(global) = global
            && world.get::<Transform>(entity).is_some()
        {
            world
                .entity_mut(entity)
                .insert(global.reparented_to(&parent_global));
        }
        Vec::new()
    });
}

fn is_ancestor(world: &World, ancestor: Entity, entity: Entity) -> bool {
    let mut current = entity;
    while let Some(child_of) = world.get::<ChildOf>(current) {
        current = child_of.parent();
        if current == ancestor {
            return true;
        }
    }
    false
}

/// A component an entity has, as the components panel lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentEntry {
    /// Its short type name.
    pub name: String,
    /// Its full type path.
    pub path: String,
}

/// Components the components panel doesn't list: the editor's, and those the engine maintains.
fn is_hidden_component(type_id: TypeId) -> bool {
    [
        TypeId::of::<Children>(),
        TypeId::of::<ChildOf>(),
        TypeId::of::<GlobalTransform>(),
        TypeId::of::<InheritedVisibility>(),
        TypeId::of::<ViewVisibility>(),
        TypeId::of::<Aabb>(),
        TypeId::of::<TransformGizmoFocus>(),
        TypeId::of::<ParkedCamera>(),
        TypeId::of::<Name>(),
        TypeId::of::<VisibilityClass>(),
        TypeId::of::<TransformTreeChanged>(),
        TypeId::of::<PickingInteraction>(),
    ]
    .contains(&type_id)
}

/// Components a [`ShapeMesh`] makes, so the components panel lists the shape instead.
fn is_made_by_shape(type_id: TypeId) -> bool {
    type_id == TypeId::of::<Mesh3d>() || type_id == TypeId::of::<MeshMaterial3d<StandardMaterial>>()
}

/// The reflected components `entity` has, by short name.
pub fn components_of(world: &World, entity: Entity) -> Vec<ComponentEntry> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let Ok(entity_ref) = world.get_entity(entity) else {
        return Vec::new();
    };
    let shaped = entity_ref.contains::<ShapeMesh>();
    let mut entries: Vec<ComponentEntry> = entity_ref
        .archetype()
        .components()
        .iter()
        .filter_map(|id| world.components().get_info(*id)?.type_id())
        .filter(|type_id| !is_hidden_component(*type_id))
        .filter(|type_id| !(shaped && is_made_by_shape(*type_id)))
        .filter_map(|type_id| {
            let registration = registry.get(type_id)?;
            registration.data::<ReflectComponent>()?;
            let table = registration.type_info().type_path_table();
            Some(ComponentEntry {
                name: table.short_path().to_string(),
                path: table.path().to_string(),
            })
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// The components that can be added to `entity`: registered, constructible, and not on it yet.
pub fn addable_components(world: &World, entity: Entity) -> Vec<ComponentEntry> {
    let registry = world.resource::<AppTypeRegistry>().read();
    let present: Vec<String> = components_of(world, entity)
        .into_iter()
        .map(|entry| entry.path)
        .collect();
    let mut entries: Vec<ComponentEntry> = registry
        .iter()
        .filter(|registration| registration.data::<ReflectComponent>().is_some())
        .filter(|registration| {
            registration.data::<ReflectDefault>().is_some()
                || registration.data::<ReflectFromWorld>().is_some()
        })
        .filter(|registration| !is_hidden_component(registration.type_id()))
        .filter_map(|registration| {
            let table = registration.type_info().type_path_table();
            let path = table.path();
            // The editor's and the inspector's own components don't belong in a scene.
            if path.starts_with("hearth_editor::")
                || path.starts_with("hearth_inspector::")
                || path.starts_with("hearth_ui")
                || path.starts_with("hearth_feathers::")
                || present.iter().any(|present| present == path)
            {
                return None;
            }
            Some(ComponentEntry {
                name: table.short_path().to_string(),
                path: path.to_string(),
            })
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Adds the component with type path `path`, at its default value, to `entity`.
pub fn add_component(world: &mut World, entity: Entity, path: &str) {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let value = {
        let registry = registry.read();
        default_value(world, &registry, path)
    };
    let Some(value) = value else {
        Status::error(world, format!("{path} can't be made with a default value"));
        return;
    };
    let short = short_name(path);
    history::record(world, format!("Add {short}"), &[entity], |world| {
        let registry = registry.read();
        if let Some(reflect) = registry
            .get_with_type_path(path)
            .and_then(|registration| registration.data::<ReflectComponent>())
        {
            reflect.insert(
                &mut world.entity_mut(entity),
                value.as_partial_reflect(),
                &registry,
            );
        }
        Vec::new()
    });
}

fn default_value(
    world: &mut World,
    registry: &TypeRegistry,
    path: &str,
) -> Option<Box<dyn hearth_reflect::Reflect>> {
    let registration = registry.get_with_type_path(path)?;
    if let Some(default) = registration.data::<ReflectDefault>() {
        return Some(default.default());
    }
    registration
        .data::<ReflectFromWorld>()
        .map(|from_world| from_world.from_world(world))
}

/// Removes the component with type path `path` from `entity`.
pub fn remove_component(world: &mut World, entity: Entity, path: &str) {
    let registry = world.resource::<AppTypeRegistry>().clone();
    let short = short_name(path);
    history::record(world, format!("Remove {short}"), &[entity], |world| {
        let registry = registry.read();
        if let Some(reflect) = registry
            .get_with_type_path(path)
            .and_then(|registration| registration.data::<ReflectComponent>())
        {
            reflect.remove(&mut world.entity_mut(entity));
        }
        Vec::new()
    });
}

/// The last segment of a type path, generics included: `hearth_pbr::ShapeMesh` becomes `ShapeMesh`.
pub fn short_name(path: &str) -> &str {
    let end = path.find('<').unwrap_or(path.len());
    let start = path[..end].rfind("::").map_or(0, |at| at + 2);
    &path[start..]
}

fn label_of(world: &World, entity: Entity) -> String {
    world
        .get::<Name>(entity)
        .map(|name| name.as_str().to_owned())
        .unwrap_or_else(|| entity.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(base_name("Cube (2)"), "Cube");
        assert_eq!(base_name("Cube (x)"), "Cube (x)");
        assert_eq!(base_name("Cube"), "Cube");
        assert_eq!(short_name("hearth_pbr::shape::ShapeMesh"), "ShapeMesh");
        assert_eq!(
            short_name("hearth_pbr::MeshMaterial3d<hearth_pbr::StandardMaterial>"),
            "MeshMaterial3d<hearth_pbr::StandardMaterial>"
        );
    }
}
