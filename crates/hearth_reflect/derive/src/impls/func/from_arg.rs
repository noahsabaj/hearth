use crate::where_clause_options::WhereClauseOptions;
use hearth_macro_utils::fq_std::FQResult;
use quote::quote;

pub(crate) fn impl_from_arg(where_clause_options: &WhereClauseOptions) -> proc_macro2::TokenStream {
    let meta = where_clause_options.meta();
    let hearth_reflect = meta.hearth_reflect_path();
    let type_path = meta.type_path();

    let (impl_generics, ty_generics, where_clause) = type_path.generics().split_for_impl();
    let where_reflect_clause = where_clause_options.extend_where_clause(where_clause);

    quote! {
        impl #impl_generics #hearth_reflect::func::args::FromArg for #type_path #ty_generics #where_reflect_clause {
            type This<'from_arg> = #type_path #ty_generics;
            fn from_arg(arg: #hearth_reflect::func::args::Arg) ->
                #FQResult<Self::This<'_>, #hearth_reflect::func::args::ArgError>
            {
                arg.take_owned()
            }
        }
    }
}
