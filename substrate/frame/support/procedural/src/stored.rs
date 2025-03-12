use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Ident, Meta, Token, TypeParamBound,
};
use syn::punctuated::Punctuated;
use syn::parse::{Parse, ParseStream};

/// A helper struct to hold a comma-separated list of identifiers, ie `no_bounds(A, B, C)`.
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
/// - Parses attribute no_bounds argument to determines type parameters that should not be bounded.
/// - Adds default trait bounds to any type parameter that should be bounded.
/// - Generates the necessary derive attributes and other metadata required for storage in a substrate runtime.
/// - Adjusts those attribute based on whether the item is a struct or an enum.
/// - Utilizes the `NoBound` version of the derives if there's a type parameter that should be unbounded.
pub fn stored(attr: TokenStream, input: TokenStream) -> TokenStream {
    // Initial parsing.
    let no_bound_params = parse_no_bounds_from_args(attr);
    let mut input = parse_macro_input!(input as DeriveInput);

    // Remove the #[stored] attribute to prevent re-emission.
    input.attrs.retain(|attr| !attr.path().is_ident("stored"));

    // Should we use NoBounds version of derives?
    let should_nobound_derive = !no_bound_params.is_empty();
    if should_nobound_derive {
        // Add standard trait bounds to any generics that need bounds still.
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

    // Add scale_info attribute if necessary.
    let skip_list = if should_nobound_derive && !no_bound_params.is_empty() {
        quote! {
            #[scale_info(skip_type_params(#(#no_bound_params),*))]
        }
    } else {
        quote! {}
    };

    // Adjust serde bounds if necessary.
    let serde_attrs = if should_nobound_derive {
        quote! {
            #[serde(bound(serialize = "", deserialize = ""))]
        }
    } else {
        quote! {}
    };

    // Input extraction.
    let struct_ident = &input.ident;
    let (impl_generics, _ty_generics, where_clause) = input.generics.split_for_impl();
    let attrs = &input.attrs;
    let vis = &input.vis;

    // Switch behaviour depending on Struct or Enum.
    let is_enum = matches!(input.data, Data::Enum(_));
    let common_derive = if is_enum {
        quote! {
            #[derive(
                #default_i,
                #partial_eq_i,
                #eq_i,
                #clone_i,
                Copy,
                Encode,
                Decode,
                DecodeWithMemTracking,
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
                Copy,
                Encode,
                Decode,
                DecodeWithMemTracking,
                #debug_i,
                TypeInfo,
                MaxEncodedLen,
                Serialize,
                Deserialize
            )]
        }
    };

    // Combination.
    let common_attrs = quote! {
        #common_derive
        #skip_list
        #serde_attrs
        #(#attrs)*
    };

    // Appropriate ordering for each type of input.
    let expanded = match input.data {
        Data::Struct(ref data_struct) => match data_struct.fields {
            // Named-field structs.
            syn::Fields::Named(ref fields) => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #impl_generics #where_clause #fields
                }
            },
            // Tuple structs.
            syn::Fields::Unnamed(ref fields) => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #impl_generics #fields #where_clause;
                }
            },
            // Unit structs.
            syn::Fields::Unit => {
                quote! {
                    #common_attrs
                    #vis struct #struct_ident #impl_generics #where_clause;
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
                #vis enum #struct_ident #impl_generics #where_clause {
                    #(#variant_tokens),*
                }
            }
        },
        Data::Union(_) => {
            // Unions are not supported.
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

/// Extract a list of type parameters that should not be bounded from the attribute arguments.
///
/// For example, given `#[stored(no_bounds(A, B))]`, this function extracts A and B.
fn parse_no_bounds_from_args(args: TokenStream) -> Vec<Ident> {
    if args.is_empty() {
        return Vec::new();
    }
    // Parse the arguments into a Meta representation.
    let meta = syn::parse::<Meta>(args).unwrap();
    // Check if the attribute is "no_bounds".
    if meta.path().is_ident("no_bounds") {
        if let Meta::List(meta_list) = meta {
            // Parse the inner tokens as an IdentList.
            let ident_list: IdentList = syn::parse2(meta_list.tokens).unwrap_or_default();
            return ident_list.0.into_iter().collect();
        }
    }
    Vec::new()
}

/// Adds standard trait bounds to generic parameters of the input type,
/// except for those parameters listed in `no_bound_params`.
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

    // For each param.
    for param in &mut generics.params {
        if let syn::GenericParam::Type(type_param) = param {
            // But not those in no_bound_params.
            if !no_bound_params.contains(&type_param.ident) {
                for bound_name in normal_bounds {
                    // Add the bound.
                    let bound: TypeParamBound = syn::parse_str(bound_name)
                        .unwrap_or_else(|_| panic!("Failed to parse bound: {}", bound_name));
                    type_param.bounds.push(bound);
                }
            }
        }
    }
}