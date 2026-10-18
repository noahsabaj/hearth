use crate::{
    impls::{common_partial_reflect_methods, impl_full_reflect, impl_type_path, impl_typed},
    struct_utility::FieldAccessors,
    ReflectStruct,
};
use hearth_macro_utils::fq_std::{FQDefault, FQOption, FQResult};
use quote::{quote, ToTokens};

/// Implements `Struct`, `GetTypeRegistration`, and `Reflect` for the given derive data.
pub(crate) fn impl_struct(reflect_struct: &ReflectStruct) -> proc_macro2::TokenStream {
    let fqoption = FQOption.into_token_stream();

    let hearth_reflect_path = reflect_struct.meta().hearth_reflect_path();
    let struct_path = reflect_struct.meta().type_path();

    let field_names = reflect_struct
        .active_fields()
        .map(|field| {
            field
                .data
                .ident
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| field.declaration_index.to_string())
        })
        .collect::<Vec<String>>();

    let FieldAccessors {
        fields_ref,
        fields_mut,
        field_indices,
        field_count,
        ..
    } = FieldAccessors::new(reflect_struct);

    let where_clause_options = reflect_struct.where_clause_options();
    let typed_impl = impl_typed(&where_clause_options, reflect_struct.to_info_tokens(false));

    let type_path_impl = impl_type_path(reflect_struct.meta());
    let full_reflect_impl = impl_full_reflect(&where_clause_options);
    let common_methods = common_partial_reflect_methods(
        reflect_struct.meta(),
        || Some(quote!(#hearth_reflect_path::structs::struct_partial_eq)),
        || None,
        || Some(quote!(#hearth_reflect_path::structs::struct_partial_cmp)),
    );
    let clone_fn = reflect_struct.get_clone_impl();

    #[cfg(not(feature = "functions"))]
    let function_impls = None::<proc_macro2::TokenStream>;
    #[cfg(feature = "functions")]
    let function_impls = crate::impls::impl_function_traits(&where_clause_options);

    let get_type_registration_impl = reflect_struct.get_type_registration(&where_clause_options);

    let (impl_generics, ty_generics, where_clause) = reflect_struct
        .meta()
        .type_path()
        .generics()
        .split_for_impl();

    #[cfg(not(feature = "auto_register"))]
    let auto_register = None::<proc_macro2::TokenStream>;
    #[cfg(feature = "auto_register")]
    let auto_register = crate::impls::reflect_auto_registration(reflect_struct.meta());

    let where_reflect_clause = where_clause_options.extend_where_clause(where_clause);

    quote! {
        #get_type_registration_impl

        #typed_impl

        #type_path_impl

        #full_reflect_impl

        #function_impls

        #auto_register

        impl #impl_generics #hearth_reflect_path::structs::Struct for #struct_path #ty_generics #where_reflect_clause {
            fn field(&self, name: &str) -> #FQOption<&dyn #hearth_reflect_path::PartialReflect> {
                match name {
                    #(#field_names => #fqoption::Some(#fields_ref),)*
                    _ => #FQOption::None,
                }
            }

            fn field_mut(&mut self, name: &str) -> #FQOption<&mut dyn #hearth_reflect_path::PartialReflect> {
                match name {
                    #(#field_names => #fqoption::Some(#fields_mut),)*
                    _ => #FQOption::None,
                }
            }

            fn field_at(&self, index: usize) -> #FQOption<&dyn #hearth_reflect_path::PartialReflect> {
                match index {
                    #(#field_indices => #fqoption::Some(#fields_ref),)*
                    _ => #FQOption::None,
                }
            }

            fn field_at_mut(&mut self, index: usize) -> #FQOption<&mut dyn #hearth_reflect_path::PartialReflect> {
                match index {
                    #(#field_indices => #fqoption::Some(#fields_mut),)*
                    _ => #FQOption::None,
                }
            }

            fn name_at(&self, index: usize) -> #FQOption<&str> {
                match index {
                    #(#field_indices => #fqoption::Some(#field_names),)*
                    _ => #FQOption::None,
                }
            }

            fn index_of_name(&self, name: &str) -> #FQOption<usize> {
                match name {
                    #(#field_names => #fqoption::Some(#field_indices),)*
                    _ => #FQOption::None,
                }
            }

            fn field_len(&self) -> usize {
                #field_count
            }

            fn iter_fields(&self) -> #hearth_reflect_path::structs::FieldIter {
                #hearth_reflect_path::structs::FieldIter::new(self)
            }

            fn to_dynamic_struct(&self) -> #FQResult<#hearth_reflect_path::structs::DynamicStruct, #hearth_reflect_path::ReflectCloneError> {
                let mut dynamic: #hearth_reflect_path::structs::DynamicStruct = #FQDefault::default();
                dynamic.set_represented_type(#hearth_reflect_path::PartialReflect::get_represented_type_info(self));
                #(dynamic.insert_boxed(#field_names, #hearth_reflect_path::PartialReflect::to_dynamic(#fields_ref)?);)*
                #FQResult::Ok(dynamic)
            }
        }

        impl #impl_generics #hearth_reflect_path::PartialReflect for #struct_path #ty_generics #where_reflect_clause {
            #[inline]
            fn get_represented_type_info(&self) -> #FQOption<&'static #hearth_reflect_path::TypeInfo> {
                #FQOption::Some(<Self as #hearth_reflect_path::Typed>::type_info())
            }

            #[inline]
            fn try_apply(
                &mut self,
                value: &dyn #hearth_reflect_path::PartialReflect
            ) -> #FQResult<(), #hearth_reflect_path::ApplyError> {
                if let #hearth_reflect_path::ReflectRef::Struct(struct_value)
                    = #hearth_reflect_path::PartialReflect::reflect_ref(value) {
                    for (name, value) in #hearth_reflect_path::structs::Struct::iter_fields(struct_value) {
                        if let #FQOption::Some(v) = #hearth_reflect_path::structs::Struct::field_mut(self, name) {
                           #hearth_reflect_path::PartialReflect::try_apply(v, value)?;
                        }
                    }
                } else {
                    return #FQResult::Err(
                        #hearth_reflect_path::ApplyError::MismatchedKinds {
                            from_kind: #hearth_reflect_path::PartialReflect::reflect_kind(value),
                            to_kind: #hearth_reflect_path::ReflectKind::Struct
                        }
                    );
                }
                #FQResult::Ok(())
            }
            #[inline]
            fn reflect_kind(&self) -> #hearth_reflect_path::ReflectKind {
                #hearth_reflect_path::ReflectKind::Struct
            }
            #[inline]
            fn reflect_ref(&self) -> #hearth_reflect_path::ReflectRef {
                #hearth_reflect_path::ReflectRef::Struct(self)
            }
            #[inline]
            fn reflect_mut(&mut self) -> #hearth_reflect_path::ReflectMut {
                #hearth_reflect_path::ReflectMut::Struct(self)
            }
            #[inline]
            fn reflect_owned(self: #hearth_reflect_path::__macro_exports::alloc_utils::Box<Self>) -> #hearth_reflect_path::ReflectOwned {
                #hearth_reflect_path::ReflectOwned::Struct(self)
            }

            #common_methods

            #clone_fn
        }
    }
}
