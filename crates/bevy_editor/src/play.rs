//! Play mode: running the game with `hearth run`, and inspecting it while it runs.
//!
//! Play saves the scene, starts the game as a `hearth` session, and points the hierarchy and
//! inspector at it over the Bevy Remote Protocol. Stop ends the session and goes back to editing.
//! The game's window is its own; closing it stops play too.

use alloc::{
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use std::{process::Command, sync::Mutex};

use bevy_app::{App, Update};
use bevy_ecs::{resource::Resource, world::World};
use bevy_inspector::{
    remote::{RemoteConnection, RemoteConnectionState, RemoteSource},
    InspectorSelection, InspectorSource,
};
use bevy_time::{Real, Time};

use crate::{history, project::EditorProject, scene, status::Status};

/// The session name the game runs under while playing.
pub const SESSION: &str = "editor-play";

/// What Play is doing.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub enum PlayState {
    /// Editing the scene.
    #[default]
    Editing,
    /// Building and starting the game.
    Starting,
    /// The game is running; the hierarchy and inspector show it.
    Playing {
        /// The port it answers the Bevy Remote Protocol on.
        port: u16,
    },
    /// Ending the game.
    Stopping,
}

impl PlayState {
    /// Whether the game is starting, running or stopping.
    pub fn is_busy(&self) -> bool {
        !matches!(self, Self::Editing)
    }
}

/// The answer a background `hearth` command will give.
#[derive(Resource, Clone, Default)]
struct Pending(Arc<Mutex<Option<Answer>>>);

enum Answer {
    Started(Result<u16, String>),
    Stopped(Result<(), String>),
}

/// Saves the scene and starts the game.
pub fn play(world: &mut World) {
    if world.resource::<PlayState>().is_busy() {
        return;
    }
    history::commit(world);
    if !scene::save_and_report(world) {
        return;
    }
    let project = world.resource::<EditorProject>().clone();
    let mut args: Vec<String> = ["run", "--detach", "--name", SESSION]
        .map(String::from)
        .to_vec();
    if let Some(package) = &project.package {
        args.extend(["-p".into(), package.clone()]);
    }
    args.extend(project.run_args.iter().cloned());
    *world.resource_mut::<PlayState>() = PlayState::Starting;
    Status::info(world, "Building and starting the game...");
    let pending = world.resource::<Pending>().clone();
    std::thread::spawn(move || {
        let answer = run_cli(&project, &args).and_then(|data| {
            data.get("port")
                .and_then(serde_json::Value::as_u64)
                .and_then(|port| u16::try_from(port).ok())
                .ok_or_else(|| String::from("`hearth run` didn't say which port the game uses"))
        });
        *pending.0.lock().unwrap() = Some(Answer::Started(answer));
    });
}

/// Ends the game and goes back to editing.
pub fn stop(world: &mut World) {
    if !matches!(
        *world.resource::<PlayState>(),
        PlayState::Playing { .. } | PlayState::Starting
    ) {
        return;
    }
    leave_play(world);
    *world.resource_mut::<PlayState>() = PlayState::Stopping;
    let project = world.resource::<EditorProject>().clone();
    let pending = world.resource::<Pending>().clone();
    std::thread::spawn(move || {
        let answer = run_cli(&project, &["stop".into(), SESSION.into()]).map(|_| ());
        *pending.0.lock().unwrap() = Some(Answer::Stopped(answer));
    });
}

/// Plays, or stops when playing.
pub fn toggle(world: &mut World) {
    match *world.resource::<PlayState>() {
        PlayState::Editing => play(world),
        PlayState::Playing { .. } => stop(world),
        PlayState::Starting | PlayState::Stopping => {}
    }
}

/// Points the panels back at the scene.
fn leave_play(world: &mut World) {
    *world.resource_mut::<InspectorSource>() = InspectorSource::Local;
    world.resource_mut::<InspectorSelection>().0 = None;
}

/// Runs `hearth` with `args` in the project, and returns the `data` of its JSON answer.
fn run_cli(project: &EditorProject, args: &[String]) -> Result<serde_json::Value, String> {
    let output = Command::new(&project.cli)
        .args(["--format", "json"])
        .args(args)
        .current_dir(&project.dir)
        // The game picks its own port; don't hand it the editor's.
        .env_remove("HEARTH_REMOTE_PORT")
        .output()
        .map_err(|error| format!("could not run {}: {error}", project.cli.display()))?;
    let text = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value = serde_json::from_str(text.trim()).map_err(|_| {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("");
        format!(
            "`hearth {}` failed: {detail}",
            args.first().map_or("", String::as_str)
        )
    })?;
    if envelope["success"] == serde_json::Value::Bool(true) {
        return Ok(envelope["data"].clone());
    }
    let error = &envelope["errors"][0];
    let message = error["message"]
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| error.to_string());
    Err(message)
}

/// Applies the answers of background `hearth` commands, and stops play when the game goes away.
fn follow_play(world: &mut World) {
    let answer = world.resource::<Pending>().0.lock().unwrap().take();
    match answer {
        Some(Answer::Started(Ok(port))) => {
            if *world.resource::<PlayState>() != PlayState::Starting {
                return;
            }
            *world.resource_mut::<PlayState>() = PlayState::Playing { port };
            *world.resource_mut::<InspectorSource>() =
                InspectorSource::Remote(RemoteSource::localhost(port));
            world.resource_mut::<InspectorSelection>().0 = None;
            world.insert_resource(LostSince(None));
            Status::info(
                world,
                format!("Playing; the panels show the running game (port {port})"),
            );
        }
        Some(Answer::Started(Err(error))) => {
            *world.resource_mut::<PlayState>() = PlayState::Editing;
            Status::error(world, format!("Could not start the game: {error}"));
        }
        Some(Answer::Stopped(result)) => {
            *world.resource_mut::<PlayState>() = PlayState::Editing;
            match result {
                Ok(()) => Status::info(world, "Stopped the game"),
                // The game may have exited by itself already.
                Err(error) => Status::info(world, format!("Stopped ({error})")),
            }
        }
        None => {}
    }

    // A game whose window was closed stops answering; give it a moment, then stop.
    if matches!(*world.resource::<PlayState>(), PlayState::Playing { .. }) {
        let failed = world
            .get_resource::<RemoteConnection>()
            .is_some_and(|connection| matches!(connection.state, RemoteConnectionState::Failed(_)));
        let now = world.resource::<Time<Real>>().elapsed_secs_f64();
        let mut lost = world.get_resource_or_insert_with(|| LostSince(None));
        match (failed, lost.0) {
            (false, _) => lost.0 = None,
            (true, None) => lost.0 = Some(now),
            (true, Some(since)) if now - since > 3.0 => {
                lost.0 = None;
                stop(world);
            }
            _ => {}
        }
    }
}

/// When the running game stopped answering.
#[derive(Resource, Default)]
struct LostSince(Option<f64>);

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<PlayState>()
        .init_resource::<Pending>()
        .add_systems(Update, follow_play);
}
