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

use crate::{
	pallet::{parse::GenericKind, Def},
	COUNTER,
};
use inflector::Inflector;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{spanned::Spanned, Ident};

/// expand the `is_origin_part_defined` macro.
fn expand_origin_macro_helper(def: &mut Def, pallet_has_origin: bool) -> TokenStream {
	let count = COUNTER.with(|counter| counter.borrow_mut().inc());
	let macro_ident = Ident::new(&format!("__is_origin_part_defined_{}", count), def.item.span());

	let maybe_compile_error = if !pallet_has_origin {
		quote! {
			compile_error!(concat!(
				"`",
				stringify!($pallet_name),
				"` does not have #[pallet::origin] defined, perhaps you should \
				remove `Origin` from construct_runtime?",
			));
		}
	} else {
		TokenStream::new()
	};

	quote! {
		#[doc(hidden)]
		pub mod __substrate_origin_check {
			#[macro_export]
			#[doc(hidden)]
			macro_rules! #macro_ident {
				($pallet_name:ident) => {
					#maybe_compile_error
				}
			}
			#[doc(hidden)]
			pub use #macro_ident as is_origin_part_defined;
		}
	}
}

///
/// * if needed by authorize functions:
///   * create an enum `AuthorizedCallOrigin` with one variant per authorized call.
///   * create an origin if there is none
///   * add `AuthorizedCallOrigin` enum to the origin
/// * expand the `is_origin_part_defined` macro.
pub fn expand_origin(def: &mut Def) -> proc_macro2::TokenStream {
	let authorize_origin_variants = def
		.call
		.as_ref()
		.map(|call| {
			call.methods
				.iter()
				.filter_map(|call| {
					call.authorize.is_some().then(|| {
						syn::Ident::new(
							call.name.to_string().to_camel_case().as_str(),
							call.name.span(),
						)
					})
				})
				.collect::<Vec<_>>()
		})
		.unwrap_or_else(Vec::new);

	let pallet_has_origin = def.origin.is_some() || !authorize_origin_variants.is_empty();
	let origin_macro_helper = expand_origin_macro_helper(def, pallet_has_origin);

	// NOTE: It is checked in parser that if call doesn't use authorize then origin mustn't as well.
	if authorize_origin_variants.is_empty() {
		return origin_macro_helper;
	}

	let frame_support = &def.frame_support;
	let span = def
		.origin
		.as_ref()
		.map(|origin| origin.authorized_call.expect("consistency is checked by parser").1)
		.unwrap_or_else(|| proc_macro2::Span::call_site());

	let authorize_origin_enum = quote::quote_spanned!(span =>
		#[derive(
			Clone,
			PartialEq,
			Eq,
			#frame_support::RuntimeDebugNoBound,
			#frame_support::__private::codec::MaxEncodedLen,
			#frame_support::__private::codec::Encode,
			#frame_support::__private::codec::Decode,
			#frame_support::__private::scale_info::TypeInfo,
		)]
		pub enum AuthorizedCallOrigin {
			#( #authorize_origin_variants, )*
		}
	);

	let maybe_origin = if let Some(origin_def) = &def.origin {
		let (index, _) = origin_def.authorized_call.expect("consistency is checked by parser");

		let syn::Item::Enum(origin) =
			&mut def.item.content.as_mut().expect("Checked by parser").1[origin_def.index]
		else {
			unreachable!("Checked by parser")
		};

		let variant = &mut origin.variants[index];

		let syn::Fields::Unnamed(fields) = &mut variant.fields else {
			unreachable!("Parse stage ensures variant has unnamed fields")
		};

		*fields = syn::parse_quote!((AuthorizedCallOrigin));

		None
	} else {
		// Default origin is generic
		let gen_kind = GenericKind::from_gens(true, def.config.has_instance)
			.expect("Default is generic so no conflict");
		let type_decl_bounded_gen = gen_kind.type_decl_bounded_gen(span);
		let type_use_gen = gen_kind.type_use_gen(span);

		Some(quote::quote_spanned!(span =>
			#[derive(
				#frame_support::EqNoBound,
				#frame_support::PartialEqNoBound,
				#frame_support::CloneNoBound,
				#frame_support::RuntimeDebugNoBound,
				#frame_support::__private::codec::Encode,
				#frame_support::__private::codec::Decode,
				#frame_support::__private::scale_info::TypeInfo,
				#frame_support::__private::codec::MaxEncodedLen,
			)]
			#[scale_info(skip_type_params(#type_use_gen))]
			pub enum Origin<#type_decl_bounded_gen> {
				AuthorizedCall(AuthorizedCallOrigin),
			}
		))
	};

	quote::quote!(
		#origin_macro_helper
		#authorize_origin_enum
		#maybe_origin
	)
}
