//! The editor's window: the toolbar, the hierarchy, the viewport, the asset browser, the
//! components and inspector panels, and the status bar.

use alloc::{
    borrow::ToOwned,
    format,
    string::{String, ToString},
    vec::Vec,
};

use hearth_app::{App, PostUpdate, Startup, Update};
use hearth_ecs::{
    change_detection::DetectChanges,
    component::Component,
    entity::Entity,
    event::EntityEvent,
    hierarchy::Children,
    name::Name,
    observer::On,
    query::{Changed, With},
    resource::Resource,
    schedule::IntoScheduleConfigs,
    system::{Commands, Query, Res, ResMut, Single},
    world::World,
};
use hearth_feathers::{
    containers::{flex_spacer, pane, pane_body, pane_header, pane_header_divider},
    controls::{
        ButtonVariant, FeathersButton, FeathersMenu, FeathersMenuButton, FeathersMenuDivider,
        FeathersMenuItem, FeathersMenuPopup, FeathersPane, FeathersSplitPane,
        FeathersSplitPaneHandle, FeathersTextInput, FeathersTextInputContainer, FeathersToolButton,
    },
    display::{caption, label, label_dim},
    theme::ThemeBackgroundColor,
    tokens,
};
use hearth_gizmos::transform_gizmo::{
    TransformGizmoMode, TransformGizmoSettings, TransformGizmoSpace,
};
use hearth_input_focus::tab_navigation::TabGroup;
use hearth_inspector::{
    details_panel::details_panel, entity_tree::entity_tree_panel, InspectorSelection,
    InspectorSource,
};
use hearth_picking::Pickable;
use hearth_scene::{bsn, bsn_list, on, Scene, WorldSceneExt};
use hearth_text::{EditableText, TextEdit, TextEditChange};
use hearth_ui::{
    percent, px, widget::Text, AlignItems, FlexDirection, InteractionDisabled, JustifyContent,
    Node, Overflow, PositionType, UiRect,
};
use hearth_ui_widgets::{split_pane_self_update, Activate, DragOverlayRoot};
use hearth_window::{PrimaryWindow, Window};

use crate::{
    assets::{self, AssetBrowser},
    history::{self, History},
    ops::{self, NewObject},
    play::{self, PlayState},
    scene::{self, EditedScene},
    status::Status,
    viewport::{self, ViewportPanel},
};

/// Marker for the toolbar button that puts the gizmo in a mode.
#[derive(Component, Debug, Default, Clone, Copy)]
struct ModeButton(TransformGizmoMode);

/// Marker for the toolbar button that switches the gizmo between world and local axes.
#[derive(Component, Debug, Default, Clone, Copy)]
struct SpaceButton;

/// Marker for the Play button.
#[derive(Component, Debug, Default, Clone, Copy)]
struct PlayButton;

/// Marker for the text naming the scene in the toolbar.
#[derive(Component, Debug, Default, Clone, Copy)]
struct SceneTitle;

/// Marker for the status bar's message.
#[derive(Component, Debug, Default, Clone, Copy)]
struct StatusText;

/// Marker for the status bar's undo hint.
#[derive(Component, Debug, Default, Clone, Copy)]
struct UndoText;

/// Marker for the body of the components panel, rebuilt when the selection changes.
#[derive(Component, Debug, Default, Clone, Copy)]
struct ComponentsBody;

/// Marker for the selection's name field.
#[derive(Component, Debug, Default, Clone, Copy)]
struct NameInput;

/// Marker for the field that filters the components that can be added.
#[derive(Component, Debug, Default, Clone, Copy)]
struct ComponentFilter;

/// Marker for the list of components matching the filter.
#[derive(Component, Debug, Default, Clone, Copy)]
struct ComponentMatches;

/// Marker for the asset browser's list of files.
#[derive(Component, Debug, Default, Clone, Copy)]
pub(crate) struct AssetList;

/// Marker for the asset browser's current folder.
#[derive(Component, Debug, Default, Clone, Copy)]
pub(crate) struct AssetFolderLabel;

