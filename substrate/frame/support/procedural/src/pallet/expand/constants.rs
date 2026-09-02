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

use crate::{deprecation::extract_or_return_allow_attrs, pallet::Def};

/// Implement the `pallet_constants_metadata` function for the pallet.
pub fn expand_constants(def: &mut Def) -> proc_macro2::TokenStream {
	let frame_support = &def.frame_support;
	let type_impl_gen = &def.type_impl_generics(proc_macro2::Span::call_site());
	let type_use_gen = &def.type_use_generics(proc_macro2::Span::call_site());
	let pallet_ident = &def.pallet_struct.pallet;
	let trait_use_gen = &def.trait_use_generics(proc_macro2::Span::call_site());

	let mut where_clauses = vec![&def.config.where_clause];
	where_clauses.extend(def.extra_constants.iter().map(|d| &d.where_clause));
	let completed_where_clause = super::merge_where_clauses(&where_clauses);

	let mut config_consts = vec![];
	for const_ in def.config.consts_metadata.iter() {
		let ident = &const_.ident;
		let deprecation_info = match crate::deprecation::get_deprecation(
			&quote::quote! { #frame_support },
			&const_.attrs,
		) {
			Ok(deprecation) => deprecation,
			Err(e) => return e.into_compile_error(),
		};

		// Extracts #[allow] attributes, necessary so that we don't run into compiler warnings
		let maybe_allow_attrs = extract_or_return_allow_attrs(&const_.attrs);

		let access = const_
			.value_path
			.as_ref()
			.map(|p| quote::quote!(#p))
			.unwrap_or_else(|| quote::quote!(::get()));

		let trait_bound = &const_.trait_bound;

		let ident_str = format!("{}", const_.ident);

		let no_docs = vec![];
		let doc = if cfg!(feature = "no-metadata-docs") { &no_docs } else { &const_.doc };

		config_consts.push(quote::quote!({
			fn __meta_type_of<__V: #frame_support::__private::scale_info::TypeInfo + 'static>(
				_: &__V
			) -> #frame_support::__private::scale_info::MetaType {
				#frame_support::__private::scale_info::meta_type::<__V>()
			}
			#(#maybe_allow_attrs)*
			let value = <<T as Config #trait_use_gen>::#ident as #trait_bound>#access;
			#frame_support::__private::metadata_ir::PalletConstantMetadataIR {
				name: #ident_str,
				ty: __meta_type_of(&value),
				value: #frame_support::__private::codec::Encode::encode(&value),
				docs: #frame_support::__private::vec![ #( #doc ),* ],
				deprecation_info: #deprecation_info
			}
		}));
	}

	let mut extra_consts = vec![];
	for const_ in def.extra_constants.iter().flat_map(|d| &d.extra_constants) {
		let ident = &const_.ident;
		let const_type = &const_.type_;
		let deprecation_info = match crate::deprecation::get_deprecation(
			&quote::quote! { #frame_support },
			&const_.attrs,
		) {
			Ok(deprecation) => deprecation,
			Err(e) => return e.into_compile_error(),
		};
		// Extracts #[allow] attributes, necessary so that we don't run into compiler warnings
		let maybe_allow_attrs = extract_or_return_allow_attrs(&const_.attrs);

		let ident_str = format!("{}", const_.metadata_name.as_ref().unwrap_or(&const_.ident));

		let no_docs = vec![];
		let doc = if cfg!(feature = "no-metadata-docs") { &no_docs } else { &const_.doc };

		extra_consts.push(quote::quote!({
			#frame_support::__private::metadata_ir::PalletConstantMetadataIR {
				name: #ident_str,
				ty: #frame_support::__private::scale_info::meta_type::<#const_type>(),
				value: {
					#(#maybe_allow_attrs)*
					let value = <Pallet<#type_use_gen>>::#ident();
					#frame_support::__private::codec::Encode::encode(&value)
				},
				docs: #frame_support::__private::vec![ #( #doc ),* ],
				deprecation_info: #deprecation_info
			}
		}));
	}

	let consts = config_consts.into_iter().chain(extra_consts.into_iter());

	quote::quote!(
		impl<#type_impl_gen> #pallet_ident<#type_use_gen> #completed_where_clause{

			#[doc(hidden)]
			pub fn pallet_constants_metadata()
				-> #frame_support::__private::Vec<#frame_support::__private::metadata_ir::PalletConstantMetadataIR>
			{
				#frame_support::__private::vec![ #( #consts ),* ]
			}
		}
	)
}
