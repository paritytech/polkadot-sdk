use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Ident, Meta, Token, TypeParamBound,
};
use syn::punctuated::Punctuated;
use syn::parse::{Parse, ParseStream};

#[derive(Default)]
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

    // Remove the #[stored] attribute so it isn’t re‑emitted.
    input.attrs.retain(|attr| !attr.path().is_ident("stored"));

    let should_nobound_derive = !no_bound_params.is_empty();
    if should_nobound_derive {
        add_normal_trait_bounds(&mut input.generics, &no_bound_params);
    }

    let span = input.ident.span();
    let (default_i, ord_i, partial_ord_i, partial_eq_i, eq_i, clone_i, debug_i) =
        if should_nobound_derive {
            (
                Ident::new("DefaultNoBound", span),
                Ident::new("OrdNoBound", span),
                Ident::new("PartialOrdNoBound", span),
                Ident::new("PartialEqNoBound", span),
                Ident::new("EqNoBound", span),
                Ident::new("CloneNoBound", span),
                Ident::new("RuntimeDebugNoBound", span),
            )
        } else {
            (
                Ident::new("Default", span),
                Ident::new("Ord", span),
                Ident::new("PartialOrd", span),
                Ident::new("PartialEq", span),
                Ident::new("Eq", span),
                Ident::new("Clone", span),
                Ident::new("RuntimeDebug", span),
            )
        };

    let skip_list = if should_nobound_derive && !no_bound_params.is_empty() {
        quote! {
            #[scale_info(skip_type_params(#(#no_bound_params),*))]
        }
    } else {
        quote! {}
    };

    let serde_attrs = if should_nobound_derive {
        quote! {
            #[serde(bound(serialize = "", deserialize = ""))]
        }
    } else {
        quote! {}
    };

    let struct_ident = &input.ident;
    let (impl_generics, _ty_generics, where_clause) = input.generics.split_for_impl();
    let attrs = &input.attrs;
    let vis = &input.vis;

    let is_enum = matches!(input.data, Data::Enum(_));
    let common_derive = if is_enum {
        quote! {
            #[derive(
                #default_i,
                #partial_eq_i,
                #eq_i,
                #clone_i,
                Encode,
                Decode,
                #debug_i,
                TypeInfo,
                MaxEncodedLen,
                Serialize,
                Deserialize
            )]
        }
    } else {
        quote! {
            #[derive(
                #default_i,
                #ord_i,
                #partial_ord_i,
                #partial_eq_i,
                #eq_i,
                #clone_i,
                Encode,
                Decode,
                #debug_i,
                TypeInfo,
                MaxEncodedLen,
                Serialize,
                Deserialize
            )]
        }
    };

    let common_attrs = quote! {
        #common_derive
        #skip_list
        #serde_attrs
        #(#attrs)*
    };

    let expanded = match input.data {
        Data::Struct(ref data_struct) => match data_struct.fields {
            syn::Fields::Named(ref fields) => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #impl_generics #where_clause #fields
                }
            },
            syn::Fields::Unnamed(ref fields) => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #impl_generics #fields #where_clause;
                }
            },
            syn::Fields::Unit => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #impl_generics #where_clause;
                }
            },
        },
        Data::Enum(ref data_enum) => {
            let variant_tokens: Vec<_> = data_enum.variants
                .iter()
                .map(|variant| quote! { #variant })
                .collect();
            quote! {
                #common_attrs
                #vis enum #struct_ident #impl_generics #where_clause {
                    #(#variant_tokens),*
                }
            }
        },
        Data::Union(_) => {
            return syn::Error::new_spanned(
                &input,
                "The `#[stored]` attribute cannot be used on unions."
            )
            .to_compile_error()
            .into()
        },
    };

    expanded.into()
}

fn parse_no_bounds_from_args(args: TokenStream) -> Vec<Ident> {
    if args.is_empty() {
        return Vec::new();
    }
    let meta = syn::parse::<Meta>(args).unwrap();
    if meta.path().is_ident("no_bounds") {
        if let Meta::List(meta_list) = meta {
            let ident_list: IdentList = syn::parse2(meta_list.tokens).unwrap_or_default();
            return ident_list.0.into_iter().collect();
        }
    }
    Vec::new()
}

fn add_normal_trait_bounds(
    generics: &mut syn::Generics,
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
        "Serialize",
        "for<'a> Deserialize<'a>",
    ];
    for param in &mut generics.params {
        if let syn::GenericParam::Type(type_param) = param {
            if !no_bound_params.contains(&type_param.ident) {
                for bound_name in normal_bounds {
                    let bound: TypeParamBound = syn::parse_str(bound_name)
                        .unwrap_or_else(|_| panic!("Failed to parse bound: {}", bound_name));
                    type_param.bounds.push(bound);
                }
            }
        }
    }
}