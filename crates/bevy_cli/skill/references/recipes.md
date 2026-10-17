# Recipes

Each recipe assumes `--json` (or `BEVY_FORMAT=json`) when you read results programmatically.

## Check a code change in the running app

```bash
bevy run --detach --json     # stops the old session, rebuilds, starts, waits for BRP
```

- `BUILD_FAILED`: read `errors[0].data.errors` (file, line, message), fix, run again.
- `APP_EXITED`: the app panicked while starting; `errors[0].data.log_tail` has the panic.
- Success: `data.session` names the session. Then check what changed:

```bash
bevy find --with my_game::Enemy
bevy get Player my_game::Health
bevy screenshot --max-size 1024     # open data.path to look at it
```

## Find an entity and change it

```bash
bevy find player                     # names containing "player", any case
bevy find --with Camera3d            # entities with a component
bevy get Camera Transform
bevy set Camera Transform.translation '[0, 10, 20]'
```

If a name matches several entities you get `AMBIGUOUS_ENTITY` with the ids in
`data.candidates`; use one of them.

## Try a value before changing the code

```bash
bevy schema my_game::PlayerSettings              # fields and an example value
bevy set Player my_game::PlayerSettings.jump_height 4
bevy input key Space && bevy screenshot
```

Changes made over BRP last until the app restarts. Put the value that works into the code.

## Drive gameplay with input

```bash
bevy input key w --hold 1000          # hold W for a second
bevy get Player Transform.translation # did it move?
bevy input click 640 360              # logical pixels from the window's top-left
bevy input type "player one"
bevy input key Enter
```

Positions are logical pixels, as `bevy status` reports the window's size. Input goes to the
primary window, even without focus.

## Wait for something to happen

```bash
bevy watch Player my_game::Health --count 1 --duration 30   # the next change, or give up
bevy observe my_game::LevelCompleted --count 1 --duration 60
```

Exit code 0 with `data.count` 0 means nothing happened within the duration.

## Freeze or slow down the game

```bash
bevy time pause
bevy screenshot
bevy time speed 0.1
bevy time resume
```

This changes `Time<Virtual>`, which gameplay that uses `Res<Time>` follows.

## Debug a crash or a warning

```bash
bevy logs --level warn
bevy logs --grep panicked --tail 40
bevy logs --since 18234                # only lines after an earlier call's cursor
```

## Test against a fresh entity

```bash
bevy spawn --name Probe 'Transform={"translation": [0, 1, 0]}' Visibility
bevy insert Probe my_game::Enemy
bevy despawn Probe
```

## Run two apps, such as a server and a client

```bash
bevy run --bin server --detach
bevy run --bin client --detach --name client
bevy status --app server
bevy status --app client
```

Each session gets its own BRP port. `bevy ps` lists them.

## Send a raw BRP request

```bash
bevy methods
bevy call world.query '{"data": {"components": ["bevy_ecs::name::Name"]}}'
bevy call world.get_components+watch '{"entity": "12v0", "components": ["bevy_transform::components::transform::Transform"]}' --count 3
```

`bevy skill show brp` lists the methods and their parameters.
