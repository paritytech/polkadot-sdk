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
	punctuated::Punctuated,
	spanned::Spanned,
	Error, Ident, Result, Token, WherePredicate,
};

mod keywords {
	syn::custom_keyword!(mel);
	syn::custom_keyword!(mel_bound);
}

/// Parsed arguments for the `#[derive_stored]` attribute.
struct StoredArgs {
	/// Generic parameters that require MaxEncodedLen.
	mel: Vec<Ident>,
	/// Custom MaxEncodedLen bounds to use instead of inferring.
	mel_bound: Option<Punctuated<WherePredicate, Token![,]>>,
}

impl Parse for StoredArgs {
	fn parse(input: ParseStream) -> Result<Self> {
		let mut mel = Vec::new();
		let mut mel_bound = None;

		let args = Punctuated::<StoredArg, Token![,]>::parse_terminated(input)?;

		for arg in args {
			match arg {
				StoredArg::Mel(idents) => {
					if !mel.is_empty() {
						return Err(Error::new(
							idents.first().unwrap().span(),
							"`mel` can only be specified once",
						))
					}
					mel = idents;
				},
				StoredArg::MelBound(bounds) => {
					if mel_bound.is_some() {
						return Err(Error::new(
							input.span(),
							"`mel_bound` can only be specified once",
						))
					}
					mel_bound = Some(bounds);
				},
			}
		}

		Ok(StoredArgs { mel, mel_bound })
	}
}

/// Individual argument in the `#[derive_stored(...)]` attribute.
enum StoredArg {
	/// `mel(A, B, C)`
	Mel(Vec<Ident>),
	/// `mel_bound(A: MaxEncodedLen, B: MaxEncodedLen + Encode)`
	MelBound(Punctuated<WherePredicate, Token![,]>),
}

impl Parse for StoredArg {
	fn parse(input: ParseStream) -> Result<Self> {
		let lookahead = input.lookahead1();
		if lookahead.peek(keywords::mel_bound) {
			input.parse::<keywords::mel_bound>()?;
			let content;
			syn::parenthesized!(content in input);
			let bounds = Punctuated::<WherePredicate, Token![,]>::parse_terminated(&content)?;
			Ok(StoredArg::MelBound(bounds))
		} else if lookahead.peek(keywords::mel) {
			input.parse::<keywords::mel>()?;
			let content;
			syn::parenthesized!(content in input);
			let idents = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
			Ok(StoredArg::Mel(idents.into_iter().collect()))
		} else {
			Err(lookahead.error())
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
	let args: StoredArgs = syn::parse2(attr)?;
	let input: syn::DeriveInput = syn::parse2(item)?;

	// Extract field types from the struct
	let field_types = match &input.data {
		syn::Data::Struct(data_struct) => match &data_struct.fields {
			syn::Fields::Named(fields) => {
				fields.named.iter().map(|f| &f.ty).collect::<Vec<_>>()
			},
			syn::Fields::Unnamed(fields) => {
				fields.unnamed.iter().map(|f| &f.ty).collect::<Vec<_>>()
			},
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

	// Generate codec mel_bound attribute
	let codec_attr = if let Some(ref mel_bound) = args.mel_bound {
		// Use custom bounds
		quote! {
			#[codec(mel_bound(#mel_bound))]
		}
	} else if !args.mel.is_empty() {
		// Generate bounds for mel parameters
		let mel_bounds = args.mel.iter().map(|ident| {
			quote! { #ident: ::codec::MaxEncodedLen }
		});
		quote! {
			#[codec(mel_bound(#(#mel_bounds),*))]
		}
	} else {
		quote! {}
	};

	// Generate derive_where with field-based bounds
	// This ensures consistent bounding strategy: bounds are applied to field types, not type parameters
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

	// Reconstruct the struct body
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
		_ => unreachable!(), // Already checked above
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
		#codec_attr
		#(#attrs)*
		#vis struct #name #generics #body
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use quote::quote;

	#[test]
	fn stored_parse_mel() {
		let input = quote! {
			mel(Votes)
		};
		let args: StoredArgs = syn::parse2(input).unwrap();
		assert_eq!(args.mel.len(), 1);
		assert_eq!(args.mel[0].to_string(), "Votes");
	}

	#[test]
	fn stored_parse_mel_bound() {
		let input = quote! {
			mel_bound(S: MaxEncodedLen)
		};
		let args: StoredArgs = syn::parse2(input).unwrap();
		assert!(args.mel_bound.is_some());
	}

	#[test]
	fn stored_rejects_duplicate_mel() {
		let input = quote! {
			mel(A), mel(B)
		};
		let result: Result<StoredArgs> = syn::parse2(input);
		assert!(result.is_err());
	}

	#[test]
	fn stored_macro_expands() {
		let attr = quote! { mel(Votes) };
		let item = quote! {
			pub struct Tally<Votes, Total> {
				pub ayes: Votes,
				dummy: PhantomData<Total>,
			}
		};
		let result = stored_impl(attr, item);
		assert!(result.is_ok());
	}
}