/// What the components panel shows, so it is rebuilt only when that changes.
#[derive(Resource, Debug, Default, Clone, PartialEq)]
struct ComponentsShown {
    entity: Option<Entity>,
    components: Vec<ops::ComponentEntry>,
    filter: String,
    /// Whether the panels show the running game, which changes the empty panel's message.
    remote: bool,
}

/// Runs an editor operation with the world, from an observer or system.
fn run(commands: &mut Commands, operation: impl FnOnce(&mut World) + Send + 'static) {
    commands.queue(operation);
}

fn root() -> impl Scene {
    bsn! {
        Name::new("Editor")
        hearth_inspector::entity_tree::InspectorUi
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            flex_direction: FlexDirection::Column,
        }
        TabGroup
        DragOverlayRoot
        Pickable::IGNORE
        Children [
            @toolbar()
            --
            @FeathersSplitPane
            Node {
                flex_grow: 1.0,
                min_height: px(0),
            }
            Pickable::IGNORE
            on(split_pane_self_update)
            Children [
                @FeathersPane { @size: 1.0, @min_size: 200.0 }
                ThemeBackgroundColor(tokens::WINDOW_BG)
                Children [ @hierarchy() ]
                --
                @FeathersSplitPaneHandle
                --
                @FeathersPane { @size: 4.0, @min_size: 300.0 }
                Pickable::IGNORE
                Children [ @center() ]
                --
                @FeathersSplitPaneHandle
                --
                @FeathersPane { @size: 1.4, @min_size: 260.0 }
                ThemeBackgroundColor(tokens::WINDOW_BG)
                Children [ @inspector() ]
            ]
            --
            @status_bar()
        ]
    }
}

fn toolbar() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(4),
            padding: UiRect::axes(px(6), px(4)),
        }
        ThemeBackgroundColor(tokens::PANE_HEADER_BG)
        Children [
            @file_menu()
            --
            @edit_menu()
            --
            @add_menu()
            --
            @pane_header_divider()
            --
            @mode_button("Move", TransformGizmoMode::Translate)
            --
            @mode_button("Rotate", TransformGizmoMode::Rotate)
            --
            @mode_button("Scale", TransformGizmoMode::Scale)
            --
            @FeathersToolButton {
                @caption: bsn! { @caption("World") },
            }
            SpaceButton
            on(|_: On<Activate>, mut settings: ResMut<TransformGizmoSettings>| {
                settings.space = match settings.space {
                    TransformGizmoSpace::World => TransformGizmoSpace::Local,
                    TransformGizmoSpace::Local => TransformGizmoSpace::World,
                };
            })
            --
            @flex_spacer()
            --
            @label("")
            SceneTitle
            --
            @flex_spacer()
            --
            @FeathersButton {
                @caption: bsn! { @caption("Play") },
                @variant: ButtonVariant::Primary,
            }
            PlayButton
            on(|_: On<Activate>, mut commands: Commands| run(&mut commands, play::toggle))
        ]
    }
}

fn mode_button(text: &'static str, mode: TransformGizmoMode) -> impl Scene {
    bsn! {
        @FeathersToolButton {
            @caption: bsn! { @caption(text) },
        }
        ModeButton({mode})
        on(move |_: On<Activate>, mut settings: ResMut<TransformGizmoSettings>| {
            settings.mode = mode;
        })
    }
}

fn file_menu() -> impl Scene {
    bsn! {
        @FeathersMenu
        Children [
            @FeathersMenuButton { @caption: bsn! { @caption("File") } }
            --
            @FeathersMenuPopup
            Children [
                @FeathersMenuItem { @caption: bsn! { @caption("New Scene    Ctrl+N") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, new_scene_checked))
                --
                @FeathersMenuItem { @caption: bsn! { @caption("Save    Ctrl+S") } }
                on(|_: On<Activate>, mut commands: Commands| {
                    run(&mut commands, |world| { scene::save_and_report(world); });
                })
                --
                @FeathersMenuDivider
                --
                @FeathersMenuItem { @caption: bsn! { @caption("Play    F5") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, play::play))
                --
                @FeathersMenuItem { @caption: bsn! { @caption("Stop    Shift+F5") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, play::stop))
            ]
        ]
    }
}

