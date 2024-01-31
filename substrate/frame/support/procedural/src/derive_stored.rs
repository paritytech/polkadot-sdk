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

//! Implements the `#[frame_support::stored]` attribute macro.

use frame_support_procedural_tools::generate_access_from_frame_or_crate;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::ToTokens;
use syn::{
	parse::{Parse, ParseBuffer},
	parse_macro_input,
	punctuated::Punctuated,
	DeriveInput, Result, Token, Type,
};

/// Derive all traits that are needed to place a type into FRAME storage.
///
/// This function only parses the inputs and then delegates the actual work to
/// `derive_frame_stored_inner`.
pub fn derive_frame_stored(attrs: TokenStream, input: TokenStream) -> TokenStream {
	let frame_support = match generate_access_from_frame_or_crate("frame-support") {
		Ok(path) => path,
		Err(err) => return err.to_compile_error().into(),
	};

	let input = parse_macro_input!(input as DeriveInput);
	let attrs = parse_macro_input!(attrs as CustomAttributes);

	quote::quote! {
		#[derive(
			::#frame_support::__private::codec::MaxEncodedLen,
			::#frame_support::__private::codec::Encode,
			::#frame_support::__private::codec::Decode,
			::#frame_support::__private::scale_info::TypeInfo,
		)]
		#attrs
		#input
	}
	.into()
}

mod keywords {
	syn::custom_keyword!(skip);
	syn::custom_keyword!(mel_bound);
	syn::custom_keyword!(mel);
}

/// Custom meta attributes for the `#[frame_support::stored(..)]` macro.
pub struct CustomAttributes(Vec<CustomAttribute>);

impl Parse for CustomAttributes {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		if input.is_empty() {
			return Ok(Self(Vec::new()))
		}

		let attrs = input.parse_terminated(CustomAttribute::parse, Token![,])?;
		Ok(Self(attrs.into_iter().collect()))
	}
}

impl ToTokens for CustomAttributes {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		for attr in &self.0 {
			attr.to_tokens(tokens);
		}
	}
}

/// A custom attribute helper for the `#[frame_support::stored]` attribute.
///
/// Can be used to tweak the behaviour of the attribute.
#[derive(Debug, Clone)]
pub enum CustomAttribute {
	/// Skip trait bounds for the given types.
	Skip(SkipAttribute),
	/// Set explicit MEL bounds for a type.
	MelBound(MelBoundAttribute),
	/// Use the default `MaxEncodedLen` bound for MEL fulfillment.
	Mel(MelAttribute),
}

impl Parse for CustomAttribute {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		let lookahead = input.lookahead1();

		if lookahead.peek(keywords::skip) {
			input.parse().map(CustomAttribute::Skip)
		} else if lookahead.peek(keywords::mel) {
			input.parse().map(CustomAttribute::Mel)
		} else if lookahead.peek(keywords::mel_bound) {
			input.parse().map(CustomAttribute::MelBound)
		} else {
			Err(lookahead.error())
		}
	}
}

impl ToTokens for CustomAttribute {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		match self {
			CustomAttribute::Skip(attr) => attr.to_tokens(tokens),
			CustomAttribute::MelBound(attr) => attr.to_tokens(tokens),
			CustomAttribute::Mel(attr) => attr.to_tokens(tokens),
		}
	}
}

/// Do not apply any trait bounds to the given types.
#[derive(Debug, Clone)]
pub struct SkipAttribute {
	types: Punctuated<Type, Token![,]>,
}

impl Parse for SkipAttribute {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		input.parse::<keywords::skip>()?;

		let content;
		syn::parenthesized!(content in input);

		let types = content.parse_terminated(Type::parse, Token![,])?;
		Ok(Self { types })
	}
}

impl ToTokens for SkipAttribute {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let tys = self.types.iter().collect::<Vec<_>>();

		tokens.extend(quote::quote! {
			#[scale_info(skip_type_params(#(#tys),*))]
			#[codec(encode_bound(skip_type_params(#(#tys),*)))]
			#[codec(decode_bound(skip_type_params(#(#tys),*)))]
			#[codec(mel_bound(skip_type_params(#(#tys),*)))]
		});
	}
}

/// Apply a specific bound to each type.
#[derive(Debug, Clone)]
pub struct MelBoundAttribute {
	bounds: Vec<MelBound>,
}

impl Parse for MelBoundAttribute {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		input.parse::<keywords::mel_bound>()?;

		let content;
		syn::parenthesized!(content in input);

		let mut bounds = Vec::new();
		loop {
			if content.is_empty() {
				break
			}

			let bound: MelBound = content.parse()?;
			bounds.push(bound);

			let lookahead = content.lookahead1();
			if lookahead.peek(Token![,]) {
				content.parse::<Token![,]>()?;
			}
		}

		Ok(Self { bounds })
	}
}

impl ToTokens for MelBoundAttribute {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		for bound in &self.bounds {
			bound.to_tokens(tokens);
		}
	}
}

/// Require `ty` to fullfil the given bounds to be eligible for MEL.
#[derive(Debug, Clone)]
pub struct MelBound {
	ty: Type,
	bounds: Punctuated<Type, Token![+]>,
}

impl Parse for MelBound {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		let ty = input.parse().unwrap();
		let lookahead = input.lookahead1();

		if lookahead.peek(Token![:]) {
			input.parse::<Token![:]>().unwrap();
		} else {
			return Ok(Self { ty, bounds: Punctuated::new() })
		}

		let mut bounds = Punctuated::new();
		loop {
			bounds.push(input.parse().unwrap());
			if input.is_empty() {
				break
			}

			let lookahead = input.lookahead1();

			if lookahead.peek(Token![+]) {
				input.parse::<Token![+]>().unwrap();
			} else if lookahead.peek(Token![,]) {
				break
			} else {
				break
			}
		}

		Ok(Self { ty, bounds })
	}
}

impl ToTokens for MelBound {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let ty = &self.ty;
		let bounds = self.bounds.iter().collect::<Vec<_>>();

		tokens.extend(quote::quote! {
			#[codec(mel_bound(#ty: #(#bounds)+*))]
			#[codec(encode_bound(#ty: #(#bounds)+*))]
			#[codec(decode_bound(#ty: #(#bounds)+*))]
		});
	}
}

/// Require all `tys` to fullfil the `MaxEncodedLen` bound.
#[derive(Debug, Clone)]
pub struct MelAttribute {
	tys: Punctuated<Type, Token![,]>,
}

impl Parse for MelAttribute {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		input.parse::<keywords::mel>()?;

		let content;
		syn::parenthesized!(content in input);

		let tys = content.parse_terminated(Type::parse, Token![,])?;
		Ok(Self { tys })
	}
}

impl ToTokens for MelAttribute {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let frame_support = match generate_access_from_frame_or_crate("frame-support") {
			Ok(path) => path,
			Err(err) => return tokens.extend(err.to_compile_error()),
		};
		let tys = self.tys.iter();

		tokens.extend(quote::quote! {
			#[codec(mel_bound(
				#(
					#tys: ::#frame_support::__private::codec::MaxEncodedLen
				),*
			))]
		});
	}
}
