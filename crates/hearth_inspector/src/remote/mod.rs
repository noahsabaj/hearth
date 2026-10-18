//! A source inspecting a separate running app over the Hearth Remote Protocol.
//!
//! The remote entities are mirrored into a separate [`World`], the [`RemoteWorld`], at their
//! remote ids. The entity tree and the details panel read that world instead of the local one, so
//! no local hook, observer or system ever sees remote data. See [`world`] for what is mirrored.
//!
//! The entity tree is polled with `world.query` for the components it needs only: [`Name`],
//! [`ChildOf`] and the label-defining components. A full poll reading every component runs when
//! entities the inspector has not seen appear, and at least every 10 seconds. It tells which
//! entities hold a reflected component, since the others, such as observers and systems, are not
//! mirrored. Answers are parsed off the main thread, and only what changed is written.
//!
//! Every component of the selected entity is fetched for the details panel at the same pace, and
//! the fields of the components the local type registry knows can be edited, see
//! [`RemoteDetails`].
//!
//! Remote entities with the [`Disabled`] component are not shown, since `world.query` skips them.
//!
//! [`Name`]: hearth_ecs::name::Name
//! [`ChildOf`]: hearth_ecs::hierarchy::ChildOf
//! [`Disabled`]: hearth_ecs::entity_disabling::Disabled
//! [`World`]: hearth_ecs::world::World

mod details;
mod source;
pub mod world;

pub(crate) use details::send_field_edit;
pub use details::{poll_remote_details, RemoteDetails};
pub(crate) use source::is_remote;
#[cfg(test)]
pub(crate) use source::tests;
pub use source::{
    poll_remote_connection, sync_remote_source, sync_remote_world, RemoteConnection,
    RemoteConnectionState, RemoteSnapshot, RemoteSource,
};
pub use world::{RemoteComponents, RemoteValue, RemoteValues, RemoteWorld, UnknownComponent};
