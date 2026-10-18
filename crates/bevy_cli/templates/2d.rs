//! {{title}}'s game: its components and systems. `src/main.rs` runs it, and `hearth editor`
//! links it so that the editor knows the game's types.

use bevy::prelude::*;

/// The scene `hearth editor` saves, in the `assets` folder. When it exists, the game starts
/// from it instead of the scene `setup` builds in code.
const SCENE: &str = "scenes/main.scn.ron";

/// The game: its systems and the types they use.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        register_types(app);
        app.add_systems(Startup, setup).add_systems(Update, spin);
    }
}

/// Registers the game's components for reflection, so that the editor and the `hearth` CLI can
/// show and change them. Register each new component here. `hearth editor` calls this, as
/// `[package.metadata.hearth]` in `Cargo.toml` says.
pub fn register_types(app: &mut App) {
    app.register_type::<Spin>();
}

/// Spins an entity in the plane of the screen.
#[derive(Component, Reflect)]
#[reflect(Component, Default)]
pub struct Spin {
    /// Radians per second.
    pub speed: f32,
}

impl Default for Spin {
    fn default() -> Self {
        Self { speed: 1.0 }
    }
}

fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    if editor_scene_exists() {
        commands.spawn(DynamicWorldRoot(asset_server.load(SCENE)));
        return;
    }
    commands.spawn((Name::new("Camera"), Camera2d));
    commands.spawn((
        Name::new("Square"),
        Sprite::from_color(Color::srgb(0.25, 0.75, 0.6), Vec2::splat(120.0)),
        Transform::default(),
        Spin::default(),
    ));
}

/// Whether the editor saved a scene for the game to start from.
#[cfg(not(target_arch = "wasm32"))]
fn editor_scene_exists() -> bool {
    bevy::asset::io::file::FileAssetReader::get_base_path()
        .join("assets")
        .join(SCENE)
        .is_file()
}

/// On the web, the game builds its scene in code.
#[cfg(target_arch = "wasm32")]
fn editor_scene_exists() -> bool {
    false
}

fn spin(time: Res<Time>, mut query: Query<(&Spin, &mut Transform)>) {
    for (spin, mut transform) in &mut query {
        transform.rotate_z(spin.speed * time.delta_secs());
    }
}
