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
pub fn derive_frame_stored(attrs: TokenStream, input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);
	let attrs = parse_macro_input!(attrs as CustomAttributes);

	let frame_support = match generate_access_from_frame_or_crate("frame-support") {
		Ok(path) => path,
		Err(err) => return err.to_compile_error().into(),
	};

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
pub struct CustomAttributes(Punctuated<CustomAttribute, Token![,]>);

impl Parse for CustomAttributes {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		Punctuated::parse_terminated(input).map(Self)
	}
}

impl ToTokens for CustomAttributes {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		for attr in &self.0 {
			attr.to_tokens(tokens);
		}
	}
}

/// A custom attribute helper for the `#[frame_support::stored(..)]` attribute.
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
pub struct SkipAttribute(Punctuated<Type, Token![,]>);

impl Parse for SkipAttribute {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		input.parse::<keywords::skip>()?;

		let content;
		syn::parenthesized!(content in input);

		content.parse_terminated(Type::parse, Token![,]).map(Self)
	}
}

impl ToTokens for SkipAttribute {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let tys = self.0.iter().collect::<Vec<_>>();

		tokens.extend(quote::quote! {
			#[scale_info(			skip_type_params(#( #tys ),*))]
			#[codec(encode_bound(	skip_type_params(#( #tys ),*)))]
			#[codec(decode_bound(	skip_type_params(#( #tys ),*)))]
			#[codec(mel_bound(		skip_type_params(#( #tys ),*)))]
		});
	}
}

/// Apply a specific bound to each type.
#[derive(Debug, Clone)]
pub struct MelBoundAttribute(Punctuated<MelBound, Token![,]>);

impl Parse for MelBoundAttribute {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		input.parse::<keywords::mel_bound>()?;

		let content;
		syn::parenthesized!(content in input);

		content.parse_terminated(MelBound::parse, Token![,]).map(Self)
	}
}

impl ToTokens for MelBoundAttribute {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		for bound in &self.0 {
			bound.to_tokens(tokens);
		}
	}
}

/// Require `ty` to fullfil the given bounds to be eligible for MEL.
#[derive(Debug, Clone)]
pub struct MelBound {
	typ: Type,
	bounds: Punctuated<Type, Token![+]>,
}

impl Parse for MelBound {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		let typ = input.parse().unwrap();

		let bounds = if input.parse::<Token![:]>().is_ok() {
			Punctuated::parse_separated_nonempty(input)?
		} else {
			Punctuated::new()
		};

		Ok(Self { typ, bounds })
	}
}

impl ToTokens for MelBound {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let typ = &self.typ;
		let bounds = self.bounds.iter().collect::<Vec<_>>();

		tokens.extend(quote::quote! {
			#[codec(mel_bound(	 	#typ: #( #bounds )+*))]
			#[codec(encode_bound(	#typ: #( #bounds )+*))]
			#[codec(decode_bound(	#typ: #( #bounds )+*))]
		});
	}
}

/// Require all `tys` to fullfil the `MaxEncodedLen` bound.
#[derive(Debug, Clone)]
pub struct MelAttribute(Punctuated<Type, Token![,]>);

impl Parse for MelAttribute {
	fn parse(input: &ParseBuffer) -> Result<Self> {
		input.parse::<keywords::mel>()?;

		let content;
		syn::parenthesized!(content in input);

		content.parse_terminated(Type::parse, Token![,]).map(Self)
	}
}

impl ToTokens for MelAttribute {
	fn to_tokens(&self, tokens: &mut TokenStream2) {
		let tys = self.0.iter();
		let frame_support = match generate_access_from_frame_or_crate("frame-support") {
			Ok(path) => path,
			Err(err) => return tokens.extend(err.to_compile_error()),
		};

		tokens.extend(quote::quote! {
			#[codec(mel_bound(
				#(
					#tys: ::#frame_support::__private::codec::MaxEncodedLen
				),*
			))]
		});
	}
}
