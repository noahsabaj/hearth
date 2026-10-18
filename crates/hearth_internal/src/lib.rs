#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]
#![no_std]

//! This module is separated into its own crate to enable simple dynamic linking for Hearth, and should not be used directly

/// `use hearth::prelude::*;` to import common components, bundles, and plugins.
pub mod prelude;

mod default_plugins;
pub use default_plugins::*;

#[cfg(feature = "hearth_window")]
pub use hearth_a11y as a11y;
#[cfg(target_os = "android")]
pub use hearth_android as android;
#[cfg(feature = "hearth_animation")]
pub use hearth_animation as animation;
pub use hearth_annotations as annotations;
#[cfg(feature = "hearth_anti_alias")]
pub use hearth_anti_alias as anti_alias;
pub use hearth_app as app;
#[cfg(feature = "hearth_asset")]
pub use hearth_asset as asset;
#[cfg(feature = "hearth_audio")]
pub use hearth_audio as audio;
#[cfg(feature = "hearth_bsn")]
pub use hearth_bsn as bsn;
#[cfg(feature = "hearth_camera")]
pub use hearth_camera as camera;
#[cfg(feature = "hearth_camera_controller")]
pub use hearth_camera_controller as camera_controller;
#[cfg(feature = "hearth_clipboard")]
pub use hearth_clipboard as clipboard;
#[cfg(feature = "hearth_color")]
pub use hearth_color as color;
#[cfg(feature = "hearth_core_pipeline")]
pub use hearth_core_pipeline as core_pipeline;
#[cfg(feature = "hearth_curve")]
pub use hearth_curve as curve;
#[cfg(feature = "hearth_dev_tools")]
pub use hearth_dev_tools as dev_tools;
pub use hearth_diagnostic as diagnostic;
pub use hearth_ecs as ecs;
#[cfg(feature = "hearth_editor")]
pub use hearth_editor as editor;
#[cfg(feature = "hearth_extract")]
pub use hearth_extract as extract;
#[cfg(feature = "hearth_feathers")]
pub use hearth_feathers as feathers;
#[cfg(feature = "hearth_gilrs")]
pub use hearth_gilrs as gilrs;
#[cfg(feature = "hearth_gizmos")]
pub use hearth_gizmos as gizmos;
#[cfg(feature = "hearth_gizmos_render")]
pub use hearth_gizmos_render as gizmos_render;
#[cfg(feature = "hearth_gltf")]
pub use hearth_gltf as gltf;
#[cfg(feature = "hearth_image")]
pub use hearth_image as image;
pub use hearth_input as input;
#[cfg(feature = "hearth_input_focus")]
pub use hearth_input_focus as input_focus;
#[cfg(feature = "hearth_inspector")]
pub use hearth_inspector as inspector;
#[cfg(feature = "hearth_light")]
pub use hearth_light as light;
#[cfg(feature = "hearth_log")]
pub use hearth_log as log;
#[cfg(feature = "hearth_material")]
pub use hearth_material as material;
pub use hearth_math as math;
#[cfg(feature = "hearth_mesh")]
pub use hearth_mesh as mesh;
#[cfg(feature = "hearth_pbr")]
pub use hearth_pbr as pbr;
#[cfg(feature = "hearth_picking")]
pub use hearth_picking as picking;
pub use hearth_platform as platform;
#[cfg(feature = "hearth_post_process")]
pub use hearth_post_process as post_process;
pub use hearth_ptr as ptr;
pub use hearth_reflect as reflect;
#[cfg(feature = "hearth_remote")]
pub use hearth_remote as remote;
#[cfg(feature = "hearth_render")]
pub use hearth_render as render;
#[cfg(feature = "hearth_scene")]
pub use hearth_scene as scene;
#[cfg(feature = "hearth_settings")]
pub use hearth_settings as settings;
#[cfg(feature = "hearth_shader")]
pub use hearth_shader as shader;
pub use hearth_shape as shape;
#[cfg(feature = "hearth_soft_render")]
pub use hearth_soft_render as soft_render;
#[cfg(feature = "hearth_solari")]
pub use hearth_solari as solari;
#[cfg(feature = "hearth_sprite")]
pub use hearth_sprite as sprite;
#[cfg(feature = "hearth_sprite_render")]
pub use hearth_sprite_render as sprite_render;
#[cfg(feature = "hearth_state")]
pub use hearth_state as state;
pub use hearth_tasks as tasks;
#[cfg(feature = "hearth_text")]
pub use hearth_text as text;
pub use hearth_time as time;
pub use hearth_transform as transform;
#[cfg(feature = "hearth_ui")]
pub use hearth_ui as ui;
#[cfg(feature = "hearth_ui_render")]
pub use hearth_ui_render as ui_render;
#[cfg(feature = "hearth_ui_widgets")]
pub use hearth_ui_widgets as ui_widgets;
pub use hearth_utils as utils;
#[cfg(feature = "hearth_window")]
pub use hearth_window as window;
#[cfg(feature = "hearth_winit")]
pub use hearth_winit as winit;
#[cfg(feature = "hearth_world_serialization")]
pub use hearth_world_serialization as world_serialization;
