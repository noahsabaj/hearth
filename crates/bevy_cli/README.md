# Hearth CLI

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/noahsabaj/hearth#license)

`hearth` is the command line tool for [Hearth](https://github.com/noahsabaj/hearth) projects. It creates, builds and
runs them, and it inspects and drives a running app over the Bevy Remote Protocol (BRP), so it works with plain
Bevy apps too. It is made for people at a terminal and for scripts and coding agents alike:

- every command prints text by default and one JSON envelope with `--json`
- errors carry stable codes, hints and close matches, and exit codes mean the same thing everywhere
- `hearth commands --json` describes every command, argument, error code and exit code
- `hearth skill install` teaches agents such as Claude Code and Codex to use it
- `hearth mcp` serves the same commands as Model Context Protocol tools

## Install

From a checkout of the Hearth repository:

```sh
cargo install --path crates/bevy_cli
```

## Quick start

```sh
hearth new my_game          # a project with a camera, a light and a spinning cube
cd my_game
hearth run --detach         # build, start in the background, wait until the app answers
hearth tree                 # the entity hierarchy
hearth get Cube             # every component of the entity named Cube
hearth set Cube Spin.speed 4
hearth screenshot           # writes a PNG of the window
hearth input key space      # presses a key in the app
hearth logs --level warn    # what the app printed
hearth stop
```

Change the code, run `hearth run --detach` again, and it replaces the running session with the new build.

## Connecting your own app

The live-app commands need the app to serve BRP: enable the engine's `bevy_remote` feature and add the plugins.
Hearth keeps Bevy's crate names, so the code is the same for both.

```rust
use bevy::prelude::*;
use bevy::remote::{http::RemoteHttpPlugin, RemotePlugin};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, RemotePlugin::default(), RemoteHttpPlugin::default()))
        .run();
}
```

Projects made with `hearth new` depend on Hearth's git repository (`--engine-path`, `--engine-git` and
`--bevy-version` choose another engine), do this behind a `remote` cargo feature, on by default, and read the port
from `HEARTH_REMOTE_PORT`, which `hearth run` sets so that several apps can run at once. Leave the feature out of
the builds you ship with `--no-default-features`. Their dev profile optimizes the engine but keeps only line tables
in its debug info, and on Windows, when rustup's `rust-lld` is on `PATH`, a `.cargo/config.toml` links with it. With
both, rebuilding the 3D template after a one-line change takes about 10 seconds instead of 30.

To reflect your own components and resources, derive `Reflect` and add `#[reflect(Component)]` or
`#[reflect(Resource)]`.

## Commands

| Group      | Commands                                                                                    |
|------------|---------------------------------------------------------------------------------------------|
| Project    | `new`, `build`, `check`, `run`, `ps`, `logs`, `stop`, `examples`, `doctor`                  |
| Live app   | `status`, `tree`, `find`, `get`, `set`, `insert`, `remove`, `spawn`, `despawn`, `reparent`, |
|            | `query`, `resource`, `types`, `schema`, `watch`, `observe`, `trigger`, `send`,              |
|            | `screenshot`, `input`, `time`, `schedules`, `diagnostics`, `methods`, `call`, `exit`        |
| Agents     | `commands`, `skill`, `mcp`                                                                  |

`hearth --help` groups them, `hearth <command> --help` has examples, and `hearth doctor` checks the toolchain, the
project and the connection when something doesn't work.

Entities are given by id (`495v0`) or by name. Types are given by short name (`Transform`) or full path, and
`Component.field.path` reaches into a value. Values are JSON, with shortcuts: a bare word is a string and
`1,2,3` is an array. `@file` and `-` read a value from a file or stdin.

## Which app

Live-app commands talk to the first of:

1. `--app` or `HEARTH_APP`: a session name, a port, `host:port` or a URL
2. the session `hearth run --detach` started from the current project
3. `127.0.0.1:15702`, the default BRP port

`hearth ps` lists sessions and other apps answering BRP on the usual ports.

## Output and exit codes

With `--json` (or `HEARTH_FORMAT=json`), stdout is one object:

```json
{ "success": true, "command": "get", "data": { ... }, "errors": [], "warnings": [] }
```

On failure, `errors[0]` has a stable `code` such as `ENTITY_NOT_FOUND`, a `message`, usually a `hint`, and
often `data.candidates` with close matches. `--format ndjson` prints streams such as `hearth watch` one event
per line, then a `result` line.

| Exit | Meaning                                                    |
|------|------------------------------------------------------------|
| 0    | success                                                    |
| 1    | local error, such as a file that couldn't be written       |
| 2    | invalid arguments                                          |
| 3    | no app answered, or the target was ambiguous               |
| 4    | the app rejected the request, or a name didn't resolve     |
| 5    | timeout                                                    |
| 6    | the build failed                                           |

## Agents

```sh
hearth skill install            # Claude Code; `codex` and `agents` install for other clients
hearth skill install --project  # into the current project instead of your home directory
hearth skill show               # the guide itself; `hearth skill show errors` and other topics
claude mcp add hearth -- hearth mcp
```

The skill teaches the loop: run the app, look at it, change it, check the change in the running app. The
installed skill points at `hearth skill show`, so it stays current as the CLI updates.

## Environment

| Variable             | Meaning                                                     |
|----------------------|-------------------------------------------------------------|
| `HEARTH_APP`         | the app to talk to, like `--app`                            |
| `HEARTH_FORMAT`      | the output format, like `--format`                          |
| `HEARTH_TIMEOUT`     | seconds to wait for the app, like `--timeout`               |
| `HEARTH_TOOLCHAIN`   | the rustup toolchain to build with, like `--toolchain`      |
| `HEARTH_CLI_HOME`    | where sessions and their logs are kept                      |
| `HEARTH_TRACE`       | when set, each BRP request's method, sizes and time go to stderr |
| `HEARTH_REMOTE_PORT` | set by `hearth run` for the app: the port to answer BRP on  |
| `NO_COLOR`           | disables colors                                             |
