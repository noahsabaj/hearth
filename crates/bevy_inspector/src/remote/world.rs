//! A [`World`] mirroring the entities of the remote app.
//!
//! Remote entities are spawned at their remote ids, so the entity tree reads the mirror like any
//! other world. Only their [`Name`] and [`ChildOf`] are inserted. The other components the tree
//! labels entities with are recorded on the entity's [`RemoteComponents`]. The selected entity
//! also holds every component the remote app reported for it, read back through the local type
//! registry, in its [`RemoteValues`].
//!
//! Spawning at the remote ids keeps every id, such as the selection or a [`ChildOf`] target, the
//! same in both worlds. [`World::spawn_at`] only accepts the current generation of an index, and a
//! fresh world starts every index at generation 0, so the index is stepped up to the remote
//! generation by spawning and despawning it without freeing. Indices up to the remote ones are
//! reserved, so the allocator never hands them out. A `bevy_ecs` API setting the generation of an
//! index and reserving it would replace this stepping.

use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::cmp::Ordering;

use bevy_ecs::{
    component::{Component, ComponentId, ComponentInfo},
    entity::{Entity, SpawnError},
    hierarchy::{ChildOf, Children},
    name::Name,
    query::With,
    reflect::{AppTypeRegistry, ReflectComponent},
    resource::Resource,
    world::{FromWorld, World},
};
use bevy_log::warn_once;
use bevy_platform::collections::HashMap;
use bevy_reflect::{
    serde::{TypedReflectDeserializer, TypedReflectSerializer},
    PartialReflect, ReflectFromReflect, ReflectPath, TypeRegistry,
};
use serde::de::DeserializeSeed;
use serde_json::Value;

use crate::details_panel::{holds_value, write_field_value, FieldEdit, FieldValue, FieldWrite};

/// The most generations [`spawn_at_remote_id`] steps through for one entity in one call.
const MAX_GENERATION_STEPS: u32 = 4096;

/// The [`World`] mirroring the remote app.
///
/// It holds a clone of the local [`AppTypeRegistry`], sharing its types, and the remote entities
/// the inspector shows, each with a [`RemoteComponents`] record.
#[derive(Resource)]
pub struct RemoteWorld {
    world: World,
    reserved: u32,
    types: HashMap<String, Option<ComponentId>>,
}

impl FromWorld for RemoteWorld {
    fn from_world(world: &mut World) -> Self {
        Self::new(
            world
                .get_resource::<AppTypeRegistry>()
                .cloned()
                .unwrap_or_default(),
        )
    }
}

/// Why a remote entity could not be spawned in the [`RemoteWorld`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpawnRemoteError {
    /// The index is taken by an entity of the remote world itself, such as a resource.
    Taken,
    /// The index has a later generation than the remote entity, so the remote app restarted.
    Behind,
    /// The generation is more than [`MAX_GENERATION_STEPS`] ahead. Later calls catch up further.
    Ahead,
}

/// The components a poll reported for one remote entity.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct TreeComponents {
    /// The value of the entity's [`Name`].
    pub(crate) name: Option<String>,
    /// The entity's [`ChildOf`] target.
    pub(crate) parent: Option<Entity>,
    /// The sorted type paths of the label-defining components read with a value.
    pub(crate) labels: Vec<String>,
    /// The sorted type paths of the label-defining components `has` reported present, or `None`
    /// if the poll did not ask for `has`.
    pub(crate) detected: Option<Vec<String>>,
}

impl RemoteWorld {
    /// An empty remote world sharing the types of `registry`.
    pub(crate) fn new(registry: AppTypeRegistry) -> Self {
        let mut world = World::new();
        world.insert_resource(registry);
        world.register_component::<Name>();
        world.register_component::<ChildOf>();
        world.register_component::<Children>();
        world.register_component::<RemoteComponents>();
        Self {
            world,
            reserved: 0,
            types: HashMap::default(),
        }
    }

    /// The mirrored world.
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Replaces the world with an empty one.
    pub(crate) fn reset(&mut self, registry: AppTypeRegistry) {
        *self = Self::new(registry);
    }

    /// Whether `remote` is mirrored.
    pub(crate) fn contains(&self, remote: Entity) -> bool {
        self.world.get::<RemoteComponents>(remote).is_some()
    }

