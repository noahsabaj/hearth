//! Choosing the app a command talks to, and finding running apps.

use std::time::Duration;

use serde_json::{json, Value};

use crate::brp::{Client, DEFAULT_PORT};
use crate::http::{self, Endpoint};
use crate::output::{CliError, Exit};
use crate::session::{self, Session};

/// How the app was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The `--app` flag.
    Flag,
    /// The `HEARTH_APP` environment variable.
    Env,
    /// The one running session started from the current project.
    Project,
    /// Nothing chose: the default port.
    Default,
}

/// The app a command talks to.
pub struct Target {
    /// Where it listens.
    pub endpoint: Endpoint,
    /// Its session, when it was started with `hearth run --detach`.
    pub session: Option<Session>,
    /// How it was chosen.
    pub source: Source,
}

impl Target {
    /// What to suggest when the app doesn't answer.
    pub fn unreachable_hint(&self) -> String {
        if let Some(session) = &self.session {
            return format!(
                "The session `{0}` may have exited: `hearth logs {0}` shows its output, and `hearth run --detach` starts it again.",
                session.name
            );
        }
        match self.source {
            Source::Default => "Start the app with `hearth run --detach`, or pass --app. The app needs the engine's `bevy_remote` feature with `RemotePlugin` and `RemoteHttpPlugin` added; `hearth ps` lists running apps.".to_string(),
            _ => "Check the address, or run `hearth ps` to list running apps.".to_string(),
        }
    }

    /// The JSON description reported by commands.
    pub fn to_json(&self) -> Value {
        json!({
            "endpoint": self.endpoint.to_string(),
            "session": self.session.as_ref().map(|session| session.name.clone()),
            "source": match self.source {
                Source::Flag => "flag",
                Source::Env => "env",
                Source::Project => "project",
                Source::Default => "default",
            },
        })
    }
}

/// Resolves `--app` (or `HEARTH_APP`) to a target.
///
/// A session name, a port, `host:port` and a URL are accepted. Without either, the one running
/// session started from the current directory's project is used, else the default port.
pub fn resolve(flag: Option<&str>) -> Result<Target, CliError> {
    let env = std::env::var("HEARTH_APP")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let chosen = match (flag, env) {
        (Some(flag), _) => Some((flag.to_string(), Source::Flag)),
        (None, Some(env)) => Some((env, Source::Env)),
        (None, None) => None,
    };
    if let Some((text, source)) = chosen {
        return resolve_text(text.trim(), source);
    }

    let cwd = std::env::current_dir()
        .ok()
        .and_then(|cwd| std::fs::canonicalize(cwd).ok());
    let mine: Vec<Session> = session::list()
        .into_iter()
        .filter(|session| {
            cwd.as_ref().is_some_and(|cwd| {
                std::fs::canonicalize(&session.project)
                    .is_ok_and(|project| cwd.starts_with(project))
            })
        })
        .filter(Session::is_running)
        .collect();
    match mine.len() {
        0 => Ok(Target {
            endpoint: Endpoint::localhost(DEFAULT_PORT),
            session: None,
            source: Source::Default,
        }),
        1 => {
            let session = mine.into_iter().next().expect("one session");
            Ok(Target {
                endpoint: Endpoint::localhost(session.port),
                session: Some(session),
                source: Source::Project,
            })
        }
        _ => Err(CliError::new(
            "AMBIGUOUS_APP",
            Exit::Unreachable,
            "Several sessions of this project are running",
        )
        .hint("Pick one with --app <name>.")
        .data(json!({
            "candidates": mine
                .iter()
                .map(|session| format!("{} (port {}, {})", session.name, session.port, session.target))
                .collect::<Vec<_>>(),
        }))),
    }
}

fn resolve_text(text: &str, source: Source) -> Result<Target, CliError> {
    if let Some(session) = session::find(text) {
        return Ok(Target {
            endpoint: Endpoint::localhost(session.port),
            session: Some(session),
            source,
        });
    }
    if let Some(endpoint) = Endpoint::parse(text) {
        let session = if crate::brp::is_loopback(&endpoint.host) {
            session::list()
                .into_iter()
                .find(|session| session.port == endpoint.port && session.is_running())
        } else {
            None
        };
        return Ok(Target {
            endpoint,
            session,
            source,
        });
    }
    let names: Vec<String> = session::list()
        .into_iter()
        .map(|session| session.name)
        .collect();
    Err(CliError::new(
        "UNKNOWN_APP",
        Exit::Unreachable,
        format!("`{text}` is neither a session nor an address"),
    )
    .hint("Pass a session name from `hearth ps`, a port such as 15702, `host:port` or a URL.")
    .data(json!({ "candidates": names })))
}

/// A running app found by [`scan`].
#[derive(Debug, Clone)]
pub struct Found {
    /// Where it listens.
    pub endpoint: Endpoint,
    /// What `app.info` reported, when the app supports it.
    pub info: Option<Value>,
}

impl Found {
    /// A one-line description, such as `127.0.0.1:15702 my_game (0.20.0, main)`.
    pub fn describe(&self) -> String {
        match &self.info {
            Some(info) => format!(
                "{} {} ({}, {})",
                self.endpoint,
                info["app_name"].as_str().unwrap_or("?"),
                info["bevy_version"].as_str().unwrap_or("?"),
                info["sub_app"].as_str().unwrap_or("?"),
            ),
            None => format!("{} (a BRP app)", self.endpoint),
        }
    }
}

/// The ports [`scan`] checks: the default port and the ten after it.
pub fn scan_ports() -> Vec<u16> {
    (DEFAULT_PORT..=DEFAULT_PORT + 10).collect()
}

/// Finds apps answering BRP on `ports` of this machine, checking them in parallel.
pub fn scan(ports: &[u16]) -> Vec<Found> {
    let handles: Vec<_> = ports
        .iter()
        .map(|&port| {
            std::thread::spawn(move || {
                let endpoint = Endpoint::localhost(port);
                if !http::is_listening(&endpoint, Duration::from_millis(300)) {
                    return None;
                }
                let client = Client::new(endpoint.clone(), Duration::from_secs(2));
                match client.call("app.info", None) {
                    Ok(info) => Some(Found {
                        endpoint,
                        info: Some(info),
                    }),
                    // Older Bevy versions don't have `app.info`; anything else isn't BRP.
                    Err(error) if error.code == "METHOD_NOT_FOUND" => Some(Found {
                        endpoint,
                        info: None,
                    }),
                    Err(_) => None,
                }
            })
        })
        .collect();
    handles
        .into_iter()
        .filter_map(|handle| handle.join().ok().flatten())
        .collect()
}
