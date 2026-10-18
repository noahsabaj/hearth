//! {{title}}, made with Hearth. The game itself is in `src/lib.rs`.

use hearth::prelude::*;
use {{crate}}::GamePlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "{{title}}".into(),
                ..default()
            }),
            ..default()
        }),
        GamePlugin,
    ));

    // Lets the `hearth` CLI inspect and drive the app while it runs.
    #[cfg(feature = "remote")]
    app.add_plugins((
        hearth::remote::RemotePlugin::default(),
        hearth::remote::http::RemoteHttpPlugin::default().with_port(remote_port()),
    ));

    app.run();
}

/// The port the app answers the Hearth Remote Protocol on: `HEARTH_REMOTE_PORT`, which `hearth run`
/// sets, else the default.
#[cfg(feature = "remote")]
fn remote_port() -> u16 {
    std::env::var("HEARTH_REMOTE_PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(hearth::remote::http::DEFAULT_PORT)
}
