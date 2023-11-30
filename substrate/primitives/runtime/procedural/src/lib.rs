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

//! Derive macros to derive traits without bounding generic parameters.

mod clone;
mod debug;
mod default;
mod partial_eq;

use proc_macro::TokenStream;

/// Derive [`Clone`] but do not bound any generic. Docs are at `sp_runtime::CloneNoBound`.
#[proc_macro_derive(CloneNoBound)]
pub fn derive_clone_no_bound(input: TokenStream) -> TokenStream {
	clone::derive_clone_no_bound(input)
}

/// Derive [`Debug`] but do not bound any generics. Docs are at `sp_runtime::DebugNoBound`.
#[proc_macro_derive(DebugNoBound)]
pub fn derive_debug_no_bound(input: TokenStream) -> TokenStream {
	debug::derive_debug_no_bound(input)
}

/// Derive [`Debug`], if `std` is enabled it uses `sp_runtime::DebugNoBound`, if `std` is not
/// enabled it just returns `"<wasm:stripped>"`.
/// This behaviour is useful to prevent bloating the runtime WASM blob from unneeded code.
#[proc_macro_derive(RuntimeDebugNoBound)]
pub fn derive_runtime_debug_no_bound(input: TokenStream) -> TokenStream {
	if cfg!(any(feature = "std", feature = "try-runtime")) {
		debug::derive_debug_no_bound(input)
	} else {
		let input: syn::DeriveInput = match syn::parse(input) {
			Ok(input) => input,
			Err(e) => return e.to_compile_error().into(),
		};

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
		.into()
	}
}

/// Derive [`PartialEq`] but do not bound any generic. Docs are at
/// `sp_runtime::PartialEqNoBound`.
#[proc_macro_derive(PartialEqNoBound)]
pub fn derive_partial_eq_no_bound(input: TokenStream) -> TokenStream {
	partial_eq::derive_partial_eq_no_bound(input)
}

/// derive Eq but do no bound any generic. Docs are at `sp_runtime::EqNoBound`.
#[proc_macro_derive(EqNoBound)]
pub fn derive_eq_no_bound(input: TokenStream) -> TokenStream {
	let input: syn::DeriveInput = match syn::parse(input) {
		Ok(input) => input,
		Err(e) => return e.to_compile_error().into(),
	};

	let name = &input.ident;
	let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

	quote::quote_spanned!(name.span() =>
		const _: () = {
			impl #impl_generics ::core::cmp::Eq for #name #ty_generics #where_clause {}
		};
	)
	.into()
}

/// derive `Default` but do no bound any generic. Docs are at `sp_runtime::DefaultNoBound`.
#[proc_macro_derive(DefaultNoBound, attributes(default))]
pub fn derive_default_no_bound(input: TokenStream) -> TokenStream {
	default::derive_default_no_bound(input)
}
