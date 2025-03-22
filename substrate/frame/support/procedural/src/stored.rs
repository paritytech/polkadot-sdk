use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Ident, Token, Type
};
use syn::punctuated::Punctuated;
use syn::parse::{Parse, ParseStream, Parser};

/// A helper struct to hold a comma-separated list of identifiers, e.g. no_bounds(A, B, C).
#[derive(Default)]
struct IdentList(Punctuated<Ident, Token![,]>);

impl Parse for IdentList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let idents = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;
        Ok(IdentList(idents))
    }
}

/// Represents a single mel_bounds item.
#[derive(Clone)]
struct MelBoundItem {
    ty: Type,
    bound: Option<Type>,
}

impl Parse for MelBoundItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty: Type = input.parse()?;
        let bound = if input.peek(Token![:]) {
            let _colon: Token![:] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(MelBoundItem { ty, bound })
    }
}

/// A helper struct to hold a comma-separated list of MelBoundItem.
#[derive(Default)]
struct MelBoundList(Punctuated<MelBoundItem, Token![,]>);

impl Parse for MelBoundList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let list = Punctuated::parse_terminated(input)?;
        Ok(MelBoundList(list))
    }
}

/// The #[stored] attribute.
pub fn stored(attr: TokenStream, input: TokenStream) -> TokenStream {
    // Parse stored attribute arguments.
    let (no_bound_params, mel_bound_params) = parse_stored_args(attr);
    let mut input = parse_macro_input!(input as DeriveInput);

    // Remove the #[stored] attribute to prevent re-emission.
    input.attrs.retain(|attr| !attr.path().is_ident("stored"));

    // We no longer add normal trait bounds.
    // Instead, if no_bounds is provided, we add a #[still_bind(...)] attribute with the generics
    // that are not listed in no_bounds.
    let still_bind_attr = if !no_bound_params.is_empty() {
        let still_bound_gens: Vec<_> = input.generics.params.iter().filter_map(|param| {
            if let syn::GenericParam::Type(type_param) = param {
                if !no_bound_params.contains(&type_param.ident) {
                    Some(&type_param.ident)
                } else {
                    None
                }
            } else {
                None
            }
        }).collect();
        if still_bound_gens.is_empty() {
            quote! {}
        } else {
            quote! {
                #[still_bind( #(#still_bound_gens),* )]
            }
        }
    } else {
        quote! {}
    };

    // Compute the #[codec(mel_bound(...))] attribute.
    let codec_mel_bound_attr = if let Some(mel_bounds) = mel_bound_params {
        let bounds_mel = mel_bounds.clone().into_iter().map(|item| {
            let MelBoundItem { ty, bound } = item;
            if let Some(explicit_bound) = bound {
                quote! { #ty: #explicit_bound }
            } else {
                quote! { #ty: MaxEncodedLen }
            }
        });
        let bounds_encode = mel_bounds.clone().into_iter().map(|item| {
            let MelBoundItem { ty, bound } = item;
            if let Some(explicit_bound) = bound {
                quote! { #ty: #explicit_bound }
            } else {
                quote! { #ty: Encode }
            }
        });
        let bounds_decode = mel_bounds.clone().into_iter().map(|item| {
            let MelBoundItem { ty, bound } = item;
            if let Some(explicit_bound) = bound {
                quote! { #ty: #explicit_bound }
            } else {
                quote! { #ty: Decode }
            }
        });
        let bounds_decode_mem_tracking = mel_bounds.clone().into_iter().map(|item| {
            let MelBoundItem { ty, bound } = item;
            if let Some(explicit_bound) = bound {
                quote! { #ty: #explicit_bound }
            } else {
                quote! { #ty: DecodeWithMemTracking }
            }
        });
        quote! {
            #[codec(encode_bound( #(#bounds_encode),*))]
            #[codec(decode_bound( #(#bounds_decode),*))]
            #[codec(decode_with_mem_tracking_bound( #(#bounds_decode_mem_tracking),*))]
            #[codec(mel_bound( #(#bounds_mel),* ))]
        }
    } else if !no_bound_params.is_empty() {
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
        let bounds_mel = mel_bound_gens.iter().map(|ident| {
            quote! { #ident: MaxEncodedLen }
        });
        let bounds_encode = mel_bound_gens.iter().map(|ident| {
            quote! { #ident: Encode }
        });
        let bounds_decode = mel_bound_gens.iter().map(|ident| {
            quote! { #ident: Decode }
        });
        let bounds_decode_mem_tracking = mel_bound_gens.iter().map(|ident| {
            quote! { #ident: DecodeWithMemTracking }
        });

        quote! {
            #[codec(encode_bound( #(#bounds_encode),*))]
            #[codec(decode_bound( #(#bounds_decode),*))]
            #[codec(decode_with_mem_tracking_bound( #(#bounds_decode_mem_tracking),*))]
            #[codec(mel_bound( #(#bounds_mel),* ))]
        }
    } else {
        quote! {}
    };

    // Retain the skip_list attribute as before.
    let skip_list = if !no_bound_params.is_empty() {
        quote! {
            #[scale_info(skip_type_params(#(#no_bound_params),*))]
        }
    } else {
        quote! {}
    };

    // Choose derive macros. (Left unchanged.)
    let span = input.ident.span();
    let (partial_eq_i, eq_i, clone_i, debug_i) =
        if !no_bound_params.is_empty() {
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

    // let mem_tracking_derive = quote! {
    //     #[cfg_attr(test, derive(DecodeWithMemTracking))]
    // };

    let struct_ident = &input.ident;
    let (_generics, _ty_generics, where_clause) = input.generics.split_for_impl();
    let generics = &input.generics;
    let attrs = &input.attrs;
    let vis = &input.vis;

    let common_derives = quote! {
        #[derive(
            #partial_eq_i,
            #clone_i,
            #eq_i,
            #debug_i,
            Encode,
            Decode,
            DecodeWithMemTracking,
            TypeInfo,
            MaxEncodedLen
        )]
    };

    let common_attrs = quote! {
        #common_derives
        // #mem_tracking_derive
        #skip_list
        #codec_mel_bound_attr
        #still_bind_attr
        #(#attrs)*
    };

    let expanded = match input.data {
        Data::Struct(ref data_struct) => match data_struct.fields {
            syn::Fields::Named(ref fields) => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #generics #where_clause #fields
                }
            },
            syn::Fields::Unnamed(ref fields) => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #generics #fields #where_clause;
                }
            },
            syn::Fields::Unit => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #generics #where_clause;
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
                #vis enum #struct_ident #generics #where_clause {
                    #(#variant_tokens),*
                }
            }
        },
        Data::Union(_) => {
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
fn parse_stored_args(args: TokenStream) -> (Vec<Ident>, Option<Vec<MelBoundItem>>) {
    let mut no_bounds = Vec::new();
    let mut mel_bounds: Option<Vec<MelBoundItem>> = None;
    if args.is_empty() {
        return (no_bounds, None);
    }
    let parsed = Punctuated::<syn::Meta, Token![,]>::parse_terminated
        .parse2(args.into())
        .unwrap_or_default();
    for meta in parsed {
        if let syn::Meta::List(meta_list) = meta {
            if let Some(ident) = meta_list.path.get_ident() {
                if ident == "no_bounds" {
                    let ident_list: IdentList = syn::parse2(meta_list.tokens).unwrap_or_default();
                    no_bounds.extend(ident_list.0.into_iter());
                } else if ident == "mel_bounds" {
                    let mel_bound_list: MelBoundList = syn::parse2(meta_list.tokens).unwrap_or_default();
                    mel_bounds = Some(mel_bound_list.0.into_iter().collect());
                }
            }
        }
    }
    (no_bounds, mel_bounds)
}