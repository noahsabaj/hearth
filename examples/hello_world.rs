//! A minimal example that outputs "Hello, World!"

use hearth::prelude::*;

fn main() {
    App::new().add_systems(Startup, hello_world_system).run();
}

fn hello_world_system() {
    println!("Hello, World!");
}
