//! The Hearth editor, editing scenes in this repository's `assets` folder.
//!
//! Games made with `hearth new` open it with `hearth editor`, which links the game's own
//! components in. This example has only the engine's.
//!
//! ```bash
//! cargo run --example editor --features="hearth_editor"
//! ```

use hearth::{
    editor::{editor_window, EditorPlugin, EditorProject},
    prelude::*,
};

fn main() {
    let mut project = EditorProject::from_env();
    if std::env::var_os("HEARTH_EDITOR_SCENE").is_none() {
        project.scene = "scenes/editor_example.scn.ron".into();
    }
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(editor_window()),
                ..default()
            }),
            EditorPlugin {
                project: Some(project),
            },
        ))
        .run();
}
