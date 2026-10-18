---
title: The Hearth CLI
authors: ["@noahsabaj"]
pull_requests: []
---

Hearth now has a command line tool, `hearth`, in the new `hearth_cli` crate. It creates, builds and runs projects,
and it inspects and drives a running app over the Hearth Remote Protocol: list entities, read and change
components and resources, take screenshots, press keys and click, pause or speed up time, and stream changes
and events as they happen.

```sh
hearth new my_game && cd my_game
hearth run --detach            # build, start in the background, wait until the app answers
hearth tree                    # the entity hierarchy
hearth set Cube Spin.speed 4   # change a component field in the running app
hearth screenshot              # a PNG of the window
hearth input key space
hearth logs --level warn
```

It is built for coding agents as much as for people. Every command prints one JSON envelope with `--json`,
failures carry stable error codes with hints and close matches, and exit codes mean the same thing for every
command. `hearth commands --json` describes the whole interface, `hearth skill install` teaches agents such as
Claude Code and Codex the workflow, and `hearth mcp` serves the commands as Model Context Protocol tools.

Build errors come back as structured diagnostics, `hearth run --detach` keeps a log and replaces the running
build when you run it again, and `hearth doctor` checks the toolchain, the project and the connection when
something doesn't work.

Install it from a checkout with `cargo install --path crates/hearth_cli`.
