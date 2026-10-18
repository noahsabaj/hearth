#![expect(missing_docs, reason = "Not all docs are written yet, see #3492.")]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]

pub mod blit;
pub mod core_2d;
pub mod core_3d;
pub mod deferred;
pub mod fullscreen_material;
pub mod mip_generation;
pub mod oit;
pub mod prepass;
pub mod schedule;
pub mod skybox;
pub mod tonemapping;
pub mod upscaling;

pub use fullscreen_vertex_shader::FullscreenShader;
use hearth_ecs::schedule::IntoScheduleConfigs;
pub use hearth_light::Skybox;
pub use schedule::{Core2d, Core2dSystems, Core3d, Core3dSystems};

mod fullscreen_vertex_shader;

use crate::schedule::{
    camera_driver, handle_uncovered_swap_chains, submit_pending_command_buffers,
};
use crate::{
    blit::BlitPlugin, core_2d::Core2dPlugin, core_3d::Core3dPlugin,
    deferred::copy_lighting_id::CopyDeferredLightingIdPlugin, mip_generation::MipGenerationPlugin,
    prepass::BackgroundMotionVectorsPlugin, tonemapping::TonemappingPlugin,
    upscaling::UpscalingPlugin,
};
use hearth_app::{App, Plugin};
use hearth_render::renderer::{RenderGraph, RenderGraphSystems};
use hearth_render::RenderApp;
use hearth_shader::load_shader_library;
use oit::OrderIndependentTransparencyPlugin;

#[derive(Default)]
pub struct CorePipelinePlugin;

impl Plugin for CorePipelinePlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "fullscreen_vertex_shader.wesl");
        load_shader_library!(app, "tonemapping.wesl");

        app.add_plugins((Core2dPlugin, Core3dPlugin, CopyDeferredLightingIdPlugin))
            .add_plugins((
                BlitPlugin,
                TonemappingPlugin,
                UpscalingPlugin,
                OrderIndependentTransparencyPlugin,
                MipGenerationPlugin,
                BackgroundMotionVectorsPlugin,
            ));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.init_resource::<FullscreenShader>().add_systems(
            RenderGraph,
            (
                camera_driver.in_set(RenderGraphSystems::Render),
                (submit_pending_command_buffers, handle_uncovered_swap_chains)
                    .chain()
                    .in_set(RenderGraphSystems::Submit),
            ),
        );
    }
}
