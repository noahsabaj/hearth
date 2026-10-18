//! Undo and redo.
//!
//! Every edit is recorded as snapshots of the entities it touched, taken before and after it, so
//! any change to them, however it was made, can be undone: field edits in the inspector, gizmo
//! drags, renames, added and removed components, new, deleted and reparented entities.
//!
//! Record an edit made in one go with [`record`]. For one made over many frames, such as a drag,
//! call [`begin`] before it and [`History::touch`] while it goes on; it is committed once it stops
//! changing for [`MERGE_WINDOW`], or when another edit or an undo begins.

use alloc::{string::String, vec::Vec};
use core::time::Duration;

use hearth_app::{App, Last};
use hearth_ecs::{
    entity::{Entity, EntityHashMap, EntityHashSet},
    hierarchy::{ChildOf, Children},
    reflect::{AppTypeRegistry, ReflectComponent},
    relationship::RelationshipHookMode,
    resource::Resource,
    world::World,
};
use hearth_gizmos::transform_gizmo::TransformGizmoFocus;
use hearth_platform::time::Instant;
use hearth_reflect::{FromReflect, PartialReflect, TypeRegistry};
use hearth_world_serialization::{DynamicWorld, DynamicWorldBuilder, WorldFilter};

use crate::scene::EditedScene;

/// How long an edit made over many frames may pause and still count as the same edit.
pub const MERGE_WINDOW: Duration = Duration::from_millis(600);

/// The most edits kept for undo.
pub const MAX_EDITS: usize = 200;

/// One recorded edit.
pub struct Edit {
    /// What the edit did, for the Edit menu and the status bar.
    pub label: String,
    before: Snapshot,
    after: Snapshot,
}

/// The state of some entities: which of them existed, and their components.
struct Snapshot {
    entities: Vec<Entity>,
    world: DynamicWorld,
}

/// An edit that has begun and not been committed yet.
struct Pending {
    label: String,
    entities: Vec<Entity>,
    before: Snapshot,
    touched: Instant,
}

/// The undo and redo stacks.
#[derive(Resource, Default)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    pending: Option<Pending>,
    /// Entities that were despawned and spawned again by an undo or redo, under their new ids.
    respawned: EntityHashMap<Entity>,
}

impl History {
    /// Whether there is an edit to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.pending.is_some()
    }

    /// Whether there is an edit to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The label of the edit [`undo`] would undo.
    pub fn undo_label(&self) -> Option<&str> {
        match &self.pending {
            Some(pending) => Some(&pending.label),
            None => self.undo.last().map(|edit| edit.label.as_str()),
        }
    }

    /// The label of the edit [`redo`] would redo.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|edit| edit.label.as_str())
    }

    /// Marks the pending edit as still going on, so it isn't committed yet.
    pub fn touch(&mut self) {
        if let Some(pending) = &mut self.pending {
            pending.touched = Instant::now();
        }
    }

    /// Whether an edit to `entities` labelled `label` is pending, so a new change to them can join it.
    pub fn is_pending(&self, label: &str, entities: &[Entity]) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.label == label && pending.entities == entities)
    }

    /// The id an entity recorded as `entity` has now.
    fn current(&self, entity: Entity) -> Entity {
        let mut entity = entity;
        // Respawned entities can be respawned again; follow the chain.
        for _ in 0..64 {
            match self.respawned.get(&entity) {
                Some(next) if *next != entity => entity = *next,
                _ => break,
            }
        }
        entity
    }
}

/// The marker components an edit never records: they belong to the editor, not the scene.
pub(crate) fn snapshot_filter() -> WorldFilter {
    crate::scene::deny_computed(
        WorldFilter::default()
            .deny::<Children>()
            .deny::<TransformGizmoFocus>(),
    )
}

/// Records `entities`, and the descendants they own, before and after `edit` runs, as one edit.
///
/// Entities `edit` spawns should be returned, so undo despawns them.
pub fn record(
    world: &mut World,
    label: impl Into<String>,
    entities: &[Entity],
    edit: impl FnOnce(&mut World) -> Vec<Entity>,
) {
    commit(world);
    let entities = with_descendants(world, entities);
    let before = snapshot(world, &entities);
    let spawned = edit(world);
    let mut after_entities = entities;
    for entity in with_descendants(world, &spawned) {
        if !after_entities.contains(&entity) {
            after_entities.push(entity);
        }
    }
    let after = snapshot(world, &after_entities);
    push(
        world,
        Edit {
            label: label.into(),
            before,
            after,
        },
    );
}

