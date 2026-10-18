//! A panel listing the entities of the inspected world, and the systems that keep it in sync.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use hearth_dev_tools::inspection::label_resolution::{
    resolve_label, ComponentLabelData, LabelDefinitionPriority, LabelResolutionRegistry,
};
use hearth_ecs::{
    change_detection::{DetectChanges, Mut},
    component::{Component, ComponentId, ComponentInfo},
    entity::Entity,
    hierarchy::{ChildOf, Children},
    observer::{Observer, On},
    query::With,
    reflect::{ReflectComponent, ReflectResource},
    resource::{IsResource, Resource},
    system::{Commands, Query, Res, ResMut, SystemIdMarker},
    world::World,
};
use hearth_feathers::{
    containers::{subpane, subpane_body, subpane_header},
    controls::{
        FeathersTreeItem, FeathersTreeItemChildren, FeathersTreeItemHeader, FeathersTreeView,
    },
    display::caption,
};
use hearth_log::warn;
use hearth_platform::collections::{HashMap, HashSet};
use hearth_reflect::{prelude::ReflectDefault, Reflect};
use hearth_scene::{bsn, bsn_list, on, Scene, WorldSceneExt};
use hearth_time::{Time, Timer, TimerMode};
use hearth_ui::{percent, px, widget::Text, ComputedNode, Expanded, Node, Overflow};
use hearth_ui_widgets::{
    tree_view_expand_self_update, tree_view_self_update, ScrollArea, ScrollIntoView,
    SelectedTreeItem, TreeItem, TreeItemExpandChange, ValueChange,
};

use crate::{InspectorSelection, InspectorTreeRoot};

/// The number of frames [`reveal_tree_selection`] waits for a revealed row to be laid out before
/// it gives up scrolling it into view.
const SCROLL_FRAMES: u32 = 60;

/// Marker for root entities spawned by the inspector. An `InspectorUi` entity's descendants are
/// hidden from the entity tree, but the marked entity itself still shows as a single,
/// non-expandable row.
#[derive(Component, Debug, Default, Clone, Copy, Reflect)]
#[reflect(Component, Debug, Default, Clone)]
pub struct InspectorUi;

/// Marker for the tree view that holds the entity tree rows.
#[derive(Component, Debug, Default, Clone, Copy, Reflect)]
#[reflect(Component, Debug, Default, Clone)]
pub struct InspectorTreeView;

/// A row of the entity tree, and the entity it displays.
#[derive(Component, Debug, Clone, Copy, Reflect)]
#[reflect(Component, Debug, Clone)]
pub struct InspectorRow {
    /// The inspected entity this row displays.
    pub source: Entity,
}

/// Marker for the text entity holding a row's label.
#[derive(Component, Debug, Default, Clone, Copy, Reflect)]
#[reflect(Component, Debug, Default, Clone)]
pub struct InspectorRowLabel;

/// Marker for rows whose child rows are kept in sync, added the first time a row is expanded.
#[derive(Component, Debug, Default, Clone, Copy, Reflect)]
#[reflect(Component, Debug, Default, Clone)]
pub struct InspectorRowPopulated;

/// Maps inspected entities to the tree rows displaying them, and back.
#[derive(Resource, Debug, Default, Reflect)]
#[reflect(Resource, Debug, Default)]
pub struct TreeRowIndex {
    source_to_row: HashMap<Entity, Entity>,
    row_to_source: HashMap<Entity, Entity>,
}

impl TreeRowIndex {
    /// The row displaying `source`, if one exists.
    pub fn row(&self, source: Entity) -> Option<Entity> {
        self.source_to_row.get(&source).copied()
    }

    /// The inspected entity displayed by `row`, if one exists.
    pub fn source(&self, row: Entity) -> Option<Entity> {
        self.row_to_source.get(&row).copied()
    }

    /// The number of rows currently tracked.
    pub fn len(&self) -> usize {
        self.row_to_source.len()
    }

    /// Whether no rows are currently tracked.
    pub fn is_empty(&self) -> bool {
        self.row_to_source.is_empty()
    }
}

/// Pacing of the entity tree synchronization pass.
#[derive(Resource, Debug)]
pub struct EntityTreeSync {
    /// Time between synchronization passes.
    pub timer: Timer,
}

impl Default for EntityTreeSync {
    fn default() -> Self {
        let mut timer = Timer::from_seconds(0.25, TimerMode::Repeating);
        timer.set_elapsed(timer.duration());
        Self { timer }
    }
}

impl EntityTreeSync {
    /// Forces a synchronization pass on the next tick.
    pub fn set_dirty(&mut self) {
        // A timer with no time remaining finishes on the next tick already, and ticking it here
        // would use that up.
        if !self.timer.remaining().is_zero() {
            self.timer.almost_finish();
        }
    }
}

/// A panel listing the entities of the inspected world as an expandable tree.
pub fn entity_tree_panel() -> impl Scene {
    bsn! {
        InspectorUi
        @subpane()
        Node {
            width: px(280),
            max_height: percent(100),
        }
        Children [
            @subpane_header() Children [
                @caption("Entities")
            ]
            --
            @subpane_body()
            Node {
                overflow: Overflow::scroll_y(),
            }
            ScrollArea
            Children [
                InspectorTreeView
                @FeathersTreeView
                on(tree_view_self_update)
                on(tree_view_expand_self_update)
                on(inspector_tree_selected)
                on(inspector_tree_expanded)
            ]
        ]
    }
}

