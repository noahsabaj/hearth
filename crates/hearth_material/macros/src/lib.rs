#![expect(missing_docs, reason = "Not all docs are written yet, see #3492.")]
#![cfg_attr(docsrs, feature(doc_cfg))]

use hearth_macro_utils::{derive_label, HearthManifest};
use proc_macro::TokenStream;
use quote::format_ident;
use syn::{parse_macro_input, DeriveInput};

pub(crate) fn hearth_material_path() -> syn::Path {
    HearthManifest::shared(|manifest| manifest.get_path("hearth_material"))
}

#[proc_macro_derive(ShaderLabel)]
pub fn derive_shader_label(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let mut trait_path = hearth_material_path();
    trait_path.segments.push(format_ident!("labels").into());
    trait_path
        .segments
        .push(format_ident!("ShaderLabel").into());
    derive_label(input, "ShaderLabel", &trait_path)
}

#[proc_macro_derive(DrawFunctionLabel)]
pub fn derive_draw_function_label(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let mut trait_path = hearth_material_path();
    trait_path.segments.push(format_ident!("labels").into());
    trait_path
        .segments
        .push(format_ident!("DrawFunctionLabel").into());
    derive_label(input, "DrawFunctionLabel", &trait_path)
}