fn edit_menu() -> impl Scene {
    bsn! {
        @FeathersMenu
        Children [
            @FeathersMenuButton { @caption: bsn! { @caption("Edit") } }
            --
            @FeathersMenuPopup
            Children [
                @FeathersMenuItem { @caption: bsn! { @caption("Undo    Ctrl+Z") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, undo))
                --
                @FeathersMenuItem { @caption: bsn! { @caption("Redo    Ctrl+Y") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, redo))
                --
                @FeathersMenuDivider
                --
                @FeathersMenuItem { @caption: bsn! { @caption("Duplicate    Ctrl+D") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, ops::duplicate_selected))
                --
                @FeathersMenuItem { @caption: bsn! { @caption("Delete    Del") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, ops::delete_selected))
                --
                @FeathersMenuItem { @caption: bsn! { @caption("Move to Top Level") } }
                on(|_: On<Activate>, mut commands: Commands| {
                    run(&mut commands, |world| {
                        if let Some(entity) = ops::selected(world) {
                            ops::reparent(world, entity, None);
                        }
                    });
                })
                --
                @FeathersMenuDivider
                --
                @FeathersMenuItem { @caption: bsn! { @caption("Frame Selection    F") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, viewport::frame_selection))
            ]
        ]
    }
}

fn add_menu() -> impl Scene {
    let items: Vec<Box<dyn hearth_scene::SceneList>> = NewObject::ALL
        .iter()
        .map(|(name, object)| -> Box<dyn hearth_scene::SceneList> {
            let object = *object;
            let name = *name;
            Box::new(bsn_list! {
                @FeathersMenuItem { @caption: bsn! { @caption(name) } }
                on(move |_: On<Activate>, mut commands: Commands| {
                    run(&mut commands, move |world| { ops::add_object(world, object); });
                })
            })
        })
        .collect();
    bsn! {
        @FeathersMenu
        Children [
            @FeathersMenuButton { @caption: bsn! { @caption("Add") } }
            --
            @FeathersMenuPopup
            Children [ {items} ]
        ]
    }
}

fn hierarchy() -> impl Scene {
    bsn! {
        @pane()
        Node {
            width: percent(100),
            height: percent(100),
        }
        Children [
            @pane_header() Children [
                @caption("Hierarchy")
                --
                @flex_spacer()
                --
                @FeathersToolButton { @caption: bsn! { @caption("+") } }
                on(|_: On<Activate>, mut commands: Commands| {
                    run(&mut commands, |world| { ops::add_object(world, NewObject::Empty); });
                })
                --
                @FeathersToolButton { @caption: bsn! { @caption("Dup") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, ops::duplicate_selected))
                --
                @FeathersToolButton { @caption: bsn! { @caption("Del") } }
                on(|_: On<Activate>, mut commands: Commands| run(&mut commands, ops::delete_selected))
            ]
            --
            @pane_body()
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                padding: px(0),
            }
            Children [
                @entity_tree_panel()
                Node {
                    width: percent(100),
                    height: percent(100),
                    max_height: percent(100),
                }
            ]
        ]
    }
}

fn center() -> impl Scene {
    bsn! {
        @FeathersSplitPane { @orientation: hearth_ui_widgets::ControlOrientation::Vertical }
        Node {
            width: percent(100),
            height: percent(100),
        }
        Pickable::IGNORE
        on(split_pane_self_update)
        Children [
            @FeathersPane { @size: 3.0, @min_size: 120.0 }
            Pickable::IGNORE
            Children [
                ViewportPanel
                Node {
                    width: percent(100),
                    height: percent(100),
                }
                Pickable::IGNORE
            ]
            --
            @FeathersSplitPaneHandle
            --
            @FeathersPane { @size: 1.0, @min_size: 80.0 }
            ThemeBackgroundColor(tokens::WINDOW_BG)
            Children [ @asset_browser() ]
        ]
    }
}