    /// The mirrored entities.
    pub(crate) fn entities(&mut self) -> Vec<Entity> {
        self.world
            .query_filtered::<Entity, With<RemoteComponents>>()
            .iter(&self.world)
            .collect()
    }

    /// Spawns `remote` at its remote id. Returns whether it was not mirrored yet.
    pub(crate) fn spawn(&mut self, remote: Entity) -> Result<bool, SpawnRemoteError> {
        if self.contains(remote) {
            return Ok(false);
        }
        spawn_at_remote_id(&mut self.world, &mut self.reserved, remote)?;
        self.world
            .entity_mut(remote)
            .insert(RemoteComponents::default());
        Ok(true)
    }

    /// Despawns `remote` without freeing its index, detaching its children first so that they
    /// are not despawned with it.
    pub(crate) fn despawn(&mut self, remote: Entity) {
        let Ok(mut entity) = self.world.get_entity_mut(remote) else {
            return;
        };
        entity.remove::<Children>();
        entity.despawn_no_free();
    }

    /// Writes the components a poll reported for `remote`. Returns whether anything changed.
    ///
    /// A [`ChildOf`] whose target is not mirrored is left out until the target is, so that the
    /// entity shows as a root meanwhile. The detected label types are kept until a poll that asked
    /// for `has` reports them again.
    pub(crate) fn write(&mut self, remote: Entity, components: TreeComponents) -> bool {
        let parent = components
            .parent
            .filter(|parent| *parent != remote && self.contains(*parent));
        let world = &mut self.world;
        let Ok(mut entity) = world.get_entity_mut(remote) else {
            return false;
        };
        let mut changed = false;
        if entity.get::<Name>().map(Name::as_str) != components.name.as_deref() {
            match components.name {
                Some(name) => entity.insert(Name::new(name)),
                None => entity.remove::<Name>(),
            };
            changed = true;
        }
        if entity.get::<ChildOf>().map(ChildOf::parent) != parent {
            match parent {
                Some(parent) => entity.insert(ChildOf(parent)),
                None => entity.remove::<ChildOf>(),
            };
            changed = true;
        }

        let Some(record) = entity.get::<RemoteComponents>() else {
            return changed;
        };
        let detected = components
            .detected
            .filter(|detected| *detected != record.detected);
        if record.labels == components.labels && detected.is_none() {
            return changed;
        }
        let detected = detected.unwrap_or_else(|| record.detected.clone());
        let registry = world.resource::<AppTypeRegistry>().clone();
        let registry = registry.read();
        let mut ids = Vec::new();
        for type_path in components.labels.iter().chain(&detected) {
            ids.extend(component_id(&mut self.types, world, &registry, type_path));
        }
        world.entity_mut(remote).insert(RemoteComponents {
            labels: components.labels,
            detected,
            ids,
        });
        true
    }

    /// Reads the components `world.get_components` reported for `remote` back through the local
    /// type registry into its [`RemoteValues`]. Returns whether anything the details panel shows
    /// changed.
    ///
    /// A value whose serialized form did not change is kept as it is. Components the local
    /// registry does not know or cannot read back, and those the remote app could not serialize,
    /// are recorded as [`UnknownComponent`]s.
    pub(crate) fn write_values(&mut self, remote: Entity, fetched: FetchedComponents) -> bool {
        if !self.contains(remote) {
            return false;
        }
        let previous = self
            .world
            .get_mut::<RemoteValues>(remote)
            .map(|mut values| core::mem::take(&mut *values));
        let mut changed = previous.is_none();
        let mut previous = previous.unwrap_or_default();
        let registry = self.world.resource::<AppTypeRegistry>().clone();
        let registry = registry.read();

        let mut reported: Vec<(String, Value)> = fetched.components.into_iter().collect();
        reported.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let mut values = RemoteValues::default();
        for (type_path, json) in reported {
            if let Some(kept) = previous
                .components
                .iter()
                .position(|value| value.type_path == type_path && value.json == json)
            {
                values
                    .components
                    .push(previous.components.swap_remove(kept));
                continue;
            }
            match self.read_value(&registry, &type_path, &json) {
                Ok((id, value)) => {
                    changed = true;
                    values.components.push(RemoteValue {
                        id,
                        type_path,
                        json,
                        value,
                    });
                }
                Err(reason) => values.unknown.push(UnknownComponent { type_path, reason }),
            }
        }
        values
            .unknown
            .extend(
                fetched
                    .errors
                    .into_iter()
                    .map(|(type_path, error)| UnknownComponent {
                        type_path,
                        reason: error_message(&error),
                    }),
            );
        values
            .unknown
            .sort_unstable_by(|left, right| left.type_path.cmp(&right.type_path));
        changed |= !previous.components.is_empty() || previous.unknown != values.unknown;
        self.world.entity_mut(remote).insert(values);
        changed
    }

