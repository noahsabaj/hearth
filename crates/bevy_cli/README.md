# Bevy CLI

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/bevyengine/bevy#license)
[![Discord](https://img.shields.io/discord/691052431525675048.svg?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://discord.gg/bevy)

`bevy` is a command line tool for [Bevy](https://bevy.org) projects. It creates, builds and runs them, and it
inspects and drives a running app over the Bevy Remote Protocol (BRP). It is made for people at a terminal and
for scripts and coding agents alike:

- every command prints text by default and one JSON envelope with `--json`
- errors carry stable codes, hints and close matches, and exit codes mean the same thing everywhere
- `bevy commands --json` describes every command, argument, error code and exit code
- `bevy skill install` teaches agents such as Claude Code and Codex to use it
- `bevy mcp` serves the same commands as Model Context Protocol tools

## Install

From a checkout of the Bevy repository:

```sh
cargo install --path crates/bevy_cli
```

## Quick start

```sh
bevy new my_game          # a project with a camera, a light and a spinning cube
cd my_game
bevy run --detach         # build, start in the background, wait until the app answers
bevy tree                 # the entity hierarchy
bevy get Cube             # every component of the entity named Cube
bevy set Cube Spin.speed 4
bevy screenshot           # writes a PNG of the window
bevy input key space      # presses a key in the app
bevy logs --level warn    # what the app printed
bevy stop
```

Change the code, run `bevy run --detach` again, and it replaces the running session with the new build.

## Connecting your own app

The live-app commands need the app to serve BRP: enable Bevy's `bevy_remote` feature and add the plugins.

```rust
use bevy::prelude::*;
use bevy::remote::{http::RemoteHttpPlugin, RemotePlugin};

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, RemotePlugin::default(), RemoteHttpPlugin::default()))
        .run();
}
```

Projects made with `bevy new` do this behind a `remote` cargo feature, on by default, and read the port from
`BEVY_REMOTE_PORT`, which `bevy run` sets so that several apps can run at once. Leave the feature out of the
builds you ship with `--no-default-features`.

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

`bevy --help` groups them, `bevy <command> --help` has examples, and `bevy doctor` checks the toolchain, the
project and the connection when something doesn't work.

Entities are given by id (`495v0`) or by name. Types are given by short name (`Transform`) or full path, and
`Component.field.path` reaches into a value. Values are JSON, with shortcuts: a bare word is a string and
`1,2,3` is an array. `@file` and `-` read a value from a file or stdin.

## Which app

Live-app commands talk to the first of:

1. `--app` or `BEVY_APP`: a session name, a port, `host:port` or a URL
2. the session `bevy run --detach` started from the current project
3. `127.0.0.1:15702`, Bevy's default BRP port

`bevy ps` lists sessions and other apps answering BRP on the usual ports.

## Output and exit codes

With `--json` (or `BEVY_FORMAT=json`), stdout is one object:

```json
{ "success": true, "command": "get", "data": { ... }, "errors": [], "warnings": [] }
```

On failure, `errors[0]` has a stable `code` such as `ENTITY_NOT_FOUND`, a `message`, usually a `hint`, and
often `data.candidates` with close matches. `--format ndjson` prints streams such as `bevy watch` one event
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
bevy skill install            # Claude Code; `codex` and `agents` install for other clients
bevy skill install --project  # into the current project instead of your home directory
bevy skill show               # the guide itself; `bevy skill show errors` and other topics
claude mcp add bevy -- bevy mcp
```

The skill teaches the loop: run the app, look at it, change it, check the change in the running app. The
installed skill points at `bevy skill show`, so it stays current as the CLI updates.

## Environment

| Variable          | Meaning                                                     |
|-------------------|-------------------------------------------------------------|
| `BEVY_APP`        | the app to talk to, like `--app`                            |
| `BEVY_FORMAT`     | the output format, like `--format`                          |
| `BEVY_TIMEOUT`    | seconds to wait for the app, like `--timeout`               |
| `BEVY_TOOLCHAIN`  | the rustup toolchain to build with, like `--toolchain`      |
| `BEVY_CLI_HOME`   | where sessions and their logs are kept                      |
| `NO_COLOR`        | disables colors                                             |
