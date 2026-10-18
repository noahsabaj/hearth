//! The line of text at the bottom of the editor that says what just happened.

use alloc::string::String;

use bevy_app::App;
use bevy_ecs::{resource::Resource, world::World};
use bevy_log::{info, warn};

/// What the status bar shows.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct Status {
    /// The message.
    pub text: String,
    /// Whether it reports a failure.
    pub error: bool,
}

impl Status {
    /// Shows `text` and logs it.
    pub fn info(world: &mut World, text: impl Into<String>) {
        let text = text.into();
        info!("{text}");
        world.insert_resource(Self { text, error: false });
    }

    /// Shows `text` as a failure and logs it.
    pub fn error(world: &mut World, text: impl Into<String>) {
        let text = text.into();
        warn!("{text}");
        world.insert_resource(Self { text, error: true });
    }
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<Status>();
}
