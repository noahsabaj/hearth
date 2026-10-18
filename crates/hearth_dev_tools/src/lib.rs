#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]

//! This crate provides additional utilities for the [Hearth game engine](https://github.com/noahsabaj/hearth),
//! focused on improving developer experience.

extern crate alloc;

#[cfg(feature = "hearth_ci_testing")]
pub mod ci_testing;

#[cfg(feature = "render")]
pub mod diagnostics_overlay;

mod easy_camera;

#[cfg(feature = "render")]
mod easy_screenshot;

#[cfg(feature = "render")]
pub mod fps_overlay;

#[cfg(feature = "render")]
pub mod frame_time_graph;

pub mod inspection;

pub mod picking_debug;

#[cfg(feature = "schedule_data")]
pub mod schedule_data;

pub mod states;

pub use easy_camera::*;

#[cfg(feature = "render")]
pub use easy_screenshot::*;

#[cfg(feature = "render")]
pub mod render_debug;

#[cfg(feature = "render")]
pub mod infinite_grid;

pub mod world_asset_helpers;
