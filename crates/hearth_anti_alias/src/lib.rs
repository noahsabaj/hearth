#![expect(missing_docs, reason = "Not all docs are written yet, see #3492.")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]

use contrast_adaptive_sharpening::CasPlugin;
use fxaa::FxaaPlugin;
use hearth_app::Plugin;
use smaa::SmaaPlugin;
use taa::TemporalAntiAliasPlugin;

pub mod contrast_adaptive_sharpening;
#[cfg(all(feature = "dlss", not(feature = "force_disable_dlss")))]
pub mod dlss;
pub mod fxaa;
pub mod smaa;
pub mod taa;

/// Adds fxaa, smaa, taa, contrast aware sharpening, and optional dlss support.
#[derive(Default)]
pub struct AntiAliasPlugin;

impl Plugin for AntiAliasPlugin {
    fn build(&self, app: &mut hearth_app::App) {
        app.add_plugins((
            FxaaPlugin,
            SmaaPlugin,
            TemporalAntiAliasPlugin,
            CasPlugin,
            #[cfg(all(feature = "dlss", not(feature = "force_disable_dlss")))]
            dlss::DlssPlugin,
        ));
    }
}
