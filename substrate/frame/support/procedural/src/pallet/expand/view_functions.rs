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

use crate::pallet::{
	parse::view_functions::{ViewFunctionDef, ViewFunctionsImplDef},
	Def,
};
use proc_macro2::TokenStream;

pub fn expand_view_functions(def: &Def) -> TokenStream {
	let Some(view_fns_def) = def.view_functions.as_ref() else {
		return TokenStream::new();
	};

	let view_fn_impls = view_fns_def
		.view_functions
		.iter()
		.map(|view_fn| expand_view_function(def, &view_fns_def, view_fn));
	let impl_dispatch_query = impl_dispatch_query(def, view_fns_def);

	quote::quote! {
		#( #view_fn_impls )*
		#impl_dispatch_query
	}
}

fn expand_view_function(
	def: &Def,
	view_fns_impl: &ViewFunctionsImplDef,
	view_fn: &ViewFunctionDef,
) -> TokenStream {
	let span = view_fns_impl.attr_span;
	let frame_support = &def.frame_support;
	let frame_system = &def.frame_system;
	let type_impl_gen = &def.type_impl_generics(span);
	let type_decl_bounded_gen = &def.type_decl_bounded_generics(span);
	let type_use_gen = &def.type_use_generics(span);
	let trait_use_gen = &def.trait_use_generics(span);
	let capture_docs = if cfg!(feature = "no-metadata-docs") { "never" } else { "always" };
	let where_clause = &view_fns_impl.where_clause;

	let query_struct_ident = view_fn.query_struct_ident();
	// let args = todo!();
	let return_type = &view_fn.return_type;

	quote::quote! {
		#[derive(
			#frame_support::RuntimeDebugNoBound,
			#frame_support::CloneNoBound,
			#frame_support::EqNoBound,
			#frame_support::PartialEqNoBound,
			#frame_support::__private::codec::Encode,
			#frame_support::__private::codec::Decode,
			#frame_support::__private::scale_info::TypeInfo,
		)]
		#[codec(encode_bound())]
		#[codec(decode_bound())]
		#[scale_info(skip_type_params(#type_use_gen), capture_docs = #capture_docs)]
		#[allow(non_camel_case_types)]
		pub struct #query_struct_ident<#type_decl_bounded_gen> #where_clause {
			_marker: ::core::marker::PhantomData<(#type_use_gen,)>,
		}

		impl<#type_impl_gen> #query_struct_ident<#type_use_gen> #where_clause {
			pub fn new() -> Self {
				Self { _marker: ::core::default::Default::default() }
			}
		}

		impl<#type_impl_gen> #frame_support::traits::QueryIdSuffix for #query_struct_ident<#type_use_gen> #where_clause {
			const SUFFIX: [u8; 16] = [0u8; 16];
		}

		impl<#type_impl_gen> #frame_support::traits::Query for #query_struct_ident<#type_use_gen> #where_clause {
			const ID: #frame_support::traits::QueryId = #frame_support::traits::QueryId {
				prefix: <<T as #frame_system::Config #trait_use_gen>::RuntimeQuery as #frame_support::traits::QueryIdPrefix>::PREFIX,
				suffix: <Self as #frame_support::traits::QueryIdSuffix>::SUFFIX, // todo: [AJ] handle instantiatable pallet suffix
			};
			type ReturnType = #return_type;

			fn query(self) -> Self::ReturnType {
				todo!()
			}
		}
	}
}

fn impl_dispatch_query(def: &Def, view_fns_impl: &ViewFunctionsImplDef) -> TokenStream {
	let span = view_fns_impl.attr_span;
	let frame_support = &def.frame_support;
	let pallet_ident = &def.pallet_struct.pallet;
	let type_impl_gen = &def.type_impl_generics(span);
	let type_decl_bounded_gen = &def.type_decl_bounded_generics(span);
	let type_use_gen = &def.type_use_generics(span);

	quote::quote! {
		impl<#type_impl_gen> #frame_support::traits::DispatchQuery
			for #pallet_ident<#type_use_gen>
		{
			fn dispatch_query<
				I: #frame_support::__private::codec::Input,
				O: #frame_support::__private::codec::Output,
			>
				(id: & #frame_support::traits::QueryId, input: I) -> Result<O, #frame_support::__private::codec::Error>
			{
				todo!()
			}
		}
	}
}