/// Observer that records the inspected entity of the selected row in [`InspectorSelection`].
pub fn inspector_tree_selected(
    change: On<ValueChange<Option<Entity>>>,
    trees: Query<(), With<InspectorTreeView>>,
    rows: Query<&InspectorRow>,
    mut selection: ResMut<InspectorSelection>,
) {
    if !trees.contains(change.source) {
        return;
    }
    selection.0 = change
        .value
        .and_then(|row| rows.get(row).ok())
        .map(|row| row.source);
}

/// Observer that starts keeping a row's child rows in sync the first time it is expanded.
pub fn inspector_tree_expanded(
    change: On<TreeItemExpandChange>,
    rows: Query<(), With<InspectorRow>>,
    mut sync: ResMut<EntityTreeSync>,
    mut commands: Commands,
) {
    if !change.expanded || !rows.contains(change.item) {
        return;
    }
    commands.entity(change.item).insert(InspectorRowPopulated);
    sync.set_dirty();
}

/// Forces a synchronization pass when [`InspectorTreeRoot`] changes, so that the tree is rescoped
/// on the next tick.
pub fn resync_on_tree_root_change(root: Res<InspectorTreeRoot>, mut sync: ResMut<EntityTreeSync>) {
    if root.is_changed() {
        sync.set_dirty();
    }
}

/// The entity whose children are the top-level rows of the tree: the [`InspectorTreeRoot`] while
/// the inspector reads from the local world, and `None` otherwise.
pub(crate) fn tree_scope(world: &World) -> Option<Entity> {
    #[cfg(feature = "remote")]
    if crate::remote::is_remote(world) {
        return None;
    }
    world.get_resource::<InspectorTreeRoot>()?.0
}

/// Adds, removes, relabels and reorders tree rows so that they match the inspected world.
pub fn sync_entity_tree(world: &mut World) {
    let delta = world
        .get_resource::<Time>()
        .map(Time::delta)
        .unwrap_or_default();

    if !world
        .resource_mut::<EntityTreeSync>()
        .timer
        .tick(delta)
        .just_finished()
    {
        return;
    }

    let plan = plan_sync(world);
    apply_sync(world, plan);
    rebuild_index(world);
}

struct RowSpawn {
    container: Entity,
    source: Entity,
    label: String,
    expandable: bool,
}

/// The set of changes a single synchronization pass needs to apply.
///
/// [`plan_sync`] reads the world without mutating it, diffing the root container against the
/// current top-level entities and each populated row's children container against that row's
/// entity's current children (see [`diff_container`]). Rows that were never expanded, and so have
/// no children container, are not diffed. The result records which rows to despawn, which to
/// spawn, which labels to update, which `expandable` flags to flip and which containers to
/// reorder. [`apply_sync`] then applies those changes in that order and rebuilds [`TreeRowIndex`].
#[derive(Default)]
struct SyncPlan {
    despawn: Vec<Entity>,
    spawn: Vec<RowSpawn>,
    relabel: Vec<(Entity, String)>,
    expandable: Vec<(Entity, bool)>,
    /// Containers whose rows are out of order, and the entities they display, in order.
    reorder: Vec<(Entity, Vec<Entity>)>,
}

/// Builds the [`SyncPlan`] for one synchronization pass.
///
/// The rows live in `world`, while the entities they display are read from the inspected world,
/// see [`crate::world_to_inspect`].
fn plan_sync(world: &World) -> SyncPlan {
    let mut plan = SyncPlan::default();
    let mut tree_view = None;
    let mut populated = Vec::new();
    let inspected = crate::world_to_inspect(world);
    let priorities = world.get_resource::<LabelResolutionRegistry>();

    for entity_ref in world.iter_entities() {
        let entity = entity_ref.id();
        if entity_ref.contains::<InspectorTreeView>() {
            tree_view = Some(entity);
        }
        if entity_ref.contains::<InspectorRow>() && entity_ref.contains::<InspectorRowPopulated>() {
            populated.push(entity);
        }
    }

    let Some(tree_view) = tree_view else {
        return plan;
    };

    let roots = match tree_scope(world) {
        Some(root) => visible_children(inspected, root),
        None => root_entities(inspected),
    };
    let sources = Sources {
        world: inspected,
        priorities,
    };
    diff_container(world, &sources, tree_view, &roots, &mut plan);

    for row in populated {
        let Some(source) = world.get::<InspectorRow>(row).map(|row| row.source) else {
            continue;
        };
        if inspected.get_entity(source).is_err() {
            continue;
        }
        let Some(container) = child_with::<FeathersTreeItemChildren>(world, row) else {
            continue;
        };
        let expected = visible_children(inspected, source);
        diff_container(world, &sources, container, &expected, &mut plan);
    }

    plan
}

/// The visible entities of `world` without a parent, sorted by index.
fn root_entities(world: &World) -> Vec<Entity> {
    let mut roots: Vec<Entity> = world
        .iter_entities()
        .filter(|entity_ref| !entity_ref.contains::<ChildOf>())
        .map(|entity_ref| entity_ref.id())
        .filter(|entity| !is_excluded(world, *entity))
        .collect();
    roots.sort_unstable_by_key(|root| root.index());
    roots
}

