//! The components of the selected remote entity, fetched for the details panel, and the edits
//! made in the details panel, sent back to the remote app.

use alloc::{string::ToString, vec::Vec};
use core::time::Duration;

use bevy_ecs::{
    entity::Entity,
    resource::Resource,
    system::{Res, ResMut},
    world::World,
};
use bevy_log::{debug, warn};
use bevy_platform::collections::HashMap;
use bevy_remote::{
    builtin_methods::{
        BrpGetComponentsParams, BrpGetComponentsResponse, BrpListComponentsParams,
        BrpListComponentsResponse, BrpMutateComponentsParams, BRP_GET_COMPONENTS_METHOD,
        BRP_LIST_COMPONENTS_METHOD, BRP_MUTATE_COMPONENTS_METHOD,
    },
    client::{BrpClient, BrpClientError},
};
use bevy_tasks::{block_on, poll_once, Task};
use bevy_time::{Real, Time};
use serde_json::Value;

use super::{
    source::{spawn, POLL_INTERVAL, REQUEST_TIMEOUT, RETRY_INTERVAL},
    world::{FetchedComponents, RemoteWorld},
    RemoteConnection, RemoteConnectionState,
};
use crate::{
    details_panel::{DetailsPanelSync, FieldEdit},
    InspectorSelection,
};

/// The fetches of the selected remote entity's components, and the edits sent to the remote app.
///
/// While a mirrored remote entity is selected, its components are listed with
/// `world.list_components` and fetched with `world.get_components` at the poll interval of the
/// entity tree, then read back into its [`RemoteValues`](super::RemoteValues), which the details
/// panel shows.
///
/// Edits are sent with `world.mutate_components`, at most one per field at a time: while an edit
/// of a field is in flight, only the latest later edit of that field is kept. No fetch runs while
/// edits are in flight, so that no answer reports a value from before an edit, and the components
/// are fetched again once the edits are answered.
#[derive(Resource, Default)]
pub struct RemoteDetails {
    entity: Option<Entity>,
    fetch: Option<PendingFetch>,
    next_fetch: Duration,
    sent: Vec<PendingEdit>,
    queued: Vec<BrpMutateComponentsParams>,
}

struct PendingFetch {
    entity: Entity,
    task: Task<Result<FetchedComponents, BrpClientError>>,
    started: Duration,
}

struct PendingEdit {
    params: BrpMutateComponentsParams,
    task: Task<Result<Value, BrpClientError>>,
    started: Duration,
}

impl RemoteDetails {
    /// The remote entity whose components are fetched.
    pub fn entity(&self) -> Option<Entity> {
        self.entity
    }

    /// Whether a fetch of the selected entity's components is in flight.
    pub fn is_fetching(&self) -> bool {
        self.fetch.is_some()
    }

    /// The number of edits sent to the remote app and not answered yet, or waiting to be sent.
    pub fn pending_edits(&self) -> usize {
        self.sent.len() + self.queued.len()
    }

    /// Queues `params` to be sent, replacing a queued edit of the same field, and drops the fetch
    /// in flight, whose answer may predate the edit.
    fn queue(&mut self, params: BrpMutateComponentsParams) {
        self.fetch = None;
        match self
            .queued
            .iter_mut()
            .find(|queued| same_field(queued, &params))
        {
            Some(queued) => *queued = params,
            None => self.queued.push(params),
        }
    }

    /// Handles the edits that were answered or timed out, warning about the failed ones. Fetches
    /// again at `now` once every edit is answered.
    fn finish_edits(&mut self, now: Duration) {
        if self.sent.is_empty() {
            return;
        }
        self.sent.retain_mut(|edit| {
            let error = match block_on(poll_once(&mut edit.task)) {
                Some(Ok(_)) => None,
                Some(Err(error)) => Some(error.to_string()),
                None if now.saturating_sub(edit.started) >= REQUEST_TIMEOUT => {
                    Some("the remote app did not answer in time".to_string())
                }
                None => return true,
            };
            if let Some(error) = error {
                warn!(
                    "the remote app did not apply the edit of `{}` at `{}`: {error}",
                    edit.params.component, edit.params.path
                );
            }
            false
        });
        if self.sent.is_empty() && self.queued.is_empty() {
            self.next_fetch = now;
        }
    }

