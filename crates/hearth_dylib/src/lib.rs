#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]

//! Forces dynamic linking of Hearth.
//!
//! Dynamic linking causes Hearth to be built and linked as a dynamic library. This will make
//! incremental builds compile much faster.
//!
//! # Warning
//!
//! Do not enable this feature for release builds because this would require you to ship
//! `libstd.so` and `libhearth_dylib.so` with your game.
//!
//! # Enabling dynamic linking
//!
//! ## The recommended way
//!
//! The easiest way to enable dynamic linking is to use the `--features hearth/dynamic_linking` flag when
//! using the `cargo run` command:
//!
//! `cargo run --features hearth/dynamic_linking`
//!
//! ## The unrecommended way
//!
//! It is also possible to enable the `dynamic_linking` feature inside of the `Cargo.toml` file. This is
//! unrecommended because it requires you to remove this feature every time you want to create a
//! release build to avoid having to ship additional files with your game.
//!
//! To enable dynamic linking inside of the `Cargo.toml` file add the `dynamic_linking` feature to the
//! hearth dependency:
//!
//! `features = ["dynamic_linking"]`
//!
//! ## The manual way
//!
//! Manually enabling dynamic linking is achieved by adding `hearth_dylib` as a dependency and
//! adding the following code to the `main.rs` file:
//!
//! ```
//! #[allow(unused_imports)]
//! use hearth_dylib;
//! ```
//!
//! It is recommended to disable the `hearth_dylib` dependency in release mode by adding the
//! following code to the `use` statement to avoid having to ship additional files with your game:
//!
//! ```
//! #[allow(unused_imports)]
//! #[cfg(debug_assertions)] // new
//! use hearth_dylib;
//! ```

// Force linking of the main hearth crate
#[expect(
    unused_imports,
    clippy::single_component_path_imports,
    reason = "This links the main hearth crate when using dynamic linking, and as such cannot be removed or changed without affecting dynamic linking."
)]
use hearth_internal;
