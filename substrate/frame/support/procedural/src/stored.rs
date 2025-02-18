use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, GenericParam, Ident, Meta, Token, TypeParamBound,
    WherePredicate,
};
use syn::punctuated::Punctuated;
use syn::parse::{Parse, ParseStream};

struct IdentList(Punctuated<Ident, Token![,]>);

impl Parse for IdentList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let idents = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;
        Ok(IdentList(idents))
    }
}

pub fn stored(attr: TokenStream, input: TokenStream) -> TokenStream {
    let no_bound_params = parse_no_bounds_from_args(attr);
    let mut input = parse_macro_input!(input as DeriveInput);

    if let Data::Struct(_) = &input.data {
        let (has_config_bound, config_params) = has_config_bound(&input.generics);
        let should_nobound_derive = has_config_bound || !no_bound_params.is_empty();

        if should_nobound_derive {
            add_normal_trait_bounds(&mut input.generics, &config_params, &no_bound_params);
        }

        let (default_i, ord_i, partial_ord_i, partial_eq_i, eq_i, clone_i, debug_i) =
            if should_nobound_derive {
                (
                    Ident::new("DefaultNoBound", input.ident.span()),
                    Ident::new("OrdNoBound", input.ident.span()),
                    Ident::new("PartialOrdNoBound", input.ident.span()),
                    Ident::new("PartialEqNoBound", input.ident.span()),
                    Ident::new("EqNoBound", input.ident.span()),
                    Ident::new("CloneNoBound", input.ident.span()),
                    Ident::new("RuntimeDebugNoBound", input.ident.span()),
                )
            } else {
                (
                    Ident::new("Default", input.ident.span()),
                    Ident::new("Ord", input.ident.span()),
                    Ident::new("PartialOrd", input.ident.span()),
                    Ident::new("PartialEq", input.ident.span()),
                    Ident::new("Eq", input.ident.span()),
                    Ident::new("Clone", input.ident.span()),
                    Ident::new("RuntimeDebug", input.ident.span()),
                )
            };

        let mut skip_list = config_params.clone();
        for nb in &no_bound_params {
            if !skip_list.contains(nb) {
                skip_list.push(nb.clone());
            }
        }
        let skip_type_params_attr = if should_nobound_derive && !skip_list.is_empty() {
            quote! {
                #[scale_info(skip_type_params(#(#skip_list),*))]
            }
        } else {
            quote! {}
        };

        let struct_ident = &input.ident;
        let (impl_generics, _ty_generics, where_clause) = input.generics.split_for_impl();
        let attrs = &input.attrs;
        let vis = &input.vis;
        let fields = match &input.data {
            Data::Struct(s) => &s.fields,
            _ => unreachable!(),
        };

        // Only add a semicolon for tuple or unit structs.
        let semicolon = match fields {
            syn::Fields::Named(_) => quote! {},
            _ => quote! {;},
        };

        let expanded = match fields {
            // Named struct: place where clause before the fields block.
            syn::Fields::Named(_) => {
                quote! {
                    #[derive(#default_i,
                             #ord_i,
                             #partial_ord_i,
                             #partial_eq_i,
                             #eq_i,
                             #clone_i,
                             Encode,
                             Decode,
                             #debug_i,
                             TypeInfo,
                             MaxEncodedLen)]
                    #skip_type_params_attr
                    #(#attrs)*
                    #vis struct #struct_ident #impl_generics #where_clause #fields
                }
            }
            // Tuple or unit struct: place where clause after the fields.
            syn::Fields::Unnamed(_) | syn::Fields::Unit => {
                quote! {
                    #[derive(#default_i,
                             #ord_i,
                             #partial_ord_i,
                             #partial_eq_i,
                             #eq_i,
                             #clone_i,
                             Encode,
                             Decode,
                             #debug_i,
                             TypeInfo,
                             MaxEncodedLen)]
                    #skip_type_params_attr
                    #(#attrs)*
                    #vis struct #struct_ident #impl_generics #fields #where_clause #semicolon
                }
            }
        };

        expanded.into()
    } else {
        syn::Error::new_spanned(
            &input,
            "The `#[stored]` attribute can only be used on structs.",
        )
        .to_compile_error()
        .into()
    }
}

/// Parse macro arguments expecting a meta item like: `no_bounds(T, U)`.
fn parse_no_bounds_from_args(args: TokenStream) -> Vec<Ident> {
    if args.is_empty() {
        return Vec::new();
    }
    let meta = syn::parse::<Meta>(args).unwrap();
    if meta.path().is_ident("no_bounds") {
        if let Meta::List(meta_list) = meta {
            let ident_list: IdentList =
                syn::parse2(meta_list.tokens).unwrap_or_else(|_| IdentList(Punctuated::new()));
            return ident_list.0.into_iter().collect();
        }
    }
    Vec::new()
}

/// Check if any generic has a bound of `Config`.
fn has_config_bound(generics: &syn::Generics) -> (bool, Vec<Ident>) {
    let mut config_params = Vec::new();
    for param in &generics.params {
        if let GenericParam::Type(type_param) = param {
            if type_param.bounds.iter().any(|b| is_config_bound(b)) {
                config_params.push(type_param.ident.clone());
            }
        }
    }
    if let Some(where_clause) = &generics.where_clause {
        for predicate in &where_clause.predicates {
            if let WherePredicate::Type(tp) = predicate {
                if tp.bounds.iter().any(|b| is_config_bound(b)) {
                    if let syn::Type::Path(path_ty) = &tp.bounded_ty {
                        if let Some(seg) = path_ty.path.segments.last() {
                            config_params.push(seg.ident.clone());
                        }
                    }
                }
            }
        }
    }
    (!config_params.is_empty(), config_params)
}

/// Returns true if the bound contains `Config`.
fn is_config_bound(bound: &TypeParamBound) -> bool {
    matches!(
        bound,
        TypeParamBound::Trait(tb) if tb.path.segments.last().map_or(false, |seg| seg.ident == "Config")
    )
}

/// For generics not bound by `Config` or marked in `no_bounds`, add normal trait bounds.
fn add_normal_trait_bounds(
    generics: &mut syn::Generics,
    config_params: &[Ident],
    no_bound_params: &[Ident],
) {
    let normal_bounds: &[&str] = &[
        "Default",
        "Clone",
        "Ord",
        "PartialOrd",
        "PartialEq",
        "Eq",
        "core::fmt::Debug",
    ];
    for param in &mut generics.params {
        if let GenericParam::Type(type_param) = param {
            let this_ident = &type_param.ident;
            if !config_params.contains(this_ident) && !no_bound_params.contains(this_ident) {
                for bound_name in normal_bounds {
                    let bound_ident = Ident::new(bound_name, this_ident.span());
                    type_param.bounds.push(syn::parse_quote! { #bound_ident });
                }
            }
        }
    }
}