/// The inspected world, and the label priorities of the world the inspector runs in.
struct Sources<'w> {
    world: &'w World,
    priorities: Option<&'w LabelResolutionRegistry>,
}

/// Diffs one container's rows in `world` against the `expected` inspected entities, recording the
/// changes into `plan`. See [`SyncPlan`].
fn diff_container(
    world: &World,
    sources: &Sources,
    container: Entity,
    expected: &[Entity],
    plan: &mut SyncPlan,
) {
    let mut existing: HashMap<Entity, Entity> = HashMap::new();
    let mut order: Vec<Entity> = Vec::new();
    if let Some(children) = world.get::<Children>(container) {
        for child in children.iter().copied() {
            if let Some(row) = world.get::<InspectorRow>(child) {
                existing.insert(row.source, child);
                order.push(row.source);
            }
        }
    }

    let expected_set: HashSet<Entity> = expected.iter().copied().collect();
    for (source, row) in existing.iter() {
        if !expected_set.contains(source) {
            plan.despawn.push(*row);
        }
    }

    // Spawned rows are appended, so this is the order the rows end up in unless reordered.
    order.retain(|source| expected_set.contains(source));
    order.extend(
        expected
            .iter()
            .filter(|source| !existing.contains_key(*source)),
    );
    if order != expected {
        plan.reorder.push((container, expected.to_vec()));
    }

    for source in expected.iter().copied() {
        let expandable = !visible_children(sources.world, source).is_empty();
        let label = entity_label(sources.world, sources.priorities, source);
        let Some(row) = existing.get(&source).copied() else {
            plan.spawn.push(RowSpawn {
                container,
                source,
                label,
                expandable,
            });
            continue;
        };

        if let Some(label_entity) = row_label_entity(world, row)
            && world
                .get::<Text>(label_entity)
                .is_some_and(|text| text.0 != label)
        {
            plan.relabel.push((label_entity, label));
        }

        if world
            .get::<TreeItem>(row)
            .is_some_and(|item| item.expandable != expandable)
        {
            plan.expandable.push((row, expandable));
        }
    }
}

fn apply_sync(world: &mut World, plan: SyncPlan) {
    for row in plan.despawn {
        if let Ok(row) = world.get_entity_mut(row) {
            row.despawn();
        }
    }

    for (label_entity, label) in plan.relabel {
        if let Some(mut text) = world.get_mut::<Text>(label_entity) {
            text.0 = label;
        }
    }

    for (row, expandable) in plan.expandable {
        if let Some(mut item) = world.get_mut::<TreeItem>(row) {
            item.expandable = expandable;
        }
    }

    for spawn in plan.spawn {
        if world.get_entity(spawn.container).is_err() {
            continue;
        }
        match world.spawn_scene(entity_row(spawn.label, spawn.expandable)) {
            Ok(mut row) => {
                row.insert((
                    InspectorRow {
                        source: spawn.source,
                    },
                    ChildOf(spawn.container),
                ));
            }
            Err(error) => warn!("failed to spawn an inspector tree row: {error}"),
        }
    }

    for (container, expected) in plan.reorder {
        order_rows(world, container, &expected);
    }
}

/// Sorts the rows of `container` so that they follow `expected`, the entities they display in
/// order. Children of `container` that are not rows keep their place.
fn order_rows(world: &mut World, container: Entity, expected: &[Entity]) {
    let rank: HashMap<Entity, usize> = expected
        .iter()
        .enumerate()
        .map(|(rank, source)| (*source, rank))
        .collect();
    let Some(children) = world.get::<Children>(container) else {
        return;
    };
    let mut slots = Vec::new();
    let mut rows = Vec::new();
    for (slot, child) in children.iter().copied().enumerate() {
        if let Some(row) = world.get::<InspectorRow>(child) {
            slots.push(slot);
            rows.push((rank.get(&row.source).copied().unwrap_or(usize::MAX), child));
        }
    }
    rows.sort_by_key(|(rank, _)| *rank);
    let mut desired = children.to_vec();
    for (slot, (_, row)) in slots.into_iter().zip(rows) {
        desired[slot] = row;
    }
    if desired.as_slice() == &**children {
        return;
    }
    let position: HashMap<Entity, usize> = desired
        .into_iter()
        .enumerate()
        .map(|(position, child)| (child, position))
        .collect();
    if let Some(mut children) = world.get_mut::<Children>(container) {
        children.sort_by_key(|child| position.get(child).copied().unwrap_or(usize::MAX));
    }
}

/// Despawns every tree row, for when the inspected world is replaced and its entity ids no longer
/// mean the same entities.
#[cfg(feature = "remote")]
pub(crate) fn clear_rows(world: &mut World) {
    let rows: Vec<Entity> = world
        .query_filtered::<Entity, With<InspectorRow>>()
        .iter(world)
        .collect();
    for row in rows {
        if let Ok(row) = world.get_entity_mut(row) {
            row.despawn();
        }
    }
    if let Some(mut index) = world.get_resource_mut::<TreeRowIndex>() {
        *index = TreeRowIndex::default();
    }
}

fn rebuild_index(world: &mut World) {
    let mut source_to_row = HashMap::new();
    let mut row_to_source = HashMap::new();
    for (entity, row) in world.query::<(Entity, &InspectorRow)>().iter(world) {
        source_to_row.insert(row.source, entity);
        row_to_source.insert(entity, row.source);
    }

    let mut index = world.resource_mut::<TreeRowIndex>();
    index.source_to_row = source_to_row;
    index.row_to_source = row_to_source;
}

