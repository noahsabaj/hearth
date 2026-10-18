use crate::generics::generate_generics;
use crate::{
    impls::{common_partial_reflect_methods, impl_full_reflect, impl_type_path, impl_typed},
    where_clause_options::WhereClauseOptions,
    ReflectMeta,
};
use hearth_macro_utils::fq_std::{FQClone, FQInto, FQOption, FQResult};
use quote::quote;

/// Implements `GetTypeRegistration` and `Reflect` for the given type data.
pub(crate) fn impl_opaque(meta: &ReflectMeta) -> proc_macro2::TokenStream {
    let hearth_reflect_path = meta.hearth_reflect_path();
    let type_path = meta.type_path();

    #[cfg(feature = "reflect_documentation")]
    let with_docs = {
        let doc = quote::ToTokens::to_token_stream(meta.doc());
        Some(quote!(.with_docs(#doc)))
    };
    #[cfg(not(feature = "reflect_documentation"))]
    let with_docs: Option<proc_macro2::TokenStream> = None;

    let where_clause_options = WhereClauseOptions::new(meta);
    let mut info = quote! {
        #hearth_reflect_path::OpaqueInfo::new::<Self>() #with_docs
    };
    if let Some(generics) = generate_generics(meta) {
        info.extend(quote! {
            .with_generics(#generics)
        });
    }
    let typed_impl = impl_typed(
        &where_clause_options,
        quote! {
            let info = #info;
            #hearth_reflect_path::TypeInfo::Opaque(#info)
        },
    );

    let type_path_impl = impl_type_path(meta);
    let full_reflect_impl = impl_full_reflect(&where_clause_options);
    let common_methods = common_partial_reflect_methods(meta, || None, || None, || None);
    let clone_fn = meta.attrs().get_clone_impl(hearth_reflect_path);

    let apply_impl = if let Some(remote_ty) = meta.remote_ty() {
        let ty = remote_ty.type_path();
        quote! {
            if let #FQOption::Some(value) = <dyn #hearth_reflect_path::PartialReflect>::try_downcast_ref::<#ty>(value) {
                *self = Self(#FQClone::clone(value));
                return #FQResult::Ok(());
            }
        }
    } else {
        quote! {
            if let #FQOption::Some(value) = <dyn #hearth_reflect_path::PartialReflect>::try_downcast_ref::<Self>(value) {
                *self = #FQClone::clone(value);
                return #FQResult::Ok(());
            }
        }
    };

    #[cfg(not(feature = "functions"))]
    let function_impls = None::<proc_macro2::TokenStream>;
    #[cfg(feature = "functions")]
    let function_impls = crate::impls::impl_function_traits(&where_clause_options);

    #[cfg(not(feature = "auto_register"))]
    let auto_register = None::<proc_macro2::TokenStream>;
    #[cfg(feature = "auto_register")]
    let auto_register = crate::impls::reflect_auto_registration(meta);

    let (impl_generics, ty_generics, where_clause) = type_path.generics().split_for_impl();
    let where_reflect_clause = where_clause_options.extend_where_clause(where_clause);
    let get_type_registration_impl = meta.get_type_registration(&where_clause_options);

    quote! {
        #get_type_registration_impl

        #type_path_impl

        #typed_impl

        #full_reflect_impl

        #function_impls

        #auto_register

        impl #impl_generics #hearth_reflect_path::PartialReflect for #type_path #ty_generics #where_reflect_clause  {
            #[inline]
            fn get_represented_type_info(&self) -> #FQOption<&'static #hearth_reflect_path::TypeInfo> {
                #FQOption::Some(<Self as #hearth_reflect_path::Typed>::type_info())
            }

            #[inline]
            fn to_dynamic(&self) -> #FQResult<#hearth_reflect_path::__macro_exports::alloc_utils::Box<dyn #hearth_reflect_path::PartialReflect>, #hearth_reflect_path::ReflectCloneError> {
                #FQResult::Ok(#hearth_reflect_path::__macro_exports::alloc_utils::Box::new(#FQClone::clone(self)))
            }

             #[inline]
            fn try_apply(
                &mut self,
                value: &dyn #hearth_reflect_path::PartialReflect
            ) -> #FQResult<(), #hearth_reflect_path::ApplyError> {
                #apply_impl

                #FQResult::Err(
                    #hearth_reflect_path::ApplyError::MismatchedTypes {
                        from_type: #FQInto::into(#hearth_reflect_path::DynamicTypePath::reflect_type_path(value)),
                        to_type: #FQInto::into(<Self as #hearth_reflect_path::TypePath>::type_path()),
                    }
                )
            }

            #[inline]
            fn reflect_kind(&self) -> #hearth_reflect_path::ReflectKind {
                #hearth_reflect_path::ReflectKind::Opaque
            }

            #[inline]
            fn reflect_ref(&self) -> #hearth_reflect_path::ReflectRef {
                #hearth_reflect_path::ReflectRef::Opaque(self)
            }

            #[inline]
            fn reflect_mut(&mut self) -> #hearth_reflect_path::ReflectMut {
                #hearth_reflect_path::ReflectMut::Opaque(self)
            }

            #[inline]
            fn reflect_owned(self: #hearth_reflect_path::__macro_exports::alloc_utils::Box<Self>) -> #hearth_reflect_path::ReflectOwned {
                #hearth_reflect_path::ReflectOwned::Opaque(self)
            }

            #common_methods

            #clone_fn
        }
    }
}
