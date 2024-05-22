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
	parse::view_functions::{ViewFunctionDef, ViewFunctionsDef},
	Def,
};
use inflector::Inflector;
use proc_macro2::TokenStream;
use quote::ToTokens;

pub fn expand_view_functions(def: &mut Def) -> TokenStream {
	let Some(view_fns_def) = def.view_functions.as_ref() else {
		return TokenStream::new();
	};

	quote::quote! {
		#view_fns_def
	}
}

impl ToTokens for ViewFunctionsDef {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let view_fn_impls = self.view_functions.iter().map(|view_fn| {
			quote::quote! { #view_fn }
		});

		tokens.extend(quote::quote! {
			#( #view_fn_impls )*
		});
	}
}

impl ToTokens for ViewFunctionDef {
	fn to_tokens(&self, tokens: &mut TokenStream) {
		let name = self.query_struct_ident();
		tokens.extend(quote::quote! {
			pub struct #name;
		});
	}
}
