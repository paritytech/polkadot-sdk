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

//! Implementation of the `#[stored]` attribute macro for storage types.
//!
//! This macro simplifies storage type definitions by automatically generating derives
//! with consistent field-based bounding strategy. It extracts field types and applies
//! bounds to those fields (like codec does), ensuring consistent behavior across all traits.

use frame_support_procedural_tools::generate_access_from_frame_or_crate;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
	parse::{Parse, ParseStream},
	spanned::Spanned,
	Error, Result,
};

/// Parsed arguments for the `#[stored]` attribute.
/// Currently no arguments are needed - codec uses its default field-bounding strategy.
struct StoredArgs;

impl Parse for StoredArgs {
	fn parse(input: ParseStream) -> Result<Self> {
		// Allow empty attributes or no attributes at all
		if input.is_empty() {
			Ok(StoredArgs)
		} else {
			// Codec derives use their default strategy which bounds fields automatically
			Err(Error::new(
				input.span(),
				"#[stored] does not accept arguments. Codec derives use their default \
				field-bounding strategy.",
			))
		}
	}
}

/// Main implementation of the `#[stored]` macro.
pub fn stored(
	attr: proc_macro::TokenStream,
	item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	match stored_impl(attr.into(), item.into()) {
		Ok(tokens) => tokens.into(),
		Err(e) => e.to_compile_error().into(),
	}
}

fn stored_impl(attr: TokenStream2, item: TokenStream2) -> Result<TokenStream2> {
	let _args: StoredArgs = syn::parse2(attr)?;
	let mut input: syn::DeriveInput = syn::parse2(item)?;

	// Get the frame_support crate path to use __private re-exports
	let frame_support = match generate_access_from_frame_or_crate("frame-support") {
		Ok(path) => path,
		Err(e) =>
			return Err(Error::new(
				proc_macro2::Span::call_site(),
				format!("Failed to find `frame-support` in dependencies: {}", e),
			)),
	};

	// Extract field types from structs or enums
	let field_types = match &input.data {
		syn::Data::Struct(data_struct) => match &data_struct.fields {
			syn::Fields::Named(fields) => fields.named.iter().map(|f| &f.ty).collect::<Vec<_>>(),
			syn::Fields::Unnamed(fields) =>
				fields.unnamed.iter().map(|f| &f.ty).collect::<Vec<_>>(),
			syn::Fields::Unit => Vec::new(),
		},
		syn::Data::Enum(data_enum) => {
			// Collect field types from all enum variants
			let mut field_types = Vec::new();
			for variant in &data_enum.variants {
				match &variant.fields {
					syn::Fields::Named(fields) => {
						field_types.extend(fields.named.iter().map(|f| &f.ty));
					},
					syn::Fields::Unnamed(fields) => {
						field_types.extend(fields.unnamed.iter().map(|f| &f.ty));
					},
					syn::Fields::Unit => {},
				}
			}
			field_types
		},
		syn::Data::Union(_) =>
			return Err(Error::new(
				input.span(),
				"#[stored] is only supported on structs and enums, not unions",
			)),
	};

	// Collect all type parameters for scale_info skip_type_params.
	// By default, we skip all type parameters in TypeInfo metadata as they're rarely needed.
	let all_type_params: Vec<_> = input
		.generics
		.params
		.iter()
		.filter_map(|p| match p {
			syn::GenericParam::Type(tp) => Some(&tp.ident),
			_ => None,
		})
		.collect();

	// Generate scale_info attribute to skip all type parameters
	if !all_type_params.is_empty() {
		let scale_info_attr: syn::Attribute = syn::parse_quote! {
			#[scale_info(skip_type_params(#(#all_type_params),*))]
		};
		input.attrs.insert(0, scale_info_attr);
	}

	// Generate derive_where with field-based bounds
	// This ensures consistent bounding strategy: bounds are applied to field types, not type
	// parameters. Codec derives use their default strategy which also bounds fields automatically.
	let derive_where_attr: syn::Attribute = if !field_types.is_empty() {
		syn::parse_quote! {
			#[derive_where(Clone, Eq, PartialEq, Debug; #(#field_types),*)]
		}
	} else {
		// For unit structs/enums, no field types to bound
		syn::parse_quote! {
			#[derive_where(Clone, Eq, PartialEq, Debug)]
		}
	};
	input.attrs.insert(0, derive_where_attr);

	// Add codec derives
	let codec_derive_attr: syn::Attribute = syn::parse_quote! {
		#[derive(
			#frame_support::__private::scale_info::TypeInfo,
			#frame_support::__private::codec::Encode,
			#frame_support::__private::codec::Decode,
			#frame_support::__private::codec::DecodeWithMemTracking,
			#frame_support::__private::codec::MaxEncodedLen,
		)]
	};
	input.attrs.insert(0, codec_derive_attr);

	Ok(quote! { #input })
}

#[cfg(test)]
mod tests {
	use super::*;
	use quote::quote;

	#[test]
	fn stored_accepts_empty_attributes() {
		let input = quote! {};
		let args: Result<StoredArgs> = syn::parse2(input);
		assert!(args.is_ok());
	}

	#[test]
	fn stored_rejects_arguments() {
		let input = quote! {
			some_arg
		};
		let result: Result<StoredArgs> = syn::parse2(input);
		assert!(result.is_err());
	}

	#[test]
	fn stored_macro_expands() {
		let attr = quote! {};
		let item = quote! {
			pub struct Tally<Votes, Total> {
				pub ayes: Votes,
				dummy: PhantomData<Total>,
			}
		};
		let result = stored_impl(attr, item);
		assert!(result.is_ok());
	}

	#[test]
	fn stored_extracts_field_types() {
		let attr = quote! {};
		let item = quote! {
			pub struct Foo<T: Config> {
				f: T::Foo,
				f2: Vec<T::Foo2>,
			}
		};
		let result = stored_impl(attr, item);
		assert!(result.is_ok());
		// The macro should extract T::Foo and Vec<T::Foo2> for derive_where
	}

	#[test]
	fn stored_supports_enums() {
		let attr = quote! {};
		let item = quote! {
			pub enum MyEnum<T: Config> {
				Variant1 { field: T::Foo },
				Variant2(Vec<T::Foo2>),
				Variant3,
			}
		};
		let result = stored_impl(attr, item);
		assert!(result.is_ok());
	}

	#[test]
	fn stored_rejects_unions() {
		let attr = quote! {};
		let item = quote! {
			pub union MyUnion {
				f1: u32,
				f2: u64,
			}
		};
		let result = stored_impl(attr, item);
		assert!(result.is_err());
	}
}
