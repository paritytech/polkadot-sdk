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

//! Macros to derive runtime debug implementation.
//!
//! This macro is deprecated. Use `#[derive(Debug)]` directly instead.
//!
//! ```rust
//! #[derive(sp_debug_derive::RuntimeDebug)]
//! struct MyStruct;
//!
//! assert_eq!(format!("{:?}", MyStruct), "MyStruct");
//! ```

mod impls;

use proc_macro::TokenStream;
use quote::quote;

/// Derive macro for `Debug` that emits a deprecation warning.
///
/// This macro is deprecated. Use `#[derive(Debug)]` directly instead.
#[proc_macro_derive(RuntimeDebug)]
pub fn runtime_debug_derive(input: TokenStream) -> TokenStream {
	let input: syn::DeriveInput = syn::parse_macro_input!(input);
	let name = &input.ident;

	let warning = proc_macro_warning::Warning::new_deprecated(&format!("RuntimeDebug_{}", name))
		.old("derive `RuntimeDebug`")
		.new("derive `Debug`")
		.span(input.ident.span())
		.build_or_panic();

	let debug_impl: proc_macro2::TokenStream = impls::debug_derive(input).into();

	quote!(#warning #debug_impl).into()
}
