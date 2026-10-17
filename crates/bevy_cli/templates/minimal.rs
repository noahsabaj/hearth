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
    .add_systems(Startup, setup);

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

fn setup(mut commands: Commands) {
    commands.spawn((Name::new("Camera"), Camera2d));
}
