//! {{title}}, made with Bevy.

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

    // Lets the `bevy` CLI inspect and drive the app while it runs.
    #[cfg(feature = "remote")]
    app.add_plugins((
        bevy::remote::RemotePlugin::default(),
        bevy::remote::http::RemoteHttpPlugin::default().with_port(remote_port()),
    ));

    app.run();
}

/// The port the app answers the Bevy Remote Protocol on: `BEVY_REMOTE_PORT`, which `bevy run`
/// sets, else the default.
#[cfg(feature = "remote")]
fn remote_port() -> u16 {
    std::env::var("BEVY_REMOTE_PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(bevy::remote::http::DEFAULT_PORT)
}

/// Spins an entity in the plane of the screen.
#[derive(Component, Reflect)]
#[reflect(Component)]
struct Spin {
    /// Radians per second.
    speed: f32,
}

fn setup(mut commands: Commands) {
    commands.spawn((Name::new("Camera"), Camera2d));
    commands.spawn((
        Name::new("Square"),
        Sprite::from_color(Color::srgb(0.25, 0.75, 0.6), Vec2::splat(120.0)),
        Transform::default(),
        Spin { speed: 1.0 },
    ));
}

fn spin(time: Res<Time>, mut query: Query<(&Spin, &mut Transform)>) {
    for (spin, mut transform) in &mut query {
        transform.rotate_z(spin.speed * time.delta_secs());
    }
}
