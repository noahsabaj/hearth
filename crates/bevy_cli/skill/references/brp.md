# The Bevy Remote Protocol

The live commands use the Bevy Remote Protocol: JSON-RPC 2.0 over HTTP, served by
`RemoteHttpPlugin` on 127.0.0.1:15702 by default (the render sub-app answers on 15703).
`hearth call <method> [params]` sends any request and prints the result; methods ending in
`+watch` stream results until `--count` or `--duration`.

```bash
hearth methods                         # what this app supports
hearth call world.list_resources
hearth call world.query '{"data": {"components": ["bevy_ecs::name::Name"]}, "filter": {"with": ["bevy_camera::components::Camera3d"]}}'
```

Requests use full type paths, not short names; `hearth types <text>` finds them. Entity ids are
strings such as `"495v0"`.

## Methods and the commands that use them

| Method                         | Command                      | Parameters                                              |
|--------------------------------|------------------------------|---------------------------------------------------------|
| `rpc.discover`                 | `hearth methods`               | none                                                    |
| `app.info`                     | `hearth status`                | none                                                    |
| `world.query`                  | `tree`, `find`, `query`      | `{"data": {"components": [], "option": [] or "all", "has": []}, "filter": {"with": [], "without": []}, "strict": false}` |
| `world.inspect`                | `hearth get <entity>`          | `{"entity": "495v0"}`                                   |
| `world.get_components`         | `hearth get <entity> <C>...`   | `{"entity": "495v0", "components": ["<path>"]}`         |
| `world.list_components`        |                              | `{"entity": "495v0"}`                                   |
| `world.mutate_components`      | `hearth set`                   | `{"entity": "495v0", "component": "<path>", "path": ".translation.y", "value": 2}` |
| `world.insert_components`      | `hearth insert`                | `{"entity": "495v0", "components": {"<path>": value}}`  |
| `world.remove_components`      | `hearth remove`                | `{"entity": "495v0", "components": ["<path>"]}`         |
| `world.spawn_entity`           | `hearth spawn`                 | `{"components": {"<path>": value}}`                     |
| `world.despawn_entity`         | `hearth despawn`               | `{"entity": "495v0"}`                                   |
| `world.reparent_entities`      | `hearth reparent`              | `{"entities": ["495v0"], "parent": "12v0" or null}`     |
| `world.list_resources`         | `hearth resource list`         | none                                                    |
| `world.get_resources`          | `hearth resource get`          | `{"resource": "<path>"}`                                |
| `world.mutate_resources`       | `hearth resource set`, `time`  | `{"resource": "<path>", "path": ".field", "value": v}`  |
| `world.insert_resources`       | `hearth resource insert`       | `{"resource": "<path>", "value": v}`                    |
| `world.remove_resources`       | `hearth resource remove`       | `{"resource": "<path>"}`                                |
| `world.trigger_event`          | `hearth trigger`               | `{"event": "<path>", "value": v}`                       |
| `world.write_message`          | `hearth send`, `input`, `exit` | `{"message": "<path>", "value": v}`                     |
| `world.get_components+watch`   | `hearth watch <entity> <C>...` | like `world.get_components`                             |
| `world.list_components+watch`  | `hearth watch <entity>`        | `{"entity": "495v0"}`                                   |
| `world.observe+watch`          | `hearth observe`, `screenshot` | `{"event": "<path>", "entity": "495v0"}` (entity optional) |
| `registry.schema`              | `types`, `schema`            | `{"with_crates": [], "without_crates": [], "type_limit": {"with": [], "without": []}}`, all optional |
| `schedule.list`                | `hearth schedules`             | none                                                    |
| `schedule.graph`               | `hearth schedules <label>`     | `{"schedule_label": "Update"}`                          |
| `diagnostics.get`              | `hearth diagnostics`           | `{"paths": ["fps"]}`, optional                          |

Newer apps have more, such as `world.summarize`, `world.inspect_resource` and
`diagnostics.list`; `hearth methods` shows them.

## Errors

The app's error codes map to the CLI's: -23401 `ENTITY_NOT_FOUND`, -23402 `COMPONENT_ERROR`,
-23403 `COMPONENT_NOT_PRESENT`, -23404 `SELF_REPARENT`, -23405 `COMPONENT_NOT_IN_METADATA`,
-23501 `RESOURCE_ERROR`, -23502 `RESOURCE_NOT_PRESENT`, -32601 `METHOD_NOT_FOUND`,
-32602 `INVALID_PARAMS`, -32603 `APP_ERROR`. The original code is in `errors[0].data.brp_code`.

## Costs

The app handles requests once per frame, one request each frame for each connection, so every
call costs at least a frame. Ask for many entities with one `world.query` rather than calling
per entity.
