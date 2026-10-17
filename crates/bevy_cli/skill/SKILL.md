---
name: hearth
description: Build, run, inspect and drive Hearth and Bevy apps with the `hearth` CLI. Use it when working on a Hearth or Bevy game or app - to create a project, build it and read compiler errors as JSON, run it in the background and read its logs, and to look at or change a running app's entities, components and resources, take screenshots, send keyboard and mouse input, or control time over the Bevy Remote Protocol.
---

# Hearth CLI

`hearth` builds and runs Hearth projects, and talks to running apps over the Bevy Remote Protocol
(BRP). Hearth is a fork of Bevy that keeps Bevy's crate names, so code still says `use bevy::`
and `bevy_remote`, and the CLI drives plain Bevy apps too.

Use it to check your work in the real, running app instead of guessing: look at the entities,
change a value, take a screenshot, press a key.

## The loop

```bash
hearth run --detach --json       # build, start in the background, return once the app answers
hearth status --json             # app, engine version, entity counts, window, time
hearth tree                      # the entity hierarchy, with names and ids
hearth get Player --json         # every component of an entity, by name or id
hearth set Player Transform.translation '[0, 2, 0]'
hearth screenshot --json         # writes a PNG; its path is data.path - open it to look
hearth logs --level warn         # what the app printed
hearth stop                      # ask the app to exit, and end it if it won't
```

After changing code, run `hearth run --detach` again: it stops the running session of the same
name, rebuilds and starts the new build. Arguments after `--` go to the app.

## Output

- Add `--json` to any command, or set `HEARTH_FORMAT=json` once. stdout is then one object:
  `{"success": true, "command": "get", "data": {...}, "errors": [], "warnings": []}`.
- On failure `success` is false and `errors[0]` has a stable `code`, a `message`, usually a
  `hint`, and often `data.candidates`: close matches to retry with.
- Exit codes: 0 success, 1 local error, 2 invalid arguments, 3 no app answered, 4 the app
  rejected the request or a name didn't resolve, 5 timeout, 6 build failed.
- `hearth commands --json` lists every command, argument, error code and exit code.
  `hearth <command> --help` has examples.

## Talking to a running app

- **Which app.** By default the running session started from the current project, else
  127.0.0.1:15702. `--app` takes a session name, a port, `host:port` or a URL. `hearth ps` lists
  sessions and other apps answering BRP.
- **What the app needs.** The engine's `bevy_remote` feature with `RemotePlugin` and
  `RemoteHttpPlugin` added. Projects from `hearth new` have both. `hearth doctor` checks.
- **Entities** are a name (`Player`) or an id (`495v0`). Ids change between runs; names don't.
  `hearth find <text>` and `hearth find --with Camera3d` find them. Use `name:<text>` for a name
  that looks like an id.
- **Types** are short names (`Transform`, `Time<Virtual>`) or full type paths. `hearth types
  <text>` searches the registered types; `hearth schema <Type>` shows fields and an example value.
- **Values** are JSON. Text that isn't JSON is a string (`Bob`), `1,2,3` is `[1, 2, 3]`,
  `@file.json` reads a file and `-` reads stdin. `Vec3` is `[x, y, z]`, `Quat` is
  `[x, y, z, w]`, unit enum variants are strings (`"Hidden"`), other variants are
  `{"Variant": value}`. Details: `hearth skill show values`.
- **Field paths** read or change one field: `hearth get Player Transform.translation`,
  `hearth set Player Transform.translation.y 2`. A partial object changes only the fields it
  has: `hearth set Player Transform '{"scale": [2, 2, 2]}'`.
- **Cost.** The app answers one request per frame. One `hearth query Transform --with Enemy`
  beats twenty `hearth get` calls.

## Seeing and driving the app

- `hearth screenshot` captures the window to a PNG (`--out shot.png`, `--max-size 1024`).
- `hearth input key Space`, `hearth input key ctrl+s`, `hearth input key w --hold 1000`,
  `hearth input type "hello"`, `hearth input click 640 360` (logical pixels from the window's
  top-left; `hearth status` shows the size), `hearth input scroll -3`. The window needn't have focus.
- `hearth time pause`, `hearth time resume`, `hearth time speed 0.25` control virtual time.
- `hearth watch Player Transform --count 3` streams changes; `hearth observe <Event> --duration 5`
  streams events. In scripts always pass `--count` or `--duration`, or they run until stopped.
- `hearth trigger <Event> <value>` triggers an event; `hearth send <Message> <value>` writes a
  message, such as `hearth send AppExit Success`.
- `hearth spawn`, `hearth insert`, `hearth remove`, `hearth despawn`, `hearth reparent` edit the world;
  `hearth resource list|get|set|insert|remove` the resources.
- `hearth call <method> <json>` sends any BRP request; `hearth methods` lists what the app supports.

## Building and running

- `hearth build --json` and `hearth check --json`: on failure the exit code is 6 and
  `errors[0].data.errors` lists each compiler error with `file`, `line`, `column`, `message`,
  `code` and the compiler's full `rendered` text.
- `hearth run --detach` builds, starts the app as a session and waits until it answers BRP.
  `--example <name>` runs an example and enables the features it requires; `hearth examples`
  lists them. `--port` picks the BRP port; it's passed to the app as `HEARTH_REMOTE_PORT`.
- `hearth logs` shows a session's output: `--tail 50`, `--level warn`, `--grep text`,
  `--since <cursor>` (the `cursor` of an earlier call, for only new lines), `--follow`.
- `hearth stop` stops the session, `hearth stop --all` every session.
- `hearth new my_game` creates a project that `hearth run --detach` can drive right away.

## When something fails

- `APP_UNREACHABLE` (exit 3): nothing answered. Run `hearth ps`, start the app with
  `hearth run --detach`, or pass `--app`. `data.candidates` lists apps found on other ports.
- `APP_EXITED`: the app quit while starting; `errors[0].data.log_tail` has its last lines.
- `BUILD_FAILED` (exit 6): fix the errors in `errors[0].data.errors` and run again.
- `UNKNOWN_TYPE`, `AMBIGUOUS_TYPE`, `ENTITY_NOT_FOUND`, `AMBIGUOUS_ENTITY`: retry with one of
  `data.candidates`.
- `COMPONENT_ERROR`: the value doesn't fit the type; the hint has an example value and
  `hearth schema <Type>` shows the fields.
- `METHOD_NOT_FOUND`: the app's engine version is older, or it lacks the plugin; `hearth methods` lists what
  it supports.
- Every code: `hearth skill show errors`. Recipes for common tasks: `hearth skill show recipes`.

## MCP

`hearth mcp` serves these commands as Model Context Protocol tools on stdio, for clients that
prefer tools to a shell: `claude mcp add hearth -- hearth mcp`.
