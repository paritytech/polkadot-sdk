use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Ident, Token, TypeParamBound,
};
use syn::punctuated::Punctuated;
use syn::parse::{Parse, ParseStream, Parser}; // Added Parser trait

/// A helper struct to hold a comma-separated list of identifiers, ie no_bounds(A, B, C).
#[derive(Default)]
struct IdentList(Punctuated<Ident, Token![,]>);

impl Parse for IdentList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let idents = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;
        Ok(IdentList(idents))
    }
}

/// The #[stored] attribute. To be used for structs or enums that will find themselves placed in
/// runtime storage.
///
/// It does the following:
/// - Parses attribute no_bounds and mel_bounds arguments to determine type parameters that should not be bounded,
///   and those for which additional bounds may be applied later (mel_bounds is not used for now).
/// - Adds default trait bounds to any type parameter that should be bounded.
/// - Conditionally adds #[codec(mel_bound(...))] for generic parameters not listed in no_bounds.
/// - Generates the necessary derive attributes and other metadata required for storage in a substrate runtime.
/// - Adjusts those attributes based on whether the item is a struct or an enum.
/// - Utilizes the NoBound version of the derives if there's a type parameter that should be unbounded.
pub fn stored(attr: TokenStream, input: TokenStream) -> TokenStream {
    // Initial parsing.
    let (no_bound_params, _mel_bound_params) = parse_stored_args(attr);
    let mut input = parse_macro_input!(input as DeriveInput);

    // Remove the #[stored] attribute to prevent re-emission.
    input.attrs.retain(|attr| !attr.path().is_ident("stored"));

    // Should we use NoBounds version of derives?
    let should_nobound_derive = !no_bound_params.is_empty();
    if should_nobound_derive {
        // Add standard trait bounds to any generics that need bounds still.
        add_normal_trait_bounds(&mut input.generics, &no_bound_params);
    }

    // Compute #[codec(mel_bound(...))] if no_bounds is provided.
    // We add bounds for any generic type parameter not listed in no_bounds.
    let codec_mel_bound_attr = if !no_bound_params.is_empty() {
        let all_generics: Vec<_> = input.generics.params.iter().filter_map(|param| {
            if let syn::GenericParam::Type(type_param) = param {
                Some(&type_param.ident)
            } else {
                None
            }
        }).collect();
        let mel_bound_gens: Vec<_> = all_generics.into_iter()
            .filter(|ident| !no_bound_params.contains(ident))
            .collect();
        if !mel_bound_gens.is_empty() {
            let bounds = mel_bound_gens.iter().map(|ident| {
                quote! { #ident: MaxEncodedLen }
            });
            quote! {
                #[codec(mel_bound( #(#bounds),* ))]
            }
        } else {
            quote! {}
        }
    } else {
        quote! {}
    };

    let span = input.ident.span();
    let (partial_eq_i, eq_i, clone_i, debug_i) =
        if should_nobound_derive {
            (
                Ident::new("PartialEqNoBound", span),
                Ident::new("EqNoBound", span),
                Ident::new("CloneNoBound", span),
                Ident::new("RuntimeDebugNoBound", span),
            )
        } else {
            (
                Ident::new("PartialEq", span),
                Ident::new("Eq", span),
                Ident::new("Clone", span),
                Ident::new("RuntimeDebug", span),
            )
        };

    // Add scale_info attribute if necessary.
    let skip_list = if should_nobound_derive && !no_bound_params.is_empty() {
        quote! {
            #[scale_info(skip_type_params(#(#no_bound_params),*))]
        }
    } else {
        quote! {}
    };

    // Add cfg_attr for DecodeWithMemTracking
    let mem_tracking_derive = quote! {
        #[cfg_attr(test, derive(DecodeWithMemTracking))]
    };

    // Input extraction.
    let struct_ident = &input.ident;
    let (_generics, _ty_generics, where_clause) = input.generics.split_for_impl();
    let generics = &input.generics;
    let attrs = &input.attrs;
    let vis = &input.vis;

    let common_derives =
        quote! {
            #[derive(
                #partial_eq_i,
                #clone_i,
                #eq_i,
                #debug_i,
                Encode,
                Decode,
                TypeInfo,
                MaxEncodedLen
            )]
        };

    let common_attrs = quote! {
        #common_derives
        #mem_tracking_derive
        #skip_list
        #codec_mel_bound_attr
        #(#attrs)*
    };

    // Appropriate ordering for each type of input.
    let expanded = match input.data {
        Data::Struct(ref data_struct) => match data_struct.fields {
            // Named-field structs.
            syn::Fields::Named(ref fields) => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #generics #where_clause #fields
                }
            },
            // Tuple structs.
            syn::Fields::Unnamed(ref fields) => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #generics #fields #where_clause;
                }
            },
            // Unit structs.
            syn::Fields::Unit => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #generics #where_clause;
                }
            },
        },
        Data::Enum(ref data_enum) => {
            // Enums.
            let variant_tokens: Vec<_> = data_enum.variants
                .iter()
                .map(|variant| quote! { #variant })
                .collect();
            quote! {
                #common_attrs
                #vis enum #struct_ident #generics #where_clause {
                    #(#variant_tokens),*
                }
            }
        },
        Data::Union(_) => {
            // Unions are not supported.
            return syn::Error::new_spanned(
                &input,
                "The #[stored] attribute cannot be used on unions."
            )
            .to_compile_error()
            .into()
        },
    };

    expanded.into()
}

/// Extracts type parameters from the attribute arguments for no_bounds and mel_bounds.
/// For example, given #[stored(no_bounds(A, B), mel_bounds(X, Y))], this function extracts
/// A and B into no_bounds and X and Y into mel_bounds.
fn parse_stored_args(args: TokenStream) -> (Vec<Ident>, Vec<Ident>) {
    let mut no_bounds = Vec::new();
    let mut mel_bounds = Vec::new();
    if args.is_empty() {
        return (no_bounds, mel_bounds);
    }
    let parsed = Punctuated::<syn::Meta, Token![,]>::parse_terminated.parse2(args.into())
        .unwrap_or_default();
    for meta in parsed {
        if let syn::Meta::List(meta_list) = meta {
            if let Some(ident) = meta_list.path.get_ident() {
                if ident == "no_bounds" {
                    let ident_list: IdentList = syn::parse2(meta_list.tokens).unwrap_or_default();
                    no_bounds.extend(ident_list.0.into_iter());
                } else if ident == "mel_bounds" {
                    let ident_list: IdentList = syn::parse2(meta_list.tokens).unwrap_or_default();
                    mel_bounds.extend(ident_list.0.into_iter());
                }
            }
        }
    }
    (no_bounds, mel_bounds)
}

/// Adds standard trait bounds to generic parameters of the input type,
/// except for those parameters listed in no_bound_params.
fn add_normal_trait_bounds(
    generics: &mut syn::Generics,
    no_bound_params: &[Ident],
) {
    let normal_bounds: Vec<&str> = vec![
        "Clone",
        "PartialEq",
        "Eq",
        "core::fmt::Debug",
    ];

    // For each type parameter.
    for param in &mut generics.params {
        if let syn::GenericParam::Type(type_param) = param {
            // Skip parameters specified in no_bound_params.
            if !no_bound_params.contains(&type_param.ident) {
                for bound_name in &normal_bounds {
                    // Add the bound.
                    let bound: TypeParamBound = syn::parse_str(bound_name)
                        .unwrap_or_else(|_| panic!("Failed to parse bound: {}", bound_name));
                    type_param.bounds.push(bound);
                }
            }
        }
    }
}