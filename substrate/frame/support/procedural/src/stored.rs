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

//! Implementation of the `#[derive_stored]` attribute macro for storage types.
//!
//! This macro simplifies storage type definitions by automatically generating derives
//! with consistent field-based bounding strategy. It extracts field types and applies
//! bounds to those fields (like codec does), ensuring consistent behavior across all traits.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
	parse::{Parse, ParseStream},
	spanned::Spanned,
	Error, Result,
};

/// Parsed arguments for the `#[derive_stored]` attribute.
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
				"#[derive_stored] does not accept arguments. Codec derives use their default \
				field-bounding strategy.",
			))
		}
	}
}

/// Main implementation of the `#[derive_stored]` macro.
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
	let input: syn::DeriveInput = syn::parse2(item)?;

	let field_types = match &input.data {
		syn::Data::Struct(data_struct) => match &data_struct.fields {
			syn::Fields::Named(fields) => fields.named.iter().map(|f| &f.ty).collect::<Vec<_>>(),
			syn::Fields::Unnamed(fields) =>
				fields.unnamed.iter().map(|f| &f.ty).collect::<Vec<_>>(),
			syn::Fields::Unit => Vec::new(),
		},
		syn::Data::Enum(_) =>
			return Err(Error::new(
				input.span(),
				"#[derive_stored] is only supported on structs, not enums",
			)),
		syn::Data::Union(_) =>
			return Err(Error::new(
				input.span(),
				"#[derive_stored] is only supported on structs, not unions",
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
	let scale_info_attr = if !all_type_params.is_empty() {
		quote! {
			#[scale_info(skip_type_params(#(#all_type_params),*))]
		}
	} else {
		quote! {}
	};

	// Generate derive_where with field-based bounds
	// This ensures consistent bounding strategy: bounds are applied to field types, not type
	// parameters. Codec derives use their default strategy which also bounds fields automatically.
	let derive_where_attr = if !field_types.is_empty() {
		quote! {
			#[derive_where(Clone, Eq, PartialEq, Debug; #(#field_types),*)]
		}
	} else {
		// For unit structs, no field types to bound
		quote! {
			#[derive_where(Clone, Eq, PartialEq, Debug)]
		}
	};

	let name = &input.ident;
	let vis = &input.vis;
	let generics = &input.generics;
	let attrs = &input.attrs;

	let body = match &input.data {
		syn::Data::Struct(data_struct) => match &data_struct.fields {
			syn::Fields::Named(fields) => {
				let named = &fields.named;
				quote! { { #named } }
			},
			syn::Fields::Unnamed(fields) => {
				let unnamed = &fields.unnamed;
				quote! { ( #unnamed ); }
			},
			syn::Fields::Unit => quote! { ; },
		},
		_ => unreachable!(
			"input.data is already matched above for Struct/Enum/Union;\
			all variants covered;\
			qed"
		),
	};

	Ok(quote! {
		#derive_where_attr
		#[derive(
			::scale_info::TypeInfo,
			::codec::Encode,
			::codec::Decode,
			::codec::DecodeWithMemTracking,
			::codec::MaxEncodedLen,
		)]
		#scale_info_attr
		#(#attrs)*
		#vis struct #name #generics #body
	})
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
}
