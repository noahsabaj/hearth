# Hearth Platform Support

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/noahsabaj/hearth#license)

Rust is a fantastic multi-platform language with extensive support for modern targets through its [standard library](https://doc.rust-lang.org/stable/std/).
However, some items within the standard library have alternatives that are better suited for Hearth and game engines in general.
Additionally, to support embedded and other esoteric platforms, it's often necessary to shed reliance on `std`, making your crate [`no_std`](https://docs.rust-embedded.org/book/intro/no-std.html).

These needs are handled by this crate, `hearth_platform`.
The goal of this crate is to provide alternatives and extensions to the Rust standard library which minimize friction when developing with and for Hearth across multiple platforms.

## Getting Started

Like any dependency from [crates.io](https://crates.io/), use `cargo` to add it to your `Cargo.toml` file:

```sh
cargo add hearth_platform
```

Now, instead of importing from `std` you can use `hearth_platform` for items it has alternative for.
See the documentation for what items are available, and explanations for _why_ you may want to use them.

## `no_std` Support

By default, `hearth_platform` will activate the `std` feature, requiring access to the `std` crate for whichever platforms you're targeting.
To use this crate on `no_std` platforms, disable default features:

```toml
hearth_platform = { version = "x.y.z", default-features = false }
```

## Features

### `std` (_default_)

Enables usage of the standard library. Note that where this crate has alternatives to the standard library that it considers _better_ than what's provided, it will provide the alternative even when `std` is enabled.
This is explicitly incompatible with `no_std` targets.

### `alloc` (_default_)

Enables usage of the [`alloc`](https://doc.rust-lang.org/stable/alloc/) crate. Note that this feature is automatically enabled when enabling `std`.
This is compatible with most `no_std` targets, but not all.

### `critical-section`

Switches to using [`critical-section`](https://docs.rs/critical-section/latest/critical_section/) as a backend for synchronization.
You may need to enable this feature on platforms with little to no support for atomic operations.
