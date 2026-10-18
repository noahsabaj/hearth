# Errors

A failed command exits with a non-zero code and, with `--json`, prints:

```json
{
  "success": false,
  "command": "get",
  "data": null,
  "errors": [{ "code": "ENTITY_NOT_FOUND", "message": "...", "hint": "...", "data": { "candidates": ["..."] } }],
  "warnings": []
}
```

Branch on `code`, which is stable; `message` and `hint` are for reading.

## Exit codes

| Code | Meaning                                                        |
|------|----------------------------------------------------------------|
| 0    | Success                                                        |
| 1    | A local failure, such as a file that couldn't be written       |
| 2    | Invalid arguments                                              |
| 3    | No app answered, or the target was ambiguous                   |
| 4    | The app rejected the request, or a name didn't resolve         |
| 5    | Timeout                                                        |
| 6    | The build failed                                               |

## Reaching the app

| Code              | What to do                                                                 |
|-------------------|----------------------------------------------------------------------------|
| `APP_UNREACHABLE` | Start the app (`hearth run --detach`) or pass `--app`. `data.candidates` lists apps on other ports; `hearth ps` lists sessions. |
| `UNKNOWN_APP`     | `--app` must be a session from `hearth ps`, a port, `host:port` or a URL.    |
| `AMBIGUOUS_APP`   | Several sessions of this project run; pass `--app <name>`.                 |
| `TIMEOUT`         | The app didn't answer in time: it may be loading or stalled. Raise `--timeout`. |
| `PROTOCOL_ERROR`  | Something answered, but not as an app with `RemoteHttpPlugin`.             |
| `METHOD_NOT_FOUND`| The app lacks the HRP method: an older engine version, or a missing plugin. `hearth methods` lists what it has. |

## Names

| Code               | What to do                                                    |
|--------------------|---------------------------------------------------------------|
| `ENTITY_NOT_FOUND` | Ids change between runs. Find it again with `hearth find`.      |
| `AMBIGUOUS_ENTITY` | Use one of the ids in `data.candidates`.                      |
| `UNKNOWN_TYPE`     | Use a name from `data.candidates`, or search with `hearth types <text>`. The type must derive `Reflect`. |
| `AMBIGUOUS_TYPE`   | Use a full type path from `data.candidates`.                  |
| `WRONG_KIND`       | The type exists but isn't a component, resource or event; it needs `#[reflect(Component)]` and the like. |
| `UNKNOWN_KEY`      | Use a `KeyCode` name (`KeyA`, `ArrowUp`, `F5`) or an alias (`space`, `ctrl`). |

## The app rejected the request

| Code                        | What to do                                                       |
|-----------------------------|------------------------------------------------------------------|
| `COMPONENT_ERROR`           | The value or path doesn't fit the type. The hint has an example; `hearth schema <Type>` shows the fields. |
| `COMPONENT_NOT_PRESENT`     | The entity lacks the component; `hearth get <entity>` lists them, `hearth insert` adds one. |
| `COMPONENT_NOT_IN_METADATA` | The app doesn't know the component type.                         |
| `RESOURCE_ERROR`            | The value or path doesn't fit the resource; see `hearth schema <Resource>`. |
| `RESOURCE_NOT_PRESENT`      | The app has no such resource; `hearth resource list` lists them.   |
| `SELF_REPARENT`             | An entity can't be its own parent.                               |
| `INVALID_PARAMS`            | The request's parameters are wrong; check the value's shape.     |
| `APP_ERROR`, `HRP_ERROR`    | The app failed to handle the request; `message` says why.        |
| `NO_WINDOW`                 | Input and screenshots need a primary window.                     |
| `CAPTURE_FAILED`            | The window must be rendering: not minimized, with a renderer.    |

## Projects and sessions

| Code                      | What to do                                                    |
|---------------------------|---------------------------------------------------------------|
| `NO_PROJECT`              | Run in a project directory, pass `--manifest-path`, or `hearth new <name>`. |
| `BUILD_FAILED`            | Fix the errors in `data.errors` (file, line, column, message, rendered). |
| `CARGO_NOT_FOUND`         | Install Rust with rustup.                                     |
| `CARGO_FAILED`            | cargo failed before building; `message` has its output.       |
| `TOOLCHAIN_NOT_INSTALLED` | `rustup toolchain install <name>`.                            |
| `UNKNOWN_PACKAGE`, `AMBIGUOUS_PACKAGE` | Pass `-p` with a name from `data.candidates`.    |
| `UNKNOWN_EXAMPLE`, `UNKNOWN_BIN` | Use a name from `data.candidates`; `hearth examples` lists examples. |
| `NO_BINARY`, `AMBIGUOUS_TARGET` | Pass `--bin` or `--example`.                            |
| `NO_ENGINE_DEPENDENCY`    | `hearth editor` needs a package that depends on `hearth`; pick the game's with `-p`. |
| `INVALID_EDITOR_HOOK`     | Set `[package.metadata.hearth] editor-hook` to a function of the library, such as `"register_types"`. |
| `APP_EXITED`              | The app quit before answering HRP; `data.log_tail` has its last output. |
| `APP_FAILED`              | The app exited with a failure; `data.exit_code` has the code. |
| `DIRECTORY_NOT_EMPTY`     | Pick another name or `--path`, or pass `--force`.             |
| `UNKNOWN_SESSION`, `NO_SESSION`, `AMBIGUOUS_SESSION` | `hearth ps` lists the sessions; name one. |
| `NOT_A_SESSION`           | Only sessions can be stopped; `hearth exit` asks any app to exit. |
| `STOP_FAILED`             | The process couldn't be ended; end it by its `pid`.           |

## The command line and the CLI

| Code                | What to do                                                  |
|---------------------|-------------------------------------------------------------|
| `INVALID_ARGUMENTS`, `UNKNOWN_COMMAND`, `UNKNOWN_ARGUMENT`, `MISSING_ARGUMENT`, `INVALID_VALUE` | See `hearth <command> --help`; `data.candidates` may suggest a command. |
| `IO_ERROR`          | A file couldn't be read or written; `message` says which.   |
| `SKILL_EXISTS`      | Another skill is installed there; `--force` replaces it.    |
| `UNKNOWN_TOPIC`     | `hearth skill show --list` lists the topics.                  |
| `INTERNAL_ERROR`    | A bug in the CLI; please report it.                         |