/// Begins an edit of `entities` that goes on over several frames.
pub fn begin(world: &mut World, label: impl Into<String>, entities: &[Entity]) {
    commit(world);
    let entities = with_descendants(world, entities);
    let before = snapshot(world, &entities);
    world.resource_mut::<History>().pending = Some(Pending {
        label: label.into(),
        entities,
        before,
        touched: Instant::now(),
    });
}

/// Commits the pending edit, if there is one.
pub fn commit(world: &mut World) {
    let Some(pending) = world.resource_mut::<History>().pending.take() else {
        return;
    };
    let after = snapshot(world, &pending.entities);
    push(
        world,
        Edit {
            label: pending.label,
            before: pending.before,
            after,
        },
    );
}

/// Undoes the latest edit. Returns its label, or `None` when there was nothing to undo.
pub fn undo(world: &mut World) -> Option<String> {
    commit(world);
    let edit = world.resource_mut::<History>().undo.pop()?;
    restore(world, &edit.before, &edit.after.entities);
    let label = edit.label.clone();
    world.resource_mut::<History>().redo.push(edit);
    mark_dirty(world);
    Some(label)
}

/// Redoes the latest undone edit. Returns its label, or `None` when there was nothing to redo.
pub fn redo(world: &mut World) -> Option<String> {
    commit(world);
    let edit = world.resource_mut::<History>().redo.pop()?;
    restore(world, &edit.after, &edit.before.entities);
    let label = edit.label.clone();
    world.resource_mut::<History>().undo.push(edit);
    mark_dirty(world);
    Some(label)
}

/// Forgets every edit, for when the scene is replaced.
pub fn clear(world: &mut World) {
    *world.resource_mut::<History>() = History::default();
}

fn push(world: &mut World, edit: Edit) {
    if same_state(&edit.before, &edit.after) {
        return;
    }
    let mut history = world.resource_mut::<History>();
    history.redo.clear();
    history.undo.push(edit);
    if history.undo.len() > MAX_EDITS {
        history.undo.remove(0);
    }
    mark_dirty(world);
}

fn mark_dirty(world: &mut World) {
    if let Some(mut scene) = world.get_resource_mut::<EditedScene>() {
        scene.dirty = true;
    }
}

/// `entities`, their current ids, and the descendants they own. Descendants of a glTF or scene
/// instance are left out: the instance spawns them again from its asset.
pub(crate) fn with_descendants(world: &World, entities: &[Entity]) -> Vec<Entity> {
    let mut seen = EntityHashSet::default();
    let mut out = Vec::new();
    let mut stack: Vec<Entity> = entities.iter().rev().copied().collect();
    while let Some(entity) = stack.pop() {
        if !seen.insert(entity) {
            continue;
        }
        out.push(entity);
        if crate::scene::is_instance(world, entity) {
            continue;
        }
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter().rev());
        }
    }
    out
}

fn snapshot(world: &World, entities: &[Entity]) -> Snapshot {
    let registry = world.resource::<AppTypeRegistry>().read();
    let alive: Vec<Entity> = entities
        .iter()
        .copied()
        .filter(|entity| world.get_entity(*entity).is_ok())
        .collect();
    let dynamic = DynamicWorldBuilder::from_world(world, &registry)
        .with_component_filter(snapshot_filter())
        .deny_all_resources()
        .extract_entities(alive.iter().copied())
        .build();
    Snapshot {
        entities: alive,
        world: dynamic,
    }
}

/// Whether two snapshots describe the same state, so recording them would make an empty edit.
fn same_state(a: &Snapshot, b: &Snapshot) -> bool {
    if a.entities != b.entities || a.world.entities.len() != b.world.entities.len() {
        return false;
    }
    a.world
        .entities
        .iter()
        .zip(&b.world.entities)
        .all(|(a, b)| {
            a.entity == b.entity
                && a.components.len() == b.components.len()
                && a.components.iter().zip(&b.components).all(|(a, b)| {
                    a.reflect_partial_eq(b.as_partial_reflect())
                        .unwrap_or(false)
                        || is_derived(a.as_partial_reflect())
                })
        })
}

