//! An empty application with default plugins.

use hearth::prelude::*;

fn main() {
    App::new().add_plugins(DefaultPlugins).run();
}