/// Progress of showing the [`InspectorSelection`] in the entity tree, see
/// [`reveal_tree_selection`].
#[derive(Resource, Debug, Default)]
pub struct TreeReveal {
    /// The selection the progress is for.
    selection: Option<Entity>,
    state: RevealState,
}

/// A step of [`reveal_tree_selection`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum RevealState {
    /// Nothing left to do. The selected row is still followed when rows are rebuilt.
    #[default]
    Idle,
    /// Expanding the rows above the selected entity until it has a row.
    Expanding,
    /// Waiting for the selected `row` to be laid out to scroll it into view, for at most `frames`
    /// more frames.
    Scrolling { row: Entity, frames: u32 },
}

/// Shows the [`InspectorSelection`] in the entity tree when it changes from outside the tree, such
/// as by a click in a viewport.
///
/// The rows above the selected entity are expanded from the top down, which spawns their child
/// rows on the following synchronization passes, until the selected entity has a row. That row is
/// then selected by setting the tree view's [`SelectedTreeItem`], which emits no selection event,
/// and scrolled into view once it is laid out. A selection the tree cannot show yet, such as an
/// entity outside the [`InspectorTreeRoot`], clears the tree selection and is retried every frame
/// until it can be shown or the selection changes. A selection made by clicking a row is left
/// alone, and no selection clears the tree selection.
///
/// Once done, the selected row is followed when rows are rebuilt, such as when the selected entity
/// is reparented.
pub fn reveal_tree_selection(world: &mut World) {
    world.resource_scope(|world, mut reveal: Mut<TreeReveal>| {
        let selection = world.resource::<InspectorSelection>().0;
        if reveal.selection != selection {
            reveal.selection = selection;
            reveal.state = start_reveal(world, selection);
        }
        reveal.state = match (reveal.state, selection) {
            (RevealState::Expanding, Some(target)) => expand_to(world, target),
            (RevealState::Scrolling { row, frames }, _) => scroll_to(world, row, frames),
            (_, selection) => {
                follow_selection(world, selection);
                RevealState::Idle
            }
        };
    });
}

/// The first step of revealing a new `selection`, clearing the tree selection unless the
/// selected row is already selected in the tree, as it is after a click on the row.
fn start_reveal(world: &mut World, selection: Option<Entity>) -> RevealState {
    let row = selection.and_then(|target| world.resource::<TreeRowIndex>().row(target));
    if let Some(row) = row
        && owning_tree(world, row)
            .and_then(|tree| world.get::<SelectedTreeItem>(tree))
            .is_some_and(|selected| selected.0 == Some(row))
    {
        return RevealState::Idle;
    }
    let trees: Vec<Entity> = world
        .query_filtered::<Entity, With<InspectorTreeView>>()
        .iter(world)
        .collect();
    for tree in trees {
        set_tree_selection(world, tree, None);
    }
    match selection {
        Some(_) => RevealState::Expanding,
        None => RevealState::Idle,
    }
}

/// Expands the collapsed rows above `target`, and selects the row of `target` once it exists.
fn expand_to(world: &mut World, target: Entity) -> RevealState {
    let inspected = crate::world_to_inspect(world);
    let index = world.resource::<TreeRowIndex>();
    let Some(path) = reveal_path(inspected, tree_scope(world), target) else {
        return RevealState::Expanding;
    };
    let mut collapsed = Vec::new();
    for ancestor in path {
        let Some(row) = index.row(ancestor) else {
            break;
        };
        if world.get_entity(row).is_ok_and(|row| {
            !row.contains::<Expanded>() || !row.contains::<InspectorRowPopulated>()
        }) {
            collapsed.push(row);
        }
    }
    let target_row = index.row(target);

    if !collapsed.is_empty() {
        for row in collapsed {
            world
                .entity_mut(row)
                .insert((Expanded, InspectorRowPopulated));
        }
        world.resource_mut::<EntityTreeSync>().set_dirty();
    }
    let Some(row) = target_row else {
        return RevealState::Expanding;
    };
    if let Some(tree) = owning_tree(world, row) {
        set_tree_selection(world, tree, Some(row));
    }
    RevealState::Scrolling {
        row,
        frames: SCROLL_FRAMES,
    }
}

/// Scrolls the header of `row` into view once it is laid out, giving up after `frames` frames.
fn scroll_to(world: &mut World, row: Entity, frames: u32) -> RevealState {
    if world.get_entity(row).is_err() {
        return RevealState::Idle;
    }
    let target = child_with::<FeathersTreeItemHeader>(world, row).unwrap_or(row);
    if world
        .get::<ComputedNode>(target)
        .is_some_and(|node| !node.is_empty())
    {
        world.trigger(ScrollIntoView { entity: target });
        return RevealState::Idle;
    }
    match frames.checked_sub(1) {
        Some(frames) => RevealState::Scrolling { row, frames },
        None => RevealState::Idle,
    }
}

/// Selects the current row of `selection` in its tree, if it has one.
fn follow_selection(world: &mut World, selection: Option<Entity>) {
    let Some(row) = selection.and_then(|target| world.resource::<TreeRowIndex>().row(target))
    else {
        return;
    };
    if let Some(tree) = owning_tree(world, row) {
        set_tree_selection(world, tree, Some(row));
    }
}