fn asset_browser() -> impl Scene {
    bsn! {
        @pane()
        Node {
            width: percent(100),
            height: percent(100),
        }
        Children [
            @pane_header() Children [
                @caption("Assets")
                --
                @label_dim("")
                AssetFolderLabel
                --
                @flex_spacer()
                --
                @FeathersToolButton { @caption: bsn! { @caption("Up") } }
                on(|_: On<Activate>, mut browser: ResMut<AssetBrowser>| browser.up())
                --
                @FeathersToolButton { @caption: bsn! { @caption("Refresh") } }
                on(|_: On<Activate>, mut browser: ResMut<AssetBrowser>| browser.rescan_now())
            ]
            --
            @pane_body()
            Node {
                flex_grow: 1.0,
                min_height: px(0),
                overflow: Overflow::scroll_y(),
            }
            Children [
                AssetList
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: hearth_ui::FlexWrap::Wrap,
                    align_content: hearth_ui::AlignContent::FlexStart,
                    column_gap: px(4),
                    row_gap: px(4),
                    width: percent(100),
                }
            ]
        ]
    }
}

fn inspector() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Column,
            width: percent(100),
            height: percent(100),
            min_height: px(0),
        }
        Children [
            @pane()
            Node { width: percent(100) }
            Children [
                @pane_header() Children [ @caption("Entity") ]
                --
                @pane_body()
                ComponentsBody
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(4),
                }
            ]
            --
            @details_panel()
            Node {
                width: percent(100),
                flex_grow: 1.0,
                min_height: px(0),
            }
        ]
    }
}

fn status_bar() -> impl Scene {
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            padding: UiRect::axes(px(8), px(2)),
            min_height: px(22),
        }
        ThemeBackgroundColor(tokens::PANE_HEADER_BG)
        Children [
            @label_dim("")
            StatusText
            --
            @label_dim("")
            UndoText
        ]
    }
}

fn setup(world: &mut World) {
    if let Err(error) = world.spawn_scene(root()) {
        hearth_log::error!("could not build the editor's window: {error}");
    }
}

fn undo(world: &mut World) {
    match history::undo(world) {
        Some(label) => Status::info(world, format!("Undid {label}")),
        None => Status::info(world, "Nothing to undo"),
    }
}

fn redo(world: &mut World) {
    match history::redo(world) {
        Some(label) => Status::info(world, format!("Redid {label}")),
        None => Status::info(world, "Nothing to redo"),
    }
}

/// When a scene with unsaved changes is about to be replaced, and the user was already warned.
#[derive(Resource, Debug, Default, Clone)]
pub(crate) struct DiscardWarning {
    what: String,
    at: f64,
}

/// Whether replacing the scene with `what` may go ahead: it has no unsaved changes, or the user
/// asked for the same thing again within a few seconds of being warned.
pub(crate) fn may_discard(world: &mut World, what: &str) -> bool {
    let dirty = world.resource::<EditedScene>().dirty;
    if !dirty {
        return true;
    }
    let now = world
        .resource::<hearth_time::Time<hearth_time::Real>>()
        .elapsed_secs_f64();
    let warning = world
        .get_resource::<DiscardWarning>()
        .cloned()
        .unwrap_or_default();
    if warning.what == what && now - warning.at < 4.0 {
        world.remove_resource::<DiscardWarning>();
        return true;
    }
    world.insert_resource(DiscardWarning {
        what: what.to_owned(),
        at: now,
    });
    let name = world.resource::<EditedScene>().file_name();
    Status::error(
        world,
        format!("{name} has unsaved changes. Do it again to discard them, or save first (Ctrl+S)"),
    );
    false
}

pub(crate) fn new_scene_checked(world: &mut World) {
    if !may_discard(world, "new scene") {
        return;
    }
    let path = world.resource::<EditedScene>().path.clone();
    scene::new_scene(world, path);
    Status::info(world, "New scene");
}

