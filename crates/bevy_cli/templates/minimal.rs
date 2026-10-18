//! {{title}}'s game: its components and systems. `src/main.rs` runs it, and `hearth editor`
//! links it so that the editor knows the game's types.

use bevy::prelude::*;

/// The game: its systems and the types they use.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        register_types(app);
        app.add_systems(Startup, setup);
    }
}

/// Registers the game's components for reflection, so that the editor and the `hearth` CLI can
/// show and change them, as in `app.register_type::<Health>();` for a component declared with
/// `#[derive(Component, Reflect)]` and `#[reflect(Component)]`. `hearth editor` calls this, as
/// `[package.metadata.hearth]` in `Cargo.toml` says.
pub fn register_types(_app: &mut App) {}

fn setup(mut commands: Commands) {
    commands.spawn((Name::new("Camera"), Camera2d));
}
