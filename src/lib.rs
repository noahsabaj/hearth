#![cfg_attr(docsrs, feature(doc_cfg))]
#![expect(
    clippy::doc_markdown,
    reason = "Clippy lints for un-backticked identifiers within the cargo features list, which we don't want."
)]
//! [![Hearth Logo](https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/hearth_icon.svg)](https://github.com/noahsabaj/hearth)
//!
//! Hearth is an open-source, modular game engine built in Rust, with a focus on developer productivity
//! and performance.
//!
//! The [repository](https://github.com/noahsabaj/hearth) has the source, the examples and the
//! `hearth` command-line tool.
//!
//! ## Example
//!
//! Here is a simple "Hello, World!" Hearth app:
//! ```
//! use hearth::prelude::*;
//!
//! fn main() {
//!    App::new()
//!        .add_systems(Update, hello_world_system)
//!        .run();
//! }
//!
//! fn hello_world_system() {
//!    println!("hello world");
//! }
//! ```
//!
//! Don't let the simplicity of the example above fool you. Hearth is a fully featured game engine,
//! and it gets more powerful every day!
//!
//! ## This Crate
//!
//! The `hearth` crate is a container crate that makes it easier to consume Hearth subcrates.
//! The defaults provide a "full engine" experience, but you can easily enable or disable features
//! in your project's `Cargo.toml` to meet your specific needs. See Hearth's `Cargo.toml` for a full
//! list of available features.
//!
//! If you prefer, you can also use the individual Hearth crates directly.
//! Each module in the root of this crate, except for the prelude, can be found on crates.io
//! with `hearth_` appended to the front, e.g., `app` -> `hearth_app`.
#![doc = include_str!("../docs/cargo_features.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]
#![no_std]

pub use hearth_internal::*;

// Wasm does not support dynamic linking.
#[cfg(all(feature = "dynamic_linking", not(target_family = "wasm")))]
#[expect(
    unused_imports,
    clippy::single_component_path_imports,
    reason = "This causes Hearth to be compiled as a dylib when using dynamic linking and therefore cannot be removed or changed without affecting dynamic linking."
)]
use hearth_dylib;
