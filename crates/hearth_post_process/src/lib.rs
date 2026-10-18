#![expect(missing_docs, reason = "Not all docs are written yet, see #3492.")]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]

pub mod auto_exposure;
pub mod bloom;
pub mod dof;
pub mod effect_stack;
pub mod motion_blur;
pub mod msaa_writeback;

use crate::{
    bloom::BloomPlugin, dof::DepthOfFieldPlugin, effect_stack::EffectStackPlugin,
    motion_blur::MotionBlurPlugin, msaa_writeback::MsaaWritebackPlugin,
};
use hearth_app::{App, Plugin};
use hearth_shader::load_shader_library;

/// Adds bloom, motion blur, depth of field, and chromatic aberration support.
#[derive(Default)]
pub struct PostProcessPlugin;

impl Plugin for PostProcessPlugin {
    fn build(&self, app: &mut App) {
        load_shader_library!(app, "gaussian_blur.wesl");

        app.add_plugins((
            MsaaWritebackPlugin,
            BloomPlugin,
            MotionBlurPlugin,
            DepthOfFieldPlugin,
            EffectStackPlugin,
        ));
    }
}