    /// Reads the reported `json` value of the component `type_path` back through `registry`,
    /// returning its id in the remote world and its value, or why it cannot be read.
    fn read_value(
        &mut self,
        registry: &TypeRegistry,
        type_path: &str,
        json: &Value,
    ) -> Result<(ComponentId, Box<dyn PartialReflect>), String> {
        let registration = registry
            .get_with_type_path(type_path)
            .ok_or_else(|| "the type is not registered in this app".to_string())?;
        let id = component_id(&mut self.types, &mut self.world, registry, type_path)
            .ok_or_else(|| "the type is not a reflected component in this app".to_string())?;
        let value = TypedReflectDeserializer::new(registration, registry)
            .deserialize(json)
            .map_err(|error| format!("the value cannot be read: {error}"))?;
        let value = registration
            .data::<ReflectFromReflect>()
            .and_then(|from_reflect| from_reflect.from_reflect(value.as_ref()))
            .map(PartialReflect::into_partial_reflect)
            .unwrap_or(value);
        Ok((id, value))
    }

    /// Drops the [`RemoteValues`] of `remote`, if it has any.
    pub(crate) fn clear_values(&mut self, remote: Entity) {
        if let Ok(mut entity) = self.world.get_entity_mut(remote) {
            entity.remove::<RemoteValues>();
        }
    }

    /// The value the remote app last reported for the component `type_path` of `remote`, read
    /// back through the local type registry.
    ///
    /// Only the selected remote entity has its components fetched, see
    /// [`RemoteDetails`](super::RemoteDetails).
    pub fn reflected(&self, remote: Entity, type_path: &str) -> Option<&dyn PartialReflect> {
        Some(
            self.world
                .get::<RemoteValues>(remote)?
                .get(type_path)?
                .value(),
        )
    }

    /// Writes `edit` into the [`RemoteValues`] of its entity, so that the details panel shows it
    /// before the remote app reports it back, and serializes the edited field for the remote app.
    ///
    /// The serialized form of an edited component is forgotten, so that the next report of the
    /// remote app is always read back, reverting the edit if the remote app rejected it.
    pub(crate) fn edit_value(&mut self, edit: &FieldEdit) -> ValueEdit {
        let Some(id) = self
            .world
            .get::<RemoteValues>(edit.entity)
            .and_then(|values| values.get(&edit.component))
            .map(RemoteValue::id)
        else {
            return ValueEdit::rejected("the remote app has not reported the component");
        };
        if self
            .world
            .components()
            .get_info(id)
            .is_some_and(|info| !ComponentInfo::mutable(info))
        {
            return ValueEdit::rejected("the component is immutable");
        }
        let registry = self.world.resource::<AppTypeRegistry>().clone();
        let registry = registry.read();
        let Some(mut values) = self.world.get_mut::<RemoteValues>(edit.entity) else {
            return ValueEdit::rejected("the remote app has not reported the component");
        };
        let Some(RemoteValue { json, value, .. }) = values
            .components
            .iter_mut()
            .find(|value| value.type_path == edit.component)
        else {
            return ValueEdit::rejected("the remote app has not reported the component");
        };
        let value: &mut dyn PartialReflect = value.as_mut();
        let field = if edit.path.is_empty() {
            Ok(value)
        } else {
            edit.path.as_str().reflect_element_mut(value)
        };
        let Ok(field) = field else {
            return ValueEdit::rejected("the component has no such field");
        };

        let write = write_field_value(field, &edit.value, &registry);
        let held = write != FieldWrite::Rejected && holds_value(field, &edit.value);
        match write {
            FieldWrite::Unchanged => ValueEdit {
                held,
                ..ValueEdit::default()
            },
            FieldWrite::Rejected => {
                let typing_a_char = matches!(edit.value, FieldValue::Text(_))
                    && field.try_downcast_ref::<char>().is_some();
                ValueEdit {
                    warning: (!typing_a_char)
                        .then(|| "the value does not fit the field".to_string()),
                    held,
                    ..ValueEdit::default()
                }
            }
            FieldWrite::Written => {
                *json = Value::Null;
                match serde_json::to_value(TypedReflectSerializer::new(&*field, &registry)) {
                    Ok(serialized) => ValueEdit {
                        send: Some(serialized),
                        held,
                        ..ValueEdit::default()
                    },
                    Err(error) => ValueEdit::rejected(format!(
                        "the edited value cannot be serialized: {error}"
                    )),
                }
            }
        }
    }
}