    /// Sends the queued edits whose field has no edit in flight.
    fn send_edits(&mut self, client: &BrpClient, now: Duration) {
        let mut index = 0;
        while index < self.queued.len() {
            if self
                .sent
                .iter()
                .any(|sent| same_field(&sent.params, &self.queued[index]))
            {
                index += 1;
                continue;
            }
            let params = self.queued.remove(index);
            let request = serde_json::to_value(&params).unwrap_or_default();
            let client = client.clone();
            self.sent.push(PendingEdit {
                params,
                task: spawn(async move {
                    client
                        .call(BRP_MUTATE_COMPONENTS_METHOD, Some(request))
                        .await
                }),
                started: now,
            });
        }
    }

    /// The answer of the fetch in flight, with the entity it is for, if it arrived.
    fn finish_fetch(&mut self, now: Duration) -> Option<(Entity, FetchedComponents)> {
        let mut fetch = self.fetch.take()?;
        let elapsed = now.saturating_sub(fetch.started);
        match block_on(poll_once(&mut fetch.task)) {
            Some(Ok(fetched)) => {
                self.next_fetch = now + POLL_INTERVAL.max(elapsed * 2);
                Some((fetch.entity, fetched))
            }
            Some(Err(error)) => {
                debug!(
                    "the remote inspector could not fetch the components of {}: {error}",
                    fetch.entity
                );
                self.next_fetch = now + POLL_INTERVAL;
                None
            }
            None if elapsed >= REQUEST_TIMEOUT => {
                self.next_fetch = now + RETRY_INTERVAL;
                None
            }
            None => {
                self.fetch = Some(fetch);
                None
            }
        }
    }

    /// Starts fetching the components of the selected entity if it is mirrored and a fetch is
    /// due.
    fn start_fetch(&mut self, remote: &RemoteWorld, client: &BrpClient, now: Duration) {
        let Some(entity) = self.entity else {
            return;
        };
        if self.fetch.is_some()
            || !self.sent.is_empty()
            || !self.queued.is_empty()
            || now < self.next_fetch
            || !remote.contains(entity)
        {
            return;
        }
        self.fetch = Some(PendingFetch {
            entity,
            task: spawn(fetch_components(client.clone(), entity)),
            started: now,
        });
    }
}

/// Whether two edits write the same field.
fn same_field(left: &BrpMutateComponentsParams, right: &BrpMutateComponentsParams) -> bool {
    left.entity == right.entity && left.component == right.component && left.path == right.path
}

/// Lists the components of `entity`, and fetches every one of them.
async fn fetch_components(
    client: BrpClient,
    entity: Entity,
) -> Result<FetchedComponents, BrpClientError> {
    let listed = client
        .call(
            BRP_LIST_COMPONENTS_METHOD,
            Some(serde_json::to_value(BrpListComponentsParams { entity })?),
        )
        .await?;
    let components: BrpListComponentsResponse = serde_json::from_value(listed)?;
    let params = BrpGetComponentsParams {
        entity,
        components,
        strict: false,
    };
    let fetched = client
        .call(
            BRP_GET_COMPONENTS_METHOD,
            Some(serde_json::to_value(params)?),
        )
        .await?;
    Ok(match serde_json::from_value(fetched)? {
        BrpGetComponentsResponse::Lenient { components, errors } => {
            FetchedComponents { components, errors }
        }
        BrpGetComponentsResponse::Strict(components) => FetchedComponents {
            components,
            errors: HashMap::default(),
        },
    })
}

/// Fetches the components of the selected remote entity when due and reads the answers back into
/// the [`RemoteWorld`], and sends the queued edits to the remote app.
///
/// See [`RemoteDetails`].
pub fn poll_remote_details(
    time: Option<Res<Time<Real>>>,
    selection: Res<InspectorSelection>,
    connection: Res<RemoteConnection>,
    mut details: ResMut<RemoteDetails>,
    mut remote: ResMut<RemoteWorld>,
    mut panel: ResMut<DetailsPanelSync>,
) {
    let now = time.map(|time| time.elapsed()).unwrap_or_default();
    let details = &mut *details;
    details.finish_edits(now);
    let (RemoteConnectionState::Connected { .. }, Some(client)) =
        (&connection.state, connection.client())
    else {
        details.fetch = None;
        return;
    };
    if details.entity != selection.0 {
        if let Some(previous) = details.entity {
            remote.clear_values(previous);
        }
        details.entity = selection.0;
        details.fetch = None;
        details.next_fetch = now;
    }
    details.send_edits(client, now);
    if let Some((entity, fetched)) = details.finish_fetch(now)
        && remote.write_values(entity, fetched)
    {
        panel.set_dirty();
    }
    details.start_fetch(&remote, client, now);
}

