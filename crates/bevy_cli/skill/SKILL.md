---
name: bevy
description: Build, run, inspect and drive Bevy apps with the `bevy` CLI. Use it when working on a Bevy game or app - to create a project, build it and read compiler errors as JSON, run it in the background and read its logs, and to look at or change a running app's entities, components and resources, take screenshots, send keyboard and mouse input, or control time over the Bevy Remote Protocol.
---

# Bevy CLI

`bevy` builds and runs Bevy projects, and talks to running apps over the Bevy Remote Protocol
(BRP). Use it to check your work in the real, running app instead of guessing: look at the
entities, change a value, take a screenshot, press a key.

## The loop

```bash
bevy run --detach --json       # build, start in the background, return once the app answers
bevy status --json             # app, Bevy version, entity counts, window, time
bevy tree                      # the entity hierarchy, with names and ids
bevy get Player --json         # every component of an entity, by name or id
bevy set Player Transform.translation '[0, 2, 0]'
bevy screenshot --json         # writes a PNG; its path is data.path - open it to look
bevy logs --level warn         # what the app printed
bevy stop                      # ask the app to exit, and end it if it won't
```

After changing code, run `bevy run --detach` again: it stops the running session of the same
name, rebuilds and starts the new build. Arguments after `--` go to the app.

## Output

- Add `--json` to any command, or set `BEVY_FORMAT=json` once. stdout is then one object:
  `{"success": true, "command": "get", "data": {...}, "errors": [], "warnings": []}`.
- On failure `success` is false and `errors[0]` has a stable `code`, a `message`, usually a
  `hint`, and often `data.candidates`: close matches to retry with.
- Exit codes: 0 success, 1 local error, 2 invalid arguments, 3 no app answered, 4 the app
  rejected the request or a name didn't resolve, 5 timeout, 6 build failed.
- `bevy commands --json` lists every command, argument, error code and exit code.
  `bevy <command> --help` has examples.

## Talking to a running app

- **Which app.** By default the running session started from the current project, else
  127.0.0.1:15702. `--app` takes a session name, a port, `host:port` or a URL. `bevy ps` lists
  sessions and other apps answering BRP.
- **What the app needs.** Bevy's `bevy_remote` feature with `RemotePlugin` and
  `RemoteHttpPlugin` added. Projects from `bevy new` have both. `bevy doctor` checks.
- **Entities** are a name (`Player`) or an id (`495v0`). Ids change between runs; names don't.
  `bevy find <text>` and `bevy find --with Camera3d` find them. Use `name:<text>` for a name
  that looks like an id.
- **Types** are short names (`Transform`, `Time<Virtual>`) or full type paths. `bevy types
  <text>` searches the registered types; `bevy schema <Type>` shows fields and an example value.
- **Values** are JSON. Text that isn't JSON is a string (`Bob`), `1,2,3` is `[1, 2, 3]`,
  `@file.json` reads a file and `-` reads stdin. `Vec3` is `[x, y, z]`, `Quat` is
  `[x, y, z, w]`, unit enum variants are strings (`"Hidden"`), other variants are
  `{"Variant": value}`. Details: `bevy skill show values`.
- **Field paths** read or change one field: `bevy get Player Transform.translation`,
  `bevy set Player Transform.translation.y 2`. A partial object changes only the fields it
  has: `bevy set Player Transform '{"scale": [2, 2, 2]}'`.
- **Cost.** The app answers one request per frame. One `bevy query Transform --with Enemy`
  beats twenty `bevy get` calls.

## Seeing and driving the app

- `bevy screenshot` captures the window to a PNG (`--out shot.png`, `--max-size 1024`).
- `bevy input key Space`, `bevy input key ctrl+s`, `bevy input key w --hold 1000`,
  `bevy input type "hello"`, `bevy input click 640 360` (logical pixels from the window's
  top-left; `bevy status` shows the size), `bevy input scroll -3`. The window needn't have focus.
- `bevy time pause`, `bevy time resume`, `bevy time speed 0.25` control virtual time.
- `bevy watch Player Transform --count 3` streams changes; `bevy observe <Event> --duration 5`
  streams events. In scripts always pass `--count` or `--duration`, or they run until stopped.
- `bevy trigger <Event> <value>` triggers an event; `bevy send <Message> <value>` writes a
  message, such as `bevy send AppExit Success`.
- `bevy spawn`, `bevy insert`, `bevy remove`, `bevy despawn`, `bevy reparent` edit the world;
  `bevy resource list|get|set|insert|remove` the resources.
- `bevy call <method> <json>` sends any BRP request; `bevy methods` lists what the app supports.

## Building and running

- `bevy build --json` and `bevy check --json`: on failure the exit code is 6 and
  `errors[0].data.errors` lists each compiler error with `file`, `line`, `column`, `message`,
  `code` and the compiler's full `rendered` text.
- `bevy run --detach` builds, starts the app as a session and waits until it answers BRP.
  `--example <name>` runs an example and enables the features it requires; `bevy examples`
  lists them. `--port` picks the BRP port; it's passed to the app as `BEVY_REMOTE_PORT`.
- `bevy logs` shows a session's output: `--tail 50`, `--level warn`, `--grep text`,
  `--since <cursor>` (the `cursor` of an earlier call, for only new lines), `--follow`.
- `bevy stop` stops the session, `bevy stop --all` every session.
- `bevy new my_game` creates a project that `bevy run --detach` can drive right away.

## When something fails

- `APP_UNREACHABLE` (exit 3): nothing answered. Run `bevy ps`, start the app with
  `bevy run --detach`, or pass `--app`. `data.candidates` lists apps found on other ports.
- `APP_EXITED`: the app quit while starting; `errors[0].data.log_tail` has its last lines.
- `BUILD_FAILED` (exit 6): fix the errors in `errors[0].data.errors` and run again.
- `UNKNOWN_TYPE`, `AMBIGUOUS_TYPE`, `ENTITY_NOT_FOUND`, `AMBIGUOUS_ENTITY`: retry with one of
  `data.candidates`.
- `COMPONENT_ERROR`: the value doesn't fit the type; the hint has an example value and
  `bevy schema <Type>` shows the fields.
- `METHOD_NOT_FOUND`: the app's Bevy is older, or lacks the plugin; `bevy methods` lists what
  it supports.
- Every code: `bevy skill show errors`. Recipes for common tasks: `bevy skill show recipes`.

## MCP

`bevy mcp` serves these commands as Model Context Protocol tools on stdio, for clients that
prefer tools to a shell: `claude mcp add bevy -- bevy mcp`.
