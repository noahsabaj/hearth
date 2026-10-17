---
title: The Bevy CLI
authors: ["@noahsabaj"]
pull_requests: []
---

Bevy now has a command line tool, `bevy`, in the new `bevy_cli` crate. It creates, builds and runs projects,
and it inspects and drives a running app over the Bevy Remote Protocol: list entities, read and change
components and resources, take screenshots, press keys and click, pause or speed up time, and stream changes
and events as they happen.

```sh
bevy new my_game && cd my_game
bevy run --detach            # build, start in the background, wait until the app answers
bevy tree                    # the entity hierarchy
bevy set Cube Spin.speed 4   # change a component field in the running app
bevy screenshot              # a PNG of the window
bevy input key space
bevy logs --level warn
```

It is built for coding agents as much as for people. Every command prints one JSON envelope with `--json`,
failures carry stable error codes with hints and close matches, and exit codes mean the same thing for every
command. `bevy commands --json` describes the whole interface, `bevy skill install` teaches agents such as
Claude Code and Codex the workflow, and `bevy mcp` serves the commands as Model Context Protocol tools.

Build errors come back as structured diagnostics, `bevy run --detach` keeps a log and replaces the running
build when you run it again, and `bevy doctor` checks the toolchain, the project and the connection when
something doesn't work.

Install it from a checkout with `cargo install --path crates/bevy_cli`.