/// Writes `edit` into the values the details panel shows for the remote entity, and queues it to
/// be sent to the remote app.
///
/// An edit that cannot be applied logs a warning and is not sent. The details panel is refreshed
/// whenever the field does not end up holding the edited value, so that its widget is reverted.
pub(crate) fn send_field_edit(world: &mut World, edit: &FieldEdit) {
    let Some(applied) = world
        .get_resource_mut::<RemoteWorld>()
        .map(|mut remote| remote.edit_value(edit))
    else {
        return;
    };
    if let Some(warning) = &applied.warning {
        warn!(
            "the inspector cannot edit `{}` at `{}` on the remote app: {warning}",
            edit.component, edit.path
        );
    }
    if let Some(value) = applied.send
        && let Some(mut details) = world.get_resource_mut::<RemoteDetails>()
    {
        details.queue(BrpMutateComponentsParams {
            entity: edit.entity,
            component: edit.component.clone(),
            path: edit.path.clone(),
            value,
        });
    }
    if !applied.held
        && let Some(mut sync) = world.get_resource_mut::<DetailsPanelSync>()
    {
        sync.set_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        details_panel::{inspect_components, FieldKind, FieldValue},
        remote::{
            tests::{apply, mirrored, remote, row, test_world},
            world::ValueEdit,
        },
    };
    use alloc::{borrow::Cow, string::String};
    use bevy_color::Color;
    use bevy_ecs::{
        component::Component,
        reflect::{AppTypeRegistry, ReflectComponent},
    };
    use bevy_reflect::{prelude::ReflectDefault, Reflect, TypePath};
    use bevy_ui_widgets::NumericValue;
    use serde_json::json;

    #[derive(Component, Reflect, Debug, Default, Clone, PartialEq)]
    #[reflect(Component, Default)]
    struct Health {
        value: f32,
        label: String,
    }

    #[derive(Component, Reflect, Debug, Default)]
    #[component(immutable)]
    #[reflect(Component, Default)]
    struct Frozen {
        value: f32,
    }

    #[derive(Reflect, Debug, Default, Clone, Copy, PartialEq)]
    enum Mode {
        #[default]
        Idle,
        Running,
    }

    #[derive(Component, Reflect, Debug, Default, Clone, PartialEq)]
    #[reflect(Component, Default)]
    struct Kinds {
        flag: bool,
        byte: u8,
        short: i16,
        word: u32,
        long: i64,
        wide: u64,
        size: usize,
        single: f32,
        double: f64,
        text: String,
        cow: Cow<'static, str>,
        letter: char,
        color: Color,
        mode: Mode,
    }

    fn registered_world() -> World {
        let world = test_world();
        {
            let registry = world.resource::<AppTypeRegistry>();
            let mut registry = registry.write();
            registry.register::<Health>();
            registry.register::<Frozen>();
            registry.register::<Kinds>();
        }
        world
    }

    /// A world mirroring the remote entity `5v0`, named `Target`.
    fn mirrored_world() -> (World, Entity) {
        let mut world = registered_world();
        let entity = remote(5);
        apply(
            &mut world,
            alloc::vec![row(entity, json!({ "bevy_ecs::name::Name": "Target" }))],
        );
        (world, entity)
    }

    fn fetched(components: Value, errors: Value) -> FetchedComponents {
        FetchedComponents {
            components: serde_json::from_value(components).unwrap(),
            errors: serde_json::from_value(errors).unwrap(),
        }
    }

    fn write(world: &mut World, entity: Entity, fetched: FetchedComponents) -> bool {
        world
            .resource_mut::<RemoteWorld>()
            .write_values(entity, fetched)
    }

    fn health(value: f32) -> Value {
        json!({ Health::type_path(): { "value": value, "label": "hp" } })
    }

    fn reflected<T: Reflect + TypePath + Clone>(world: &World, entity: Entity) -> Option<T> {
        world
            .resource::<RemoteWorld>()
            .reflected(entity, T::type_path())?
            .try_downcast_ref::<T>()
            .cloned()
    }

    #[test]
    fn reads_reported_components_back_through_the_local_registry() {
        let (mut world, entity) = mirrored_world();
        let components = json!({
            Health::type_path(): { "value": 1.5, "label": "hp" },
            "bevy_ecs::name::Name": "Target",
            "demo::Unregistered": 3,
        });
        let errors = json!({
            "demo::Unserializable": {
                "code": -23402,
                "message": "Component `demo::Unserializable` could not be serialized",
            },
        });
        assert!(write(
            &mut world,
            entity,
            fetched(components.clone(), errors.clone())
        ));
        assert_eq!(
            reflected::<Health>(&world, entity),
            Some(Health {
                value: 1.5,
                label: "hp".to_string(),
            })
        );

        let details = inspect_components(mirrored(&world), Some(entity));
        let names: Vec<&str> = details.iter().map(|group| group.name.as_str()).collect();
        assert_eq!(names, ["Health", "Name", "Unknown components"]);
        let kinds: Vec<(&str, FieldKind)> = details[0]
            .fields
            .iter()
            .map(|field| (field.path.as_str(), field.value.kind()))
            .collect();
        assert_eq!(
            kinds,
            [("value", FieldKind::Number), ("label", FieldKind::Text)]
        );
        let unknown: Vec<(&str, &FieldValue)> = details[2]
            .fields
            .iter()
            .map(|field| (field.label.as_str(), &field.value))
            .collect();
        assert_eq!(unknown.len(), 2);
        assert_eq!(unknown[0].0, "Unregistered");
        assert_eq!(
            unknown[1],
            (
                "Unserializable",
                &FieldValue::Label(
                    "Component `demo::Unserializable` could not be serialized".to_string()
                )
            )
        );

        assert!(!write(&mut world, entity, fetched(components, errors)));
        assert!(write(&mut world, entity, fetched(health(2.0), json!({}))));
        assert_eq!(reflected::<Health>(&world, entity).unwrap().value, 2.0);
        let details = inspect_components(mirrored(&world), Some(entity));
        assert_eq!(details.len(), 1);
    }

    #[test]
    fn values_are_dropped_with_the_selection() {
        let (mut world, entity) = mirrored_world();
        write(&mut world, entity, fetched(health(1.0), json!({})));
        world.resource_mut::<RemoteWorld>().clear_values(entity);
        assert!(reflected::<Health>(&world, entity).is_none());
        let names: Vec<String> = inspect_components(mirrored(&world), Some(entity))
            .into_iter()
            .map(|group| group.name)
            .collect();
        assert_eq!(names, ["Name"]);
        assert!(!write(
            &mut world,
            remote(6),
            fetched(health(1.0), json!({}))
        ));
    }

    #[test]
    fn immutable_remote_components_are_read_only() {
        let (mut world, entity) = mirrored_world();
        let frozen = json!({ Frozen::type_path(): { "value": 1.0 } });
        write(&mut world, entity, fetched(frozen, json!({})));
        let details = inspect_components(mirrored(&world), Some(entity));
        assert!(details[0]
            .fields
            .iter()
            .all(|field| field.value.kind() == FieldKind::Label));

        let edit = world.resource_mut::<RemoteWorld>().edit_value(&FieldEdit {
            entity,
            component: Frozen::type_path().to_string(),
            path: "value".to_string(),
            value: FieldValue::Number(NumericValue::F32(2.0)),
        });
        assert_eq!(edit.send, None);
        assert!(edit.warning.is_some());
    }

    fn kinds_world() -> (World, Entity) {
        let (mut world, entity) = mirrored_world();
        let registry = world.resource::<AppTypeRegistry>().clone();
        let value = serde_json::to_value(bevy_reflect::serde::TypedReflectSerializer::new(
            &Kinds::default(),
            &registry.read(),
        ))
        .unwrap();
        write(
            &mut world,
            entity,
            fetched(json!({ Kinds::type_path(): value }), json!({})),
        );
        (world, entity)
    }

    fn edit_kinds(world: &mut World, entity: Entity, path: &str, value: FieldValue) -> ValueEdit {
        world.resource_mut::<RemoteWorld>().edit_value(&FieldEdit {
            entity,
            component: Kinds::type_path().to_string(),
            path: path.to_string(),
            value,
        })
    }

    #[test]
    fn edits_send_the_field_in_its_own_type() {
        let (mut world, entity) = kinds_world();
        // The number inputs of integers narrower than `i32` report `i32` values.
        let narrow = |value| FieldValue::Number(NumericValue::I32(value));
        let int = |value| FieldValue::Number(NumericValue::I64(value));
        let text = |value: &str| FieldValue::Text(value.to_string());
        // Whether the field ends up holding the edited value, which a typed `char` and a variant
        // never do, so that their widgets are refreshed.
        let cases = [
            ("flag", FieldValue::Bool(true), json!(true), true),
            ("byte", narrow(200), json!(200), true),
            ("short", narrow(-300), json!(-300), true),
            ("word", int(70_000), json!(70_000), true),
            ("long", int(-5_000_000_000), json!(-5_000_000_000_i64), true),
            ("wide", int(5_000_000_000), json!(5_000_000_000_u64), true),
            ("size", int(12), json!(12), true),
            (
                "single",
                FieldValue::Number(NumericValue::F32(2.5)),
                json!(2.5),
                true,
            ),
            (
                "double",
                FieldValue::Number(NumericValue::F64(0.125)),
                json!(0.125),
                true,
            ),
            ("text", text("hello"), json!("hello"), true),
            ("cow", text("borrowed"), json!("borrowed"), true),
            ("letter", text("ab"), json!("b"), false),
            (
                "mode",
                FieldValue::Variant {
                    variants: alloc::vec!["Idle".to_string(), "Running".to_string()],
                    selected: 1,
                },
                json!("Running"),
                false,
            ),
        ];
        for (path, value, expected, held) in cases {
            let edit = edit_kinds(&mut world, entity, path, value);
            assert_eq!(edit.send, Some(expected), "editing `{path}`");
            assert_eq!(edit.warning, None, "editing `{path}`");
            assert_eq!(edit.held, held, "editing `{path}`");
        }

        let edit = edit_kinds(
            &mut world,
            entity,
            "color",
            FieldValue::Color(Color::srgb(1.0, 0.0, 0.0)),
        );
        // The default color is linear, and edits keep the color space of the field.
        let sent = edit.send.unwrap();
        assert_eq!(sent["LinearRgba"]["red"], json!(1.0), "{sent}");
        assert_eq!(sent["LinearRgba"]["green"], json!(0.0), "{sent}");

        let kinds = reflected::<Kinds>(&world, entity).unwrap();
        assert!(kinds.flag);
        assert_eq!(kinds.byte, 200);
        assert_eq!(kinds.long, -5_000_000_000);
        assert_eq!(kinds.letter, 'b');
        assert_eq!(kinds.cow, "borrowed");
        assert_eq!(kinds.mode, Mode::Running);
        assert_eq!(bevy_color::Srgba::from(kinds.color).to_hex(), "#FF0000");
    }

    #[test]
    fn rejected_and_unchanged_edits_are_not_sent() {
        let (mut world, entity) = kinds_world();
        let out_of_range = edit_kinds(
            &mut world,
            entity,
            "byte",
            FieldValue::Number(NumericValue::I64(300)),
        );
        assert_eq!(out_of_range.send, None);
        assert!(out_of_range.warning.is_some());
        assert!(!out_of_range.held);

        let unchanged = edit_kinds(&mut world, entity, "flag", FieldValue::Bool(false));
        assert_eq!(
            unchanged,
            ValueEdit {
                held: true,
                ..ValueEdit::default()
            }
        );

        let missing = edit_kinds(&mut world, entity, "missing", FieldValue::Bool(true));
        assert!(missing.warning.is_some());

        let empty_char = edit_kinds(
            &mut world,
            entity,
            "letter",
            FieldValue::Text(String::new()),
        );
        assert_eq!(empty_char.warning, None);
        assert!(!empty_char.held);
    }

    #[test]
    fn an_edited_component_is_read_back_from_the_next_report() {
        let (mut world, entity) = mirrored_world();
        write(&mut world, entity, fetched(health(1.0), json!({})));
        let edit = world.resource_mut::<RemoteWorld>().edit_value(&FieldEdit {
            entity,
            component: Health::type_path().to_string(),
            path: "value".to_string(),
            value: FieldValue::Number(NumericValue::F32(4.0)),
        });
        assert_eq!(edit.send, Some(json!(4.0)));
        assert_eq!(reflected::<Health>(&world, entity).unwrap().value, 4.0);

        // The remote app rejected the edit, and reports the old value again.
        assert!(write(&mut world, entity, fetched(health(1.0), json!({}))));
        assert_eq!(reflected::<Health>(&world, entity).unwrap().value, 1.0);
    }

    fn params(path: &str, value: Value) -> BrpMutateComponentsParams {
        BrpMutateComponentsParams {
            entity: remote(5),
            component: Health::type_path().to_string(),
            path: path.to_string(),
            value,
        }
    }

    #[test]
    fn queued_edits_keep_the_latest_value_of_each_field() {
        let mut details = RemoteDetails {
            fetch: Some(PendingFetch {
                entity: remote(5),
                task: spawn(core::future::pending()),
                started: Duration::ZERO,
            }),
            ..RemoteDetails::default()
        };
        details.queue(params("value", json!(1.0)));
        assert!(!details.is_fetching());
        details.queue(params("value", json!(2.0)));
        details.queue(params("label", json!("hp")));
        assert_eq!(
            details.queued,
            [params("value", json!(2.0)), params("label", json!("hp"))]
        );
    }

    #[test]
    fn an_edit_waits_for_the_edit_of_its_field_in_flight() {
        let mut details = RemoteDetails {
            sent: alloc::vec![PendingEdit {
                params: params("value", json!(1.0)),
                task: spawn(core::future::pending()),
                started: Duration::ZERO,
            }],
            ..RemoteDetails::default()
        };
        details.queue(params("value", json!(2.0)));
        details.queue(params("label", json!("hp")));
        let client = BrpClient::localhost(1);
        details.send_edits(&client, Duration::ZERO);
        assert_eq!(details.queued, [params("value", json!(2.0))]);
        assert_eq!(details.pending_edits(), 3);

        details.sent[0].task = spawn(async { Ok(Value::Null) });
        details.finish_edits(Duration::from_secs(1));
        details.send_edits(&client, Duration::from_secs(1));
        assert!(details.queued.is_empty());
        assert!(details
            .sent
            .iter()
            .any(|sent| sent.params == params("value", json!(2.0))));
    }

    #[test]
    fn unanswered_edits_time_out() {
        let mut details = RemoteDetails {
            sent: alloc::vec![PendingEdit {
                params: params("value", json!(1.0)),
                task: spawn(core::future::pending()),
                started: Duration::ZERO,
            }],
            next_fetch: Duration::from_secs(60),
            ..RemoteDetails::default()
        };
        details.finish_edits(REQUEST_TIMEOUT - Duration::from_millis(1));
        assert_eq!(details.pending_edits(), 1);
        details.finish_edits(REQUEST_TIMEOUT);
        assert_eq!(details.pending_edits(), 0);
        assert_eq!(details.next_fetch, REQUEST_TIMEOUT);
    }

    /// Tests against a second app serving the Bevy Remote Protocol on a free port.
    mod over_brp {
        use super::*;
        use crate::{remote::RemoteSource, InspectorPlugin, InspectorSource};
        use bevy_app::{App, TaskPoolPlugin};
        use bevy_asset::{AssetApp, AssetPlugin};
        use bevy_ecs::name::Name;
        use bevy_remote::{http::RemoteHttpPlugin, RemotePlugin};
        use std::{net::TcpListener, time::Instant};

        fn free_port() -> u16 {
            TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        }

        /// An app serving the Bevy Remote Protocol on `port`, with one entity holding a
        /// [`Health`].
        fn target_app(port: u16) -> (App, Entity) {
            let mut app = App::new();
            app.add_plugins((
                TaskPoolPlugin::default(),
                RemotePlugin::default(),
                RemoteHttpPlugin::default().with_port(port),
            ));
            app.register_type::<Health>().register_type::<Name>();
            let entity = app
                .world_mut()
                .spawn((
                    Name::new("Target"),
                    Health {
                        value: 1.0,
                        label: "hp".to_string(),
                    },
                ))
                .id();
            app.finish();
            app.cleanup();
            app.update();
            (app, entity)
        }

        fn inspector_app(port: u16) -> App {
            let mut app = App::new();
            app.add_plugins((
                TaskPoolPlugin::default(),
                bevy_time::TimePlugin,
                AssetPlugin::default(),
                bevy_scene::ScenePlugin,
                InspectorPlugin,
            ));
            app.init_asset::<bevy_image::Image>();
            app.init_asset::<bevy_text::Font>();
            app.register_type::<Health>().register_type::<Name>();
            app.insert_resource(InspectorSource::Remote(RemoteSource::localhost(port)));
            app
        }

        /// Updates both apps until `done` holds, failing after a while.
        fn run_until(target: &mut App, inspector: &mut App, done: impl Fn(&App, &App) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(30);
            while !done(target, inspector) {
                assert!(Instant::now() < deadline, "the remote app never caught up");
                target.update();
                inspector.update();
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        fn remote_health(inspector: &App, entity: Entity) -> Option<f32> {
            Some(reflected::<Health>(inspector.world(), entity)?.value)
        }

        fn edit_health(inspector: &mut App, entity: Entity, value: f32) {
            inspector.world_mut().trigger(FieldEdit {
                entity,
                component: Health::type_path().to_string(),
                path: "value".to_string(),
                value: FieldValue::Number(NumericValue::F32(value)),
            });
            inspector.world_mut().flush();
        }

        #[test]
        fn fetches_and_edits_the_selected_remote_entity() {
            let port = free_port();
            let (mut target, entity) = target_app(port);
            let mut inspector = inspector_app(port);

            run_until(&mut target, &mut inspector, |_, inspector| {
                inspector.world().resource::<RemoteWorld>().contains(entity)
            });
            inspector.world_mut().resource_mut::<InspectorSelection>().0 = Some(entity);
            run_until(&mut target, &mut inspector, |_, inspector| {
                remote_health(inspector, entity).is_some()
            });
            assert_eq!(remote_health(&inspector, entity), Some(1.0));
            let names: Vec<String> = inspect_components(
                inspector.world().resource::<RemoteWorld>().world(),
                Some(entity),
            )
            .into_iter()
            .map(|group| group.name)
            .collect();
            assert!(names.contains(&"Health".to_string()), "{names:?}");
            assert!(names.contains(&"Name".to_string()), "{names:?}");

            edit_health(&mut inspector, entity, 2.5);
            assert_eq!(remote_health(&inspector, entity), Some(2.5));
            // Answering the edit fetches again at once, so wait for that fetch too.
            run_until(&mut target, &mut inspector, |target, inspector| {
                let details = inspector.world().resource::<RemoteDetails>();
                target.world().get::<Health>(entity).unwrap().value == 2.5
                    && details.pending_edits() == 0
                    && !details.is_fetching()
            });
            assert_eq!(remote_health(&inspector, entity), Some(2.5));

            // Only the periodic fetch reports a change the remote app makes itself.
            target.world_mut().get_mut::<Health>(entity).unwrap().value = 7.0;
            let changed = Instant::now();
            run_until(&mut target, &mut inspector, |_, inspector| {
                remote_health(inspector, entity) == Some(7.0)
            });
            assert!(changed.elapsed() < Duration::from_secs(5));

            // An edit the remote app rejects is reverted by the next fetch.
            target.world_mut().entity_mut(entity).remove::<Health>();
            edit_health(&mut inspector, entity, 9.0);
            run_until(&mut target, &mut inspector, |_, inspector| {
                remote_health(inspector, entity).is_none()
                    && inspector
                        .world()
                        .resource::<RemoteDetails>()
                        .pending_edits()
                        == 0
            });
        }
    }
}