/// Components whose changes don't make an edit by themselves: they follow from others.
fn is_derived(component: &dyn PartialReflect) -> bool {
    let Some(info) = component.get_represented_type_info() else {
        return false;
    };
    matches!(
        info.type_path(),
        "hearth_transform::components::global_transform::GlobalTransform"
            | "hearth_camera::visibility::InheritedVisibility"
            | "hearth_camera::visibility::ViewVisibility"
            | "hearth_camera::primitives::Aabb"
    )
}

/// Puts the entities of `snapshot` back the way it recorded them, and despawns those of `other`
/// that it doesn't have.
fn restore(world: &mut World, snapshot: &Snapshot, other: &[Entity]) {
    // Despawn entities the snapshot doesn't have, children before parents.
    let gone: Vec<Entity> = other
        .iter()
        .filter(|entity| !snapshot.entities.contains(entity))
        .copied()
        .collect();
    for recorded in gone.iter().rev() {
        let current = world.resource::<History>().current(*recorded);
        if let Ok(entity) = world.get_entity_mut(current) {
            entity.despawn();
        }
    }

    // Every living entity maps to itself; recorded entities map to their current ids, and the
    // ones that no longer exist are spawned again.
    let mut map = EntityHashMap::default();
    for entity in world.iter_entities() {
        map.insert(entity.id(), entity.id());
    }
    for recorded in &snapshot.entities {
        let current = world.resource::<History>().current(*recorded);
        if world.get_entity(current).is_ok() {
            map.insert(*recorded, current);
        } else {
            let respawned = world.spawn_empty().id();
            map.insert(*recorded, respawned);
            world
                .resource_mut::<History>()
                .respawned
                .insert(*recorded, respawned);
        }
    }

    let registry = world.resource::<AppTypeRegistry>().clone();
    let registry = registry.read();
    for dynamic in &snapshot.world.entities {
        let entity = map[&dynamic.entity];
        restore_entity(world, &registry, entity, &dynamic.components, &mut map);
    }
}

/// Makes `entity`'s recordable components equal to `components`: removes the others, and replaces
/// those that differ.
fn restore_entity(
    world: &mut World,
    registry: &TypeRegistry,
    entity: Entity,
    components: &[Box<dyn PartialReflect>],
    map: &mut EntityHashMap<Entity>,
) {
    let filter = snapshot_filter();
    let recorded: Vec<core::any::TypeId> = components
        .iter()
        .filter_map(|component| component.get_represented_type_info())
        .map(hearth_reflect::TypeInfo::type_id)
        .collect();

    // Remove what the snapshot doesn't have.
    let present: Vec<(core::any::TypeId, ReflectComponent)> = {
        let Ok(entity_ref) = world.get_entity(entity) else {
            return;
        };
        entity_ref
            .archetype()
            .components()
            .iter()
            .filter_map(|id| world.components().get_info(*id))
            .filter_map(hearth_ecs::component::ComponentInfo::type_id)
            .filter(|type_id| filter.is_allowed_by_id(*type_id))
            .filter_map(|type_id| {
                let reflect = registry.get(type_id)?.data::<ReflectComponent>()?.clone();
                Some((type_id, reflect))
            })
            .collect()
    };
    for (type_id, reflect) in &present {
        if !recorded.contains(type_id) {
            reflect.remove(&mut world.entity_mut(entity));
        }
    }

    // Put back what differs.
    for component in components {
        let Some(info) = component.get_represented_type_info() else {
            continue;
        };
        let Some(reflect) = registry
            .get(info.type_id())
            .and_then(|registration| registration.data::<ReflectComponent>())
        else {
            continue;
        };
        if info.type_id() == core::any::TypeId::of::<ChildOf>() {
            // Reinserting the same parent would move the entity to the end of its siblings.
            let parent = ChildOf::from_reflect(component.as_partial_reflect()).map(|child_of| {
                map.get(&child_of.parent())
                    .copied()
                    .unwrap_or(child_of.parent())
            });
            if parent.is_some() && world.get::<ChildOf>(entity).map(ChildOf::parent) == parent {
                continue;
            }
        } else if let Some(current) = reflect.reflect(world.entity(entity))
            && current
                .reflect_partial_eq(component.as_partial_reflect())
                .unwrap_or(false)
        {
            continue;
        }
        let mut entity_mut = world.entity_mut(entity);
        reflect.remove(&mut entity_mut);
        reflect.apply_or_insert_mapped(
            &mut entity_mut,
            component.as_partial_reflect(),
            registry,
            map,
            RelationshipHookMode::Run,
        );
    }
}