/// The id in `world` of the reflected component `type_path`, registering it on first use, or
/// `None` if `registry` does not know it as a component. Lookups are cached in `types`.
fn component_id(
    types: &mut HashMap<String, Option<ComponentId>>,
    world: &mut World,
    registry: &TypeRegistry,
    type_path: &str,
) -> Option<ComponentId> {
    if let Some(id) = types.get(type_path) {
        return *id;
    }
    let id = registry
        .get_with_type_path(type_path)
        .and_then(|registration| registration.data::<ReflectComponent>())
        .map(|reflect_component| reflect_component.register_component(world));
    types.insert(type_path.to_string(), id);
    id
}

/// The message of a serialized `BrpError`, or the whole error if it has none.
fn error_message(error: &Value) -> String {
    error
        .get("message")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| error.to_string())
}

/// The components `world.get_components` reported for one remote entity.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct FetchedComponents {
    /// The serialized value of each component the remote app could serialize, by type path.
    pub(crate) components: HashMap<String, Value>,
    /// The error the remote app reported for each other component, by type path.
    pub(crate) errors: HashMap<String, Value>,
}

/// What [`RemoteWorld::edit_value`] did.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ValueEdit {
    /// The serialized field to send to the remote app, if the edit changed it.
    pub(crate) send: Option<Value>,
    /// Why the edit was not applied, if it should be reported.
    pub(crate) warning: Option<String>,
    /// Whether the field holds the edited value, so that its widget is up to date.
    pub(crate) held: bool,
}

impl ValueEdit {
    fn rejected(warning: impl Into<String>) -> Self {
        Self {
            warning: Some(warning.into()),
            ..Self::default()
        }
    }
}

/// The components the remote app last reported for the selected remote entity, read back through
/// the local type registry.
///
/// The values are held here instead of being inserted as components, so that no hook, observer
/// or required component of the local app ever runs on remote data.
#[derive(Component, Debug, Default)]
pub struct RemoteValues {
    components: Vec<RemoteValue>,
    unknown: Vec<UnknownComponent>,
}

impl RemoteValues {
    /// The components that were read back, sorted by type path.
    pub fn components(&self) -> &[RemoteValue] {
        &self.components
    }

    /// The components that could not be read back, sorted by type path.
    pub fn unknown(&self) -> &[UnknownComponent] {
        &self.unknown
    }

    /// The component `type_path`, if it was read back.
    pub fn get(&self, type_path: &str) -> Option<&RemoteValue> {
        self.components
            .iter()
            .find(|value| value.type_path == type_path)
    }
}

/// One component of a [`RemoteValues`].
#[derive(Debug)]
pub struct RemoteValue {
    id: ComponentId,
    type_path: String,
    /// The serialized value the remote app reported, or `Null` once edited locally.
    json: Value,
    value: Box<dyn PartialReflect>,
}

impl RemoteValue {
    /// The id of the component in the [`RemoteWorld`].
    pub fn id(&self) -> ComponentId {
        self.id
    }

    /// The type path of the component.
    pub fn type_path(&self) -> &str {
        &self.type_path
    }

    /// The value of the component.
    pub fn value(&self) -> &dyn PartialReflect {
        self.value.as_ref()
    }
}

