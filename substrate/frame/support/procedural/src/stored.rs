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
//! This macro simplifies the definition of storage types by automatically generating
//! the appropriate derive macros. It handles skipping type parameters in TypeInfo
//! metadata and `MaxEncodedLen` bounds configuration.

use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
	parse::{Parse, ParseStream},
	punctuated::Punctuated,
	spanned::Spanned,
	Error, Ident, Result, Token, WherePredicate,
};

mod keywords {
	syn::custom_keyword!(skip);
	syn::custom_keyword!(mel);
	syn::custom_keyword!(mel_bound);
}

/// Parsed arguments for the `#[stored]` attribute.
struct StoredArgs {
	/// Generic parameters to exclude from TypeInfo metadata generation.
	/// These are typically indirectly-used types or phantom data.
	skip: Vec<Ident>,
	/// Generic parameters that require MaxEncodedLen.
	mel: Vec<Ident>,
	/// Custom MaxEncodedLen bounds to use instead of inferring.
	mel_bound: Option<Punctuated<WherePredicate, Token![,]>>,
}

impl Parse for StoredArgs {
	fn parse(input: ParseStream) -> Result<Self> {
		let mut skip = Vec::new();
		let mut mel = Vec::new();
		let mut mel_bound = None;

		let args = Punctuated::<StoredArg, Token![,]>::parse_terminated(input)?;

		for arg in args {
			match arg {
				StoredArg::Skip(idents) => {
					if !skip.is_empty() {
						return Err(Error::new(
							idents.first().unwrap().span(),
							"`skip` can only be specified once",
						))
					}
					skip = idents;
				},
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

		Ok(StoredArgs { skip, mel, mel_bound })
	}
}

/// Individual argument in the `#[stored(...)]` attribute.
enum StoredArg {
	/// `skip(A, B, C)`
	Skip(Vec<Ident>),
	/// `mel(A, B, C)`
	Mel(Vec<Ident>),
	/// `mel_bound(A: MaxEncodedLen, B: MaxEncodedLen + Encode)`
	MelBound(Punctuated<WherePredicate, Token![,]>),
}

impl Parse for StoredArg {
	fn parse(input: ParseStream) -> Result<Self> {
		let lookahead = input.lookahead1();
		if lookahead.peek(keywords::skip) {
			input.parse::<keywords::skip>()?;
			let content;
			syn::parenthesized!(content in input);
			let idents = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
			Ok(StoredArg::Skip(idents.into_iter().collect()))
		} else if lookahead.peek(keywords::mel_bound) {
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
	let args: StoredArgs = syn::parse2(attr)?;
	let mut input: syn::DeriveInput = syn::parse2(item)?;

	// Validate that skipped parameters are actually generic parameters
	for skip_param in &args.skip {
		if !input.generics.params.iter().any(|p| match p {
			syn::GenericParam::Type(tp) => &tp.ident == skip_param,
			_ => false,
		}) {
			return Err(Error::new(
				skip_param.span(),
				format!("generic parameter `{}` not found", skip_param),
			))
		}
	}

	// Validate mel parameters
	for mel_param in &args.mel {
		if !input.generics.params.iter().any(|p| match p {
			syn::GenericParam::Type(tp) => &tp.ident == mel_param,
			_ => false,
		}) {
			return Err(Error::new(
				mel_param.span(),
				format!("generic parameter `{}` not found", mel_param),
			))
		}
	}

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

	let name = &input.ident;
	let vis = &input.vis;
	let (impl_generics, _ty_generics, where_clause) = input.generics.split_for_impl();
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
		syn::Data::Enum(_) =>
			return Err(Error::new(
				input.span(),
				"#[stored] is only supported on structs, not enums",
			)),
		syn::Data::Union(_) =>
			return Err(Error::new(
				input.span(),
				"#[stored] is only supported on structs, not unions",
			)),
	};

	Ok(quote! {
		#[derive(
			::frame_support::CloneNoBound,
			::frame_support::PartialEqNoBound,
			::frame_support::EqNoBound,
			::frame_support::RuntimeDebugNoBound,
			::scale_info::TypeInfo,
			::codec::Encode,
			::codec::Decode,
			::codec::DecodeWithMemTracking,
			::codec::MaxEncodedLen,
		)]
		#scale_info_attr
		#codec_attr
		#(#attrs)*
		#vis struct #name #impl_generics #where_clause #body
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use quote::quote;

	#[test]
	fn stored_parse_skip_and_mel() {
		let input = quote! {
			skip(Total), mel(Votes)
		};
		let args: StoredArgs = syn::parse2(input).unwrap();
		assert_eq!(args.skip.len(), 1);
		assert_eq!(args.mel.len(), 1);
		assert_eq!(args.skip[0].to_string(), "Total");
		assert_eq!(args.mel[0].to_string(), "Votes");
	}

	#[test]
	fn stored_parse_mel_bound() {
		let input = quote! {
			skip(T), mel_bound(S: MaxEncodedLen)
		};
		let args: StoredArgs = syn::parse2(input).unwrap();
		assert_eq!(args.skip.len(), 1);
		assert!(args.mel_bound.is_some());
	}

	#[test]
	fn stored_rejects_duplicate_skip() {
		let input = quote! {
			skip(A), skip(B)
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
