//! A headless app for trying the CLI and measuring it: it serves the Hearth Remote Protocol and
//! updates at 60 frames a second, like a windowed game.
//!
//! ```sh
//! cargo run -p hearth_cli --example remote_app -- 1000
//! hearth tree
//! ```
//!
//! The optional argument is how many extra entities to spawn. `HEARTH_REMOTE_PORT` picks the
//! port.

use core::time::Duration;
use std::time::Instant;

use hearth_app::{App, TaskPoolPlugin};
use hearth_ecs::hierarchy::{ChildOf, Children};
use hearth_ecs::name::Name;
use hearth_ecs::prelude::*;
use hearth_ecs::reflect::{ReflectComponent, ReflectResource};
use hearth_reflect::{std_traits::ReflectDefault, Reflect};
use hearth_remote::http::RemoteHttpPlugin;
use hearth_remote::RemotePlugin;
use hearth_time::TimePlugin;

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
struct Health(u32);

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
struct Stats {
    speed: f32,
    tags: Vec<String>,
}

#[derive(Resource, Reflect)]
#[reflect(Resource)]
struct Score(u32);

fn main() {
    let extra: usize = std::env::args()
        .nth(1)
        .and_then(|count| count.parse().ok())
        .unwrap_or(100);
    let port = std::env::var("HEARTH_REMOTE_PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(15702);

    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        TimePlugin,
        RemotePlugin::default(),
        RemoteHttpPlugin::default().with_port(port),
    ))
    .register_type::<Name>()
    .register_type::<ChildOf>()
    .register_type::<Children>()
    .register_type::<Health>()
    .register_type::<Stats>()
    .register_type::<Score>()
    .insert_resource(Score(11));

    let world = app.world_mut();
    let player = world
        .spawn((
            Name::new("Player"),
            Health(7),
            Stats {
                speed: 1.5,
                tags: vec!["hero".to_string()],
            },
        ))
        .id();
    world.spawn((Name::new("Sword"), ChildOf(player)));
    for index in 0..extra {
        let parent = world
            .spawn((Name::new(format!("Crate {index}")), Health(index as u32)))
            .id();
        world.spawn((Name::new(format!("Lid {index}")), ChildOf(parent)));
    }

    app.finish();
    app.cleanup();
    let frame = Duration::from_secs_f64(1.0 / 60.0);
    loop {
        let started = Instant::now();
        app.update();
        if let Some(rest) = frame.checked_sub(started.elapsed()) {
            std::thread::sleep(rest);
        }
    }
}
