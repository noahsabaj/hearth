#![cfg_attr(docsrs, feature(doc_cfg))]

//! Derive implementations for `hearth_gizmos`.

use hearth_macro_utils::{fq_std::FQDefault, HearthManifest};
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, parse_quote, DeriveInput, Path};

/// Implements the [`GizmoConfigGroup`] trait for a gizmo config group type.
#[proc_macro_derive(GizmoConfigGroup)]
pub fn derive_gizmo_config_group(input: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(input as DeriveInput);
    HearthManifest::shared(|manifest| {
        let hearth_gizmos_path: Path = manifest.get_path("hearth_gizmos");
        let hearth_reflect_path: Path = manifest.get_path("hearth_reflect");

        ast.generics.make_where_clause().predicates.push(
            parse_quote! { Self: #hearth_reflect_path::Reflect + #hearth_reflect_path::TypePath + #FQDefault},
        );

        let struct_name = &ast.ident;
        let (impl_generics, type_generics, where_clause) = &ast.generics.split_for_impl();

        TokenStream::from(quote! {
            impl #impl_generics #hearth_gizmos_path::config::GizmoConfigGroup for #struct_name #type_generics #where_clause {
            }
        })
    })
}
