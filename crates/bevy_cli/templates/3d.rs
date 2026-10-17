//! {{title}}, made with Hearth.

use bevy::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "{{title}}".into(),
            ..default()
        }),
        ..default()
    }))
    .add_systems(Startup, setup)
    .add_systems(Update, spin);

    // Lets the `hearth` CLI inspect and drive the app while it runs.
    #[cfg(feature = "remote")]
    app.add_plugins((
        bevy::remote::RemotePlugin::default(),
        bevy::remote::http::RemoteHttpPlugin::default().with_port(remote_port()),
    ));

    app.run();
}

/// The port the app answers the Bevy Remote Protocol on: `HEARTH_REMOTE_PORT`, which `hearth run`
/// sets, else the default.
#[cfg(feature = "remote")]
fn remote_port() -> u16 {
    std::env::var("HEARTH_REMOTE_PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(bevy::remote::http::DEFAULT_PORT)
}

/// Spins an entity around its vertical axis.
#[derive(Component, Reflect)]
#[reflect(Component)]
struct Spin {
    /// Radians per second.
    speed: f32,
}

fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
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
        Spin { speed: 1.0 },
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

fn spin(time: Res<Time>, mut query: Query<(&Spin, &mut Transform)>) {
    for (spin, mut transform) in &mut query {
        transform.rotate_y(spin.speed * time.delta_secs());
    }
}
