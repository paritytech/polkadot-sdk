use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Ident, Token, Type
};
use syn::punctuated::Punctuated;
use syn::parse::{Parse, ParseStream, Parser};
use frame_support_procedural_tools::generate_access_from_frame_or_crate;

#[derive(Default)]
struct SkipList(Punctuated<Ident, Token![,]>);

impl Parse for SkipList {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let idents = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;
        Ok(SkipList(idents))
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

    let all_generics: Vec<_> = input.generics.params.iter().filter_map(|param| {
        if let syn::GenericParam::Type(type_param) = param {
            Some(&type_param.ident)
        } else {
            None
        }
    }).collect();

    let not_skipped_generics: Vec<_> = all_generics.into_iter()
    .filter(|gen| !skip_params.contains(gen))
    .collect();

    let still_bind_attr = if !skip_params.is_empty() && !not_skipped_generics.is_empty() {
        quote! {
            #[still_bind( #(#not_skipped_generics),* )]
        }
    } else {
        quote! {}
    };

    let mut codec_needed = true;
    let mut codec_bound_attr = quote! {};
    let (bounds_mel, bounds_encode, bounds_decode) = if let Some(codec_bounds) = codec_bound_params {
        (
            codec_bounds.iter()
                .map(|item| explicit_or_default_bound(item, quote! { ::#frame_support::__private::codec::MaxEncodedLen }))
                .collect::<Vec<_>>(),
            codec_bounds.iter()
                .map(|item| explicit_or_default_bound(item, quote! { ::#frame_support::__private::codec::Encode }))
                .collect::<Vec<_>>(),
            codec_bounds.iter()
                .map(|item| explicit_or_default_bound(item, quote! { ::#frame_support::__private::codec::Decode }))
                .collect::<Vec<_>>(),
        )
    } else if !skip_params.is_empty() {
        (
            not_skipped_generics.iter()
                .map(|ident| quote! { #ident: ::#frame_support::__private::codec::MaxEncodedLen })
                .collect::<Vec<_>>(),
            not_skipped_generics.iter()
                .map(|ident| quote! { #ident: ::#frame_support::__private::codec::Encode })
                .collect::<Vec<_>>(),
            not_skipped_generics.iter()
                .map(|ident| quote! { #ident: ::#frame_support::__private::codec::Decode })
                .collect::<Vec<_>>(),
        )
    } else {
        codec_needed = false;
        (vec![], vec![], vec![])
    };

    if codec_needed {
        codec_bound_attr = quote! {
            #[codec(encode_bound( #(#bounds_encode),*))]
            #[codec(decode_bound( #(#bounds_decode),*))]
            #[codec(mel_bound( #(#bounds_mel),*))]
        };
    }

    let use_no_bounds_derives = !skip_params.is_empty();

    let mut scale_skip_attr = quote! {};
    if use_no_bounds_derives {
        scale_skip_attr = quote! {
            #[scale_info(skip_type_params(#(#skip_params),*))]
        }
    }
    
    let (partial_eq_i, eq_i, clone_i, debug_i) =
        if use_no_bounds_derives {
            (
                quote! { ::#frame_support::PartialEqNoBound },
                quote! { ::#frame_support::EqNoBound },
                quote! { ::#frame_support::CloneNoBound },
                quote! { ::#frame_support::RuntimeDebugNoBound },
            )
        } else {
            (
                quote! { ::core::cmp::PartialEq },
                quote! { ::core::cmp::Eq },
                quote! { ::core::clone::Clone },
                quote! { ::#frame_support::pallet_prelude::RuntimeDebug },
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
            // ::#frame_support::__private::codec::DecodeWithMemTracking,
			::#frame_support::__private::scale_info::TypeInfo,
        )]
    };

    let common_attrs = quote! {
        #common_derives
        #still_bind_attr
        #scale_skip_attr
        #codec_bound_attr
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
            let variant_tokens = data_enum.variants
                .iter()
                .map(|variant| quote! { #variant });
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
    let mut codec_bounds = None;
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
                    let ident_list: SkipList =
                        syn::parse2(meta_list.tokens).unwrap_or_default();
                    skip.extend(ident_list.0.into_iter());
                } else if ident == "codec_bounds" {
                    let codec_bound_list: CodecBoundList =
                        syn::parse2(meta_list.tokens).unwrap_or_default();
                    codec_bounds = Some(codec_bound_list.0.into_iter().collect());
                }
            }
        }
    }
    (skip, codec_bounds)
}

fn explicit_or_default_bound(item: &CodecBoundItem, default_bound: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let ty = &item.ty;
    if let Some(ref explicit_bound) = item.bound {
        quote! { #ty: #explicit_bound }
    } else {
        quote! { #ty: #default_bound }
    }
}