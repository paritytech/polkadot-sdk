use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Ident, Token, Type, TypeParamBound,
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

/// Represents a single mel_bounds item, e.g.:
/// - `S`            (shorthand for `S: MaxEncodedLen`)
/// - `T: Default`   (explicit bound, used as-is)
/// - Complex types like `BlockNumberFor<T>` are allowed.
struct MelBoundItem {
    ty: Type,
    bound: Option<Type>,
}

impl Parse for MelBoundItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Parse an arbitrary type (which could be a simple identifier or a more complex type).
        let ty: Type = input.parse()?;
        // If a colon is present, parse the explicit bound.
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

/// The #[stored] attribute. To be used for structs or enums that will find themselves placed in
/// runtime storage.
///
/// It does the following:
/// - Parses attribute `no_bounds` and `mel_bounds` arguments. If `mel_bounds` is provided, it takes
///   precedence over `no_bounds` for generating the codec attribute.
/// - Adds default trait bounds to any type parameter that should be bounded.
/// - Conditionally adds `#[codec(mel_bound(...))]`:
///     - If `mel_bounds` is provided, each bare item (e.g. `S` or `BlockNumberFor<T>`)
///       is expanded to `...: MaxEncodedLen` unless an explicit bound is provided.
///     - Otherwise, if `no_bounds` is provided, then all generics not listed in `no_bounds` get
///       `: MaxEncodedLen` (even if that results in an empty list, the attribute is still added).
/// - Generates the necessary derive attributes and other metadata required for storage.
pub fn stored(attr: TokenStream, input: TokenStream) -> TokenStream {
    // Parse stored attribute arguments.
    // Returns a tuple: (no_bounds identifiers, optional mel_bounds items)
    let (no_bound_params, mel_bound_params) = parse_stored_args(attr);
    let mut input = parse_macro_input!(input as DeriveInput);

    // Remove the #[stored] attribute to prevent re-emission.
    input.attrs.retain(|attr| !attr.path().is_ident("stored"));

    // Should we use the NoBound version of derives?
    let should_nobound_derive = !no_bound_params.is_empty();
    if should_nobound_derive {
        add_normal_trait_bounds(&mut input.generics, &no_bound_params);
    }

    // Compute the #[codec(mel_bound(...))] attribute.
    let codec_mel_bound_attr = if let Some(mel_bounds) = mel_bound_params {
        let bounds = mel_bounds.into_iter().map(|item| {
            let MelBoundItem { ty, bound } = item;
            if let Some(explicit_bound) = bound {
                quote! { #ty: #explicit_bound }
            } else {
                quote! { #ty: MaxEncodedLen }
            }
        });
        quote! {
            #[codec(mel_bound( #(#bounds),* ))]
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
        let bounds = mel_bound_gens.iter().map(|ident| {
            quote! { #ident: MaxEncodedLen }
        });
        quote! {
            #[codec(mel_bound( #(#bounds),* ))]
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

    let skip_list = if should_nobound_derive && !no_bound_params.is_empty() {
        quote! {
            #[scale_info(skip_type_params(#(#no_bound_params),*))]
        }
    } else {
        quote! {}
    };

    let mem_tracking_derive = quote! {
        #[cfg_attr(test, derive(DecodeWithMemTracking))]
    };

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
/// For example, given:
///   #[stored(no_bounds(A, B), mel_bounds(U, BlockNumberFor<T>))]
/// this function extracts A and B into no_bounds and U, BlockNumberFor<T> into mel_bounds.
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

    for param in &mut generics.params {
        if let syn::GenericParam::Type(type_param) = param {
            if !no_bound_params.contains(&type_param.ident) {
                for bound_name in &normal_bounds {
                    let bound: TypeParamBound = syn::parse_str(bound_name)
                        .unwrap_or_else(|_| panic!("Failed to parse bound: {}", bound_name));
                    type_param.bounds.push(bound);
                }
            }
        }
    }
}