/// The entities above `target` whose rows hold its row, top-level first, or `None` if the tree
/// shows no row for `target`, as when it is hidden or outside the tree's `scope`.
fn reveal_path(world: &World, scope: Option<Entity>, target: Entity) -> Option<Vec<Entity>> {
    if Some(target) == scope || is_excluded(world, target) {
        return None;
    }
    let mut ancestors = Vec::new();
    let mut current = target;
    while let Some(child_of) = world.get::<ChildOf>(current) {
        current = child_of.parent();
        if Some(current) == scope {
            ancestors.reverse();
            return Some(ancestors);
        }
        ancestors.push(current);
    }
    if scope.is_some() {
        return None;
    }
    ancestors.reverse();
    Some(ancestors)
}

/// The [`InspectorTreeView`] holding `row`.
fn owning_tree(world: &World, row: Entity) -> Option<Entity> {
    let mut current = row;
    while let Some(child_of) = world.get::<ChildOf>(current) {
        current = child_of.parent();
        if world
            .get_entity(current)
            .is_ok_and(|entity| entity.contains::<InspectorTreeView>())
        {
            return Some(current);
        }
    }
    None
}

/// Sets the [`SelectedTreeItem`] of `tree` to `row`, unless it already holds `row`.
fn set_tree_selection(world: &mut World, tree: Entity, row: Option<Entity>) {
    if world
        .get::<SelectedTreeItem>(tree)
        .map(|selected| selected.0)
        == Some(row)
    {
        return;
    }
    if let Ok(mut tree) = world.get_entity_mut(tree) {
        tree.insert(SelectedTreeItem(row));
    }
}

fn entity_row(label: String, expandable: bool) -> impl Scene {
    bsn! {
        @FeathersTreeItem {
            @expandable: {expandable},
            @label: bsn_list! { @row_label({label}) },
        }
    }
}

fn row_label(text: String) -> impl Scene {
    bsn! {
        InspectorRowLabel
        @caption(text)
    }
}

fn is_excluded(world: &World, entity: Entity) -> bool {
    let Ok(entity_ref) = world.get_entity(entity) else {
        return true;
    };
    if entity_ref.contains::<IsResource>()
        || entity_ref.contains::<SystemIdMarker>()
        || entity_ref.contains::<Observer>()
    {
        return true;
    }

    let mut current = entity;
    while let Some(child_of) = world.get::<ChildOf>(current) {
        current = child_of.parent();
        match world.get_entity(current) {
            Ok(parent) => {
                if parent.contains::<InspectorUi>() {
                    return true;
                }
            }
            Err(_) => return true,
        }
    }
    false
}

fn visible_children(world: &World, entity: Entity) -> Vec<Entity> {
    world
        .get::<Children>(entity)
        .map(|children| {
            children
                .iter()
                .copied()
                .filter(|child| !is_excluded(world, *child))
                .collect()
        })
        .unwrap_or_default()
}

/// The label of `entity` in the inspected `world`, resolved with the label `priorities` of the
/// world the inspector runs in.
///
/// For a remote entity, only the components the remote app reported count, see
/// [`crate::remote::RemoteComponents`].
pub(crate) fn entity_label(
    world: &World,
    priorities: Option<&LabelResolutionRegistry>,
    entity: Entity,
) -> String {
    let Ok(entity_ref) = world.get_entity(entity) else {
        return entity.to_string();
    };
    #[cfg(feature = "remote")]
    let ids = match entity_ref.get::<crate::remote::RemoteComponents>() {
        Some(record) => record.reported().collect(),
        None => entity_ref.archetype().components().to_vec(),
    };
    #[cfg(not(feature = "remote"))]
    let ids = entity_ref.archetype().components().to_vec();

    let component_data: Vec<(ComponentId, String, Option<LabelDefinitionPriority>)> = ids
        .into_iter()
        .map(|component_id| {
            let priority = world
                .components()
                .get_info(component_id)
                .and_then(ComponentInfo::type_id)
                .and_then(|type_id| priorities?.get_priority_by_type_id(type_id));
            (
                component_id,
                crate::component_short_name(world, component_id),
                priority,
            )
        })
        .collect();

    let label_data: Vec<ComponentLabelData> = component_data
        .iter()
        .map(|(component_id, short_name, priority)| ComponentLabelData {
            component_id: *component_id,
            short_name: short_name.as_str(),
            label_definition_priority: *priority,
        })
        .collect();

    resolve_label(world, entity, &label_data)
        .map(|label| label.label.as_str().to_string())
        .unwrap_or_else(|| entity.to_string())
}

fn row_label_entity(world: &World, row: Entity) -> Option<Entity> {
    let header = child_with::<FeathersTreeItemHeader>(world, row)?;
    child_with::<InspectorRowLabel>(world, header)
}