/// Shows the active gizmo mode and space on the toolbar.
fn update_toolbar(
    settings: Res<TransformGizmoSettings>,
    mut modes: Query<(&ModeButton, &mut ButtonVariant)>,
    space: Query<&Children, With<SpaceButton>>,
    mut texts: Query<&mut Text>,
) {
    if !settings.is_changed() {
        return;
    }
    for (button, mut variant) in &mut modes {
        let wanted = ButtonVariant::selected(button.0 == settings.mode);
        if *variant != wanted {
            *variant = wanted;
        }
    }
    let caption = match settings.space {
        TransformGizmoSpace::World => "World",
        TransformGizmoSpace::Local => "Local",
    };
    for children in &space {
        set_text_below(children, &mut texts, caption);
    }
}

fn set_text_below(children: &Children, texts: &mut Query<&mut Text>, value: &str) {
    for child in children.iter() {
        if let Ok(mut text) = texts.get_mut(*child)
            && text.0 != value
        {
            text.0 = value.to_owned();
        }
    }
}

/// Shows the scene's name, the play state and the status message.
fn update_labels(
    scene: Option<Res<EditedScene>>,
    play: Res<PlayState>,
    status: Res<Status>,
    history: Res<History>,
    source: Res<InspectorSource>,
    titles: Query<Entity, With<SceneTitle>>,
    statuses: Query<Entity, With<StatusText>>,
    undos: Query<Entity, With<UndoText>>,
    play_buttons: Query<(Entity, &Children), With<PlayButton>>,
    mut texts: Query<&mut Text>,
    mut window: Option<Single<&mut Window, With<PrimaryWindow>>>,
    mut commands: Commands,
) {
    let Some(scene) = scene else {
        return;
    };
    let mut title = scene.file_name();
    if scene.dirty {
        title.push_str(" *");
    }
    if scene.is_loading() {
        title.push_str(" (opening)");
    }
    let shown = match *play {
        PlayState::Editing => title.clone(),
        PlayState::Starting => format!("{title}  -  starting the game..."),
        PlayState::Playing { .. } => format!("{title}  -  playing"),
        PlayState::Stopping => format!("{title}  -  stopping..."),
    };
    let mut set = |entity: Entity, value: &str| {
        if let Ok(mut text) = texts.get_mut(entity)
            && text.0 != value
        {
            text.0 = value.to_owned();
        }
    };
    for entity in &titles {
        set(entity, &shown);
    }
    for entity in &statuses {
        set(entity, &status.text);
    }
    let undo_hint = match (&*source, history.undo_label()) {
        (InspectorSource::Remote(_), _) => String::from("Showing the running game"),
        (_, Some(label)) => format!("Undo: {label}"),
        (_, None) => String::new(),
    };
    for entity in &undos {
        set(entity, &undo_hint);
    }
    let caption = match *play {
        PlayState::Editing => "Play",
        PlayState::Starting => "Starting...",
        PlayState::Playing { .. } => "Stop",
        PlayState::Stopping => "Stopping...",
    };
    for (button, children) in &play_buttons {
        for child in children.iter() {
            if let Ok(mut text) = texts.get_mut(*child)
                && text.0 != caption
            {
                text.0 = caption.to_owned();
            }
        }
        if matches!(*play, PlayState::Starting | PlayState::Stopping) {
            commands.entity(button).insert(InteractionDisabled);
        } else {
            commands.entity(button).remove::<InteractionDisabled>();
        }
    }
    if let Some(window) = &mut window {
        let wanted = format!("{title} - Hearth Editor");
        if window.title != wanted {
            window.title = wanted;
        }
    }
}

