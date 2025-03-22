use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Ident, Token, Type
};
use syn::punctuated::Punctuated;
use syn::parse::{Parse, ParseStream, Parser};
use frame_support_procedural_tools::generate_access_from_frame_or_crate;

#[derive(Default)]
struct IdentList(Punctuated<Ident, Token![,]>);

impl Parse for IdentList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let idents = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;
        Ok(IdentList(idents))
    }
}

#[derive(Clone)]
struct CodecBoundItem {
    ty: Type,
    bound: Option<Type>,
}

impl Parse for CodecBoundItem {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ty: Type = input.parse()?;
        let bound = if input.peek(Token![:]) {
            let _colon: Token![:] = input.parse()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(CodecBoundItem { ty, bound })
    }
}

#[derive(Default)]
struct CodecBoundList(Punctuated<CodecBoundItem, Token![,]>);

impl Parse for CodecBoundList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let list = Punctuated::parse_terminated(input)?;
        Ok(CodecBoundList(list))
    }
}

pub fn stored(attr: TokenStream, input: TokenStream) -> TokenStream {
    let (skip_params, codec_bound_params) = parse_stored_args(attr);
    let mut input = parse_macro_input!(input as DeriveInput);

    let frame_support = match generate_access_from_frame_or_crate("frame-support") {
		Ok(path) => path,
		Err(err) => return err.to_compile_error().into(),
	};

    input.attrs.retain(|attr| !attr.path().is_ident("stored"));

    let still_bind_attr = if !skip_params.is_empty() {
        let still_bound_gens: Vec<_> = input.generics.params.iter().filter_map(|param| {
            if let syn::GenericParam::Type(type_param) = param {
                if !skip_params.contains(&type_param.ident) {
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

    let codec_bound_attr = if let Some(codec_bounds) = codec_bound_params {
        let bounds_codec = codec_bounds.clone().into_iter().map(|item| {
            let CodecBoundItem { ty, bound } = item;
            if let Some(explicit_bound) = bound {
                quote! { #ty: #explicit_bound }
            } else {
                quote! { #ty: ::#frame_support::__private::codec::MaxEncodedLen }
            }
        });
        let bounds_encode = codec_bounds.clone().into_iter().map(|item| {
            let CodecBoundItem { ty, bound } = item;
            if let Some(explicit_bound) = bound {
                quote! { #ty: #explicit_bound }
            } else {
                quote! { #ty: ::#frame_support::__private::codec::Encode }
            }
        });
        let bounds_decode = codec_bounds.clone().into_iter().map(|item| {
            let CodecBoundItem { ty, bound } = item;
            if let Some(explicit_bound) = bound {
                quote! { #ty: #explicit_bound }
            } else {
                quote! { #ty: ::#frame_support::__private::codec::Decode }
            }
        });
        let bounds_decode_mem_tracking = codec_bounds.clone().into_iter().map(|item| {
            let CodecBoundItem { ty, bound } = item;
            if let Some(explicit_bound) = bound {
                quote! { #ty: #explicit_bound }
            } else {
                quote! { #ty: ::#frame_support::__private::codec::DecodeWithMemTracking }
            }
        });
        quote! {
            #[codec(encode_bound( #(#bounds_encode),*))]
            #[codec(decode_bound( #(#bounds_decode),*))]
            #[codec(decode_with_mem_tracking_bound( #(#bounds_decode_mem_tracking),*))]
            #[codec(codec_bound( #(#bounds_codec),* ))]
        }
    } else if !skip_params.is_empty() {
        let all_generics: Vec<_> = input.generics.params.iter().filter_map(|param| {
            if let syn::GenericParam::Type(type_param) = param {
                Some(&type_param.ident)
            } else {
                None
            }
        }).collect();
        let codec_bound_gens: Vec<_> = all_generics.into_iter()
            .filter(|ident| !skip_params.contains(ident))
            .collect();
        let bounds_codec = codec_bound_gens.iter().map(|ident| {
            quote! { #ident: ::#frame_support::__private::codec::MaxEncodedLen }
        });
        let bounds_encode = codec_bound_gens.iter().map(|ident| {
            quote! { #ident: ::#frame_support::__private::codec::Encode }
        });
        let bounds_decode = codec_bound_gens.iter().map(|ident| {
            quote! { #ident: ::#frame_support::__private::codec::Decode }
        });
        let bounds_decode_mem_tracking = codec_bound_gens.iter().map(|ident| {
            quote! { #ident: ::#frame_support::__private::codec::DecodeWithMemTracking }
        });

        quote! {
            #[codec(encode_bound( #(#bounds_encode),*))]
            #[codec(decode_bound( #(#bounds_decode),*))]
            #[codec(decode_with_mem_tracking_bound( #(#bounds_decode_mem_tracking),*))]
            #[codec(codec_bound( #(#bounds_codec),* ))]
        }
    } else {
        quote! {}
    };

    let skip_list = if !skip_params.is_empty() {
        quote! {
            #[scale_info(skip_type_params(#(#skip_params),*))]
        }
    } else {
        quote! {}
    };

    let prefix = if !skip_params.is_empty() {
        quote! { #frame_support }
    } else {
        quote! { #frame_support::pallet_prelude }
    };

    let (partial_eq_i, eq_i, clone_i, debug_i) =
        if !skip_params.is_empty() {
            (
                quote! { ::#prefix::PartialEqNoBound },
                quote! { ::#prefix::EqNoBound },
                quote! { ::#prefix::CloneNoBound },
                quote! { ::#prefix::RuntimeDebugNoBound },
            )
        } else {
            (
                quote! { ::core::cmp::PartialEq },
                quote! { ::core::cmp::Eq },
                quote! { ::core::clone::Clone },
                quote! { ::#prefix::RuntimeDebug },
            )
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
            ::#frame_support::__private::codec::MaxEncodedLen,
			::#frame_support::__private::codec::Encode,
			::#frame_support::__private::codec::Decode,
            ::#frame_support::__private::codec::DecodeWithMemTracking,
			::#frame_support::__private::scale_info::TypeInfo,
        )]
    };

    let common_attrs = quote! {
        #common_derives
        #skip_list
        #codec_bound_attr
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

fn parse_stored_args(args: TokenStream) -> (Vec<Ident>, Option<Vec<CodecBoundItem>>) {
    let mut skip = Vec::new();
    let mut codec_bounds: Option<Vec<CodecBoundItem>> = None;
    if args.is_empty() {
        return (skip, None);
    }
    let parsed = Punctuated::<syn::Meta, Token![,]>::parse_terminated
        .parse2(args.into())
        .unwrap_or_default();
    for meta in parsed {
        if let syn::Meta::List(meta_list) = meta {
            if let Some(ident) = meta_list.path.get_ident() {
                if ident == "skip" {
                    let ident_list: IdentList = syn::parse2(meta_list.tokens).unwrap_or_default();
                    skip.extend(ident_list.0.into_iter());
                } else if ident == "codec_bounds" {
                    let codec_bound_list: CodecBoundList = syn::parse2(meta_list.tokens).unwrap_or_default();
                    codec_bounds = Some(codec_bound_list.0.into_iter().collect());
                }
            }
        }
    }
    (skip, codec_bounds)
}