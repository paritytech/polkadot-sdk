// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The `stored` macro. Used for tagging types to be used in Substrate runtime storage.

use frame_support_procedural_tools::generate_access_from_frame_or_crate;
use proc_macro::TokenStream;
use quote::quote;
use syn::{
	parse::{Parse, ParseStream, Parser},
	parse_macro_input,
	punctuated::Punctuated,
	Data, DeriveInput, Ident, Token, Type,
};

/// Helper to parse the `skip` attribute
#[derive(Default)]
struct SkipList(Punctuated<Ident, Token![,]>);

impl Parse for SkipList {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		let idents = Punctuated::<Ident, Token![,]>::parse_terminated(input)?;
		Ok(SkipList(idents))
	}
}

/// Helper to parse the `codec_bounds` attribute.
#[derive(Default)]
struct CodecBoundList(Punctuated<CodecBoundItem, Token![,]>);

impl Parse for CodecBoundList {
	fn parse(input: ParseStream) -> syn::Result<Self> {
		let list = Punctuated::parse_terminated(input)?;
		Ok(CodecBoundList(list))
	}
}

/// An item in a `CodecBoundList`.
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

/// Generates the derives and attributes
/// necessary for types to be used in substrate storage.
///
/// The `#[stored]` macro modifies the annotated type by:
/// - Generating the required derives, using either the standard or `NoBound` variants.
/// - Determining which type parameters should be bound and which shouldn't.
///
/// # Attributes
///
/// - `skip`: A list of type parameters to exclude from recieving automatic bounds, e.g.,
///   `#stored(skip(T, U))`.
/// - `codec_bounds`: A list of type parameters which should recieve default codec bounds, e.g.,
///   `#[stored(codec_bounds(T))]`. If not used, codec bounds will be inferred. In addition,
///   explicit codec bounds can be specified, e.g., `#[stored(codec_bounds(T: Default + Clone +
///   Encode))]`, but is usually not necessary.
///
/// # Example
///
/// ```rust,ignore
/// #[stored(skip(T))]
/// struct MyStruct<T: Config, U> {
///     field1: T,
///     field2: U,
/// }
/// ```
///
/// In the above example, the generic `T` is excluded from being bounded. `U` will still be bound
/// by all derives.
///
/// # Errors
///
/// This macro will generate a compile-time error if applied to a union type.
pub fn stored(attr: TokenStream, input: TokenStream) -> TokenStream {
	// Parse input and attributes.
	let (skip_params, codec_bound_params) = parse_stored_args(attr);
	let mut input = parse_macro_input!(input as DeriveInput);

	// Find path to frame support.
	let frame_support = match generate_access_from_frame_or_crate("frame-support") {
		Ok(path) => path,
		Err(err) => return err.to_compile_error().into(),
	};

	// Remove stored attribute from output.
	input.attrs.retain(|attr| !attr.path().is_ident("stored"));

	// Build useful lists and flags.
	let all_generics: Vec<_> = input
		.generics
		.params
		.iter()
		.filter_map(|param| {
			if let syn::GenericParam::Type(type_param) = param {
				Some(&type_param.ident)
			} else {
				None
			}
		})
		.collect();
	let not_skipped_generics: Vec<_> =
		all_generics.into_iter().filter(|gen| !skip_params.contains(gen)).collect();
	let use_no_bounds_derives = !skip_params.is_empty();

	// Use `still_bind` attribute if using NoBounds derives & some generics haven't been skipped.
	let mut still_bind_attr = quote! {};
	if use_no_bounds_derives && !not_skipped_generics.is_empty() {
		still_bind_attr = quote! {
			#[still_bind( #(#not_skipped_generics),* )]
		}
	}

	// Build various codec attributes if necessary.
	let mut codec_needed = true;
	let mut codec_bound_attr = quote! {};
	let (bounds_mel, bounds_encode, bounds_decode) = if let Some(codec_bounds) = codec_bound_params
	{
		// `codec_bounds` was passed explicitly.
		(
			codec_bounds
				.iter()
				.map(|item| {
					explicit_or_default_bound(
						item,
						quote! { ::#frame_support::__private::codec::MaxEncodedLen },
					)
				})
				.collect::<Vec<_>>(),
			codec_bounds
				.iter()
				.map(|item| {
					explicit_or_default_bound(
						item,
						quote! { ::#frame_support::__private::codec::Encode },
					)
				})
				.collect::<Vec<_>>(),
			codec_bounds
				.iter()
				.map(|item| {
					explicit_or_default_bound(
						item,
						quote! { ::#frame_support::__private::codec::Decode },
					)
				})
				.collect::<Vec<_>>(),
		)
	} else if !skip_params.is_empty() {
		// `codec_bounds` was not passed explicitly, generate codec bounds based on what generics
		// were not skipped.
		(
			not_skipped_generics
				.iter()
				.map(|ident| quote! { #ident: ::#frame_support::__private::codec::MaxEncodedLen })
				.collect::<Vec<_>>(),
			not_skipped_generics
				.iter()
				.map(|ident| quote! { #ident: ::#frame_support::__private::codec::Encode })
				.collect::<Vec<_>>(),
			not_skipped_generics
				.iter()
				.map(|ident| quote! { #ident: ::#frame_support::__private::codec::Decode })
				.collect::<Vec<_>>(),
		)
	} else {
		// Various codec bounds not needed.
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

	// Use `scale_info` if NoBounds derives were used.
	let mut scale_skip_attr = quote! {};
	if use_no_bounds_derives {
		scale_skip_attr = quote! {
			#[scale_info(skip_type_params(#(#skip_params),*))]
		}
	}

	// Select between standard or NoBound derives.
	let (partial_eq_i, eq_i, clone_i, debug_i) = if use_no_bounds_derives {
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

	// Put it all together and expand.
	let struct_ident = &input.ident;
	let (_generics, _ty_generics, where_clause) = input.generics.split_for_impl();
	let generics = &input.generics;
	let attrs = &input.attrs;
	let vis = &input.vis;

	let common_attrs = quote! {
		#common_derives
		#still_bind_attr
		#scale_skip_attr
		#codec_bound_attr
		#(#attrs)*
	};

	expand(&input, common_attrs, vis, struct_ident, generics, where_clause).into()
}

/// Helper to parse possible attributes `skip` & `codec_bounds`.
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
					let ident_list: SkipList = syn::parse2(meta_list.tokens).unwrap_or_default();
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

/// Helper for macro expansion based on input type and structure.
fn expand(
	input: &DeriveInput,
	common_attrs: proc_macro2::TokenStream,
	vis: &syn::Visibility,
	struct_ident: &Ident,
	generics: &syn::Generics,
	where_clause: Option<&syn::WhereClause>,
) -> proc_macro2::TokenStream {
	match &input.data {
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
			let variant_tokens = data_enum.variants.iter().map(|variant| quote! { #variant });
			quote! {
				#common_attrs
				#vis enum #struct_ident #generics #where_clause {
					#(#variant_tokens),*
				}
			}
		},
		Data::Union(_) =>
			return syn::Error::new_spanned(
				&input,
				"The #[stored] attribute cannot be used on unions.",
			)
			.to_compile_error()
			.into(),
	}
}

/// Helper to either use an explicitly passed bound or a default one.
fn explicit_or_default_bound(
	item: &CodecBoundItem,
	default_bound: proc_macro2::TokenStream,
) -> proc_macro2::TokenStream {
	let ty = &item.ty;
	if let Some(ref explicit_bound) = item.bound {
		quote! { #ty: #explicit_bound }
	} else {
		quote! { #ty: #default_bound }
	}
}