/// A component the remote app reported that the inspector cannot show, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownComponent {
    /// The type path the remote app reported.
    pub type_path: String,
    /// Why the component cannot be shown.
    pub reason: String,
}

/// Spawns an empty entity at exactly `remote`, an id the remote app allocated.
///
/// This is the only place that works around the entity allocator of the remote world:
/// - Every index up to `remote.index()` is allocated and leaked first, so that the allocator never
///   hands one out later, which would make that spawn panic. `reserved` counts them.
/// - An index starts at the first generation, so it is stepped up to `remote.generation()` by
///   spawning and despawning it without freeing, which bumps the generation by one each time. At
///   most [`MAX_GENERATION_STEPS`] steps are taken per call.
fn spawn_at_remote_id(
    world: &mut World,
    reserved: &mut u32,
    remote: Entity,
) -> Result<(), SpawnRemoteError> {
    let index = remote.index_u32();
    while *reserved <= index {
        for entity in world.entity_allocator().alloc_many(index + 1 - *reserved) {
            *reserved = (*reserved).max(entity.index_u32() + 1);
        }
    }
    for _ in 0..MAX_GENERATION_STEPS {
        match world.entities().check_can_spawn_at(remote) {
            Ok(()) => {
                world
                    .spawn_empty_at(remote)
                    .map_err(|_| SpawnRemoteError::Taken)?;
                return Ok(());
            }
            Err(SpawnError::AlreadySpawned) => return Err(SpawnRemoteError::Taken),
            Err(SpawnError::Invalid(error)) => {
                if error.current_generation.cmp_approx(&remote.generation()) != Ordering::Less {
                    return Err(SpawnRemoteError::Behind);
                }
                let current =
                    Entity::from_index_and_generation(remote.index(), error.current_generation);
                world
                    .spawn_empty_at(current)
                    .map_err(|_| SpawnRemoteError::Taken)?
                    .despawn_no_free();
            }
        }
    }
    warn_once!(
        "the remote inspector is still catching up with the generation of {remote}, which can \
         take several polls"
    );
    Err(SpawnRemoteError::Ahead)
}

/// The label-defining components the remote app reported for one mirrored entity.
///
/// Only [`Name`] and [`ChildOf`] are inserted into the remote world, so the entity tree labels
/// remote entities with the components recorded here.
#[derive(Component, Debug, Default)]
pub struct RemoteComponents {
    labels: Vec<String>,
    detected: Vec<String>,
    ids: Vec<ComponentId>,
}

impl RemoteComponents {
    /// The ids in the remote world of the label-defining components the remote app reported that
    /// are registered locally.
    pub fn reported(&self) -> impl Iterator<Item = ComponentId> + '_ {
        self.ids.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_reflect::{Reflect, TypePath, TypeRegistryArc};

    #[derive(Component, Reflect)]
    #[reflect(Component)]
    struct Labeled;

    fn registry() -> AppTypeRegistry {
        let registry = AppTypeRegistry(TypeRegistryArc::default());
        registry.write().register::<Labeled>();
        registry
    }