fn child_with<C: Component>(world: &World, entity: Entity) -> Option<Entity> {
    world
        .get::<Children>(entity)?
        .iter()
        .copied()
        .find(|child| {
            world
                .get_entity(*child)
                .is_ok_and(|child| child.contains::<C>())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InspectorPlugin;
    use hearth_app::{App, TaskPoolPlugin};
    use hearth_asset::{AssetApp, AssetPlugin};
    use hearth_ecs::name::Name;

    fn test_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            TaskPoolPlugin::default(),
            hearth_time::TimePlugin,
            AssetPlugin::default(),
            hearth_scene::ScenePlugin,
            InspectorPlugin,
        ));
        app.init_asset::<hearth_image::Image>();
        app.init_asset::<hearth_text::Font>();
        app
    }

    fn row_sources(world: &World, container: Entity) -> Vec<Entity> {
        world
            .get::<Children>(container)
            .map(|children| {
                children
                    .iter()
                    .copied()
                    .filter_map(|child| world.get::<InspectorRow>(child).map(|row| row.source))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn excludes_inspector_ui_descendants_but_shows_the_root() {
        let mut world = World::new();
        let panel = world.spawn(InspectorUi).id();
        let inner = world.spawn(ChildOf(panel)).id();
        let subject = world.spawn_empty().id();

        assert!(!is_excluded(&world, panel));
        assert!(is_excluded(&world, inner));
        assert!(!is_excluded(&world, subject));
    }

    #[test]
    fn nested_inspector_ui_entities_stay_hidden() {
        let mut world = World::new();
        let panel = world.spawn(InspectorUi).id();
        let nested = world.spawn((InspectorUi, ChildOf(panel))).id();
        let nested_inner = world.spawn(ChildOf(nested)).id();

        assert!(!is_excluded(&world, panel));
        assert!(is_excluded(&world, nested));
        assert!(is_excluded(&world, nested_inner));
    }

    #[cfg(feature = "remote")]
    #[test]
    fn shows_the_inspected_world_only() {
        use crate::remote::{
            tests::{apply, row},
            RemoteSource,
        };
        use crate::InspectorSource;
        use serde_json::json;

        let mut app = test_app();
        app.register_type::<ChildOf>().register_type::<Name>();
        app.insert_resource(InspectorSource::Remote(RemoteSource::localhost(1)));
        app.update();

        let panel = app.world_mut().spawn(InspectorUi).id();
        let tree = app
            .world_mut()
            .spawn((InspectorTreeView, ChildOf(panel)))
            .id();
        let local = app.world_mut().spawn(Name::new("Local")).id();
        let parent = Entity::from_raw_u32(40).unwrap();
        let child = Entity::from_raw_u32(41).unwrap();
        apply(
            app.world_mut(),
            alloc::vec![
                row(parent, json!({ "hearth_ecs::name::Name": "Remote" })),
                row(child, json!({ "hearth_ecs::hierarchy::ChildOf": parent })),
            ],
        );
        app.update();

        assert_eq!(row_sources(app.world(), tree), [parent]);
        let row = app.world().resource::<TreeRowIndex>().row(parent).unwrap();
        let label = row_label_entity(app.world(), row).unwrap();
        assert_eq!(app.world().get::<Text>(label).unwrap().0, "Remote");
        assert!(app.world().get::<TreeItem>(row).unwrap().expandable);

        app.insert_resource(InspectorSource::Local);
        app.update();
        app.world_mut().resource_mut::<EntityTreeSync>().set_dirty();
        app.update();
        let sources = row_sources(app.world(), tree);
        assert!(sources.contains(&local));
        assert!(!sources.contains(&parent));
    }

    #[test]
    fn diffs_many_rows_against_the_expected_set() {
        let mut app = test_app();
        let panel = app.world_mut().spawn(InspectorUi).id();
        let tree = app
            .world_mut()
            .spawn((InspectorTreeView, ChildOf(panel)))
            .id();
        let roots: Vec<Entity> = (0..500)
            .map(|index| app.world_mut().spawn(Name::new(format!("{index}"))).id())
            .collect();
        app.update();
        assert_eq!(row_sources(app.world(), tree).len(), 501);

        for root in roots.iter().step_by(2) {
            app.world_mut().entity_mut(*root).despawn();
        }
        app.world_mut().resource_mut::<EntityTreeSync>().set_dirty();
        app.update();

        let sources = row_sources(app.world(), tree);
        assert_eq!(sources.len(), 251);
        assert!(roots
            .iter()
            .skip(1)
            .step_by(2)
            .all(|root| sources.contains(root)));
    }

    #[test]
    fn syncs_root_rows_with_the_world() {
        let mut app = test_app();
        let panel = app.world_mut().spawn(InspectorUi).id();
        let tree = app
            .world_mut()
            .spawn((InspectorTreeView, ChildOf(panel)))
            .id();
        let parent = app.world_mut().spawn(Name::new("Parent")).id();
        app.world_mut().spawn((Name::new("Child"), ChildOf(parent)));
        let sibling = app.world_mut().spawn(Name::new("Sibling")).id();

        app.update();

        let sources = row_sources(app.world(), tree);
        assert_eq!(sources.len(), 3);
        assert!(sources.contains(&panel));
        assert!(sources.contains(&parent));
        assert!(sources.contains(&sibling));

        let panel_row = app.world().resource::<TreeRowIndex>().row(panel).unwrap();
        assert!(!app.world().get::<TreeItem>(panel_row).unwrap().expandable);
        let parent_row = app.world().resource::<TreeRowIndex>().row(parent).unwrap();
        assert!(app.world().get::<TreeItem>(parent_row).unwrap().expandable);
        let sibling_row = app.world().resource::<TreeRowIndex>().row(sibling).unwrap();
        assert!(!app.world().get::<TreeItem>(sibling_row).unwrap().expandable);

        app.world_mut().entity_mut(sibling).despawn();
        app.world_mut().resource_mut::<EntityTreeSync>().set_dirty();
        app.update();

        let sources = row_sources(app.world(), tree);
        assert_eq!(sources.len(), 2);
        assert!(sources.contains(&panel));
        assert!(sources.contains(&parent));
        assert!(app
            .world()
            .resource::<TreeRowIndex>()
            .row(sibling)
            .is_none());
    }

    /// Spawns a tree view under an [`InspectorUi`] panel, observing its own selection requests
    /// like the one [`entity_tree_panel`] spawns.
    fn spawn_tree(app: &mut App) -> Entity {
        let panel = app.world_mut().spawn(InspectorUi).id();
        app.world_mut()
            .spawn((
                InspectorTreeView,
                hearth_ui_widgets::TreeView::default(),
                ChildOf(panel),
            ))
            .observe(tree_view_self_update)
            .observe(inspector_tree_selected)
            .id()
    }

    fn row_of(app: &App, source: Entity) -> Option<Entity> {
        app.world().resource::<TreeRowIndex>().row(source)
    }

    fn tree_selection(app: &App, tree: Entity) -> Option<Entity> {
        app.world()
            .get::<SelectedTreeItem>(tree)
            .and_then(|selected| selected.0)
    }

    fn is_open(app: &App, source: Entity) -> bool {
        row_of(app, source).is_some_and(|row| {
            let row = app.world().entity(row);
            row.contains::<Expanded>() && row.contains::<InspectorRowPopulated>()
        })
    }

    fn select(app: &mut App, entity: Option<Entity>) {
        app.world_mut().resource_mut::<InspectorSelection>().0 = entity;
    }

    fn updates(app: &mut App, count: usize) {
        for _ in 0..count {
            app.update();
        }
    }

    /// A root entity with the children `c`, `a` and `b`, in that order, spawned in another order.
    fn scoped_world(app: &mut App) -> (Entity, [Entity; 3]) {
        let root = app.world_mut().spawn(Name::new("Root")).id();
        let a = app.world_mut().spawn(Name::new("A")).id();
        let b = app.world_mut().spawn(Name::new("B")).id();
        let c = app.world_mut().spawn(Name::new("C")).id();
        for child in [c, a, b] {
            app.world_mut().entity_mut(child).insert(ChildOf(root));
        }
        (root, [c, a, b])
    }

    #[test]
    fn scopes_the_tree_to_the_root_children_in_their_order() {
        let mut app = test_app();
        let tree = spawn_tree(&mut app);
        let (root, children) = scoped_world(&mut app);
        let outside = app.world_mut().spawn(Name::new("Outside")).id();
        app.insert_resource(InspectorTreeRoot(Some(root)));
        app.update();
        assert_eq!(row_sources(app.world(), tree), children);
        assert!(row_of(&app, root).is_none());
        assert!(row_of(&app, outside).is_none());

        app.insert_resource(InspectorTreeRoot(None));
        app.update();
        let sources = row_sources(app.world(), tree);
        assert!(sources.contains(&root));
        assert!(sources.contains(&outside));
        assert!(!sources.contains(&children[0]));

        app.insert_resource(InspectorTreeRoot(Some(outside)));
        app.update();
        assert!(row_sources(app.world(), tree).is_empty());
    }

    #[test]
    fn rows_follow_the_order_of_their_entities() {
        let mut app = test_app();
        let tree = spawn_tree(&mut app);
        let (root, [c, a, b]) = scoped_world(&mut app);
        app.insert_resource(InspectorTreeRoot(Some(root)));
        app.update();

        app.world_mut()
            .entity_mut(root)
            .get_mut::<Children>()
            .unwrap()
            .sort_by_key(|child| child.index());
        let d = app.world_mut().spawn((Name::new("D"), ChildOf(root))).id();
        app.world_mut().resource_mut::<EntityTreeSync>().set_dirty();
        app.update();

        let expected = [a, b, c, d];
        assert_eq!(
            app.world().get::<Children>(root).unwrap().to_vec(),
            expected
        );
        assert_eq!(row_sources(app.world(), tree), expected);
    }

    #[cfg(feature = "remote")]
    #[test]
    fn ignores_the_tree_root_while_remote() {
        use crate::remote::{
            tests::{apply, row},
            RemoteSource,
        };
        use crate::InspectorSource;
        use serde_json::json;

        let mut app = test_app();
        app.register_type::<ChildOf>().register_type::<Name>();
        app.insert_resource(InspectorSource::Remote(RemoteSource::localhost(1)));
        app.update();
        let tree = spawn_tree(&mut app);
        let parent = Entity::from_raw_u32(40).unwrap();
        let child = Entity::from_raw_u32(41).unwrap();
        app.insert_resource(InspectorTreeRoot(Some(parent)));
        apply(
            app.world_mut(),
            alloc::vec![
                row(parent, json!({ "hearth_ecs::name::Name": "Remote" })),
                row(child, json!({ "hearth_ecs::hierarchy::ChildOf": parent })),
            ],
        );
        app.update();
        assert_eq!(row_sources(app.world(), tree), [parent]);
    }

    /// A tree showing `top`, with `top` holding `middle` and `middle` holding `leaf`.
    fn nested_app() -> (App, Entity, [Entity; 3]) {
        let mut app = test_app();
        let tree = spawn_tree(&mut app);
        let top = app.world_mut().spawn(Name::new("Top")).id();
        let middle = app
            .world_mut()
            .spawn((Name::new("Middle"), ChildOf(top)))
            .id();
        let leaf = app
            .world_mut()
            .spawn((Name::new("Leaf"), ChildOf(middle)))
            .id();
        app.update();
        (app, tree, [top, middle, leaf])
    }

    #[test]
    fn reveals_an_outside_selection_from_the_top_down() {
        let (mut app, tree, [top, middle, leaf]) = nested_app();
        assert!(row_of(&app, middle).is_none());

        select(&mut app, Some(leaf));
        app.update();
        assert!(is_open(&app, top));
        assert!(row_of(&app, leaf).is_none());
        assert_eq!(tree_selection(&app, tree), None);

        updates(&mut app, 3);
        assert!(is_open(&app, middle));
        let leaf_row = row_of(&app, leaf).unwrap();
        assert!(!app.world().entity(leaf_row).contains::<Expanded>());
        assert_eq!(tree_selection(&app, tree), Some(leaf_row));

        select(&mut app, Some(top));
        app.update();
        assert_eq!(tree_selection(&app, tree), row_of(&app, top));

        select(&mut app, None);
        app.update();
        assert_eq!(tree_selection(&app, tree), None);
    }

    #[derive(Resource, Default)]
    struct SelectionRequests(usize);

    #[test]
    fn leaves_a_selection_made_in_the_tree_alone() {
        let (mut app, tree, [top, middle, leaf]) = nested_app();
        select(&mut app, Some(leaf));
        updates(&mut app, 4);
        let leaf_row = row_of(&app, leaf).unwrap();
        let middle_row = row_of(&app, middle).unwrap();

        app.init_resource::<SelectionRequests>();
        app.add_observer(
            |_: On<ValueChange<Option<Entity>>>, mut requests: ResMut<SelectionRequests>| {
                requests.0 += 1;
            },
        );
        app.world_mut().trigger(ValueChange {
            source: tree,
            value: Some(middle_row),
            is_final: true,
        });
        app.world_mut().flush();
        assert_eq!(app.world().resource::<InspectorSelection>().0, Some(middle));
        let top_row = row_of(&app, top).unwrap();
        app.world_mut().entity_mut(top_row).remove::<Expanded>();
        let changed = app
            .world()
            .entity(tree)
            .get_ref::<SelectedTreeItem>()
            .unwrap()
            .last_changed();

        updates(&mut app, 4);
        assert_eq!(app.world().resource::<SelectionRequests>().0, 1);
        assert_eq!(tree_selection(&app, tree), Some(middle_row));
        assert_eq!(
            app.world()
                .entity(tree)
                .get_ref::<SelectedTreeItem>()
                .unwrap()
                .last_changed(),
            changed
        );
        assert!(!app
            .world()
            .entity(row_of(&app, top).unwrap())
            .contains::<Expanded>());
        assert_ne!(tree_selection(&app, tree), Some(leaf_row));
    }

    #[test]
    fn retries_a_selection_outside_the_tree_root() {
        let mut app = test_app();
        let tree = spawn_tree(&mut app);
        let (root, [c, ..]) = scoped_world(&mut app);
        let nested = app
            .world_mut()
            .spawn((Name::new("Nested"), ChildOf(c)))
            .id();
        let outside = app.world_mut().spawn(Name::new("Outside")).id();
        app.insert_resource(InspectorTreeRoot(Some(root)));
        app.update();

        select(&mut app, Some(nested));
        updates(&mut app, 3);
        assert!(is_open(&app, c));
        assert_eq!(tree_selection(&app, tree), row_of(&app, nested));

        select(&mut app, Some(outside));
        updates(&mut app, 3);
        assert_eq!(tree_selection(&app, tree), None);
        assert!(row_of(&app, outside).is_none());

        app.world_mut().entity_mut(outside).insert(ChildOf(c));
        app.world_mut().resource_mut::<EntityTreeSync>().set_dirty();
        updates(&mut app, 2);
        assert!(row_of(&app, outside).is_some());
        assert_eq!(tree_selection(&app, tree), row_of(&app, outside));

        select(&mut app, Some(root));
        updates(&mut app, 2);
        assert_eq!(tree_selection(&app, tree), None);
    }

    #[derive(Resource, Default)]
    struct Scrolled(Vec<Entity>);

    #[test]
    fn scrolls_a_revealed_row_into_view_once_it_is_laid_out() {
        let (mut app, _, [_, _, leaf]) = nested_app();
        app.init_resource::<Scrolled>();
        app.add_observer(
            |mut scroll: On<ScrollIntoView>, mut scrolled: ResMut<Scrolled>| {
                scroll.propagate(false);
                scrolled.0.push(scroll.entity);
            },
        );
        select(&mut app, Some(leaf));
        updates(&mut app, 4);
        assert!(app.world().resource::<Scrolled>().0.is_empty());

        let header =
            child_with::<FeathersTreeItemHeader>(app.world(), row_of(&app, leaf).unwrap()).unwrap();
        app.world_mut().entity_mut(header).insert(ComputedNode {
            size: hearth_math::Vec2::new(120.0, 24.0),
            ..Default::default()
        });
        updates(&mut app, 3);
        assert_eq!(app.world().resource::<Scrolled>().0, [header]);
    }
}
