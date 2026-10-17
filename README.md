# <picture><source media="(prefers-color-scheme: dark)" srcset="assets/branding/hearth_logo_dark.svg"><img alt="Hearth" src="assets/branding/hearth_logo_light.svg" height="96"></picture>

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](#license)

## What is Hearth?

Hearth is a data-driven game engine built in Rust, free and open source. It is a fork of
[Bevy](https://github.com/bevyengine/bevy) that keeps Bevy's crate names and module paths, so Bevy code, examples
and plugins work in Hearth as they are, and Hearth keeps taking in Bevy's improvements.

What Hearth adds:

* **The `hearth` CLI** ([crates/bevy_cli](crates/bevy_cli)): create, build and run projects, and inspect and drive
  a running app over the Bevy Remote Protocol. Every command speaks JSON, so scripts and coding agents can use it
  as well as people, and it ships an agent skill and an MCP server.

## WARNING

Hearth, like the Bevy it builds on, is still early. Important features are missing, documentation is sparse, and
breaking changes arrive with each Bevy release that Hearth takes in. Use it only if you are willing to work in
this environment.

**MSRV:** the Minimum Supported Rust Version is generally close to the latest stable release of Rust.

## Design Goals

* **Capable**: Offer a complete 2D and 3D feature set
* **Simple**: Easy for newbies to pick up, but infinitely flexible for power users
* **Data Focused**: Data-oriented architecture using the Entity Component System paradigm
* **Modular**: Use only what you need. Replace what you don't like
* **Fast**: App logic should run quickly, and when possible, in parallel
* **Productive**: Changes should compile quickly, and you should be able to see and drive the running app

## Getting Started

Install the CLI from a checkout of this repository, then make a project and run it:

```sh
cargo install --path crates/bevy_cli
hearth new my_game
cd my_game
hearth run --detach    # build and start the game in the background
hearth tree            # look at its entities while it runs
```

To try the examples, run them from the checkout:

```sh
cargo run --example breakout
```

To draw a window with standard functionality enabled, use:

```rust
use bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .run();
}
```

Bevy's [Quick Start Guide](https://bevy.org/learn/quick-start/introduction), [API docs](https://docs.rs/bevy) and
[examples](examples/README.md) apply to Hearth too, since the APIs are the same.

## Cargo Features

[This list](docs/cargo_features.md) outlines the cargo features that customize the engine's feature set.

## Thanks

Hearth is built on Bevy, the result of the hard work of many people. A huge thanks to all Bevy contributors, the
many open source projects that came before, the [Rust gamedev ecosystem](https://arewegameyet.rs/), and the many
libraries the engine builds on.

## License

Hearth is free, open source and permissively licensed!
Except where noted (below and/or in individual files), all code in this repository is dual-licensed under either:

* MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

at your option.

Hearth is a modified version of Bevy, distributed under Bevy's licenses with its license files and notices kept as they are.
Some of the engine's code carries additional copyright notices and license terms due to their external origins.
These are generally BSD-like, but exact details vary by crate:
If the README of a crate contains a 'License' header (or similar), the additional copyright notices and license terms applicable to that crate will be listed.
The above licensing requirement still applies to contributions to those crates, and sections of those crates will carry those license terms.
The [license](https://doc.rust-lang.org/cargo/reference/manifest.html#the-license-and-license-file-fields) field of each crate will also reflect this.

The [assets](assets) included in this repository (for our [examples](./examples/README.md)) typically fall under different open licenses.
These will not be included in your game (unless copied in by you).
See [CREDITS.md](CREDITS.md) for the details of the licenses of those files.

### Your contributions

Unless you explicitly state otherwise,
any contribution intentionally submitted for inclusion in the work by you,
as defined in the Apache-2.0 license,
shall be dual licensed as above,
without any additional terms or conditions.