    fn remote(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    fn at_generation(index: u32, generation: u32) -> Entity {
        let entity = remote(index);
        Entity::from_index_and_generation(
            entity.index(),
            entity.generation().after_versions(generation),
        )
    }

    fn spawned(entity: Entity) -> RemoteWorld {
        let mut remote_world = RemoteWorld::new(registry());
        remote_world.spawn(entity).unwrap();
        remote_world
    }

    fn child_of(parent: Entity) -> TreeComponents {
        TreeComponents {
            parent: Some(parent),
            ..TreeComponents::default()
        }
    }

    #[test]
    fn spawn_at_rejects_a_later_generation_in_a_fresh_world() {
        let entity = at_generation(5, 3);
        let mut world = World::new();
        assert!(matches!(
            world.spawn_at(entity, ()),
            Err(SpawnError::Invalid(_))
        ));

        let mut remote_world = RemoteWorld::new(registry());
        assert_eq!(remote_world.spawn(entity), Ok(true));
        assert!(remote_world.world.get_entity(entity).is_ok());
        assert_eq!(remote_world.spawn(entity), Ok(false));
    }

    #[test]
    fn hand_placed_ids_are_never_allocated_again() {
        let mut remote_world = RemoteWorld::new(registry());
        let placed = [remote(20), at_generation(40, 2)];
        for entity in placed {
            remote_world.spawn(entity).unwrap();
        }
        remote_world.despawn(placed[0]);
        for _ in 0..100 {
            let entity = remote_world.world.spawn_empty().id();
            assert!(placed.iter().all(|placed| placed.index() != entity.index()));
        }
        assert!(remote_world.world.get_entity(placed[1]).is_ok());
    }

    #[test]
    fn a_respawn_at_the_same_index_catches_up() {
        let mut remote_world = spawned(remote(4));
        remote_world.despawn(remote(4));
        assert_eq!(remote_world.spawn(at_generation(4, 2)), Ok(true));
        assert!(remote_world.world.get_entity(remote(4)).is_err());
    }

    #[test]
    fn an_earlier_generation_means_the_remote_app_restarted() {
        let mut remote_world = spawned(at_generation(4, 2));
        assert_eq!(
            remote_world.spawn(at_generation(4, 1)),
            Err(SpawnRemoteError::Behind)
        );
        remote_world.despawn(at_generation(4, 2));
        assert_eq!(remote_world.spawn(remote(4)), Err(SpawnRemoteError::Behind));
    }

    #[test]
    fn ids_taken_by_the_remote_world_itself_are_skipped() {
        let mut remote_world = RemoteWorld::new(registry());
        assert_eq!(remote_world.spawn(remote(0)), Err(SpawnRemoteError::Taken));
        assert!(!remote_world.contains(remote(0)));
    }

    #[test]
    fn distant_generations_are_caught_up_over_several_calls() {
        let mut remote_world = RemoteWorld::new(registry());
        let entity = at_generation(3, MAX_GENERATION_STEPS + 10);
        assert_eq!(remote_world.spawn(entity), Err(SpawnRemoteError::Ahead));
        assert_eq!(remote_world.spawn(entity), Ok(true));
    }

    #[test]
    fn despawning_a_parent_keeps_its_children() {
        let parent = remote(10);
        let child = remote(11);
        let mut remote_world = spawned(parent);
        remote_world.spawn(child).unwrap();
        remote_world.write(child, child_of(parent));
        remote_world.despawn(parent);
        assert!(remote_world.contains(child));
        assert!(remote_world.world.get::<ChildOf>(child).is_none());
    }

    #[test]
    fn a_child_of_a_missing_parent_waits_until_it_appears() {
        let parent = remote(20);
        let child = remote(21);
        let mut remote_world = spawned(child);
        assert!(!remote_world.write(child, child_of(parent)));
        assert!(remote_world.world.get::<ChildOf>(child).is_none());

        remote_world.spawn(parent).unwrap();
        assert!(remote_world.write(child, child_of(parent)));
        assert_eq!(
            remote_world
                .world
                .get::<ChildOf>(child)
                .map(ChildOf::parent),
            Some(parent)
        );
        assert!(!remote_world.write(child, child_of(parent)));
    }

    #[test]
    fn detected_labels_are_kept_until_has_is_asked_again() {
        let entity = remote(3);
        let mut remote_world = spawned(entity);
        let id = remote_world.world.register_component::<Labeled>();
        let reported = |remote_world: &RemoteWorld| -> Vec<ComponentId> {
            remote_world
                .world
                .get::<RemoteComponents>(entity)
                .unwrap()
                .reported()
                .collect()
        };

        let detected = TreeComponents {
            detected: Some(alloc::vec![Labeled::type_path().into()]),
            ..TreeComponents::default()
        };
        assert!(remote_world.write(entity, detected.clone()));
        assert_eq!(reported(&remote_world), [id]);
        assert!(!remote_world.write(entity, TreeComponents::default()));
        assert_eq!(reported(&remote_world), [id]);
        assert!(!remote_world.write(entity, detected));

        let unknown = TreeComponents {
            detected: Some(alloc::vec!["demo::Unknown".into()]),
            ..TreeComponents::default()
        };
        assert!(remote_world.write(entity, unknown));
        assert!(reported(&remote_world).is_empty());
        assert!(remote_world.world.get::<Labeled>(entity).is_none());
    }
}
