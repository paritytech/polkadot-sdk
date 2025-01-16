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
// limitations under the License

use crate::construct_runtime::Pallet;
use core::str::FromStr;
use proc_macro2::{Ident, TokenStream as TokenStream2};

/// Expands implementation of runtime level `DispatchViewFunction`.
pub fn expand_outer_query(
	runtime_name: &Ident,
	pallet_decls: &[Pallet],
	scrate: &TokenStream2,
) -> TokenStream2 {
	let prefix_conditionals = pallet_decls.iter().map(|pallet| {
		let pallet_name = &pallet.name;
		let attr = pallet.cfg_pattern.iter().fold(TokenStream2::new(), |acc, pattern| {
			let attr = TokenStream2::from_str(&format!("#[cfg({})]", pattern.original()))
				.expect("was successfully parsed before; qed");
			quote::quote! {
				#acc
				#attr
			}
		});
		quote::quote! {
			#attr
			if id.prefix == <#pallet_name as #scrate::traits::ViewFunctionIdPrefix>::prefix() {
				return <#pallet_name as #scrate::traits::DispatchViewFunction>::dispatch_view_function(id, input, output)
			}
		}
	});

	quote::quote! {
		const _: () = {
			impl #scrate::traits::DispatchViewFunction for #runtime_name {
				fn dispatch_view_function<O: #scrate::__private::codec::Output>(
					id: & #scrate::__private::ViewFunctionId,
					input: &mut &[u8],
					output: &mut O
				) -> Result<(), #scrate::__private::ViewFunctionDispatchError>
				{
					#( #prefix_conditionals )*
					Err(#scrate::__private::ViewFunctionDispatchError::NotFound(id.clone()))
				}
			}
		};
	}
}