/// Rebuilds the components panel when the selection, its components or the filter change.
fn rebuild_components_panel(world: &mut World) {
    let entity = world
        .resource::<InspectorSelection>()
        .0
        .filter(|entity| scene::in_scene(world, *entity));
    let components = entity
        .map(|entity| ops::components_of(world, entity))
        .unwrap_or_default();
    let filter = world
        .query_filtered::<&EditableText, With<ComponentFilter>>()
        .iter(world)
        .next()
        .map(|text| text.value().to_string())
        .unwrap_or_default();
    let remote = matches!(
        *world.resource::<InspectorSource>(),
        InspectorSource::Remote(_)
    );
    let shown = ComponentsShown {
        entity,
        components,
        filter,
        remote,
    };
    if world.get_resource::<ComponentsShown>() == Some(&shown) {
        return;
    }
    let previous = world.get_resource::<ComponentsShown>().cloned();
    world.insert_resource(shown.clone());

    // Typing in the filter only changes the matches; keep the filter field and its focus.
    let only_filter = previous.as_ref().is_some_and(|previous| {
        previous.entity == shown.entity
            && previous.components == shown.components
            && previous.remote == shown.remote
    });
    if only_filter {
        rebuild_matches(world, &shown);
        return;
    }

    let Some(body) = world
        .query_filtered::<Entity, With<ComponentsBody>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let old: Vec<Entity> = world
        .get::<Children>(body)
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    for child in old {
        if let Ok(child) = world.get_entity_mut(child) {
            child.despawn();
        }
    }

    let Some(entity) = shown.entity else {
        let message = match *world.resource::<InspectorSource>() {
            InspectorSource::Remote(_) => "The running game's entities are shown below",
            InspectorSource::Local => "Select an entity in the hierarchy or the viewport",
        };
        spawn_into(world, body, bsn! { @label_dim(message) });
        return;
    };

    let name = world
        .get::<Name>(entity)
        .map(|name| name.as_str().to_owned())
        .unwrap_or_default();
    spawn_into(
        world,
        body,
        bsn! {
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(6),
            }
            Children [
                @label("Name")
                --
                @FeathersTextInputContainer
                Node { flex_grow: 1.0 }
                Children [
                    @FeathersTextInput { @visible_width: 18f32 }
                    NameInput
                    on(move |change: On<TextEditChange>, inputs: Query<&EditableText, With<NameInput>>, mut commands: Commands| {
                        let Ok(text) = inputs.get(change.event_target()) else {
                            return;
                        };
                        let name = text.value().to_string();
                        run(&mut commands, move |world| ops::rename(world, entity, &name));
                    })
                ]
            ]
        },
    );
    set_text(world, NameInput, &name);

    for component in &shown.components {
        let text = component.name.clone();
        let path = component.path.clone();
        spawn_into(
            world,
            body,
            bsn! {
                Node {
                    flex_direction: FlexDirection::Row,
                    align_items: AlignItems::Center,
                    column_gap: px(4),
                }
                Children [
                    @label(text)
                    --
                    @flex_spacer()
                    --
                    @FeathersToolButton { @caption: bsn! { @caption("Remove") }, @variant: ButtonVariant::Plain }
                    on(move |_: On<Activate>, mut commands: Commands| {
                        let path = path.clone();
                        run(&mut commands, move |world| ops::remove_component(world, entity, &path));
                    })
                ]
            },
        );
    }

    spawn_into(
        world,
        body,
        bsn! {
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(6),
                margin: UiRect::top(px(4)),
            }
            Children [
                @label("Add")
                --
                @FeathersTextInputContainer
                Node { flex_grow: 1.0 }
                Children [
                    @FeathersTextInput { @visible_width: 18f32 }
                    ComponentFilter
                ]
            ]
        },
    );
    spawn_into(
        world,
        body,
        bsn! {
            ComponentMatches
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(2),
            }
        },
    );
    rebuild_matches(world, &shown);
}

