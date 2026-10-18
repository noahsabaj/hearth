use hearth_macro_utils::HearthManifest;
use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput};

pub fn derive_animation_event(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    let (hearth_ecs, hearth_animation) = HearthManifest::shared(|manifest| {
        let hearth_ecs = manifest.get_path("hearth_ecs");
        let hearth_animation = manifest.get_path("hearth_animation");
        (hearth_ecs, hearth_animation)
    });

    let generics = ast.generics;
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let struct_name = &ast.ident;

    quote! {
        impl #impl_generics #hearth_ecs::event::Event for #struct_name #type_generics #where_clause {
            type Trigger = #hearth_animation::AnimationEventTrigger;
        }

        impl #impl_generics #hearth_animation::AnimationEvent for #struct_name #type_generics #where_clause {
        }
    }
    .into()
}
