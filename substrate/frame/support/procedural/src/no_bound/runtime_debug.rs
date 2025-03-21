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

use frame_support_procedural_tools::generate_access_from_frame_or_crate;
use super::utils::apply_still_bind;
use super::debug::derive_debug_no_bound;

/// Derive [`Debug`]. If `std` is enabled, it uses `frame_support::DebugNoBound`, if `std` is not
/// enabled it just returns `"<wasm:stripped>"`.
/// This behaviour is useful to prevent bloating the runtime WASM blob from unneeded code.
/// 
/// Optionally select which generics will still be bound with `still_bind(...)`.
pub fn derive_runtime_debug_no_bound(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let try_runtime_or_std_impl: proc_macro2::TokenStream = derive_debug_no_bound(input.clone()).into();

	let stripped_impl = {
		let mut input = syn::parse_macro_input!(input as syn::DeriveInput);

        if let Err(e) = apply_still_bind(&mut input, quote::quote!(::core::fmt::Debug)) {
            return e.to_compile_error().into();
        }

		let name = &input.ident;
		let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

		quote::quote!(
			const _: () = {
				impl #impl_generics ::core::fmt::Debug for #name #ty_generics #where_clause {
					fn fmt(&self, fmt: &mut ::core::fmt::Formatter) -> core::fmt::Result {
						fmt.write_str("<wasm:stripped>")
					}
				}
			};
		)
	};

	let frame_support = match generate_access_from_frame_or_crate("frame-support") {
		Ok(frame_support) => frame_support,
		Err(e) => return e.to_compile_error().into(),
	};

	quote::quote!(
		#frame_support::try_runtime_or_std_enabled! {
			#try_runtime_or_std_impl
		}
		#frame_support::try_runtime_and_std_not_enabled! {
			#stripped_impl
		}
	)
	.into()
}