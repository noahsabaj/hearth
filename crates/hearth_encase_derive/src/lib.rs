#![expect(missing_docs, reason = "Not all docs are written yet, see #3492.")]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png",
    html_favicon_url = "https://raw.githubusercontent.com/noahsabaj/hearth/main/assets/branding/icon.png"
)]

use encase_derive_impl::{implement, syn};
use hearth_macro_utils::HearthManifest;

const ENCASE: &str = "encase";

fn hearth_encase_path() -> syn::Path {
    HearthManifest::shared(|hearth_manifest| {
        hearth_manifest
            .maybe_get_path("hearth_render")
            .map(|hearth_render_path| {
                let mut segments = hearth_render_path.segments;
                segments.push(HearthManifest::parse_str("render_resource"));
                syn::Path {
                    leading_colon: None,
                    segments,
                }
            })
            .map(|path| {
                let mut segments = path.segments;
                segments.push(HearthManifest::parse_str(ENCASE));
                syn::Path {
                    leading_colon: None,
                    segments,
                }
            })
            .unwrap_or_else(|| hearth_manifest.get_path(ENCASE))
    })
}

implement!(hearth_encase_path());
