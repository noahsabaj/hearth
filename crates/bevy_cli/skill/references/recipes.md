# Recipes

Each recipe assumes `--json` (or `HEARTH_FORMAT=json`) when you read results programmatically.

## Check a code change in the running app

```bash
hearth run --detach --json     # stops the old session, rebuilds, starts, waits for BRP
```

- `BUILD_FAILED`: read `errors[0].data.errors` (file, line, message), fix, run again.
- `APP_EXITED`: the app panicked while starting; `errors[0].data.log_tail` has the panic.
- Success: `data.session` names the session. Then check what changed:

```bash
hearth find --with my_game::Enemy
hearth get Player my_game::Health
hearth screenshot --max-size 1024     # open data.path to look at it
```

## Find an entity and change it

```bash
hearth find player                     # names containing "player", any case
hearth find --with Camera3d            # entities with a component
hearth get Camera Transform
hearth set Camera Transform.translation '[0, 10, 20]'
```

If a name matches several entities you get `AMBIGUOUS_ENTITY` with the ids in
`data.candidates`; use one of them.

## Try a value before changing the code

```bash
hearth schema my_game::PlayerSettings              # fields and an example value
hearth set Player my_game::PlayerSettings.jump_height 4
hearth input key Space && hearth screenshot
```

Changes made over BRP last until the app restarts. Put the value that works into the code.

## Drive gameplay with input

```bash
hearth input key w --hold 1000          # hold W for a second
hearth get Player Transform.translation # did it move?
hearth input click 640 360              # logical pixels from the window's top-left
hearth input type "player one"
hearth input key Enter
```

Positions are logical pixels, as `hearth status` reports the window's size. Input goes to the
primary window, even without focus.

## Wait for something to happen

```bash
hearth watch Player my_game::Health --count 1 --duration 30   # the next change, or give up
hearth observe my_game::LevelCompleted --count 1 --duration 60
```

Exit code 0 with `data.count` 0 means nothing happened within the duration.

## Freeze or slow down the game

```bash
hearth time pause
hearth screenshot
hearth time speed 0.1
hearth time resume
```

This changes `Time<Virtual>`, which gameplay that uses `Res<Time>` follows.

## Debug a crash or a warning

```bash
hearth logs --level warn
hearth logs --grep panicked --tail 40
hearth logs --since 18234                # only lines after an earlier call's cursor
```

## Test against a fresh entity

```bash
hearth spawn --name Probe 'Transform={"translation": [0, 1, 0]}' Visibility
hearth insert Probe my_game::Enemy
hearth despawn Probe
```

## Run two apps, such as a server and a client

```bash
hearth run --bin server --detach
hearth run --bin client --detach --name client
hearth status --app server
hearth status --app client
```

Each session gets its own BRP port. `hearth ps` lists them.

## Send a raw BRP request

```bash
hearth methods
hearth call world.query '{"data": {"components": ["bevy_ecs::name::Name"]}}'
hearth call world.get_components+watch '{"entity": "12v0", "components": ["bevy_transform::components::transform::Transform"]}' --count 3
```

`hearth skill show brp` lists the methods and their parameters.
