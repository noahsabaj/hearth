#![cfg_attr(docsrs, feature(doc_cfg))]

//! Macros for deriving `States` and `SubStates` traits.

extern crate proc_macro;

mod states;

use hearth_macro_utils::HearthManifest;
use proc_macro::TokenStream;

/// Implements the `States` trait for a type - see the trait
/// docs for an example usage.
#[proc_macro_derive(States)]
pub fn derive_states(input: TokenStream) -> TokenStream {
    states::derive_states(input)
}

/// Implements the `SubStates` trait for a type - see the trait
/// docs for an example usage.
#[proc_macro_derive(SubStates, attributes(source))]
pub fn derive_substates(input: TokenStream) -> TokenStream {
    states::derive_substates(input)
}

pub(crate) fn hearth_state_path() -> syn::Path {
    HearthManifest::shared(|manifest| manifest.get_path("hearth_state"))
}
