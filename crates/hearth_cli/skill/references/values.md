# Writing values

Commands that take a value (`set`, `insert`, `spawn`, `resource set`, `trigger`, `send`) take
the JSON that the engine's reflection serializes the type to. `hearth schema <Type>` shows a type's
fields and an example value; `hearth get <entity> <Component> --json` shows the current one,
which is the easiest starting point.

## On the command line

| You write          | It means                 |
|--------------------|--------------------------|
| `2.5`, `true`      | a number, a boolean      |
| `'[0, 1, 0]'`      | an array                 |
| `'{"x": 1}'`       | an object                |
| `Hidden`, `Bob`    | the string `"Hidden"`    |
| `'"42"'`           | the string `"42"`        |
| `1,2,3`            | `[1, 2, 3]`              |
| `@value.json`      | the JSON in a file       |
| `-`                | the JSON on stdin        |

Quote JSON for your shell: single quotes in bash, zsh and PowerShell 7. Windows PowerShell 5
strips double quotes inside arguments, so there write the value to a file and pass `@file`, or
pipe it to `-`.

## Common types

| Type                      | JSON                                                          |
|---------------------------|---------------------------------------------------------------|
| `Vec2`, `Vec3`, `Vec4`    | `[x, y]`, `[x, y, z]`, `[x, y, z, w]`                         |
| `Quat`                    | `[x, y, z, w]`; identity is `[0, 0, 0, 1]`                    |
| `Transform`               | `{"translation": [0, 0, 0], "rotation": [0, 0, 0, 1], "scale": [1, 1, 1]}` |
| `Name`                    | `"Player"`                                                    |
| `Color`                   | `{"Srgba": {"red": 1, "green": 0.5, "blue": 0, "alpha": 1}}`  |
| `Visibility`              | `"Inherited"`, `"Hidden"` or `"Visible"`                      |
| `ChildOf`                 | the parent's id: `"12v0"`                                     |
| `Entity`                  | `"495v0"` (index `v` generation); Hearth 0.19 and older use a number |
| `Option<T>`               | `null`, or the value                                          |
| `Vec<T>`, sets            | an array                                                      |
| `HashMap<K, V>`           | an object                                                     |
| `Duration`                | `{"secs": 1, "nanos": 500000000}`                             |

## Structs, tuple structs and enums

- A struct is an object of its fields: `{"speed": 2.0, "target": "12v0"}`.
- A tuple struct with one field is that field's value: `Health(10.0)` is `10.0`. With more
  fields it's an array: `[1, 2]`.
- A unit enum variant is its name: `"Idle"`.
- A tuple variant is an object with one key: `{"Moving": 3.5}`, or `{"Pair": [1, 2]}`.
- A struct variant is an object with one key: `{"Walking": {"speed": 2.0}}`.

## Field paths

`hearth set` and `hearth resource set` accept a path after the type:

```bash
hearth set Player Transform.translation.y 2
hearth set Player Transform.rotation '[0, 0.707, 0, 0.707]'
hearth set Enemy my_game::Stats.0 5            # the first field of a tuple struct
hearth set Bag my_game::Inventory.items[2] '"sword"'
hearth resource set Time<Virtual>.context.relative_speed 0.5
```

A partial object changes only the fields it names; the others keep their values:

```bash
hearth set Player Transform '{"scale": [2, 2, 2]}'
```

## What can't be read or written

Some components aren't serializable, such as mesh and image handles. `hearth get` lists them
under `unserializable` with a debug description; they can't be set over HRP.