/// Lists the components matching the filter that can be added to the selection.
fn rebuild_matches(world: &mut World, shown: &ComponentsShown) {
    let Some(list) = world
        .query_filtered::<Entity, With<ComponentMatches>>()
        .iter(world)
        .next()
    else {
        return;
    };
    let old: Vec<Entity> = world
        .get::<Children>(list)
        .map(|children| children.iter().copied().collect())
        .unwrap_or_default();
    for child in old {
        if let Ok(child) = world.get_entity_mut(child) {
            child.despawn();
        }
    }
    let Some(entity) = shown.entity else {
        return;
    };
    let filter = shown.filter.trim().to_lowercase();
    if filter.is_empty() {
        spawn_into(
            world,
            list,
            bsn! { @label_dim("Type to find a component to add") },
        );
        return;
    }
    let matches: Vec<ops::ComponentEntry> = ops::addable_components(world, entity)
        .into_iter()
        .filter(|entry| entry.path.to_lowercase().contains(&filter))
        .take(12)
        .collect();
    if matches.is_empty() {
        spawn_into(world, list, bsn! { @label_dim("No component matches") });
    }
    for entry in matches {
        let text = entry.name.clone();
        let path = entry.path.clone();
        spawn_into(
            world,
            list,
            bsn! {
                @FeathersButton { @caption: bsn! { @caption({text}) }, @variant: ButtonVariant::Plain }
                Node { justify_content: JustifyContent::FlexStart }
                on(move |_: On<Activate>, mut commands: Commands| {
                    let path = path.clone();
                    run(&mut commands, move |world| {
                        ops::add_component(world, entity, &path);
                        // Clear the filter, which rebuilds the panel.
                        set_text(world, ComponentFilter, "");
                    });
                })
            },
        );
    }
}

/// Spawns `scene` as the last child of `parent`.
pub(crate) fn spawn_into(world: &mut World, parent: Entity, scene: impl Scene) {
    match world.spawn_scene(scene) {
        Ok(mut entity) => {
            entity.insert(hearth_ecs::hierarchy::ChildOf(parent));
        }
        Err(error) => hearth_log::warn!("could not build part of the editor's window: {error}"),
    }
}

/// Replaces the text of the input marked `M`.
fn set_text<M: Component>(world: &mut World, _marker: M, value: &str) {
    let inputs: Vec<Entity> = world
        .query_filtered::<Entity, With<M>>()
        .iter(world)
        .collect();
    for input in inputs {
        if let Some(mut text) = world.get_mut::<EditableText>(input) {
            text.queue_edit(TextEdit::SelectAll);
            text.queue_edit(TextEdit::Insert(value.into()));
        }
    }
}

/// Records inspector field edits for undo, merging a run of edits to one component into one.
fn record_field_edits(
    edit: On<hearth_inspector::details_panel::FieldEdit>,
    source: Res<InspectorSource>,
    mut commands: Commands,
) {
    if !matches!(*source, InspectorSource::Local) {
        return;
    }
    let entity = edit.entity;
    let label = format!("Edit {}", ops::short_name(&edit.component));
    run(&mut commands, move |world| {
        if !scene::in_scene(world, entity) {
            return;
        }
        if !world.resource::<History>().is_pending(&label, &[entity]) {
            history::begin(world, label, &[entity]);
        }
        world.resource_mut::<History>().touch();
    });
}

/// Keeps the name field in step when the name changes elsewhere, such as by undo.
fn sync_name_field(
    selection: Res<InspectorSelection>,
    names: Query<&Name, Changed<Name>>,
    inputs: Query<(Entity, &EditableText), With<NameInput>>,
    focus: Res<hearth_input_focus::InputFocus>,
    mut commands: Commands,
) {
    let Some(selected) = selection.0 else {
        return;
    };
    let Ok(name) = names.get(selected) else {
        return;
    };
    for (input, text) in &inputs {
        // Don't fight the user while they type.
        if focus.get() == Some(input) || text.value() == name.as_str() {
            continue;
        }
        let value = name.as_str().to_owned();
        commands.queue(move |world: &mut World| {
            if let Some(mut text) = world.get_mut::<EditableText>(input) {
                text.queue_edit(TextEdit::SelectAll);
                text.queue_edit(TextEdit::Insert(value.into()));
            }
        });
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                update_toolbar,
                update_labels,
                sync_name_field,
                assets::rebuild_asset_list,
            ),
        )
        .add_systems(
            PostUpdate,
            rebuild_components_panel.before(hearth_ui::UiSystems::Prepare),
        )
        .add_observer(record_field_edits);
}