/// Commits a pending edit that has stopped changing.
fn commit_settled(world: &mut World) {
    let settled = world
        .resource::<History>()
        .pending
        .as_ref()
        .is_some_and(|pending| pending.touched.elapsed() >= MERGE_WINDOW);
    if settled {
        commit(world);
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<History>()
        .add_systems(Last, commit_settled);
}

#[cfg(test)]
mod tests {
    use hearth_ecs::{name::Name, reflect::AppTypeRegistry};
    use hearth_transform::components::Transform;

    use super::*;

    fn world() -> World {
        let mut world = World::new();
        let registry = AppTypeRegistry::default();
        {
            let mut registry = registry.write();
            registry.register::<Name>();
            registry.register::<Transform>();
            registry.register::<ChildOf>();
            registry.register::<Children>();
        }
        world.insert_resource(registry);
        world.init_resource::<History>();
        world
    }

    #[test]
    fn undo_and_redo_a_field_edit() {
        let mut world = world();
        let entity = world.spawn((Name::new("Cube"), Transform::default())).id();
        record(&mut world, "Move", &[entity], |world| {
            world.get_mut::<Transform>(entity).unwrap().translation.x = 3.0;
            Vec::new()
        });
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 3.0);

        assert_eq!(undo(&mut world).as_deref(), Some("Move"));
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 0.0);
        assert_eq!(redo(&mut world).as_deref(), Some("Move"));
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 3.0);
    }

    #[test]
    fn undo_brings_back_a_deleted_hierarchy() {
        let mut world = world();
        let parent = world.spawn(Name::new("Parent")).id();
        let child = world.spawn((Name::new("Child"), ChildOf(parent))).id();
        record(&mut world, "Delete", &[parent], |world| {
            world.entity_mut(parent).despawn();
            Vec::new()
        });
        assert!(world.get_entity(child).is_err());

        undo(&mut world);
        let mut names = world.query::<(&Name, Option<&ChildOf>)>();
        let restored: Vec<_> = names
            .iter(&world)
            .map(|(name, child_of)| (name.as_str().to_string(), child_of.map(ChildOf::parent)))
            .collect();
        assert_eq!(restored.len(), 2);
        let parent_now = world.resource::<History>().current(parent);
        assert!(restored.contains(&("Child".into(), Some(parent_now))));

        // Redo deletes them again, under their new ids.
        redo(&mut world);
        assert_eq!(names.iter(&world).count(), 0);
        undo(&mut world);
        assert_eq!(names.iter(&world).count(), 2);
    }

    #[test]
    fn undo_removes_what_an_edit_spawned_and_restores_removed_components() {
        let mut world = world();
        let entity = world.spawn(Name::new("A")).id();
        let mut spawned = Entity::PLACEHOLDER;
        record(&mut world, "Add", &[entity], |world| {
            world.entity_mut(entity).insert(Transform::default());
            spawned = world.spawn((Name::new("B"), ChildOf(entity))).id();
            alloc::vec![spawned]
        });
        undo(&mut world);
        assert!(world.get_entity(spawned).is_err());
        assert!(world.get::<Transform>(entity).is_none());
        redo(&mut world);
        assert!(world.get::<Transform>(entity).is_some());
        assert_eq!(world.query::<&Name>().iter(&world).count(), 2);
    }

    #[test]
    fn pending_edits_commit_as_one() {
        let mut world = world();
        let entity = world.spawn(Transform::default()).id();
        begin(&mut world, "Drag", &[entity]);
        for x in 1..=5 {
            world.get_mut::<Transform>(entity).unwrap().translation.x = x as f32;
            world.resource_mut::<History>().touch();
        }
        commit(&mut world);
        assert_eq!(world.resource::<History>().undo.len(), 1);
        undo(&mut world);
        assert_eq!(world.get::<Transform>(entity).unwrap().translation.x, 0.0);
    }

    #[test]
    fn edits_that_change_nothing_are_dropped() {
        let mut world = world();
        let entity = world.spawn(Transform::default()).id();
        record(&mut world, "Nothing", &[entity], |_| Vec::new());
        assert!(!world.resource::<History>().can_undo());
    }
}
