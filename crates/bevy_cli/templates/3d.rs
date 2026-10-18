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

/// Spins an entity around its vertical axis.
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

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if editor_scene_exists() {
        commands.spawn(DynamicWorldRoot(asset_server.load(SCENE)));
        return;
    }
    commands.spawn((
        Name::new("Ground"),
        Mesh3d(meshes.add(Plane3d::default().mesh().size(8.0, 8.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.5, 0.3))),
    ));
    commands.spawn((
        Name::new("Cube"),
        Mesh3d(meshes.add(Cuboid::new(1.0, 1.0, 1.0))),
        MeshMaterial3d(materials.add(Color::srgb_u8(124, 144, 255))),
        Transform::from_xyz(0.0, 0.5, 0.0),
        Spin::default(),
    ));
    commands.spawn((
        Name::new("Light"),
        PointLight::default(),
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
    commands.spawn((
        Name::new("Camera"),
        Camera3d::default(),
        Transform::from_xyz(-2.5, 4.5, 9.0).looking_at(Vec3::ZERO, Vec3::Y),
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
        transform.rotate_y(spin.speed * time.delta_secs());
    }
}
