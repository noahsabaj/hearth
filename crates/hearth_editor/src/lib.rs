#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]

//! The Hearth editor.
//!
//! The editor is a separate app that edits scene files: `assets/scenes/*.scn.ron` in the game's
//! project, which the game spawns with `DynamicWorldRoot`. It has a 3D viewport with a fly camera
//! and a transform gizmo, an entity hierarchy and inspector, an asset browser, and undo for every
//! edit. Its Play button runs the game with `hearth run` and switches the hierarchy and inspector
//! to the running game, over the Hearth Remote Protocol.
//!
//! `hearth editor` builds the editor with the game's code linked in, so the game's own reflected
//! components can be added and edited, and launches it. To build an editor by hand:
//!
//! ```no_run
//! # use hearth_app::App;
//! # use hearth_editor::EditorPlugin;
//! # let mut app = App::new();
//! // DefaultPlugins first, with `editor_window()` as the primary window.
//! app.add_plugins(EditorPlugin::default());
//! # app.run();
//! ```
//!
//! # Controls
//!
//! - Click an object to select it; W, E and R switch the gizmo between move, rotate and scale.
//! - Hold the right mouse button in the viewport to fly: move the mouse to look, WASD to move, Q
//!   and E to go down and up, Shift to go faster, and the wheel to change speed.
//! - F frames the selection, Delete deletes it, Ctrl+D duplicates it.
//! - Ctrl+Z and Ctrl+Y (or Ctrl+Shift+Z) undo and redo, Ctrl+S saves, Ctrl+N starts a new scene.
//! - F5 plays the game, Shift+F5 stops it.

extern crate alloc;

pub mod assets;
pub mod history;
pub mod icon;
mod idle;
mod layout;
pub mod ops;
pub mod play;
pub mod project;
pub mod scene;
mod shortcuts;
pub mod status;
pub mod viewport;

use hearth_app::{App, Plugin};
use hearth_camera_controller::free_camera::FreeCameraPlugin;
use hearth_dev_tools::infinite_grid::InfiniteGridPlugin;
use hearth_feathers::{dark_theme::create_dark_theme, theme::UiTheme, FeathersPlugins};
use hearth_gizmos::transform_gizmo::TransformGizmoPlugin;
use hearth_inspector::InspectorPlugin;
use hearth_picking::mesh_picking::MeshPickingPlugin;
use hearth_remote::{http::RemoteHttpPlugin, RemotePlugin};
use hearth_window::{Window, WindowResolution};

pub use project::EditorProject;

/// The primary window the editor expects: titled, and large enough for its panels.
pub fn editor_window() -> Window {
    Window {
        title: "Hearth Editor".into(),
        resolution: WindowResolution::new(1600, 900),
        ..Default::default()
    }
}

/// Adds the editor to an app that already has `DefaultPlugins`.
///
/// The project, the scene to open, and the `hearth` executable used for Play come from the
/// environment `hearth editor` sets (see [`EditorProject::from_env`]) unless `project` is given.
#[derive(Default)]
pub struct EditorPlugin {
    /// The project to edit. Read from the environment when `None`.
    pub project: Option<EditorProject>,
}

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        let project = self.project.clone().unwrap_or_else(EditorProject::from_env);
        app.insert_resource(project)
            .insert_resource(UiTheme(create_dark_theme()))
            .add_plugins((
                FeathersPlugins,
                InspectorPlugin,
                MeshPickingPlugin,
                TransformGizmoPlugin,
                InfiniteGridPlugin,
                FreeCameraPlugin,
            ))
            .add_plugins((
                status::plugin,
                history::plugin,
                icon::plugin,
                idle::plugin,
                scene::plugin,
                viewport::plugin,
                layout::plugin,
                assets::plugin,
                play::plugin,
                shortcuts::plugin,
            ));

        // `hearth editor` hands the editor a port, so that `hearth` can inspect and drive it too.
        let port = std::env::var("HEARTH_REMOTE_PORT")
            .ok()
            .and_then(|port| port.parse::<u16>().ok());
        if let Some(port) = port
            && !app.is_plugin_added::<RemotePlugin>()
        {
            app.add_plugins((
                RemotePlugin::default(),
                RemoteHttpPlugin::default().with_port(port),
            ));
        }
    }
}
